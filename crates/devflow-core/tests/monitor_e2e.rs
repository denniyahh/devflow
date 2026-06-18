//! End-to-end monitor integration test.
//!
//! Spawns a real background monitor that owns a *fake* agent (a `sh` script
//! that prints a `DEVFLOW_RESULT` marker and exits 0), then asserts the monitor
//! captured everything `devflow check` needs to advance the state machine.
//!
//! Boundary note: the real `devflow` binary is not built inside the library's
//! integration-test harness, so the monitor's tail `devflow check` self-call is
//! a no-op here (it re-invokes this test binary with a non-matching filter).
//! We therefore assert the **capture/result** side — that Layers 1–2 evaluate
//! to `Success` from the files the monitor wrote — rather than the state
//! transition itself.

use devflow_core::agent_result::{
    self, AgentStatus, agent_pid_path, evaluate_agent_result, exit_code_path, stdout_path,
};
use devflow_core::config::GitFlowConfig;
use devflow_core::monitor::{spawn_monitor, wait_for_agent_pid};
use devflow_core::state::{Agent, State, Step};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Init a temp repo with `develop` and a `feature/phase-NN` branch holding one
/// commit, hooks disabled for isolation.
fn init_repo(root: &Path, phase: u32) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);

    let branch = format!("feature/phase-{phase:02}");
    git(root, &["checkout", "-q", "-b", &branch]);
    std::fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
}

#[test]
fn monitor_owns_fake_agent_and_records_devflow_result() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = 7;
    init_repo(root, phase);

    let mut state = State::new(phase, Agent::Claude, root.to_path_buf());
    state.step = Step::Executing;

    // Fake agent: emit the success marker on stdout, then exit 0.
    let args = vec![
        "-c".to_string(),
        "printf 'work done\\nDEVFLOW_RESULT: {\"status\":\"success\",\"commits\":1}\\n'".to_string(),
    ];

    spawn_monitor(&state, "sh", &args).expect("spawn monitor");

    // The monitor records the agent PID.
    let agent_pid = wait_for_agent_pid(root, phase).expect("agent pid recorded");
    assert!(agent_pid > 0);

    // Wait until the monitor writes the exit-code file (agent finished).
    let exit_path = exit_code_path(root, phase);
    let mut exit_seen = false;
    for _ in 0..200 {
        if exit_path.exists() {
            exit_seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(exit_seen, "monitor never wrote the exit-code file");

    // Captured stdout contains the marker.
    let stdout = std::fs::read_to_string(stdout_path(root, phase)).unwrap();
    assert!(
        stdout.contains("DEVFLOW_RESULT"),
        "captured stdout missing marker: {stdout:?}"
    );

    // Exit code recorded as 0.
    let exit = std::fs::read_to_string(&exit_path).unwrap();
    assert_eq!(exit.trim(), "0");

    // The pid file exists (the monitor created the capture directory + files).
    assert!(agent_pid_path(root, phase).exists());

    // End-to-end: the layered evaluator reads the monitor's files and reports
    // Success — exactly what `devflow check` would act on to advance state.
    let result = evaluate_agent_result(root, &state, &GitFlowConfig::default()).unwrap();
    assert_eq!(result.status, AgentStatus::Success);

    // Sanity: the capture path helpers agree with what the monitor wrote.
    assert_eq!(
        stdout_path(root, phase),
        agent_result::stdout_path(root, phase)
    );
}
