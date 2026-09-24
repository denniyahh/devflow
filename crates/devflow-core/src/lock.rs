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

use crate::phase_id::PhaseId;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

static LOCK_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

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

/// What a read-only inspection can establish about a phase lock's holder.
///
/// A [`Recycled`](Self::Recycled) PID is not a waiter: the lock file names a
/// process instance that has exited, even though that numeric PID now belongs
/// to another live process. [`Unconfirmable`](Self::Unconfirmable) remains
/// conservatively waiter-like because the process exists but its identity
/// cannot be established from the lock record and `/proc` together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolderStatus {
    /// No readable lock record names a currently running process.
    NoHolder,
    /// A running process matches both recorded halves of its identity.
    Live { pid: u32 },
    /// A running process has reused the recorded PID with a different start time.
    Recycled { pid: u32 },
    /// A process is running but its identity cannot be checked safely.
    Unconfirmable { pid: u32 },
}

impl HolderStatus {
    /// Whether this status may still describe a process waiting on a gate.
    ///
    /// Command responses can treat [`Recycled`](Self::Recycled) as no waiter,
    /// while destructive recovery must still use lock acquisition instead of
    /// this observational result to serialize any cleanup.
    pub fn may_be_waiting(self) -> bool {
        match self {
            Self::NoHolder | Self::Recycled { .. } => false,
            Self::Live { .. } | Self::Unconfirmable { .. } => true,
        }
    }
}

/// Acquire an exclusive lock for the given project root and phase.
///
/// Writes the current PID into `.devflow/lock-{phase:02}`. Returns a guard
/// that releases the lock when dropped.
pub fn acquire(project_root: &Path, phase: PhaseId) -> Result<LockGuard, LockError> {
    acquire_path(lock_path(project_root, phase))
}

