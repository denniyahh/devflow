//! Host-only tests of the foreground runner as a systemd user unit (D-01, D-04a, D-07).
//!
//! Every test here is `#[ignore]` and runs only under `scripts/check.sh service`, which
//! needs a running systemd user manager; the container suite has none. Each test's first
//! assertion is that `systemctl --user show -p Version --value` succeeds, so a host with
//! no manager fails the test instead of skipping it.
//!
//! `systemctl` and `systemd-run` get the caller's real `XDG_RUNTIME_DIR` and
//! `DBUS_SESSION_BUS_ADDRESS`, which they need to reach the user bus. The unit itself
//! gets the test's scratch `HOME`, `XDG_*`, `DEVFLOW_CACHE_DIR` and `PATH` as
//! `--setenv=NAME=value`: test values, not secrets. Every unit's name carries this test
//! process's pid, and a [`UnitGuard`] stops it by that recorded name, never by pattern.

mod common;

use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// How long a runner unit may take to finish a stubbed Plan and read inactive.
const UNIT_FINISH_WAIT: Duration = Duration::from_secs(60);

/// How often the unit's state is read while waiting.
const UNIT_POLL: Duration = Duration::from_millis(200);

/// The variables `systemctl` and `systemd-run` need to reach the user manager.
const USER_BUS_VARS: [&str; 2] = ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"];

