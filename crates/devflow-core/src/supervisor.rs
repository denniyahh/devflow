//! Addressable monitor liveness: a per-launch Unix socket that a supervisor
//! listens on, and the probe that turns a recorded socket path into exactly
//! one of [`Liveness::Gone`], [`Liveness::Stale`] or [`Liveness::Alive`].
//!
//! **Storage shape.** Each launch gets its own socket, and later a JSON
//! sidecar with the same stem, under `cache_dir/sock`
//! ([`registry::socket_dir_in`]): `<root digest:016x>-<phase padded>-<launch
//! id>.sock`. The directory is mode `0700` and each socket `0600`. The socket
//! path is persisted in state when the launch is recorded and is never
//! re-derived at probe time, so a probe only ever asks about a path somebody
//! wrote down.
//!
//! **Publication.** A socket is bound and listening under a temporary
//! `<stem>.part` name first, then hard-linked to its final `<stem>.sock`
//! name. The final name therefore never exists before the listener is
//! listening, and a hard link onto an existing name fails instead of
//! replacing a live socket, as `rename` would.
//!
//! **Liveness reads no pid.** [`probe`] decides from the file at the path, a
//! connect, and one ping round trip. Only a same-uid socket whose connect is
//! refused is `Stale`; any other surprise at the path is a [`ProbeError`], so
//! a stray file can never be mistaken for a dead supervisor (R-6).

pub mod protocol;

use crate::phase_id::PhaseId;
use crate::registry;
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// The longest socket path Linux accepts: `sun_path` is 108 bytes including
/// its terminating NUL.
pub const SUN_PATH_MAX_LINUX: usize = 107;

/// The longest socket path macOS accepts: `sun_path` is 104 bytes including
/// its terminating NUL. Unverified on a macOS host.
pub const SUN_PATH_MAX_MACOS: usize = 103;

/// The socket path limit for the target this was compiled for.
#[cfg(target_os = "macos")]
pub const SUN_PATH_MAX: usize = SUN_PATH_MAX_MACOS;

/// The socket path limit for the target this was compiled for.
#[cfg(not(target_os = "macos"))]
pub const SUN_PATH_MAX: usize = SUN_PATH_MAX_LINUX;

/// A socket path DevFlow refuses to build.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SocketPathError {
    /// The path, or its temporary publish name, is longer than the
    /// `sun_path` limit; binding it would fail or truncate.
    #[error("socket path is {len} bytes, longer than the {max}-byte sun_path limit")]
    TooLong {
        /// The offending path's length in bytes.
        len: usize,
        /// The limit it exceeds.
        max: usize,
    },
}

/// A supervisor socket path whose final name and temporary publish name both
/// fit `sun_path`. Callers pass a path ending in `.sock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketPath(PathBuf);

impl SocketPath {
    /// Accept `path` when both it and its [`part_path`](Self::part_path) fit
    /// [`SUN_PATH_MAX`] bytes; refuse it by name otherwise, before anything
    /// is bound.
    pub fn new(path: PathBuf) -> Result<Self, SocketPathError> {
        let socket = Self(path);
        for candidate in [socket.0.clone(), socket.part_path()] {
            let len = candidate.as_os_str().len();
            if len > SUN_PATH_MAX {
                return Err(SocketPathError::TooLong {
                    len,
                    max: SUN_PATH_MAX,
                });
            }
        }
        Ok(socket)
    }

    /// The socket path for one launch:
    /// `socket_dir_in(cache_dir)/<path_digest(project_root):016x>-<phase padded>-<launch_id>.sock`.
    pub fn for_launch(
        cache_dir: &Path,
        project_root: &Path,
        phase: PhaseId,
        launch_id: &str,
    ) -> Result<Self, SocketPathError> {
        let digest = registry::path_digest(project_root);
        Self::new(registry::socket_dir_in(cache_dir).join(format!(
            "{digest:016x}-{padded}-{launch_id}.sock",
            padded = phase.padded()
        )))
    }

    /// The final socket path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// The sidecar beside the socket: same stem, `.json`.
    pub fn sidecar_path(&self) -> PathBuf {
        self.0.with_extension("json")
    }

    /// The temporary publish name: same stem, `.part`, in the same directory.
    /// It is exactly as long as the final `.sock` name, so the length limit
    /// is the same for both, and a sweep that filters on `.sock` never sees a
    /// half-published socket.
    pub fn part_path(&self) -> PathBuf {
        self.0.with_extension("part")
    }
}

