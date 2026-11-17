//! Per-launch liveness: a `flock` a launcher takes before it saves a pending
//! record, and a spawned supervisor inherits and holds for life, so a pending
//! supervisor is probeable before its socket exists.
//!
//! D-01's two cases are told apart by the lock alone, with no pid read:
//! a supervisor spawned but not yet published holds the lock, so the file
//! reads [`Held::Live`]; a reservation whose launcher died before spawning
//! anything holds nothing, so it reads [`Held::Abandoned`].
//!
//! Two traps, both pinned by this module's tests:
//!
//! - `dup2(3, 3)` is a no-op that leaves close-on-exec set, so a descriptor
//!   that already is fd 3 would lose the lock at exec. `pass_to_child`
//!   clears the flag in that case.
//! - A `flock` belongs to the open file description, so `unlock()` on any
//!   duplicate releases the lock a child inherited. Dropping a [`Hold`] is the
//!   only release.
//!
//! The identity record a hold writes (pid and start time) is advisory, not state,
//! and stays outside D-09's durable helpers and their scan: the lock, not its
//! bytes, decides liveness, and a crash that loses the record loses nothing. It
//! only names the holder for an operator.

use std::fs::{File, OpenOptions, TryLockError};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::phase_id::PhaseId;

/// The suffix of the temporary name a hold is locked under before it is
/// published: `.{final_file_name}.{pid}.{seq}.hold-tmp`, beside the final name.
///
/// A temp is never a hold. No probe, holder lookup or sweep of final names
/// reads one, and only `remove_orphan_hold_temps` removes one.
pub const HOLD_TEMP_SUFFIX: &str = ".hold-tmp";

/// How long taking an exclusive lock waits out a probe's momentary shared lock.
/// A real holder holds for its lifetime, so it outlasts this.
const PROBE_RACE_WAIT: Duration = Duration::from_millis(50);

const PROBE_RACE_POLL: Duration = Duration::from_millis(1);

static HOLD_SEQ: AtomicU64 = AtomicU64::new(0);

/// An exclusive lock on a liveness file, released only by dropping it.
///
/// Never call `unlock()` on a hold or on any duplicate of its descriptor: the
/// lock belongs to the open file description, so that releases the lock a
/// child inherited while the child still runs.
#[derive(Debug)]
pub struct Hold {
    file: File,
    path: PathBuf,
}

/// The points of a take, in order, at which `take_inner` calls its hook. Each
/// call passes the temp's path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TakeStep {
    /// The temp exists and is not locked yet.
    TempCreated,
    /// The temp is locked and has its record; the final name does not exist.
    Locked,
    /// The final name exists, and the temp has not been removed.
    Published,
}

impl Hold {
    /// Take a hold on `path`, publishing that name only once the lock is held,
    /// so no visible hold file is ever unheld while its creator lives. A sweep
    /// may therefore remove a file that probes [`Held::Abandoned`] with no age
    /// bound: nothing takes an exclusive lock through an existing hold name,
    /// so such a file can never read [`Held::Live`] again.
    ///
    /// # Errors
    /// [`std::io::ErrorKind::AlreadyExists`] if `path` exists, which is never
    /// replaced; any other error creating, locking or linking the file. On any
    /// error `path` was not created.
    pub fn take(path: &Path) -> std::io::Result<Hold> {
        take_inner(path, &mut |_, _| {})
    }

    /// The path this hold was taken on.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRawFd for Hold {
    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

/// [`Hold::take`], calling `hook` at each [`TakeStep`].
fn take_inner(path: &Path, hook: &mut dyn FnMut(TakeStep, &Path)) -> std::io::Result<Hold> {
    let (temp, file) = create_hold_temp(path)?;
    hook(TakeStep::TempCreated, &temp);
    if let Err(error) = lock_and_publish(&file, &temp, path, hook) {
        remove_temp_or_warn(&temp);
        return Err(error);
    }
    hook(TakeStep::Published, &temp);
    // The hold is already published and held. A temp left here names this live
    // process, so it stays out of any sweep's reach until this process exits.
    remove_temp_or_warn(&temp);
    Ok(Hold {
        file,
        path: path.to_path_buf(),
    })
}

/// Create `path`'s temp beside it, taking the next sequence on a collision.
fn create_hold_temp(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let Some(name) = path.file_name() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} has no file name", path.display()),
        ));
    };
    let name = name.to_string_lossy();
    let pid = std::process::id();
    loop {
        let seq = HOLD_SEQ.fetch_add(1, Ordering::Relaxed);
        let temp = path.with_file_name(format!(".{name}.{pid}.{seq}{HOLD_TEMP_SUFFIX}"));
        let created = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp);
        match created {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

/// Lock `temp`, record this process in it, and link it to `path`. A hard link
/// never replaces: an existing `path` fails `AlreadyExists`, and a temp removed
/// under the take fails `NotFound` rather than publishing an unheld name.
fn lock_and_publish(
    file: &File,
    temp: &Path,
    path: &Path,
    hook: &mut dyn FnMut(TakeStep, &Path),
) -> std::io::Result<()> {
    try_lock_past_probes(file)?;
    write_identity_record(file)?;
    hook(TakeStep::Locked, temp);
    std::fs::hard_link(temp, path)
}

fn remove_temp_or_warn(temp: &Path) {
    if let Err(error) = std::fs::remove_file(temp)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("could not remove the hold temp {}: {error}", temp.display());
    }
}

