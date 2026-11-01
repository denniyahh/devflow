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

use std::fs::{File, OpenOptions, TryLockError};
use std::os::fd::{AsRawFd, RawFd};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        assert_child_ran_exactly_one_passing_test, in_child_test, run_test_in_child,
    };
    use std::os::fd::FromRawFd;
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
}
