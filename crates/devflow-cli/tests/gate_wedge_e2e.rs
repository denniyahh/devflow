//! #200 regression: the same `gate reject` command must distinguish a gate
//! left by an interrupted foreground driver from a live `advance` consumer.

use devflow_core::gates::Gates;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::path::Path;
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

#[test]
fn wedge_arm_killed_start_leaves_a_gate_that_reject_reports_honestly() {
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
}

#[test]
fn self_resolving_arm_live_start_consumes_the_rejection() {
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
    println!("pickup_ms={pickup_ms}");
    assert!(
        status.success(),
        "advance must abort cleanly after pickup: {status:?}"
    );
    assert!(
        !Gates::gate_path(root, phase, Stage::Code).exists(),
        "consuming the response clears the gate"
    );
}