/// Take `file`'s exclusive lock, waiting out a probe's momentary shared lock
/// for at most [`PROBE_RACE_WAIT`] before reporting `WouldBlock`.
fn try_lock_past_probes(file: &File) -> Result<(), TryLockError> {
    let deadline = Instant::now() + PROBE_RACE_WAIT;
    loop {
        match file.try_lock() {
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(PROBE_RACE_POLL);
            }
            other => return other,
        }
    }
}

/// Write this process's identity into a hold's file: `"{pid} {start}\n"`, or
/// the pid alone when its start time cannot be read.
///
/// The write is positional, because it neither reads nor moves the descriptor's
/// offset. A descriptor inherited across exec shares its offset with every
/// duplicate, so a cursor write after a truncate would land past a NUL hole.
pub(crate) fn write_identity_record(file: &File) -> std::io::Result<()> {
    let pid = std::process::id();
    let record = match crate::agent::process_start_time(pid) {
        Some(start) => format!("{pid} {start}\n"),
        None => format!("{pid}\n"),
    };
    file.set_len(0)?;
    file.write_all_at(record.as_bytes(), 0)
}

/// What a probe of a liveness file found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Some process holds the lock.
    Live,
    /// The file exists and nothing holds its lock.
    Abandoned,
    /// There is no such file.
    Missing,
}

/// Read a liveness file's state from its lock alone, with no pid read.
///
/// The probe tries a shared lock through its own open, so concurrent probes
/// never read each other as live, while an exclusive holder still blocks it.
///
/// # Errors
/// Any error opening or locking the file other than its absence.
pub fn probe(path: &Path) -> std::io::Result<Held> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Held::Missing),
        Err(error) => return Err(error),
    };
    match file.try_lock_shared() {
        Ok(()) => Ok(Held::Abandoned),
        Err(TryLockError::WouldBlock) => Ok(Held::Live),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// A fresh launch id: 16 lowercase hex characters from this process's pid,
/// the current time in nanoseconds and a process-wide counter. Uniqueness is
/// also enforced where an id names a file, which refuses an existing name.
pub fn new_launch_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64);
    let mixed = u64::from(std::process::id()) ^ nanos ^ count.rotate_left(48);
    format!("{mixed:016x}")
}

/// Whether `id` has [`new_launch_id`]'s format: 16 lowercase hex digits. A
/// launch id read from disk is built into paths and printed into copy-paste
/// lines, so this is the one rule every reader applies before trusting one.
pub(crate) fn is_launch_id(id: &str) -> bool {
    const LAUNCH_ID_LEN: usize = 16;
    id.len() == LAUNCH_ID_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The kernel boot id, trimmed, or `None` where it cannot be read.
pub fn boot_id() -> Option<String> {
    read_boot_id(Path::new("/proc/sys/kernel/random/boot_id"))
}

/// [`boot_id`] against an explicit file, for tests.
pub(crate) fn read_boot_id(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let trimmed = contents.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The liveness file of launch `launch_id`: `.devflow/supervisor-{launch_id}.live`.
/// `None` unless the id has [`new_launch_id`]'s format, so a
/// launch id read from disk never builds a path outside `.devflow/`.
pub fn launch_liveness_path(project_root: &Path, launch_id: &str) -> Option<PathBuf> {
    is_launch_id(launch_id).then(|| {
        project_root
            .join(".devflow")
            .join(format!("supervisor-{launch_id}.live"))
    })
}

/// The per-phase holder file (D-17): `.devflow/.holder-{padded}`.
pub fn phase_holder_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    project_root
        .join(".devflow")
        .join(format!(".holder-{}", phase.padded()))
}

/// A holder's recorded pid and start time, as [`live_holder`] reads them.
type HolderIdentity = (u32, Option<u64>);

/// Why [`claim_phase_holder`] did not claim.
#[derive(Debug)]
pub enum HolderError {
    /// Another process holds the phase. `holder` is its recorded pid and start time, or
    /// `None` when the record does not read.
    Held { holder: Option<(u32, Option<u64>)> },
    /// The file could not be created, locked or written.
    Io(std::io::Error),
}

impl std::fmt::Display for HolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HolderError::Held {
                holder: Some((pid, _)),
            } => write!(f, "held by a foreground process (pid {pid})"),
            HolderError::Held { holder: None } => {
                write!(f, "held by a foreground process whose record does not read")
            }
            HolderError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl From<std::io::Error> for HolderError {
    fn from(error: std::io::Error) -> Self {
        HolderError::Io(error)
    }
}

/// Claim `phase`'s holder for this process (D-17): an exclusive lock on
/// [`phase_holder_path`], held until the returned [`Hold`] drops, whose record names
/// this process. A foreground process that does workflow work outside a supervisor and
/// outside a gate wait holds it for its lifetime, so `stop` can reach it and a second
/// such process refuses at once. A probe's momentary shared lock is waited out first.
///
/// The file is never removed, replaced or `unlock()`ed: a removed file lets two claimers
/// lock different inodes. It is never a [`Hold::take`] file, so the publish-before-visible
/// rule does not apply: its hazard needs a removal between an open and a lock. No state
/// write reads or waits on it. Its record is advisory and outside D-09's scope: the lock,
/// not its bytes, decides liveness, and a reader racing the rewrite gets a record that
/// does not read or does not match, and refuses.
///
/// The descriptor is close-on-exec, so nothing this process spawns keeps the phase held.
///
/// # Errors
/// [`HolderError::Held`] when another open holds the lock; [`HolderError::Io`] when the
/// file could not be created, locked or written.
pub fn claim_phase_holder(project_root: &Path, phase: PhaseId) -> Result<Hold, HolderError> {
    crate::workflow::ensure_devflow_dir(&project_root.join(".devflow"))?;
    match claim_lock_file(phase_holder_path(project_root, phase))? {
        Ok(hold) => Ok(hold),
        Err(holder) => Err(HolderError::Held { holder }),
    }
}

/// Open `path` (created owner-only, never truncated), take its exclusive lock past a
/// probe's momentary shared lock, and record this process in it. The inner `Err` is
/// another holder's recorded identity, `None` when its record does not read.
fn claim_lock_file(path: PathBuf) -> std::io::Result<Result<Hold, Option<HolderIdentity>>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)?;
    match try_lock_past_probes(&file) {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(Err(live_holder(&path).ok().flatten())),
        Err(TryLockError::Error(error)) => return Err(error),
    }
    write_identity_record(&file)?;
    Ok(Ok(Hold { file, path }))
}

