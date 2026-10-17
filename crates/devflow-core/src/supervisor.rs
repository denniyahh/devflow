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

pub mod backstop;
pub mod protocol;
pub mod runtime;

use crate::phase_id::PhaseId;
use crate::registry;
use serde::{Deserialize, Serialize};
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
    if !own_socket_at(path)? {
        return Ok(Liveness::Gone);
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

/// [`probe`]'s pre-connect checks, shared with [`request_shutdown`]:
/// `Ok(false)` when no file is at `path` (`lstat` NotFound), `Ok(true)` for a
/// socket owned by this uid, and a [`ProbeError`] for a stat failure, a
/// non-socket (a symlink included) or another uid's socket.
fn own_socket_at(path: &Path) -> Result<bool, ProbeError> {
    let meta = match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(ProbeError::Stat(error)),
        Ok(meta) => meta,
    };
    if !meta.file_type().is_socket() {
        return Err(ProbeError::NotASocket(path.to_path_buf()));
    }
    if meta.uid() != protocol::euid() {
        return Err(ProbeError::ForeignOwner { uid: meta.uid() });
    }
    Ok(true)
}

/// The JSON file beside a socket (same stem, `.json`, mode `0600`) that
/// records who the socket belongs to, so a sweep can answer by project root
/// and phase without loading any state file (R-7).
///
/// Its identity fields (`agent_pgid`, `agent_start_time`, `boot_id`) are only
/// an input to the stale-socket backstop's own validation; nothing signals a
/// process because a sidecar names it (T-50-13). Built only inside this
/// crate; other crates read it through [`read_sidecar`] and [`sweep_in`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Sidecar {
    /// The per-launch id, also the last segment of the socket's file name.
    pub launch_id: String,
    /// The project root the launch runs for.
    pub project_root: PathBuf,
    /// The phase the launch runs for.
    pub phase: PhaseId,
    /// The stage the supervised agent runs, when one is known.
    pub stage: Option<String>,
    /// The agent kind, as its label.
    pub agent: String,
    /// The launch shape: `capture` or `stream`.
    pub launch: String,
    /// The agent's pid.
    pub agent_pid: u32,
    /// The agent's process group id.
    pub agent_pgid: u32,
    /// The agent's start time in clock ticks since boot.
    pub agent_start_time: u64,
    /// The kernel boot id at launch.
    pub boot_id: String,
    /// The supervisor's pid.
    pub supervisor_pid: u32,
    /// The supervisor's start time in clock ticks since boot.
    pub supervisor_start_time: u64,
    /// When the sidecar was written, RFC 3339 in UTC
    /// (`YYYY-MM-DDTHH:MM:SSZ`).
    pub created_at: String,
}

/// Why a sidecar could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SidecarError {
    /// The file could not be read.
    #[error("could not read the sidecar: {0}")]
    Io(#[from] io::Error),
    /// The file is not a complete sidecar JSON object.
    #[error("the sidecar does not parse: {0}")]
    Json(#[from] serde_json::Error),
}

/// `unix_secs` as an RFC 3339 UTC timestamp, `YYYY-MM-DDTHH:MM:SSZ`.
pub(crate) fn rfc3339_from_unix(unix_secs: u64) -> String {
    const SECS_PER_MINUTE: u64 = 60;
    const SECS_PER_HOUR: u64 = 60 * SECS_PER_MINUTE;
    const SECS_PER_DAY: u64 = 24 * SECS_PER_HOUR;
    // u64::MAX / SECS_PER_DAY is below 2^48, so the day count fits i64.
    let days = i64::try_from(unix_secs / SECS_PER_DAY).unwrap_or(i64::MAX);
    let (year, month, day) = crate::ship::civil_from_days(days);
    let of_day = unix_secs % SECS_PER_DAY;
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z",
        hour = of_day / SECS_PER_HOUR,
        minute = of_day % SECS_PER_HOUR / SECS_PER_MINUTE,
        second = of_day % SECS_PER_MINUTE
    )
}

/// The current time as [`rfc3339_from_unix`] formats it, for
/// [`Sidecar::created_at`].
pub(crate) fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    rfc3339_from_unix(secs)
}

