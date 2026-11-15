//! Exclusive ownership of a RustFRP data directory.
//!
//! Two RustFRP processes sharing one SQLite database would each run their own
//! `ProcessManager`, so the same profile would get two `frpc` children bound to
//! the same server. The daemon and the desktop shell therefore take an exclusive
//! advisory lock on `<data dir>/instance.lock` before touching the database.
//!
//! The lock uses `flock(2)` (Unix) / `LockFileEx` (Windows) through
//! [`std::fs::File::try_lock`], so it disappears with the process — there is no
//! stale PID file to reap.

use std::fs::{File, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Name of the lock file inside the data directory.
pub const LOCK_FILE_NAME: &str = "instance.lock";

/// Why the data directory could not be claimed.
#[derive(Debug, thiserror::Error)]
pub enum InstanceLockError {
    /// Another process already holds the lock.
    #[error("another RustFRP instance is using this data directory ({0})")]
    AlreadyLocked(PathBuf),

    /// The lock file could not be created or the lock could not be acquired.
    #[error("cannot lock {path}: {source}")]
    Io {
        /// Lock file path.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
}

/// Held for as long as the owning process runs.
///
/// Dropping it releases the lock; process exit releases it as well.
#[derive(Debug)]
pub struct InstanceLock {
    file: File,
    path: PathBuf,
}

impl InstanceLock {
    /// Claim `dir` for this process, creating the directory if needed.
    ///
    /// Never blocks: a held lock returns [`InstanceLockError::AlreadyLocked`].
    pub fn acquire(dir: impl AsRef<Path>) -> Result<Self, InstanceLockError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|source| InstanceLockError::Io {
            path: dir.to_path_buf(),
            source,
        })?;

        let path = dir.join(LOCK_FILE_NAME);
        // Open without `truncate`: the file may belong to a live owner whose
        // PID line is the only hint of which process to stop.
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| InstanceLockError::Io {
                path: path.clone(),
                source,
            })?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(InstanceLockError::AlreadyLocked(path));
            }
            Err(TryLockError::Error(source)) => {
                return Err(InstanceLockError::Io { path, source });
            }
        }

        // Diagnostic only: the lock is the file handle, not the contents.
        let _ = file.set_len(0);
        let _ = writeln!(file, "{}", std::process::id());

        Ok(Self { file, path })
    }

    /// Path of the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_in_same_process_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let first = InstanceLock::acquire(dir.path()).unwrap();
        assert!(first.path().exists());

        let error = InstanceLock::acquire(dir.path()).unwrap_err();
        assert!(
            matches!(error, InstanceLockError::AlreadyLocked(_)),
            "expected AlreadyLocked, got {error:?}"
        );

        drop(first);
        InstanceLock::acquire(dir.path()).expect("lock must be reusable after release");
    }

    #[test]
    fn acquire_creates_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("data");
        let lock = InstanceLock::acquire(&nested).unwrap();
        assert_eq!(lock.path(), nested.join(LOCK_FILE_NAME));
        assert_eq!(
            std::fs::read_to_string(lock.path()).unwrap().trim(),
            std::process::id().to_string()
        );
    }
}
