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
        match self.kind {
            ManagerUnavailableKind::Absent => format!(
                "note: {}; phase {phase} checked by its runner lock only",
                self.reason()
            ),
            ManagerUnavailableKind::Unreachable | ManagerUnavailableKind::Failed => self.reason(),
        }
    }

    /// Why no manager answered, systemctl's stderr included (agy-6): what [`Self::describe`]
    /// prints, without the Absent note's runner-lock-only part.
    pub(crate) fn reason(&self) -> String {
        let runtime = self
            .runtime_dir
            .as_deref()
            .map(|dir| Path::new(dir).display().to_string());
        match self.kind {
            ManagerUnavailableKind::Absent => format!(
                "no systemd user manager runs for this user (no {}/systemd/private; \
                 systemctl: {})",
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
        /// Whether a launch runs the whole runner rather than an idle lock holder.
        full_runner: bool,
        /// Every runner launch on this thread: the unit and its runner child.
        launches: Vec<(String, std::process::Child)>,
        /// Children [`register_stop_target`] gave a unit without a launch.
        stop_targets: Vec<(String, std::process::Child)>,
        /// How many times the cfg(test) [`super::stop_unit`] ran on this thread.
        stop_calls: usize,
    }

    thread_local! {
        static DOUBLE: RefCell<Double> = RefCell::new(Double {
            manager: Ok(()),
            unit_state: Ok("inactive".to_string()),
            unit_reads: 0,
            full_runner: false,
            launches: Vec::new(),
            stop_targets: Vec::new(),
            stop_calls: 0,
        });
    }

    /// Make later launches on this thread run the whole runner (`true`), or only hold
    /// the runner lock (`false`, the default).
    pub(crate) fn set_full_runner(full: bool) {
        DOUBLE.with_borrow_mut(|double| double.full_runner = full);
    }

    pub(super) fn full_runner() -> bool {
        DOUBLE.with_borrow(|double| double.full_runner)
    }

    pub(super) fn record_launch(unit: &str, child: std::process::Child) {
        DOUBLE.with_borrow_mut(|double| double.launches.push((unit.to_string(), child)));
    }

    /// The units launched on this thread, in order.
    pub(crate) fn launched_units() -> Vec<String> {
        DOUBLE.with_borrow(|double| {
            double
                .launches
                .iter()
                .map(|(unit, _)| unit.clone())
                .collect()
        })
    }

    /// Whether the launched runner child `pid` has not exited (reaping it if it has).
    pub(super) fn runner_running(pid: u32) -> bool {
        DOUBLE.with_borrow_mut(|double| {
            double
                .launches
                .iter_mut()
                .find(|(_, child)| child.id() == pid)
                .is_some_and(|(_, child)| matches!(child.try_wait(), Ok(None)))
        })
    }

    /// Record `child` as a process the cfg(test) [`super::stop_unit`] of `unit` ends, as
    /// it ends a launched runner, without counting a launch: [`launched_units`] is
    /// unchanged. For a test whose lock-holding child stands in for a unit's runner.
    pub(crate) fn register_stop_target(unit: &str, child: std::process::Child) {
        DOUBLE.with_borrow_mut(|double| double.stop_targets.push((unit.to_string(), child)));
    }

    pub(super) fn record_stop() {
        DOUBLE.with_borrow_mut(|double| double.stop_calls += 1);
    }

    /// How many times the cfg(test) [`super::stop_unit`] ran on this thread.
    pub(crate) fn stop_calls() -> usize {
        DOUBLE.with_borrow(|double| double.stop_calls)
    }

    /// End every runner child launched, or registered by [`register_stop_target`], on
    /// this thread for `unit` (every unit when `None`): SIGTERM, a bounded wait for its
    /// exit, then SIGKILL, then reap.
    pub(crate) fn end_runners(unit: Option<&str>, wait: std::time::Duration) {
        DOUBLE.with_borrow_mut(|double| {
            let children = double
                .launches
                .iter_mut()
                .chain(double.stop_targets.iter_mut());
            for (launched, child) in children {
                if unit.is_some_and(|unit| unit != launched) {
                    continue;
                }
                if !matches!(child.try_wait(), Ok(None)) {
                    continue;
                }
                if let Ok(pid) = libc::pid_t::try_from(child.id()) {
                    // SAFETY: `pid` is this thread's own unreaped child.
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                }
                let deadline = std::time::Instant::now() + wait;
                while matches!(child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                }
                let _ = child.wait();
            }
        });
    }

    /// Ends this thread's launched runners when dropped. Bind it after the test's
    /// tempdirs, so it drops first and no runner writes under a removed root.
    pub(crate) struct LaunchedRunners;

    impl Drop for LaunchedRunners {
        fn drop(&mut self) {
            end_runners(None, super::TIMEOUT_STOP_SEC);
        }
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

/// How long the cfg(test) [`launch_runner_unit`] waits for its runner child to hold
/// the runner lock: the unit's `TimeoutStartSec`.
#[cfg(test)]
const TEST_RUNNER_READY_WAIT: Duration = Duration::from_secs(30);

/// Launch `devflow __runner` for `phase` of `project_root` as the phase's user unit
/// (D-01): [`runner_unit_argv`] with every [`passable_env_names`] name of this
/// process's environment, run by `systemd-run`, which under `Type=notify` returns once
/// the runner holds its lock and reports ready, or once the start failed (D-04a).
///
/// # Errors
/// The unit name, the executable or the argv could not be built, `systemd-run` could
/// not run, or it exited non-zero: text naming the unit and carrying systemd-run's
/// stderr.
#[cfg(not(test))]
pub(crate) fn launch_runner_unit(project_root: &Path, phase: PhaseId) -> Result<String, String> {
    let unit = unit_name(project_root, phase)
        .map_err(|error| format!("phase {phase}'s unit name could not be derived ({error})"))?;
    let exe = std::env::current_exe().map_err(|error| {
        format!("unit {unit}: the devflow executable could not be resolved ({error})")
    })?;
    let names = passable_env_names(std::env::vars_os().map(|(name, _)| name));
    let argv = runner_unit_argv(project_root, phase, &exe, &names).map_err(|error| {
        format!("unit {unit}: the systemd-run argv could not be built ({error})")
    })?;
    let output = tool("systemd-run")
        .args(argv)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("unit {unit}: systemd-run could not run ({error})"))?;
    if output.status.success() {
        return Ok(unit);
    }
    Err(format!(
        "systemd-run could not start unit {unit} ({}); systemd-run: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// The variable the cfg(test) [`launch_runner_unit`] passes its runner child the
/// project root and phase in, as a JSON `[root, phase]` pair.
#[cfg(test)]
const RUNNER_HARNESS_ARGS_ENV: &str = "DF_TEST_RUNNER_ARGS";

/// `1` on the runner child when [`double::set_full_runner`] asked for the whole runner.
#[cfg(test)]
const RUNNER_HARNESS_FULL_ENV: &str = "DF_TEST_RUNNER_FULL";

/// The libtest name of the runner harness entry the cfg(test) launch runs.
#[cfg(test)]
pub(crate) const RUNNER_HARNESS_ENTRY: &str = "service::tests::runner_harness::entry";

/// The cfg(test) build (D-07b): run [`RUNNER_HARNESS_ENTRY`] as a direct child with no
/// `systemd-run`, record the launch and the child in the [`double`], and wait, as
/// `Type=notify` does, until the runner lock names the child. By default the child takes
/// the runner lock and runs no stage, as a unit whose runner has just started, so a test
/// reads what the launching verb wrote with nothing racing it; after
/// [`double::set_full_runner`] it runs `run_runner`. The child gets SIGTERM if the
/// launching thread exits first.
///
/// # Errors
/// The child could not be spawned, it exited before it held the lock, or it did not
/// hold it within [`TEST_RUNNER_READY_WAIT`].
#[cfg(test)]
pub(crate) fn launch_runner_unit(project_root: &Path, phase: PhaseId) -> Result<String, String> {
    use std::os::unix::process::CommandExt as _;
    let unit = unit_name(project_root, phase)
        .map_err(|error| format!("phase {phase}'s unit name could not be derived ({error})"))?;
    let root = project_root
        .canonicalize()
        .map_err(|error| format!("unit {unit}: {error}"))?;
    let exe = std::env::current_exe().map_err(|error| format!("unit {unit}: {error}"))?;
    let args = serde_json::json!([root, phase.to_string()]).to_string();
    let mut command = Command::new(exe);
    command
        .args([
            "--exact",
            RUNNER_HARNESS_ENTRY,
            "--ignored",
            "--test-threads=1",
        ])
        .env(RUNNER_HARNESS_ARGS_ENV, args)
        .env(
            RUNNER_HARNESS_FULL_ENV,
            if double::full_runner() { "1" } else { "0" },
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: prctl is async-signal-safe and touches no memory of the parent.
    unsafe {
        command.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command
        .spawn()
        .map_err(|error| format!("unit {unit}: the runner child could not start ({error})"))?;
    let pid = child.id();
    double::record_launch(&unit, child);
    let lock = devflow_core::liveness::runner_lock_path(&root, phase);
    let deadline = Instant::now() + TEST_RUNNER_READY_WAIT;
    loop {
        if devflow_core::liveness::live_holder(&lock)
            .ok()
            .flatten()
            .is_some_and(|(holder, _)| holder == pid)
        {
            return Ok(unit);
        }
        if !double::runner_running(pid) {
            return Err(format!(
                "unit {unit}: the runner exited before it held its runner lock"
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "unit {unit}: the runner did not hold its runner lock within {} s",
                TEST_RUNNER_READY_WAIT.as_secs()
            ));
        }
        std::thread::sleep(BOUNDED_CALL_POLL);
    }
}

/// D-05: stop `unit` with `systemctl --user stop`, which returns once the unit is down:
/// the runner's own stop, or the manager's SIGKILL at [`TIMEOUT_STOP_SEC`]. Bounded by
/// [`UNIT_STOP_WAIT`] plus [`SYSTEMCTL_CALL_BOUND`].
///
/// # Errors
/// systemctl failed, exited non-zero or did not return within its bound.
#[cfg(not(test))]
pub(crate) fn stop_unit(unit: &str) -> Result<(), String> {
    let mut command = tool("systemctl");
    command.args(["--user", "stop", unit]);
    match run_bounded(command, UNIT_STOP_WAIT + SYSTEMCTL_CALL_BOUND) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!(
            "`systemctl --user stop {unit}` failed ({}); systemctl: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Err(format!(
            "`systemctl --user stop {unit}` did not return: {error}"
        )),
    }
}

/// The cfg(test) build: count the call, then end the runner child the cfg(test)
/// [`launch_runner_unit`] recorded for `unit` (or a child
/// [`double::register_stop_target`] gave it), SIGTERM, a wait of [`TIMEOUT_STOP_SEC`],
/// then SIGKILL, as the manager does. Never reaches [`tool`].
#[cfg(test)]
pub(crate) fn stop_unit(unit: &str) -> Result<(), String> {
    double::record_stop();
    double::end_runners(Some(unit), TIMEOUT_STOP_SEC);
    Ok(())
}

/// D-04c: whether lingering is on for this user
/// (`loginctl show-user <uid> -p Linger --value`). Without it the user manager, and
/// every run under it, ends at logout.
///
/// # Errors
/// loginctl failed, exited non-zero or timed out: its stderr or the failure.
#[cfg(not(test))]
pub(crate) fn lingering_enabled() -> Result<bool, String> {
    // SAFETY: getuid takes no arguments, cannot fail and touches no memory.
    let uid = unsafe { libc::getuid() };
    let mut command = tool("loginctl");
    command.args(["show-user", &uid.to_string(), "-p", "Linger", "--value"]);
    match run_bounded(command, SYSTEMCTL_CALL_BOUND) {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim() == "yes")
        }
        Ok(output) => Err(format!(
            "loginctl failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Err(format!("loginctl did not answer: {error}")),
    }
}

/// The cfg(test) build: lingering reads on, so no unit test prints the logout warning.
#[cfg(test)]
pub(crate) fn lingering_enabled() -> Result<bool, String> {
    Ok(true)
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

    /// The libtest entry the cfg(test) [`launch_runner_unit`] runs for the root and phase
    /// in [`RUNNER_HARNESS_ARGS_ENV`]: `run_runner`, exiting with its code, when
    /// [`RUNNER_HARNESS_FULL_ENV`] is `1`; otherwise it takes the runner lock and holds it
    /// until it is signalled. A no-op without the variable, so a run of ignored tests that
    /// reaches it does nothing.
    mod runner_harness {
        #[test]
        #[ignore = "runner re-exec entry; run only by the cfg(test) launch_runner_unit"]
        fn entry() {
            let Some(args) = std::env::var_os(super::super::RUNNER_HARNESS_ARGS_ENV) else {
                return;
            };
            let (root, phase): (std::path::PathBuf, String) =
                serde_json::from_str(&args.to_string_lossy()).expect("the runner harness args");
            let phase = phase.parse().expect("the runner harness phase");
            if std::env::var_os(super::super::RUNNER_HARNESS_FULL_ENV).is_some_and(|v| v == "1") {
                let code = match crate::pipeline_launch::run_runner(&root, phase) {
                    Ok(code) => code,
                    Err(error) => {
                        eprintln!("{error}");
                        1
                    }
                };
                std::process::exit(code);
            }
            let _lock = devflow_core::liveness::claim_runner_lock(&root, phase)
                .expect("the test runner takes its lock");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }

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

    /// The kind [`classify_unanswered`] gives `runtime_dir` with `own` as this user's
    /// runtime dir.
    fn kind_of(runtime_dir: Option<&OsStr>, own: &Path) -> ManagerUnavailableKind {
        classify_unanswered(runtime_dir, own, String::new()).kind
    }

    /// Review codex-2/pi-8, r3 codex-1/pi-5: an unanswered Version probe reads Absent
    /// only under this user's own runtime dir with no `systemd/private`. The wrong-dir
    /// row W is round 3's control (round 2's rule read it Absent); the socket row and
    /// the ENOTDIR row are the controls for absence (a sort reading any failed stat as
    /// absence fails them).
    #[test]
    fn an_unanswered_manager_reads_absent_only_without_its_socket() {
        use ManagerUnavailableKind::{Absent, Failed, Unreachable};
        let own = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let own_path = own.path();
        let mut trailing = own_path.as_os_str().to_os_string();
        trailing.push("/");
        let rows: [(&str, Option<&OsStr>, ManagerUnavailableKind); 5] = [
            ("unset", None, Unreachable),
            ("relative", Some(OsStr::new("run/user/1000")), Unreachable),
            ("own", Some(own_path.as_os_str()), Absent),
            ("own with a trailing /", Some(trailing.as_os_str()), Absent),
            (
                "W: another empty absolute dir",
                Some(other.path().as_os_str()),
                Unreachable,
            ),
        ];
        for (label, runtime_dir, expected) in rows {
            assert_eq!(kind_of(runtime_dir, own_path), expected, "row `{label}`");
        }

        std::fs::create_dir(own_path.join("systemd")).unwrap();
        std::fs::write(own_path.join("systemd").join("private"), "").unwrap();
        assert_eq!(
            kind_of(Some(own_path.as_os_str()), own_path),
            Failed,
            "row `own holding systemd/private`"
        );
        let file = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(
            kind_of(Some(file.path().as_os_str()), file.path()),
            Failed,
            "row `a regular file, whose socket stat fails ENOTDIR`"
        );
    }

    /// D-07b, the seam's control: a CLI unit test that builds a service tool panics,
    /// naming the tool and the double, so a cfg(test) path that reached a real tool
    /// fails rather than passes.
    #[test]
    fn a_unit_test_that_reaches_a_service_tool_panics() {
        let panic = std::panic::catch_unwind(|| tool("systemctl"))
            .expect_err("tool(\"systemctl\") returned a Command in a unit test");
        let text = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_string()))
            .unwrap_or_default();
        assert!(
            text.contains("systemctl") && text.contains("service test double"),
            "the panic names the tool and the double: {text:?}"
        );
    }

    /// Codex-7: a child still running at its deadline is killed and reaped, and the
    /// call is a TimedOut error.
    #[test]
    fn a_bounded_call_kills_and_reaps_a_child_past_its_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("echo $$ > \"$1\"; exec sleep 30")
            .arg("sh")
            .arg(&pid_file);
        let started = Instant::now();
        let error = run_bounded(command, Duration::from_millis(200))
            .expect_err("child outlived its deadline: run_bounded returned its output");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut, "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "child outlived its deadline: the call took {:?}",
            started.elapsed()
        );
        let pid: libc::pid_t = std::fs::read_to_string(&pid_file)
            .expect("the child recorded its pid")
            .trim()
            .parse()
            .expect("a pid");
        // SAFETY: signal 0 only checks that `pid` exists; nothing is delivered.
        let found = unsafe { libc::kill(pid, 0) };
        let errno = std::io::Error::last_os_error().raw_os_error();
        assert!(
            found == -1 && errno == Some(libc::ESRCH),
            "child not reaped: kill({pid}, 0) returned {found} (errno {errno:?})"
        );
    }

    /// The control for the kill: a child that exits in time keeps its output.
    #[test]
    fn a_bounded_call_returns_the_output_of_a_child_that_exits_in_time() {
        let mut command = Command::new("echo");
        command.arg("inactive");
        let output = run_bounded(command, Duration::from_secs(5)).expect("in-time output lost");
        assert!(output.status.success(), "in-time output lost: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "inactive",
            "in-time output lost"
        );
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
