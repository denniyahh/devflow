//! #200 regression: the same gate answer command (`gate abort`, which ends
//! the run) must tell a gate left by an interrupted foreground driver apart
//! from a live consumer of that gate. The file also pins that a rejection never
//! ends a run, whatever its note says, and that `gate abort` does.
//!
//! The file pins both drivers:
//! - the `start_*` arms cover criterion 4's shape: a foreground `devflow
//!   start` parked at its Plan preflight gate holds no lock-NN. It is that
//!   gate's registered waiter, and it holds its launch reservation;
//! - the `advance_*` arms cover a foreground `devflow advance` parked at a
//!   Code gate.

mod common;

use devflow_core::gates::{GateFile, GateId, Gates, WaiterStatus};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::test_support::gate_answer_files;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

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
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
    let branch = format!("feature/phase-{}", phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    std::fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
}

fn wait_for(mut predicate: impl FnMut() -> bool, what: &str) {
    let start = Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn spawn_gated_advance(home: &common::HermeticHome, root: &Path, phase: PhaseId) -> Child {
    spawn_gated_advance_with_path(home, root, phase, None)
}

/// [`spawn_gated_advance`], with `bin_first` (when given) put first on the
/// child's PATH, so any agent the advance launches after its gate is a stub.
fn spawn_gated_advance_with_path(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    bin_first: Option<&Path>,
) -> Child {
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.verification_baseline_captured = true;
    state.stage = Stage::Code;
    devflow_core::workflow::save_state(&state).unwrap();
    let mut command = common::hermetic_devflow(home);
    command
        .args(["advance", "--phase", &phase.to_string()])
        .arg(root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "15");
    if let Some(bin_first) = bin_first {
        let existing = std::env::var_os("PATH").unwrap_or_default();
        command.env(
            "PATH",
            format!("{}:{}", bin_first.display(), existing.to_string_lossy()),
        );
    }
    let child = command.spawn().expect("spawn foreground advance");
    wait_for(
        || Gates::gate_path(root, phase, Stage::Code).exists(),
        "Code gate",
    );
    let gate = GateId::parse(
        &Gates::list_open(root)
            .into_iter()
            .find(|gate| gate.phase == phase && gate.stage == Stage::Code)
            .expect("the advance's Code gate is listed")
            .gate_id,
    )
    .expect("the advance's gate has a gate id");
    wait_for(
        || {
            Gates::waiter_status(root, phase, Stage::Code, &gate)
                .is_ok_and(|status| status == WaiterStatus::Live)
        },
        "the advance's registration at its Code gate",
    );
    child
}

/// End the run at the phase's Code gate with `devflow gate abort`.
fn abort_code_gate(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
) -> std::process::Output {
    answer_code_gate(home, root, phase, "abort", "#200 e2e")
}

/// Run `devflow gate <verb> <phase> --stage code --note <note>` against `root`.
fn answer_code_gate(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    verb: &str,
    note: &str,
) -> std::process::Output {
    common::hermetic_devflow(home)
        .args([
            "gate",
            verb,
            &phase.to_string(),
            "--stage",
            "code",
            "--note",
            note,
            "--project",
        ])
        .arg(root)
        .output()
        .unwrap_or_else(|error| panic!("run gate {verb}: {error}"))
}

/// Every event line in `root`'s log named `event`, parsed.
fn events_named(root: &Path, event: &str) -> Vec<serde_json::Value> {
    let Ok(contents) = fs::read_to_string(devflow_core::events::events_path(root)) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|line| line["event"] == event)
        .collect()
}

/// A rejection is never an abort, whatever its note says: a note reading "do
/// not abort, fix the test" resolves the live advance's Code gate as a
/// loop-back, and the run is not aborted.
#[test]
fn a_note_saying_do_not_abort_keeps_the_run_alive() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4821);
    init_repo(root, phase);
    // The loop-back relaunches Code: the agent it finds is a stub that exits at
    // once, never a real one, and the reaper ends any supervisor it started.
    let _reaper = common::LaunchedRunReaper::new(root, phase, Vec::new());
    let fake_bin = common::fake_bin_dir("exit 1");
    let _child = ReapOnDrop(spawn_gated_advance_with_path(
        &home,
        root,
        phase,
        Some(&fake_bin.path),
    ));

    let output = answer_code_gate(&home, root, phase, "reject", "do not abort, fix the test");
    assert!(
        output.status.success(),
        "live reject failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    common::wait_for(
        || !events_named(root, "gate_resolved").is_empty(),
        Duration::from_secs(20),
        "the advance to resolve its Code gate",
    );
    let resolved = events_named(root, "gate_resolved");
    assert_eq!(
        resolved[0]["action"], "loop_back",
        "a rejection whose note says `do not abort` must loop back: {resolved:?}"
    );
    assert!(
        events_named(root, "workflow_aborted").is_empty(),
        "a rejection must not abort the run: {:?}",
        events_named(root, "workflow_aborted")
    );
}