/// The per-phase runner lock (D-07): `.devflow/.runner-{padded}`.
pub fn runner_lock_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    project_root
        .join(".devflow")
        .join(format!(".runner-{}", phase.padded()))
}

/// Why [`claim_runner_lock`] did not claim.
#[derive(Debug)]
pub enum RunnerLockError {
    /// Another runner holds the phase's runner lock. `holder` is its recorded pid and
    /// start time, or `None` when the record does not read.
    Held { holder: Option<(u32, Option<u64>)> },
    /// The file could not be created, locked or written.
    Io(std::io::Error),
}

impl std::fmt::Display for RunnerLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerLockError::Held {
                holder: Some((pid, _)),
            } => write!(f, "the runner lock is held by another runner (pid {pid})"),
            RunnerLockError::Held { holder: None } => write!(
                f,
                "the runner lock is held by another runner whose record does not read"
            ),
            RunnerLockError::Io(error) => write!(f, "the runner lock: {error}"),
        }
    }
}

impl From<std::io::Error> for RunnerLockError {
    fn from(error: std::io::Error) -> Self {
        RunnerLockError::Io(error)
    }
}

/// Claim `phase`'s runner lock for this process (D-07): an exclusive lock on
/// [`runner_lock_path`], held until the returned [`Hold`] drops, whose record names this
/// process. `devflow runner` takes it before anything else and holds it for its whole
/// life, so the lock alone says whether a runner is alive for the phase.
///
/// Same invariants as [`claim_phase_holder`]: the file is never removed, replaced or
/// `unlock()`ed, it is created mode 0o600, and its record is advisory. The descriptor is
/// close-on-exec, so the agent a runner spawns never keeps the lock held.
///
/// # Errors
/// [`RunnerLockError::Held`] when another open holds the lock; [`RunnerLockError::Io`]
/// when the file could not be created, locked or written.
pub fn claim_runner_lock(project_root: &Path, phase: PhaseId) -> Result<Hold, RunnerLockError> {
    crate::workflow::ensure_devflow_dir(&project_root.join(".devflow"))?;
    match claim_lock_file(runner_lock_path(project_root, phase))? {
        Ok(hold) => Ok(hold),
        Err(holder) => Err(RunnerLockError::Held { holder }),
    }
}

/// Turn SIGTERM and SIGINT into a stop flag for this process (D-18), once per process,
/// with `SA_RESTART` and an empty mask: a foreground process `stop` signals ends at its
/// next safe point instead of wherever the signal lands. It reads the flag through
/// [`holder_stop_signal`].
///
/// It installs the process's one SIGTERM/SIGINT handler,
/// [`crate::stop::install_signal_pipe`], so a holder and a supervisor or
/// runner in one process never replace each other's handler. No signal is blocked,
/// because a blocked mask would be inherited by every child. A caught signal goes back to
/// its default at exec, so the supervisors, agents and hooks a holder spawns are
/// unaffected.
///
/// # Errors
/// The pipe or `sigaction` failed. The result of the one attempt is cached.
pub fn install_holder_stop_handler() -> std::io::Result<()> {
    crate::stop::install_signal_pipe()
}

/// The signal [`install_holder_stop_handler`]'s handler caught, if one has arrived. A
/// later signal replaces an earlier one.
pub fn holder_stop_signal() -> Option<i32> {
    crate::stop::caught_stop_signal()
}

/// Whose lock this is: the pid and start time the holder recorded, while the
/// lock is held; `None` when nothing holds it. For telling an operator or the
/// stray-reaper safety set who holds it, never for deciding liveness: [`probe`]
/// alone decides that.
///
/// # Errors
/// [`std::io::ErrorKind::InvalidData`] for an empty or unparsable record, which
/// a reader racing a rewrite can get, so its caller refuses rather than acts;
/// any other error probing or reading the file.
pub fn live_holder(path: &Path) -> std::io::Result<Option<(u32, Option<u64>)>> {
    if probe(path)? != Held::Live {
        return Ok(None);
    }
    let record = match std::fs::read_to_string(path) {
        Ok(record) => record,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    parse_identity_record(&record).map(Some).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} has an unreadable record: {record:?}", path.display()),
        )
    })
}

/// Parse [`write_identity_record`]'s format. A record without its final newline
/// is a partial one.
fn parse_identity_record(record: &str) -> Option<(u32, Option<u64>)> {
    let mut fields = record.strip_suffix('\n')?.split(' ');
    let pid = fields.next()?.parse().ok()?;
    let start = fields.next().map(str::parse).transpose().ok()?;
    fields.next().is_none().then_some((pid, start))
}

/// [`Hold::take`], pausing at each [`TakeStep`], so a test drives the exact code
/// a take ships.
#[cfg(test)]
pub(crate) fn take_with_hook(
    path: &Path,
    hook: &mut dyn FnMut(TakeStep, &Path),
) -> std::io::Result<Hold> {
    take_inner(path, hook)
}