/// Blocking variant of [`acquire`]: waits out a live phase-lock holder,
/// retrying acquisition so a stale lock is reclaimed by [`acquire`].
pub fn acquire_blocking(
    project_root: &Path,
    phase: PhaseId,
    timeout: std::time::Duration,
) -> Result<LockGuard, LockError> {
    let start = std::time::Instant::now();
    let mut backoff = std::time::Duration::from_millis(100);
    loop {
        match acquire(project_root, phase) {
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

enum LockPublication {
    Published,
    AlreadyExists,
}

/// Publish a fully written lock record without ever exposing an empty lock path.
fn publish_lock(path: &Path, before_publish: impl FnOnce()) -> io::Result<LockPublication> {
    let (tmp, mut file) =
        crate::workflow::create_unique_temp(path, std::process::id(), &LOCK_TEMP_SEQ)?;
    if let Err(error) = file
        .write_all(lock_contents().as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    drop(file);

    before_publish();
    let publish = fs::hard_link(&tmp, path);
    let cleanup = fs::remove_file(&tmp);
    match publish {
        Ok(()) => {
            if let Err(error) = cleanup {
                tracing::warn!(
                    "published devflow lock at {} but could not remove temporary {}: {error}",
                    path.display(),
                    tmp.display()
                );
            }
            Ok(LockPublication::Published)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = cleanup;
            Ok(LockPublication::AlreadyExists)
        }
        Err(error) => {
            let _ = cleanup;
            Err(error)
        }
    }
}

fn contended(path: PathBuf) -> LockError {
    LockError::Contended {
        pid: read_holder_pid(&path),
        path,
    }
}

/// The advisory coordination inode is never replaced or removed by DevFlow.
/// It serializes all mutation of the replaceable public lock pathname.
fn coordination_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.coord"))
}

fn acquire_coordination(path: &Path) -> Result<File, LockError> {
    let coordination = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(coordination_path(path))?;
    match coordination.try_lock() {
        Ok(()) => Ok(coordination),
        Err(TryLockError::WouldBlock) => Err(contended(path.to_path_buf())),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

fn acquire_after_existing(
    path: PathBuf,
    coordination: File,
    before_reclaim: impl FnOnce(),
) -> Result<LockGuard, LockError> {
    let Some(record) = read_holder_record(&path) else {
        return Err(contended(path));
    };
    if crate::agent::agent_running(record.pid) {
        return Err(LockError::Contended {
            pid: record.pid.to_string(),
            path,
        });
    }

    tracing::warn!(
        "reclaiming stale devflow lock at {} (holder pid {} is not alive)",
        path.display(),
        record.pid
    );
    before_reclaim();
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    match publish_lock(&path, || {})? {
        LockPublication::Published => Ok(LockGuard {
            path,
            _coordination: coordination,
        }),
        LockPublication::AlreadyExists => Err(contended(path)),
    }
}

fn acquire_path(path: PathBuf) -> Result<LockGuard, LockError> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "lock path has no parent directory",
        )
    })?;
    crate::workflow::ensure_devflow_dir(parent)?;
    let coordination = acquire_coordination(&path)?;

    match publish_lock(&path, || {})? {
        LockPublication::Published => Ok(LockGuard {
            path,
            _coordination: coordination,
        }),
        LockPublication::AlreadyExists => acquire_after_existing(path, coordination, || {}),
    }
}

/// The lock file's contents: the holder's pid on line 1, and its start time
/// (`/proc/<pid>/stat` field 22) on line 2 when readable.
///
/// Two lines rather than one field, deliberately: every existing reader takes
/// the first line, so the format stays backward compatible in both
/// directions — an old binary reads a new lock file's pid correctly, and a
/// new binary reads an old single-line lock file with the start time simply
/// absent.
///
/// The start time is what makes the record an *identity* rather than a
/// number. See [`crate::agent::process_start_time`]: a pid alone can be
/// recycled, and a pid inspected via `/proc` can also be a devflow process's
/// own child caught mid-`execve` (999.47), so anything that later signals
/// this holder must match both halves.
fn lock_contents() -> String {
    let pid = std::process::id();
    match crate::agent::process_start_time(pid) {
        Some(start) => format!("{pid}\n{start}"),
        // Fail soft on write, fail closed on use: a lock without a start
        // time still works for mutual exclusion, and readers that need
        // identity refuse to signal rather than guess.
        None => format!("{pid}"),
    }
}

/// The holder pid recorded in a lock file — the first line only, so a
/// two-line lock file parses identically to the historical one-line form.
fn read_holder_pid(path: &Path) -> String {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| text.lines().next().map(|line| line.trim().to_string()))
        .filter(|pid| !pid.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// The holder's recorded start time, if the lock file carries one. `None`
/// for a legacy single-line lock, or when the value is unparseable.
fn read_holder_start_time(path: &Path) -> Option<u64> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .nth(1)?
        .trim()
        .parse::<u64>()
        .ok()
}

#[derive(Debug, Clone, Copy)]
struct HolderRecord {
    pid: u32,
    recorded_start_time: Option<u64>,
}

/// Read a lock record without changing the file.
///
/// This parser belongs only to [`holder_status`]. Unlike [`holder`], status
/// inspection must let doctor report empty and corrupt artifacts without
/// deleting them.
fn parse_holder_record(text: &str) -> Option<HolderRecord> {
    let mut lines = text.lines();
    let pid = lines.next()?.trim().parse::<u32>().ok()?;
    let recorded_start_time = lines.next().and_then(|line| line.trim().parse().ok());
    Some(HolderRecord {
        pid,
        recorded_start_time,
    })
}

fn read_holder_record(path: &Path) -> Option<HolderRecord> {
    parse_holder_record(&fs::read_to_string(path).ok()?)
}

fn classify_holder_record(
    record: HolderRecord,
    is_running: bool,
    observed_start_time: Option<u64>,
) -> HolderStatus {
    if !is_running {
        return HolderStatus::NoHolder;
    }

    match (record.recorded_start_time, observed_start_time) {
        (Some(recorded), Some(observed)) if recorded == observed => {
            HolderStatus::Live { pid: record.pid }
        }
        (Some(_), Some(_)) => HolderStatus::Recycled { pid: record.pid },
        _ => HolderStatus::Unconfirmable { pid: record.pid },
    }
}

/// Read the current phase lock without changing it and classify its holder.
///
/// Empty, unreadable, corrupt, and dead-holder records are all
/// [`HolderStatus::NoHolder`]. A live PID is only [`HolderStatus::Live`] when
/// the recorded start time matches the observed process start time; missing
/// identity data is [`HolderStatus::Unconfirmable`] rather than an unsafe
/// guess.
pub fn holder_status(project_root: &Path, phase: PhaseId) -> HolderStatus {
    let path = lock_path(project_root, phase);
    let Some(record) = read_holder_record(&path) else {
        return HolderStatus::NoHolder;
    };
    classify_holder_record(
        record,
        crate::agent::agent_running(record.pid),
        crate::agent::process_start_time(record.pid),
    )
}

/// The recorded identity of a phase lock's holder: its pid, and its start
/// time when the lock records one.
///
/// Callers that intend to **signal** the holder must require the start time
/// to be present and to match [`crate::agent::process_start_time`] for that
/// pid. A `None` start time means the lock predates identity recording and
/// the holder cannot be confirmed — refuse, do not guess.
pub fn holder_identity(project_root: &Path, phase: PhaseId) -> Option<(u32, Option<u64>)> {
    let path = lock_path(project_root, phase);
    let pid = read_holder_pid(&path).parse::<u32>().ok()?;
    Some((pid, read_holder_start_time(&path)))
}

/// Check whether a lock is currently held for this project/phase,
/// returning the PID of the holder if the file exists.
pub fn holder(project_root: &Path, phase: PhaseId) -> Option<(String, PathBuf)> {
    let path = lock_path(project_root, phase);
    // First line only: lock files now carry the holder's start time on line
    // 2, and reading the whole file would yield "1234\n5678" as the "pid".
    fs::read_to_string(&path).ok()?;
    let pid = read_holder_pid(&path);
    let pid = if pid == "unknown" { String::new() } else { pid };
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
    // Kept alive until after Drop removes `path`, so a later acquirer cannot
    // classify or reclaim a replaced lock while this guard owns the phase.
    _coordination: File,
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

pub(crate) enum StaleLockRemovalOutcome {
    Notice(String),
    Failure(String),
}

pub(crate) fn lock_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    project_root.join(".devflow").join(format!(
        "{LOCK_FILE_PREFIX}{padded}",
        padded = phase.padded()
    ))
}

pub(crate) fn project_lock_path(project_root: &Path) -> PathBuf {
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
    remove_stale_locks_with_outcomes(project_root)
        .into_iter()
        .map(|outcome| match outcome {
            StaleLockRemovalOutcome::Notice(message)
            | StaleLockRemovalOutcome::Failure(message) => message,
        })
        .collect()
}

pub(crate) fn remove_stale_locks_with_outcomes(
    project_root: &Path,
) -> Vec<StaleLockRemovalOutcome> {
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
        let coordination = match acquire_coordination(&path) {
            Ok(coordination) => coordination,
            Err(LockError::Contended { .. }) => {
                warnings.push(StaleLockRemovalOutcome::Notice(format!(
                    "kept {} — lock coordination is busy",
                    path.display()
                )));
                continue;
            }
            Err(error) => {
                warnings.push(StaleLockRemovalOutcome::Failure(format!(
                    "could not coordinate {}: {error}",
                    path.display()
                )));
                continue;
            }
        };
        let Some(record) = read_holder_record(&path) else {
            warnings.push(StaleLockRemovalOutcome::Notice(format!(
                "kept {} — lock record is unreadable",
                path.display()
            )));
            continue;
        };
        if crate::agent::agent_running(record.pid) {
            warnings.push(StaleLockRemovalOutcome::Notice(format!(
                "kept {} — holder pid {} is still alive",
                path.display(),
                record.pid
            )));
            drop(coordination);
            continue;
        }
        if let Err(err) = fs::remove_file(&path) {
            warnings.push(StaleLockRemovalOutcome::Failure(format!(
                "could not remove {}: {err}",
                path.display()
            )));
        }
        drop(coordination);
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_creates_lock_and_records_pid() {
        let dir = tempfile::tempdir().unwrap();
        let guard = acquire(dir.path(), PhaseId::new(1)).expect("acquire");

        let (pid, path) = holder(dir.path(), PhaseId::new(1)).expect("holder present");
        assert_eq!(pid, std::process::id().to_string());
        assert!(path.exists());
        drop(guard);
    }

    #[test]
    fn acquire_creates_devflow_directory_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(".devflow").exists());
        let _guard = acquire(dir.path(), PhaseId::new(1)).expect("acquire");
        assert!(dir.path().join(".devflow").exists());
    }

    #[test]
    fn second_acquire_is_contended() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = acquire(dir.path(), PhaseId::new(1)).expect("first acquire");

        match acquire(dir.path(), PhaseId::new(1)) {
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
        let _guard_a = acquire(dir.path(), PhaseId::new(1)).expect("acquire phase 1");
        let _guard_b =
            acquire(dir.path(), PhaseId::new(2)).expect("acquire phase 2 must not contend");
    }

    #[test]
    fn dropping_guard_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), PhaseId::new(1));
        {
            let _guard = acquire(dir.path(), PhaseId::new(1)).expect("acquire");
            assert!(holder(dir.path(), PhaseId::new(1)).is_some());
        }
        // After the guard drops the lock file is gone and re-acquiring works.
        assert!(
            !path.exists(),
            "dropping the guard must remove the public lock path before another acquirer runs"
        );
        assert!(holder(dir.path(), PhaseId::new(1)).is_none());
        let _again = acquire(dir.path(), PhaseId::new(1)).expect("re-acquire after release");
    }

    /// Hold `path`'s coordination lock through a separate open file
    /// description, the way a child forked mid-guard keeps it until it execs.
    fn hold_coordination_elsewhere(path: &Path) -> File {
        fs::create_dir_all(path.parent().expect("lock path has a parent")).unwrap();
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(coordination_path(path))
            .unwrap();
        held.try_lock().expect("hold coordination");
        held
    }

    /// CI failure of `dropping_guard_releases_lock`: a child forked by another
    /// test thread while a guard was live kept a duplicate of the coordination
    /// fd, so the flock outlived the guard. With the public lock path already
    /// removed, `acquire` reported `Contended { pid: "unknown" }` for a phase
    /// nobody holds. A coordination hold with no lock record behind it is a
    /// release in progress, and `acquire` must wait it out.
    #[test]
    fn acquire_waits_out_a_coordination_hold_left_after_release() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), PhaseId::new(1));
        let held = hold_coordination_elsewhere(&path);
        assert!(!path.exists(), "the public lock path must already be gone");
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            drop(held);
        });
        let acquired = acquire(dir.path(), PhaseId::new(1));
        releaser.join().unwrap();
        assert!(
            acquired.is_ok(),
            "a coordination hold with no lock record must be waited out, got {acquired:?}"
        );
    }

    /// Control for the wait above: a coordination hold that never ends is
    /// still refused, and within a bounded time.
    #[test]
    fn acquire_still_refuses_a_coordination_hold_that_does_not_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), PhaseId::new(1));
        let _held = hold_coordination_elsewhere(&path);
        let started = std::time::Instant::now();
        let refused = acquire(dir.path(), PhaseId::new(1));
        assert!(
            matches!(refused, Err(LockError::Contended { .. })),
            "a hold that never ends must still be contended, got {refused:?}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "the wait must be bounded, took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn holder_is_none_without_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(holder(dir.path(), PhaseId::new(1)).is_none());
    }

    #[test]
    fn holder_cleans_up_empty_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), PhaseId::new(1));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "   \n").unwrap();

        assert!(holder(dir.path(), PhaseId::new(1)).is_none());
        // Empty/stale lock should be removed so a fresh acquire succeeds.
        assert!(!path.exists());
        let _guard = acquire(dir.path(), PhaseId::new(1)).expect("acquire after stale cleanup");
    }

    #[test]
    fn acquire_treats_an_empty_lock_record_as_contended_without_deleting_it() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let path = lock_path(dir.path(), phase);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();

        let err = acquire(dir.path(), phase)
            .expect_err("an unreadable lock record must not be reclaimed");
        assert!(matches!(err, LockError::Contended { .. }));
        assert_eq!(fs::read(&path).unwrap(), b"");
    }

    #[test]
    fn complete_record_publication_loses_cleanly_to_a_concurrent_acquirer() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let path = lock_path(dir.path(), phase);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let temp_ready = std::sync::Arc::new(std::sync::Barrier::new(2));
        let allow_publish = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first_path = path.clone();
        let first_ready = temp_ready.clone();
        let first_publish = allow_publish.clone();

        let first = std::thread::spawn(move || {
            publish_lock(&first_path, || {
                first_ready.wait();
                first_publish.wait();
            })
            .expect("first publication attempt")
        });

        temp_ready.wait();
        let guard =
            acquire(dir.path(), phase).expect("second acquirer publishes the complete record");
        allow_publish.wait();
        assert!(matches!(
            first.join().unwrap(),
            LockPublication::AlreadyExists
        ));
        assert_eq!(
            holder_status(dir.path(), phase),
            HolderStatus::Live {
                pid: std::process::id()
            }
        );
        drop(guard);
    }

    #[test]
    fn stale_reclaim_is_serialized_before_it_removes_the_lock_path() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let path = lock_path(dir.path(), phase);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "9999999\n1").unwrap();
        let reclaim_ready = std::sync::Arc::new(std::sync::Barrier::new(2));
        let allow_reclaim = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first_path = path.clone();
        let first_ready = reclaim_ready.clone();
        let first_allow = allow_reclaim.clone();

        let first = std::thread::spawn(move || {
            let coordination =
                acquire_coordination(&first_path).expect("first reclaimer coordinates");
            acquire_after_existing(first_path, coordination, || {
                first_ready.wait();
                first_allow.wait();
            })
        });

        reclaim_ready.wait();
        let second = acquire(dir.path(), phase).expect_err("second reclaimer must contend");
        assert!(matches!(second, LockError::Contended { .. }));
        allow_reclaim.wait();
        let guard = first
            .join()
            .expect("first reclaimer thread")
            .expect("first reclaimer acquires the replacement lock");
        assert_eq!(
            holder_status(dir.path(), phase),
            HolderStatus::Live {
                pid: std::process::id()
            }
        );
        drop(guard);
    }

    #[test]
    fn stale_sweeper_keeps_a_dead_lock_while_reclaim_coordination_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let path = lock_path(dir.path(), phase);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "9999999\n1").unwrap();
        let coordination = acquire_coordination(&path).expect("hold reclaim coordination");

        let warnings = remove_stale_locks(dir.path());

        assert!(
            path.exists(),
            "sweeper must not unlink during coordinated reclaim"
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("coordination is busy")),
            "busy coordination must be reported: {warnings:?}"
        );
        drop(coordination);
    }

    #[test]
    fn holder_status_reports_no_holder_and_preserves_empty_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let path = lock_path(dir.path(), phase);

        assert_eq!(holder_status(dir.path(), phase), HolderStatus::NoHolder);
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        fs::write(&path, "not-a-pid").unwrap();
        assert_eq!(holder_status(dir.path(), phase), HolderStatus::NoHolder);

        fs::write(&path, "9999999").unwrap();
        assert_eq!(holder_status(dir.path(), phase), HolderStatus::NoHolder);

        fs::write(&path, "   \n").unwrap();
        assert_eq!(holder_status(dir.path(), phase), HolderStatus::NoHolder);
        assert!(
            path.exists(),
            "holder status inspection must preserve an empty lock file"
        );
    }

    #[test]
    fn holder_status_reports_live_for_an_identity_matched_lock() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let _guard = acquire(dir.path(), phase).expect("acquire");

        assert_eq!(
            holder_status(dir.path(), phase),
            HolderStatus::Live {
                pid: std::process::id()
            }
        );
    }

    #[test]
    fn holder_status_distinguishes_recycled_and_unconfirmable() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let path = lock_path(dir.path(), phase);
        let pid = std::process::id();
        let start = crate::agent::process_start_time(pid)
            .expect("the test process must expose its start time");
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        fs::write(&path, format!("{pid}\n{}", start.saturating_add(1))).unwrap();
        assert_eq!(
            holder_status(dir.path(), phase),
            HolderStatus::Recycled { pid }
        );

        fs::write(&path, pid.to_string()).unwrap();
        assert_eq!(
            holder_status(dir.path(), phase),
            HolderStatus::Unconfirmable { pid }
        );

        fs::write(&path, format!("{pid}\nnot-a-start-time")).unwrap();
        assert_eq!(
            holder_status(dir.path(), phase),
            HolderStatus::Unconfirmable { pid }
        );

        assert_eq!(
            classify_holder_record(
                HolderRecord {
                    pid,
                    recorded_start_time: Some(start),
                },
                true,
                None,
            ),
            HolderStatus::Unconfirmable { pid },
            "a live pid with an unavailable observed start time is unconfirmable"
        );
    }

    #[test]
    fn holder_status_may_be_waiting_only_for_live_and_unconfirmable() {
        let pid = std::process::id();
        assert!(!HolderStatus::NoHolder.may_be_waiting());
        assert!(HolderStatus::Live { pid }.may_be_waiting());
        assert!(!HolderStatus::Recycled { pid }.may_be_waiting());
        assert!(HolderStatus::Unconfirmable { pid }.may_be_waiting());
    }

    /// 13-06 dogfood regression: a killed poller's abandoned lock wedged
    /// every subsequent `advance` for the project. A lock whose holder pid
    /// is dead (or non-numeric) must be reclaimed transparently.
    #[test]
    fn acquire_reclaims_lock_from_dead_holder() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_path(dir.path(), PhaseId::new(1));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Above default kernel pid_max (4194304) — guaranteed not alive.
        fs::write(&path, "9999999").unwrap();

        let guard = acquire(dir.path(), PhaseId::new(1)).expect("stale lock must be reclaimed");
        let (pid, _) = holder(dir.path(), PhaseId::new(1)).expect("holder present");
        assert_eq!(pid, std::process::id().to_string());
        drop(guard);
    }

    #[test]
    fn acquire_treats_a_corrupt_lock_record_as_contended_without_deleting_it() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let path = lock_path(dir.path(), phase);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not-a-pid").unwrap();

        let err = acquire(dir.path(), phase).expect_err("corrupt lock must not be reclaimed");
        assert!(matches!(err, LockError::Contended { .. }));
        assert_eq!(fs::read(&path).unwrap(), b"not-a-pid");
    }

    /// `remove_stale_locks` must sweep dead-holder locks but never a live
    /// holder's — deleting a held lock lets a duplicate advance acquire it,
    /// and the original guard's Drop then removes the new holder's file.
    #[test]
    fn remove_stale_locks_keeps_live_holder_and_sweeps_dead() {
        let dir = tempfile::tempdir().unwrap();
        let live = lock_path(dir.path(), PhaseId::new(1));
        let dead = lock_path(dir.path(), PhaseId::new(2));
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
        let _phase = acquire(dir.path(), PhaseId::new(1)).expect("phase lock");
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

    #[test]
    fn phase_lock_blocking_waits_for_release() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let held = acquire(dir.path(), phase).expect("first acquire");
        let root = dir.path().to_path_buf();

        std::thread::scope(|scope| {
            let waiter = scope
                .spawn(move || acquire_blocking(&root, phase, std::time::Duration::from_secs(10)));
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(held);
            waiter
                .join()
                .expect("waiter thread")
                .expect("blocking acquire must succeed once the holder releases");
        });
    }

    #[test]
    fn phase_lock_blocking_times_out_against_live_holder() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(1);
        let _held = acquire(dir.path(), phase).expect("first acquire");
        let err = acquire_blocking(dir.path(), phase, std::time::Duration::from_millis(300))
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
        let path = lock_path(dir.path(), PhaseId::new(1));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "0").unwrap();

        acquire(dir.path(), PhaseId::new(1)).expect("pid-0 lock must be reclaimed");
    }
}
