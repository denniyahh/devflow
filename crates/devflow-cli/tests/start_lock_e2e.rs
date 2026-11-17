//! End-to-end coverage for `start`'s launch reservation.
//!
//! These tests deliberately hold a reservation's liveness lock from the test
//! process, then drive the compiled CLI as a separate process. That makes the
//! refusal observable at the same boundary an operator uses, rather than coupling
//! the test to `commands::start`'s implementation details.
//!
//! Every `devflow` child runs through `common::hermetic_devflow`, so its
//! registry entries land in a tempdir `HOME`, never the operator's real
//! `~/.cache/devflow` (999.141).

mod common;

use common::HermeticPath;
use devflow_core::gates::{GateResponse, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

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

fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    fs::create_dir_all(root.join(".planning")).unwrap();
    fs::write(root.join("README.md"), "base\n").unwrap();
    fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: lock fixture\n"),
    )
    .unwrap();
    let context = common::write_phase_context(root, phase);
    git(
        root,
        &["add", "README.md", ".planning/ROADMAP.md", &context],
    );
    git(root, &["commit", "-q", "-m", "base"]);
}

/// A fake `codex`, the capture-shape agent these launches use, whose body is
/// `agent_body`.
fn fake_codex(agent_body: &str) -> common::FakeBin {
    common::fake_bin_dir_for("codex", agent_body)
}

fn child(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    mode: &str,
) -> Command {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let mut command = common::hermetic_devflow(home);
    command
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "codex",
            "--mode",
            mode,
            "--no-worktree",
        ])
        .arg(root)
        .current_dir(root)
        .hermetic_path(path);
    command
}

#[test]
fn start_proceeds_when_no_reservation_is_held() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(91);
    init_repo(root, phase);
    let fake_bin = fake_codex("exit 0");
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);

    let output = child(&home, root, phase, &fake_bin, "supervise")
        .output()
        .expect("run devflow start");
    assert!(
        output.status.success(),
        "uncontended start failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let state = root.join(format!(".devflow/state-{}.json", phase.padded()));
    let events = devflow_core::events::events_path(root);
    assert!(
        state.exists()
            || fs::read_to_string(events)
                .unwrap_or_default()
                .contains("\"workflow_started\""),
        "an uncontended start must write state or its workflow_started event"
    );
    assert!(
        common::no_reservation_file(root),
        "a finished start leaves no reservation"
    );
}

#[test]
fn start_clears_leftover_gate_files_after_reserving() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(92);
    init_repo(root, phase);
    let fake_bin = fake_codex("sleep 60");
    // A legacy unkeyed answer: the one form no gate id can keep from a new gate.
    let response_path = Gates::dir(root).join(format!("{}-plan.response.json", phase.padded()));
    fs::create_dir_all(response_path.parent().unwrap()).unwrap();
    let planted = serde_json::to_vec(&GateResponse {
        approved: false,
        abort: true,
        note: Some("abort: planted stale response".into()),
        responded_by: Some("start-lock-e2e-planted-response".into()),
    })
    .unwrap();
    fs::write(&response_path, &planted).unwrap();
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);

    let output = child(&home, root, phase, &fake_bin, "auto")
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "30")
        .output()
        .expect("run devflow start");
    assert!(
        output.status.success(),
        "start failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    common::wait_for(
        || Gates::gate_path(root, phase, Stage::Plan).exists() || !runner_lock_live(root, phase),
        Duration::from_secs(10),
        "Plan gate or runner exit",
    );
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        runner_lock_live(root, phase),
        "planted response must not abort a fresh start's run"
    );
    assert!(
        root.join(format!(".devflow/state-{}.json", phase.padded()))
            .exists(),
        "a fresh start's live run must keep state"
    );
    let events = fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
    assert!(
        !events.contains("start-lock-e2e-planted-response"),
        "the planted response must not resolve the fresh gate\nevents:\n{events}"
    );
    assert!(
        !response_path.exists(),
        "the planted response must be gone from its old name"
    );
    let set_aside: Vec<_> = Gates::quarantined(root)
        .into_iter()
        .filter(|answer| fs::read(&answer.path).is_ok_and(|bytes| bytes == planted))
        .collect();
    assert_eq!(
        set_aside.len(),
        1,
        "a quarantine file must hold the planted response's bytes: {:?}",
        Gates::quarantined(root)
    );
}

