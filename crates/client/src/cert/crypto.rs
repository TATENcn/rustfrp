//! At-rest encryption for certificate material.
//!
//! Certificate bodies and private keys are stored encrypted so that a leaked
//! database file (a backup, a copied SQLite file) does not leak keys. The master
//! key lives beside the database in its own file with `0600` permissions.
//!
//! Scope, stated plainly: this defends the *database file*, not a compromised
//! host. The frpc process must be able to read the plaintext key from `keyPath`,
//! so filesystem permissions and the service account remain the real boundary.

use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};

use crate::error::{ClientError, Result};

/// File name of the master key, resolved next to the database file.
pub const MASTER_KEY_FILE: &str = "cert_store.key";

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

/// Encrypts certificate blobs with a per-installation XChaCha20-Poly1305 key.
pub struct CertCipher {
    cipher: XChaCha20Poly1305,
    path: PathBuf,
}

impl std::fmt::Debug for CertCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertCipher")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl CertCipher {
    /// Load the master key from `dir`, creating it on first use.
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        let path = dir.join(MASTER_KEY_FILE);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let generated = Self::generate_key();
                write_private_file(&path, &generated)?;
                tracing::info!(path = %path.display(), "Generated certificate store master key");
                generated.to_vec()
            }
            Err(error) => {
                return Err(ClientError::CertificateSecrets(format!(
                    "failed to read {}: {error}",
                    path.display()
                )));
            }
        };

        if bytes.len() != KEY_LEN {
            return Err(ClientError::CertificateSecrets(format!(
                "{} must contain exactly {KEY_LEN} bytes, found {}",
                path.display(),
                bytes.len()
            )));
        }

        let mut key = [0_u8; KEY_LEN];
        key.copy_from_slice(&bytes);
        Ok(Self {
            cipher: XChaCha20Poly1305::new((&key).into()),
            path,
        })
    }

    /// Path of the master key file (for diagnostics; never expose contents).
    pub fn key_path(&self) -> &Path {
        &self.path
    }

    /// Encrypt `plaintext` into `nonce || ciphertext || tag`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = self.cipher.encrypt(&nonce, plaintext).map_err(|_| {
            ClientError::CertificateSecrets("failed to encrypt certificate material".into())
        })?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt a blob produced by [`Self::encrypt`].
    pub fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>> {
        if blob.len() <= NONCE_LEN {
            return Err(ClientError::CertificateSecrets(
                "certificate blob is truncated".into(),
            ));
        }
        let (nonce, body) = blob.split_at(NONCE_LEN);
        self.cipher
            .decrypt(XNonce::from_slice(nonce), body)
            .map_err(|_| {
                ClientError::CertificateSecrets(
                    "failed to decrypt certificate material (wrong master key?)".into(),
                )
            })
    }

    fn generate_key() -> [u8; KEY_LEN] {
        let mut key = [0_u8; KEY_LEN];
        use chacha20poly1305::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut key);
        key
    }
}

/// Write a file readable only by the current user.
pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            ClientError::CertificateSecrets(format!(
                "failed to create {}: {error}",
                parent.display()
            ))
        })?;
    }

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path).map_err(|error| {
        ClientError::CertificateSecrets(format!("failed to open {}: {error}", path.display()))
    })?;
    file.write_all(contents).map_err(|error| {
        ClientError::CertificateSecrets(format!("failed to write {}: {error}", path.display()))
    })?;
    file.sync_all().ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_wrong_key_detection() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let cipher = CertCipher::load_or_create(first.path()).unwrap();
        let blob = cipher.encrypt(b"PRIVATE KEY MATERIAL").unwrap();
        assert_ne!(&blob[..], b"PRIVATE KEY MATERIAL");
        assert_eq!(cipher.decrypt(&blob).unwrap(), b"PRIVATE KEY MATERIAL");

        // Reloading from the same directory reuses the key.
        let reloaded = CertCipher::load_or_create(first.path()).unwrap();
        assert_eq!(reloaded.decrypt(&blob).unwrap(), b"PRIVATE KEY MATERIAL");

        // A different installation must fail closed, not return garbage.
        let other = CertCipher::load_or_create(second.path()).unwrap();
        assert!(other.decrypt(&blob).is_err());
    }

    #[test]
    fn rejects_truncated_blob() {
        let dir = tempfile::tempdir().unwrap();
        let cipher = CertCipher::load_or_create(dir.path()).unwrap();
        assert!(cipher.decrypt(&[0_u8; 4]).is_err());
    }
}
