//! The phase's systemd user unit (D-01, D-04, D-04a): its name, the `systemd-run`
//! argv that starts `devflow __runner` in it, which caller variables cross into it,
//! and the runner's readiness report.

use devflow_core::phase_id::PhaseId;
use std::ffi::OsString;
use std::path::Path;

/// Variables the service manager sets for a unit itself, never passed from the caller.
pub(crate) const MANAGER_OWNED_ENV: &[&str] = &[
    "NOTIFY_SOCKET",
    "LISTEN_FDS",
    "LISTEN_PID",
    "LISTEN_FDNAMES",
    "INVOCATION_ID",
    "JOURNAL_STREAM",
    "MANAGERPID",
    "SYSTEMD_EXEC_PID",
    "WATCHDOG_PID",
    "WATCHDOG_USEC",
];

/// How long the manager waits for a stopping runner before it SIGKILLs the unit's
/// cgroup: about twice the runner's own SIGTERM-then-SIGKILL path for its agent.
pub(crate) const TIMEOUT_STOP_SEC: &str = "10s";

/// How long the manager waits for the runner's `READY=1` before it fails the start.
pub(crate) const TIMEOUT_START_SEC: &str = "30s";

/// The phase's unit name: `devflow-{digest:016x}-{padded}.service`, the digest being
/// [`devflow_core::registry::path_digest`] over the canonical project root, so one
/// project's phase always names the same unit and systemd refuses a second active one.
///
/// # Errors
/// The project root cannot be canonicalised.
pub(crate) fn unit_name(project_root: &Path, phase: PhaseId) -> std::io::Result<String> {
    let root = project_root.canonicalize()?;
    Ok(format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(&root),
        phase.padded()
    ))
}

/// Whether systemd accepts `name` as an environment variable name.
fn is_valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// The caller variables a runner unit receives (D-04): every name systemd accepts
/// (`^[A-Za-z_][A-Za-z0-9_]*$`) except [`MANAGER_OWNED_ENV`]. A name it would refuse
/// (bash's exported `BASH_FUNC_x%%`, a non-UTF-8 name) is dropped, since one such name
/// fails the whole `systemd-run`.
#[expect(
    dead_code,
    reason = "`start` launches the runner unit from plan 52.2-06"
)]
pub(crate) fn passable_env_names(names: impl IntoIterator<Item = OsString>) -> Vec<String> {
    let mut passable: Vec<String> = names
        .into_iter()
        .filter_map(|name| name.into_string().ok())
        .filter(|name| is_valid_env_name(name) && !MANAGER_OWNED_ENV.contains(&name.as_str()))
        .collect();
    passable.sort();
    passable.dedup();
    passable
}

/// The `systemd-run` arguments (after the program) that run `exe __runner` for `phase`
/// of `project_root` as the phase's unit (D-01). Each variable in `env_names` is passed
/// as `--setenv NAME`, by name only, so its value never appears on a command line,
/// which every user can read in `/proc` (D-04).
///
/// # Errors
/// The project root cannot be canonicalised.
#[expect(
    dead_code,
    reason = "`start` launches the runner unit from plan 52.2-06"
)]
pub(crate) fn runner_unit_argv(
    project_root: &Path,
    phase: PhaseId,
    exe: &Path,
    env_names: &[String],
) -> std::io::Result<Vec<OsString>> {
    let root = project_root.canonicalize()?;
    let mut argv: Vec<OsString> = vec![
        "--user".into(),
        "--collect".into(),
        format!("--unit={}", unit_name(&root, phase)?).into(),
        "-p".into(),
        "Type=notify".into(),
        "-p".into(),
        "NotifyAccess=main".into(),
        "-p".into(),
        "KillMode=control-group".into(),
        "-p".into(),
        format!("TimeoutStopSec={TIMEOUT_STOP_SEC}").into(),
        "-p".into(),
        format!("TimeoutStartSec={TIMEOUT_START_SEC}").into(),
        "--expand-environment=no".into(),
    ];
    let mut working = OsString::from("--working-directory=");
    working.push(&root);
    argv.push(working);
    for name in env_names {
        argv.push("--setenv".into());
        argv.push(name.into());
    }
    argv.push("--".into());
    argv.push(exe.into());
    argv.push("__runner".into());
    argv.push("--project".into());
    argv.push(root.into());
    argv.push("--phase".into());
    argv.push(phase.to_string().into());
    Ok(argv)
}

/// Tell systemd this unit is ready (D-04a). The runner calls it only once it holds its
/// runner lock, so a `Type=notify` start succeeds only over a runner that has the lock.
/// A no-op when `NOTIFY_SOCKET` is unset: the runner verb run outside a unit.
///
/// # Errors
/// The notify socket address is unusable or the datagram could not be sent.
pub(crate) fn notify_ready() -> std::io::Result<()> {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixDatagram};
    let Some(target) = std::env::var_os("NOTIFY_SOCKET") else {
        return Ok(());
    };
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(target.as_os_str());
    let addr = match bytes.strip_prefix(b"@") {
        Some(name) => SocketAddr::from_abstract_name(name)?,
        None => SocketAddr::from_pathname(Path::new(&target))?,
    };
    UnixDatagram::unbound()?
        .send_to_addr(b"READY=1", &addr)
        .map(|_| ())
}