/// D-06, at the boundary an operator reads: `devflow resume` prints the answer it set
/// aside. The route is the Auto refusal that opens the rerun's gate in the foreground, so
/// nothing is left running, and the gate's short timeout is what ends the command.
#[test]
fn resume_prints_the_answer_it_set_aside() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(93);
    init_repo(root, phase);
    let fake_bin = fake_codex("exit 0");
    let planted = plant_leftover_run(root, phase);
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());

    let output = common::hermetic_devflow(&home)
        .args(["resume", "--phase", &phase.to_string()])
        .arg(root)
        .current_dir(root)
        .hermetic_path(path)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "1")
        .output()
        .expect("run devflow resume");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !planted.response.exists(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    let set_aside = Gates::quarantined(root);
    assert_eq!(set_aside.len(), 1, "stdout: {stdout}\nstderr: {stderr}");
    let line = format!(
        "resume: set aside an unused answer for the code gate (gate {}): {} — the rerun asks again",
        set_aside[0].label,
        set_aside[0].path.display()
    );
    assert!(
        stdout.lines().any(|printed| printed == line),
        "{line} not on stdout: {stdout}\nstderr: {stderr}"
    );
}

/// D-05, at the boundary an operator reads: the real `status`, `doctor` and `doctor --json`
/// each name a set-aside answer, and the control run before it names none.
#[test]
fn status_and_doctor_name_a_set_aside_answer_on_stdout() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(94);
    init_repo(root, phase);
    let run = |args: &[&str]| {
        let output = common::hermetic_devflow(&home)
            .args(args)
            .arg(root)
            .current_dir(root)
            .output()
            .expect("run devflow");
        assert!(
            output.status.success(),
            "devflow {args:?}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    for args in [&["status"][..], &["doctor"][..]] {
        let stdout = run(args);
        assert!(
            !stdout.contains("set-aside gate answers"),
            "control: nothing to report in {args:?}: {stdout}"
        );
    }

    plant_code_gate(root, phase);
    Gates::close(root, phase, Stage::Code).expect("set the planted answer aside");
    let set_aside = Gates::quarantined(root);
    assert_eq!(set_aside.len(), 1, "{set_aside:?}");
    let full = set_aside[0].path.display().to_string();
    let relative = set_aside[0]
        .path
        .strip_prefix(root)
        .unwrap()
        .display()
        .to_string();

    let status = run(&["status"]);
    assert!(status.contains("set-aside gate answers:"), "{status}");
    assert!(status.contains(&full), "{status}");
    let doctor = run(&["doctor"]);
    assert!(doctor.contains("set-aside gate answers:"), "{doctor}");
    assert!(doctor.contains(&relative), "{doctor}");
    let body: serde_json::Value = serde_json::from_str(&run(&["doctor", "--json"])).unwrap();
    let listed = body["set_aside_gate_answers"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "{body}");
    assert!(
        listed[0]["detail"].as_str().unwrap().contains(&relative),
        "{body}"
    );
}

// ---------------------------------------------------------------------------
// 48-19 (criterion 2, D-01, D-02): `start` must not overwrite a live run.
//
// Each arm plants the same leftover run (a Code-stage state and a Code gate
// request and response); a live run is a launched runner (`LiveRun::launch`).
// ---------------------------------------------------------------------------

/// Bounds any gate a launched monitor parks at, so a start that proceeds
/// (every control, and the refusal arms at RED) leaves no immortal monitor.
const GATE_TIMEOUT_SECS: &str = "15";

/// A fake `codex` that answers `--version` and otherwise sleeps until it is
/// ended, so a launched run stays live for the whole test.
const SLEEPING_CODEX: &str =
    "if [ \"$1\" = \"--version\" ]; then echo \"codex-cli 0.0.0-fake\"; exit 0; fi\nexec sleep 120";

/// The phase's unit name as the binary derives it (`service::unit_name`).
fn unit_name(root: &Path, phase: PhaseId) -> String {
    format!(
        "devflow-{:016x}-{}.service",
        devflow_core::registry::path_digest(root),
        phase.padded()
    )
}