/// A fresh launch id: 16 lowercase hex characters from this process's pid,
/// the current time in nanoseconds and a process-wide counter. Uniqueness is
/// also enforced at publish time, which refuses an existing name.
pub fn new_launch_id() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64);
    let mixed = u64::from(std::process::id()) ^ nanos ^ count.rotate_left(48);
    format!("{mixed:016x}")
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

/// Why a socket could not be published.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PublishError {
    /// Something already exists at the final name; it was left untouched.
    #[error("a socket already exists at {0}")]
    AlreadyExists(PathBuf),
    /// The socket directory is a symlink, not a directory, or owned by
    /// another uid.
    #[error("unsafe socket directory: {0}")]
    UnsafeDir(String),
    /// Any other filesystem or socket failure.
    #[error("socket publish failed: {0}")]
    Io(#[from] io::Error),
}

/// Publish a listening socket at `path`: bind and listen under the temporary
/// name, restrict it to the owner, then hard-link the final name. The final
/// name never exists before the listener is listening, and an existing final
/// name is never unlinked or replaced.
pub fn publish(path: &SocketPath) -> Result<UnixListener, PublishError> {
    publish_with(path, || {})
}

/// [`publish`] with a seam that runs after the socket is listening and
/// before the final name is linked, so tests can hold that window open.
pub(crate) fn publish_with(
    path: &SocketPath,
    before_link: impl FnOnce(),
) -> Result<UnixListener, PublishError> {
    let dir = path
        .as_path()
        .parent()
        .ok_or_else(|| PublishError::UnsafeDir("socket path has no parent directory".into()))?;
    ensure_socket_dir(dir)?;
    let part = path.part_path();
    let listener = UnixListener::bind(&part)?;
    let linked =
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o600)).and_then(|()| {
            before_link();
            std::fs::hard_link(&part, path.as_path())
        });
    let cleanup = std::fs::remove_file(&part);
    match linked {
        Ok(()) => {
            if let Err(error) = cleanup {
                tracing::warn!(
                    "published supervisor socket at {} but could not remove {}: {error}",
                    path.as_path().display(),
                    part.display()
                );
            }
            Ok(listener)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(PublishError::AlreadyExists(path.as_path().to_path_buf()))
        }
        Err(error) => Err(PublishError::Io(error)),
    }
}

/// Create the socket directory with mode `0700` if absent, then refuse it
/// unless it is a real directory owned by this uid, and reset its mode.
fn ensure_socket_dir(dir: &Path) -> Result<(), PublishError> {
    match std::fs::symlink_metadata(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match std::fs::DirBuilder::new().mode(0o700).create(dir) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let meta = std::fs::symlink_metadata(dir)?;
    if meta.file_type().is_symlink() {
        return Err(PublishError::UnsafeDir(format!(
            "{} is a symlink",
            dir.display()
        )));
    }
    if !meta.is_dir() {
        return Err(PublishError::UnsafeDir(format!(
            "{} is not a directory",
            dir.display()
        )));
    }
    if meta.uid() != protocol::euid() {
        return Err(PublishError::UnsafeDir(format!(
            "{} is owned by uid {}, not this user",
            dir.display(),
            meta.uid()
        )));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// What a recorded socket path says about its supervisor. Deliberately
/// exhaustive: these three states are the contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Liveness {
    /// No file exists at the path.
    Gone,
    /// Our own socket is there, but its connect is refused: nothing listens.
    Stale,
    /// A connect followed by a valid ping reply.
    Alive(protocol::PingReply),
}

/// Why a probe could not answer Gone, Stale or Alive.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProbeError {
    /// The path could not be examined.
    #[error("could not stat the socket path: {0}")]
    Stat(#[source] io::Error),
    /// The path holds something other than a socket (a file, a directory, a
    /// symlink).
    #[error("{} is not a socket", .0.display())]
    NotASocket(PathBuf),
    /// The socket belongs to another uid.
    #[error("the socket is owned by uid {uid}, not this user")]
    ForeignOwner {
        /// The owning uid.
        uid: u32,
    },
    /// The connect failed with something other than a refusal, including a
    /// socket removed between the stat and the connect.
    #[error("could not connect to the socket: {0}")]
    Connect(#[source] io::Error),
    /// The connect succeeded but no valid ping reply arrived.
    #[error("the socket gave no valid ping reply: {0}")]
    NoReply(String),
}

/// Classify the socket recorded at `path`. Reads no pid.
///
/// 1. No file at the path (`lstat` NotFound) is `Gone`; any other stat
///    failure is [`ProbeError::Stat`].
/// 2. Anything but a socket (a symlink included) is
///    [`ProbeError::NotASocket`]; a socket of another uid is
///    [`ProbeError::ForeignOwner`]. Both are checked before connecting,
///    because a connect to a regular file or a directory is also refused.
/// 3. A refused connect is `Stale`. Every other connect error is
///    [`ProbeError::Connect`], NotFound included: a socket removed between
///    the stat and the connect is reported, never guessed to be `Gone`.
/// 4. A connect is not proof of life (a listen backlog accepts it with
///    nobody serving): only a valid ping reply makes it `Alive`.
pub fn probe(path: &Path) -> Result<Liveness, ProbeError> {
    let meta = match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Liveness::Gone),
        Err(error) => return Err(ProbeError::Stat(error)),
        Ok(meta) => meta,
    };
    if !meta.file_type().is_socket() {
        return Err(ProbeError::NotASocket(path.to_path_buf()));
    }
    if meta.uid() != protocol::euid() {
        return Err(ProbeError::ForeignOwner { uid: meta.uid() });
    }
    let stream = match UnixStream::connect(path) {
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            return Ok(Liveness::Stale);
        }
        Err(error) => return Err(ProbeError::Connect(error)),
        Ok(stream) => stream,
    };
    protocol::ping(stream)
        .map(Liveness::Alive)
        .map_err(ProbeError::NoReply)
}