/// Remove the hold temps in `dir` that a creator died leaving: a name of
/// [`HOLD_TEMP_SUFFIX`]'s shape, unheld, whose creator pid is not a live process.
/// Returns the paths removed. `recover --clean` (50.1-21) calls this for
/// `.devflow/` and the gates directory.
///
/// A temp is never a hold. Removing an unheld temp of a dead creator is safe
/// because the hold, if it was ever published, lives on under its final name,
/// which this function never touches. The pid only keeps the sweep off a creator
/// mid-take, so it can only keep a file: a reused pid keeps a leftover until that
/// process exits, and is never a wrong removal.
///
/// Anything else is kept, including a name that does not parse. An entry this
/// cannot judge is kept and warned about, never skipped silently.
///
/// # Errors
/// `dir` could not be read for a reason other than being absent.
pub fn remove_orphan_hold_temps(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut removed = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!("could not read an entry of {}: {error}", dir.display());
                continue;
            }
        };
        let file_name = entry.file_name();
        let Some(creator) = file_name.to_str().and_then(hold_temp_creator) else {
            continue;
        };
        let path = entry.path();
        if !creator_may_be_alive(creator) && remove_if_unheld(&path) {
            removed.push(path);
        }
    }
    Ok(removed)
}

/// The creator pid in a hold temp's name, `.{final_name}.{pid}.{seq}.hold-tmp`.
/// `None` for any other name, and for a pid outside `1..=i32::MAX`, which `kill`
/// would read as a process group.
fn hold_temp_creator(name: &str) -> Option<i32> {
    let mut fields = name.strip_suffix(HOLD_TEMP_SUFFIX)?.rsplitn(3, '.');
    fields.next()?.parse::<u64>().ok()?;
    let pid: i32 = fields.next()?.parse().ok()?;
    let final_name = fields.next()?.strip_prefix('.')?;
    (pid >= 1 && !final_name.is_empty()).then_some(pid)
}