/// `devflow gate abort` writes an answer whose `abort` field is `true`: the
/// field, not the note, is what ends the run.
#[test]
fn gate_abort_writes_an_answer_with_the_abort_field_set() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4823);
    let gate = Gates::open_gate(
        root,
        phase,
        Stage::Code,
        devflow_core::gates::GateKind::StageFailure,
        "an open Code gate",
    )
    .unwrap()
    .gate_id;
    let _waiter = Gates::register_waiter(root, phase, Stage::Code, &gate).unwrap();

    let output = answer_code_gate(&home, root, phase, "abort", "operator stop");
    assert!(
        output.status.success(),
        "gate abort failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with(&format!("aborted gate for phase {phase} code")),
        "gate abort must report the abort: {stdout}"
    );
    let path = Gates::answer_path(root, phase, Stage::Code, &gate);
    let answer: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("gate abort wrote the answer file"))
            .expect("the answer file is JSON");
    assert_eq!(answer["abort"], true, "{answer}");
    assert_eq!(answer["approved"], false, "{answer}");
    assert_eq!(answer["note"], "operator stop", "{answer}");
}

/// `devflow gate abort` ends a live advance parked at its Code gate: the
/// advance resolves the gate as an abort and records `workflow_aborted`.
#[test]
fn gate_abort_ends_the_run() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4822);
    init_repo(root, phase);
    let _reaper = common::LaunchedRunReaper::new(root, phase, Vec::new());
    let mut child = ReapOnDrop(spawn_gated_advance(&home, root, phase));

    let output = answer_code_gate(&home, root, phase, "abort", "operator stop");
    assert!(
        output.status.success(),
        "gate abort failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let before = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("poll advance") {
            break status;
        }
        assert!(
            before.elapsed() < Duration::from_secs(20),
            "the live advance did not act on the abort within 20s"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "the advance must abort cleanly: {status:?}"
    );
    let resolved = events_named(root, "gate_resolved");
    assert_eq!(resolved.len(), 1, "{resolved:?}");
    assert_eq!(resolved[0]["action"], "abort", "{resolved:?}");
    assert_eq!(
        events_named(root, "workflow_aborted").len(),
        1,
        "gate abort must end the run"
    );
    assert!(
        !devflow_core::workflow::state_path(root, phase).exists(),
        "an aborted run clears its state"
    );
}

/// A `develop`-only repo whose ROADMAP names `phase` and which has no
/// `.planning/config.json`, so `start --mode auto` parks at its Plan
/// preflight gate. Same shape as start_lock_e2e.rs `init_repo`.
fn init_start_repo(root: &Path, phase: PhaseId) {
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

struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fake_bin_dir(agent_body: &str) -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let codex = dir.path().join("codex");
    fs::write(&codex, format!("#!/bin/sh\n{agent_body}\n")).unwrap();
    let mut permissions = fs::metadata(&codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&codex, permissions).unwrap();
    for program in ["git", "sh"] {
        symlink(format!("/usr/bin/{program}"), dir.path().join(program)).unwrap();
    }
    FakeBin {
        path: dir.path().to_path_buf(),
        _dir: dir,
    }
}

/// A foreground `devflow start --mode auto` with the fake agent first on the
/// child's PATH. PATH is set on the child only (TEST-01).
fn start_child(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &FakeBin,
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
            "auto",
            "--no-worktree",
        ])
        .arg(root)
        .current_dir(root)
        .env("PATH", path);
    command
}

/// Owns a spawned `devflow start` so a panicking assertion never leaks a
/// parked process for the length of its gate timeout.
struct ReapOnDrop(Child);

impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// Spawns `start` (owned by `ReapOnDrop` from the moment it exists) and waits
/// up to 20 s for it to park at the Plan preflight gate.
fn spawn_parked_start(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
    fake_bin: &FakeBin,
    gate_timeout_secs: &str,
) -> ReapOnDrop {
    let mut start = ReapOnDrop(
        start_child(home, root, phase, fake_bin)
            .env("DEVFLOW_GATE_TIMEOUT_SECS", gate_timeout_secs)
            .spawn()
            .expect("spawn foreground start"),
    );
    let waited = Instant::now();
    while !Gates::gate_path(root, phase, Stage::Plan).exists() {
        if let Some(status) = start.0.try_wait().expect("poll start child") {
            panic!("start exited ({status:?}) before parking at its Plan gate");
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "timed out after 20s waiting for start's Plan gate"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    start
}

/// End the run at the phase's Plan gate with `devflow gate abort`.
fn abort_plan_gate(
    home: &common::HermeticHome,
    root: &Path,
    phase: PhaseId,
) -> std::process::Output {
    common::hermetic_devflow(home)
        .args([
            "gate",
            "abort",
            &phase.to_string(),
            "--stage",
            "plan",
            "--note",
            "#200 start e2e",
            "--project",
        ])
        .arg(root)
        .output()
        .expect("run gate abort for the Plan gate")
}

/// Names of entries in `dir` accepted by `keep`; a missing directory is empty.
fn dir_entries(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<String> {
    match dir.read_dir() {
        Ok(entries) => entries
            .map(|entry| {
                entry
                    .expect("directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| keep(name))
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("read {}: {error}", dir.display()),
    }
}

fn phase_gate_entries(root: &Path, phase: PhaseId) -> Vec<String> {
    let prefix = format!("{}-", phase.padded());
    dir_entries(&Gates::dir(root), |name| name.starts_with(&prefix))
}

#[test]
fn advance_wedge_arm_killed_advance_leaves_a_code_gate_that_reject_reports_honestly() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4816);
    init_repo(root, phase);
    let mut child = spawn_gated_advance(&home, root, phase);
    child.kill().expect("kill foreground advance");
    child.wait().expect("reap foreground advance");

    let output = abort_code_gate(&home, root, phase);
    assert!(
        !output.status.success(),
        "wedge reject unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        gate_answer_files(root, phase, Stage::Code),
        Vec::<PathBuf>::new(),
        "a wedged gate must receive no stale response"
    );
    let output_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output_text.contains("no registered waiter"),
        "unexpected output: {output_text}"
    );

    let recovery = common::hermetic_devflow(&home)
        .args(["recover", "--clean", "--phase", &phase.to_string()])
        .arg(root)
        .output()
        .expect("recover the wedge");
    assert!(
        recovery.status.success(),
        "recover --clean failed: {}",
        String::from_utf8_lossy(&recovery.stderr)
    );
    assert!(
        !devflow_core::workflow::state_path(root, phase).exists(),
        "recovery must remove the wedged state"
    );
    assert!(
        !Gates::dir(root)
            .read_dir()
            .expect("gates directory")
            .any(|entry| entry
                .expect("gate entry")
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{}-", phase.padded()))),
        "recovery must remove every gate artifact for the wedged phase"
    );
    assert!(
        !root
            .join(".devflow")
            .read_dir()
            .expect("devflow directory")
            .any(|entry| {
                entry
                    .expect("devflow entry")
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            }),
        "recovery must remove orphaned temporary files"
    );
}

#[test]
fn advance_self_resolving_arm_live_advance_consumes_the_code_rejection() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4817);
    init_repo(root, phase);
    let mut child = spawn_gated_advance(&home, root, phase);
    let before = Instant::now();
    let output = abort_code_gate(&home, root, phase);
    assert!(
        output.status.success(),
        "live reject failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll advance") {
            break status;
        }
        assert!(
            before.elapsed() < Duration::from_secs(20),
            "live advance did not consume rejection"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let pickup_ms = before.elapsed().as_millis();
    println!("advance_pickup_ms={pickup_ms}");
    assert!(
        status.success(),
        "advance must abort cleanly after pickup: {status:?}"
    );
    assert!(
        !Gates::gate_path(root, phase, Stage::Code).exists(),
        "consuming the response clears the gate"
    );
}

#[test]
fn dry_run_sweep_reports_no_waiter_gates_as_left_alone() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4818);
    let validate_phase = PhaseId::new(4819);
    let live_validate_phase = PhaseId::new(4820);
    Gates::write_gate(
        root,
        phase,
        Stage::Code,
        devflow_core::gates::GateKind::StageFailure,
        "abandoned",
    )
    .unwrap();
    Gates::write_gate(
        root,
        validate_phase,
        Stage::Validate,
        devflow_core::gates::GateKind::StageFailure,
        "release",
    )
    .unwrap();
    let live_validate_gate = Gates::open_gate(
        root,
        live_validate_phase,
        Stage::Validate,
        devflow_core::gates::GateKind::StageFailure,
        "live poller",
    )
    .unwrap()
    .gate_id;
    let _live_validate_waiter = Gates::register_waiter(
        root,
        live_validate_phase,
        Stage::Validate,
        &live_validate_gate,
    )
    .unwrap();

    let output = common::hermetic_devflow(&home)
        .args([
            "gate",
            "sweep",
            "--dry-run",
            "--max-age-secs",
            "0",
            "--root",
        ])
        .arg(root)
        .output()
        .expect("run dry-run sweep");
    assert!(
        output.status.success(),
        "dry-run sweep failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("left phase {phase} code alone")),
        "no-waiter gate must be left alone: {stdout}"
    );
    assert!(
        !stdout.contains(&format!("would reap phase {phase} code")),
        "no-waiter dry-run must not claim it would reap the Code gate: {stdout}"
    );
    assert!(
        stdout.contains(&format!("left phase {validate_phase} validate alone")),
        "a no-waiter Validate gate must also be left alone: {stdout}"
    );
    assert!(
        !stdout.contains(&format!("would reap phase {validate_phase} validate")),
        "a no-waiter dry-run must not claim it would reap the Validate gate: {stdout}"
    );
    assert!(
        stdout.contains(&format!("would reap phase {live_validate_phase} validate")),
        "a registered Validate waiter must remain the positive would-reap control: {stdout}"
    );
    assert!(
        stdout.contains("1 would be reaped"),
        "summary must count only the confirmed-live would-reap decision: {stdout}"
    );
    assert_eq!(
        gate_answer_files(root, phase, Stage::Code),
        Vec::<PathBuf>::new(),
        "dry-run sweep must not write a response"
    );
}

/// The id of the phase's parked Plan gate.
fn plan_gate_id(root: &Path, phase: PhaseId) -> GateId {
    let gate = fs::read(Gates::gate_path(root, phase, Stage::Plan)).expect("read the Plan gate");
    let gate: GateFile = serde_json::from_slice(&gate).expect("parse the Plan gate");
    GateId::parse(&gate.gate_id).expect("the Plan gate has a gate id")
}

/// The parked start is the Plan gate's registered waiter, at once: every gate
/// wait registers before it publishes, so `gate reject` will find it. The
/// reservation is asserted beside that, never instead of it: it predates the gate.
fn assert_parked_start_is_the_plan_gates_waiter(root: &Path, phase: PhaseId, start_pid: u32) {
    let gate_id = plan_gate_id(root, phase);
    assert_eq!(
        Gates::waiter_status(root, phase, Stage::Plan, &gate_id).unwrap(),
        WaiterStatus::Live,
        "the parked start must be registered on its Plan gate"
    );
    assert!(
        Gates::live_waiter_holders(root)
            .iter()
            .any(|(pid, _)| *pid == start_pid),
        "the Plan gate's waiter must be the parked start, pid {start_pid}"
    );
    let pending = devflow_core::workflow::load_state(root, phase)
        .unwrap()
        .pending_supervisor
        .expect("the parked start holds its reservation");
    assert_eq!(
        devflow_core::liveness::probe(&pending.liveness_path).unwrap(),
        devflow_core::liveness::Held::Live,
        "the parked start holds its reservation's lock"
    );
}

#[test]
fn start_wedge_arm_interrupted_start_leaves_a_plan_gate_nothing_answers() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(93);
    init_start_repo(root, phase);
    let fake_bin = fake_bin_dir("sleep 60");
    let mut start = spawn_parked_start(&home, root, phase, &fake_bin, "30");

    assert_parked_start_is_the_plan_gates_waiter(root, phase, start.0.id());
    let gate_id = plan_gate_id(root, phase);

    start.0.kill().expect("kill foreground start");
    start.0.wait().expect("reap foreground start");
    assert!(
        Gates::gate_path(root, phase, Stage::Plan).exists(),
        "the interrupted start must leave its Plan gate behind"
    );

    let output = abort_plan_gate(&home, root, phase);
    let output_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "reject against an interrupted start unexpectedly succeeded: {output_text}"
    );
    assert!(
        output_text.contains("no registered waiter"),
        "reject must say no registered waiter: {output_text}"
    );
    let recovery_hint = format!("devflow recover --clean --phase {phase}");
    assert!(
        output_text.contains(&recovery_hint),
        "reject must name `{recovery_hint}`: {output_text}"
    );
    assert_eq!(
        gate_answer_files(root, phase, Stage::Plan),
        Vec::<PathBuf>::new(),
        "an unanswerable Plan gate must receive no response"
    );

    assert!(
        !phase_gate_entries(root, phase).is_empty(),
        "control: the gate scan must see the wedged phase's gate before recovery"
    );
    // The SIGKILL lands in a gate wait with no write in flight, so no temp
    // exists on its own (48-REVIEW WR-06). Plant one orphaned write temp per
    // directory in the shape `unique_temp_path_with` builds:
    // `.{target name}.{pid}.{sequence}.tmp`.
    let devflow_dir = root.join(".devflow");
    let gates_dir = Gates::dir(root);
    let state_path = devflow_core::workflow::state_path(root, phase);
    let state_temp = devflow_dir.join(format!(
        ".{}.1.0.tmp",
        state_path.file_name().unwrap().to_string_lossy()
    ));
    let gate_temp = gates_dir.join(format!(
        ".{}.1.0.tmp",
        Gates::gate_path(root, phase, Stage::Plan)
            .file_name()
            .unwrap()
            .to_string_lossy()
    ));
    fs::write(&state_temp, b"orphaned state write").unwrap();
    fs::write(&gate_temp, b"orphaned gate write").unwrap();
    let waiter_path = Gates::waiter_path(root, phase, Stage::Plan, &gate_id);
    for precondition in [&state_temp, &gate_temp, &state_path, &waiter_path] {
        assert!(
            precondition.exists(),
            "precondition: {} must exist before recovery, or its removal proves nothing",
            precondition.display()
        );
    }
    assert_eq!(
        Gates::waiter_status(root, phase, Stage::Plan, &gate_id).unwrap(),
        WaiterStatus::Absent,
        "precondition: the killed start's registration must be unheld"
    );
    assert!(
        devflow_core::workflow::load_state(root, phase)
            .unwrap()
            .pending_supervisor
            .is_some(),
        "precondition: the killed start must leave its reservation in the state"
    );

    let recovery = common::hermetic_devflow(&home)
        .args(["recover", "--clean", "--phase", &phase.to_string()])
        .arg(root)
        .output()
        .expect("recover interrupted start");
    assert!(
        recovery.status.success(),
        "recover --clean failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&recovery.stdout),
        String::from_utf8_lossy(&recovery.stderr)
    );
    assert!(
        !state_path.exists(),
        "recovery must remove the interrupted start's state"
    );
    let gates_left = phase_gate_entries(root, phase);
    assert!(
        gates_left.is_empty(),
        "recovery must remove every gate file for the phase: {gates_left:?}"
    );
    for temp_dir in [&devflow_dir, &gates_dir] {
        let temps_left = dir_entries(temp_dir, |name| name.ends_with(".tmp"));
        assert!(
            temps_left.is_empty(),
            "recovery must remove orphaned write temps in {}: {temps_left:?}",
            temp_dir.display()
        );
    }
}