/// The runner pid the fake `systemd-run` recorded for `phase`'s unit under `home`.
fn recorded_runner_pid(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> Option<u32> {
    fs::read_to_string(
        home.service_bin()
            .join(format!("{}.pid", unit_name(root, phase))),
    )
    .ok()
    .and_then(|text| text.trim().parse().ok())
}

/// Whether `phase`'s runner lock under `root` probes Live.
fn runner_lock_live(root: &Path, phase: PhaseId) -> bool {
    devflow_core::liveness::probe(&devflow_core::liveness::runner_lock_path(root, phase))
        .is_ok_and(|held| held == devflow_core::liveness::Held::Live)
}

/// A run a real `devflow start` launched as a runner unit through `home`'s fake
/// `systemd-run` (capture shape, supervise mode) with a sleeping fake agent:
/// `runner_pid` is the runner the fake recorded, and it holds the runner lock.
/// Bind a `LaunchedRunReaper::for_home` before [`LiveRun::launch`]: it ends the
/// runner, which ends its agent's group.
struct LiveRun {
    runner_pid: u32,
    _fake_bin: common::FakeBin,
}

impl LiveRun {
    fn launch(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> Self {
        let fake_bin = fake_codex(SLEEPING_CODEX);
        let output = child(home, root, phase, &fake_bin, "supervise")
            .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS)
            .output()
            .expect("run devflow start for the live-run fixture");
        assert!(
            output.status.success(),
            "the live-run fixture's start failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let runner_pid = recorded_runner_pid(home, root, phase)
            .expect("the fixture's start launched a runner through the fake systemd-run");
        assert!(
            common::runs_the_runner_verb(runner_pid),
            "the fixture's recorded pid {runner_pid} runs the runner verb"
        );
        assert!(
            runner_lock_live(root, phase),
            "the fixture's runner holds the runner lock"
        );
        Self {
            runner_pid,
            _fake_bin: fake_bin,
        }
    }

    /// Whether the fixture's runner still runs.
    fn alive(&self) -> bool {
        common::runs_the_runner_verb(self.runner_pid)
    }
}

/// The planted Code gate files, returned so a control can assert they are gone.
struct PlantedGate {
    request: PathBuf,
    response: PathBuf,
}

/// A leftover Code-stage state and a planted Code gate. The launch guard reads
/// only the runner lock, so a test that needs a live run launches one with
/// [`LiveRun::launch`].
fn plant_leftover_run(root: &Path, phase: PhaseId) -> PlantedGate {
    let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
    state.verification_baseline_captured = true;
    state.stage = Stage::Code;
    devflow_core::workflow::save_state(&state).expect("plant leftover state");
    plant_code_gate(root, phase)
}

/// A Code gate and an aborting answer bound to it, as a leftover run leaves them.
fn plant_code_gate(root: &Path, phase: PhaseId) -> PlantedGate {
    let opened = Gates::open_gate(
        root,
        phase,
        Stage::Code,
        devflow_core::gates::GateKind::StageFailure,
        "planted leftover Code gate",
    )
    .expect("plant Code gate request");
    let response = Gates::answer(
        root,
        phase,
        Stage::Code,
        &opened.gate_id,
        &GateResponse {
            approved: false,
            abort: true,
            note: Some("abort: planted leftover response".into()),
            responded_by: Some("start-lock-e2e-planted-leftover".into()),
        },
    )
    .expect("plant Code gate response");
    PlantedGate {
        request: opened.path,
        response,
    }
}

/// Every file a live run for `phase` depends on, with its exact bytes, sorted
/// by path: the state file and each gate-directory entry named `{padded}-…`. Only existing files are listed, so a deleted file shows
/// up as a different snapshot rather than a read panic.
fn phase_file_snapshot(root: &Path, phase: PhaseId) -> Vec<(PathBuf, Vec<u8>)> {
    let mut paths = vec![devflow_core::workflow::state_path(root, phase)];
    let prefix = format!("{}-", phase.padded());
    if let Ok(entries) = fs::read_dir(Gates::dir(root)) {
        for entry in entries {
            let entry = entry.expect("read gates dir entry");
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                paths.push(entry.path());
            }
        }
    }
    paths.retain(|path| path.exists());
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path).expect("read snapshot file");
            (path, bytes)
        })
        .collect()
}

fn run_start(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
    extra: &[&str],
) -> Output {
    child(home, root, phase, fake_bin, "supervise")
        .args(extra)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS)
        .output()
        .expect("run devflow start")
}

fn events_contain_workflow_started(root: &Path) -> bool {
    // An absent events file counts as not containing the event.
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .contains("\"workflow_started\"")
}

/// How many `workflow_started` events the project's event log holds.
fn workflow_started_count(root: &Path) -> usize {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .matches("\"workflow_started\"")
        .count()
}