/// Whether `pid` might still be running: anything but `ESRCH` counts.
fn creator_may_be_alive(pid: i32) -> bool {
    // SAFETY: signal 0 sends nothing, and `pid >= 1` addresses one process.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Remove the temp at `path` if a separate open can take its lock. The removal
/// is advisory and outside D-09: nothing depends on the temp being gone.
fn remove_if_unheld(path: &Path) -> bool {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => {
            tracing::warn!("keeping hold temp {}: {error}", path.display());
            return false;
        }
    };
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return false,
        Err(TryLockError::Error(error)) => {
            tracing::warn!("keeping hold temp {}: {error}", path.display());
            return false;
        }
    }
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!("keeping hold temp {}: {error}", path.display());
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_child_ran_exactly_one_passing_test, in_child_test, run_test_in_child,
        spawn_test_in_child,
    };
    use std::process::{Child, Command, Stdio};

    /// Run `body` in a child test process of its own, with no other test
    /// beside it. Another test's fork holds a duplicate of every descriptor
    /// here until its exec, so under `cargo test` a lock read right after its
    /// release could still read held; and these tests move descriptors around
    /// by number, which nothing else in the process must be doing.
    fn in_own_process(test_name: &str, body: impl FnOnce()) {
        if in_child_test(test_name) {
            body();
            return;
        }
        let path_dir = tempfile::tempdir().expect("create the child's PATH directory");
        let path = std::env::var_os("PATH").expect("PATH is set");
        let output = run_test_in_child(test_name, path_dir.path(), &[("PATH", path.as_os_str())]);
        assert_child_ran_exactly_one_passing_test(&output, test_name);
    }

    /// A child that is killed and reaped when this drops, on a failed
    /// assertion too, so no `sleep` outlives its test.
    struct ReapOnDrop(Child);

    impl Drop for ReapOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn sleep_command() -> Command {
        let mut command = Command::new("sleep");
        command
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    /// The D-17 holder: one claim at a time, its record names the claimer, and the file
    /// outlives every hold.
    #[test]
    fn a_phase_holder_records_its_identity_and_refuses_a_second_claim() {
        in_own_process(
            "liveness::tests::a_phase_holder_records_its_identity_and_refuses_a_second_claim",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let root = dir.path();
                let phase = PhaseId::new(7);
                let path = phase_holder_path(root, phase);
                assert_eq!(path, root.join(".devflow/.holder-07"));
                let me = std::process::id();
                let start = crate::agent::process_start_time(me).expect("own start time");

                let hold = claim_phase_holder(root, phase).expect("the first claim");
                assert!(
                    root.join(".devflow/.gitignore").exists(),
                    "the claim goes through ensure_devflow_dir"
                );
                assert_eq!(
                    live_holder(&path).expect("read the holder"),
                    Some((me, Some(start)))
                );

                let second = claim_phase_holder(root, phase);
                assert!(
                    matches!(
                        second,
                        Err(HolderError::Held { holder: Some((pid, Some(recorded))) })
                            if pid == me && recorded == start
                    ),
                    "a second open is refused, naming the holder: {second:?}"
                );

                drop(hold);
                assert_eq!(live_holder(&path).expect("read the holder"), None);
                assert!(path.exists(), "the file is never removed");

                std::fs::write(&path, "junk that is no record").expect("spoil the record");
                let again = claim_phase_holder(root, phase).expect("a claim after the release");
                assert_eq!(
                    live_holder(&path).expect("read the holder"),
                    Some((me, Some(start))),
                    "a new claim rewrites the record"
                );
                drop(again);
            },
        );
    }

    /// D-07: the agent a runner spawns must not keep the runner lock, or the run would
    /// read live after its runner died.
    #[test]
    fn a_runner_lock_is_not_inherited_by_a_spawned_child() {
        in_own_process(
            "liveness::tests::a_runner_lock_is_not_inherited_by_a_spawned_child",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let root = dir.path();
                let phase = PhaseId::new(9);
                let path = runner_lock_path(root, phase);
                assert_eq!(path, root.join(".devflow/.runner-09"));

                let hold = claim_runner_lock(root, phase).expect("claim");
                let child = ReapOnDrop(sleep_command().spawn().expect("spawn the child"));
                drop(hold);
                assert_eq!(
                    probe(&path).expect("probe"),
                    Held::Abandoned,
                    "child kept the runner lock after the runner dropped it"
                );
                drop(child);

                let hold = claim_runner_lock(root, phase).expect("claim again");
                clear_close_on_exec(hold.as_raw_fd());
                let child = ReapOnDrop(sleep_command().spawn().expect("spawn the child"));
                drop(hold);
                assert_eq!(
                    probe(&path).expect("probe"),
                    Held::Live,
                    "control: with close-on-exec cleared the child keeps the lock"
                );
                drop(child);
            },
        );
    }

    /// D-07: a probe that cannot read the runner lock is an error, never a free lock, so
    /// no caller relaunches over a runner it could not see.
    #[test]
    fn a_runner_lock_probe_error_reads_as_an_error_not_free() {
        in_own_process(
            "liveness::tests::a_runner_lock_probe_error_reads_as_an_error_not_free",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let root = dir.path();
                let phase = PhaseId::new(10);
                assert_eq!(
                    probe(&runner_lock_path(root, phase)).expect("probe a missing lock"),
                    Held::Missing,
                    "control: a missing lock file reads Missing"
                );

                std::fs::write(root.join(".devflow"), "a regular file").expect("plant the file");
                let probed = probe(&runner_lock_path(root, phase));
                assert!(
                    probed.is_err(),
                    "probe error read as free: a lock under a regular file read {probed:?}"
                );
            },
        );
    }

    /// Clears `FD_CLOEXEC` on `fd`, so a spawned child inherits it.
    fn clear_close_on_exec(fd: RawFd) {
        // SAFETY: fcntl reads and sets a flag of a descriptor the caller owns.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            assert_ne!(flags, -1, "read the descriptor's flags");
            assert_ne!(
                libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC),
                -1,
                "clear close-on-exec"
            );
        }
    }

    /// A spawned supervisor or agent must not keep the holder alive: the holder would
    /// read held forever and `stop` would keep refusing.
    #[test]
    fn a_phase_holder_is_not_inherited_by_a_spawned_child() {
        in_own_process(
            "liveness::tests::a_phase_holder_is_not_inherited_by_a_spawned_child",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let root = dir.path();
                let phase = PhaseId::new(8);
                let path = phase_holder_path(root, phase);

                let hold = claim_phase_holder(root, phase).expect("claim");
                let child = ReapOnDrop(sleep_command().spawn().expect("spawn the child"));
                drop(hold);
                assert_eq!(
                    probe(&path).expect("probe"),
                    Held::Abandoned,
                    "a close-on-exec holder ends with the claimer, whatever it spawned"
                );
                drop(child);

                let hold = claim_phase_holder(root, phase).expect("claim again");
                clear_close_on_exec(hold.as_raw_fd());
                let child = ReapOnDrop(sleep_command().spawn().expect("spawn the child"));
                drop(hold);
                assert_eq!(
                    probe(&path).expect("probe"),
                    Held::Live,
                    "control: with close-on-exec cleared the child keeps the lock"
                );
                drop(child);
            },
        );
    }

    /// Which child [`the_holder_stop_handler_turns_sigterm_and_sigint_into_a_flag`] runs.
    /// Not a DevFlow setting, so it carries no `DEVFLOW_` prefix.
    #[cfg(target_os = "linux")]
    const HANDLER_CASE_ENV: &str = "P50_1_29_HANDLER_CASE";
    /// The file that child writes once it is ready for its signal.
    #[cfg(target_os = "linux")]
    const HANDLER_READY_ENV: &str = "P50_1_29_HANDLER_READY";
    /// How long a handler-test child waits for its signal, and the parent for that child.
    #[cfg(target_os = "linux")]
    const HANDLER_CHILD_WAIT: Duration = Duration::from_secs(10);

    /// The named mask field of this process's `/proc` status. Panics when the field is
    /// absent, so an unreadable mask never reads as clear.
    #[cfg(target_os = "linux")]
    fn own_signal_mask(field: &str) -> u64 {
        // This thread's, not `/proc/self`'s: that shows the main thread, which blocks
        // nearly every signal for a moment while libtest spawns this test's thread.
        let status = std::fs::read_to_string("/proc/thread-self/status").expect("read own status");
        let hex = status
            .lines()
            .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
            .unwrap_or_else(|| panic!("no {field} line in /proc/thread-self/status"))
            .trim();
        u64::from_str_radix(hex, 16).expect("signal masks are hex")
    }

    /// A handler-test child's body. It never returns to libtest: its exit code is the
    /// signal number its flag read, or 0 if none arrived.
    #[cfg(target_os = "linux")]
    fn run_handler_child(install: bool) -> ! {
        if install {
            install_holder_stop_handler().expect("install the handler");
            let bits = (1u64 << (libc::SIGTERM - 1)) | (1u64 << (libc::SIGINT - 1));
            assert_eq!(own_signal_mask("SigCgt") & bits, bits, "both are caught");
            assert_eq!(own_signal_mask("SigBlk") & bits, 0, "neither is blocked");
        }
        let ready = std::env::var_os(HANDLER_READY_ENV).expect("the parent names the ready file");
        std::fs::write(ready, "").expect("write the ready file");
        let deadline = Instant::now() + HANDLER_CHILD_WAIT;
        while Instant::now() < deadline {
            if let Some(signal) = holder_stop_signal() {
                std::process::exit(signal);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::process::exit(0);
    }

    /// One handler-test child to its end: spawn it, signal it once it is ready, and
    /// return how it ended, reaped.
    #[cfg(target_os = "linux")]
    fn signalled_handler_child(
        test_name: &str,
        case: &str,
        signal: libc::c_int,
    ) -> std::process::ExitStatus {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let ready = dir.path().join("ready");
        let path = std::env::var_os("PATH").expect("PATH is set");
        let mut child = spawn_test_in_child(
            test_name,
            dir.path(),
            &[
                ("PATH", path.as_os_str()),
                (HANDLER_CASE_ENV, std::ffi::OsStr::new(case)),
                (HANDLER_READY_ENV, ready.as_os_str()),
            ],
        );
        let deadline = Instant::now() + HANDLER_CHILD_WAIT;
        while !ready.exists() {
            if Instant::now() > deadline || child.try_wait().expect("poll the child").is_some() {
                let _ = child.kill();
                let output = child.wait_with_output().expect("reap the child");
                panic!("{case}: the child never got ready: {output:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid = i32::try_from(child.id()).expect("a pid fits");
        // SAFETY: `pid` is this test's own unreaped child.
        assert_eq!(unsafe { libc::kill(pid, signal) }, 0, "signal the child");
        let output = child.wait_with_output().expect("reap the child");
        eprintln!("{case}: {:?}", output.status);
        output.status
    }

    /// D-18: a holder turns SIGTERM and SIGINT into a flag. The control installs nothing
    /// and dies, so the survival is the handler's doing.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_holder_stop_handler_turns_sigterm_and_sigint_into_a_flag() {
        use std::os::unix::process::ExitStatusExt;
        const NAME: &str =
            "liveness::tests::the_holder_stop_handler_turns_sigterm_and_sigint_into_a_flag";
        if in_child_test(NAME) {
            let case = std::env::var(HANDLER_CASE_ENV).expect("the parent names the case");
            run_handler_child(case != "no-handler");
        }
        let term = signalled_handler_child(NAME, "sigterm", libc::SIGTERM);
        assert_eq!(term.code(), Some(libc::SIGTERM), "{term:?}");
        let int = signalled_handler_child(NAME, "sigint", libc::SIGINT);
        assert_eq!(int.code(), Some(libc::SIGINT), "{int:?}");
        let control = signalled_handler_child(NAME, "no-handler", libc::SIGTERM);
        assert_eq!(
            control.signal(),
            Some(libc::SIGTERM),
            "control: with no handler the signal ends the child: {control:?}"
        );
    }

    /// This process's identity as a hold records it.
    fn this_process() -> (u32, Option<u64>) {
        let pid = std::process::id();
        let start = crate::agent::process_start_time(pid);
        assert!(start.is_some(), "this test needs /proc/<pid>/stat");
        (pid, start)
    }

    #[test]
    fn an_unheld_reservation_reads_abandoned() {
        in_own_process(
            "liveness::tests::an_unheld_reservation_reads_abandoned",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let path = dir.path().join("r.live");
                assert_eq!(probe(&path).expect("probe"), Held::Missing);

                let hold = Hold::take(&path).expect("take a hold");
                assert_eq!(probe(&path).expect("probe"), Held::Live);

                drop(hold);
                assert_eq!(probe(&path).expect("probe"), Held::Abandoned);
            },
        );
    }

    /// The part of `source` before its test module.
    fn production_part(source: &str) -> &str {
        source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or(source)
    }

    /// The non-test, non-comment lines of `source` containing `needle`.
    fn production_lines_containing(source: &str, needle: &str) -> Vec<String> {
        production_part(source)
            .lines()
            .filter(|line| !line.trim_start().starts_with("//") && line.contains(needle))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn no_code_unlocks_a_liveness_hold() {
        let planted = production_lines_containing("    hold.file.unlock()?;\n", ".unlock(");
        assert_eq!(planted, ["    hold.file.unlock()?;"]);
        let comment = production_lines_containing("    // never call .unlock() here\n", ".unlock(");
        assert_eq!(comment, Vec::<String>::new());
        let in_tests = production_lines_containing(
            "#[cfg(test)]\nmod tests {\n  a.unlock();\n}\n",
            ".unlock(",
        );
        assert_eq!(in_tests, Vec::<String>::new());

        for source in [include_str!("liveness.rs"), include_str!("monitor.rs")] {
            assert!(
                production_part(source).contains("pub fn "),
                "the scan must have read production code"
            );
            assert_eq!(
                production_lines_containing(source, ".unlock("),
                Vec::<String>::new(),
                "unlock on any duplicate releases the lock a child inherited"
            );
        }
    }

    #[test]
    fn no_code_writes_a_hold_record_through_the_cursor() {
        let planted = production_lines_containing("    file.write_all(record)?;\n", ".write_all(");
        assert_eq!(planted, ["    file.write_all(record)?;"]);
        let positional =
            production_lines_containing("    file.write_all_at(record, 0)?;\n", ".write_all(");
        assert_eq!(positional, Vec::<String>::new());

        let source = include_str!("liveness.rs");
        assert!(production_part(source).contains(".write_all_at("));
        assert_eq!(
            production_lines_containing(source, ".write_all("),
            Vec::<String>::new()
        );
    }

    #[test]
    fn launch_liveness_path_refuses_an_invalid_launch_id() {
        let root = Path::new("/project/root");
        assert_eq!(launch_liveness_path(root, "../x"), None);
        let launch_id = new_launch_id();
        assert_eq!(
            launch_liveness_path(root, &launch_id),
            Some(root.join(format!(".devflow/supervisor-{launch_id}.live")))
        );
    }

    #[test]
    fn a_probe_in_flight_neither_reads_live_nor_fails_a_take() {
        in_own_process(
            "liveness::tests::a_probe_in_flight_neither_reads_live_nor_fails_a_take",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let path = dir.path().join("x.live");
                std::fs::write(&path, b"").expect("create the file");

                let in_flight = File::open(&path).expect("open");
                in_flight.try_lock_shared().expect("take the shared lock");
                assert_eq!(
                    probe(&path).expect("probe"),
                    Held::Abandoned,
                    "a probe must not read another probe as live"
                );
                drop(in_flight);

                // The first try always meets the shared lock, so an implementation
                // that does not wait fails every attempt. Only a stalled releaser
                // thread on a loaded machine can fail one that does, hence the retries.
                let waited_out = (0..5).any(|_| {
                    let in_flight = File::open(&path).expect("open");
                    in_flight.try_lock_shared().expect("take the shared lock");
                    let releaser = std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(10));
                        drop(in_flight);
                    });
                    let taker = File::open(&path).expect("open");
                    let taken = try_lock_past_probes(&taker);
                    releaser.join().expect("join the releaser");
                    taken.is_ok()
                });
                assert!(waited_out, "a take must wait out a probe's momentary lock");

                let holder = File::open(&path).expect("open");
                holder.try_lock().expect("take the exclusive lock");
                assert_eq!(probe(&path).expect("probe"), Held::Live);
                let second_taker = File::open(&path).expect("open");
                let started = Instant::now();
                let second = try_lock_past_probes(&second_taker);
                assert!(
                    matches!(second, Err(TryLockError::WouldBlock)),
                    "a real holder must refuse a second taker, got {second:?}"
                );
                assert!(started.elapsed() >= PROBE_RACE_WAIT);
            },
        );
    }

    #[test]
    fn live_holder_refuses_an_empty_or_partial_record() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let path = dir.path().join("x.live");
        let _hold = Hold::take(&path).expect("take a hold");
        assert_eq!(
            live_holder(&path).expect("the taken record reads"),
            Some(this_process())
        );

        for record in ["", "12", "not-a-pid 5\n", "12 not-a-start\n", "12 34 56\n"] {
            std::fs::write(&path, record).expect("rewrite the record");
            let refused = live_holder(&path);
            assert!(
                refused
                    .as_ref()
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::InvalidData),
                "record {record:?} must be refused, got {refused:?}"
            );
        }
    }

    /// The sweep a remover runs: orphaned temps, then every `*.live` that
    /// probes abandoned (the 50.1-21 and `close` rule).
    fn sweep(dir: &Path) {
        remove_orphan_hold_temps(dir).expect("sweep the temps");
        for entry in std::fs::read_dir(dir).expect("read the directory") {
            let path = entry.expect("read an entry").path();
            if path
                .extension()
                .is_some_and(|extension| extension == "live")
                && probe(&path).expect("probe") == Held::Abandoned
            {
                std::fs::remove_file(&path).expect("remove an abandoned hold");
            }
        }
    }

    fn hold_temps_in(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("read the directory")
            .map(|entry| {
                entry
                    .expect("read an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.ends_with(HOLD_TEMP_SUFFIX))
            .collect()
    }

    /// The order `take_inner` replaced: the final name exists before its lock.
    fn take_create_then_lock(path: &Path, hook: &mut dyn FnMut(&Path)) -> Hold {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .expect("create the final name");
        hook(path);
        file.try_lock().expect("lock it");
        write_identity_record(&file).expect("write the record");
        Hold {
            file,
            path: path.to_path_buf(),
        }
    }

    #[test]
    fn a_hold_is_published_only_once_it_is_held() {
        in_own_process(
            "liveness::tests::a_hold_is_published_only_once_it_is_held",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let path = dir.path().join("r.live");
                let holder = this_process();
                let mut steps = Vec::new();
                let hold = take_with_hook(&path, &mut |step, temp| {
                    sweep(dir.path());
                    let published = step == TakeStep::Published;
                    assert_eq!(
                        probe(&path).expect("probe"),
                        if published { Held::Live } else { Held::Missing },
                        "the final name at {step:?}"
                    );
                    assert_eq!(
                        live_holder(&path).expect("read the holder"),
                        published.then_some(holder),
                        "the holder at {step:?}"
                    );
                    assert!(temp.exists(), "the sweep must keep the temp at {step:?}");
                    steps.push(step);
                })
                .expect("take a hold");
                assert_eq!(
                    steps,
                    [TakeStep::TempCreated, TakeStep::Locked, TakeStep::Published]
                );
                assert_eq!(probe(&path).expect("probe"), Held::Live);
                assert_eq!(hold_temps_in(dir.path()), Vec::<String>::new());

                drop(hold);
                assert_eq!(probe(&path).expect("probe"), Held::Abandoned);
                sweep(dir.path());
                assert!(!path.exists(), "an abandoned hold stays removable");

                let control = dir.path().join("c.live");
                let held = take_create_then_lock(&control, &mut |_| sweep(dir.path()));
                assert_eq!(
                    probe(&control).expect("probe"),
                    Held::Missing,
                    "create-then-lock: the sweep unlinked the name, so a held hold reads not live"
                );
                drop(held);
            },
        );
    }

    #[test]
    fn a_take_never_replaces_an_existing_name() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let path = dir.path().join("r.live");
        std::fs::write(&path, "someone else's\n").expect("plant the name");

        let refused = Hold::take(&path);
        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists),
            "got {refused:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read the name"),
            "someone else's\n"
        );
        assert_eq!(hold_temps_in(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn a_take_whose_temp_vanished_publishes_nothing() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let path = dir.path().join("r.live");
        let refused = take_with_hook(&path, &mut |step, temp| {
            if step == TakeStep::Locked {
                std::fs::remove_file(temp).expect("remove the temp under the take");
            }
        });
        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "got {refused:?}"
        );
        assert!(!path.exists(), "an unheld name must never be published");
    }

    #[test]
    fn a_hold_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        in_own_process("liveness::tests::a_hold_file_is_owner_only", || {
            // SAFETY: umask sets one process flag, and this process runs one
            // test. With no umask to narrow it, the mode is the code's own.
            unsafe { libc::umask(0) };
            let dir = tempfile::tempdir().expect("create a temp dir");
            let path = dir.path().join("r.live");
            let _hold = Hold::take(&path).expect("take a hold");
            let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        });
    }

    /// The pid of a process that has exited and been reaped.
    fn gone_pid() -> u32 {
        let mut child = Command::new("true").spawn().expect("spawn true");
        let pid = child.id();
        child.wait().expect("reap true");
        pid
    }

    #[test]
    fn orphaned_hold_temps_are_removed_only_when_unheld_and_their_creator_is_gone() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let gone = gone_pid();
        let this = std::process::id();
        let orphan = dir.path().join(format!(".a.live.{gone}.0.hold-tmp"));
        let held = dir.path().join(format!(".b.live.{gone}.1.hold-tmp"));
        let kept = [
            held.clone(),
            dir.path().join(format!(".c.live.{this}.2.hold-tmp")),
            dir.path().join("d.live"),
            dir.path().join(".e.live.notapid.3.hold-tmp"),
            dir.path().join(format!(".f.live.{gone}.4.tmp")),
            dir.path().join(".g.live.0.5.hold-tmp"),
            dir.path().join(".h.live.4294967295.6.hold-tmp"),
            dir.path().join(".i.live.1.7.hold-tmp"),
            dir.path().join(format!("j.live.{gone}.8.hold-tmp")),
            dir.path().join(format!("..{gone}.9.hold-tmp")),
        ];
        std::fs::write(&orphan, "").expect("plant the orphan");
        for path in &kept {
            std::fs::write(path, "").expect("plant a file");
        }
        let lock = File::open(&held).expect("open the held temp");
        lock.try_lock().expect("hold the temp");

        let removed = remove_orphan_hold_temps(dir.path()).expect("sweep");
        assert_eq!(removed, std::slice::from_ref(&orphan));
        assert!(!orphan.exists());
        for path in &kept {
            assert!(path.exists(), "{} must be kept", path.display());
        }

        let missing = remove_orphan_hold_temps(&dir.path().join("nope")).expect("a missing dir");
        assert_eq!(missing, Vec::<PathBuf>::new());
    }

    #[test]
    fn a_take_skips_a_temp_name_already_taken() {
        in_own_process(
            "liveness::tests::a_take_skips_a_temp_name_already_taken",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let path = dir.path().join("r.live");
                let pid = std::process::id();
                let next = HOLD_SEQ.load(Ordering::Relaxed);
                let taken = dir
                    .path()
                    .join(format!(".r.live.{pid}.{next}{HOLD_TEMP_SUFFIX}"));
                std::fs::write(&taken, "").expect("plant the next temp name");

                let _hold = Hold::take(&path).expect("take a hold past the collision");
                assert_eq!(probe(&path).expect("probe"), Held::Live);
                assert!(taken.exists(), "another temp is left alone");
            },
        );
    }

    #[test]
    fn a_take_in_a_missing_directory_is_an_error() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let refused = Hold::take(&dir.path().join("nope/r.live"));
        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "got {refused:?}"
        );
    }

    /// A probe that read an unopenable path as absent would let a second
    /// supervisor start beside a live one.
    #[test]
    fn a_probe_reports_an_error_other_than_absence() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let file = dir.path().join("file");
        std::fs::write(&file, "").expect("create a file");
        let error = probe(&file.join("x.live")).expect_err("a path under a file is no absence");
        assert_eq!(error.kind(), std::io::ErrorKind::NotADirectory);
    }

    #[test]
    fn live_holder_reports_a_record_it_cannot_read() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let lock = File::open(dir.path()).expect("open the directory");
        lock.try_lock().expect("lock the directory");
        assert_eq!(probe(dir.path()).expect("probe"), Held::Live);
        assert!(
            live_holder(dir.path()).is_err(),
            "a held file with an unreadable record must not read as unheld"
        );
    }

    #[test]
    fn an_unreadable_directory_is_an_error_not_an_empty_sweep() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let file = dir.path().join("file");
        std::fs::write(&file, "").expect("create a file");
        let error = remove_orphan_hold_temps(&file).expect_err("a file is not a directory");
        assert_eq!(error.kind(), std::io::ErrorKind::NotADirectory);
    }
}
