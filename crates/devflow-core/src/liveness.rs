//! Per-launch liveness: a `flock` a launcher takes and a spawned supervisor
//! inherits and holds for life.

use std::fs::{File, OpenOptions, TryLockError};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// The descriptor number a hold has in the child [`pass_to_child`] spawns.
pub const INHERITED_FD: RawFd = 3;

/// An exclusive lock on a liveness file, released only by dropping it.
#[derive(Debug)]
pub struct Hold {
    file: File,
    path: PathBuf,
}

impl Hold {
    /// Take a hold on `path`.
    ///
    /// # Errors
    /// Any error creating or locking the file.
    pub fn take(path: &Path) -> std::io::Result<Hold> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.try_lock().map_err(std::io::Error::from)?;
        Ok(Hold {
            file,
            path: path.to_path_buf(),
        })
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

/// Read a liveness file's state from its lock alone.
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

/// Scaffold for the failing tests: passes nothing to the child.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "first caller: the launch path in 50.1-07")
)]
pub(crate) fn pass_to_child(_cmd: &mut std::process::Command, _hold: &Hold) {}

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
