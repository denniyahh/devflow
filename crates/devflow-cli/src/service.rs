//! The phase's systemd user unit (D-01, D-04, D-04a): its name, the `systemd-run`
//! argv that starts `devflow __runner` in it, which caller variables cross into it,
//! and the runner's readiness report.

use devflow_core::phase_id::PhaseId;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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
/// cgroup (`TimeoutStopSec`): about twice the runner's own SIGTERM-then-SIGKILL path
/// for its agent.
pub(crate) const TIMEOUT_STOP_SEC: Duration = Duration::from_secs(10);

/// How long a verb waits for a stopping unit to read inactive (D-07a):
/// [`TIMEOUT_STOP_SEC`] plus 5 s, so the manager's own SIGKILL lands inside the wait.
pub(crate) const UNIT_STOP_WAIT: Duration = Duration::from_secs(TIMEOUT_STOP_SEC.as_secs() + 5);

/// How long the manager waits for the runner's `READY=1` before it fails the start.
pub(crate) const TIMEOUT_START_SEC: &str = "30s";

/// The deadline of one `systemctl` call made on its own, outside a wait (codex-7).
pub(crate) const SYSTEMCTL_CALL_BOUND: Duration = Duration::from_secs(30);

/// How often [`run_bounded`] checks whether its child has exited.
const BOUNDED_CALL_POLL: Duration = Duration::from_millis(50);

/// How often [`wait_unit_inactive`] reads the unit's state.
const UNIT_WAIT_POLL: Duration = Duration::from_millis(200);

/// The process for the service tool `name` (`systemctl`, `systemd-run`, `loginctl`).
/// Every process this module starts is built here.
#[cfg(not(test))]
pub(crate) fn tool(name: &str) -> Command {
    Command::new(name)
}

/// D-07b: a CLI unit test never reaches a real service tool. The cfg(test) builds of
/// this module's readers use the [`double`] instead, so reaching this is a test bug.
#[cfg(test)]
pub(crate) fn tool(name: &str) -> Command {
    panic!("a CLI unit test reached {name}; use the service test double")
}

/// Run `command` with its stdout and stderr piped, and return its output once it exits.
/// A child still running at `deadline` is killed and reaped, and the call is an
/// [`std::io::ErrorKind::TimedOut`] error naming the program (codex-7). The output is
/// read after exit, which suits the one-line answers `systemctl` gives.
///
/// # Errors
/// The child could not be spawned or waited on, or it outlived `deadline`.
pub(crate) fn run_bounded(mut command: Command, deadline: Duration) -> std::io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let until = Instant::now() + deadline;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output(),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        let now = Instant::now();
        if now >= until {
            let _ = child.kill();
            child.wait()?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "{} did not exit within {} ms and was killed",
                    command.get_program().to_string_lossy(),
                    deadline.as_millis()
                ),
            ));
        }
        std::thread::sleep(BOUNDED_CALL_POLL.min(until - now));
    }
}

/// How a user manager that did not answer the Version probe is sorted (D-03, review
/// codex-2/pi-8, r3 codex-1/pi-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagerUnavailableKind {
    /// Positive evidence that no manager runs for this user: `XDG_RUNTIME_DIR` is the
    /// user's own `/run/user/<uid>` and has no `systemd/private`. No unit can exist.
    Absent,
    /// This shell cannot reach the manager: `XDG_RUNTIME_DIR` is unset, empty,
    /// relative or another path, so a missing socket there says nothing about it.
    Unreachable,
    /// The socket exists, or its stat failed some other way, and the manager did not
    /// answer.
    Failed,
}

/// Why [`manager_available`] found no answering user manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagerUnavailable {
    /// How the failure is sorted.
    pub(crate) kind: ManagerUnavailableKind,
    /// The `XDG_RUNTIME_DIR` the probe ran under.
    pub(crate) runtime_dir: Option<OsString>,
    /// This user's own runtime dir, `/run/user/<uid>`.
    pub(crate) own_runtime_dir: PathBuf,
    /// What `systemctl` printed on stderr, or why it could not run.
    pub(crate) stderr: String,
}