#[test]
fn start_replaces_a_leftover_state_whose_recorded_processes_are_dead() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(97);
    init_repo(root, phase);
    let planted = plant_leftover_run(root, phase);
    let fake_bin = fake_codex("exit 0");
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);

    let output = run_start(&home, root, phase, &fake_bin, &[]);
    assert!(
        output.status.success(),
        "a leftover state whose recorded processes are dead must be replaced as today\n\
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !planted.request.exists(),
        "the leftover Code gate request must be cleared: {}",
        planted.request.display()
    );
    assert!(
        !planted.response.exists(),
        "the leftover Code gate response must be out of its slot: {}",
        planted.response.display()
    );
    assert_eq!(
        Gates::quarantined(root).len(),
        1,
        "the leftover response is set aside, not deleted"
    );
    assert!(
        events_contain_workflow_started(root),
        "replacing a dead leftover must emit workflow_started"
    );
}

// ---------------------------------------------------------------------------
// 48-20 (criterion 2, D-01, D-02): `resume` must not overwrite a live run.
// ---------------------------------------------------------------------------

fn events_contain(root: &Path, event: &str) -> bool {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .contains(&format!("\"{event}\""))
}

/// A resumable Code-stage state. The launch guard reads only the runner lock.
fn plant_resumable_run(root: &Path, phase: PhaseId, stopped: bool) {
    let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
    state.verification_baseline_captured = true;
    state.stage = Stage::Code;
    state.stopped = stopped;
    state.stop_reason =
        stopped.then(|| "planted: marked stopped by devflow stop while the run was live".into());
    devflow_core::workflow::save_state(&state).expect("plant resumable state");
}

/// `devflow resume` for `phase`; its `DEVFLOW_CACHE_DIR` is `home`'s cache dir,
/// set by `common::hermetic_devflow`.
fn resume_command(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &common::FakeBin,
) -> Command {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let mut command = common::hermetic_devflow(home);
    command
        .args(["resume", "--phase", &phase.to_string()])
        .arg(root)
        .current_dir(root)
        .hermetic_path(path)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", GATE_TIMEOUT_SECS);
    command
}

/// How many `stage_launched` events the project's event log holds.
fn stage_launched_count(root: &Path) -> usize {
    fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .matches("\"stage_launched\"")
        .count()
}

/// `resume` over a phase whose runner is live refuses, names `devflow stop`, leaves the
/// phase's state byte-identical, launches nothing and leaves the runner running.
#[test]
fn resume_refuses_a_phase_whose_recorded_monitor_is_alive() {
    let repo = tempfile::tempdir().unwrap();
    let home = common::HermeticHome::new();
    let root = repo.path();
    let phase = PhaseId::new(85);
    init_repo(root, phase);
    let fake_bin = fake_codex("exec sleep 60");
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);
    let run = LiveRun::launch(&home, root, phase);
    let state_file = devflow_core::workflow::state_path(root, phase);
    let state_before = fs::read(&state_file).expect("read the live run's state");
    let launched_before = stage_launched_count(root);

    let output = resume_command(&home, root, phase, &fake_bin)
        .output()
        .expect("run devflow resume");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "resume must refuse a phase whose runner is live\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains(&format!("devflow stop --phase {phase}")),
        "the refusal names `devflow stop --phase {phase}`\nstderr: {stderr}"
    );
    assert_eq!(
        fs::read(&state_file).expect("read the state after the refusal"),
        state_before,
        "a refused resume must leave the state byte-identical"
    );
    assert_eq!(
        stage_launched_count(root),
        launched_before,
        "a refused resume must not emit stage_launched"
    );
    assert_eq!(
        recorded_runner_pid(&home, root, phase),
        Some(run.runner_pid),
        "a refused resume must launch no second runner"
    );
    assert!(
        run.alive(),
        "a refused resume must leave the live runner running"
    );
}

#[test]
fn resume_relaunches_a_phase_whose_recorded_processes_are_dead() {
    for (phase, stopped) in [(PhaseId::new(87), false), (PhaseId::new(88), true)] {
        let repo = tempfile::tempdir().unwrap();
        let home = common::HermeticHome::new();
        let root = repo.path();
        init_repo(root, phase);
        let fake_bin = fake_codex("exec sleep 60");
        let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);
        plant_resumable_run(root, phase, stopped);

        let output = resume_command(&home, root, phase, &fake_bin)
            .output()
            .expect("run devflow resume");
        assert!(
            output.status.success(),
            "a resume over recorded processes that are dead must relaunch as today (stopped={stopped})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let state = devflow_core::workflow::load_state(root, phase).expect("load relaunched state");
        assert!(!state.stopped, "resume must clear the stop mark");
        let runner = recorded_runner_pid(&home, root, phase)
            .unwrap_or_else(|| panic!("the relaunch must launch a runner unit: {state:?}"));
        assert!(
            common::runs_the_runner_verb(runner),
            "the relaunched unit runs the runner verb (pid {runner})"
        );
        common::wait_for(
            || events_contain(root, "stage_launched"),
            Duration::from_secs(30),
            "a resumed dead leftover to emit stage_launched",
        );
    }
}

