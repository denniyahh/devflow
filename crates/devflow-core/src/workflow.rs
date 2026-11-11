//! Workflow state persistence helpers.
//!
//! State is per-phase: each active phase persists to
//! `.devflow/state-{phase:02}.json` (mirroring the per-phase lock naming), so
//! phases running at the same time never clobber one another
//! (13-DEFERRED-CR-03). A single-slot `.devflow/state.json` left by an older
//! binary is not read, renamed or removed (Phase 50 D-20).

use crate::phase_id::PhaseId;
use crate::state::State;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, warn};

/// Errors produced while reading or writing workflow state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WorkflowError {
    /// Filesystem operation failed.
    #[error("state I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// An orphaned state-write temporary file could not be removed.
    #[error("could not remove orphaned state temp {path}: {source}")]
    StateTempRemoval {
        /// Temporary path whose removal failed.
        path: PathBuf,
        /// Filesystem error returned while removing the temporary path.
        #[source]
        source: std::io::Error,
    },
    /// JSON parse or serialization failed.
    #[error("state JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// No active state file exists.
    #[error("no active DevFlow state at {0}")]
    MissingState(PathBuf),
    /// A closure returned a state for another phase than the one it was given.
    #[error("the state to write is for phase {found}, not phase {expected}")]
    PhaseMismatch {
        /// The phase whose state file is being written.
        expected: PhaseId,
        /// The phase the returned state names.
        found: PhaseId,
    },
    /// The bounded form could not take the phase's state write lock in time.
    #[error(
        "phase {phase}'s state write lock is still held after {waited:?} by {}; nothing was written",
        write_lock_holder_phrase(*.holder)
    )]
    WriteLockTimeout {
        /// The phase whose lock is held.
        phase: PhaseId,
        /// How long this call allowed for the lock.
        waited: Duration,
        /// The holder's pid, when its record names a process that is live with
        /// the recorded start time.
        holder: Option<u32>,
    },
    /// A launch tried to write a state that does not record it as the supervisor.
    #[error(
        "phase {phase}: launch {ours} does not own the state, which records {}{}",
        recorded.as_deref().unwrap_or("no supervisor"),
        if *pending_only { "; this launch is only pending" } else { "" }
    )]
    NotOwner {
        /// The phase whose state was to be written.
        phase: PhaseId,
        /// The launch that tried to write.
        ours: String,
        /// The launch the state records as its supervisor.
        recorded: Option<String>,
        /// Whether `ours` is the state's pending supervisor, and so may still
        /// become the recorded one.
        pending_only: bool,
    },
}

/// A timed-out wait's description of who holds the lock.
fn write_lock_holder_phrase(holder: Option<u32>) -> String {
    match holder {
        Some(pid) => format!("pid {pid}"),
        None => "an unknown holder".to_string(),
    }
}

/// What [`update_state_with`] found on disk, read under the write lock.
// Passed by value to one closure and consumed there, so its size costs one move;
// a `Box<State>` would only make every caller unbox what it is about to edit.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Existing {
    /// No state file exists for the phase.
    Missing,
    /// The state file exists but does not read or parse.
    Unloadable(WorkflowError),
    /// The state as it is on disk now.
    Loaded(State),
}

/// Filename prefix shared by every per-phase state file. Owned here so
/// listing never hardcodes the naming scheme.
const STATE_FILE_PREFIX: &str = "state-";

/// Return the `.devflow` directory for a project.
pub fn devflow_dir(project_root: &Path) -> PathBuf {
    project_root.join(".devflow")
}

/// Locate the shallowest `.devflow` path component within `dir`, if any, and
/// return the path up to and including it.
///
/// Walking `dir`'s path *components* — rather than string-matching each of
/// `dir.ancestors()`'s paths — is what makes this resolve a **relative**
/// `dir` whose `.devflow` is the leaf or near-leaf component (e.g.
/// `.devflow/captures`, or the bare `.devflow`) correctly: the ancestor-tail
/// approach hits an empty final ancestor `""` on those inputs and mishandles
/// the leaf-is-`.devflow` case, whereas component-walking handles both
/// cleanly. Shallowest-first (the first match while walking root-to-leaf) is
/// the deterministic tie-break if `.devflow` appears twice in the path.
fn find_devflow_marker(dir: &Path) -> Option<PathBuf> {
    let mut acc = PathBuf::new();
    for component in dir.components() {
        acc.push(component.as_os_str());
        if component.as_os_str() == std::ffi::OsStr::new(".devflow") {
            return Some(acc);
        }
    }
    None
}

