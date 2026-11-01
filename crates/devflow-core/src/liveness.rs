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
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The descriptor number a hold has in the child `pass_to_child` spawns.
pub const INHERITED_FD: RawFd = 3;

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

/// Have `cmd`'s child inherit `hold` at [`INHERITED_FD`], so it holds the lock
/// after this process drops its copy.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "first caller: the launch path in 50.1-07")
)]
pub(crate) fn pass_to_child(cmd: &mut std::process::Command, hold: &Hold) {
    let raw = hold.as_raw_fd();
    // SAFETY: the callback only calls `fcntl` and `dup2`, both
    // async-signal-safe, and reads errno. It allocates nothing and takes no
    // locks. When the descriptor already is fd 3, `dup2` would be a no-op that
    // leaves close-on-exec set, so that case clears the flag instead.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(cmd, move || {
            let result = if raw == INHERITED_FD {
                libc::fcntl(INHERITED_FD, libc::F_SETFD, 0)
            } else {
                libc::dup2(raw, INHERITED_FD)
            };
            if result == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// The liveness file of launch `launch_id`: `.devflow/supervisor-{launch_id}.live`.
/// `None` unless the id has [`crate::supervisor::new_launch_id`]'s format, so a
/// launch id read from disk never builds a path outside `.devflow/`.
pub fn launch_liveness_path(project_root: &Path, launch_id: &str) -> Option<PathBuf> {
    crate::supervisor::is_launch_id(launch_id).then(|| {
        project_root
            .join(".devflow")
            .join(format!("supervisor-{launch_id}.live"))
    })
}

/// The supervisor's side of `pass_to_child`: take ownership of the inherited
/// descriptor `fd` on `path`, mark it close-on-exec so an agent this process
/// spawns never inherits it, and record this process as the holder.
///
/// # Errors
/// `fd` is not an open descriptor, or the record could not be written.
pub fn adopt_inherited(fd: RawFd, path: PathBuf) -> std::io::Result<Hold> {
    // SAFETY: fcntl on a descriptor number reads and sets its flags and touches
    // no memory; a closed number fails with EBADF.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags == -1 || libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    // SAFETY: the descriptor is open (checked above) and was handed to this
    // process for exclusive ownership; nothing else here uses it.
    let file = unsafe { File::from_raw_fd(fd) };
    write_identity_record(&file)?;
    Ok(Hold { file, path })
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
    };
    use std::os::unix::process::CommandExt;
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

    /// Establish that fd 3 is free: anything the harness left there is moved
    /// aside, not closed, so it stays open until this process exits.
    fn free_fd_3() {
        // SAFETY: fcntl and close on a descriptor number; this process runs
        // one test (`in_own_process`), and the moved copy keeps the file open.
        unsafe {
            if libc::fcntl(INHERITED_FD, libc::F_GETFD) == -1 {
                eprintln!("fd 3 was free");
                return;
            }
            let moved = libc::fcntl(INHERITED_FD, libc::F_DUPFD_CLOEXEC, 64);
            assert_ne!(moved, -1, "move fd 3 aside");
            assert_eq!(libc::close(INHERITED_FD), 0, "close fd 3 after moving it");
            eprintln!("fd 3 was open; moved aside to {moved}");
        }
    }

    /// Put `file` at fd 3 with close-on-exec set. With fd 3 free, a file
    /// opened since may already be there; otherwise the copy lands there and
    /// the original closes, and any lock stays, since both refer to one open
    /// file description.
    fn place_at_fd_3(file: File) -> File {
        if file.as_raw_fd() == INHERITED_FD {
            return file;
        }
        // SAFETY: fcntl on a descriptor `file` owns; the result is a new
        // descriptor nothing else owns, checked to be fd 3 before it is wrapped.
        unsafe {
            let placed = libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, INHERITED_FD);
            assert_eq!(placed, INHERITED_FD, "fd 3 must have been free");
            File::from_raw_fd(placed)
        }
    }

    fn hold_at_fd_3(hold: Hold) -> Hold {
        let Hold { file, path } = hold;
        let hold = Hold {
            file: place_at_fd_3(file),
            path,
        };
        assert_eq!(hold.as_raw_fd(), INHERITED_FD);
        // SAFETY: F_GETFD reads a flag of a descriptor `hold` owns.
        let flags = unsafe { libc::fcntl(INHERITED_FD, libc::F_GETFD) };
        assert_ne!(
            flags & libc::FD_CLOEXEC,
            0,
            "the trap needs close-on-exec set on fd 3"
        );
        hold
    }

    /// The variant Pitfall 1 describes: `dup2` alone, which is a no-op when
    /// the descriptor already is fd 3 and leaves close-on-exec set.
    fn pass_to_child_naively(cmd: &mut Command, hold: &Hold) {
        let raw = hold.as_raw_fd();
        // SAFETY: only `dup2`, which is async-signal-safe.
        unsafe {
            cmd.pre_exec(move || {
                if libc::dup2(raw, INHERITED_FD) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    #[test]
    fn an_inherited_liveness_lock_survives_the_launcher_when_it_is_fd_3() {
        in_own_process(
            "liveness::tests::an_inherited_liveness_lock_survives_the_launcher_when_it_is_fd_3",
            || {
                free_fd_3();
                let dir = tempfile::tempdir().expect("create a temp dir");

                let held = dir.path().join("x.live");
                let hold = hold_at_fd_3(Hold::take(&held).expect("take a hold"));
                let mut command = sleep_command();
                pass_to_child(&mut command, &hold);
                let child = ReapOnDrop(command.spawn().expect("spawn the child"));
                drop(hold);
                assert_eq!(
                    probe(&held).expect("probe"),
                    Held::Live,
                    "the child at fd 3 must hold the lock after the parent's drop"
                );
                drop(child);
                assert_eq!(
                    probe(&held).expect("probe"),
                    Held::Abandoned,
                    "the lock must end with the child"
                );

                let naive = dir.path().join("naive.live");
                let hold = hold_at_fd_3(Hold::take(&naive).expect("take a hold"));
                let mut command = sleep_command();
                pass_to_child_naively(&mut command, &hold);
                let child = ReapOnDrop(command.spawn().expect("spawn the child"));
                drop(hold);
                assert_eq!(
                    probe(&naive).expect("probe"),
                    Held::Abandoned,
                    "dup2 alone must lose the lock at exec, or the raw == 3 branch keeps nothing"
                );
                drop(child);
            },
        );
    }

    #[test]
    fn an_inherited_liveness_lock_survives_the_launcher_when_it_is_not_fd_3() {
        in_own_process(
            "liveness::tests::an_inherited_liveness_lock_survives_the_launcher_when_it_is_not_fd_3",
            || {
                free_fd_3();
                let dir = tempfile::tempdir().expect("create a temp dir");
                let other = File::create(dir.path().join("other")).expect("create a file");
                let other = place_at_fd_3(other);
                assert_eq!(other.as_raw_fd(), INHERITED_FD);

                let held = dir.path().join("x.live");
                let hold = Hold::take(&held).expect("take a hold");
                assert_ne!(hold.as_raw_fd(), INHERITED_FD, "fd 3 is taken");
                let mut command = sleep_command();
                pass_to_child(&mut command, &hold);
                let child = ReapOnDrop(command.spawn().expect("spawn the child"));
                drop(hold);
                assert_eq!(
                    probe(&held).expect("probe"),
                    Held::Live,
                    "the dup2 branch must keep the lock in the child"
                );
                drop(child);
                assert_eq!(probe(&held).expect("probe"), Held::Abandoned);
            },
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

    #[test]
    fn unlock_on_a_duplicate_releases_the_inherited_lock() {
        in_own_process(
            "liveness::tests::unlock_on_a_duplicate_releases_the_inherited_lock",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let path = dir.path().join("x.live");
                let hold = Hold::take(&path).expect("take a hold");
                let mut command = sleep_command();
                pass_to_child(&mut command, &hold);
                let mut child = ReapOnDrop(command.spawn().expect("spawn the child"));
                assert_eq!(probe(&path).expect("probe"), Held::Live);

                hold.file.unlock().expect("unlock the parent's copy");
                assert_eq!(
                    probe(&path).expect("probe"),
                    Held::Abandoned,
                    "unlock on a duplicate must release the lock the child inherited"
                );
                assert!(
                    child.0.try_wait().expect("poll the child").is_none(),
                    "the child still runs, and no longer holds anything"
                );
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
    fn adopting_the_inherited_descriptor_marks_it_close_on_exec_and_records_the_adopter() {
        in_own_process(
            "liveness::tests::adopting_the_inherited_descriptor_marks_it_close_on_exec_and_records_the_adopter",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let path = dir.path().join("a.live");
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .expect("create the file");
                file.try_lock().expect("lock the file");
                // SAFETY: fcntl on a descriptor `file` owns; the result is owned
                // by the adoption below.
                let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 10) };
                assert_ne!(fd, -1);
                drop(file);
                // SAFETY: F_GETFD reads a flag of the descriptor just made.
                let before = unsafe { libc::fcntl(fd, libc::F_GETFD) };
                assert_eq!(
                    before & libc::FD_CLOEXEC,
                    0,
                    "the fixture hands over an inheritable fd"
                );

                let hold = adopt_inherited(fd, path.clone()).expect("adopt the descriptor");
                // SAFETY: F_GETFD reads a flag of a descriptor `hold` owns.
                let after = unsafe { libc::fcntl(fd, libc::F_GETFD) };
                assert_ne!(after & libc::FD_CLOEXEC, 0, "an agent must not inherit it");
                assert_eq!(
                    live_holder(&path).expect("read the holder"),
                    Some(this_process())
                );

                drop(hold);
                assert_eq!(live_holder(&path).expect("read the holder"), None);
                assert!(
                    adopt_inherited(fd, path).is_err(),
                    "a closed descriptor must be refused"
                );
            },
        );
    }

    #[test]
    fn launch_liveness_path_refuses_an_invalid_launch_id() {
        let root = Path::new("/project/root");
        assert_eq!(launch_liveness_path(root, "../x"), None);
        let launch_id = crate::supervisor::new_launch_id();
        assert_eq!(
            launch_liveness_path(root, &launch_id),
            Some(root.join(format!(".devflow/supervisor-{launch_id}.live")))
        );
    }

    #[test]
    fn a_probe_in_flight_neither_reads_live_nor_fails_a_take() {
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

    /// Take a hold on `path` and return a duplicate of its descriptor that is
    /// not close-on-exec and shares its open file description, with the shared
    /// offset past the record, where any cursor write leaves it. The hold's
    /// drop closes only its own copy, so the duplicate keeps the lock.
    fn duplicate_with_an_advanced_offset(path: &Path) -> (RawFd, usize) {
        let hold = Hold::take(path).expect("take a hold");
        // SAFETY: fcntl and lseek on a descriptor `hold` owns and on its new
        // duplicate, which the caller owns.
        unsafe {
            let duplicate = libc::fcntl(hold.as_raw_fd(), libc::F_DUPFD, 10);
            assert_ne!(duplicate, -1);
            let offset = libc::lseek(duplicate, 0, libc::SEEK_END);
            assert!(offset > 0, "the shared offset must have moved");
            (duplicate, usize::try_from(offset).expect("a small offset"))
        }
    }

    /// The rewrite `write_identity_record` replaced.
    fn rewrite_through_the_cursor(file: &File) {
        use std::io::Write;
        let (pid, start) = this_process();
        file.set_len(0).expect("truncate");
        let start = start.expect("a start time");
        let mut writer = file;
        writer
            .write_all(format!("{pid} {start}\n").as_bytes())
            .expect("write through the cursor");
    }

    #[test]
    fn adopting_a_descriptor_take_wrote_through_keeps_the_record_intact() {
        in_own_process(
            "liveness::tests::adopting_a_descriptor_take_wrote_through_keeps_the_record_intact",
            || {
                let dir = tempfile::tempdir().expect("create a temp dir");
                let (pid, start) = this_process();

                let adopted = dir.path().join("a.live");
                let (duplicate, _) = duplicate_with_an_advanced_offset(&adopted);
                let _hold = adopt_inherited(duplicate, adopted.clone()).expect("adopt");
                let bytes = std::fs::read(&adopted).expect("read the record");
                assert!(
                    !bytes.contains(&0),
                    "the record has a NUL hole: {bytes:?}; live_holder reads {:?}",
                    live_holder(&adopted)
                );
                assert!(bytes.starts_with(format!("{pid} ").as_bytes()));
                assert_eq!(
                    live_holder(&adopted).expect("read the holder"),
                    Some((pid, start))
                );

                let cursor = dir.path().join("b.live");
                let (duplicate, offset) = duplicate_with_an_advanced_offset(&cursor);
                // SAFETY: the caller owns the duplicate, and nothing else uses it.
                let file = unsafe { File::from_raw_fd(duplicate) };
                rewrite_through_the_cursor(&file);
                let bytes = std::fs::read(&cursor).expect("read the record");
                assert_eq!(
                    bytes.iter().take_while(|&&byte| byte == 0).count(),
                    offset,
                    "a cursor write lands past a hole as long as the offset: {bytes:?}"
                );
                let unreadable = live_holder(&cursor);
                assert!(
                    unreadable
                        .as_ref()
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::InvalidData),
                    "a live holder would go unnamed, got {unreadable:?}"
                );
            },
        );
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
        let dir = tempfile::tempdir().expect("create a temp dir");
        let path = dir.path().join("r.live");
        let _hold = Hold::take(&path).expect("take a hold");
        let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
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
}