impl ManagerUnavailable {
    /// The line a verb prints for this answer about `phase`: the runner-lock-only note
    /// for [`ManagerUnavailableKind::Absent`], else the refusal's reason. Each carries
    /// systemctl's stderr (agy-6).
    pub(crate) fn describe(&self, phase: PhaseId) -> String {
        let runtime = self
            .runtime_dir
            .as_deref()
            .map(|dir| Path::new(dir).display().to_string());
        match self.kind {
            ManagerUnavailableKind::Absent => format!(
                "note: no systemd user manager runs for this user (no {}/systemd/private; \
                 systemctl: {}); phase {phase} checked by its runner lock only",
                runtime.unwrap_or_default(),
                self.stderr
            ),
            ManagerUnavailableKind::Unreachable => {
                let shown = match &runtime {
                    None => "unset".to_string(),
                    Some(dir) if dir.is_empty() => "empty".to_string(),
                    Some(dir) if Path::new(dir).is_relative() => format!("relative (\"{dir}\")"),
                    Some(dir) => format!("\"{dir}\""),
                };
                let bus = if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
                    "set"
                } else {
                    "unset"
                };
                format!(
                    "cannot reach the systemd user manager from this shell: XDG_RUNTIME_DIR is \
                     {shown} and not this user's {} (DBUS_SESSION_BUS_ADDRESS {bus}); \
                     systemctl: {}",
                    self.own_runtime_dir.display(),
                    self.stderr
                )
            }
            ManagerUnavailableKind::Failed => format!(
                "the systemd user manager did not answer; systemctl: {}",
                self.stderr
            ),
        }
    }
}

/// Sort a Version probe that got no answer (review codex-2/pi-8, r3 codex-1/pi-5).
/// [`ManagerUnavailableKind::Absent`] only when `runtime_dir` equals `own_runtime_dir`
/// by path components (a trailing `/` matches; a symlink or any other spelling does
/// not) and `<runtime_dir>/systemd/private`, the socket `systemctl --user` connects to,
/// is NotFound. Unset, empty, relative or any other dir is Unreachable; anything else
/// is Failed. Never canonicalised: a component compare can only read more cases as
/// Unreachable, which refuses.
pub(crate) fn classify_unanswered(
    runtime_dir: Option<&OsStr>,
    own_runtime_dir: &Path,
    stderr: String,
) -> ManagerUnavailable {
    let kind = match runtime_dir.map(Path::new) {
        None => ManagerUnavailableKind::Unreachable,
        Some(dir) if dir.as_os_str().is_empty() || dir.is_relative() => {
            ManagerUnavailableKind::Unreachable
        }
        Some(dir) if !dir.components().eq(own_runtime_dir.components()) => {
            ManagerUnavailableKind::Unreachable
        }
        Some(dir) => match std::fs::symlink_metadata(dir.join("systemd").join("private")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ManagerUnavailableKind::Absent
            }
            Ok(_) | Err(_) => ManagerUnavailableKind::Failed,
        },
    };
    ManagerUnavailable {
        kind,
        runtime_dir: runtime_dir.map(OsStr::to_os_string),
        own_runtime_dir: own_runtime_dir.to_path_buf(),
        stderr,
    }
}

/// This user's runtime dir as pam_systemd sets it: `/run/user/<uid>`.
#[cfg(not(test))]
fn own_runtime_dir() -> PathBuf {
    // SAFETY: getuid takes no arguments, cannot fail and touches no memory.
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/run/user/{uid}"))
}