#[cfg(test)]
mod tests {
    use super::protocol::{PingReply, Reply, Request};
    use super::*;
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};

    /// A short cache dir under `/tmp`, so socket paths stay far below
    /// `sun_path` however long `$TMPDIR` is.
    fn short_cache() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("dfs")
            .tempdir_in("/tmp")
            .expect("create a short cache dir under /tmp")
    }

    fn sample_reply(launch_id: &str) -> PingReply {
        PingReply {
            launch_id: launch_id.to_string(),
            phase: "50".to_string(),
            stage: "code".to_string(),
            launch: "stream".to_string(),
            owner: "advancing".to_string(),
            agent_pid: 1,
            agent_pgid: 1,
            agent_start_time: 0,
            boot_id: boot_id(),
            supervisor_pid: std::process::id(),
        }
    }

    /// Publish a fresh launch's socket under `cache`, returning its path, its
    /// launch id and the listening socket.
    fn published(cache: &Path) -> (SocketPath, String, UnixListener) {
        let launch_id = new_launch_id();
        let socket = SocketPath::for_launch(
            cache,
            Path::new("/project/root"),
            PhaseId::new(50),
            &launch_id,
        )
        .expect("socket path fits sun_path");
        let listener = publish(&socket).expect("publish the socket");
        (socket, launch_id, listener)
    }

    /// Answer one request on an accepted connection, as a supervisor would.
    fn answer_one(stream: &UnixStream, launch_id: &str) {
        assert!(
            protocol::peer_is_same_user(stream),
            "a same-user peer must pass the uid check"
        );
        let reply = match protocol::read_request(stream) {
            Ok(Request::Ping) => Reply::Ping(sample_reply(launch_id)),
            _ => Reply::Error("unknown_op".to_string()),
        };
        protocol::write_reply(stream, &reply).expect("write the reply");
    }

    /// Serve exactly `count` connections on `listener`, then hand it back.
    fn serve_pings(
        listener: UnixListener,
        launch_id: String,
        count: usize,
    ) -> JoinHandle<UnixListener> {
        thread::spawn(move || {
            for _ in 0..count {
                let (stream, _) = listener.accept().expect("accept a probe");
                answer_one(&stream, &launch_id);
            }
            listener
        })
    }

    fn expect_alive(path: &Path, launch_id: &str) {
        match probe(path) {
            Ok(Liveness::Alive(reply)) => assert_eq!(reply.launch_id, launch_id),
            other => panic!("expected Alive with launch id {launch_id}, got {other:?}"),
        }
    }

    #[test]
    fn probe_reports_alive_only_after_a_ping_reply() {
        let cache = short_cache();
        let (socket, launch_id, listener) = published(cache.path());
        let server = serve_pings(listener, launch_id.clone(), 1);
        expect_alive(socket.as_path(), &launch_id);
        drop(server.join().expect("ping server thread"));

        // Control: a listener that accepts and never replies is not Alive.
        let (silent, _, listener) = published(cache.path());
        let (release, released) = mpsc::channel::<()>();
        let holder = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept the probe");
            released.recv().expect("release signal");
            drop(stream);
        });
        let outcome = probe(silent.as_path());
        release.send(()).expect("release the holder");
        holder.join().expect("holder thread");
        assert!(
            matches!(outcome, Err(ProbeError::NoReply(_))),
            "a connect with no reply must be NoReply, got {outcome:?}"
        );
    }

    #[test]
    fn probe_reports_stale_only_for_a_socket_whose_listener_is_gone() {
        let cache = short_cache();
        let (socket, launch_id, listener) = published(cache.path());

        // Control: while the listener is held and answering, the path is Alive.
        let server = serve_pings(listener, launch_id.clone(), 1);
        expect_alive(socket.as_path(), &launch_id);
        drop(server.join().expect("ping server thread"));

        assert!(
            socket.as_path().exists(),
            "dropping the listener must leave the socket file"
        );
        let outcome = probe(socket.as_path());
        assert!(
            matches!(outcome, Ok(Liveness::Stale)),
            "a socket whose listener is gone must be Stale, got {outcome:?}"
        );
    }

    #[test]
    fn probe_reports_gone_only_when_no_socket_file_exists() {
        let cache = short_cache();
        let (socket, launch_id, listener) = published(cache.path());

        // Control: the same directory with a live listener at the path.
        let server = serve_pings(listener, launch_id.clone(), 1);
        expect_alive(socket.as_path(), &launch_id);
        drop(server.join().expect("ping server thread"));

        std::fs::remove_file(socket.as_path()).expect("remove the socket file");
        let outcome = probe(socket.as_path());
        assert!(
            matches!(outcome, Ok(Liveness::Gone)),
            "a path with no file must be Gone, got {outcome:?}"
        );
    }

    /// The V-04 test, re-invoked as a child process to hold a socket until it
    /// is SIGKILLed.
    const HOLDER_TEST: &str = "supervisor::tests::each_fixture_produces_exactly_one_liveness_state";
    /// The socket path the holder child publishes at.
    const HOLDER_SOCKET_ENV: &str = "DEVFLOW_TEST_HOLDER_SOCKET";

    /// What a probe answered, without the payload, so fixtures can be
    /// compared and counted.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Outcome {
        Gone,
        Stale,
        Alive,
        NotASocket,
        NoReply,
        OtherError,
    }

    fn outcome(result: &Result<Liveness, ProbeError>) -> Outcome {
        match result {
            Ok(Liveness::Gone) => Outcome::Gone,
            Ok(Liveness::Stale) => Outcome::Stale,
            Ok(Liveness::Alive(_)) => Outcome::Alive,
            Err(ProbeError::NotASocket(_)) => Outcome::NotASocket,
            Err(ProbeError::NoReply(_)) => Outcome::NoReply,
            Err(_) => Outcome::OtherError,
        }
    }

    /// The child half of the SIGKILLed-holder fixture: publish at the path the
    /// parent named and answer pings until killed.
    fn hold_a_published_socket_until_killed() -> ! {
        let path = std::env::var_os(HOLDER_SOCKET_ENV).expect("the parent names the socket path");
        let socket = SocketPath::new(PathBuf::from(path)).expect("holder socket path fits");
        let listener = publish(&socket).expect("holder publishes its socket");
        loop {
            let (stream, _) = listener.accept().expect("holder accepts a probe");
            answer_one(&stream, "sigkilled-holder");
        }
    }

    /// A socket whose holder was a separate process, SIGKILLed while
    /// listening, so nothing in it ran an orderly shutdown.
    fn sigkilled_holder_socket(cache: &Path) -> PathBuf {
        let socket = SocketPath::for_launch(
            cache,
            Path::new("/project/holder"),
            PhaseId::new(50),
            &new_launch_id(),
        )
        .expect("socket path fits sun_path");
        let mut child = crate::test_support::spawn_test_in_child(
            HOLDER_TEST,
            cache,
            &[(HOLDER_SOCKET_ENV, socket.as_path().as_os_str())],
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !matches!(probe(socket.as_path()), Ok(Liveness::Alive(_))) {
            let exited = child.try_wait().expect("poll the holder child").is_some();
            if exited || std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let output = child.wait_with_output().expect("collect holder output");
                panic!(
                    "the holder child never answered a ping (exited early: {exited}); stdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
        // `Child::kill` sends SIGKILL on Unix, to the pid this test spawned
        // and still owns (unreaped), never to a pid read from disk.
        child.kill().expect("SIGKILL the holder child");
        child.wait().expect("reap the holder child");
        socket.as_path().to_path_buf()
    }

    #[test]
    fn each_fixture_produces_exactly_one_liveness_state() {
        if crate::test_support::in_child_test(HOLDER_TEST) {
            hold_a_published_socket_until_killed();
        }
        let cache = short_cache();

        let no_file = cache.path().join("absent.sock");

        let (dropped, _, listener) = published(cache.path());
        drop(listener);

        let sigkilled = sigkilled_holder_socket(cache.path());

        let (answering, answering_id, listener) = published(cache.path());
        let server = serve_pings(listener, answering_id, 1);

        let regular = cache.path().join("regular.sock");
        std::fs::write(&regular, b"not a socket").expect("write a regular file");

        let (silent, _, silent_listener) = published(cache.path());

        let fixtures = [
            ("no file", no_file.as_path(), Outcome::Gone),
            ("dropped listener", dropped.as_path(), Outcome::Stale),
            ("SIGKILLed holder", sigkilled.as_path(), Outcome::Stale),
            (
                "ping-answering listener",
                answering.as_path(),
                Outcome::Alive,
            ),
            ("regular file", regular.as_path(), Outcome::NotASocket),
            (
                "never-replying listener",
                silent.as_path(),
                Outcome::NoReply,
            ),
        ];
        let mut positives = std::collections::HashSet::new();
        for (name, path, expected) in fixtures {
            let result = probe(path);
            assert_eq!(
                outcome(&result),
                expected,
                "fixture `{name}` must produce only {expected:?}, got {result:?}"
            );
            if matches!(expected, Outcome::Gone | Outcome::Stale | Outcome::Alive) {
                positives.insert(outcome(&result));
            }
        }
        drop(silent_listener);
        drop(server.join().expect("ping server thread"));
        assert_eq!(
            positives.len(),
            3,
            "the positive fixtures must yield three distinct answers, got {positives:?}"
        );
    }

    #[test]
    fn a_non_socket_or_unreadable_path_is_a_probe_error_not_stale() {
        use std::os::unix::fs::symlink;
        let cache = short_cache();

        let regular = cache.path().join("regular.sock");
        std::fs::write(&regular, b"not a socket").expect("write a regular file");
        let directory = cache.path().join("directory.sock");
        std::fs::create_dir(&directory).expect("create a directory");
        let (live, live_id, listener) = published(cache.path());
        let server = serve_pings(listener, live_id.clone(), 1);
        expect_alive(live.as_path(), &live_id);
        let link = cache.path().join("link.sock");
        symlink(live.as_path(), &link).expect("symlink to the live socket");

        for (name, path) in [
            ("regular file", regular.as_path()),
            ("directory", directory.as_path()),
            ("symlink to a live socket", link.as_path()),
        ] {
            let result = probe(path);
            assert!(
                matches!(result, Err(ProbeError::NotASocket(_))),
                "{name} must be NotASocket, never Stale, got {result:?}"
            );
        }
        drop(server.join().expect("ping server thread"));

        // A path that cannot be examined (a component is a regular file, so
        // lstat fails ENOTDIR) is a Stat error, never Gone.
        let under_a_file = regular.join("child.sock");
        let result = probe(&under_a_file);
        assert!(
            matches!(result, Err(ProbeError::Stat(_))),
            "a path that cannot be stat'ed must be a Stat error, got {result:?}"
        );

        let (unreadable, _, listener) = published(cache.path());
        drop(listener);
        if protocol::euid() == 0 {
            // Root bypasses permission bits, so a mode-000 socket is no
            // obstacle to its connect and this row cannot be built (999.113).
            eprintln!(
                "skipping the mode-000 row: running as root, which bypasses the 0o000 fixture"
            );
        } else {
            std::fs::set_permissions(unreadable.as_path(), std::fs::Permissions::from_mode(0o000))
                .expect("chmod the socket 000");
            let result = probe(unreadable.as_path());
            assert!(
                matches!(&result, Err(ProbeError::Connect(error)) if error.kind() == io::ErrorKind::PermissionDenied),
                "an unreadable socket must be a Connect error, never Stale, got {result:?}"
            );
        }

        // Control: the same socket file, readable again, is a real stale socket.
        std::fs::set_permissions(unreadable.as_path(), std::fs::Permissions::from_mode(0o600))
            .expect("chmod the socket back to 600");
        let result = probe(unreadable.as_path());
        assert!(
            matches!(result, Ok(Liveness::Stale)),
            "a readable socket with no listener must be Stale, got {result:?}"
        );
    }

    /// A `.sock` path in `cache`'s socket directory, padded to exactly `len`
    /// bytes, so the boundary does not depend on the tempdir's length.
    #[cfg(target_os = "linux")]
    fn sock_path_of_len(cache: &Path, len: usize) -> PathBuf {
        let dir = registry::socket_dir_in(cache);
        let fixed = dir.as_os_str().len() + "/".len() + ".sock".len();
        let path = dir.join(format!("{}.sock", "a".repeat(len - fixed)));
        assert_eq!(path.as_os_str().len(), len, "fixture path length");
        path
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn socket_path_of_107_bytes_binds_and_108_is_refused_by_name() {
        assert_eq!(SUN_PATH_MAX_LINUX, 107);
        assert_eq!(
            SUN_PATH_MAX_MACOS, 103,
            "the macOS limit is a constant only"
        );
        assert_eq!(SUN_PATH_MAX, SUN_PATH_MAX_LINUX);
        let cache = short_cache();

        for len in [106, 107] {
            let socket = SocketPath::new(sock_path_of_len(cache.path(), len))
                .unwrap_or_else(|error| panic!("a {len}-byte path must be accepted: {error}"));
            assert_eq!(
                socket.part_path().as_os_str().len(),
                len,
                "the temporary name must be exactly as long as the final name"
            );
            let listener = publish(&socket)
                .unwrap_or_else(|error| panic!("a {len}-byte path must publish: {error}"));
            let meta = std::fs::symlink_metadata(socket.as_path()).expect("published path");
            assert!(
                meta.file_type().is_socket(),
                "{len} bytes: a socket is published"
            );
            drop(listener);
        }

        let too_long = sock_path_of_len(cache.path(), 108);
        assert_eq!(
            SocketPath::new(too_long.clone()),
            Err(SocketPathError::TooLong { len: 108, max: 107 })
        );
        assert!(
            !too_long.exists() && !too_long.with_extension("part").exists(),
            "a refused path must be refused before anything is bound"
        );

        // `new` checks the temporary name too: a final path with no `.sock`
        // extension fits, but its `.part` name is five bytes longer.
        let bare = sock_path_of_len(cache.path(), 112).with_extension("");
        assert_eq!(bare.as_os_str().len(), 107, "fits as a final path");
        assert_eq!(
            SocketPath::new(bare),
            Err(SocketPathError::TooLong { len: 112, max: 107 })
        );
    }

    #[test]
    fn publish_never_exposes_an_unlistening_socket_or_replaces_a_live_one() {
        let cache = short_cache();
        let launch_id = new_launch_id();
        let socket = SocketPath::for_launch(
            cache.path(),
            Path::new("/project/root"),
            PhaseId::new(50),
            &launch_id,
        )
        .expect("socket path fits sun_path");

        let in_window = std::sync::Arc::new(std::sync::Barrier::new(2));
        let leave_window = std::sync::Arc::new(std::sync::Barrier::new(2));
        let publisher = {
            let socket = socket.clone();
            let in_window = in_window.clone();
            let leave_window = leave_window.clone();
            thread::spawn(move || {
                publish_with(&socket, || {
                    in_window.wait();
                    leave_window.wait();
                })
            })
        };
        in_window.wait();
        assert!(
            std::fs::symlink_metadata(socket.part_path()).is_ok(),
            "the temporary name is bound and listening inside the window"
        );
        let during = probe(socket.as_path());
        leave_window.wait();
        assert!(
            matches!(during, Ok(Liveness::Gone)),
            "the final name must not exist before the listener is listening, got {during:?}"
        );
        let listener = publisher
            .join()
            .expect("publisher thread")
            .expect("first publish");
        assert!(
            !socket.part_path().exists(),
            "the temporary name is removed"
        );

        let server = serve_pings(listener, launch_id.clone(), 1);
        let second = publish(&socket);
        assert!(
            matches!(&second, Err(PublishError::AlreadyExists(path)) if path == socket.as_path()),
            "a second publish onto a live name must fail, got {second:?}"
        );
        assert!(
            !socket.part_path().exists(),
            "the refused publish removes its temporary name"
        );
        expect_alive(socket.as_path(), &launch_id);
        drop(server.join().expect("ping server thread"));

        // Control: a socket bound with no listen() reads Stale, which is the
        // hazard binding under the temporary name avoids.
        let raw = registry::socket_dir_in(cache.path()).join("bound-not-listening.sock");
        let bytes = raw.as_os_str().as_encoded_bytes();
        // SAFETY: plain socket(2) call; the result is checked before use.
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        assert!(fd >= 0, "socket(2) failed: {}", io::Error::last_os_error());
        // SAFETY: fd is a freshly created, owned descriptor.
        let _owned = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) };
        // SAFETY: sockaddr_un is plain old data; all-zero is a valid value.
        let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        assert!(bytes.len() < addr.sun_path.len(), "raw path fits sun_path");
        for (slot, byte) in addr.sun_path.iter_mut().zip(bytes) {
            *slot = *byte as libc::c_char;
        }
        // SAFETY: addr is a valid, NUL-terminated sockaddr_un of the given size.
        let rc = unsafe {
            libc::bind(
                fd,
                (&raw const addr).cast(),
                std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
            )
        };
        assert_eq!(rc, 0, "bind(2) failed: {}", io::Error::last_os_error());
        let result = probe(&raw);
        assert!(
            matches!(result, Ok(Liveness::Stale)),
            "a bound socket with no listener must read Stale, got {result:?}"
        );
    }

    #[test]
    fn the_socket_dir_is_private_and_the_socket_is_owner_only() {
        let mode = |path: &Path| {
            std::fs::symlink_metadata(path)
                .expect("stat")
                .permissions()
                .mode()
                & 0o777
        };
        let cache = short_cache();
        let (socket, _, _listener) = published(cache.path());
        assert_eq!(mode(&registry::socket_dir_in(cache.path())), 0o700);
        assert_eq!(mode(socket.as_path()), 0o600);

        // An existing, looser socket directory is tightened, not trusted.
        let loose = short_cache();
        let loose_dir = registry::socket_dir_in(loose.path());
        std::fs::create_dir(&loose_dir).expect("create sock/");
        std::fs::set_permissions(&loose_dir, std::fs::Permissions::from_mode(0o755))
            .expect("chmod sock/ 755");
        let (_, _, _loose_listener) = published(loose.path());
        assert_eq!(mode(&loose_dir), 0o700);

        // A symlinked or non-directory sock/ is refused.
        let target = short_cache();
        let linked = short_cache();
        std::os::unix::fs::symlink(target.path(), registry::socket_dir_in(linked.path()))
            .expect("symlink sock/");
        let socket = SocketPath::for_launch(
            linked.path(),
            Path::new("/project/root"),
            PhaseId::new(50),
            &new_launch_id(),
        )
        .expect("socket path fits");
        let result = publish(&socket);
        assert!(
            matches!(&result, Err(PublishError::UnsafeDir(why)) if why.contains("is a symlink")),
            "a symlinked sock/ must be refused as a symlink, got {result:?}"
        );
        assert_eq!(
            std::fs::read_dir(target.path())
                .expect("read target")
                .count(),
            0,
            "nothing is created through the symlink"
        );

        let filed = short_cache();
        std::fs::write(registry::socket_dir_in(filed.path()), b"").expect("sock as a file");
        let socket = SocketPath::for_launch(
            filed.path(),
            Path::new("/project/root"),
            PhaseId::new(50),
            &new_launch_id(),
        )
        .expect("socket path fits");
        let result = publish(&socket);
        assert!(
            matches!(result, Err(PublishError::UnsafeDir(_))),
            "a sock/ that is not a directory must be refused, got {result:?}"
        );
    }
}
