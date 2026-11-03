//! The short, project-wide checkout lock.
//!
//! [`acquire_project`] creates `.devflow/lock-project` with the holder's PID and
//! serializes mutations of the primary checkout across concurrently finishing
//! phases. It is held for seconds, never across a gate wait.
//!
//! There is no per-phase lock. A phase's state writes take
//! `workflow::update_state`'s write lock, and whether its run is live is read from
//! the supervisor socket, the launch reservation, the gate registration and D-17's
//! per-phase holder file. [`remove_stale_locks`] also removes the `lock-NN` files
//! an older binary left, by the same dead-holder rule.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

static LOCK_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// How long [`acquire_coordination_settling`] waits out a coordination hold
/// that has no lock record behind it before reporting contention.
const COORDINATION_SETTLE_WAIT: std::time::Duration = std::time::Duration::from_millis(500);

/// Poll interval for [`acquire_coordination_settling`].
const COORDINATION_SETTLE_POLL: std::time::Duration = std::time::Duration::from_millis(5);

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

/// What a lock record establishes about its holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HolderClass {
    /// No running process has the recorded pid.
    NoHolder,
    /// A running process matches both recorded halves of its identity.
    Live,
    /// A running process has reused the recorded PID with a different start time.
    Recycled,
    /// A process is running but its identity cannot be checked safely.
    Unconfirmable,
}

/// Acquire the short-held, project-wide lock that serializes mutations of the
/// primary checkout (version-bump commits/tags, docs commits, branch
/// integration/cleanup) across concurrently finishing phases
/// (13-DEFERRED-CR-03 fix shape #3).
///
/// It guards the shared git checkout and is held for seconds. It must NEVER be
/// held across a gate wait.
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

/// [`acquire_coordination`] for a fresh acquisition, waiting out a hold that
/// has no public lock record behind it.
///
/// A live holder publishes its record right after taking coordination and
/// keeps both until its guard drops, and the guard removes the record before
/// it releases coordination. So a busy coordination lock with no record is a
/// publish or a release in progress. It can also outlive a released guard: a
/// child forked while the guard was live holds a duplicate of the
/// coordination fd, and with it the flock, until it execs. Reporting
/// `Contended { pid: "unknown" }` then refuses a phase nobody holds. The wait
/// is bounded, and a record appearing ends it at once with the holder's pid.
fn acquire_coordination_settling(path: &Path) -> Result<File, LockError> {
    let deadline = std::time::Instant::now() + COORDINATION_SETTLE_WAIT;
    loop {
        match acquire_coordination(path) {
            Err(LockError::Contended { .. })
                if !path.exists() && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(COORDINATION_SETTLE_POLL);
            }
            other => return other,
        }
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
    // 999.128 (D-08): a live pid whose start time differs from the recorded one (the
    // recorded holder exited and its pid was reused) counts as stale. Only an
    // identity-confirmed live holder, or one whose identity cannot be checked, keeps
    // the lock.
    let is_running = crate::agent::agent_running(record.pid);
    let observed_start_time = crate::agent::process_start_time(record.pid);
    let pid = record.pid;
    match classify_holder_record(record, is_running, observed_start_time) {
        HolderClass::Live | HolderClass::Unconfirmable => {
            return Err(LockError::Contended {
                pid: pid.to_string(),
                path,
            });
        }
        HolderClass::NoHolder => tracing::warn!(
            "reclaiming stale devflow lock at {} (holder pid {pid} is not alive)",
            path.display(),
        ),
        HolderClass::Recycled => {
            let show =
                |start: Option<u64>| start.map_or_else(|| "unknown".into(), |s| s.to_string());
            tracing::warn!(
                "reclaiming devflow lock at {}: holder pid {pid} is alive but its start time {} \
                 differs from the recorded {} — reclaiming a recycled pid",
                path.display(),
                show(observed_start_time),
                show(record.recorded_start_time)
            );
        }
    }
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
    let coordination = acquire_coordination_settling(&path)?;

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

#[derive(Debug, Clone, Copy)]
struct HolderRecord {
    pid: u32,
    recorded_start_time: Option<u64>,
}

/// Read a lock record without changing the file.
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
) -> HolderClass {
    if !is_running {
        return HolderClass::NoHolder;
    }

    match (record.recorded_start_time, observed_start_time) {
        (Some(recorded), Some(observed)) if recorded == observed => HolderClass::Live,
        (Some(_), Some(_)) => HolderClass::Recycled,
        _ => HolderClass::Unconfirmable,
    }
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

/// Filename prefix of the checkout lock (`lock-project`) and of the `lock-NN` files an
/// older binary left. Owned here so sweepers (e.g. `recover --clean`) never hardcode the
/// naming scheme, and shared deliberately: the same stale-holder sweep covers both.
const LOCK_FILE_PREFIX: &str = "lock-";

pub(crate) enum StaleLockRemovalOutcome {
    Notice(String),
    Failure(String),
}

pub(crate) fn project_lock_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".devflow")
        .join(format!("{LOCK_FILE_PREFIX}project"))
}

