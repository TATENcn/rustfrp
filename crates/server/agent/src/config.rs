use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ConfigStore {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedConfig {
    pub path: PathBuf,
    pub digest: String,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedConfig {
    pub path: PathBuf,
    pub digest: String,
    pub changed: bool,
}

impl ConfigStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn active_path(&self) -> PathBuf {
        self.root.join("frps.toml")
    }

    pub async fn load_cached(&self) -> Result<Option<AppliedConfig>> {
        let path = self.active_path();
        let contents = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let previous = self.previous_path();
                match tokio::fs::read_to_string(&previous).await {
                    Ok(contents) => {
                        replace_file(&previous, &path).await?;
                        contents
                    }
                    Err(previous_error)
                        if previous_error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        return Ok(None)
                    }
                    Err(previous_error) => return Err(previous_error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        };
        validate_frps_toml(&contents)?;
        Ok(Some(AppliedConfig {
            digest: digest(&contents),
            path,
            changed: false,
        }))
    }

    /// Syntax-check and durably stage a candidate for semantic verification by frps.
    pub async fn stage(&self, contents: &str) -> Result<StagedConfig> {
        validate_frps_toml(contents)?;
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.active_path();
        let new_digest = digest(contents);
        if let Ok(existing) = tokio::fs::read_to_string(&path).await {
            if digest(&existing) == new_digest {
                return Ok(StagedConfig {
                    path,
                    digest: new_digest,
                    changed: false,
                });
            }
        }

        let temporary = self.root.join("frps.toml.pending");
        tokio::fs::write(&temporary, contents)
            .await
            .context("write pending frps configuration")?;
        sync_file(&temporary).await?;
        Ok(StagedConfig {
            path: temporary,
            digest: new_digest,
            changed: true,
        })
    }

    /// Atomically promote a candidate only after `frps verify` succeeds.
    pub async fn commit(&self, staged: StagedConfig) -> Result<AppliedConfig> {
        if !staged.changed {
            return Ok(AppliedConfig {
                path: self.active_path(),
                digest: staged.digest,
                changed: false,
            });
        }
        let path = self.active_path();
        let previous = self.previous_path();
        if previous.exists() {
            tokio::fs::remove_file(&previous).await?;
        }
        if path.exists() {
            tokio::fs::rename(&path, &previous).await?;
        }
        if let Err(error) = replace_file(&staged.path, &path).await {
            if previous.exists() {
                let _ = tokio::fs::rename(&previous, &path).await;
            }
            return Err(error);
        }
        Ok(AppliedConfig {
            path,
            digest: staged.digest,
            changed: true,
        })
    }

    pub async fn discard(&self, staged: &StagedConfig) {
        if staged.changed {
            let _ = tokio::fs::remove_file(&staged.path).await;
        }
    }

    /// Restore the configuration that was active before the latest commit.
    pub async fn rollback(&self) -> Result<Option<AppliedConfig>> {
        let previous = self.previous_path();
        if !previous.exists() {
            return Ok(None);
        }
        let active = self.active_path();
        if active.exists() {
            tokio::fs::remove_file(&active).await?;
        }
        tokio::fs::rename(previous, &active).await?;
        self.load_cached().await
    }

    pub async fn finalize(&self) {
        let _ = tokio::fs::remove_file(self.previous_path()).await;
    }

    fn previous_path(&self) -> PathBuf {
        self.root.join("frps.toml.previous")
    }
}

pub fn validate_frps_toml(contents: &str) -> Result<()> {
    let value: toml::Value = toml::from_str(contents).context("invalid frps TOML")?;
    let table = value
        .as_table()
        .context("frps configuration root must be a TOML table")?;
    if table.is_empty() {
        anyhow::bail!("frps configuration must not be empty");
    }
    Ok(())
}

fn digest(contents: &str) -> String {
    format!("{:x}", Sha256::digest(contents.as_bytes()))
}

async fn sync_file(path: &Path) -> Result<()> {
    let file = tokio::fs::OpenOptions::new().write(true).open(path).await?;
    file.sync_all().await?;
    Ok(())
}

async fn replace_file(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    if target.exists() {
        tokio::fs::remove_file(target).await?;
    }
    tokio::fs::rename(source, target).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn applies_valid_config_atomically_and_preserves_it_on_error() {
        let temp = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(temp.path().into());
        let first = store
            .commit(store.stage("bindPort = 7000\n").await.unwrap())
            .await
            .unwrap();
        assert!(first.changed);
        let unchanged = store.stage("bindPort = 7000\n").await.unwrap();
        assert!(!store.commit(unchanged).await.unwrap().changed);
        assert!(store.stage("bindPort = [\n").await.is_err());
        assert_eq!(
            tokio::fs::read_to_string(first.path).await.unwrap(),
            "bindPort = 7000\n"
        );
    }

    #[tokio::test]
    async fn loads_last_valid_cache() {
        let temp = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(temp.path().into());
        assert!(store.load_cached().await.unwrap().is_none());
        let staged = store.stage("bindAddr = \"0.0.0.0\"\n").await.unwrap();
        store.commit(staged).await.unwrap();
        assert!(store.load_cached().await.unwrap().is_some());
    }

    #[tokio::test]
    async fn rolls_back_a_verified_but_unstartable_candidate() {
        let temp = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(temp.path().into());
        let first = store.stage("bindPort = 7000\n").await.unwrap();
        store.commit(first).await.unwrap();
        let second = store.stage("bindPort = 7001\n").await.unwrap();
        store.commit(second).await.unwrap();
        let restored = store.rollback().await.unwrap().unwrap();
        assert_eq!(
            tokio::fs::read_to_string(restored.path).await.unwrap(),
            "bindPort = 7000\n"
        );
    }
}
