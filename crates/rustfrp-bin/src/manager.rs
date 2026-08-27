//! Multi-version FRP installation registry.

use crate::ensure::{binary_name, ensure_binary_from, IntegrityMarker};
use crate::verify::verify_sha256;
use crate::{default_frp_dir, FrpError};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InstalledVersion {
    pub version: String,
    pub platform: String,
    pub active: bool,
    pub frpc_path: PathBuf,
    pub frps_path: Option<PathBuf>,
    pub integrity_ok: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AvailableVersion {
    #[serde(rename = "tag_name")]
    tag: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
}

impl AvailableVersion {
    pub fn version(&self) -> &str {
        self.tag.trim_start_matches('v')
    }
}

#[derive(Debug, Clone)]
pub struct VersionManager {
    root: PathBuf,
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new(default_frp_dir())
    }
}

impl VersionManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn install(
        &self,
        version: &str,
        release_base: Option<&str>,
    ) -> Result<PathBuf, FrpError> {
        validate_version(version)?;
        ensure_binary_from(
            "frpc",
            Some(version),
            Some(&self.root),
            None,
            release_base.unwrap_or(crate::download::OFFICIAL_RELEASE_BASE),
        )
        .await
    }

    pub async fn list_installed(&self) -> Result<Vec<InstalledVersion>, FrpError> {
        let platform = crate::platform::Platform::detect()
            .ok_or_else(|| FrpError::Download("Unsupported platform".into()))?;
        let active = self.active_version().await;
        let versions_dir = self.root.join("versions");
        let mut directory = match tokio::fs::read_dir(&versions_dir).await {
            Ok(directory) => directory,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut installed = Vec::new();
        while let Some(entry) = directory.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let version = entry.file_name().to_string_lossy().into_owned();
            if validate_version(&version).is_err() {
                continue;
            }
            let platform_dir = entry.path().join(&platform.slug);
            let frpc_path = platform_dir.join(binary_name("frpc"));
            if !frpc_path.exists() {
                continue;
            }
            let integrity_ok = verify_installation(&frpc_path, &version, &platform.slug).await;
            let frps = platform_dir.join(binary_name("frps"));
            installed.push(InstalledVersion {
                active: active.as_deref() == Some(version.as_str()),
                version,
                platform: platform.slug.clone(),
                frpc_path,
                frps_path: frps.exists().then_some(frps),
                integrity_ok,
            });
        }
        installed.sort_by(|left, right| {
            semver::Version::parse(&right.version)
                .ok()
                .cmp(&semver::Version::parse(&left.version).ok())
        });
        Ok(installed)
    }

    pub async fn activate(&self, version: &str) -> Result<PathBuf, FrpError> {
        validate_version(version)?;
        let installation = self
            .list_installed()
            .await?
            .into_iter()
            .find(|candidate| candidate.version == version && candidate.integrity_ok)
            .ok_or_else(|| {
                FrpError::Verify(format!(
                    "FRP {version} is not installed or failed integrity verification"
                ))
            })?;
        tokio::fs::create_dir_all(&self.root).await?;
        let marker = self.root.join("active-version");
        let temporary = self.root.join("active-version.pending");
        tokio::fs::write(&temporary, format!("{version}\n")).await?;
        #[cfg(windows)]
        if marker.exists() {
            tokio::fs::remove_file(&marker).await?;
        }
        tokio::fs::rename(temporary, marker).await?;
        Ok(installation.frpc_path)
    }

    pub async fn delete(&self, version: &str) -> Result<(), FrpError> {
        validate_version(version)?;
        if self.active_version().await.as_deref() == Some(version) {
            return Err(FrpError::Verify(format!(
                "Cannot delete active FRP version {version}"
            )));
        }
        let target = self.root.join("versions").join(version);
        match tokio::fs::remove_dir_all(target).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn active_version(&self) -> Option<String> {
        tokio::fs::read_to_string(self.root.join("active-version"))
            .await
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| validate_version(value).is_ok())
    }

    pub async fn clear_active(&self) -> Result<(), FrpError> {
        match tokio::fs::remove_file(self.root.join("active-version")).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn list_available(&self) -> Result<Vec<AvailableVersion>, FrpError> {
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?
            .get("https://api.github.com/repos/fatedier/frp/releases?per_page=30")
            .header(reqwest::header::USER_AGENT, "rustfrp-version-manager")
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(FrpError::Download(format!(
                "GitHub releases API returned HTTP {}",
                response.status()
            )));
        }
        let mut versions: Vec<AvailableVersion> = response.json().await?;
        versions.retain(|version| {
            !version.draft && !version.prerelease && validate_version(version.version()).is_ok()
        });
        Ok(versions)
    }
}

fn validate_version(version: &str) -> Result<(), FrpError> {
    semver::Version::parse(version.trim_start_matches('v'))
        .map(|_| ())
        .map_err(|error| FrpError::Download(format!("Invalid FRP version: {error}")))
}

async fn verify_installation(path: &Path, version: &str, platform: &str) -> bool {
    let marker_path = path.with_extension("sha256");
    let Ok(contents) = tokio::fs::read_to_string(marker_path).await else {
        return false;
    };
    let Some(marker) = IntegrityMarker::parse(&contents) else {
        return false;
    };
    marker.version == version
        && marker.platform == platform
        && verify_sha256(path, marker.sha256).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn rejects_unsafe_versions_and_active_deletion() {
        let temp = tempfile::tempdir().unwrap();
        let manager = VersionManager::new(temp.path().into());
        assert!(manager.delete("../../etc").await.is_err());
        tokio::fs::write(temp.path().join("active-version"), "0.70.1\n")
            .await
            .unwrap();
        assert!(manager.delete("0.70.1").await.is_err());
    }

    #[tokio::test]
    async fn lists_activates_and_deletes_isolated_versions() {
        let temp = tempfile::tempdir().unwrap();
        let manager = VersionManager::new(temp.path().into());
        let platform = crate::platform::Platform::detect().unwrap();
        let install = temp.path().join("versions/0.70.1").join(&platform.slug);
        tokio::fs::create_dir_all(&install).await.unwrap();
        let binary = install.join(binary_name("frpc"));
        tokio::fs::write(&binary, b"frpc").await.unwrap();
        let hash = format!("{:x}", Sha256::digest(b"frpc"));
        tokio::fs::write(
            binary.with_extension("sha256"),
            format!("0.70.1 {} {hash}\n", platform.slug),
        )
        .await
        .unwrap();

        assert_eq!(manager.list_installed().await.unwrap().len(), 1);
        assert_eq!(manager.activate("0.70.1").await.unwrap(), binary);
        assert_eq!(manager.active_version().await.as_deref(), Some("0.70.1"));
        assert!(manager.delete("0.70.1").await.is_err());
        tokio::fs::remove_file(temp.path().join("active-version"))
            .await
            .unwrap();
        manager.delete("0.70.1").await.unwrap();
        assert!(manager.list_installed().await.unwrap().is_empty());
    }
}