/// D-03: whether the systemd user manager answers
/// `systemctl --user show -p Version --value` (never `is-system-running`).
///
/// # Errors
/// It did not answer: [`classify_unanswered`]'s sort of `XDG_RUNTIME_DIR`. A probe
/// that timed out is [`ManagerUnavailableKind::Failed`], never Absent.
#[cfg(not(test))]
pub(crate) fn manager_available() -> Result<(), ManagerUnavailable> {
    let mut command = tool("systemctl");
    command.args(["--user", "show", "-p", "Version", "--value"]);
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR");
    let stderr = match run_bounded(command, SYSTEMCTL_CALL_BOUND) {
        Ok(output) if output.status.success() => return Ok(()),
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            return Err(ManagerUnavailable {
                kind: ManagerUnavailableKind::Failed,
                runtime_dir,
                own_runtime_dir: own_runtime_dir(),
                stderr: error.to_string(),
            });
        }
        Err(error) => format!("could not run systemctl: {error}"),
    };
    Err(classify_unanswered(
        runtime_dir.as_deref(),
        &own_runtime_dir(),
        stderr,
    ))
}

/// The cfg(test) build: the manager answer the [`double`] holds.
#[cfg(test)]
pub(crate) fn manager_available() -> Result<(), ManagerUnavailable> {
    double::manager()
}

/// The `ActiveState` text of `unit` (`inactive`, `deactivating`, `active`, ...), read
/// with `systemctl --user show -p ActiveState --value` within `bound`. Never an exit
/// code: `is-active` exits 3 for both deactivating and inactive.
///
/// # Errors
/// The call failed, exited non-zero or timed out: a description naming the unit and
/// carrying systemctl's stderr, never "inactive".
#[cfg(not(test))]
pub(crate) fn unit_active_state(unit: &str, bound: Duration) -> Result<String, String> {
    let mut command = tool("systemctl");
    command.args(["--user", "show", "-p", "ActiveState", "--value", unit]);
    match run_bounded(command, bound) {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        Ok(output) => Err(format!(
            "`systemctl --user show -p ActiveState {unit}` failed ({}); systemctl: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Err(format!(
            "`systemctl --user show -p ActiveState {unit}` did not answer: {error}"
        )),
    }
}

/// The cfg(test) build: the unit state the [`double`] holds, counting the read.
#[cfg(test)]
pub(crate) fn unit_active_state(unit: &str, _bound: Duration) -> Result<String, String> {
    double::read_unit(unit)
}

/// D-07a: wait until `unit` reads `inactive` or `failed`, reading its state every
/// [`UNIT_WAIT_POLL`] and giving each read the time left of `bound`.
///
/// # Errors
/// A read failed, or `bound` passed: the error names the unit and its last state.
pub(crate) fn wait_unit_inactive(unit: &str, bound: Duration) -> Result<(), String> {
    let deadline = Instant::now() + bound;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let state = unit_active_state(unit, remaining)?;
        if state == "inactive" || state == "failed" {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < UNIT_WAIT_POLL * 2 {
            return Err(format!(
                "unit {unit} still reads {state} after {} s",
                bound.as_secs()
            ));
        }
        std::thread::sleep(UNIT_WAIT_POLL);
    }
}

/// The CLI unit-test double for the service manager (D-07b): one per thread, so a
/// test sets what the manager and the unit report without touching the process.
#[cfg(test)]
pub(crate) mod double {
    use std::cell::RefCell;

    /// What the double reports.
    struct Double {
        /// The manager's answer to the Version probe.
        manager: Result<(), super::ManagerUnavailable>,
        /// The unit's `ActiveState`, or the read error.
        unit_state: Result<String, String>,
        /// How many times the unit was read.
        unit_reads: usize,
    }

    thread_local! {
        static DOUBLE: RefCell<Double> = RefCell::new(Double {
            manager: Ok(()),
            unit_state: Ok("inactive".to_string()),
            unit_reads: 0,
        });
    }

    /// Make the manager on this thread answer (`None`) or not, sorted as `kind`.
    pub(crate) fn set_manager(kind: Option<super::ManagerUnavailableKind>) {
        DOUBLE.with_borrow_mut(|double| {
            double.manager = kind.map_or(Ok(()), |kind| {
                Err(super::ManagerUnavailable {
                    kind,
                    runtime_dir: Some("/run/user/0".into()),
                    own_runtime_dir: "/run/user/0".into(),
                    stderr: "Failed to connect to bus: No medium found (test double)".to_string(),
                })
            });
        });
    }