/// Create `dir` (and any missing parents), then self-protect any `.devflow`
/// directory found in its path by writing `<that-dir>/.gitignore` containing
/// `*` — so a downstream user's routine `git add . && git commit` never
/// sweeps DevFlow's runtime artifacts (agent stdout, gate context, state)
/// into their repository, independent of whether their own root
/// `.gitignore` mentions `.devflow` at all (closes 19a-WR-01,
/// 19-CONTEXT.md D-14).
///
/// This is deliberately a different function from the pure path accessor
/// [`devflow_dir`] above: `devflow_dir(project_root)` takes a **project
/// root** and returns `project_root/.devflow` with zero filesystem I/O — it
/// is invoked from read-only paths (`doctor`, `status`) and from tests that
/// assert on the returned path, so giving *it* side effects would be exactly
/// the class of behavioral change this phase exists to avoid. This function,
/// `ensure_devflow_dir(dir)`, instead takes the **directory to create**,
/// which may itself be `.devflow`, a subdirectory of it, or something with no
/// `.devflow` ancestor at all. Do not confuse the two.
///
/// Contract:
/// 1. Find the shallowest `.devflow` path component (see
///    [`find_devflow_marker`]).
/// 2. Create that component, then `dir`, with all missing parents, through
///    [`crate::durable::create_dir_all_durably`]. Each returns having fsynced the
///    parent of the directory it was given, even when it already existed, so no
///    writer returns success on an entry another process has not synced (R2-08).
///    That costs one directory fsync per call, two when `dir` is below `.devflow`.
/// 3. If found, write `<marker>/.gitignore` with the bytes `*\n`, using
///    `create_new(true)` so an existing file — whatever its content — is left
///    untouched; a lost race against a concurrent creator surfaces as
///    `AlreadyExists`, which this function maps to `Ok(())`. Any other I/O
///    error propagates via `?`.
/// 4. If no `.devflow` component exists, this function only creates `dir`
///    durably.
///
/// The marker takes no temp file and no file fsync (R-15): this runs on every
/// state write, and `.gitignore` is not a state or gate record.
///
/// Returns `std::io::Result<()>`, not a crate-specific error enum: this
/// plan's seven conversion sites each live in a different module with their
/// own error enum (`WorkflowError`, `GateError`, `MonitorError`,
/// `ResultError`, `ShipError`), and every one already carries an
/// `Io(#[from] std::io::Error)` variant, so `?` converts at every call site
/// with zero signature churn.
///
/// **Deleted-marker note:** if `.devflow/.gitignore` is deleted, or lost to a
/// crash, the next call recreates it, because `create_new(true)` succeeds on a
/// missing file. An existing file is never overwritten, whatever its content.
pub fn ensure_devflow_dir(dir: &Path) -> std::io::Result<()> {
    let marker = find_devflow_marker(dir);
    if let Some(marker_dir) = marker.as_deref()
        && marker_dir != dir
    {
        crate::durable::create_dir_all_durably(marker_dir)?;
    }
    crate::durable::create_dir_all_durably(dir)?;

    let Some(marker_dir) = marker else {
        return Ok(());
    };

    let gitignore = marker_dir.join(".gitignore");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&gitignore)
    {
        Ok(mut f) => {
            f.write_all(b"*\n")?;
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

/// Return the persisted state path for a phase of a project.
pub fn state_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    devflow_dir(project_root).join(format!(
        "{STATE_FILE_PREFIX}{padded}.json",
        padded = phase.padded()
    ))
}

/// Write `state` to `.devflow/state-{NN}.json`, keyed by `state.phase`, as a test
/// fixture: a whole snapshot with no write lock and no re-read, overwriting whatever
/// is on disk. Compiled only for tests and the `test-support` feature, never for
/// production. Production writes go through [`update_state`], [`update_state_with`]
/// or [`update_owned_state`].
#[cfg(any(test, feature = "test-support"))]
pub fn save_state(state: &State) -> Result<(), WorkflowError> {
    debug!("saving state: phase={} stage={}", state.phase, state.stage);
    let path = state_path(&state.project_root, state.phase);
    let contents = serde_json::to_string_pretty(state)?;
    write_state_atomic(&path, &contents)?;
    Ok(())
}

fn unique_temp_path_with(target: &Path, pid: u32, sequence: u64) -> PathBuf {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    target.with_file_name(format!(".{name}.{pid}.{sequence}.tmp"))
}

/// Create a unique sibling temporary file, retrying only a colliding orphan.
pub(crate) fn create_unique_temp(
    target: &Path,
    pid: u32,
    sequence: &AtomicU64,
) -> std::io::Result<(PathBuf, File)> {
    loop {
        let tmp = unique_temp_path_with(target, pid, sequence.fetch_add(1, Ordering::Relaxed));
        match File::create_new(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Write state through [`crate::durable::replace_durably`], so readers never
/// observe a truncated file and a power loss cannot lose the rename (D-09).
fn write_state_atomic(path: &Path, contents: &str) -> Result<(), WorkflowError> {
    if let Some(parent) = path.parent() {
        ensure_devflow_dir(parent)?;
    }
    crate::durable::replace_durably(path, contents.as_bytes())?;
    Ok(())
}

/// Load workflow state for a phase from `.devflow/state-{NN}.json`.
pub fn load_state(project_root: &Path, phase: PhaseId) -> Result<State, WorkflowError> {
    let path = state_path(project_root, phase);
    debug!("loading state from {}", path.display());
    // `try_exists`, not `exists`: a path that cannot be examined is an I/O failure. Read as
    // a missing state, it would end a healthy agent through the promotion watch.
    if !path.try_exists()? {
        return Err(WorkflowError::MissingState(path));
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// How often a bounded acquisition tries the write lock.
pub const WRITE_LOCK_POLL: Duration = Duration::from_millis(10);

/// The wait a repair verb gives the write lock by default: longer than a closure spends
/// unless something it waits on is stalled, and short enough that a repair verb always
/// answers. Stalled, a closure can outlast it: 10 s for a fenced gate answer, 13 s for a
/// recovery decision (`recover::a_recover_decisions_worst_case_is_counted_on_its_real_paths`)
/// and, by reading, 16 s for a reservation whose two
/// [`crate::supervisor::protocol::IO_TIMEOUT`]-bounded socket probes both stall on both
/// connects. The verb then names the holder and writes nothing.
pub const WRITE_LOCK_REPAIR_WAIT: Duration = Duration::from_secs(10);

/// The file the phase's state write lock lives on: `.devflow/.state-NN.lock`.
///
/// DevFlow never replaces or removes it, because a `flock` belongs to an inode
/// and `state-NN.json` is replaced on every write (D-08).
///
/// A holder records `"{pid} {start}\n"` in it for as long as it holds the lock.
/// The record only names a process in a message: nothing signals or waits on it,
/// and it does not decide whether the lock is held (R-03).
fn state_lock_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    devflow_dir(project_root).join(format!(
        ".{STATE_FILE_PREFIX}{padded}.lock",
        padded = phase.padded()
    ))
}

/// A held state write lock. Dropping it closes the file, which is the only way
/// the lock ends: an explicit release would drop it for every duplicate of the
/// descriptor, including one a forked child has not closed yet.
///
/// `file` is `None` only under the `DEVFLOW_TEST_SKIP_WRITE_LOCK` test seam.
struct WriteLock {
    file: Option<File>,
}

impl Drop for WriteLock {
    /// Clear the holder record while the lock is still held, so a non-empty record
    /// names the current holder. A holder that dies leaves its record behind, and
    /// a reader then finds the process gone or the lock free.
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = file.set_len(0);
        }
    }
}

/// Take the phase's state write lock: for as long as it is held when `wait` is
/// `None`, else until `wait` has passed, then [`WorkflowError::WriteLockTimeout`].
///
/// The descriptor is close-on-exec, as std opens every file, so a process spawned
/// while this is held does not keep the lock.
fn lock_for_write(
    project_root: &Path,
    phase: PhaseId,
    wait: Option<Duration>,
) -> Result<WriteLock, WorkflowError> {
    // Only for 50.1-16's negative control, that two launchers both reserve once
    // the lock is skipped (VALIDATION V-05). A release build has no such read.
    #[cfg(any(test, feature = "test-support"))]
    if std::env::var_os("DEVFLOW_TEST_SKIP_WRITE_LOCK").is_some() {
        return Ok(WriteLock { file: None });
    }
    ensure_devflow_dir(&devflow_dir(project_root))?;
    let path = state_lock_path(project_root, phase);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    match wait {
        None => block_on_write_lock(&file)?,
        Some(wait) => poll_for_write_lock(&file, &path, phase, wait)?,
    }
    record_write_lock_holder(&file);
    Ok(WriteLock { file: Some(file) })
}

fn block_on_write_lock(file: &File) -> Result<(), WorkflowError> {
    // `flock` returns EINTR when a signal handler without SA_RESTART runs.
    loop {
        match file.lock() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
}

/// Try the lock every [`WRITE_LOCK_POLL`] until `wait` has passed; a zero `wait`
/// makes one attempt.
fn poll_for_write_lock(
    file: &File,
    path: &Path,
    phase: PhaseId,
    wait: Duration,
) -> Result<(), WorkflowError> {
    let started = std::time::Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        let remaining = wait.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(WorkflowError::WriteLockTimeout {
                phase,
                waited: wait,
                holder: recorded_write_lock_holder(path),
            });
        }
        std::thread::sleep(WRITE_LOCK_POLL.min(remaining));
    }
}

/// Record this process as the lock's holder. Written at an explicit offset, so the
/// descriptor's own offset never matters (R-01). The record is a diagnostic, so a
/// failure to write it is a warning and the lock is still held.
fn record_write_lock_holder(file: &File) {
    let pid = std::process::id();
    let record = match crate::agent::process_start_time(pid) {
        Some(start) => format!("{pid} {start}\n"),
        None => format!("{pid}\n"),
    };
    if let Err(error) = file
        .set_len(0)
        .and_then(|()| file.write_all_at(record.as_bytes(), 0))
    {
        warn!("could not record the state write lock's holder: {error}");
    }
}

/// The pid in the lock file's record, when that process is live with the recorded
/// start time. An empty, torn, start-less or dead record is `None`, as is a holder
/// that has acquired the lock and not yet recorded itself.
fn recorded_write_lock_holder(lock_path: &Path) -> Option<u32> {
    let record = std::fs::read_to_string(lock_path).ok()?;
    let (pid, start) = record.strip_suffix('\n')?.split_once(' ')?;
    let (pid, start) = (pid.parse().ok()?, start.parse().ok()?);
    crate::agent::is_same_process(pid, start).then_some(pid)
}

/// [`update_state`] for a caller that must see what is on disk before deciding,
/// such as one that creates a missing state: check and write are one step under
/// the write lock. `apply` returns the state to write, or `None` to write nothing,
/// and a value for the caller.
///
/// `apply` runs with the lock held, under the same contract as [`update_state`].
/// This blocks while the lock is held, for the same callers as [`update_state`].
pub fn update_state_with<T, E: From<WorkflowError>>(
    project_root: &Path,
    phase: PhaseId,
    apply: impl FnOnce(Existing) -> Result<(Option<State>, T), E>,
) -> Result<T, E> {
    update_state_with_wait(project_root, phase, None, apply)
}

/// [`update_state_with`] for the verbs whose job is to diagnose a wedge: `stop`,
/// `recover`, `status` and `doctor`. A SIGSTOPped or D-state holder must not hang
/// them, so this tries the lock until `wait` has passed
/// ([`WRITE_LOCK_REPAIR_WAIT`] is their default) and then returns
/// [`WorkflowError::WriteLockTimeout`] naming the holder, with `apply` not run and
/// nothing written.
pub fn update_state_with_within<T, E: From<WorkflowError>>(
    project_root: &Path,
    phase: PhaseId,
    wait: Duration,
    apply: impl FnOnce(Existing) -> Result<(Option<State>, T), E>,
) -> Result<T, E> {
    update_state_with_wait(project_root, phase, Some(wait), apply)
}

/// What is on disk for `phase`, as a caller holding the write lock sees it.
fn read_existing(project_root: &Path, phase: PhaseId) -> Existing {
    match load_state(project_root, phase) {
        Ok(state) => Existing::Loaded(state),
        Err(WorkflowError::MissingState(_)) => Existing::Missing,
        Err(error) => Existing::Unloadable(error),
    }
}

fn update_state_with_wait<T, E: From<WorkflowError>>(
    project_root: &Path,
    phase: PhaseId,
    wait: Option<Duration>,
    apply: impl FnOnce(Existing) -> Result<(Option<State>, T), E>,
) -> Result<T, E> {
    let _lock = lock_for_write(project_root, phase, wait)?;
    let (next, value) = apply(read_existing(project_root, phase))?;
    if let Some(state) = next {
        if state.phase != phase {
            return Err(WorkflowError::PhaseMismatch {
                expected: phase,
                found: state.phase,
            }
            .into());
        }
        let contents = serde_json::to_string_pretty(&state).map_err(WorkflowError::from)?;
        write_state_atomic(&state_path(project_root, phase), &contents)?;
    }
    Ok(value)
}

/// What [`write_lock_status`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteLockStatus {
    /// Nothing holds the lock, or its file does not exist yet.
    Free,
    /// Something holds the lock.
    Held {
        /// The holder's pid, when its record names a process that is live with the
        /// recorded start time.
        pid: Option<u32>,
    },
}

/// Read the state of the phase's write lock without waiting, for `status` and
/// `doctor`. It creates no file. It asks for a shared lock, so two concurrent reads
/// never report each other as the holder.
pub fn write_lock_status(
    project_root: &Path,
    phase: PhaseId,
) -> Result<WriteLockStatus, WorkflowError> {
    let path = state_lock_path(project_root, phase);
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WriteLockStatus::Free);
        }
        Err(error) => return Err(error.into()),
    };
    match file.try_lock_shared() {
        Ok(()) => Ok(WriteLockStatus::Free),
        Err(TryLockError::WouldBlock) => Ok(WriteLockStatus::Held {
            pid: recorded_write_lock_holder(&path),
        }),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

/// Change the state on disk: take the phase's write lock, re-read the state, run
/// `apply` on it, write the result durably, and release the lock (D-08). Returns
/// the committed state and `apply`'s value. A missing state is
/// [`WorkflowError::MissingState`], and an unreadable one is its load error;
/// neither runs `apply`. An error from `apply` writes nothing.
///
/// `apply` sets only the fields its caller changes. Assigning a whole copy loaded
/// earlier brings back the lost update this function exists to remove.
///
/// `apply` runs with the lock held, so it must be short: it must not spawn a
/// process, wait on a gate or an agent, or take another lock. That includes this
/// lock: calling this or [`clear_state`] for the same phase from inside `apply`
/// deadlocks.
///
/// This blocks while the lock is held, and is for workflow writers: supervisors,
/// launchers, gate writers and outcome handlers. A closure is short by contract, a
/// holder that dies releases the lock, and a SIGSTOPped holder delays them only
/// until it is continued or killed, where a timeout would turn a stalled peer into
/// a failed stage transition. A verb that diagnoses a wedge uses
/// [`update_state_with_within`], [`clear_state_within`] or [`write_lock_status`].
pub fn update_state<T, E: From<WorkflowError>>(
    project_root: &Path,
    phase: PhaseId,
    apply: impl FnOnce(&mut State) -> Result<T, E>,
) -> Result<(State, T), E> {
    update_state_with(project_root, phase, |existing| {
        let mut state = match existing {
            Existing::Loaded(state) => state,
            Existing::Missing => {
                return Err(WorkflowError::MissingState(state_path(project_root, phase)).into());
            }
            Existing::Unloadable(error) => return Err(error.into()),
        };
        let value = apply(&mut state)?;
        Ok((Some(state.clone()), (state, value)))
    })
}

/// Whether the phase's state write lock is free, asked through a separate open.
/// A lock file that does not exist yet is free. For tests only: a fork in another
/// thread can keep a just-released lock busy until its exec.
#[cfg(any(test, feature = "test-support"))]
pub fn write_lock_is_free(project_root: &Path, phase: PhaseId) -> bool {
    let path = state_lock_path(project_root, phase);
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
        Err(error) => panic!("open {}: {error}", path.display()),
    };
    match file.try_lock() {
        Ok(()) => true,
        Err(TryLockError::WouldBlock) => false,
        Err(TryLockError::Error(error)) => panic!("try_lock {}: {error}", path.display()),
    }
}