#[test]
fn start_self_resolving_arm_live_start_consumes_the_plan_rejection() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(94);
    init_start_repo(root, phase);
    let fake_bin = fake_bin_dir("sleep 60");
    let mut start = spawn_parked_start(&home, root, phase, &fake_bin, "60");

    assert_parked_start_is_the_plan_gates_waiter(root, phase, start.0.id());

    let before = Instant::now();
    let output = abort_plan_gate(&home, root, phase);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "live reject failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // `gate_respond` re-checks the waiter after publishing: a start that has
    // already consumed the answer and finished its registration yields the no-waiter
    // wording. That is accepted only because the start must then be seen to
    // exit successfully below.
    let reject_wording = if stdout.contains("a registered waiter may pick up this response") {
        "live"
    } else if stdout.contains("response was written, but no registered waiter will act") {
        "no_waiter_after_exit"
    } else {
        panic!("reject reported neither a registered waiter nor a post-exit no-waiter: {stdout}");
    };
    println!("reject_wording={reject_wording}");

    let status = loop {
        if let Some(status) = start.0.try_wait().expect("poll start child") {
            break status;
        }
        assert!(
            before.elapsed() < Duration::from_secs(30),
            "live start did not consume the rejection"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let pickup_ms = before.elapsed().as_millis();
    println!("pickup_ms={pickup_ms}");
    assert!(
        status.success(),
        "start must exit cleanly after consuming the rejection: {status:?}"
    );
    assert!(
        !devflow_core::workflow::state_path(root, phase).exists(),
        "the rejected start must clear its state"
    );
    assert!(
        !Gates::gate_path(root, phase, Stage::Plan).exists(),
        "consuming the response clears the Plan gate request"
    );
}