/// A service-manager tool with only `PATH` and the user-bus variables from this process.
fn manager_tool(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env_clear();
    for name in USER_BUS_VARS.iter().chain(&["PATH"]) {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

/// `systemctl --user <args>`.
fn systemctl(args: &[&str]) -> Output {
    manager_tool("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .expect("spawn systemctl")
}

/// One property of `unit`, as `systemctl --user show -p NAME --value` prints it.
fn unit_property(unit: &str, name: &str) -> String {
    let out = systemctl(&["show", "-p", name, "--value", unit]);
    assert!(
        out.status.success(),
        "could not read {name} of {unit}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The first assertion of every test: the user manager answers.
fn assert_manager_answers() {
    let out = systemctl(&["show", "-p", "Version", "--value"]);
    assert!(
        out.status.success(),
        "no systemd user manager answers (systemctl --user show -p Version failed): {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The unit's recent journal, for a failure message.
fn journal_of(unit: &str) -> String {
    manager_tool("journalctl")
        .args(["--user", "--no-pager", "-n", "60", "-u", unit])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .unwrap_or_else(|err| format!("<journalctl failed: {err}>"))
}

/// Stops and resets one recorded unit when dropped, then checks it is gone.
struct UnitGuard {
    unit: String,
}

impl UnitGuard {
    /// A unit name unique to this test process and `phase`.
    fn for_phase(phase: PhaseId) -> Self {
        Self {
            unit: format!(
                "devflow-test-{}-{}.service",
                std::process::id(),
                phase.padded()
            ),
        }
    }
}

impl UnitGuard {
    /// A guard for `unit`, the name `start` derives for a phase (`service::unit_name`).
    fn named(unit: String) -> Self {
        Self { unit }
    }
}

/// How long [`UnitGuard`]'s drop waits for a stopped transient unit to be collected.
const UNIT_COLLECT_WAIT: Duration = Duration::from_secs(10);

impl Drop for UnitGuard {
    fn drop(&mut self) {
        let _ = systemctl(&["stop", &self.unit]);
        let _ = systemctl(&["reset-failed", &self.unit]);
        // The manager collects a stopped transient unit a moment after `stop` returns, so
        // one read can still see it `loaded`.
        let load_state = || {
            let out = systemctl(&["show", "-p", "LoadState", "--value", &self.unit]);
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let deadline = Instant::now() + UNIT_COLLECT_WAIT;
        let mut load_state_now = load_state();
        while load_state_now != "not-found" && Instant::now() < deadline {
            std::thread::sleep(UNIT_POLL);
            load_state_now = load_state();
        }
        let load_state = load_state_now;
        if std::thread::panicking() {
            if load_state != "not-found" {
                eprintln!("unit {} left with LoadState={load_state}", self.unit);
            }
        } else {
            assert_eq!(
                load_state, "not-found",
                "unit {} was not removed",
                self.unit
            );
        }
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A `develop`-only repo carrying `phase`'s CONTEXT.md and one PLAN.md, and a GSD config.
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    let padded = phase.padded();
    let dir = root.join(format!(".planning/phases/{padded}-test"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{padded}-CONTEXT.md")), "ctx\n").unwrap();
    fs::write(dir.join(format!("{padded}-01-PLAN.md")), "plan\n").unwrap();
    fs::write(
        root.join(".planning/config.json"),
        "{\n  \"workflow\": {\n    \"auto_advance\": false\n  }\n}\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["branch", "main"]);
}

/// A repository whose saved state for `phase` sits at Plan with `stop_until = Plan`,
/// driven by a stub `codex` that reports success, and the scratch home its unit uses.
struct RunnerFixture {
    root: PathBuf,
    phase: PhaseId,
    fake_bin: common::FakeBin,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the fixture.
    home: common::HermeticHome,
}

impl RunnerFixture {
    fn new(phase: PhaseId) -> Self {
        let repo = tempfile::tempdir().unwrap();
        // The binary canonicalizes `--project`, so the fixture must too.
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        let fake_bin = common::fake_bin_dir_for(
            "codex",
            "printf 'DEVFLOW_RESULT: {\"status\":\"success\"}\\n'",
        );
        let mut state = devflow_core::state::State::new(
            phase,
            devflow_core::state::AgentKind::Codex,
            devflow_core::mode::Mode::Supervise,
            root.clone(),
        );
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = false;
        devflow_core::workflow::save_state(&state).unwrap();
        Self {
            root,
            phase,
            fake_bin,
            _repo: repo,
            home: common::HermeticHome::new(),
        }
    }

    /// The `systemd-run` argv (after the program) that runs `devflow __runner` for this
    /// fixture as `unit`, with D-01's flags and the scratch environment by value.
    fn runner_unit_args(&self, unit: &str) -> Vec<OsString> {
        let mut args: Vec<OsString> = [
            "--user",
            "--collect",
            "-p",
            "Type=notify",
            "-p",
            "NotifyAccess=main",
            "-p",
            "KillMode=control-group",
            "-p",
            "TimeoutStopSec=10s",
            "-p",
            "TimeoutStartSec=30s",
            "--expand-environment=no",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        args.push(format!("--unit={unit}").into());
        let mut working = OsString::from("--working-directory=");
        working.push(&self.root);
        args.push(working);
        let path = format!("{}:/usr/bin:/bin", self.fake_bin.path.display());
        for (name, value) in [
            ("HOME", self.home.home().as_os_str().to_owned()),
            ("XDG_CACHE_HOME", self.home.home().join(".cache").into()),
            ("XDG_CONFIG_HOME", self.home.config_dir().into()),
            ("DEVFLOW_CACHE_DIR", self.home.cache_dir().into()),
            ("PATH", path.into()),
        ] {
            let mut setenv = OsString::from(format!("--setenv={name}="));
            setenv.push(value);
            args.push(setenv);
        }
        args.push("--".into());
        args.push(common::devflow_bin().into());
        args.push("__runner".into());
        args.push("--project".into());
        args.push(self.root.as_os_str().to_owned());
        args.push("--phase".into());
        args.push(self.phase.to_string().into());
        args
    }

    /// Start `unit` with `systemd-run`, which returns once the runner reports ready or
    /// the unit's start fails.
    fn start_unit(&self, unit: &str) -> Output {
        manager_tool("systemd-run")
            .args(self.runner_unit_args(unit))
            .output()
            .expect("spawn systemd-run")
    }
}

/// Wait until `unit` reads inactive, at most [`UNIT_FINISH_WAIT`]; its last state otherwise.
fn wait_inactive(unit: &str) -> Result<(), String> {
    let deadline = Instant::now() + UNIT_FINISH_WAIT;
    loop {
        let state = unit_property(unit, "ActiveState");
        if state == "inactive" {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(state);
        }
        std::thread::sleep(UNIT_POLL);
    }
}

/// Every event line of `kind` in the project's event log.
fn events_of(root: &Path, kind: &str) -> Vec<serde_json::Value> {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == kind)
        .collect()
}

/// D-01, D-02, D-04a, D-07: a unit running `devflow __runner` over a Plan-stage state
/// with `stop_until = Plan` starts (systemd-run returns 0 at READY), runs Plan in the
/// runner's own process, records Plan's verdict, and ends inactive with the runner lock
/// free. The one `stage_launched` event names Plan and the runner's pid.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn a_runner_unit_runs_plan_and_ends_inactive_with_the_lock_free() {
    assert_manager_answers();
    let phase = PhaseId::new(71);
    let fixture = RunnerFixture::new(phase);
    let guard = UnitGuard::for_phase(phase);

    let started = fixture.start_unit(&guard.unit);
    assert!(
        started.status.success(),
        "systemd-run failed: {}\njournal:\n{}",
        String::from_utf8_lossy(&started.stderr),
        journal_of(&guard.unit)
    );
    if let Err(state) = wait_inactive(&guard.unit) {
        panic!(
            "unit still {state} after {UNIT_FINISH_WAIT:?}\njournal:\n{}",
            journal_of(&guard.unit)
        );
    }

    let lock = devflow_core::liveness::runner_lock_path(&fixture.root, phase);
    assert_ne!(
        devflow_core::liveness::probe(&lock).expect("probe the runner lock"),
        devflow_core::liveness::Held::Live,
        "runner lock still held after the unit ended"
    );
    let state = devflow_core::workflow::load_state(&fixture.root, phase).unwrap();
    assert_eq!(
        (state.stage, state.stopped),
        (Stage::Plan, true),
        "the run halts after Plan as stop_until asks\njournal:\n{}",
        journal_of(&guard.unit)
    );
    assert!(
        state.stop_reason.is_some(),
        "the halt records its reason: {state:?}"
    );
    let evaluated = events_of(&fixture.root, "advance_evaluated");
    assert!(
        evaluated
            .iter()
            .any(|event| event["stage"] == "plan" && event["status"] == "success"),
        "Plan's verdict is recorded: {evaluated:?}"
    );
    let launched = events_of(&fixture.root, "stage_launched");
    assert!(
        launched.len() == 1
            && launched[0]["stage"] == "plan"
            && launched[0]["runner_pid"].as_u64().is_some(),
        "expected one stage_launched for plan, with its runner_pid: {launched:?}"
    );
}

/// D-04a, the negative control for the tracer above: with the runner lock already held
/// (by this test process, through the same claim the runner makes), the runner exits
/// before READY, so `systemd-run` under `Type=notify` reports the unit's start failed.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn a_runner_that_cannot_take_its_lock_fails_the_unit_start() {
    assert_manager_answers();
    let phase = PhaseId::new(72);
    let fixture = RunnerFixture::new(phase);
    let guard = UnitGuard::for_phase(phase);
    let _held = devflow_core::liveness::claim_runner_lock(&fixture.root, phase)
        .expect("the test takes the runner lock first");

    let started = fixture.start_unit(&guard.unit);
    let stderr = String::from_utf8_lossy(&started.stderr);
    assert!(
        !started.status.success(),
        "systemd-run reported success over a runner without its lock: {stderr}\njournal:\n{}",
        journal_of(&guard.unit)
    );
    assert!(
        stderr.contains(&guard.unit) && stderr.contains("failed"),
        "systemd-run's stderr names the unit failing: {stderr}"
    );
    assert!(
        events_of(&fixture.root, "stage_launched").is_empty(),
        "a runner without its lock launched a stage"
    );
}

/// The phase's unit name as the binary derives it (`service::unit_name`).
fn derived_unit_name(root: &Path, phase: PhaseId) -> String {
    format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(root),
        phase.padded()
    )
}

/// A repository with no run state for `phase`, a codex stub whose body is `agent_tail`
/// after it logs its argv, and the scratch home `devflow start` runs under.
struct StartFixture {
    root: PathBuf,
    phase: PhaseId,
    stubs: common::FakeBin,
    logs: tempfile::TempDir,
    _repo: tempfile::TempDir,
    /// Declared last so it drops after the rest of the fixture.
    home: common::HermeticHome,
}

impl StartFixture {
    fn new(phase: PhaseId, agent_tail: &str) -> Self {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        init_repo(&root, phase);
        let logs = tempfile::tempdir().unwrap();
        let stubs = common::capture_agent_bin_then(logs.path(), agent_tail);
        Self {
            root,
            phase,
            stubs,
            logs,
            _repo: repo,
            home: common::HermeticHome::new(),
        }
    }

    /// The phase's derived unit name.
    fn unit(&self) -> String {
        derived_unit_name(&self.root, self.phase)
    }

    /// `PATH` for a `start` child: `first` (when given), the stubs, then the system dirs.
    fn path(&self, first: Option<&Path>) -> String {
        let mut dirs: Vec<String> = first.iter().map(|dir| dir.display().to_string()).collect();
        dirs.push(self.stubs.path.display().to_string());
        dirs.push("/usr/bin:/bin".to_string());
        dirs.join(":")
    }

    /// `devflow start` (codex, supervise, no worktree) from `command`.
    fn start(&self, command: Command, path: &str) -> Output {
        self.start_with(command, path, &["--no-worktree"])
    }

    /// `devflow start` (codex, supervise) from `command`, with `extra` arguments.
    fn start_with(&self, mut command: Command, path: &str, extra: &[&str]) -> Output {
        command
            .env("PATH", path)
            .args([
                "start",
                "--phase",
                &self.phase.to_string(),
                "--agent",
                "codex",
                "--mode",
                "supervise",
            ])
            .args(extra)
            .arg(&self.root)
            .current_dir(&self.root)
            .output()
            .expect("run devflow start")
    }

    /// Whether the runner lock probes Live.
    fn runner_lock_live(&self) -> bool {
        devflow_core::liveness::probe(&devflow_core::liveness::runner_lock_path(
            &self.root, self.phase,
        ))
        .is_ok_and(|held| held == devflow_core::liveness::Held::Live)
    }

    /// Whether no launch reservation is left: no `.devflow/supervisor-*.live` file.
    fn reservation_free(&self) -> bool {
        common::no_reservation_file(&self.root)
    }
}

/// stdout and stderr of `out`, for failure messages.
fn text(out: &Output) -> String {
    format!(
        "status {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// D-01: `devflow start` launches the phase's unit through the real `systemd-run`,
/// prints the unit name and the `devflow status` and `devflow logs --phase N -f` lines,
/// and returns with the runner lock held.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn start_launches_a_unit_and_returns_with_the_run_live() {
    assert_manager_answers();
    let fixture = StartFixture::new(PhaseId::new(81), "sleep 120");
    let guard = UnitGuard::named(fixture.unit());

    let out = fixture.start(
        common::host_service_devflow(&fixture.home),
        &fixture.path(None),
    );
    assert!(
        out.status.success(),
        "start failed:\n{}\njournal:\n{}",
        text(&out),
        journal_of(&guard.unit)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&guard.unit)
            && stdout.contains("devflow status")
            && stdout.contains(&format!("devflow logs --phase {} -f", fixture.phase)),
        "start did not print the unit, status and logs lines:\n{}",
        text(&out)
    );
    assert!(
        fixture.runner_lock_live(),
        "lock not held after start returned:\n{}\njournal:\n{}",
        text(&out),
        journal_of(&guard.unit)
    );
}

/// D-07: `devflow status` reads the run live right after `start` returned with the
/// phase's unit running.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn status_reads_live_for_a_service_run_after_start_returned() {
    assert_manager_answers();
    let fixture = StartFixture::new(PhaseId::new(89), "sleep 120");
    let guard = UnitGuard::named(fixture.unit());

    let out = fixture.start(
        common::host_service_devflow(&fixture.home),
        &fixture.path(None),
    );
    assert!(
        out.status.success(),
        "start failed:\n{}\njournal:\n{}",
        text(&out),
        journal_of(&guard.unit)
    );
    let status = devflow_in(&fixture, &["status"]);
    let row = format!("phase {}: live", fixture.phase);
    assert!(
        status.status.success() && String::from_utf8_lossy(&status.stdout).contains(&row),
        "status not live:\n{}\njournal:\n{}",
        text(&status),
        journal_of(&guard.unit)
    );

    // Control: once the unit is stopped the same read no longer says live.
    let stopped = systemctl(&["stop", &guard.unit]);
    assert!(stopped.status.success(), "{}", text(&stopped));
    let status = devflow_in(&fixture, &["status"]);
    assert!(
        !String::from_utf8_lossy(&status.stdout).contains(&row),
        "control: status reads live after the unit stopped:\n{}",
        text(&status)
    );
}

/// D-04, review pi-2: a caller-only `PATH` entry and `DEVFLOW_TEST_ENV_PROBE` reach the
/// agent; `DEVFLOW_GATE_NOTIFY_CMD` reaches the runner and not the agent; a name
/// systemd would refuse is dropped and the launch still succeeds.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn the_agent_sees_the_callers_path_and_devflow_variables_but_not_an_invalid_name() {
    assert_manager_answers();
    let probe = tempfile::tempdir().unwrap();
    let env_file = probe.path().join("agent-env");
    let tail = format!(
        "env > '{file}.tmp' && mv '{file}.tmp' '{file}'\nsleep 120",
        file = env_file.display()
    );
    let fixture = StartFixture::new(PhaseId::new(82), &tail);
    let guard = UnitGuard::named(fixture.unit());
    let extra = tempfile::tempdir().unwrap();
    let notify = "true devflow-notify-probe";

    let mut command = common::host_service_devflow(&fixture.home);
    command
        .env("DEVFLOW_TEST_ENV_PROBE", "probe-value")
        .env("DEVFLOW_GATE_NOTIFY_CMD", notify)
        .env("BAD%NAME", "bad-value");
    let out = fixture.start(command, &fixture.path(Some(extra.path())));
    assert!(
        out.status.success(),
        "start failed:\n{}\njournal:\n{}",
        text(&out),
        journal_of(&guard.unit)
    );
    let deadline = Instant::now() + UNIT_FINISH_WAIT;
    while !env_file.is_file() {
        assert!(
            Instant::now() < deadline,
            "the agent never recorded its env\njournal:\n{}",
            journal_of(&guard.unit)
        );
        std::thread::sleep(UNIT_POLL);
    }
    let agent_env = fs::read_to_string(&env_file).unwrap();
    let lines: Vec<&str> = agent_env.lines().collect();
    let extra_dir = extra.path().display().to_string();
    assert!(
        lines.iter().any(|line| line.starts_with("PATH=")
            && line["PATH=".len()..].split(':').any(|dir| dir == extra_dir)),
        "agent missing PATH entry {extra_dir}:\n{agent_env}"
    );
    assert!(
        lines.contains(&"DEVFLOW_TEST_ENV_PROBE=probe-value"),
        "probe variable missing from the agent:\n{agent_env}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("DEVFLOW_GATE_NOTIFY_CMD=")),
        "notify command reached the agent:\n{agent_env}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("BAD%NAME=")),
        "invalid name reached the agent:\n{agent_env}"
    );
    let main_pid = unit_property(&guard.unit, "MainPID");
    let environ = fs::read(format!("/proc/{main_pid}/environ")).unwrap_or_default();
    let wanted = format!("DEVFLOW_GATE_NOTIFY_CMD={notify}");
    assert!(
        environ
            .split(|byte| *byte == 0)
            .any(|entry| entry == wanted.as_bytes()),
        "notify command missing from the runner (MainPID {main_pid})"
    );
}

/// D-04a's control against the real systemd-run (RESEARCH B3): `start` run from a copy
/// of devflow that is deleted before the launch exits non-zero naming the unit and
/// systemd-run's `Failed to find executable`, creates no unit and leaves no reservation.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn start_with_a_runner_path_that_no_longer_exists_fails_naming_the_unit_and_the_error() {
    assert_manager_answers();
    let fixture = StartFixture::new(PhaseId::new(83), "sleep 120");
    let guard = UnitGuard::named(fixture.unit());
    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join("devflow");
    fs::copy(common::devflow_bin(), &copy).expect("copy the devflow binary");
    let wrapper = scratch.path().join("wrapper");
    fs::create_dir(&wrapper).unwrap();
    let real_systemctl = ["/usr/bin/systemctl", "/bin/systemctl"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .expect("a real systemctl");
    let script = wrapper.join("systemctl");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\n/usr/bin/rm -f '{}'\nexec {real_systemctl} \"$@\"\n",
            copy.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let out = fixture.start(
        common::host_service_devflow_from(&fixture.home, &copy),
        &fixture.path(Some(&wrapper)),
    );
    assert!(
        !out.status.success(),
        "missing runner path not reported: start exited 0:\n{}",
        text(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&guard.unit) && stderr.contains("Failed to find executable"),
        "missing runner path not reported:\n{}",
        text(&out)
    );
    assert_eq!(
        unit_property(&guard.unit, "LoadState"),
        "not-found",
        "a unit was created:\n{}",
        text(&out)
    );
    assert!(
        fixture.reservation_free(),
        "reservation left after a failed start:\n{}",
        text(&out)
    );
    let _ = &fixture.logs;
}

/// Run `devflow <args>` in `fixture`'s root under the real user manager, with the
/// stubs first on `PATH`.
fn devflow_in(fixture: &StartFixture, args: &[&str]) -> Output {
    common::host_service_devflow(&fixture.home)
        .env("PATH", fixture.path(None))
        .args(args)
        .current_dir(&fixture.root)
        .output()
        .expect("run devflow")
}

/// Wait until `ready` holds, at most [`UNIT_FINISH_WAIT`], panicking with `what` and
/// `unit`'s journal otherwise.
fn wait_until(mut ready: impl FnMut() -> bool, unit: &str, what: &str) {
    let deadline = Instant::now() + UNIT_FINISH_WAIT;
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "{what} after {UNIT_FINISH_WAIT:?}\njournal:\n{}",
            journal_of(unit)
        );
        std::thread::sleep(UNIT_POLL);
    }
}

/// D-01, D-05: a run stopped with `systemctl --user stop` records stopped; `devflow
/// resume` launches it as a new unit, which runs Plan, Code and Validate and finishes
/// once the Validate gate is approved.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn a_stopped_run_resumes_as_a_unit_and_finishes() {
    assert_manager_answers();
    let release = tempfile::tempdir().unwrap();
    let released = release.path().join("released");
    let tail = format!(
        "if [ -e '{released}' ]; then\n\
         git commit -q --allow-empty -m \"stub stage work\"\n\
         printf 'DEVFLOW_RESULT: {{\"status\":\"success\",\"commits\":1,\"verdict\":\"pass\"}}\\n'\n\
         else sleep 120; fi",
        released = released.display()
    );
    let phase = PhaseId::new(84);
    let fixture = StartFixture::new(phase, &tail);
    let passed =
        devflow_core::test_support::gsd_status_json(devflow_core::gsd_reader::GsdStatus::Passed);
    let gsd_logs = tempfile::tempdir().unwrap();
    common::stub_gsd_run(&fixture.home, &[passed; 16], gsd_logs.path());
    let guard = UnitGuard::named(fixture.unit());

    let out = fixture.start(
        common::host_service_devflow(&fixture.home),
        &fixture.path(None),
    );
    assert!(out.status.success(), "start failed:\n{}", text(&out));
    let argv_log = fixture.logs.path().join(common::AGENT_ARGV_LOG);
    wait_until(
        || argv_log.is_file(),
        &guard.unit,
        "the Plan agent never started",
    );
    let stopped = systemctl(&["stop", &guard.unit]);
    assert!(
        stopped.status.success(),
        "systemctl stop failed: {}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    let state = devflow_core::workflow::load_state(&fixture.root, phase).unwrap();
    assert!(
        state.stopped && !state.finished,
        "the stopped unit did not record stopped: {state:?}\njournal:\n{}",
        journal_of(&guard.unit)
    );

    fs::write(&released, "").unwrap();
    let number = phase.to_string();
    let root = fixture.root.display().to_string();
    let resumed = devflow_in(&fixture, &["resume", "--phase", &number, &root]);
    assert!(
        resumed.status.success() && String::from_utf8_lossy(&resumed.stdout).contains(&guard.unit),
        "resume did not launch a unit:\n{}",
        text(&resumed)
    );
    let gate = devflow_core::gates::Gates::gate_path(&fixture.root, phase, Stage::Validate);
    wait_until(
        || gate.exists(),
        &guard.unit,
        "resumed run did not finish: no Validate gate",
    );
    let approved = devflow_in(&fixture, &["gate", "approve", &number, "--project", &root]);
    assert!(
        approved.status.success(),
        "gate approve failed:\n{}",
        text(&approved)
    );
    if let Err(state) = wait_inactive(&guard.unit) {
        panic!(
            "resumed run did not finish: unit still {state}\njournal:\n{}",
            journal_of(&guard.unit)
        );
    }
    let state = devflow_core::workflow::load_state(&fixture.root, phase).unwrap();
    assert!(
        state.finished,
        "resumed run did not finish: {state:?}\njournal:\n{}",
        journal_of(&guard.unit)
    );
}

/// Whether a process or process group answers signal 0 (`target` < 0 is a group).
fn signal_zero_finds(target: libc::pid_t) -> bool {
    // SAFETY: signal 0 delivers nothing; it only checks for existence.
    unsafe { libc::kill(target, 0) == 0 }
}

/// The pid the stub agent wrote to `file`.
fn recorded_pid(file: &Path) -> libc::pid_t {
    fs::read_to_string(file)
        .expect("the agent recorded its pid")
        .trim()
        .parse()
        .expect("a pid")
}

/// D-05: `devflow stop` on a live unit exits 0, prints stopped, the state reads
/// stopped and the unit inactive.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn stop_on_a_live_unit_records_stopped() {
    assert_manager_answers();
    let phase = PhaseId::new(85);
    let fixture = StartFixture::new(phase, "sleep 120");
    let guard = UnitGuard::named(fixture.unit());
    let out = fixture.start(
        common::host_service_devflow(&fixture.home),
        &fixture.path(None),
    );
    assert!(out.status.success(), "start failed:\n{}", text(&out));
    let argv_log = fixture.logs.path().join(common::AGENT_ARGV_LOG);
    wait_until(
        || argv_log.is_file(),
        &guard.unit,
        "the agent never started",
    );

    let number = phase.to_string();
    let root = fixture.root.display().to_string();
    let stopped = devflow_in(&fixture, &["stop", "--phase", &number, &root]);
    assert!(
        stopped.status.success() && String::from_utf8_lossy(&stopped.stdout).contains("stopped"),
        "state not stopped: stop failed:\n{}\njournal:\n{}",
        text(&stopped),
        journal_of(&guard.unit)
    );
    let state = devflow_core::workflow::load_state(&fixture.root, phase).unwrap();
    assert!(
        state.stopped && !state.finished,
        "state not stopped: {state:?}\njournal:\n{}",
        journal_of(&guard.unit)
    );
    assert_eq!(
        unit_property(&guard.unit, "ActiveState"),
        "inactive",
        "the unit is not inactive after stop"
    );
}

/// D-08: SIGKILL to the unit's MainPID leaves no member of the agent's group, and the
/// unit inactive: the manager's control-group kill ends the agent. No orphan-agent
/// check is involved.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn a_hard_killed_runner_leaves_no_agent_and_the_unit_inactive() {
    assert_manager_answers();
    let probe = tempfile::tempdir().unwrap();
    let agent_pid = probe.path().join("agent.pid");
    let tail = format!("echo $$ > '{}'\nsleep 120", agent_pid.display());
    let phase = PhaseId::new(86);
    let fixture = StartFixture::new(phase, &tail);
    let guard = UnitGuard::named(fixture.unit());
    let out = fixture.start(
        common::host_service_devflow(&fixture.home),
        &fixture.path(None),
    );
    assert!(out.status.success(), "start failed:\n{}", text(&out));
    wait_until(
        || agent_pid.is_file(),
        &guard.unit,
        "the agent never started",
    );
    let agent = recorded_pid(&agent_pid);
    assert!(
        signal_zero_finds(-agent),
        "the agent's group is not running"
    );

    let main_pid: libc::pid_t = unit_property(&guard.unit, "MainPID")
        .parse()
        .expect("a MainPID");
    assert!(main_pid > 1, "the unit has no main process");
    // SAFETY: the unit's main process, read from the manager just now.
    unsafe { libc::kill(main_pid, libc::SIGKILL) };

    if let Err(state) = wait_inactive(&guard.unit) {
        panic!(
            "unit still {state} after the hard kill\njournal:\n{}",
            journal_of(&guard.unit)
        );
    }
    wait_until(
        || !signal_zero_finds(-agent),
        &guard.unit,
        "agent survived the hard kill",
    );
}

/// D-07a: a stub agent leaves a SIGTERM-ignoring child, so after `devflow stop` sends
/// SIGTERM the unit reads deactivating until the manager's SIGKILL. A `devflow discard`
/// started meanwhile removes nothing until the unit reads inactive, then removes the
/// run's state.
#[test]
#[ignore = "needs a systemd user manager; run by scripts/check.sh service"]
fn discard_waits_for_a_straggler_in_a_deactivating_unit() {
    assert_manager_answers();
    let tail = "setsid sh -c 'trap \"\" TERM; sleep 60' </dev/null >/dev/null 2>&1 &\nsleep 120";
    let phase = PhaseId::new(87);
    let fixture = StartFixture::new(phase, tail);
    let guard = UnitGuard::named(fixture.unit());
    // In a worktree, so discard can remove the branch: `--no-worktree` checks it out in
    // the main checkout, which discard refuses.
    let out = fixture.start_with(
        common::host_service_devflow(&fixture.home),
        &fixture.path(None),
        &[],
    );
    assert!(out.status.success(), "start failed:\n{}", text(&out));
    let argv_log = fixture.logs.path().join(common::AGENT_ARGV_LOG);
    wait_until(
        || argv_log.is_file(),
        &guard.unit,
        "the agent never started",
    );
    std::thread::sleep(Duration::from_millis(500));

    let number = phase.to_string();
    let root = fixture.root.display().to_string();
    let mut stop = common::host_service_devflow(&fixture.home)
        .env("PATH", fixture.path(None))
        .args(["stop", "--phase", &number, &root])
        .current_dir(&fixture.root)
        .spawn()
        .expect("spawn devflow stop");
    wait_until(
        || unit_property(&guard.unit, "ActiveState") == "deactivating",
        &guard.unit,
        "the unit never read deactivating",
    );
    let state_file = devflow_core::workflow::state_path(&fixture.root, phase);
    let mut discard = common::host_service_devflow(&fixture.home)
        .env("PATH", fixture.path(None))
        .args(["discard", "--phase", &number, &root])
        .current_dir(&fixture.root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn devflow discard");
    while unit_property(&guard.unit, "ActiveState") == "deactivating" {
        assert!(
            state_file.is_file(),
            "discard acted while deactivating: the state is gone"
        );
        std::thread::sleep(UNIT_POLL);
    }
    let _ = stop.wait();
    let deadline = Instant::now() + UNIT_FINISH_WAIT;
    while discard.try_wait().expect("poll discard").is_none() {
        assert!(Instant::now() < deadline, "discard never returned");
        std::thread::sleep(UNIT_POLL);
    }
    let discarded = discard.wait_with_output().expect("discard's output");
    assert!(
        discarded.status.success() && !state_file.exists(),
        "discard did not complete once the unit read inactive:\n{}",
        text(&discarded)
    );
}
