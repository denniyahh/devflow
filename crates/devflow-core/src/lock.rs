//! File-based lock to prevent concurrent `devflow check` invocations.
//!
//! Creates `.devflow/lock` with the PID of the lock holder.
//! Uses O_EXCL for atomic acquisition — if the file already exists,
//! the lock is contended.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Errors produced by lock operations.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Lock file already exists — another process holds it.
    #[error("lock already held by pid {pid} at {path}")]
    Contended { pid: String, path: PathBuf },
    /// Filesystem operation failed.
    #[error("lock I/O failed: {0}")]
    Io(#[from] io::Error),
}

/// Acquire an exclusive lock for the given project root.
///
/// Writes the current PID into `.devflow/lock`. Returns a guard
/// that releases the lock when dropped.
pub fn acquire(project_root: &Path) -> Result<LockGuard, LockError> {
    let path = lock_path(project_root);
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "lock path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;

    match File::create_new(&path) {
        Ok(mut f) => {
            let pid = std::process::id().to_string();
            write!(f, "{pid}")?;
            Ok(LockGuard { path })
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let pid = fs::read_to_string(&path)
                .unwrap_or_else(|_| "unknown".into())
                .trim()
                .to_string();
            Err(LockError::Contended { pid, path })
        }
        Err(err) => Err(err.into()),
    }
}

/// Check whether a lock is currently held for this project,
/// returning the PID of the holder if the file exists.
pub fn holder(project_root: &Path) -> Option<(String, PathBuf)> {
    let path = lock_path(project_root);
    let pid = fs::read_to_string(&path).ok()?;
    let pid = pid.trim().to_string();
    if pid.is_empty() {
        // Stale empty lock file — clean it up
        let _ = fs::remove_file(&path);
        return None;
    }
    Some((pid, path))
}

/// Release a lock by removing the lock file, ignoring errors
/// if it's already gone.
fn release(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Guard that releases the lock file on drop.
pub struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        release(&self.path);
    }
}

fn lock_path(project_root: &Path) -> PathBuf {
    project_root.join(".devflow").join("lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_creates_lock_and_records_pid() {
        let dir = tempfile::tempdir().unwrap();
        let guard = acquire(dir.path()).expect("acquire");

        let (pid, path) = holder(dir.path()).expect("holder present");
        assert_eq!(pid, std::process::id().to_string());
        assert!(path.exists());
        drop(guard);
    }

    #[test]
    fn acquire_creates_devflow_directory_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(".devflow").exists());
        let _guard = acquire(dir.path()).expect("acquire");
        assert!(dir.path().join(".devflow").exists());
    }

    #[test]
    fn second_acquire_is_contended() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = acquire(dir.path()).expect("first acquire");

        match acquire(dir.path()) {
            Err(LockError::Contended { pid, .. }) => {
                assert_eq!(pid, std::process::id().to_string());
            }
            Ok(_) => panic!("second acquire must fail"),
            Err(other) => panic!("expected Contended, got {other:?}"),
        }
    }

    #[test]
    fn dropping_guard_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _guard = acquire(dir.path()).expect("acquire");
            assert!(holder(dir.path()).is_some());
        }
        // After the guard drops the lock file is gone and re-acquiring works.
        assert!(holder(dir.path()).is_none());
        let _again = acquire(dir.path()).expect("re-acquire after release");
    }

    #[test]
    fn holder_is_none_without_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(holder(dir.path()).is_none());
    }

    #[test]
    fn holder_cleans_up_empty_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "   \n").unwrap();

        assert!(holder(dir.path()).is_none());
        // Empty/stale lock should be removed so a fresh acquire succeeds.
        assert!(!path.exists());
        let _guard = acquire(dir.path()).expect("acquire after stale cleanup");
    }
}