    pub(super) fn manager() -> Result<(), super::ManagerUnavailable> {
        DOUBLE.with_borrow(|double| double.manager.clone())
    }

    /// Make every later unit read on this thread answer `state`.
    pub(crate) fn set_unit_state(state: Result<&str, &str>) {
        DOUBLE.with_borrow_mut(|double| {
            double.unit_state = state.map(str::to_string).map_err(str::to_string);
        });
    }

    /// How many times a unit was read on this thread.
    pub(crate) fn unit_reads() -> usize {
        DOUBLE.with_borrow(|double| double.unit_reads)
    }

    pub(super) fn read_unit(_unit: &str) -> Result<String, String> {
        DOUBLE.with_borrow_mut(|double| {
            double.unit_reads += 1;
            double.unit_state.clone()
        })
    }
}

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
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`start` launches the runner unit from plan 52.2-06"
    )
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
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`start` launches the runner unit from plan 52.2-06"
    )
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
        format!("TimeoutStopSec={}s", TIMEOUT_STOP_SEC.as_secs()).into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The arguments in `argv` that carry a value after `--setenv`: `--setenv NAME=v`
    /// or `--setenv=NAME=v`. A by-name argument (`--setenv NAME`) carries none.
    fn setenv_values(argv: &[OsString]) -> Vec<String> {
        let argv: Vec<String> = argv
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let mut valued = Vec::new();
        for (at, arg) in argv.iter().enumerate() {
            if let Some(inline) = arg.strip_prefix("--setenv=") {
                valued.push(inline.to_string());
            } else if arg == "--setenv"
                && let Some(next) = argv.get(at + 1)
                && next.contains('=')
            {
                valued.push(next.clone());
            }
        }
        valued
    }

    /// D-04, Pitfall 1: a name systemd would refuse, or one the manager owns, never
    /// reaches `--setenv`.
    #[test]
    fn passable_env_names_drops_invalid_and_manager_owned_names() {
        let names = [
            "BASH_FUNC_x%%",
            "1BAD",
            "NOTIFY_SOCKET",
            "INVOCATION_ID",
            "PATH",
            "DEVFLOW_GATE_NOTIFY_CMD",
        ]
        .map(OsString::from);
        let passable = passable_env_names(names);
        assert!(
            !passable.iter().any(|name| name.starts_with("BASH_FUNC_")),
            "kept BASH_FUNC_x%%: {passable:?}"
        );
        for dropped in ["1BAD", "NOTIFY_SOCKET", "INVOCATION_ID"] {
            assert!(
                !passable.iter().any(|name| name == dropped),
                "kept {dropped}: {passable:?}"
            );
        }
        assert_eq!(
            passable,
            ["DEVFLOW_GATE_NOTIFY_CMD", "PATH"],
            "control: valid caller names are kept"
        );
    }

    /// D-04, T-52.2-01: the unit argv passes environment by name only, so no value is
    /// on a command line `/proc` shows every user.
    #[test]
    fn runner_unit_argv_carries_env_by_name_only() {
        let dir = tempfile::tempdir().expect("create a temp dir");
        let names = ["PATH".to_string(), "DEVFLOW_GATE_NOTIFY_CMD".to_string()];
        let argv = runner_unit_argv(
            dir.path(),
            PhaseId::new(3),
            Path::new("/usr/bin/devflow"),
            &names,
        )
        .expect("build the argv");
        let valued = setenv_values(&argv);
        assert!(
            valued.is_empty(),
            "value on the command line after --setenv: {valued:?} in {argv:?}"
        );
        let by_name: Vec<String> = argv
            .windows(2)
            .filter(|pair| pair[0] == "--setenv")
            .map(|pair| pair[1].to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            by_name, names,
            "control: each name is passed after --setenv"
        );

        let mut planted = argv.clone();
        planted.insert(1, "--setenv=X=1".into());
        assert_eq!(
            setenv_values(&planted),
            ["X=1"],
            "control: a planted value after --setenv is detected"
        );
    }
}
