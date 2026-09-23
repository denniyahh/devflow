//! #200 regression: the same `gate reject` command must tell a gate left by an
//! interrupted foreground driver apart from a live consumer of that gate.
//!
//! The file pins both drivers:
//! - the `start_*` arms cover criterion 4's shape: a foreground `devflow
//!   start` parked at its Define preflight gate holds lock-NN itself;
//! - the `advance_*` arms cover a foreground `devflow advance` parked at a
//!   Code gate.

use devflow_core::gates::Gates;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
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

fn spawn_gated_advance(root: &Path, phase: PhaseId) -> Child {
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    devflow_core::workflow::save_state(&state).unwrap();
    let child = Command::new(devflow_bin())
        .args(["advance", "--phase", &phase.to_string()])
        .arg(root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "15")
        .spawn()
        .expect("spawn foreground advance");
    wait_for(
        || devflow_core::lock::holder(root, phase).is_some(),
        "advance lock",
    );
    wait_for(
        || Gates::gate_path(root, phase, Stage::Code).exists(),
        "Code gate",
    );
    child
}

fn reject(root: &Path, phase: PhaseId) -> std::process::Output {
    Command::new(devflow_bin())
        .args([
            "gate",
            "reject",
            &phase.to_string(),
            "code",
            "--note",
            "abort: #200 e2e",
        ])
        .arg(root)
        .output()
        .expect("run same gate reject command")
}

/// A `develop`-only repo whose ROADMAP names `phase` and which has no
/// `.planning/config.json`, so `start --mode auto` parks at its Define
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
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

struct FakeBin {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fake_bin_dir(agent_body: &str) -> FakeBin {
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude");
    fs::write(&claude, format!("#!/bin/sh\n{agent_body}\n")).unwrap();
    let mut permissions = fs::metadata(&claude).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&claude, permissions).unwrap();
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
fn start_child(root: &Path, phase: PhaseId, fake_bin: &FakeBin) -> Command {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
    let mut command = Command::new(devflow_bin());
    command
        .args([
            "start",
            "--phase",
            &phase.to_string(),
            "--agent",
            "claude",
            "--mode",
            "auto",
            "--no-worktree",
            "--legacy-claude-launch",
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
/// up to 20 s for it to park at the Define preflight gate.
fn spawn_parked_start(
    root: &Path,
    phase: PhaseId,
    fake_bin: &FakeBin,
    gate_timeout_secs: &str,
) -> ReapOnDrop {
    let mut start = ReapOnDrop(
        start_child(root, phase, fake_bin)
            .env("DEVFLOW_GATE_TIMEOUT_SECS", gate_timeout_secs)
            .spawn()
            .expect("spawn foreground start"),
    );
    let waited = Instant::now();
    while !Gates::gate_path(root, phase, Stage::Define).exists() {
        if let Some(status) = start.0.try_wait().expect("poll start child") {
            panic!("start exited ({status:?}) before parking at its Define gate");
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "timed out after 20s waiting for start's Define gate"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    start
}

fn reject_define(root: &Path, phase: PhaseId) -> std::process::Output {
    Command::new(devflow_bin())
        .args([
            "gate",
            "reject",
            &phase.to_string(),
            "define",
            "--note",
            "abort: #200 start e2e",
        ])
        .arg(root)
        .output()
        .expect("run same gate reject define command")
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4816);
    init_repo(root, phase);
    let mut child = spawn_gated_advance(root, phase);
    child.kill().expect("kill foreground advance");
    child.wait().expect("reap foreground advance");

    let output = reject(root, phase);
    assert!(
        !output.status.success(),
        "wedge reject unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !Gates::response_path(root, phase, Stage::Code).exists(),
        "a wedged gate must receive no stale response"
    );
    let output_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output_text.contains("no confirmed waiter"),
        "unexpected output: {output_text}"
    );

    let recovery = Command::new(devflow_bin())
        .args(["recover", "--clean", "--phase", &phase.to_string()])
        .arg(root)
        .output()
        .expect("recover lock-free wedge");
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
    assert!(
        !root
            .join(".devflow")
            .join(format!("lock-{}", phase.padded()))
            .exists(),
        "recovery must not leave its own lock behind"
    );
}

#[test]
fn advance_self_resolving_arm_live_advance_consumes_the_code_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4817);
    init_repo(root, phase);
    let mut child = spawn_gated_advance(root, phase);
    let before = Instant::now();
    let output = reject(root, phase);
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(4818);
    let ship_phase = PhaseId::new(4819);
    let live_ship_phase = PhaseId::new(4820);
    Gates::write_gate(root, phase, Stage::Code, "abandoned").unwrap();
    Gates::write_gate(root, ship_phase, Stage::Ship, "release").unwrap();
    let pid = std::process::id();
    let start = devflow_core::agent::process_start_time(pid).expect("test process start time");
    let lock_path = root
        .join(".devflow")
        .join(format!("lock-{}", live_ship_phase.padded()));
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    std::fs::write(lock_path, format!("{pid}\n{start}")).unwrap();
    Gates::write_gate(root, live_ship_phase, Stage::Ship, "live poller").unwrap();

    let output = Command::new(devflow_bin())
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
        stdout.contains(&format!("left phase {ship_phase} ship alone")),
        "no-holder Ship gate must also be left alone: {stdout}"
    );
    assert!(
        !stdout.contains(&format!("would reap phase {ship_phase} ship")),
        "no-holder dry-run must not claim it would reap the Ship gate: {stdout}"
    );
    assert!(
        stdout.contains(&format!("would reap phase {live_ship_phase} ship")),
        "a live Ship holder must remain the positive would-reap control: {stdout}"
    );
    assert!(
        stdout.contains("1 would be reaped"),
        "summary must count only the confirmed-live would-reap decision: {stdout}"
    );
    assert!(
        !Gates::response_path(root, phase, Stage::Code).exists(),
        "dry-run sweep must not write a response"
    );
}

#[test]
fn start_wedge_arm_interrupted_start_leaves_a_define_gate_nothing_answers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(93);
    init_start_repo(root, phase);
    let fake_bin = fake_bin_dir("sleep 60");
    let mut start = spawn_parked_start(root, phase, &fake_bin, "30");

