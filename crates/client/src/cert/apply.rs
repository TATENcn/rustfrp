//! Writing certificate material to the paths a `[[proxies]]` entry declares.
//!
//! The paths are read from the proxy's FRP-native `plugin.crtPath` / `keyPath`
//! rather than being duplicated in a second table: one source of truth, and the
//! file the daemon writes is by construction the file frpc reads.
//!
//! Every write is atomic (`tmp` + `rename`) and keeps the previous file as a
//! backup, so a failed apply can be rolled back without re-reading the database.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::cert::CertificateSecrets;
use crate::error::{ClientError, Result};

/// Where one binding's material belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyTarget {
    pub crt_path: PathBuf,
    pub key_path: PathBuf,
}

/// Result of a successful write, kept so the caller can roll back.
#[derive(Debug)]
pub struct AppliedFiles {
    pub target: ApplyTarget,
    pub crt_backup: Option<PathBuf>,
    pub key_backup: Option<PathBuf>,
}

impl AppliedFiles {
    /// Restore the files that existed before this apply; remove ones we created.
    pub fn rollback(&self) {
        for (path, backup) in [
            (&self.target.crt_path, &self.crt_backup),
            (&self.target.key_path, &self.key_backup),
        ] {
            match backup {
                Some(backup) => {
                    if let Err(error) = std::fs::rename(backup, path) {
                        tracing::warn!(path = %path.display(), %error, "failed to restore certificate backup");
                    }
                }
                None => {
                    if let Err(error) = std::fs::remove_file(path) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            tracing::warn!(path = %path.display(), %error, "failed to remove written certificate file");
                        }
                    }
                }
            }
        }
    }

    /// Drop the backups once the caller decided the new material is live.
    pub fn commit(self) {
        for backup in [self.crt_backup, self.key_backup].into_iter().flatten() {
            if let Err(error) = std::fs::remove_file(&backup) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(backup = %backup.display(), %error, "failed to remove certificate backup");
                }
            }
        }
    }
}

/// Read `crtPath` / `keyPath` out of an FRP client plugin config.
///
/// Accepts the plugin types that actually hold a certificate; anything else is
/// reported instead of silently writing nowhere.
pub fn target_from_plugin_config(plugin: &Value) -> Result<ApplyTarget> {
    let Some(object) = plugin.as_object() else {
        return Err(ClientError::CertificateTarget(
            "plugin configuration is not a JSON object".into(),
        ));
    };

    match object.get("type").and_then(Value::as_str) {
        Some("https2http" | "https2https" | "tls2raw") => {}
        Some(other) => {
            return Err(ClientError::CertificateTarget(format!(
                "plugin type '{other}' does not hold a certificate (expected https2http, https2https or tls2raw)"
            )));
        }
        None => {
            return Err(ClientError::CertificateTarget(
                "plugin configuration has no 'type' field".into(),
            ));
        }
    }

    let path = |field: &str| -> Result<PathBuf> {
        match object.get(field).and_then(Value::as_str) {
            Some(value) if !value.trim().is_empty() => Ok(PathBuf::from(value.trim())),
            _ => Err(ClientError::CertificateTarget(format!(
                "plugin configuration has no '{field}'"
            ))),
        }
    };

    Ok(ApplyTarget {
        crt_path: path("crtPath")?,
        key_path: path("keyPath")?,
    })
}

/// Write both files, keeping backups. On failure nothing is left half-written.
pub fn write_material(target: &ApplyTarget, secrets: &CertificateSecrets) -> Result<AppliedFiles> {
    if target.crt_path == target.key_path {
        return Err(ClientError::CertificateTarget(
            "crtPath and keyPath must be different files".into(),
        ));
    }
    for path in [&target.crt_path, &target.key_path] {
        reject_unsafe_path(path)?;
    }

    let crt_backup = backup_existing(&target.crt_path)?;
    let key_backup = match backup_existing(&target.key_path) {
        Ok(backup) => backup,
        Err(error) => {
            // Restore what we just moved before reporting.
            if let Some(backup) = &crt_backup {
                let _ = std::fs::rename(backup, &target.crt_path);
            }
            return Err(error);
        }
    };

    let files = AppliedFiles {
        target: target.clone(),
        crt_backup,
        key_backup,
    };

    if let Err(error) = write_atomic(&target.crt_path, secrets.fullchain_pem().as_bytes()) {
        files.rollback();
        return Err(error);
    }
    if let Err(error) = write_atomic(&target.key_path, secrets.key_pem().as_bytes()) {
        files.rollback();
        return Err(error);
    }

    Ok(files)
}