/// Remove this project's `lock-*` files, skipping any whose recorded holder PID
/// is still alive — deleting a live holder's checkout lock would let a second
/// holder acquire it, after which the original holder's `LockGuard::Drop` deletes
/// the NEW holder's file.
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

    fn lock_in(dir: &Path) -> PathBuf {
        project_lock_path(dir)
    }

    /// A pid the lock file names, read back through its first line.
    fn recorded_pid(path: &Path) -> String {
        read_holder_pid(path)
    }

    #[test]
    fn acquire_creates_lock_and_records_pid() {
        let dir = tempfile::tempdir().unwrap();
        let guard = acquire_project(dir.path()).expect("acquire");

        let path = lock_in(dir.path());
        assert_eq!(recorded_pid(&path), std::process::id().to_string());
        drop(guard);
    }

    #[test]
    fn acquire_creates_devflow_directory_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(".devflow").exists());
        let _guard = acquire_project(dir.path()).expect("acquire");
        assert!(dir.path().join(".devflow").exists());
    }

    #[test]
    fn second_acquire_is_contended() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = acquire_project(dir.path()).expect("first acquire");

        match acquire_project(dir.path()) {
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
        let path = lock_in(dir.path());
        {
            let _guard = acquire_project(dir.path()).expect("acquire");
            assert!(path.exists());
        }
        // After the guard drops the lock file is gone and re-acquiring works.
        assert!(
            !path.exists(),
            "dropping the guard must remove the public lock path before another acquirer runs"
        );
        let _again = acquire_project(dir.path()).expect("re-acquire after release");
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
    /// removed, an acquire reported `Contended { pid: "unknown" }` for a lock
    /// nobody holds. A coordination hold with no lock record behind it is a
    /// release in progress, and an acquire must wait it out.
    #[test]
    fn acquire_waits_out_a_coordination_hold_left_after_release() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_in(dir.path());
        let held = hold_coordination_elsewhere(&path);
        assert!(!path.exists(), "the public lock path must already be gone");
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            drop(held);
        });
        let acquired = acquire_project(dir.path());
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
        let path = lock_in(dir.path());
        let _held = hold_coordination_elsewhere(&path);
        let started = std::time::Instant::now();
        let refused = acquire_project(dir.path());
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
    fn acquire_treats_an_empty_lock_record_as_contended_without_deleting_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_in(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();

        let err = acquire_project(dir.path())
            .expect_err("an unreadable lock record must not be reclaimed");
        assert!(matches!(err, LockError::Contended { .. }));
        assert_eq!(fs::read(&path).unwrap(), b"");
    }

    #[test]
    fn complete_record_publication_loses_cleanly_to_a_concurrent_acquirer() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_in(dir.path());
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
            acquire_project(dir.path()).expect("second acquirer publishes the complete record");
        allow_publish.wait();
        assert!(matches!(
            first.join().unwrap(),
            LockPublication::AlreadyExists
        ));
        assert_eq!(recorded_pid(&path), std::process::id().to_string());
        drop(guard);
    }

    #[test]
    fn stale_reclaim_is_serialized_before_it_removes_the_lock_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_in(dir.path());
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
        let second = acquire_project(dir.path()).expect_err("second reclaimer must contend");
        assert!(matches!(second, LockError::Contended { .. }));
        allow_reclaim.wait();
        let guard = first
            .join()
            .expect("first reclaimer thread")
            .expect("first reclaimer acquires the replacement lock");
        assert_eq!(recorded_pid(&path), std::process::id().to_string());
        drop(guard);
    }

    #[test]
    fn stale_sweeper_keeps_a_dead_lock_while_reclaim_coordination_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".devflow/lock-01");
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
    fn a_live_pid_without_an_observed_start_time_is_unconfirmable() {
        let pid = std::process::id();
        let record = HolderRecord {
            pid,
            recorded_start_time: Some(1),
        };
        assert_eq!(
            classify_holder_record(record, true, None),
            HolderClass::Unconfirmable
        );
        assert_eq!(
            classify_holder_record(record, false, None),
            HolderClass::NoHolder,
            "control: a pid not running has no holder, identity or not"
        );
    }

    /// 13-06 dogfood regression: a killed poller's abandoned lock wedged
    /// every subsequent `advance` for the project. A lock whose holder pid
    /// is dead (or non-numeric) must be reclaimed transparently.
    #[test]
    fn acquire_reclaims_lock_from_dead_holder() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_in(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Above default kernel pid_max (4194304) — guaranteed not alive.
        fs::write(&path, "9999999").unwrap();

        let guard = acquire_project(dir.path()).expect("stale lock must be reclaimed");
        assert_eq!(recorded_pid(&path), std::process::id().to_string());
        drop(guard);
    }

    /// Write a lock record naming this test process's own (live) pid, with
    /// `recorded_start` as its second line when given. Returns the pid, the
    /// observed start time of this process, and the lock path.
    fn write_own_pid_lock(
        dir: &Path,
        recorded_start: impl FnOnce(u64) -> Option<u64>,
    ) -> (u32, u64, PathBuf) {
        let path = lock_in(dir);
        let pid = std::process::id();
        let observed = crate::agent::process_start_time(pid)
            .expect("the test process must expose its start time");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let record = match recorded_start(observed) {
            Some(start) => format!("{pid}\n{start}"),
            None => pid.to_string(),
        };
        fs::write(&path, record).unwrap();
        (pid, observed, path)
    }

    /// 999.128 (D-08): a live pid whose start time differs from the recorded
    /// one is a recycled pid, not the holder — the recorded holder exited.
    /// Reclaim must treat it like a dead holder.
    #[test]
    fn acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time() {
        let dir = tempfile::tempdir().unwrap();
        let (pid, observed, path) =
            write_own_pid_lock(dir.path(), |start| Some(start.saturating_add(1)));

        let guard = acquire_project(dir.path()).expect("a recycled-pid lock must be reclaimed");

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{pid}\n{observed}"),
            "the reclaimed lock must name the caller with its real start time"
        );
        drop(guard);
    }

    /// Phase 48 D-02 one-writer rule: a live holder whose start time matches
    /// the recorded one is the holder, and is never reclaimed.
    #[test]
    fn acquire_never_reclaims_a_live_holder_whose_start_time_matches() {
        let dir = tempfile::tempdir().unwrap();
        let (pid, _, path) = write_own_pid_lock(dir.path(), Some);
        let before = fs::read(&path).unwrap();

        let err = acquire_project(dir.path()).expect_err("a live matched holder must contend");

        match err {
            LockError::Contended { pid: holder, .. } => assert_eq!(holder, pid.to_string()),
            other => panic!("expected Contended, got {other:?}"),
        }
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "the lock must be untouched"
        );
    }

    /// A live pid with no recorded start time cannot be identity-checked
    /// (Unconfirmable), so reclaim stays conservative and contends.
    #[test]
    fn acquire_never_reclaims_a_live_holder_with_no_recorded_start_time() {
        let dir = tempfile::tempdir().unwrap();
        let (pid, _, path) = write_own_pid_lock(dir.path(), |_| None);
        let before = fs::read(&path).unwrap();

        let err = acquire_project(dir.path()).expect_err("an unconfirmable holder must contend");

        match err {
            LockError::Contended { pid: holder, .. } => assert_eq!(holder, pid.to_string()),
            other => panic!("expected Contended, got {other:?}"),
        }
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "the lock must be untouched"
        );
    }

    #[test]
    fn acquire_treats_a_corrupt_lock_record_as_contended_without_deleting_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = lock_in(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not-a-pid").unwrap();

        let err = acquire_project(dir.path()).expect_err("corrupt lock must not be reclaimed");
        assert!(matches!(err, LockError::Contended { .. }));
        assert_eq!(fs::read(&path).unwrap(), b"not-a-pid");
    }

    /// `remove_stale_locks` must sweep dead-holder locks but never a live
    /// holder's — deleting a held lock lets a duplicate holder acquire it,
    /// and the original guard's Drop then removes the new holder's file.
    #[test]
    fn remove_stale_locks_keeps_live_holder_and_sweeps_dead() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join(".devflow/lock-01");
        let dead = dir.path().join(".devflow/lock-02");
        fs::create_dir_all(live.parent().unwrap()).unwrap();
        fs::write(&live, std::process::id().to_string()).unwrap();
        fs::write(&dead, "9999999").unwrap();

        let warnings = remove_stale_locks(dir.path());

        assert!(live.exists(), "live holder's lock must be kept");
        assert!(!dead.exists(), "dead holder's lock must be swept");
        assert_eq!(warnings.len(), 1, "keeping a live lock must be reported");
        assert!(warnings[0].contains("still alive"));
    }

    /// D-08: there is no per-phase lock any more, so the `lock-NN` an older binary left
    /// (a dead pid, two lines) is only a stale file. `.state-NN.lock`, `.gates-NN.lock`
    /// and `.holder-NN` share no prefix with it and stay.
    #[test]
    fn a_leftover_phase_lock_file_is_removed_as_stale() {
        let dir = tempfile::tempdir().unwrap();
        let devflow = dir.path().join(".devflow");
        fs::create_dir_all(&devflow).unwrap();
        let leftover = devflow.join("lock-07");
        fs::write(&leftover, "9999999\n1").unwrap();
        let kept =
            [".state-07.lock", ".gates-07.lock", ".holder-07"].map(|name| devflow.join(name));
        for path in &kept {
            fs::write(path, "9999999\n1").unwrap();
        }

        let warnings = remove_stale_locks(dir.path());

        assert!(!leftover.exists(), "{warnings:?}");
        for path in &kept {
            assert!(path.exists(), "{} must stay", path.display());
        }
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
        let path = lock_in(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "0").unwrap();

        acquire_project(dir.path()).expect("pid-0 lock must be reclaimed");
    }
}