    assert_eq!(
        devflow_core::lock::holder_identity(root, phase).map(|(pid, _)| pid),
        Some(start.0.id()),
        "the parked start must hold its own phase lock"
    );

    start.0.kill().expect("kill foreground start");
    start.0.wait().expect("reap foreground start");
    assert!(
        Gates::gate_path(root, phase, Stage::Define).exists(),
        "the interrupted start must leave its Define gate behind"
    );

    let output = reject_define(root, phase);
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
        output_text.contains("no confirmed waiter"),
        "reject must say no confirmed waiter: {output_text}"
    );
    let recovery_hint = format!("devflow recover --clean --phase {phase}");
    assert!(
        output_text.contains(&recovery_hint),
        "reject must name `{recovery_hint}`: {output_text}"
    );
    assert!(
        !Gates::response_path(root, phase, Stage::Define).exists(),
        "an unanswerable Define gate must receive no response"
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
    let lock_path = devflow_dir.join(format!("lock-{}", phase.padded()));
    let state_temp = devflow_dir.join(format!(
        ".{}.1.0.tmp",
        state_path.file_name().unwrap().to_string_lossy()
    ));
    let gate_temp = gates_dir.join(format!(
        ".{}.1.0.tmp",
        Gates::gate_path(root, phase, Stage::Define)
            .file_name()
            .unwrap()
            .to_string_lossy()
    ));
    fs::write(&state_temp, b"orphaned state write").unwrap();
    fs::write(&gate_temp, b"orphaned gate write").unwrap();
    for precondition in [&state_temp, &gate_temp, &state_path, &lock_path] {
        assert!(
            precondition.exists(),
            "precondition: {} must exist before recovery, or its removal proves nothing",
            precondition.display()
        );
    }

    let recovery = Command::new(devflow_bin())
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
    assert!(
        !lock_path.exists(),
        "recovery must not leave lock-{} behind",
        phase.padded()
    );
}

#[test]
fn start_self_resolving_arm_live_start_consumes_the_define_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(94);
    init_start_repo(root, phase);
    let fake_bin = fake_bin_dir("sleep 60");
    let mut start = spawn_parked_start(root, phase, &fake_bin, "60");

    assert_eq!(
        devflow_core::lock::holder_identity(root, phase).map(|(pid, _)| pid),
        Some(start.0.id()),
        "the parked start must hold its own phase lock"
    );

    let before = Instant::now();
    let output = reject_define(root, phase);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "live reject failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // `gate_respond` re-checks the holder after publishing: a start that has
    // already consumed the answer and released its lock yields the no-holder
    // wording. That is accepted only because the start must then be seen to
    // exit successfully below.
    let reject_wording = if stdout.contains("a live lock holder may pick up this response") {
        "live"
    } else if stdout.contains("response was written, but no confirmed live holder will act") {
        "no_holder_after_exit"
    } else {
        panic!("reject reported neither a live holder nor a post-exit no-holder: {stdout}");
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
        !Gates::gate_path(root, phase, Stage::Define).exists(),
        "consuming the response clears the Define gate request"
    );
}
