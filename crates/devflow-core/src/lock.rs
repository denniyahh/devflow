//! File-based lock to prevent concurrent `devflow advance` invocations for
//! the same phase.
//!
//! Creates `.devflow/lock-{phase:02}` with the PID of the lock holder.
//! Uses O_EXCL for atomic acquisition — if the file already exists,
//! the lock is contended.
//!
//! The lock is scoped per-phase (not per-project): `advance()` holds it
//! across a gate's multi-day blocking wait, and every phase run ends at a
//! mandatory Ship gate, so a project-wide lock would starve `devflow
//! parallel`'s sibling phases with no retry (CR-03, 13-REVIEW.md).

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

/// Acquire an exclusive lock for the given project root and phase.
///
/// Writes the current PID into `.devflow/lock-{phase:02}`. Returns a guard
/// that releases the lock when dropped.
pub fn acquire(project_root: &Path, phase: u32) -> Result<LockGuard, LockError> {
    acquire_path(lock_path(project_root, phase))
}

/// Acquire the short-held, project-wide lock that serializes mutations of the
/// primary checkout (version-bump commits/tags, docs commits, branch
/// integration/cleanup) across concurrently finishing phases
/// (13-DEFERRED-CR-03 fix shape #3).
///
/// This is the second level of the two-level model: per-phase locks guard a
/// phase's own advance (held across gate-days), while this coarse lock guards
/// the shared git checkout and is held for seconds. It must NEVER be held
/// across a gate wait.
pub fn acquire_project(project_root: &Path) -> Result<LockGuard, LockError> {
    acquire_path(project_lock_path(project_root))
}

/// Blocking variant of [`acquire_project`]: waits out a sibling phase's short
/// critical section, polling with backoff up to `timeout`. Returns the last
/// `Contended` error if the sibling still holds the lock at the deadline —
/// after `timeout` of waiting the holder is more likely wedged than slow, and
/// failing loudly beats mutating the checkout concurrently.
pub fn acquire_project_blocking(
    project_root: &Path,
    timeout: std::time::Duration,
) -> Result<LockGuard, LockError> {
    let start = std::time::Instant::now();
    let mut backoff = std::time::Duration::from_millis(100);
    loop {
        match acquire_project(project_root) {
            Ok(guard) => return Ok(guard),
            Err(err @ LockError::Contended { .. }) => {
                if start.elapsed() >= timeout {
                    return Err(err);
                }
                std::thread::sleep(backoff.min(timeout.saturating_sub(start.elapsed())));
                backoff = (backoff * 2).min(std::time::Duration::from_secs(2));
            }
            Err(err) => return Err(err),
        }
    }
}

fn acquire_path(path: PathBuf) -> Result<LockGuard, LockError> {
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
            // Stale-holder recovery (13-06 dogfood finding): a killed or
            // crashed holder never runs LockGuard's Drop, and its abandoned
            // lock wedges every future `advance` for the project — silently,
            // since advance usually runs from a detached monitor with no
            // terminal. If the recorded holder is not alive, reclaim the
            // lock and retry the atomic create once. Best-effort: PID reuse
            // is theoretically possible but the window is negligible for a
            // per-project lock.
            if !pid_is_alive(&pid) {
                tracing::warn!(
                    "reclaiming stale devflow lock at {} (holder pid {pid} is not alive)",
                    path.display()
                );
                let _ = fs::remove_file(&path);
                return match File::create_new(&path) {
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
                };
            }
            Err(LockError::Contended { pid, path })
        }
        Err(err) => Err(err.into()),
    }
}

/// Whether the pid recorded in a lock file refers to a live process.
///
/// A non-numeric pid (corrupt lock) is treated as dead so the lock can be
/// reclaimed. Delegates to [`crate::agent::agent_running`] — the crate's one
/// PID-liveness implementation — which also rejects `0` (a `kill -0 0`
/// probes the caller's own process group and always succeeds, making a
/// corrupted lock permanently "held") and values that would wrap negative
/// through the `pid_t` cast.
fn pid_is_alive(pid: &str) -> bool {
    pid.parse::<u32>().is_ok_and(crate::agent::agent_running)
}

