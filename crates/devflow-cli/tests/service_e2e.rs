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

impl Drop for UnitGuard {
    fn drop(&mut self) {
        let _ = systemctl(&["stop", &self.unit]);
        let _ = systemctl(&["reset-failed", &self.unit]);
        let load_state = systemctl(&["show", "-p", "LoadState", "--value", &self.unit]);
        let load_state = String::from_utf8_lossy(&load_state.stdout)
            .trim()
            .to_string();
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