/// Write `contents` to `path` through a sibling temporary file and a rename.
///
/// The temporary file is created in the destination directory: a rename across
/// filesystems is not atomic, and the destination may be anywhere on the host.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| {
            ClientError::CertificateTarget(format!(
                "failed to create {}: {error}",
                parent.display()
            ))
        })?;
    }

    let temporary = temporary_path(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // The key must never be world-readable, not even between write and chmod.
        options.mode(0o600);
    }

    let write = || -> std::io::Result<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        Ok(())
    };

    if let Err(error) = write() {
        let _ = std::fs::remove_file(&temporary);
        return Err(ClientError::CertificateTarget(format!(
            "failed to write {}: {error}",
            temporary.display()
        )));
    }

    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(ClientError::CertificateTarget(format!(
            "failed to install {}: {error}",
            path.display()
        )));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        if let Err(error) = std::fs::set_permissions(path, permissions) {
            tracing::warn!(path = %path.display(), %error, "failed to set 0600 on certificate file");
        }
    }

    Ok(())
}

/// Move an existing file aside and return the backup path.
fn backup_existing(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let backup = path.with_extension(format!(
        "{}.rustfrp-bak",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("bak")
    ));
    std::fs::rename(path, &backup).map_err(|error| {
        ClientError::CertificateTarget(format!("failed to back up {}: {error}", path.display()))
    })?;
    Ok(Some(backup))
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "cert".to_string());
    path.with_file_name(format!(".{file_name}.rustfrp-tmp"))
}

/// Refuse paths that would write outside the place an operator can see.
fn reject_unsafe_path(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(ClientError::CertificateTarget(format!(
            "certificate path {} must be absolute",
            path.display()
        )));
    }
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(ClientError::CertificateTarget(format!(
                    "certificate path {} must not contain '..'",
                    path.display()
                )));
            }
            Component::Normal(name) if name == ".." => {
                return Err(ClientError::CertificateTarget(format!(
                    "certificate path {} must not contain '..'",
                    path.display()
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> CertificateSecrets {
        CertificateSecrets {
            cert_der: vec![0x30, 0x03, 0x02, 0x01, 0x01],
            chain_der: Vec::new(),
            key_label: "PRIVATE KEY".into(),
            key_der: vec![0x30, 0x03, 0x02, 0x01, 0x02],
        }
    }

    #[test]
    fn rejects_non_certificate_plugin() {
        let plugin = serde_json::json!({"type": "http2https", "localAddr": "127.0.0.1:80"});
        assert!(target_from_plugin_config(&plugin).is_err());
    }

    #[test]
    fn accepts_tls2raw_paths() {
        let plugin = serde_json::json!({
            "type": "tls2raw",
            "localAddr": "192.168.1.206:34618",
            "crtPath": "/etc/frp/ssl/powercess/cert.pem",
            "keyPath": "/etc/frp/ssl/powercess/cert.key",
        });
        let target = target_from_plugin_config(&plugin).unwrap();
        assert_eq!(
            target.crt_path,
            PathBuf::from("/etc/frp/ssl/powercess/cert.pem")
        );
        assert_eq!(
            target.key_path,
            PathBuf::from("/etc/frp/ssl/powercess/cert.key")
        );
    }

    #[test]
    fn requires_both_paths() {
        let plugin = serde_json::json!({"type": "https2http", "crtPath": "/tmp/a.pem"});
        let error = target_from_plugin_config(&plugin).unwrap_err();
        assert!(error.to_string().contains("keyPath"));
    }

    #[test]
    fn writes_and_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let target = ApplyTarget {
            crt_path: dir.path().join("fullchain.pem"),
            key_path: dir.path().join("privkey.pem"),
        };
        std::fs::write(&target.crt_path, "OLD CERT").unwrap();

        let files = write_material(&target, &secrets()).unwrap();
        assert!(std::fs::read_to_string(&target.crt_path)
            .unwrap()
            .contains("BEGIN CERTIFICATE"));

        files.rollback();
        assert_eq!(
            std::fs::read_to_string(&target.crt_path).unwrap(),
            "OLD CERT"
        );
        assert!(!target.key_path.exists());
    }

    #[test]
    fn commit_keeps_new_material() {
        let dir = tempfile::tempdir().unwrap();
        let target = ApplyTarget {
            crt_path: dir.path().join("fullchain.pem"),
            key_path: dir.path().join("privkey.pem"),
        };
        write_material(&target, &secrets()).unwrap().commit();
        assert!(target.crt_path.exists());
        assert!(target.key_path.exists());
        assert!(!dir.path().join("fullchain.pem.rustfrp-bak").exists());
    }

    #[test]
    fn refuses_relative_or_traversing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let relative = ApplyTarget {
            crt_path: PathBuf::from("cert.pem"),
            key_path: dir.path().join("key.pem"),
        };
        assert!(write_material(&relative, &secrets()).is_err());

        let traversing = ApplyTarget {
            crt_path: dir.path().join("../escape/cert.pem"),
            key_path: dir.path().join("key.pem"),
        };
        assert!(write_material(&traversing, &secrets()).is_err());
    }
}