/// Check whether a lock is currently held for this project/phase,
/// returning the PID of the holder if the file exists.
pub fn holder(project_root: &Path, phase: u32) -> Option<(String, PathBuf)> {
    let path = lock_path(project_root, phase);
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
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        release(&self.path);
    }
}

/// Filename prefix shared by every per-phase lock file. Owned here so
/// sweepers (e.g. `recover --clean`) never hardcode the naming scheme.
/// The project-wide checkout lock (`lock-project`) shares the prefix
/// deliberately: the same stale-holder sweep covers it.
const LOCK_FILE_PREFIX: &str = "lock-";

fn lock_path(project_root: &Path, phase: u32) -> PathBuf {
    project_root
        .join(".devflow")
        .join(format!("{LOCK_FILE_PREFIX}{phase:02}"))
}

fn project_lock_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".devflow")
        .join(format!("{LOCK_FILE_PREFIX}project"))
}

/// Remove this project's per-phase lock files, skipping any whose recorded
/// holder PID is still alive — deleting a live holder's lock would let a
/// duplicate `advance` acquire it, after which the original holder's
/// `LockGuard::Drop` deletes the NEW holder's file.
///
/// Returns human-readable warnings for anything skipped or that failed to
/// delete, so callers surface problems instead of reporting a clean sweep
/// that left wedging locks behind.
pub fn remove_stale_locks(project_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let devflow_dir = project_root.join(".devflow");
    let Ok(entries) = fs::read_dir(&devflow_dir) else {
        return warnings;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(LOCK_FILE_PREFIX) {
            continue;
        }
        let path = entry.path();
        let holder_pid = fs::read_to_string(&path)
            .unwrap_or_default()
            .trim()
            .to_string();
        if pid_is_alive(&holder_pid) {
            warnings.push(format!(
                "kept {} — holder pid {holder_pid} is still alive",
                path.display()
            ));
            continue;
        }
        if let Err(err) = fs::remove_file(&path) {
            warnings.push(format!("could not remove {}: {err}", path.display()));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_creates_lock_and_records_pid() {
        let dir = tempfile::tempdir().unwrap();
        let guard = acquire(dir.path(), 1).expect("acquire");

        let (pid, path) = holder(dir.path(), 1).expect("holder present");
        assert_eq!(pid, std::process::id().to_string());
        assert!(path.exists());
        drop(guard);
    }

    #[test]
    fn acquire_creates_devflow_directory_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(".devflow").exists());
        let _guard = acquire(dir.path(), 1).expect("acquire");
        assert!(dir.path().join(".devflow").exists());
    }

    #[test]
    fn second_acquire_is_contended() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = acquire(dir.path(), 1).expect("first acquire");

        match acquire(dir.path(), 1) {
            Err(LockError::Contended { pid, .. }) => {
                assert_eq!(pid, std::process::id().to_string());
            }
            Ok(_) => panic!("second acquire must fail"),
            Err(other) => panic!("expected Contended, got {other:?}"),
        }
    }

    /// CR-03 (13-REVIEW.md): the lock is scoped per-phase, so a different
    /// phase in the same project must never contend on another phase's lock
    /// — this is what lets `devflow parallel`'s sibling phases keep making
    /// progress while one phase blocks on a multi-day gate wait.
    #[test]
    fn different_phases_do_not_contend() {
        let dir = tempfile::tempdir().unwrap();
        let _guard_a = acquire(dir.path(), 1).expect("acquire phase 1");
        let _guard_b = acquire(dir.path(), 2).expect("acquire phase 2 must not contend");
    }

    #[test]
    fn dropping_guard_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _guard = acquire(dir.path(), 1).expect("acquire");
            assert!(holder(dir.path(), 1).is_some());
        }
        // After the guard drops the lock file is gone and re-acquiring works.
        assert!(holder(dir.path(), 1).is_none());
        let _again = acquire(dir.path(), 1).expect("re-acquire after release");
    }

    #[test]
    fn holder_is_none_without_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(holder(dir.path(), 1).is_none());
    }

    #[test]
    fn holder_cleans_up_empty_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), 1);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "   \n").unwrap();

        assert!(holder(dir.path(), 1).is_none());
        // Empty/stale lock should be removed so a fresh acquire succeeds.
        assert!(!path.exists());
        let _guard = acquire(dir.path(), 1).expect("acquire after stale cleanup");
    }

    /// 13-06 dogfood regression: a killed poller's abandoned lock wedged
    /// every subsequent `advance` for the project. A lock whose holder pid
    /// is dead (or non-numeric) must be reclaimed transparently.
    #[test]
    fn acquire_reclaims_lock_from_dead_holder() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), 1);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Above default kernel pid_max (4194304) — guaranteed not alive.
        fs::write(&path, "9999999").unwrap();

        let guard = acquire(dir.path(), 1).expect("stale lock must be reclaimed");
        let (pid, _) = holder(dir.path(), 1).expect("holder present");
        assert_eq!(pid, std::process::id().to_string());
        drop(guard);
    }

    #[test]
    fn acquire_reclaims_lock_with_corrupt_pid() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), 1);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not-a-pid").unwrap();

        acquire(dir.path(), 1).expect("corrupt lock must be reclaimed");
    }

    /// `remove_stale_locks` must sweep dead-holder locks but never a live
    /// holder's — deleting a held lock lets a duplicate advance acquire it,
    /// and the original guard's Drop then removes the new holder's file.
    #[test]
    fn remove_stale_locks_keeps_live_holder_and_sweeps_dead() {
        let dir = tempfile::tempdir().unwrap();
        let live = lock_path(dir.path(), 1);
        let dead = lock_path(dir.path(), 2);
        fs::create_dir_all(live.parent().unwrap()).unwrap();
        fs::write(&live, std::process::id().to_string()).unwrap();
        fs::write(&dead, "9999999").unwrap();

        let warnings = remove_stale_locks(dir.path());

        assert!(live.exists(), "live holder's lock must be kept");
        assert!(!dead.exists(), "dead holder's lock must be swept");
        assert_eq!(warnings.len(), 1, "keeping a live lock must be reported");
        assert!(warnings[0].contains("still alive"));
    }

    /// Two-level locking (13-DEFERRED-CR-03): the coarse checkout lock is
    /// independent of every per-phase lock — holding a phase's advance lock
    /// (potentially for gate-days) must not block another phase's
    /// seconds-long checkout mutation, and vice versa.
    #[test]
    fn project_lock_is_independent_of_phase_locks() {
        let dir = tempfile::tempdir().unwrap();
        let _phase = acquire(dir.path(), 1).expect("phase lock");
        let _project = acquire_project(dir.path()).expect("project lock must not contend");
    }

    #[test]
    fn project_lock_contends_with_itself() {
        let dir = tempfile::tempdir().unwrap();
        let _held = acquire_project(dir.path()).expect("first acquire");
        assert!(matches!(
            acquire_project(dir.path()),
            Err(LockError::Contended { .. })
        ));
    }

    /// The blocking variant must wait out a short critical section instead of
    /// failing fast — that is the whole point of a seconds-scale coarse lock.
    #[test]
    fn project_lock_blocking_waits_for_release() {
        let dir = tempfile::tempdir().unwrap();
        let held = acquire_project(dir.path()).expect("first acquire");
        let root = dir.path().to_path_buf();

        std::thread::scope(|scope| {
            let waiter = scope
                .spawn(move || acquire_project_blocking(&root, std::time::Duration::from_secs(10)));
            // Give the waiter time to hit contention at least once, then
            // release; it must then acquire rather than time out.
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(held);
            waiter
                .join()
                .expect("waiter thread")
                .expect("blocking acquire must succeed once the holder releases");
        });
    }

    #[test]
    fn project_lock_blocking_times_out_against_live_holder() {
        let dir = tempfile::tempdir().unwrap();
        let _held = acquire_project(dir.path()).expect("first acquire");
        let err = acquire_project_blocking(dir.path(), std::time::Duration::from_millis(300))
            .expect_err("must time out while the live holder keeps the lock");
        assert!(matches!(err, LockError::Contended { .. }));
    }

    /// A lock file containing "0" parses as a valid u32, but `kill -0 0`
    /// probes the caller's own process group and always succeeds — the old
    /// subprocess-based check treated it as a live holder forever, wedging
    /// every future acquire behind a Contended error.
    #[test]
    fn acquire_reclaims_lock_with_pid_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), 1);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "0").unwrap();

        acquire(dir.path(), 1).expect("pid-0 lock must be reclaimed");
    }
}