/// Write `sidecar` beside `socket` atomically: a uniquely named `.tmp` file
/// in the socket directory, set to mode `0600`, then renamed onto
/// [`SocketPath::sidecar_path`]. The sidecar is a regular file, so `rename`
/// is safe here; only sockets must never be renamed into place. The socket
/// directory is created `0700` if absent and refused if it is unsafe, as
/// [`publish`] does.
pub(crate) fn write_sidecar(socket: &SocketPath, sidecar: &Sidecar) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let target = socket.sidecar_path();
    let (Some(dir), Some(stem)) = (target.parent(), target.file_stem()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no directory or stem", target.display()),
        ));
    };
    ensure_socket_dir(dir).map_err(|error| match error {
        PublishError::Io(error) => error,
        unsafe_dir => io::Error::new(io::ErrorKind::PermissionDenied, unsafe_dir.to_string()),
    })?;
    let contents = serde_json::to_vec_pretty(sidecar)?;
    let count = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut temp_name = stem.to_os_string();
    temp_name.push(format!(".{}-{count}.tmp", std::process::id()));
    let temp = dir.join(temp_name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    let written = file
        .write_all(&contents)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600)))
        .and_then(|()| std::fs::rename(&temp, &target));
    if written.is_err() {
        // The temp name is unique to this call, so removing it touches
        // nothing another writer owns.
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// Read and parse the sidecar at `path`. Read-only.
pub fn read_sidecar(path: &Path) -> Result<Sidecar, SidecarError> {
    let contents = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// One socket stem found under `sock/`: its socket (if present), its sidecar
/// (if present and readable) and its liveness.
#[derive(Debug)]
#[non_exhaustive]
pub struct SweepEntry {
    /// `sock/<stem>.sock`, whether or not that file exists.
    pub socket_path: PathBuf,
    /// Whether `<stem>.sock` was listed in the directory.
    pub socket_present: bool,
    /// The parsed sidecar, when `<stem>.json` exists and parses.
    pub sidecar: Option<Sidecar>,
    /// `Some` only when `<stem>.json` exists but does not read or parse, so a
    /// missing sidecar (`None` here and in [`Self::sidecar`]) is never
    /// confused with a broken one.
    pub sidecar_error: Option<String>,
    /// The launch id segment of a stem that fits the
    /// `<016x digest>-<padded phase>-<launch id>` scheme, else `None`.
    pub launch_id_in_name: Option<String>,
    /// [`probe`]'s answer for a present socket; `Ok(Liveness::Gone)` for a
    /// sidecar with no socket.
    pub liveness: Result<Liveness, ProbeError>,
}

/// Every socket stem under `cache_dir`'s socket directory, one entry per
/// stem that has a `.sock` or a `.json` file, sorted by socket path. Other
/// names, including a half-published `.part` and a sidecar's in-flight
/// `.tmp`, are skipped. Each present socket is [`probe`]d and each present
/// sidecar read.
///
/// Read-only and never acting (D-06): it lists, probes and reads, and removes
/// no file and sends no shutdown; [`remove_launch_files`] is the acting
/// counterpart, and only `stop` and the backstop call it.
///
/// A socket directory that is absent or cannot be listed yields an empty
/// `Vec`, as `registry::load_roots_in` does: enumeration degrades rather than
/// fails.
pub fn sweep_in(cache_dir: &Path) -> Vec<SweepEntry> {
    let dir = registry::socket_dir_in(cache_dir);
    let Ok(listing) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    // stem -> (has a `.sock`, has a `.json`)
    let mut stems: std::collections::BTreeMap<String, (bool, bool)> =
        std::collections::BTreeMap::new();
    for entry in listing.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(stem) = name.strip_suffix(".sock") {
            stems.entry(stem.to_string()).or_default().0 = true;
        } else if let Some(stem) = name.strip_suffix(".json") {
            stems.entry(stem.to_string()).or_default().1 = true;
        }
    }
    let mut entries: Vec<SweepEntry> = stems
        .into_iter()
        .map(|(stem, (has_socket, has_sidecar))| sweep_entry(&dir, &stem, has_socket, has_sidecar))
        .collect();
    // Sorted by path, not stem: `a-b.sock` sorts before `a.sock`.
    entries.sort_by(|left, right| left.socket_path.cmp(&right.socket_path));
    entries
}

/// Build one [`SweepEntry`] from what the listing saw. Read-only.
fn sweep_entry(dir: &Path, stem: &str, has_socket: bool, has_sidecar: bool) -> SweepEntry {
    let socket_path = dir.join(format!("{stem}.sock"));
    let liveness = if has_socket {
        probe(&socket_path)
    } else {
        Ok(Liveness::Gone)
    };
    let (sidecar, sidecar_error) = if has_sidecar {
        match read_sidecar(&dir.join(format!("{stem}.json"))) {
            Ok(sidecar) => (Some(sidecar), None),
            // Removed between the listing and the read: missing, not broken.
            Err(SidecarError::Io(error)) if error.kind() == io::ErrorKind::NotFound => (None, None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };
    SweepEntry {
        socket_path,
        socket_present: has_socket,
        sidecar,
        sidecar_error,
        launch_id_in_name: socket_name_parts(stem).map(|(_, _, launch_id)| launch_id.to_string()),
        liveness,
    }
}

/// The [`sweep_in`] entries that may belong to `project_root` and `phase`.
///
/// An entry whose sidecar parsed matches when the sidecar names that root and
/// phase; the sidecar wins whenever it parses. An entry whose sidecar is
/// missing or broken is attributed by its file name instead (RV-05; D-09 lets
/// the pairing be recovered from the name): a stem that fits the
/// `<016x digest>-<padded phase>-<launch id>` scheme matches when its digest
/// is `project_root`'s and its phase segment is `phase.padded()`. A stem
/// outside the scheme names no root, so it matches any root when its socket
/// is Alive and its ping reply names `phase`: it may belong, and the D-09
/// guard fails closed on it.
///
/// Read-only and never acting (D-06): it filters [`sweep_in`]'s output, so it
/// removes no file and sends no shutdown. The name fallback only widens what
/// is reported.
pub fn sweep_for_in(cache_dir: &Path, project_root: &Path, phase: PhaseId) -> Vec<SweepEntry> {
    sweep_in(cache_dir)
        .into_iter()
        .filter(|entry| may_belong(entry, project_root, phase))
        .collect()
}

/// Whether `entry` may belong to `project_root` and `phase`; see
/// [`sweep_for_in`].
fn may_belong(entry: &SweepEntry, project_root: &Path, phase: PhaseId) -> bool {
    if let Some(sidecar) = &entry.sidecar {
        return sidecar.project_root == project_root && sidecar.phase == phase;
    }
    let stem = entry
        .socket_path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    match socket_name_parts(stem) {
        Some((digest, padded, _)) => {
            digest == registry::path_digest(project_root) && padded == phase.padded()
        }
        None => matches!(
            &entry.liveness,
            Ok(Liveness::Alive(reply)) if reply.phase.parse::<PhaseId>().is_ok_and(|named| named == phase)
        ),
    }
}

/// Split a socket stem of the form `<016x digest>-<padded phase>-<launch id>`
/// into its parts: 16 lowercase hex digits, a non-empty run of digits and
/// dots (`PhaseId::padded` never contains `-`), and a non-empty launch id,
/// which may itself contain `-`. Anything else is `None`.
fn socket_name_parts(stem: &str) -> Option<(u64, &str, &str)> {
    const DIGEST_HEX_LEN: usize = 16;
    let (hex, rest) = stem.split_at_checked(DIGEST_HEX_LEN)?;
    if !hex
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let (padded, launch_id) = rest.strip_prefix('-')?.split_once('-')?;
    if padded.is_empty()
        || launch_id.is_empty()
        || !padded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    let digest = u64::from_str_radix(hex, 16).ok()?;
    Some((digest, padded, launch_id))
}

/// Remove a launch's socket and its sidecar. A file that is already gone is
/// not an error.
///
/// This is the acting counterpart of the read-only [`sweep_in`],
/// [`sweep_for_in`], [`probe`] and [`read_sidecar`] (D-06): only `stop` and
/// the stale-socket backstop call it.
pub fn remove_launch_files(socket: &SocketPath) -> io::Result<()> {
    let mut first_error = None;
    for path in [socket.as_path().to_path_buf(), socket.sidecar_path()] {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// How a supervisor answered a shutdown request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShutdownOutcome {
    /// The request was accepted and the final line reported the stop.
    Stopped {
        /// The exit code the supervisor ends with.
        exit_code: i32,
        /// Whether the stage advance was suppressed by the stop.
        advance_suppressed: bool,
    },
    /// The supervisor had already handed the run off and did not stop it.
    HandedOff,
    /// The supervisor refused the request with this error.
    Refused(String),
}

/// Ask the supervisor at `path` to stop. Applies [`probe`]'s pre-connect
/// checks (a socket, owned by this uid), connects, sends
/// `{"op":"shutdown"}`, reads the immediate `accepted` line within
/// [`protocol::IO_TIMEOUT`], then the final line within `final_timeout`.
///
/// Only a final `stopped` line is [`ShutdownOutcome::Stopped`]; a
/// `handed_off` answer is [`ShutdownOutcome::HandedOff`] and any other
/// refusal is [`ShutdownOutcome::Refused`]. A missing, late or malformed
/// line is an error, never a stop.
pub fn request_shutdown(
    path: &Path,
    final_timeout: std::time::Duration,
) -> Result<ShutdownOutcome, ProbeError> {
    use std::io::Write;
    if !own_socket_at(path)? {
        return Err(ProbeError::Stat(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no socket at {}", path.display()),
        )));
    }
    let stream = UnixStream::connect(path).map_err(ProbeError::Connect)?;
    let timeout_error =
        |error: io::Error| ProbeError::NoReply(format!("could not set socket timeouts: {error}"));
    stream
        .set_read_timeout(Some(protocol::IO_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(protocol::IO_TIMEOUT)))
        .map_err(timeout_error)?;
    let request = serde_json::to_string(&protocol::Request::Shutdown)
        .map_err(|error| ProbeError::NoReply(format!("could not encode shutdown: {error}")))?;
    (&stream)
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|error| ProbeError::NoReply(format!("could not send shutdown: {error}")))?;
    // One reader across both lines: it may already hold the final line.
    let mut reader = io::BufReader::new(&stream);
    match read_shutdown_line(&mut reader, "accepted")? {
        ShutdownLine::Accepted => {}
        ShutdownLine::Refused(error) => return Ok(refusal(error)),
        ShutdownLine::Stopped(_) => {
            return Err(ProbeError::NoReply(
                "a stopped line arrived before the accepted line".to_string(),
            ));
        }
    }
    stream
        .set_read_timeout(Some(final_timeout))
        .map_err(timeout_error)?;
    match read_shutdown_line(&mut reader, "final")? {
        ShutdownLine::Stopped(stopped) => Ok(ShutdownOutcome::Stopped {
            exit_code: stopped.exit_code,
            advance_suppressed: stopped.advance_suppressed,
        }),
        ShutdownLine::Refused(error) => Ok(refusal(error)),
        ShutdownLine::Accepted => Err(ProbeError::NoReply(
            "a second accepted line arrived instead of the final line".to_string(),
        )),
    }
}

/// One parsed shutdown reply line.
enum ShutdownLine {
    Accepted,
    Stopped(protocol::ShutdownStopped),
    Refused(String),
}

/// A refusal's error string as an outcome: `handed_off` is its own outcome.
fn refusal(error: String) -> ShutdownOutcome {
    if error == "handed_off" {
        ShutdownOutcome::HandedOff
    } else {
        ShutdownOutcome::Refused(error)
    }
}

/// Read and classify one shutdown reply line. A missing, oversized or
/// malformed line, `"ok":true` with neither marker, and `"ok":false` with no
/// error string are all errors.
fn read_shutdown_line(
    reader: &mut impl io::BufRead,
    which: &str,
) -> Result<ShutdownLine, ProbeError> {
    let line = protocol::read_line(reader)
        .map_err(|error| ProbeError::NoReply(format!("no {which} shutdown line: {error}")))?;
    let value: serde_json::Value = serde_json::from_str(&line).map_err(|error| {
        ProbeError::NoReply(format!(
            "{which} shutdown line is not JSON ({error}): {line}"
        ))
    })?;
    let is_true = |key: &str| value.get(key) == Some(&serde_json::Value::Bool(true));
    let (ok, stopped, accepted) = (is_true("ok"), is_true("stopped"), is_true("accepted"));
    if value.get("ok") == Some(&serde_json::Value::Bool(false)) {
        return match value.get("error").and_then(serde_json::Value::as_str) {
            Some(error) => Ok(ShutdownLine::Refused(error.to_string())),
            None => Err(ProbeError::NoReply(format!(
                "{which} shutdown line refuses without an error: {line}"
            ))),
        };
    }
    if ok && stopped {
        return serde_json::from_value(value)
            .map(ShutdownLine::Stopped)
            .map_err(|error| {
                ProbeError::NoReply(format!(
                    "{which} shutdown line is missing fields ({error}): {line}"
                ))
            });
    }
    if ok && accepted {
        return Ok(ShutdownLine::Accepted);
    }
    Err(ProbeError::NoReply(format!(
        "{which} shutdown line is neither accepted nor stopped: {line}"
    )))
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

    const ROOT: &str = "/project/root";
    const OTHER_ROOT: &str = "/project/other";

    fn sample_sidecar(launch_id: &str, root: &Path, phase: PhaseId) -> Sidecar {
        Sidecar {
            launch_id: launch_id.to_string(),
            project_root: root.to_path_buf(),
            phase,
            stage: Some("code".to_string()),
            agent: "claude".to_string(),
            launch: "stream".to_string(),
            agent_pid: 4242,
            agent_pgid: 4242,
            agent_start_time: 123_456,
            boot_id: "boot-test".to_string(),
            supervisor_pid: std::process::id(),
            supervisor_start_time: 654_321,
            created_at: rfc3339_from_unix(1_700_000_000),
        }
    }

    fn reply_for(launch_id: &str, phase: PhaseId) -> PingReply {
        let mut reply = sample_reply(launch_id);
        reply.phase = phase.to_string();
        reply
    }

    /// A ping server that answers every connection until stopped and records
    /// each request it read, so a test can prove no shutdown was sent.
    struct PingServer {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        handle: JoinHandle<(UnixListener, Vec<Request>)>,
    }

    impl PingServer {
        fn stop(self) -> (UnixListener, Vec<Request>) {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            self.handle.join().expect("ping server thread")
        }
    }

    fn serve_until_stopped(listener: UnixListener, reply: PingReply) -> PingServer {
        listener
            .set_nonblocking(true)
            .expect("make the listener non-blocking");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let handle = thread::spawn(move || {
            let mut seen = Vec::new();
            while !flag.load(std::sync::atomic::Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("make the accepted stream blocking");
                        let answer = match protocol::read_request(&stream) {
                            Ok(request) => {
                                seen.push(request);
                                match request {
                                    Request::Ping => Reply::Ping(reply.clone()),
                                    _ => Reply::Error("unknown_op".to_string()),
                                }
                            }
                            Err(_) => Reply::Error("unknown_op".to_string()),
                        };
                        let _ = protocol::write_reply(&stream, &answer);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("the ping server could not accept: {error}"),
                }
            }
            (listener, seen)
        });
        PingServer { stop, handle }
    }

    fn paths(entries: &[SweepEntry]) -> Vec<&Path> {
        entries
            .iter()
            .map(|entry| entry.socket_path.as_path())
            .collect()
    }

    #[test]
    fn a_published_socket_is_found_by_root_and_phase_without_state() {
        let cache = short_cache();
        let root = Path::new(ROOT);
        let phase = PhaseId::new(7);
        let launch_id = new_launch_id();
        let socket = SocketPath::for_launch(cache.path(), root, phase, &launch_id)
            .expect("socket path fits sun_path");
        write_sidecar(&socket, &sample_sidecar(&launch_id, root, phase)).expect("write a sidecar");
        let listener = publish(&socket).expect("publish the socket");
        let server = serve_until_stopped(listener, reply_for(&launch_id, phase));

        let found = sweep_for_in(cache.path(), root, phase);
        let other_phase = sweep_for_in(cache.path(), root, PhaseId::new(8));
        let other_root = sweep_for_in(cache.path(), Path::new(OTHER_ROOT), phase);
        drop(server.stop());

        let top_level: Vec<_> = std::fs::read_dir(cache.path())
            .expect("read the cache dir")
            .map(|entry| entry.expect("cache entry").file_name())
            .collect();
        assert_eq!(
            top_level,
            [std::ffi::OsString::from("sock")],
            "no state file exists: the cache holds only sock/"
        );
        assert_eq!(found.len(), 1, "exactly one entry for (root, 7): {found:?}");
        let entry = &found[0];
        assert_eq!(entry.socket_path, socket.as_path());
        assert!(entry.socket_present, "the socket is listed");
        assert_eq!(
            entry
                .sidecar
                .as_ref()
                .map(|sidecar| sidecar.launch_id.as_str()),
            Some(launch_id.as_str()),
            "the entry carries its sidecar"
        );
        assert!(
            matches!(&entry.liveness, Ok(Liveness::Alive(reply)) if reply.launch_id == launch_id),
            "the entry is Alive, got {:?}",
            entry.liveness
        );
        assert!(
            other_phase.is_empty(),
            "another phase finds nothing: {other_phase:?}"
        );
        assert!(
            other_root.is_empty(),
            "another root finds nothing: {other_root:?}"
        );
    }

    #[test]
    fn sidecar_round_trips_and_is_owner_only() {
        let cache = short_cache();
        let root = Path::new(ROOT);
        let phase = PhaseId::new(7);
        let launch_id = new_launch_id();
        let socket = SocketPath::for_launch(cache.path(), root, phase, &launch_id)
            .expect("socket path fits sun_path");
        let sidecar = sample_sidecar(&launch_id, root, phase);
        let path = socket.sidecar_path();

        let written = write_sidecar(&socket, &sidecar);
        assert!(written.is_ok(), "write_sidecar failed: {written:?}");
        let read = read_sidecar(&path);
        assert_eq!(
            read.as_ref().ok(),
            Some(&sidecar),
            "read_sidecar returns what was written, got {read:?}"
        );
        let mode = std::fs::metadata(&path)
            .map(|meta| meta.permissions().mode() & 0o777)
            .ok();
        assert_eq!(mode, Some(0o600), "the sidecar is owner-only");
        let names: Vec<_> = std::fs::read_dir(registry::socket_dir_in(cache.path()))
            .expect("read sock/")
            .map(|entry| entry.expect("sock/ entry").file_name())
            .collect();
        assert_eq!(
            names,
            [path.file_name().expect("sidecar file name").to_os_string()],
            "the temporary file is gone after the write"
        );

        // A second write replaces the first in place.
        let mut changed = sidecar.clone();
        changed.stage = Some("verify".to_string());
        write_sidecar(&socket, &changed).expect("rewrite the sidecar");
        assert_eq!(read_sidecar(&path).ok(), Some(changed));

        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(rfc3339_from_unix(951_825_599), "2000-02-29T11:59:59Z");
        let now = rfc3339_now();
        assert!(
            now.len() == "1970-01-01T00:00:00Z".len()
                && now.starts_with("20")
                && now.ends_with('Z'),
            "rfc3339_now has the RFC 3339 UTC shape, got {now:?}"
        );

        let bytes = std::fs::read(&path).expect("read the sidecar bytes");
        std::fs::write(&path, &bytes[..bytes.len() / 2]).expect("truncate the sidecar");
        let truncated = read_sidecar(&path);
        assert!(
            matches!(truncated, Err(SidecarError::Json(_))),
            "a truncated sidecar is a Json error, got {truncated:?}"
        );
        let missing = read_sidecar(&cache.path().join("absent.json"));
        assert!(
            matches!(&missing, Err(SidecarError::Io(error)) if error.kind() == io::ErrorKind::NotFound),
            "a missing sidecar is an Io error, got {missing:?}"
        );
    }

    #[test]
    fn socket_name_parts_accepts_only_the_launch_scheme() {
        let digest = registry::path_digest(Path::new(ROOT));
        let built = SocketPath::for_launch(
            Path::new("/c"),
            Path::new(ROOT),
            "35.1".parse::<PhaseId>().expect("phase"),
            "orphan-a",
        )
        .expect("fits");
        let stem = built
            .as_path()
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .expect("utf-8 stem");
        assert_eq!(
            socket_name_parts(stem),
            Some((digest, "35.1", "orphan-a")),
            "a launch id may contain '-'"
        );
        for outside in [
            "foreign",
            "",
            "0123456789ABCDEF-07-id",
            "0123456789abcde-07-id",
            "0123456789abcdef07-id",
            "0123456789abcdef--id",
            "0123456789abcdef-07-",
            "0123456789abcdef-7a-id",
            "0123456789abcdef-07",
        ] {
            assert_eq!(
                socket_name_parts(outside),
                None,
                "{outside:?} is outside the scheme"
            );
        }
    }

    #[test]
    fn a_socket_whose_sidecar_will_not_parse_is_still_found_by_its_name() {
        const NAMED: &str = "aaaa0000aaaa0000";
        let cache = short_cache();
        let root = Path::new(ROOT);
        let other_root = Path::new(OTHER_ROOT);
        let seven = PhaseId::new(7);
        let eight = PhaseId::new(8);

        let named = SocketPath::for_launch(cache.path(), root, seven, NAMED)
            .expect("socket path fits sun_path");
        write_sidecar(&named, &sample_sidecar(NAMED, root, seven)).expect("write a sidecar");
        let named_server = serve_until_stopped(
            publish(&named).expect("publish the named socket"),
            reply_for(NAMED, seven),
        );

        // (1) A broken sidecar: found by the name's root digest and phase.
        std::fs::write(named.sidecar_path(), b"{").expect("break the sidecar");
        let found = sweep_for_in(cache.path(), root, seven);
        assert_eq!(
            paths(&found),
            [named.as_path()],
            "a broken sidecar is found by name"
        );
        let entry = &found[0];
        assert!(entry.sidecar.is_none(), "the broken sidecar is not parsed");
        assert!(
            entry.sidecar_error.is_some(),
            "a broken sidecar is reported as broken"
        );
        assert_eq!(entry.launch_id_in_name.as_deref(), Some(NAMED));
        assert!(
            matches!(&entry.liveness, Ok(Liveness::Alive(_))),
            "the named socket is Alive, got {:?}",
            entry.liveness
        );
        assert!(
            sweep_for_in(cache.path(), other_root, seven).is_empty(),
            "another root's digest does not claim it"
        );
        assert!(
            sweep_for_in(cache.path(), root, eight).is_empty(),
            "another phase does not claim it"
        );

        // (2) A missing sidecar: still found, and told apart from a broken one.
        std::fs::remove_file(named.sidecar_path()).expect("remove the sidecar");
        let found = sweep_for_in(cache.path(), root, seven);
        assert_eq!(
            paths(&found),
            [named.as_path()],
            "a missing sidecar is found by name"
        );
        assert!(found[0].sidecar.is_none());
        assert!(
            found[0].sidecar_error.is_none(),
            "a missing sidecar is not reported as broken"
        );
        assert_eq!(found[0].launch_id_in_name.as_deref(), Some(NAMED));

        // (3) A name outside the scheme with no sidecar: attributed only by the
        // phase its ping reply names, for any root.
        let foreign = SocketPath::new(registry::socket_dir_in(cache.path()).join("foreign.sock"))
            .expect("foreign path fits");
        let foreign_server = serve_until_stopped(
            publish(&foreign).expect("publish the foreign socket"),
            reply_for("foreign", seven),
        );
        for owner in [root, other_root] {
            let found = sweep_for_in(cache.path(), owner, seven);
            let entry = found
                .iter()
                .find(|entry| entry.socket_path == foreign.as_path());
            assert!(
                entry.is_some(),
                "the foreign socket answering phase 7 may belong to {}: {found:?}",
                owner.display()
            );
            assert_eq!(
                entry.and_then(|entry| entry.launch_id_in_name.clone()),
                None
            );
        }
        assert!(
            !paths(&sweep_for_in(cache.path(), root, eight)).contains(&foreign.as_path()),
            "the foreign socket answering phase 7 is not claimed for phase 8"
        );

        // (4) A parsed sidecar wins over the name.
        const MISLABELED: &str = "bbbb0000bbbb0000";
        let mislabeled = SocketPath::for_launch(cache.path(), root, seven, MISLABELED)
            .expect("socket path fits sun_path");
        write_sidecar(&mislabeled, &sample_sidecar(MISLABELED, other_root, seven))
            .expect("write the mislabeled sidecar");
        let mislabeled_server = serve_until_stopped(
            publish(&mislabeled).expect("publish the mislabeled socket"),
            reply_for(MISLABELED, seven),
        );
        let for_other = sweep_for_in(cache.path(), other_root, seven);
        let for_root = sweep_for_in(cache.path(), root, seven);
        drop(mislabeled_server.stop());
        drop(foreign_server.stop());
        drop(named_server.stop());
        assert!(
            paths(&for_other).contains(&mislabeled.as_path()),
            "the sidecar's root claims it: {for_other:?}"
        );
        assert!(
            !paths(&for_root).contains(&mislabeled.as_path()),
            "the name's root does not, once a sidecar parses: {for_root:?}"
        );
    }

    /// Every file under `sock/` as `(name, length, mtime)`, sorted.
    fn snapshot(cache: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
        let mut files: Vec<_> = std::fs::read_dir(registry::socket_dir_in(cache))
            .expect("read sock/")
            .map(|entry| {
                let entry = entry.expect("sock/ entry");
                let meta = std::fs::symlink_metadata(entry.path()).expect("stat a sock/ entry");
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    meta.len(),
                    meta.modified().expect("mtime"),
                )
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn sweep_reports_without_removing_or_signalling() {
        let cache = short_cache();
        let root = Path::new(ROOT);
        let phase = PhaseId::new(7);
        let launch = |id: &str| {
            let socket = SocketPath::for_launch(cache.path(), root, phase, id)
                .expect("socket path fits sun_path");
            write_sidecar(&socket, &sample_sidecar(id, root, phase)).expect("write a sidecar");
            socket
        };
        let alive_id = new_launch_id();
        let alive = launch(&alive_id);
        let server = serve_until_stopped(
            publish(&alive).expect("publish the alive socket"),
            reply_for(&alive_id, phase),
        );
        let stale = launch(&new_launch_id());
        drop(publish(&stale).expect("publish the stale socket"));
        let leftover = launch(&new_launch_id());
        let half = SocketPath::for_launch(cache.path(), root, phase, &new_launch_id())
            .expect("socket path fits sun_path");
        std::fs::write(half.part_path(), b"half-published").expect("write a .part file");

        let before = snapshot(cache.path());
        let entries = sweep_in(cache.path());
        let after = snapshot(cache.path());
        let still_alive = probe(alive.as_path());
        let (_listener, requests) = server.stop();

        let mut expected = vec![alive.as_path(), stale.as_path(), leftover.as_path()];
        expected.sort();
        assert_eq!(
            paths(&entries),
            expected,
            "one entry per .sock/.json stem; the .part is not one"
        );
        let liveness_of = |socket: &SocketPath| {
            let entry = entries
                .iter()
                .find(|entry| entry.socket_path == socket.as_path())
                .expect("entry present");
            (entry.socket_present, &entry.liveness)
        };
        assert!(matches!(
            liveness_of(&alive),
            (true, Ok(Liveness::Alive(_)))
        ));
        assert!(matches!(liveness_of(&stale), (true, Ok(Liveness::Stale))));
        assert!(matches!(
            liveness_of(&leftover),
            (false, Ok(Liveness::Gone))
        ));
        assert_eq!(
            before, after,
            "the sweep leaves every name, length and mtime under sock/ unchanged"
        );
        assert!(
            matches!(still_alive, Ok(Liveness::Alive(_))),
            "the alive listener still answers after the sweep, got {still_alive:?}"
        );
        assert!(
            requests.iter().all(|request| *request == Request::Ping),
            "the sweep sent only pings, got {requests:?}"
        );

        // Control: the acting function changes the same snapshot, so the
        // comparison above can see a removal.
        remove_launch_files(&stale).expect("remove the stale launch's files");
        let removed = snapshot(cache.path());
        assert_ne!(before, removed, "the snapshot detects a removal");
        let stale_names = [stale.as_path(), stale.sidecar_path().as_path()].map(|path| {
            path.file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        });
        assert_eq!(
            removed,
            before
                .iter()
                .filter(|(name, _, _)| !stale_names.contains(name))
                .cloned()
                .collect::<Vec<_>>(),
            "exactly the stale socket and its sidecar are removed"
        );
        assert!(
            remove_launch_files(&stale).is_ok(),
            "removing files that are already gone succeeds"
        );
    }

    /// Accept one connection on `listener`, require a shutdown request, then
    /// run `script` on the connection.
    fn serve_shutdown(
        listener: UnixListener,
        script: impl FnOnce(&UnixStream) + Send + 'static,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            // Poll, so a client that never connects ends the thread instead
            // of wedging the test on `accept`.
            listener
                .set_nonblocking(true)
                .expect("make the listener non-blocking");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return;
                        }
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("the scripted server could not accept: {error}"),
                }
            };
            stream
                .set_nonblocking(false)
                .expect("make the accepted stream blocking");
            let request = protocol::read_request(&stream);
            assert!(
                matches!(request, Ok(Request::Shutdown)),
                "the client sends a shutdown request, got {request:?}"
            );
            script(&stream);
        })
    }

    fn write_raw(mut stream: &UnixStream, line: &str) {
        use std::io::Write;
        stream
            .write_all(format!("{line}\n").as_bytes())
            .expect("write a raw line");
    }

    #[test]
    fn request_shutdown_reads_accepted_then_stopped() {
        use std::time::Duration;
        let cache = short_cache();
        let accepted = || {
            Reply::Accepted(protocol::ShutdownAccepted {
                owner: "agent_running".to_string(),
            })
        };
        let run = |script: Box<dyn FnOnce(&UnixStream) + Send>, final_timeout: Duration| {
            let (socket, _, listener) = published(cache.path());
            let server = serve_shutdown(listener, script);
            let outcome = request_shutdown(socket.as_path(), final_timeout);
            server.join().expect("scripted server thread");
            outcome
        };

        let stopped = run(
            Box::new(move |stream| {
                protocol::write_reply(stream, &accepted()).expect("write accepted");
                thread::sleep(Duration::from_millis(200));
                protocol::write_reply(
                    stream,
                    &Reply::Stopped(protocol::ShutdownStopped {
                        exit_code: 143,
                        advance_suppressed: true,
                    }),
                )
                .expect("write stopped");
            }),
            Duration::from_secs(5),
        );
        assert!(
            matches!(
                stopped,
                Ok(ShutdownOutcome::Stopped {
                    exit_code: 143,
                    advance_suppressed: true
                })
            ),
            "accepted then stopped is Stopped, got {stopped:?}"
        );

        let handed_off = run(
            Box::new(|stream| {
                protocol::write_reply(stream, &Reply::Error("handed_off".to_string()))
                    .expect("write handed_off");
            }),
            Duration::from_secs(5),
        );
        assert!(
            matches!(handed_off, Ok(ShutdownOutcome::HandedOff)),
            "handed_off is HandedOff, got {handed_off:?}"
        );

        let late_hand_off = run(
            Box::new(move |stream| {
                protocol::write_reply(stream, &accepted()).expect("write accepted");
                protocol::write_reply(stream, &Reply::Error("handed_off".to_string()))
                    .expect("write handed_off");
            }),
            Duration::from_secs(5),
        );
        assert!(
            matches!(late_hand_off, Ok(ShutdownOutcome::HandedOff)),
            "a hand-off after accepted is HandedOff, never Stopped, got {late_hand_off:?}"
        );

        let refused = run(
            Box::new(|stream| {
                protocol::write_reply(stream, &Reply::Error("unknown_op".to_string()))
                    .expect("write the refusal");
            }),
            Duration::from_secs(5),
        );
        assert!(
            matches!(&refused, Ok(ShutdownOutcome::Refused(error)) if error == "unknown_op"),
            "another refusal is Refused, got {refused:?}"
        );

        let closed = run(
            Box::new(move |stream| {
                protocol::write_reply(stream, &accepted()).expect("write accepted");
            }),
            Duration::from_secs(5),
        );
        assert!(
            matches!(closed, Err(ProbeError::NoReply(_))),
            "a close after accepted is an error, never Stopped, got {closed:?}"
        );

        let silent = run(
            Box::new(move |stream| {
                use std::io::Read;
                protocol::write_reply(stream, &accepted()).expect("write accepted");
                // Hold the connection open until the client gives up and
                // closes it, past its final timeout.
                let _ = (&mut &*stream).read(&mut [0_u8; 1]);
            }),
            Duration::from_millis(300),
        );
        assert!(
            matches!(silent, Err(ProbeError::NoReply(_))),
            "no final line within the timeout is an error, got {silent:?}"
        );

        let malformed = run(
            Box::new(move |stream| {
                protocol::write_reply(stream, &accepted()).expect("write accepted");
                write_raw(
                    stream,
                    r#"{"ok":true,"exit_code":143,"advance_suppressed":true}"#,
                );
            }),
            Duration::from_secs(5),
        );
        assert!(
            matches!(malformed, Err(ProbeError::NoReply(_))),
            "a final line without stopped:true is an error, got {malformed:?}"
        );

        // The pre-connect checks are probe's: a regular file is never asked.
        let regular = cache.path().join("regular.sock");
        std::fs::write(&regular, b"not a socket").expect("write a regular file");
        let not_a_socket = request_shutdown(&regular, Duration::from_secs(1));
        assert!(
            matches!(not_a_socket, Err(ProbeError::NotASocket(_))),
            "a regular file is NotASocket, got {not_a_socket:?}"
        );
    }
}