#[test]
fn resume_without_phase_state_reports_the_missing_state_not_a_live_run() {
    let repo = tempfile::tempdir().unwrap();
    let home = common::HermeticHome::new();
    let root = repo.path();
    let phase = PhaseId::new(84);
    init_repo(root, phase);
    let fake_bin = fake_codex("exec sleep 60");
    assert!(
        !devflow_core::workflow::state_path(root, phase).exists(),
        "the carve-out fixture must have no phase state"
    );
    let before = phase_file_snapshot(root, phase);

    let output = resume_command(&home, root, phase, &fake_bin)
        .output()
        .expect("run devflow resume");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "resume without phase state must still fail\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !stderr.contains("refusing to resume"),
        "without phase state, resume must report the missing state, not a live run\nstderr: {stderr}"
    );
    assert_eq!(phase_file_snapshot(root, phase), before);
    assert!(!events_contain(root, "stage_launched"));
}

// ---------------------------------------------------------------------------
// 50-15 (D-09, R-12, 999.139): a state file that will not load must not hide
// a live run from `start` or `resume`. Each fixture is a real launched run, so
// its runner lock is what refuses.
// ---------------------------------------------------------------------------

/// Bytes no `State` deserializes.
const CORRUPT_JSON: &[u8] = b"{ this does not parse";

/// Overwrite `phase`'s state file with invalid JSON; returns the bytes.
fn corrupt_state(root: &Path, phase: PhaseId) -> Vec<u8> {
    let path = devflow_core::workflow::state_path(root, phase);
    fs::write(&path, CORRUPT_JSON).expect("corrupt the state file");
    assert!(
        devflow_core::workflow::load_state(root, phase).is_err(),
        "the corrupted state must not load"
    );
    CORRUPT_JSON.to_vec()
}

/// `devflow start` for `phase`, as the 48-19 refusal tests run it.
fn plain_start(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> Output {
    let fake_bin = fake_codex("exit 0");
    run_start(home, root, phase, &fake_bin, &[])
}

/// Run `launch` (a `start` or `resume` over a corrupt state) and assert it is
/// refused with no side effect: no second runner launched, the corrupt state
/// bytes unchanged, no `workflow_started` or `stage_launched` appended.
/// Returns its stderr.
fn assert_refused_over_a_corrupt_state(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    launch: impl FnOnce() -> Output,
) -> String {
    let state_file = devflow_core::workflow::state_path(root, phase);
    let state_before = fs::read(&state_file).expect("read the corrupt state");
    let runner_before = recorded_runner_pid(home, root, phase);
    let started_before = workflow_started_count(root);
    let launched_before = stage_launched_count(root);

    let output = launch();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "a launch over a live run whose state will not load must refuse\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        recorded_runner_pid(home, root, phase),
        runner_before,
        "a refused launch must launch no second runner\nstderr: {stderr}"
    );
    assert_eq!(
        fs::read(&state_file).expect("read the state after the refusal"),
        state_before,
        "a refused launch must leave the corrupt state bytes unchanged"
    );
    assert_eq!(
        workflow_started_count(root),
        started_before,
        "a refused launch must not emit workflow_started"
    );
    assert_eq!(
        stage_launched_count(root),
        launched_before,
        "a refused launch must not emit stage_launched"
    );
    stderr
}

/// The backticked `devflow stop --phase …` command in `stderr`, split into
/// its words without the leading `devflow`.
fn printed_stop_command(stderr: &str, phase: PhaseId) -> Vec<String> {
    let head = format!("`devflow stop --phase {phase}");
    let at = stderr
        .find(&head)
        .unwrap_or_else(|| panic!("no {head}… command in the refusal\nstderr: {stderr}"));
    let command = &stderr[at + 1..];
    let end = command
        .find('`')
        .expect("the printed command closes its backtick");
    command[..end]
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect()
}

