//! End-to-end coverage for `start`'s per-phase writer lock.
//!
//! These tests deliberately hold the lock from the test process, then drive
//! the compiled CLI as a separate process. That makes the refusal observable
//! at the same boundary an operator uses, rather than coupling the test to
//! `commands::start`'s implementation details.

use devflow_core::gates::{GateResponse, Gates};
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
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

fn child(root: &Path, phase: PhaseId, fake_bin: &FakeBin, mode: &str) -> Command {
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
            mode,
            "--no-worktree",
            "--legacy-claude-launch",
        ])
        .arg(root)
        .current_dir(root)
        .env("PATH", path);
    command
}

fn wait_for(mut predicate: impl FnMut() -> bool, timeout: Duration, what: &str) {
    let started = Instant::now();
    while !predicate() {
        assert!(
            started.elapsed() < timeout,
            "timed out after {timeout:?} waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn kill_and_reap(child: &mut Child) {
    if child.try_wait().expect("poll start child").is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

#[test]
fn start_refuses_while_another_process_holds_the_phase_lock() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(90);
    init_repo(root, phase);
    let fake_bin = fake_bin_dir("exit 0");
    let holder = devflow_core::lock::acquire(root, phase).expect("test holds phase lock");
    let holder_pid = std::process::id();
    let worktrees_before = devflow_core::test_support::git_command(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("list baseline worktrees");
    assert!(worktrees_before.status.success());

    let output = child(root, phase, &fake_bin, "supervise")
        .output()
        .expect("run devflow start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "start must refuse a held lock\nstdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains(&phase.to_string()),
        "stderr must name phase: {stderr}"
    );
    assert!(
        stderr.contains(&holder_pid.to_string()),
        "stderr must name holder pid {holder_pid}: {stderr}"
    );
    assert!(
        !root
            .join(format!(".devflow/state-{}.json", phase.padded()))
            .exists(),
        "a refused start must not write phase state"
    );
    let worktrees = devflow_core::test_support::git_command(root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("list worktrees");
    assert!(worktrees.status.success());
    assert_eq!(
        worktrees.stdout, worktrees_before.stdout,
        "a refused start must not add a worktree"
    );
    let branches = devflow_core::test_support::git_command(root)
        .args([
            "branch",
            "--list",
            &format!("feature/phase-{padded}", padded = phase.padded()),
        ])
        .output()
        .expect("list phase branch");
    assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
    drop(holder);
}

#[test]
fn start_proceeds_when_no_process_holds_the_phase_lock() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(91);
    init_repo(root, phase);
    let fake_bin = fake_bin_dir("exit 0");

    let output = child(root, phase, &fake_bin, "supervise")
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
}

#[test]
fn start_clears_leftover_gate_files_after_taking_the_lock() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    let phase = PhaseId::new(92);
    init_repo(root, phase);
    let fake_bin = fake_bin_dir("sleep 60");
    let response_path = Gates::response_path(root, phase, Stage::Define);
    fs::create_dir_all(response_path.parent().unwrap()).unwrap();
    fs::write(
        &response_path,
        serde_json::to_vec(&GateResponse {
            approved: false,
            note: Some("abort: planted stale response".into()),
            responded_by: Some("start-lock-e2e-planted-response".into()),
        })
        .unwrap(),
    )
    .unwrap();

    let mut process = child(root, phase, &fake_bin, "auto")
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "30")
        .spawn()
        .expect("spawn devflow start");
    wait_for(
        || {
            Gates::gate_path(root, phase, Stage::Define).exists()
                || process.try_wait().expect("poll start child").is_some()
        },
        Duration::from_secs(10),
        "Define gate or start exit",
    );
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        process.try_wait().expect("poll start child").is_none(),
        "planted response must not abort a fresh start"
    );
    assert!(
        root.join(format!(".devflow/state-{}.json", phase.padded()))
            .exists(),
        "a fresh start parked at Define must keep state"
    );
    let events = fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
    assert!(
        !events.contains("start-lock-e2e-planted-response"),
        "the planted response must not resolve the fresh gate\nevents:\n{events}"
    );
    assert!(
        !response_path.exists(),
        "the planted response must be removed"
    );
    kill_and_reap(&mut process);
}