/// How a state stands to one launch id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ownership {
    /// The state records this launch as its supervisor.
    Recorded,
    /// This launch is the pending supervisor, not yet the recorded one.
    PendingOnly,
    /// The state names this launch nowhere.
    Superseded {
        /// The launch the state records instead, if any.
        recorded: Option<String>,
    },
}

/// How `state` stands to `launch_id`. A launch that is only pending may yet become
/// the recorded one, so a caller can wait for its publish (D-15); a superseded one
/// never will.
pub fn ownership(state: &State, launch_id: &str) -> Ownership {
    let recorded = state.supervisor.as_ref().map(|record| &record.launch_id);
    if recorded.is_some_and(|recorded| recorded == launch_id) {
        return Ownership::Recorded;
    }
    let pending = state.pending_supervisor.as_ref();
    if pending.is_some_and(|pending| pending.launch_id == launch_id) {
        return Ownership::PendingOnly;
    }
    Ownership::Superseded {
        recorded: recorded.cloned(),
    }
}

/// The refusal for a launch `state` does not record, or `None` when it records it.
fn not_owner(state: &State, phase: PhaseId, launch_id: &str) -> Option<WorkflowError> {
    let (recorded, pending_only) = match ownership(state, launch_id) {
        Ownership::Recorded => return None,
        Ownership::PendingOnly => (state.supervisor.as_ref().map(|r| r.launch_id.clone()), true),
        Ownership::Superseded { recorded } => (recorded, false),
    };
    Some(WorkflowError::NotOwner {
        phase,
        ours: launch_id.to_string(),
        recorded,
        pending_only,
    })
}

/// [`update_state`] fenced to the launch the state records (D-03): unless the
/// state records `launch_id` as its supervisor, this returns
/// [`WorkflowError::NotOwner`] with `apply` not run and nothing written. The check
/// is made under the write lock, on the state as it is on disk now.
pub fn update_owned_state<T, E: From<WorkflowError>>(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
    apply: impl FnOnce(&mut State) -> Result<T, E>,
) -> Result<(State, T), E> {
    update_state(project_root, phase, |state| {
        match not_owner(state, phase, launch_id) {
            None => apply(state),
            Some(refusal) => Err(refusal.into()),
        }
    })
}

/// Run `f` while the state records `launch_id`, sharing one hold of the write lock
/// between the check and `f` (D-03, R-09). A launch the state does not record gets
/// [`WorkflowError::NotOwner`] with `f` not called. It writes no state.
///
/// There is no window between the check and `f`: a supersession is itself a state
/// write under this lock, so it lands wholly before the check or wholly after `f`.
///
/// This takes the lock in its blocking form, as every workflow writer does. `f` must
/// be a bounded file operation. It never waits on a human or an agent (D-08). The only
/// lock it may take is the gate lock, a bounded wait the operator's D-08 ruling of
/// 2026-10-26 allows inside the write lock: gate-lock holders run only file operations
/// and never take the state write lock, so the order is always write lock, then gate
/// lock, and each wait is capped by `gates::GATE_LOCK_WAIT`, below
/// [`WRITE_LOCK_REPAIR_WAIT`]. `gates::tests::gates_never_takes_the_state_write_lock`
/// and `gates::tests::the_gate_lock_wait_stays_below_the_write_lock_repair_wait` pin
/// both rules. A fenced `Gates::answer` makes two waits, 10 s in all, which reaches
/// [`WRITE_LOCK_REPAIR_WAIT`] if a holder stalls both: a repair verb then gives up and
/// names this lock's holder, and nothing hangs
/// (`gates::tests::a_fenced_answers_two_gate_waits_reach_the_write_lock_repair_wait`).
///
/// Not reentrant: this must not be called inside an [`update_state`],
/// [`update_state_with`], [`update_owned_state`] or [`clear_state_if_owned`] closure
/// or another `while_owner` closure, and `f` must not call any of them. A second
/// `flock` on the write-lock file from one process is a second open file description,
/// so the blocking form waits forever.
///
/// # Errors
/// [`WorkflowError::NotOwner`], the state's load error, an error taking the lock, or
/// whatever `f` returns.
pub fn while_owner<T, E: From<WorkflowError>>(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
    f: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let _lock = lock_for_write(project_root, phase, None)?;
    let state = load_state(project_root, phase)?;
    if let Some(refusal) = not_owner(&state, phase, launch_id) {
        return Err(refusal.into());
    }
    f()
}

/// What [`clear_state_if`] did.
#[derive(Debug)]
pub(crate) struct Cleared {
    /// Whether a file was removed, by the removal or before its error.
    pub(crate) removed: bool,
    /// The removal's outcome; `Ok` when the decision kept the state.
    pub(crate) result: Result<(), WorkflowError>,
}

/// Enumerate every active phase state, sorted by phase number.
///
/// Unparsable state files are skipped with a warning rather than failing the
/// whole listing — `status`/`recover` must degrade, not die, on one corrupt
/// file.
pub fn list_states(project_root: &Path) -> Vec<State> {
    let mut states = Vec::new();
    let Ok(entries) = std::fs::read_dir(devflow_dir(project_root)) else {
        return states;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(STATE_FILE_PREFIX) || !name.ends_with(".json") {
            continue;
        }
        match std::fs::read_to_string(entry.path())
            .map_err(WorkflowError::from)
            .and_then(|c| Ok(serde_json::from_str::<State>(&c)?))
        {
            Ok(state) => states.push(state),
            Err(err) => warn!("skipping unreadable state file {name}: {err}"),
        }
    }
    states.sort_by_key(|s| s.phase);
    states
}

/// Every phase with a per-phase state file on disk, parsable or not — the
/// complement [`list_states`] cannot give, since it skips a file it cannot
/// parse. Named by file, so no content is read.
pub fn state_file_phases(project_root: &Path) -> Vec<PhaseId> {
    let mut phases = Vec::new();
    let Ok(entries) = std::fs::read_dir(devflow_dir(project_root)) else {
        return phases;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(phase) = name
            .to_str()
            .and_then(|name| name.strip_prefix(STATE_FILE_PREFIX))
            .and_then(|name| name.strip_suffix(".json"))
            .and_then(|phase| phase.parse::<PhaseId>().ok())
        else {
            continue;
        };
        phases.push(phase);
    }
    phases.sort();
    phases
}

/// Remove a phase's persisted state if present, with its orphaned write temps.
/// Returns whether this call removed any file, so a caller can report only what
/// it cleaned.
///
/// Takes the write lock, so a removal never interleaves with an update. This blocks
/// while the lock is held, for the same callers as [`update_state`].
pub fn clear_state(project_root: &Path, phase: PhaseId) -> Result<bool, WorkflowError> {
    let (removed, result) =
        clear_state_with_partial_result_impl(project_root, phase, None, false, None);
    result.map(|()| removed)
}

/// [`clear_state`] for a supervisor (D-03): unless the state records `launch_id`, this
/// returns [`WorkflowError::NotOwner`] and removes nothing. The check and the removal
/// share one hold of the write lock. Returns whether a file was removed.
///
/// Not reentrant, under the rule [`while_owner`] states: this must not be called inside
/// an [`update_state`], [`update_state_with`], [`update_owned_state`] or `while_owner`
/// closure, because a second `flock` on the write-lock file from one process blocks
/// forever. The D-08 ruling of 2026-10-26 on the gate lock does not reach here: nothing
/// in this removal takes another lock.
///
/// # Errors
/// [`WorkflowError::NotOwner`], the state's load error, an error taking the lock, or a
/// failed removal.
pub fn clear_state_if_owned(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
) -> Result<bool, WorkflowError> {
    let (removed, result) =
        clear_state_with_partial_result_impl(project_root, phase, None, false, Some(launch_id));
    result.map(|()| removed)
}

/// [`clear_state`] for the verbs whose job is to diagnose a wedge, as
/// [`update_state_with_within`]: on [`WorkflowError::WriteLockTimeout`] nothing was
/// removed.
pub fn clear_state_within(
    project_root: &Path,
    phase: PhaseId,
    wait: Duration,
) -> Result<bool, WorkflowError> {
    let (removed, result) =
        clear_state_with_partial_result_impl(project_root, phase, Some(wait), false, None);
    result.map(|()| removed)
}