/// Run a printed `devflow stop` command exactly as printed, from `root`.
fn run_printed_stop(home: &common::HermeticHome, root: &Path, words: &[String]) -> Output {
    common::hermetic_devflow(home)
        .args(words)
        .current_dir(root)
        .output()
        .expect("run the printed devflow stop")
}

/// D-07: a live runner refuses `start` whatever the state names, here a state
/// that will not load; the refusal names `devflow stop` and the runner keeps running.
#[test]
fn start_refuses_over_a_live_supervisor_when_the_state_will_not_load() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(70);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);
    let run = LiveRun::launch(&home, root, phase);
    corrupt_state(root, phase);

    let stderr =
        assert_refused_over_a_corrupt_state(&home, root, phase, || plain_start(&home, root, phase));
    assert!(
        stderr.contains(&format!("devflow stop --phase {phase}")),
        "the refusal names `devflow stop --phase {phase}`\nstderr: {stderr}"
    );
    assert!(
        run.alive(),
        "a refused start must leave the live runner running"
    );
}

/// The refusal over a live run whose state will not load prints a `devflow stop` that,
/// run as printed, ends the run: the unit's runner is gone, the corrupt state bytes are
/// unchanged, and `stop` says the state does not load, naming `devflow discard`. Once the
/// run is gone, `start` no longer refuses a live run: it refuses only the branch the run
/// left, naming `devflow discard` (52.1 D-09.1).
#[test]
fn the_corrupt_state_refusal_names_a_stop_that_ends_the_run() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(71);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);
    let run = LiveRun::launch(&home, root, phase);
    let corrupt = corrupt_state(root, phase);
    let stderr =
        assert_refused_over_a_corrupt_state(&home, root, phase, || plain_start(&home, root, phase));
    let words = printed_stop_command(&stderr, phase);
    assert_eq!(
        words,
        ["stop", "--phase", &phase.to_string()],
        "the refusal must print the plain stop form\nstderr: {stderr}"
    );

    let stop = run_printed_stop(&home, root, &words);
    let stop_stderr = String::from_utf8_lossy(&stop.stderr);
    assert!(
        stop_stderr.contains("the unit is down")
            && stop_stderr.contains(&format!("devflow discard --phase {phase}")),
        "the printed stop must end the unit and name `devflow discard` for the state that \
         does not load\nstdout: {}\nstderr: {stop_stderr}",
        String::from_utf8_lossy(&stop.stdout)
    );
    common::wait_for(
        || !run.alive(),
        Duration::from_secs(5),
        "the stopped run's runner to be gone",
    );
    assert_eq!(
        fs::read(devflow_core::workflow::state_path(root, phase)).unwrap(),
        corrupt,
        "stop must leave the corrupt state bytes unchanged"
    );

    let again = plain_start(&home, root, phase);
    let again_stderr = String::from_utf8_lossy(&again.stderr);
    assert!(
        !again.status.success()
            && again_stderr.contains(&format!("devflow discard --phase {phase}"))
            && !again_stderr.contains("devflow stop"),
        "once the run is stopped, start refuses only the branch it left, naming discard\n\
         stderr: {again_stderr}"
    );
}

#[test]
fn start_proceeds_over_a_corrupt_state_with_no_live_supervisor() {
    let home = common::HermeticHome::new();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(73);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::for_home(&home, root, phase);
    let run = LiveRun::launch(&home, root, phase);
    let signed = libc::pid_t::try_from(run.runner_pid).expect("pid fits pid_t");
    assert!(signed > 1, "refusing to signal pid {signed}");
    // SAFETY: `signed > 1` names the runner this test's fake launcher recorded, whose
    // argv `LiveRun::launch` checked.
    unsafe { libc::kill(signed, libc::SIGTERM) };
    common::wait_for(
        || !run.alive() && !runner_lock_live(root, phase),
        Duration::from_secs(20),
        "the stopped run's runner to exit and free its lock",
    );
    corrupt_state(root, phase);
    // The stopped run's branch is a leftover `start` refuses (52.1 D-09.1); removed here, as
    // the operator would, so the corrupt state is the only thing left over.
    git(root, &["checkout", "-q", "develop"]);
    git(
        root,
        &["branch", "-D", &format!("feature/phase-{}", phase.padded())],
    );
    let started_before = workflow_started_count(root);

    let output = plain_start(&home, root, phase);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "with no live runner, start over a corrupt state proceeds as today\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        workflow_started_count(root),
        started_before + 1,
        "the start over the corrupt state must emit workflow_started"
    );
}