/// Decide on a fresh read of the state and remove it, in one hold of the write lock
/// (R-07), so a `start` or `resume` whose reservation needs that lock lands wholly
/// before the read or wholly after the removal. The general form of
/// [`clear_state_if_owned`], which stays as it is.
///
/// `decide` gets what is on disk: `Ok(false)` keeps it, `Ok(true)` removes the state and
/// its orphaned write temps, and `Err` removes nothing and is returned as it is. With
/// `wait` set, a lock still held then is [`WorkflowError::WriteLockTimeout`] with `decide`
/// not run.
///
/// `decide` may not take this lock: a second `flock` on the file from this process is
/// a second open file description and blocks on the first. So it never calls
/// [`update_state`], [`update_state_with`], [`update_owned_state`], [`clear_state`],
/// [`while_owner`] or [`clear_state_if_owned`].
///
/// A gate operation in `decide` waits for the gate lock inside this one, the bounded wait
/// the operator's D-08 ruling of 2026-10-26 allows: write lock, then gate lock, never the
/// reverse, and `gates::GATE_LOCK_WAIT` is below [`WRITE_LOCK_REPAIR_WAIT`].
/// `recover::a_recover_decisions_worst_case_is_counted_on_its_real_paths` counts what one
/// recovery decision can wait for: two socket probes and one gate wait, 13 s when both
/// sockets die mid-ping and the gate lock is wedged, which outlasts the 10 s of
/// [`WRITE_LOCK_REPAIR_WAIT`]; a repair verb then names the holder and writes nothing.
/// That leaves fsync-bound file operations on a slow disk unmeasured, and a SIGSTOPped
/// caller outside the bound, as for every writer.
pub(crate) fn clear_state_if<E: From<WorkflowError>>(
    project_root: &Path,
    phase: PhaseId,
    wait: Option<Duration>,
    decide: impl FnOnce(Existing) -> Result<bool, E>,
) -> Result<Cleared, E> {
    let _lock = lock_for_write(project_root, phase, wait)?;
    if !decide(read_existing(project_root, phase))? {
        return Ok(Cleared {
            removed: false,
            result: Ok(()),
        });
    }
    let (removed, result) = remove_state_files(project_root, phase, true);
    Ok(Cleared { removed, result })
}

fn clear_state_with_partial_result_impl(
    project_root: &Path,
    phase: PhaseId,
    wait: Option<Duration>,
    report_temp_removal_failure: bool,
    owner: Option<&str>,
) -> (bool, Result<(), WorkflowError>) {
    // Nothing to remove, so no lock file to leave behind in a project DevFlow has
    // not touched.
    if !devflow_dir(project_root).exists() {
        return (false, Ok(()));
    }
    let _lock = match lock_for_write(project_root, phase, wait) {
        Ok(lock) => lock,
        Err(error) => return (false, Err(error)),
    };
    if let Some(launch_id) = owner {
        let refusal = load_state(project_root, phase)
            .map(|state| not_owner(&state, phase, launch_id))
            .unwrap_or_else(Some);
        if let Some(refusal) = refusal {
            return (false, Err(refusal));
        }
    }
    remove_state_files(project_root, phase, report_temp_removal_failure)
}

/// Remove the state file and its orphaned write temps. The caller holds the write lock.
fn remove_state_files(
    project_root: &Path,
    phase: PhaseId,
    report_temp_removal_failure: bool,
) -> (bool, Result<(), WorkflowError>) {
    let path = state_path(project_root, phase);
    let mut removed = false;
    if path.exists() {
        debug!("clearing state at {}", path.display());
        match crate::durable::remove_durably(&path) {
            Ok(was_removed) => removed |= was_removed,
            Err(error) => return (removed, Err(error.into())),
        }
    }
    let temp_prefix = format!(
        ".{}.",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    if let Ok(entries) = std::fs::read_dir(devflow_dir(project_root)) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with(&temp_prefix) || !name.ends_with(".tmp") {
                continue;
            }
            let temp = entry.path();
            match crate::durable::remove_durably(&temp) {
                Ok(was_removed) => removed |= was_removed,
                Err(error) if report_temp_removal_failure => {
                    return (
                        removed,
                        Err(WorkflowError::StateTempRemoval {
                            path: temp,
                            source: error,
                        }),
                    );
                }
                Err(error) => warn!(
                    "could not remove orphaned state temp {}: {error}",
                    temp.display()
                ),
            }
        }
    }
    (removed, Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_devflow_dir_writes_star_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");
        ensure_devflow_dir(&target).expect("ensure_devflow_dir");
        assert!(target.is_dir());
        let contents = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(contents.trim(), "*");
    }

    #[test]
    fn ensure_devflow_dir_is_idempotent_and_preserves_existing_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");
        ensure_devflow_dir(&target).expect("first call");
        let first = std::fs::read(target.join(".gitignore")).unwrap();

        ensure_devflow_dir(&target).expect("second call");
        let second = std::fs::read(target.join(".gitignore")).unwrap();
        assert_eq!(
            first, second,
            "second call must leave the file byte-identical"
        );
    }

    #[test]
    fn ensure_devflow_dir_preserves_foreign_gitignore_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join(".gitignore"), "# owned by something else\n").unwrap();

        ensure_devflow_dir(&target).expect("must not fail on a foreign .gitignore");

        let contents = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(contents, "# owned by something else\n");
    }

    #[test]
    fn ensure_devflow_dir_on_nested_subpath_marks_the_devflow_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow").join("history").join("phase-01");
        ensure_devflow_dir(&target).expect("nested ensure_devflow_dir");

        assert!(target.is_dir());
        let marker = dir.path().join(".devflow").join(".gitignore");
        assert!(
            marker.is_file(),
            "gitignore must land at the .devflow ancestor"
        );
        assert!(
            !target.join(".gitignore").exists(),
            "gitignore must not land at the leaf directory"
        );
    }

    /// Antigravity review edge case: a relative path whose `.devflow` is the
    /// leaf or near-leaf component. Exercises the component-detection logic
    /// directly rather than a real filesystem call, so the assertion does not
    /// depend on the test process's (global, shared-across-threads) cwd.
    #[test]
    fn ensure_devflow_dir_on_relative_devflow_leaf_path_marks_it() {
        assert_eq!(
            find_devflow_marker(Path::new(".devflow/captures")),
            Some(PathBuf::from(".devflow")),
            "leaf-adjacent relative path must mark .devflow, not captures/"
        );
        assert_eq!(
            find_devflow_marker(Path::new(".devflow")),
            Some(PathBuf::from(".devflow")),
            "bare relative .devflow path must mark itself"
        );
    }

    #[test]
    fn ensure_devflow_dir_without_a_devflow_ancestor_only_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("plain").join("sub");
        ensure_devflow_dir(&target).expect("ensure_devflow_dir");

        assert!(target.is_dir());
        assert!(!dir.path().join(".gitignore").exists());
        assert!(!dir.path().join("plain").join(".gitignore").exists());
        assert!(!target.join(".gitignore").exists());
    }

    #[test]
    fn ensure_devflow_dir_concurrent_calls_both_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");

        let t1 = {
            let target = target.clone();
            std::thread::spawn(move || ensure_devflow_dir(&target))
        };
        let t2 = {
            let target = target.clone();
            std::thread::spawn(move || ensure_devflow_dir(&target))
        };
        assert!(
            t1.join().unwrap().is_ok(),
            "first concurrent call must succeed"
        );
        assert!(
            t2.join().unwrap().is_ok(),
            "second concurrent call must succeed"
        );

        let contents = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(contents.trim(), "*");
    }

    use crate::mode::Mode;
    use crate::stage::Stage;
    use crate::state::AgentKind;

    fn state_in(root: &Path, phase: PhaseId, stage: Stage) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = stage;
        state
    }

    /// Wait out a fork in another test thread: between its fork and its exec a
    /// child holds a duplicate of every descriptor, so `cargo test`'s threads can
    /// keep a just-released `flock` busy for a moment. A real leak outlasts this.
    fn wait_for_free_write_lock(root: &Path, phase: PhaseId) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !write_lock_is_free(root, phase) {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        true
    }

    /// One of `update_state`'s callers, changing one field.
    fn update_field(root: &Path, phase: PhaseId, set: impl FnOnce(&mut State)) {
        update_state(root, phase, |state| {
            set(state);
            Ok::<(), WorkflowError>(())
        })
        .expect("update_state");
    }

    #[test]
    fn update_state_rereads_so_a_write_made_between_loads_survives() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(50);
        save_state(&state_in(dir.path(), phase, Stage::Code)).unwrap();

        update_field(dir.path(), phase, |state| state.stopped = true);
        update_field(dir.path(), phase, |state| state.gate_pending = true);

        let both = load_state(dir.path(), phase).unwrap();
        assert!(both.stopped, "the other writer's change must survive");
        assert!(both.gate_pending, "this writer's change must land");

        // The lost update this API removes: a copy loaded before the other
        // writer's change, saved whole after it.
        save_state(&state_in(dir.path(), phase, Stage::Code)).unwrap();
        let mut stale_copy = load_state(dir.path(), phase).unwrap();
        update_field(dir.path(), phase, |state| state.stopped = true);
        stale_copy.gate_pending = true;
        save_state(&stale_copy).unwrap();
        assert!(
            !load_state(dir.path(), phase).unwrap().stopped,
            "control: saving the older copy must lose the other writer's change"
        );
    }

    #[test]
    fn a_second_writer_waits_for_the_first_closure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(50);
        save_state(&state_in(root, phase, Stage::Code)).unwrap();
        let log = std::sync::Mutex::new(Vec::new());
        let (entered, first_is_inside) = std::sync::mpsc::channel();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                update_field(root, phase, |_| {
                    log.lock().unwrap().push("first start");
                    entered.send(()).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    log.lock().unwrap().push("first end");
                });
            });
            first_is_inside.recv().unwrap();
            update_field(root, phase, |_| {
                log.lock().unwrap().push("second start");
            });
        });

        assert_eq!(
            *log.lock().unwrap(),
            ["first start", "first end", "second start"]
        );
    }

    #[test]
    fn the_write_lock_is_held_only_inside_the_closure() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(50);
        save_state(&state_in(dir.path(), phase, Stage::Code)).unwrap();

        update_field(dir.path(), phase, |_| {
            assert!(
                !write_lock_is_free(dir.path(), phase),
                "the lock must be held inside the closure"
            );
        });

        assert!(
            wait_for_free_write_lock(dir.path(), phase),
            "the lock must be released once update_state returns"
        );
    }

    #[test]
    fn update_state_on_a_missing_state_is_missing_state() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(50);

        let error = update_state(dir.path(), phase, |_| Ok::<(), WorkflowError>(())).unwrap_err();

        assert!(
            matches!(&error, WorkflowError::MissingState(path) if *path == state_path(dir.path(), phase)),
            "got {error}"
        );
        assert!(!state_path(dir.path(), phase).exists());
    }

    #[test]
    fn clear_state_in_a_project_without_a_devflow_dir_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!clear_state(dir.path(), PhaseId::new(50)).expect("clear"));

        assert!(!devflow_dir(dir.path()).exists());
    }

    #[test]
    fn clear_state_waits_for_a_write_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(50);
        save_state(&state_in(root, phase, Stage::Code)).unwrap();
        let log = std::sync::Mutex::new(Vec::new());
        let (entered, writer_is_inside) = std::sync::mpsc::channel();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                update_field(root, phase, |_| {
                    entered.send(()).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    log.lock().unwrap().push("write end");
                });
            });
            writer_is_inside.recv().unwrap();
            assert!(clear_state(root, phase).expect("clear"));
            log.lock().unwrap().push("clear end");
        });

        assert_eq!(*log.lock().unwrap(), ["write end", "clear end"]);
        assert!(
            !state_path(root, phase).exists(),
            "the writer finished first, so the clear has the last word"
        );
    }

    /// A child killed and reaped when the test ends, however it ends.
    struct ReapedChild(std::process::Child);

    impl Drop for ReapedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn send_signal(pid: u32, signal: libc::c_int) {
        let signed = libc::pid_t::try_from(pid).expect("pid fits pid_t");
        // SAFETY: `kill` takes no pointers; `signed` is this test's own child.
        assert_eq!(unsafe { libc::kill(signed, signal) }, 0, "kill {pid}");
    }

    /// Take the write lock, spawn `sleep 5` while holding it, and drop the handle.
    fn spawn_sleep_holding_the_write_lock(
        root: &Path,
        phase: PhaseId,
        before_spawn: impl FnOnce(&File),
    ) -> ReapedChild {
        let lock = lock_for_write(root, phase, None).expect("take the write lock");
        before_spawn(lock.file.as_ref().expect("a real lock has a file"));
        let child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep");
        drop(lock);
        ReapedChild(child)
    }

    #[test]
    fn the_write_lock_is_not_inherited_by_a_spawned_child() {
        use std::os::fd::AsRawFd;
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(50));

        let plain = spawn_sleep_holding_the_write_lock(root, phase, |_| {});
        assert!(
            wait_for_free_write_lock(root, phase),
            "a close-on-exec lock must not outlive its handle in a child"
        );
        assert!(crate::test_support::process_is_live(plain.0.id()));

        // Control: the same child with close-on-exec cleared does keep the lock.
        let leaky = spawn_sleep_holding_the_write_lock(root, phase, |file| {
            // SAFETY: `file` is an open descriptor for the length of this call, and
            // F_SETFD with 0 only clears its close-on-exec flag.
            assert_eq!(
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, 0) },
                0
            );
        });
        assert!(
            !write_lock_is_free(root, phase),
            "control: the child with the descriptor keeps the lock"
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            !write_lock_is_free(root, phase),
            "control: it keeps it for as long as it runs"
        );
        assert!(crate::test_support::process_is_live(leaky.0.id()));
        drop(leaky);
        assert!(
            wait_for_free_write_lock(root, phase),
            "control: the lock ends with the child that kept it"
        );
    }

    #[test]
    fn update_state_with_creates_a_missing_state_only_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(50));

        let saw_missing = update_state_with(root, phase, |existing| {
            Ok::<_, WorkflowError>((None, matches!(existing, Existing::Missing)))
        })
        .unwrap();
        assert!(saw_missing);
        assert!(
            !state_path(root, phase).exists(),
            "`None` must create nothing"
        );

        update_state_with(root, phase, |existing| {
            assert!(matches!(existing, Existing::Missing));
            Ok::<_, WorkflowError>((Some(state_in(root, phase, Stage::Validate)), ()))
        })
        .unwrap();
        assert_eq!(load_state(root, phase).unwrap().stage, Stage::Validate);

        let seen_stage = update_state_with(root, phase, |existing| {
            let Existing::Loaded(state) = existing else {
                panic!("the saved state must load, got {existing:?}");
            };
            Ok::<_, WorkflowError>((None, state.stage))
        })
        .unwrap();
        assert_eq!(seen_stage, Stage::Validate);
    }

    #[test]
    fn update_state_with_reports_an_unloadable_state() {
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(50));
        let path = state_path(root, phase);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();

        let unloadable = update_state_with(root, phase, |existing| {
            Ok::<_, WorkflowError>((None, matches!(existing, Existing::Unloadable(_))))
        })
        .unwrap();

        assert!(unloadable);
        assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
    }

    #[test]
    fn update_state_with_refuses_a_state_for_another_phase() {
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(50));

        let error = update_state_with(root, phase, |_| {
            Ok::<_, WorkflowError>((Some(state_in(root, PhaseId::new(51), Stage::Code)), ()))
        })
        .unwrap_err();

        assert!(
            matches!(
                error,
                WorkflowError::PhaseMismatch { expected, found }
                    if expected == phase && found == PhaseId::new(51)
            ),
            "got {error}"
        );
        assert!(!state_path(root, phase).exists());
        assert!(!state_path(root, PhaseId::new(51)).exists());
    }

    #[test]
    fn the_skip_seam_takes_no_lock() {
        const NAME: &str = "workflow::tests::the_skip_seam_takes_no_lock";
        if crate::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let (root, phase) = (dir.path(), PhaseId::new(50));
            save_state(&state_in(root, phase, Stage::Code)).unwrap();
            update_field(root, phase, |_| {
                assert!(
                    write_lock_is_free(root, phase),
                    "the seam must take no lock"
                );
                assert!(
                    !state_lock_path(root, phase).exists(),
                    "the seam must not touch the lock file"
                );
            });
            return;
        }

        let empty_path = tempfile::tempdir().unwrap();
        let output = crate::test_support::run_test_in_child(
            NAME,
            empty_path.path(),
            &[("DEVFLOW_TEST_SKIP_WRITE_LOCK", std::ffi::OsStr::new("1"))],
        );
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
    }

    #[test]
    fn a_bounded_write_names_a_stopped_holder_and_returns_by_its_deadline() {
        const NAME: &str =
            "workflow::tests::a_bounded_write_names_a_stopped_holder_and_returns_by_its_deadline";
        const ROOT_VAR: &str = "WRITE_LOCK_TEST_ROOT";
        const BOUND: std::time::Duration = std::time::Duration::from_millis(500);
        const SLACK: std::time::Duration = std::time::Duration::from_secs(2);
        const READY: &str = "holder.ready";
        let phase = PhaseId::new(50);

        if crate::test_support::in_child_test(NAME) {
            let root = PathBuf::from(std::env::var_os(ROOT_VAR).expect(ROOT_VAR));
            update_field(&root, phase, |_| {
                std::fs::write(root.join(READY), "").unwrap();
                std::thread::sleep(std::time::Duration::from_secs(60));
            });
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = state_path(root, phase);
        save_state(&state_in(root, phase, Stage::Code)).unwrap();
        let before = std::fs::read(&path).unwrap();
        let empty_path = tempfile::tempdir().unwrap();
        let mut holder = ReapedChild(crate::test_support::spawn_test_in_child(
            NAME,
            empty_path.path(),
            &[(ROOT_VAR, root.as_os_str())],
        ));
        let holder_pid = holder.0.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join(READY).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the holder never reached its closure"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        send_signal(holder_pid, libc::SIGSTOP);

        let ran = std::cell::Cell::new(false);
        let bounded_write = || {
            update_state_with_within(root, phase, BOUND, |_| {
                ran.set(true);
                Ok::<_, WorkflowError>((None, ()))
            })
        };
        let started = std::time::Instant::now();
        let error = bounded_write().unwrap_err();
        let elapsed = started.elapsed();
        eprintln!("the bounded write returned after {elapsed:?}, deadline {BOUND:?}");
        assert!(
            matches!(
                error,
                WorkflowError::WriteLockTimeout { holder: Some(pid), .. } if pid == holder_pid
            ),
            "got {error}"
        );
        assert!(elapsed >= BOUND, "returned early, after {elapsed:?}");
        assert!(elapsed < BOUND + SLACK, "returned late, after {elapsed:?}");
        assert!(!ran.get(), "a timed-out write must not run its closure");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(
            error.to_string().contains(&format!("pid {holder_pid}")),
            "got {error}"
        );

        let error = clear_state_within(root, phase, BOUND).unwrap_err();
        assert!(
            matches!(
                error,
                WorkflowError::WriteLockTimeout { holder: Some(pid), .. } if pid == holder_pid
            ),
            "got {error}"
        );
        assert!(path.exists(), "a timed-out clear must remove nothing");
        assert_eq!(
            write_lock_status(root, phase).unwrap(),
            WriteLockStatus::Held {
                pid: Some(holder_pid)
            }
        );

        send_signal(holder_pid, libc::SIGKILL);
        holder.0.wait().expect("reap the holder");

        // A record is never reported for a free lock.
        let lock_path = state_lock_path(root, phase);
        assert!(
            std::fs::read_to_string(&lock_path)
                .unwrap()
                .starts_with(&format!("{holder_pid} ")),
            "the killed holder's record must still be in the file"
        );
        assert_eq!(
            write_lock_status(root, phase).unwrap(),
            WriteLockStatus::Free
        );

        // A record naming a dead process is never reported for a held lock.
        let unrecorded_holder = File::options()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        unrecorded_holder.lock().unwrap();
        let error = bounded_write().unwrap_err();
        assert!(
            matches!(error, WorkflowError::WriteLockTimeout { holder: None, .. }),
            "got {error}"
        );
        assert!(error.to_string().contains("unknown holder"), "got {error}");
        assert!(!ran.get());
        drop(unrecorded_holder);

        // The earlier timeouts were the holders'; the lock ended with them.
        bounded_write().expect("the same call succeeds once the lock is free");
        assert!(ran.get());
    }

    #[test]
    fn write_lock_status_reads_free_held_and_the_recorded_holder() {
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(50));

        assert_eq!(
            write_lock_status(root, phase).unwrap(),
            WriteLockStatus::Free
        );
        assert!(!devflow_dir(root).exists(), "the read must create nothing");

        save_state(&state_in(root, phase, Stage::Code)).unwrap();
        update_field(root, phase, |_| {
            assert_eq!(
                write_lock_status(root, phase).unwrap(),
                WriteLockStatus::Held {
                    pid: Some(std::process::id())
                }
            );
        });
        assert!(wait_for_free_write_lock(root, phase));
        assert_eq!(
            write_lock_status(root, phase).unwrap(),
            WriteLockStatus::Free
        );
        let lock_path = state_lock_path(root, phase);
        assert_eq!(
            std::fs::metadata(&lock_path).unwrap().len(),
            0,
            "the record must be cleared before release"
        );

        let unrecorded_holder = File::options()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        unrecorded_holder.lock().unwrap();
        assert_eq!(
            write_lock_status(root, phase).unwrap(),
            WriteLockStatus::Held { pid: None }
        );
    }

    fn supervisor_record(launch_id: &str) -> crate::state::SupervisorRecord {
        crate::state::SupervisorRecord::new(
            PathBuf::from(format!("/c/sock/{launch_id}.sock")),
            launch_id.to_string(),
            4242,
            123_456,
            "boot-id".to_string(),
        )
    }

    fn pending_supervisor(launch_id: &str) -> crate::state::PendingSupervisor {
        crate::state::PendingSupervisor::new(
            launch_id.to_string(),
            PathBuf::from(format!("/c/sock/{launch_id}.sock")),
            PathBuf::from(format!("/c/supervisor-{launch_id}.live")),
        )
    }

    #[test]
    fn ownership_tells_recorded_pending_and_superseded_apart() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_in(dir.path(), PhaseId::new(50), Stage::Code);
        assert_eq!(
            ownership(&state, "X"),
            Ownership::Superseded { recorded: None }
        );

        state.supervisor = Some(supervisor_record("X"));
        assert_eq!(ownership(&state, "X"), Ownership::Recorded);
        assert_eq!(
            ownership(&state, "Y"),
            Ownership::Superseded {
                recorded: Some("X".to_string())
            }
        );

        state.pending_supervisor = Some(pending_supervisor("Y"));
        assert_eq!(ownership(&state, "Y"), Ownership::PendingOnly);
        assert_eq!(ownership(&state, "X"), Ownership::Recorded);
        assert_eq!(
            ownership(&state, "Z"),
            Ownership::Superseded {
                recorded: Some("X".to_string())
            },
            "a third launch is named nowhere"
        );
    }

    #[test]
    fn update_owned_state_refuses_a_launch_the_state_does_not_record() {
        let dir = tempfile::tempdir().unwrap();
        let (root, phase) = (dir.path(), PhaseId::new(50));
        let path = state_path(root, phase);
        let mut state = state_in(root, phase, Stage::Code);
        state.supervisor = Some(supervisor_record("X"));
        save_state(&state).unwrap();
        let before = std::fs::read(&path).unwrap();
        let ran = std::cell::Cell::new(false);
        let write_as = |launch_id: &str| {
            update_owned_state(root, phase, launch_id, |state| {
                ran.set(true);
                state.stopped = true;
                Ok::<(), WorkflowError>(())
            })
        };

        let error = write_as("Y").unwrap_err();
        assert!(
            matches!(
                &error,
                WorkflowError::NotOwner { phase: refused, ours, recorded, pending_only: false }
                    if *refused == phase && ours == "Y" && recorded.as_deref() == Some("X")
            ),
            "got {error}"
        );
        let message = error.to_string();
        assert!(message.contains('X') && message.contains('Y'), "{message}");
        assert!(!ran.get(), "a superseded launch must not reach its closure");
        assert_eq!(std::fs::read(&path).unwrap(), before);

        state.pending_supervisor = Some(pending_supervisor("Y"));
        save_state(&state).unwrap();
        let before = std::fs::read(&path).unwrap();
        let error = write_as("Y").unwrap_err();
        assert!(
            matches!(
                &error,
                WorkflowError::NotOwner { ours, recorded, pending_only: true, .. }
                    if ours == "Y" && recorded.as_deref() == Some("X")
            ),
            "got {error}"
        );
        assert!(!ran.get(), "a pending launch must not reach its closure");
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let (committed, ()) = write_as("X").expect("the recorded launch writes");
        assert!(ran.get());
        assert!(committed.stopped);
        assert!(load_state(root, phase).unwrap().stopped);
    }

    /// R-09's check-to-side-effect gap: the closure runs only for the recorded launch,
    /// inside the one hold of the lock the check made, and nothing is written.
    #[test]
    fn while_owner_runs_its_closure_only_for_the_recorded_launch_under_the_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1901);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.supervisor = Some(crate::state::SupervisorRecord::new(
            root.join("x.sock"),
            "xxxx".to_string(),
            4_294_967_000,
            1,
            String::new(),
        ));
        save_state(&state).unwrap();
        let path = state_path(root, phase);
        let before = std::fs::read(&path).unwrap();

        let mut called = false;
        let refused = while_owner(root, phase, "yyyy", || {
            called = true;
            Ok::<(), WorkflowError>(())
        });
        assert!(
            matches!(
                refused,
                Err(WorkflowError::NotOwner { ref ours, ref recorded, pending_only: false, .. })
                    if ours == "yyyy" && recorded.as_deref() == Some("xxxx")
            ),
            "{refused:?}"
        );
        assert!(!called, "a launch the state does not record runs nothing");

        let held = while_owner(root, phase, "xxxx", || write_lock_status(root, phase));
        assert_eq!(
            held.unwrap(),
            WriteLockStatus::Held {
                pid: Some(std::process::id())
            },
            "the closure runs inside the check's hold of the lock"
        );
        // Another test's fork can keep the released lock busy until its exec.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while write_lock_status(root, phase).unwrap() != WriteLockStatus::Free {
            assert!(
                std::time::Instant::now() < deadline,
                "control: the probe reads the lock free once the hold ends"
            );
            std::thread::sleep(WRITE_LOCK_POLL);
        }
        assert_eq!(std::fs::read(&path).unwrap(), before, "it writes no state");
    }

    /// R-07's primitive: the decision sees what is on disk with the lock held, and the
    /// removal shares that hold.
    #[test]
    fn clear_state_if_decides_on_a_fresh_read_under_the_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2105);
        save_state(&state_in(root, phase, Stage::Code)).unwrap();
        let path = state_path(root, phase);
        let before = std::fs::read(&path).unwrap();
        let held_by_us = WriteLockStatus::Held {
            pid: Some(std::process::id()),
        };

        // `Ok(false)`: nothing removed.
        let kept = clear_state_if::<WorkflowError>(root, phase, None, |existing| {
            let Existing::Loaded(seen) = existing else {
                panic!("the saved state must read as loaded: {existing:?}");
            };
            let on_disk: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(&seen).unwrap(), on_disk);
            assert_eq!(write_lock_status(root, phase).unwrap(), held_by_us);
            Ok(false)
        })
        .unwrap();
        assert!(!kept.removed);
        assert!(kept.result.is_ok());
        assert_eq!(std::fs::read(&path).unwrap(), before);

        // `Err`: nothing removed.
        let failed = clear_state_if::<WorkflowError>(root, phase, None, |_| {
            Err(WorkflowError::MissingState(PathBuf::from("the decision's")))
        });
        assert!(
            matches!(&failed, Err(WorkflowError::MissingState(path))
                if path.as_path() == Path::new("the decision's")),
            "{failed:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);

        // A held lock with a bound: the decision does not run and nothing changes.
        let (entered, writer_is_inside) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();
        let decided = std::cell::Cell::new(false);
        std::thread::scope(|scope| {
            scope.spawn(move || {
                update_field(root, phase, |_| {
                    entered.send(()).unwrap();
                    released.recv().unwrap();
                });
            });
            writer_is_inside.recv().unwrap();
            let timed_out = clear_state_if::<WorkflowError>(
                root,
                phase,
                Some(Duration::from_millis(200)),
                |_| {
                    decided.set(true);
                    Ok(true)
                },
            );
            release.send(()).unwrap();
            assert!(
                matches!(
                    &timed_out,
                    Err(WorkflowError::WriteLockTimeout { holder: Some(pid), .. })
                        if *pid == std::process::id()
                ),
                "{timed_out:?}"
            );
        });
        assert!(!decided.get(), "a timed-out clear must not decide");
        assert_eq!(std::fs::read(&path).unwrap(), before);

        // `Ok(true)`: removed.
        let cleared = clear_state_if::<WorkflowError>(root, phase, None, |_| Ok(true)).unwrap();
        assert!(cleared.removed);
        assert!(cleared.result.is_ok());
        assert!(!path.exists(), "an `Ok(true)` decision removes the state");
    }

    #[test]
    fn clear_state_if_owned_refuses_a_launch_the_state_does_not_record() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1902);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.supervisor = Some(crate::state::SupervisorRecord::new(
            root.join("x.sock"),
            "xxxx".to_string(),
            4_294_967_000,
            1,
            String::new(),
        ));
        save_state(&state).unwrap();
        let path = state_path(root, phase);

        let refused = clear_state_if_owned(root, phase, "yyyy");
        assert!(
            matches!(refused, Err(WorkflowError::NotOwner { .. })),
            "{refused:?}"
        );
        assert!(path.exists(), "a refused clear leaves the file");

        assert!(
            clear_state_if_owned(root, phase, "xxxx").unwrap(),
            "control: the recorded launch's clear removes it"
        );
        assert!(!path.exists(), "control: the file is gone");
    }

    #[test]
    fn write_lock_status_reports_a_lock_file_it_cannot_open() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(devflow_dir(dir.path()), "a file, not a directory").unwrap();

        let result = write_lock_status(dir.path(), PhaseId::new(50));

        assert!(
            matches!(result, Err(WorkflowError::Io(_))),
            "a lock nobody can ask about must not read as free, got {result:?}"
        );
    }

    /// A signal handler installed without `SA_RESTART` makes a blocked `flock` return
    /// EINTR. DevFlow's own stop handler sets `SA_RESTART`, but this crate is a library
    /// and its host's handlers are not ours. The handler is process-global, so this
    /// runs in a child.
    #[test]
    fn a_signal_does_not_fail_a_blocked_write() {
        use std::os::unix::thread::JoinHandleExt;
        const NAME: &str = "workflow::tests::a_signal_does_not_fail_a_blocked_write";
        extern "C" fn do_nothing(_signal: libc::c_int) {}

        if !crate::test_support::in_child_test(NAME) {
            let empty_path = tempfile::tempdir().unwrap();
            let output = crate::test_support::run_test_in_child(NAME, empty_path.path(), &[]);
            crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        // SAFETY: an all-zero `sigaction` is a valid value of the struct: no flags,
        // an empty mask. The handler does nothing, so it is async-signal-safe.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = do_nothing as extern "C" fn(libc::c_int) as libc::sighandler_t;
            assert_eq!(
                libc::sigaction(libc::SIGUSR1, &action, std::ptr::null_mut()),
                0
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(50);
        save_state(&state_in(&root, phase, Stage::Code)).unwrap();
        update_field(&root, phase, |_| {});
        let holder = File::options()
            .read(true)
            .write(true)
            .open(state_lock_path(&root, phase))
            .unwrap();
        holder.lock().unwrap();

        let writer_root = root.clone();
        let writer = std::thread::spawn(move || {
            update_state(&writer_root, phase, |state| {
                state.stopped = true;
                Ok::<(), WorkflowError>(())
            })
            .map(|_| ())
        });
        for _ in 0..5 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            // SAFETY: the writer has not been joined, so its pthread_t is valid.
            assert_eq!(
                unsafe { libc::pthread_kill(writer.as_pthread_t(), libc::SIGUSR1) },
                0
            );
        }
        drop(holder);

        writer
            .join()
            .unwrap()
            .expect("a signal must not fail a write blocked on the lock");
        assert!(load_state(&root, phase).unwrap().stopped);
    }

    #[test]
    fn paths_are_per_phase_under_devflow_dir() {
        let root = Path::new("/repo");
        assert_eq!(devflow_dir(root), Path::new("/repo/.devflow"));
        assert_eq!(
            state_path(root, PhaseId::new(7)),
            Path::new("/repo/.devflow/state-07.json")
        );
        assert_eq!(
            state_path(root, PhaseId::new(14)),
            Path::new("/repo/.devflow/state-14.json")
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), PhaseId::new(1), Stage::Code);
        save_state(&state).expect("save");

        let loaded = load_state(dir.path(), PhaseId::new(1)).expect("load");
        assert_eq!(loaded.stage, Stage::Code);
        assert_eq!(loaded.phase, PhaseId::new(1));
        assert_eq!(loaded.agent, AgentKind::Claude);
        assert_eq!(loaded.mode, Mode::Auto);
    }

    /// 13-DEFERRED-CR-03 scenario 1: two phases' states must coexist — the
    /// second `start` no longer clobbers the first phase's state.
    #[test]
    fn two_phases_states_coexist_without_clobbering() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), PhaseId::new(13), Stage::Code)).unwrap();
        save_state(&state_in(dir.path(), PhaseId::new(14), Stage::Validate)).unwrap();

        let a = load_state(dir.path(), PhaseId::new(13)).expect("phase 13 state");
        let b = load_state(dir.path(), PhaseId::new(14)).expect("phase 14 state");
        assert_eq!(a.phase, PhaseId::new(13));
        assert_eq!(a.stage, Stage::Code);
        assert_eq!(b.phase, PhaseId::new(14));
        assert_eq!(b.stage, Stage::Validate);
    }

    #[test]
    fn clear_state_removes_durably() {
        use crate::durable::{Synced, take_synced};
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), PhaseId::new(1), Stage::Validate);
        save_state(&state).expect("save");
        take_synced();

        assert!(clear_state(dir.path(), PhaseId::new(1)).expect("clear"));

        assert!(!state_path(dir.path(), PhaseId::new(1)).exists());
        let synced = take_synced();
        assert!(
            synced.contains(&Synced::Dir(dir.path().join(".devflow"))),
            "clearing must fsync .devflow after the unlink, recorded: {synced:?}"
        );
    }

    fn sorted_entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn ensure_devflow_dir_steady_state_creates_no_temp_and_fsyncs_only_the_parent() {
        use crate::durable::{Publication, Synced, publish_exclusive_durably, take_synced};
        let root = tempfile::tempdir().unwrap();
        let devflow = root.path().join(".devflow");
        let gitignore = devflow.join(".gitignore");

        ensure_devflow_dir(&devflow).expect("first call");
        assert!(take_synced().contains(&Synced::Dir(root.path().to_path_buf())));
        assert_eq!(std::fs::read_to_string(&gitignore).unwrap(), "*\n");

        let before = sorted_entries(&devflow);
        ensure_devflow_dir(&devflow).expect("second call");
        assert_eq!(take_synced(), [Synced::Dir(root.path().to_path_buf())]);
        assert_eq!(sorted_entries(&devflow), before, "no temp and no new entry");

        // Control: the exclusive publish on the existing marker syncs a temp on
        // every call, so the recorder would see one if the routing ever used it.
        assert_eq!(
            publish_exclusive_durably(&gitignore, b"*\n").unwrap(),
            Publication::AlreadyExists
        );
        let synced = take_synced();
        assert!(
            synced.iter().any(|entry| matches!(
                entry,
                Synced::File(temp) if {
                    let name = temp.file_name().unwrap().to_string_lossy();
                    // The shared temp shape `.{name}.{pid}.{n}.tmp`, for a name
                    // that itself starts with a dot.
                    name.starts_with("..gitignore.") && name.ends_with(".tmp")
                }
            )),
            "the publish syncs a temp: {synced:?}"
        );
    }

    #[test]
    fn ensure_devflow_dir_syncs_the_devflow_entry_for_a_nested_dir() {
        use crate::durable::{Synced, take_synced};
        let root = tempfile::tempdir().unwrap();
        let devflow = root.path().join(".devflow");
        std::fs::create_dir(&devflow).unwrap();

        ensure_devflow_dir(&devflow.join("gates")).expect("nested call");

        let synced = take_synced();
        let count = |wanted: &Synced| synced.iter().filter(|entry| *entry == wanted).count();
        assert_eq!(
            count(&Synced::Dir(root.path().to_path_buf())),
            1,
            "the .devflow entry: {synced:?}"
        );
        assert_eq!(
            count(&Synced::Dir(devflow.clone())),
            1,
            "the gates entry: {synced:?}"
        );
        assert!(
            !synced.iter().any(|entry| matches!(entry, Synced::File(_))),
            "no file fsync: {synced:?}"
        );
    }

    #[test]
    fn ensure_devflow_dir_recreates_a_deleted_marker() {
        let root = tempfile::tempdir().unwrap();
        let devflow = root.path().join(".devflow");
        ensure_devflow_dir(&devflow).expect("first call");
        std::fs::remove_file(devflow.join(".gitignore")).unwrap();

        ensure_devflow_dir(&devflow).expect("second call");

        assert_eq!(
            std::fs::read_to_string(devflow.join(".gitignore")).unwrap(),
            "*\n"
        );
    }

    #[test]
    fn save_state_is_written_through_the_durable_replace() {
        use crate::durable::{Synced, take_synced};
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), PhaseId::new(1), Stage::Validate);
        take_synced();

        save_state(&state).expect("save");

        let synced = take_synced();
        assert!(
            synced.contains(&Synced::Dir(dir.path().join(".devflow"))),
            "a state save must fsync .devflow after the rename, recorded: {synced:?}"
        );
        let loaded = load_state(dir.path(), PhaseId::new(1)).expect("load");
        assert_eq!(loaded.stage, Stage::Validate);
        assert_eq!(loaded.phase, state.phase);
    }

    #[test]
    fn save_state_writes_atomically_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), PhaseId::new(1), Stage::Validate);

        save_state(&state).expect("save");

        let path = state_path(dir.path(), PhaseId::new(1));
        assert!(path.exists());
        let loaded = load_state(dir.path(), PhaseId::new(1)).expect("load");
        assert_eq!(loaded.stage, Stage::Validate);
        assert_eq!(loaded.phase, state.phase);
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(
            std::fs::read_dir(path.parent().unwrap())
                .unwrap()
                .flatten()
                .all(|entry| {
                    let entry = entry.file_name();
                    let entry = entry.to_string_lossy();
                    !entry.starts_with(&format!(".{name}.")) || !entry.ends_with(".tmp")
                }),
            "successful state writes leave no temporary files"
        );
    }

    #[test]
    fn load_missing_state_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_state(dir.path(), PhaseId::new(1)).unwrap_err();
        assert!(matches!(err, WorkflowError::MissingState(_)));
    }

    #[test]
    fn clear_removes_state_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), PhaseId::new(1), Stage::Validate);
        save_state(&state).unwrap();
        assert!(state_path(dir.path(), PhaseId::new(1)).exists());

        assert!(clear_state(dir.path(), PhaseId::new(1)).expect("clear"));
        assert!(!state_path(dir.path(), PhaseId::new(1)).exists());
        // Clearing when nothing is present is a no-op success, and says so.
        assert!(!clear_state(dir.path(), PhaseId::new(1)).expect("clear again"));
    }

    #[test]
    fn clear_only_touches_its_own_phase() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), PhaseId::new(13), Stage::Code)).unwrap();
        save_state(&state_in(dir.path(), PhaseId::new(14), Stage::Ship)).unwrap();

        clear_state(dir.path(), PhaseId::new(13)).unwrap();

        assert!(!state_path(dir.path(), PhaseId::new(13)).exists());
        assert!(
            load_state(dir.path(), PhaseId::new(14)).is_ok(),
            "phase 14 must survive"
        );
    }

    #[test]
    fn state_write_leaves_no_temp_and_list_states_ignores_a_planted_temp() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        let state = state_in(dir.path(), phase, Stage::Code);

        save_state(&state).unwrap();
        let path = state_path(dir.path(), phase);
        let temp_prefix = format!(".{}.", path.file_name().unwrap().to_string_lossy());
        assert!(
            std::fs::read_dir(path.parent().unwrap())
                .unwrap()
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .all(|name| !name.starts_with(&temp_prefix)),
            "successful state writes leave no unique temp"
        );
        let orphan = path.with_file_name(format!(
            ".{}.1.0.tmp",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&orphan, "orphan").unwrap();

        let states = list_states(dir.path());
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, phase);
        assert_eq!(states[0].stage, Stage::Code);
        assert!(orphan.exists(), "the planted orphan is not a state file");
    }

    #[test]
    fn clear_state_sweeps_orphan_temps_for_its_phase_only() {
        let dir = tempfile::tempdir().unwrap();
        let phase_48 = state_path(dir.path(), PhaseId::new(48));
        let phase_49 = state_path(dir.path(), PhaseId::new(49));
        std::fs::create_dir_all(phase_48.parent().unwrap()).unwrap();
        let orphan_48 = phase_48.with_file_name(format!(
            ".{}.1.0.tmp",
            phase_48.file_name().unwrap().to_string_lossy()
        ));
        let orphan_49 = phase_49.with_file_name(format!(
            ".{}.1.0.tmp",
            phase_49.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&orphan_48, "orphan").unwrap();
        std::fs::write(&orphan_49, "orphan").unwrap();

        clear_state(dir.path(), PhaseId::new(48)).unwrap();

        assert!(!orphan_48.exists());
        assert!(orphan_49.exists());
    }

    #[test]
    fn clear_state_warns_but_succeeds_when_an_orphan_temp_cannot_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        let path = state_path(dir.path(), phase);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "state").unwrap();
        let orphan = path.with_file_name(format!(
            ".{}.1.0.tmp",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::create_dir_all(&orphan).unwrap();

        assert!(clear_state(dir.path(), phase).expect("clear"));

        assert!(!path.exists(), "the state file was removed");
        assert!(
            orphan.is_dir(),
            "the temp directory is the negative control for the warn-only path"
        );
    }

    #[test]
    fn unique_temp_creation_retries_after_a_same_pid_orphan() {
        let dir = tempfile::tempdir().unwrap();
        let path = state_path(dir.path(), PhaseId::new(48));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let counter = std::sync::atomic::AtomicU64::new(0);
        let orphan = path.with_file_name(format!(
            ".{}.{pid}.0.tmp",
            path.file_name().unwrap().to_string_lossy(),
            pid = std::process::id()
        ));
        std::fs::write(&orphan, "orphan").unwrap();

        let (tmp, _file) = create_unique_temp(&path, std::process::id(), &counter).unwrap();

        assert!(orphan.exists());
        assert!(tmp.ends_with(format!(
            ".{}.{pid}.1.tmp",
            path.file_name().unwrap().to_string_lossy(),
            pid = std::process::id()
        )));
    }

    #[test]
    fn list_states_enumerates_sorted_by_phase() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), PhaseId::new(14), Stage::Ship)).unwrap();
        save_state(&state_in(dir.path(), PhaseId::new(3), Stage::Code)).unwrap();

        let states = list_states(dir.path());
        assert_eq!(
            states.iter().map(|s| s.phase).collect::<Vec<_>>(),
            vec![PhaseId::new(3), PhaseId::new(14)]
        );
    }

    #[test]
    fn list_states_empty_when_no_devflow_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_states(dir.path()).is_empty());
    }

    #[test]
    fn list_states_skips_corrupt_files() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), PhaseId::new(5), Stage::Code)).unwrap();
        std::fs::write(state_path(dir.path(), PhaseId::new(6)), "not json").unwrap();

        let states = list_states(dir.path());
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, PhaseId::new(5));
    }

    /// Phase 50 D-20: a `state.json` left by an old binary is not state. Every read
    /// and the phase's clear ignore it, and the file stays exactly as it was.
    #[test]
    fn a_legacy_state_json_is_neither_read_nor_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(7);
        let legacy = devflow_dir(root).join("state.json");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let bytes = serde_json::to_vec_pretty(&state_in(root, phase, Stage::Code)).unwrap();
        std::fs::write(&legacy, &bytes).unwrap();

        assert!(matches!(
            load_state(root, phase),
            Err(WorkflowError::MissingState(_))
        ));
        assert!(list_states(root).is_empty());
        assert!(!clear_state(root, phase).expect("clear"));

        assert_eq!(std::fs::read(&legacy).unwrap(), bytes);
        assert!(!state_path(root, phase).exists());
    }

    /// Whether `line` names `save_state` as a whole identifier: an occurrence with no
    /// ASCII letter, digit or `_` on either side, which also catches the function
    /// passed as a value.
    fn names_save_state(line: &str) -> bool {
        const NAME: &str = "save_state";
        let continues_identifier = |c: char| c.is_ascii_alphanumeric() || c == '_';
        line.match_indices(NAME).any(|(at, _)| {
            !line[..at]
                .chars()
                .next_back()
                .is_some_and(continues_identifier)
                && !line[at + NAME.len()..]
                    .chars()
                    .next()
                    .is_some_and(continues_identifier)
        })
    }

    /// Every line of `source` that names `save_state`, before the first line that is
    /// exactly `mod tests {`, leaving out whole-line `//` comments and the
    /// definition line. Lines keep their
    /// numbers in `source`, and a trailing comment stays as part of its code line.
    fn save_state_violations(path: &str, source: &str) -> Vec<String> {
        source
            .lines()
            .enumerate()
            .take_while(|(_, line)| *line != "mod tests {")
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .filter(|(_, line)| !line.trim_start().starts_with("pub fn save_state("))
            .filter(|(_, line)| names_save_state(line))
            .map(|(index, _)| format!("{path}: line {}: save_state", index + 1))
            .collect()
    }

    /// Every `.rs` file under `dir`, recursively, in sorted order.
    fn rust_files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.extend(rust_files_under(&path));
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
        files.sort();
        files
    }

    /// V-11 (criterion 3): no production code in this crate names `save_state`, whose
    /// whole-snapshot write would bring back the lost update `update_state` removes.
    /// Only test code is excluded: `mod tests` and the `test_support` module, whose
    /// test-only declaration is asserted here, so no production file is allowlisted.
    #[test]
    fn no_production_code_saves_state_outside_update_state() {
        const SOURCE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        assert!(
            include_str!("lib.rs")
                .contains("#[cfg(any(test, feature = \"test-support\"))]\npub mod test_support;"),
            "test_support.rs is skipped only while it is declared test-only"
        );
        let root = std::path::Path::new(SOURCE_ROOT);
        let files = rust_files_under(root);
        let relative: Vec<String> = files
            .iter()
            .map(|file| file.strip_prefix(root).unwrap().display().to_string())
            .collect();
        for expected in ["workflow.rs", "supervisor/runtime.rs"] {
            assert!(
                relative.iter().any(|file| file == expected),
                "the walk must reach {expected}: {relative:?}"
            );
        }
        let real: Vec<String> = files
            .iter()
            .zip(&relative)
            .filter(|(_, relative)| relative.as_str() != "test_support.rs")
            .flat_map(|(file, relative)| {
                save_state_violations(relative, &std::fs::read_to_string(file).unwrap())
            })
            .collect();
        assert_eq!(
            real,
            Vec::<String>::new(),
            "production save_state references"
        );

        let call = "fn f() {\n    workflow::save_state(&state)?;\n}\n";
        assert_eq!(
            save_state_violations("planted.rs", call),
            vec!["planted.rs: line 2: save_state"],
            "a planted call"
        );
        let value = "    match publish_record(state, published, workflow::save_state)? {\n";
        assert_eq!(
            save_state_violations("planted.rs", value),
            vec!["planted.rs: line 1: save_state"],
            "the function passed as a value"
        );
        for (source, shape) in [
            (
                "let n = save_state_violations(p, s);",
                "a longer identifier",
            ),
            (
                "let n = my_save_state(p, s);",
                "a longer identifier, on the left",
            ),
            ("// workflow::save_state(&state)?;", "a commented-out call"),
            (
                "fn f() {}\nmod tests {\n    workflow::save_state(&state)?;\n}\n",
                "a call after `mod tests {`",
            ),
            (
                "pub fn save_state(state: &State) -> Result<(), WorkflowError> {",
                "the definition",
            ),
        ] {
            assert_eq!(
                save_state_violations("planted.rs", source),
                Vec::<String>::new(),
                "{shape}"
            );
        }
    }
}
