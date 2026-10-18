//! End-to-end proof for `devflow stop` (23c, the missing primitive
//! `23-ORPHAN-FORENSICS.md` names as the reason 54 orphaned processes
//! accumulated with no remedy but `kill(1)`): stop ends a running phase by
//! writing a file the target is already polling for, and the target unwinds
//! through its own abort path — no signal sent.
//!
//! `commands::stop` is `pub(crate)` inside the `devflow` BINARY crate (no
//! `lib.rs`, so integration tests cannot link against it at all), so — like
//! every other devflow-cli integration test (`phase7_cli.rs`,
//! `gate_sweep_e2e.rs`) — this drives the real compiled binary via
//! `Command::new`, not an in-process call.

mod common;

use devflow_core::gates::{GateResponse, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State, SupervisorRecord};
use devflow_core::supervisor::{self, Liveness};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Hermetic `git` invocation for fixture setup (999.37) — never a bare
/// `Command::new("git")`.
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

/// A minimal repo with `develop` and a `feature/phase-NN` branch holding one
/// commit, hooks disabled — the same shape `gate_sweep_e2e.rs`'s `init_repo`
/// uses to reach a Code-stage phase whose gate a real `devflow advance`
/// child parks on.
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

    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    std::fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
}

/// Poll `predicate` until it's true, panicking with `what` if `timeout_secs`
/// elapses first.
fn wait_for(mut predicate: impl FnMut() -> bool, timeout_secs: u64, what: &str) {
    let start = Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < Duration::from_secs(timeout_secs),
            "timed out after {timeout_secs}s waiting for: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// `DEVFLOW_E2E_CHILD_TIMEOUT_SECS` (test-only, read here — never by
/// production code): how long this test's patience with the spawned
/// `devflow advance` child extends before the bounded wait below panics
/// instead of hanging CI indefinitely. Shared with `gate_sweep_e2e.rs`'s
/// identically-named helper so the two real-child fixtures cannot drift
/// into different reliability characteristics. Defaults comfortably above
/// `Gates::poll_response`'s 60s backoff cap.
fn e2e_child_timeout() -> Duration {
    let secs: u64 = std::env::var("DEVFLOW_E2E_CHILD_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);
    Duration::from_secs(secs)
}

fn write_live_lock(root: &Path, phase: PhaseId, child: &Child, recorded_start: u64) {
    let lock = root
        .join(".devflow")
        .join(format!("lock-{}", phase.padded()));
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    std::fs::write(lock, format!("{}\n{recorded_start}", child.id())).unwrap();
}

#[test]
fn stop_marks_stopped_after_the_signalled_lock_holder_exits() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(103);
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();
    let mut holder = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn holder");
    let start = devflow_core::agent::process_start_time(holder.id()).expect("holder start time");
    write_live_lock(root, phase, &holder, start);

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "stop failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!holder.wait().expect("reap holder").success());
    let state = devflow_core::workflow::load_state(root, phase).unwrap();
    assert!(state.stopped);
    assert!(
        state
            .stop_reason
            .as_deref()
            .is_some_and(|r| r.contains("devflow stop"))
    );
}

#[test]
fn stop_writes_no_state_while_the_lock_holder_survives_the_signal() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(104);
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();
    let state_path = root.join(format!(".devflow/state-{}.json", phase.padded()));
    let before = std::fs::read(&state_path).unwrap();
    let mut holder = Command::new("sh")
        .args(["-c", "trap '' TERM; exec sleep 60"])
        .spawn()
        .expect("spawn TERM-ignoring holder");
    let start = devflow_core::agent::process_start_time(holder.id()).expect("holder start time");
    write_live_lock(root, phase, &holder, start);

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        !output.status.success(),
        "stop must refuse to mark state while the holder survives\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&holder.id().to_string()) && stderr.contains("not marked"));
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    assert!(holder.try_wait().expect("poll holder").is_none());
    common::kill_and_reap(&mut holder);
}

#[test]
fn stop_never_signals_the_recorded_monitor_pid() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(105);
    let mut monitor = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn monitor");
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.monitor_pid = Some(monitor.id());
    devflow_core::workflow::save_state(&state).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(output.status.success());
    assert!(monitor.try_wait().expect("poll monitor").is_none());
    assert!(
        devflow_core::workflow::load_state(root, phase)
            .unwrap()
            .stopped
    );
    common::kill_and_reap(&mut monitor);
}

#[test]
fn stop_refuses_to_signal_a_lock_holder_whose_start_time_does_not_match() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(106);
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();
    let state_path = root.join(format!(".devflow/state-{}.json", phase.padded()));
    let before = std::fs::read(&state_path).unwrap();
    let mut holder = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn holder");
    let wrong_start =
        devflow_core::agent::process_start_time(holder.id()).expect("holder start time") + 1;
    write_live_lock(root, phase, &holder, wrong_start);

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to signal"));
    assert!(holder.try_wait().expect("poll holder").is_none());
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    common::kill_and_reap(&mut holder);
}

/// Saves a fresh state for `phase`, starts a live `sleep` as the process the
/// lock names, and returns the state path, its bytes, and the holder.
fn gated_phase_with_foreign_holder(
    root: &Path,
    phase: PhaseId,
) -> (std::path::PathBuf, Vec<u8>, Child) {
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();
    let state_path = root.join(format!(".devflow/state-{}.json", phase.padded()));
    let before = std::fs::read(&state_path).unwrap();
    let holder = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn holder");
    (state_path, before, holder)
}

/// T-48-16-03: a lock naming a recycled pid has no waiter. Ship's manual
/// recovery exception applies only when there is no holder at all; writing an
/// abort response for an unrelated live process would permanently poison the
/// gate for the real recovery path.
#[test]
fn stop_at_a_ship_gate_with_a_recycled_holder_refuses_without_writing_a_response() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(108);
    let (state_path, _before, mut holder) = gated_phase_with_foreign_holder(root, phase);
    let mut state = devflow_core::workflow::load_state(root, phase).unwrap();
    state.stage = Stage::Ship;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();
    let before = std::fs::read(&state_path).unwrap();
    let wrong_start =
        devflow_core::agent::process_start_time(holder.id()).expect("holder start time") + 1;
    write_live_lock(root, phase, &holder, wrong_start);
    Gates::write_gate(root, phase, Stage::Ship, "merge").unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "phase state was not marked, so stop must fail; stdout: {stdout} stderr: {stderr}"
    );
    assert!(
        !stdout.contains("is waiting on the gate") && !stderr.contains("is waiting on the gate"),
        "a recycled pid must never be described as a waiter; stdout: {stdout}"
    );
    assert!(
        stderr.contains("refusing to signal") && stderr.contains("recycled"),
        "stderr: {stderr}"
    );
    assert!(
        !Gates::response_path(root, phase, Stage::Ship).exists(),
        "a recycled Ship holder must not receive an unconsumable abort response"
    );
    assert!(holder.try_wait().expect("poll holder").is_none());
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    common::kill_and_reap(&mut holder);
}

/// Finding B: a legacy single-line lock makes the holder's identity
/// unconfirmable. `stop` must fail rather than return success after writing a
/// response that no confirmed process can consume.
#[test]
fn stop_at_a_gate_with_an_unconfirmable_holder_refuses_unevidenced_success() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(109);
    let (state_path, _before, mut holder) = gated_phase_with_foreign_holder(root, phase);
    let mut state = devflow_core::workflow::load_state(root, phase).unwrap();
    state.stage = Stage::Code;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();
    let before = std::fs::read(&state_path).unwrap();
    let lock = root
        .join(".devflow")
        .join(format!("lock-{}", phase.padded()));
    std::fs::write(lock, holder.id().to_string()).unwrap();
    Gates::write_gate(root, phase, Stage::Code, "paused").unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "stop must not report success without a confirmed response consumer; stdout: {stdout} stderr: {stderr}"
    );
    assert!(
        !stdout.contains("is waiting on the gate"),
        "an unconfirmable holder must not be described as a waiter; stdout: {stdout}"
    );
    assert!(
        stdout.contains("no confirmed live waiter")
            && stdout.contains(&format!("devflow resume --phase {phase}")),
        "stdout: {stdout}"
    );
    assert!(
        stderr.contains("refusing to signal") && stderr.contains("no start time"),
        "stderr: {stderr}"
    );
    assert!(
        !Gates::response_path(root, phase, Stage::Code).exists(),
        "an unconfirmable non-Ship holder must not receive a stale abort response"
    );
    assert!(holder.try_wait().expect("poll holder").is_none());
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    common::kill_and_reap(&mut holder);
}

/// Finding C-3: an old gate request is not evidence that the current live
/// lock holder is polling it. `stop` must signal that holder instead of
/// writing an abort response and reporting an unevidenced successful stop.
#[test]
fn stop_does_not_treat_a_stale_gate_as_the_live_holders_wait() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(112);
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    state.gate_pending = false;
    devflow_core::workflow::save_state(&state).unwrap();

    let mut holder = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn live non-waiting holder");
    let start = devflow_core::agent::process_start_time(holder.id()).expect("holder start time");
    write_live_lock(root, phase, &holder, start);
    Gates::write_gate(root, phase, Stage::Code, "stale request").unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "stale gate must fall through to live-holder stop\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let Some(status) = holder.try_wait().expect("poll holder") else {
        common::kill_and_reap(&mut holder);
        panic!("stop must signal the live holder instead of trusting the stale gate");
    };
    assert!(!status.success(), "SIGTERM must end the live holder");
    assert!(
        !Gates::response_path(root, phase, Stage::Code).exists(),
        "a stale gate must not receive a response"
    );
    assert!(
        devflow_core::workflow::load_state(root, phase)
            .unwrap()
            .stopped,
        "the actually stopped holder must allow state to be marked"
    );
}

/// Finding E: persisting an answer is not evidence that a process will act on
/// it. Ship permits the response for manual `devflow ship` recovery, but its
/// CLI confirmation must not claim that the workflow will advance while no
/// live holder exists.
#[test]
fn gate_approve_without_a_live_holder_does_not_claim_advance() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(110);
    Gates::write_gate(root, phase, Stage::Ship, "approve merge").unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["gate", "approve", &phase.to_string(), "--project"])
        .arg(root)
        .output()
        .expect("run devflow gate approve");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(Gates::response_path(root, phase, Stage::Ship).exists());
    assert!(
        !stdout.contains("workflow will advance"),
        "no-holder response must not claim an outcome: {stdout}"
    );
    assert!(
        stdout.contains("no confirmed live holder will act"),
        "no-holder response must name the evidence limit: {stdout}"
    );
}

/// Wait for `child` to exit, `try_wait`-polling on a short interval rather
/// than blocking indefinitely on `wait()`. On expiry, reaps the child (a
/// bounded `wait()` — the child's own deliberately short
/// `DEVFLOW_GATE_TIMEOUT_SECS` bounds this) so a failing test never leaks a
/// process into the CI runner, then `panic!`s naming the elapsed budget, the
/// child's pid, and what was still true on disk. Never sends a signal to
/// the child — no process-termination call appears anywhere in this file —
/// the only path off this wait is the child exiting on its own, proving the
/// no-signal claim this plan exists to demonstrate.
fn wait_for_child_exit(
    child: &mut std::process::Child,
    root: &Path,
    phase: PhaseId,
    deadline: Duration,
) -> std::process::ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait on devflow advance child") {
            return status;
        }
        if start.elapsed() >= deadline {
            let pid = child.id();
            let lock_present = devflow_core::lock::holder(root, phase).is_some();
            let gate_present = Gates::gate_path(root, phase, Stage::Code).exists();
            let response_present = Gates::response_path(root, phase, Stage::Code).exists();
            // Reap before panicking — see doc comment above for why this
            // stays bounded without ever signalling the child.
            let _ = child.wait();
            panic!(
                "devflow advance (pid {pid}) did not exit within {deadline:?}; \
                 on disk: lock_present={lock_present} gate_present={gate_present} \
                 response_present={response_present}"
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Task 1's core proof: a real, separate `devflow advance` process parked on
/// a Code gate is ended by `devflow stop` writing a rejection file — no
/// signal sent — unwinds through its own `abort()` path, releases its
/// per-phase lock, and leaves a `workflow_aborted` audit event. The orphan
/// class `23-ORPHAN-FORENSICS.md` documented, now remedied.
#[test]
fn stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(95);

    init_repo(root, phase);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    devflow_core::workflow::save_state(&state).unwrap();

    // The child's OWN gate timeout — deliberately short so even a totally
    // failed stop still lets `wait_for_child_exit`'s cleanup `wait()`
    // return promptly. Set per-`Command` via `.env(...)`, never via a
    // process-global env mutation (999.37).
    let mut child = common::hermetic_devflow(&home)
        .args(["advance", "--phase", &phase.to_string()])
        .arg(root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "15")
        .spawn()
        .expect("spawn devflow advance");

    // Wait for the child to acquire the per-phase lock (proves it is inside
    // `advance()`, holding the lock across the gate wait) and to write the
    // Code gate (proves it reached `run_gate_with_timeout` and is now
    // polling for a response).
    wait_for(
        || devflow_core::lock::holder(root, phase).is_some(),
        10,
        "the child to acquire .devflow/lock-95",
    );
    let gate_path = Gates::gate_path(root, phase, Stage::Code);
    wait_for(
        || gate_path.exists(),
        10,
        "the child to write the Code gate",
    );
    assert!(
        devflow_core::lock::holder(root, phase).is_some(),
        "lock must still be held once the gate is written — proof a live poller is genuinely \
         blocking on it"
    );

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "devflow stop failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let status = wait_for_child_exit(&mut child, root, phase, e2e_child_timeout());

    assert!(
        status.success(),
        "devflow advance must exit cleanly once its own abort() path runs, got {status:?}"
    );
    assert!(
        devflow_core::lock::holder(root, phase).is_none(),
        "LockGuard's Drop must have released the per-phase lock — proof the process unwound \
         cleanly through its own code, not by being terminated from outside"
    );

    let events = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap();
    assert!(
        events
            .lines()
            .any(|line| line.contains("\"workflow_aborted\"")),
        "the target process must have run its own abort() path, recording workflow_aborted — \
         the whole claim this plan rests on\nevents:\n{events}"
    );

    // The negative that separates this plan from a reaper that terminates
    // processes: neither `devflow stop` (exercised above) nor this test
    // itself ever sent the child a signal. Checked mechanically by this
    // file's own acceptance grep for signalling call forms (must be zero),
    // not asserted at runtime — there is no API surface here that could
    // send one.
}

/// Task 1 behavior: stop persists the operator's intent on the phase state —
/// `stopped: true` and a `stop_reason` naming `devflow stop` as the cause.
/// No live process contends for the state file here, so this is
/// deterministic (unlike the gated-path fixture above, where a live poller
/// may race a second writer).
#[test]
fn stop_marks_state_stopped_and_records_reason() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(96);

    Gates::write_gate(root, phase, Stage::Ship, "approve merge?").unwrap();
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Ship;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "devflow stop failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let reloaded = devflow_core::workflow::load_state(root, phase).unwrap();
    assert!(reloaded.stopped, "stop must set stopped=true");
    assert!(
        reloaded
            .stop_reason
            .as_deref()
            .is_some_and(|r| r.contains("devflow stop")),
        "stop_reason must name devflow stop as the cause, got {:?}",
        reloaded.stop_reason
    );
}

/// Task 1 behavior: a pre-existing `stop_reason` (e.g. from an earlier
/// `--until` halt) must survive — appended to, not overwritten.
#[test]
fn stop_preserves_pre_existing_stop_reason() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(97);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stopped = true;
    state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
    devflow_core::workflow::save_state(&state).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(output.status.success());

    let reloaded = devflow_core::workflow::load_state(root, phase).unwrap();
    assert!(reloaded.stopped);
    let reason = reloaded.stop_reason.expect("stop_reason must be present");
    assert!(
        reason.contains("stopped after plan completed"),
        "the earlier reason must be preserved, got: {reason}"
    );
    assert!(
        reason.contains("devflow stop"),
        "the new reason must also be recorded, got: {reason}"
    );
}

/// Task 1 behavior: `stop_until` means something different (the requested
/// halt stage for `devflow start --until`) and must be left untouched.
#[test]
fn stop_leaves_stop_until_unchanged() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(98);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stop_until = Some(Stage::Plan);
    devflow_core::workflow::save_state(&state).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(output.status.success());

    let reloaded = devflow_core::workflow::load_state(root, phase).unwrap();
    assert_eq!(reloaded.stop_until, Some(Stage::Plan));
}

/// CLI discoverability: BOTH ways of naming the target must be documented in
/// the subcommand's own `--help` — the `--phase` flag, and (phase 46 / D-11)
/// the `[PROJECT]` positional root.
///
/// The name of this function is cited verbatim as the VALID-02 acceptance
/// command in `46-RESEARCH.md:886` and in `46-VALIDATION.md`; it is widened in
/// place rather than renamed, because a rename silently invalidates the
/// verification map.
#[test]
fn stop_help_documents_phase_flag() {
    let home = common::HermeticHome::new();
    let output = common::hermetic_devflow(&home)
        .args(["stop", "--help"])
        .output()
        .expect("run devflow stop --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--phase"),
        "--help missing --phase:\n{stdout}"
    );
    // The BRACKETED form specifically: the bare word "project" already occurs
    // case-insensitively in the long flag's own description text, so only
    // `[PROJECT]` unambiguously indicates the positional was added.
    assert!(
        stdout.contains("[PROJECT]"),
        "--help missing the [PROJECT] positional:\n{stdout}"
    );
}

/// Task 3 behavior: stop run twice against the same gated phase both return
/// success, and the second run does not modify the response file the first
/// one wrote.
#[test]
fn stop_is_idempotent_against_an_already_answered_gate() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(99);

    Gates::write_gate(root, phase, Stage::Ship, "approve merge?").unwrap();
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Ship;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();

    let run_stop = || {
        common::hermetic_devflow(&home)
            .args(["stop", "--phase", &phase.to_string(), "--root"])
            .arg(root)
            .output()
            .expect("run devflow stop")
    };

    let first = run_stop();
    assert!(
        first.status.success(),
        "first stop failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let response_path = Gates::response_path(root, phase, Stage::Ship);
    let first_bytes =
        std::fs::read(&response_path).expect("response file must exist after first stop");

    let second = run_stop();
    assert!(
        second.status.success(),
        "second stop failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_bytes = std::fs::read(&response_path).expect("response file must still exist");
    assert_eq!(
        first_bytes, second_bytes,
        "the second stop must not modify the response file the first one wrote"
    );
}

/// Task 3 behavior: a response written by hand before `stop` ever runs (a
/// human beat the operator to it) must survive byte-identical — `stop` is a
/// success no-op here, not a clobber.
#[test]
fn stop_against_a_hand_written_response_is_a_success_no_op() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(100);

    Gates::write_gate(root, phase, Stage::Ship, "approve merge?").unwrap();
    Gates::respond(
        root,
        phase,
        Stage::Ship,
        &GateResponse {
            approved: false,
            note: Some("hand-written rejection".into()),
            responded_by: Some("human".into()),
        },
    )
    .unwrap();
    let response_path = Gates::response_path(root, phase, Stage::Ship);
    let before = std::fs::read(&response_path).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "devflow stop failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = std::fs::read(&response_path).unwrap();
    assert_eq!(
        before, after,
        "stop must not clobber an existing hand-written response"
    );
}

/// Task 3 behavior: a root with no persisted state at all — a phase that
/// was never started, or whose state was already cleared — is a success,
/// not an error.
#[test]
fn stop_against_a_root_with_no_state_is_a_success() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(101);

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "stop against a root with no state must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Task 3's composition proof (23-CONTEXT.md's Integration Points, T-23-54):
/// stop makes a phase not-live; `cleanup`'s existing `stopped && !force`
/// refusal (20c/CR-02) must still decline it, and `cleanup --force` must
/// then succeed — the two verbs compose in that order, and `cleanup`'s
/// fail-closed guarantee is provably unweakened by this plan.
#[test]
fn stop_then_cleanup_composes_refuse_then_force() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(102);

    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);

    let branch = format!("feature/phase-{padded}", padded = phase.padded());
    git(root, &["checkout", "-q", "-b", &branch]);
    std::fs::write(root.join("work.txt"), "agent work\n").unwrap();
    git(root, &["add", "work.txt"]);
    git(root, &["commit", "-q", "-m", "agent work"]);
    git(root, &["checkout", "-q", "develop"]);

    let wt_path = root
        .join(".worktrees")
        .join(format!("phase-{padded}", padded = phase.padded()));
    devflow_core::worktree::add(root, &wt_path, &branch, &branch, false).unwrap();

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.worktree_path = Some(wt_path.clone());
    devflow_core::workflow::save_state(&state).unwrap();

    // No open gate, no lock — the lock-holder fallback is a no-op, and the
    // only observable effect of this stop is state.stopped/stop_reason.
    let stopped = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        stopped.status.success(),
        "devflow stop failed: {}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    let stopped_state = devflow_core::workflow::load_state(root, phase).unwrap();
    assert!(
        stopped_state.stopped,
        "stop must have marked the phase stopped before the composition check runs"
    );

    // cleanup without --force must still decline — the fail-closed
    // guarantee this plan's design depends on and does not weaken.
    let refused = common::hermetic_devflow(&home)
        .args(["cleanup"])
        .arg(root)
        .output()
        .expect("run devflow cleanup");
    assert!(
        refused.status.success(),
        "cleanup on a stopped phase must not error, only skip: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(
        wt_path.is_dir(),
        "cleanup without --force must not remove a stop-marked phase's worktree"
    );

    // cleanup --force is the documented escape hatch — the two verbs
    // compose in that order.
    let forced = common::hermetic_devflow(&home)
        .args(["cleanup", "--force"])
        .arg(root)
        .output()
        .expect("run devflow cleanup --force");
    assert!(
        forced.status.success(),
        "cleanup --force must succeed: {}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert!(
        !wt_path.is_dir(),
        "cleanup --force must remove the worktree after stop"
    );
}

/// Phase 46 / D-11 (VALID-02): `stop` must accept its project root as a
/// POSITIONAL argument, the way `start`, `resume`, `status`, `ship` and
/// `approve` already do.
///
/// Asserting only the exit code here would not prove the positional was USED:
/// a subcommand that silently ignored it and defaulted to the current
/// directory would also exit zero. So the state is reloaded from the named
/// root and the stop is asserted to have landed THERE.
#[test]
fn stop_accepts_the_project_root_as_a_positional_argument() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(103);

    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string()])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(
        output.status.success(),
        "devflow stop with a positional root failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let reloaded = devflow_core::workflow::load_state(root, phase).unwrap();
    assert!(
        reloaded.stopped,
        "the positional root must be the root that actually gets stopped"
    );
    assert!(
        reloaded
            .stop_reason
            .as_deref()
            .is_some_and(|r| r.contains("devflow stop")),
        "stop_reason must name devflow stop as the cause, got {:?}",
        reloaded.stop_reason
    );
}

/// Phase 46 / D-12 (VALID-02): when BOTH spellings are supplied the FLAG
/// wins, and the both-supplied case is NOT an error.
///
/// Proven in BOTH orderings deliberately. A single direction is equally
/// consistent with the POSITIONAL silently winning, so one direction alone
/// does not discriminate between the two hypotheses — it is a negative
/// control for the precedence claim, not a redundant second case.
///
/// Both roots carry their own saved state before either invocation runs, so
/// each resolves at depth zero and neither escapes upward through
/// `project_root`'s `.devflow` ancestor walk. That keeps the assertion a
/// measurement of PRECEDENCE rather than of the walk.
#[test]
fn stop_root_flag_takes_precedence_over_the_positional() {
    let home = common::HermeticHome::new();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let root_a = dir_a.path();
    let root_b = dir_b.path();
    let phase = PhaseId::new(104);

    let seed = |root: &Path| {
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        devflow_core::workflow::save_state(&state).unwrap();
    };
    let stopped = |root: &Path| {
        devflow_core::workflow::load_state(root, phase)
            .expect("state must be loadable")
            .stopped
    };

    // Ordering 1 — flag names A, positional names B.
    seed(root_a);
    seed(root_b);
    let first = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root_a)
        .arg(root_b)
        .output()
        .expect("run devflow stop");
    assert!(
        first.status.success(),
        "supplying both spellings must not be an error: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(stopped(root_a), "ordering 1: the flag's root (A) must stop");
    assert!(
        !stopped(root_b),
        "ordering 1: the positional's root (B) must be left alone"
    );

    // Ordering 2 — roles reversed: flag names B, positional names A.
    seed(root_a);
    seed(root_b);
    let second = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root_b)
        .arg(root_a)
        .output()
        .expect("run devflow stop");
    assert!(
        second.status.success(),
        "supplying both spellings must not be an error: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(stopped(root_b), "ordering 2: the flag's root (B) must stop");
    assert!(
        !stopped(root_a),
        "ordering 2: the positional's root (A) must be left alone"
    );
}

/// Phase 46 / D-13 (VALID-02): a genuinely wrong positional root must fail
/// with `project_root`'s own path-naming message, not with a bare clap usage
/// error that names no offending argument.
///
/// The non-zero-exit assertion ALONE is a FALSE POSITIVE and is not
/// acceptable evidence: the clap usage error this case exists to eliminate
/// also exits non-zero, so a bare exit-code check reads green both before and
/// after the fix. The two stderr assertions are the only halves that
/// discriminate.
#[test]
fn stop_against_a_nonexistent_positional_root_names_the_offending_path() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-project-root");
    let phase = PhaseId::new(105);

    assert!(
        !missing.exists(),
        "fixture error: the offending path must NOT exist"
    );

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string()])
        .arg(&missing)
        .output()
        .expect("run devflow stop");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a wrong root must fail:\n{stderr}"
    );
    let missing_display = missing.display().to_string();
    assert!(
        stderr.contains("project path does not exist:"),
        "a wrong root must reach project_root's message, not a clap usage error:\n{stderr}"
    );
    assert!(
        stderr.contains(&missing_display),
        "the message must NAME the offending path ({missing_display}):\n{stderr}"
    );
}

/// Phase 46 / VALID-02 edge "empty": what BOTH spellings do with an empty
/// root, MEASURED against the binary rather than predicted.
///
/// The measurement CORRECTS this plan's own stated expectation. The plan
/// predicted both spellings would reach `project_root` and render
/// `project path does not exist: ` with nothing after the colon. They do not.
/// clap rejects an empty value at the PARSER for both, exit code 2, with its
/// own `a value is required for '<ARG>' but none was supplied`. The two
/// spellings therefore CONVERGE on parser rejection while DIVERGING in which
/// argument the message names — `'--root <ROOT>'` for the flag, `'[PROJECT]'`
/// for the positional. Both name the offending argument, which is what D-13
/// asks for.
///
/// Each half asserts the ABSENCE of `project path does not exist:` as its
/// negative control. Without that, this test would still pass on a build that
/// let the empty value through to a message naming an invisible value — which
/// is the precise legibility failure the plan was worried about.
#[test]
fn stop_with_an_empty_project_root_is_refused_identically_for_both_spellings() {
    let home = common::HermeticHome::new();
    let phase = PhaseId::new(106);

    // Spelling 1 — empty POSITIONAL.
    let positional = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string()])
        .arg("")
        .output()
        .expect("run devflow stop");
    let positional_stderr = String::from_utf8_lossy(&positional.stderr);
    assert!(
        !positional.status.success(),
        "an empty positional root must be refused:\n{positional_stderr}"
    );
    assert!(
        positional_stderr.contains("a value is required for"),
        "the empty positional must be refused by clap:\n{positional_stderr}"
    );
    assert!(
        positional_stderr.contains("[PROJECT]"),
        "the refusal must NAME the positional it rejected:\n{positional_stderr}"
    );
    assert!(
        !positional_stderr.contains("project path does not exist:"),
        "an empty value must never reach project_root:\n{positional_stderr}"
    );

    // Spelling 2 — empty value supplied to the long FLAG.
    let flag = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg("")
        .output()
        .expect("run devflow stop");
    let flag_stderr = String::from_utf8_lossy(&flag.stderr);
    assert!(
        !flag.status.success(),
        "an empty --root value must be refused:\n{flag_stderr}"
    );
    assert!(
        flag_stderr.contains("a value is required for"),
        "the empty flag value must be refused by clap:\n{flag_stderr}"
    );
    assert!(
        flag_stderr.contains("--root"),
        "the refusal must NAME the flag it rejected:\n{flag_stderr}"
    );
    assert!(
        !flag_stderr.contains("project path does not exist:"),
        "an empty value must never reach project_root:\n{flag_stderr}"
    );
}

/// Phase 46 / VALID-02 edge "encoding": the positional is typed `PathBuf`, so
/// clap parses it from the raw `OsString` and a byte-invalid path SURVIVES to
/// `project_root` rather than being rejected as invalid UTF-8. A
/// `String`-typed argument would have failed earlier, with different text.
///
/// No assertion is made on the rendered path bytes: `Path::display` is lossy
/// by design, and asserting on its output would pin an implementation detail
/// of `std` rather than a behaviour DevFlow owns.
#[cfg(unix)]
#[test]
fn stop_positional_root_survives_a_non_utf8_path() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let phase = PhaseId::new(107);

    // A lone 0xFF byte is not valid UTF-8 in any position.
    let mut raw = b"root-".to_vec();
    raw.push(0xFF);
    let name = std::ffi::OsStr::from_bytes(&raw);
    assert!(
        name.to_str().is_none(),
        "fixture error: the name must NOT be valid UTF-8"
    );

    let missing = dir.path().join(name);
    assert!(
        !missing.exists(),
        "fixture error: the offending path must NOT exist"
    );

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string()])
        .arg(&missing)
        .output()
        .expect("run devflow stop");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a non-existent non-UTF-8 root must fail:\n{stderr}"
    );
    assert!(
        stderr.contains("project path does not exist:"),
        "a non-UTF-8 root must survive clap and reach project_root:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// 50-14: `stop` addresses the run's supervisor socket (D-04, D-06).
//
// These tests launch real supervised runs with `devflow start --mode
// supervise` under a `HermeticHome`, so every socket and sidecar lives under
// the test's tempdir cache. A pid a test signals comes from a sidecar and is
// confirmed with `agent::is_same_process` against its recorded start time.
// ---------------------------------------------------------------------------

/// How long a test waits for a supervised run to reach a state.
const RUN_WAIT: Duration = Duration::from_secs(60);

/// A repository on `develop` with a roadmap entry for `phase`, the shape
/// `supervisor_e2e.rs` launches a real supervised run from.
fn init_supervised_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    std::fs::create_dir_all(root.join(".planning")).unwrap();
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    std::fs::write(
        root.join(".planning/ROADMAP.md"),
        format!("# Roadmap\n\n### Phase {phase}: stop fixture\n"),
    )
    .unwrap();
    git(root, &["add", "README.md", ".planning/ROADMAP.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}

/// The fake `claude`'s health check and delivery canary, as in
/// `supervisor_e2e.rs`; `body` runs for the real stage turn.
fn fake_claude(body: &str) -> String {
    format!(
        r#"if [ "$1" = "--version" ]; then echo "2.1.220 (Claude Code)"; exit 0; fi
read -r turn
case "$turn" in
  *DEVFLOW_DELIVERY_CANARY_*)
    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
    printf '{{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}}\n' "$token"
    exit 0
    ;;
esac
{body}"#
    )
}

/// A fake agent that keeps running until `trigger` exists, then commits one
/// empty commit, prints the success marker and exits 0.
fn triggered_agent(trigger: &Path) -> String {
    fake_claude(&format!(
        r#"while [ ! -e '{trigger}' ]; do sleep 0.05; done
git commit -q --allow-empty -m "fake stage work"
printf 'work done\nDEVFLOW_RESULT: {{"status":"success","commits":1}}\n'
exit 0"#,
        trigger = trigger.display()
    ))
}

/// The `stage_launched` events recorded for `phase`.
fn stage_launched_count(root: &Path, phase: PhaseId) -> usize {
    std::fs::read_to_string(devflow_core::events::events_path(root))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| {
            phase.matches_json(event.get("phase"))
                && event["event"].as_str() == Some("stage_launched")
        })
        .count()
}

/// The pids whose process group (`/proc/<pid>/stat` field 5) is `pgid`.
fn group_members(pgid: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|stat| {
                    let rest = &stat[stat.rfind(')')? + 1..];
                    rest.split_whitespace().nth(2)?.parse::<u32>().ok()
                })
                == Some(pgid)
        })
        .collect()
}

/// Poll `done` every 25 ms until it holds or `within` elapses.
fn holds_within(within: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + within;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Ends, on drop, the run a captured supervisor record names, whether or not
/// its state still loads (a test may corrupt it): a socket shutdown, then a
/// SIGKILL to the agent group when its leader is still the recorded process.
struct RecordReaper(SupervisorRecord);

impl Drop for RecordReaper {
    fn drop(&mut self) {
        let _ = supervisor::request_shutdown(&self.0.socket_path, Duration::from_secs(15));
        let leader = self.0.agent_pgid;
        if devflow_core::agent::agent_running(leader)
            && devflow_core::agent::is_same_process(leader, self.0.agent_start_time)
        {
            common::signal_group(leader, libc::SIGKILL);
        }
    }
}

/// One live supervised run of a [`triggered_agent`], started with
/// `devflow start --mode supervise --until plan`. Its reapers are declared
/// first so they drop before the home and repository they need.
struct LiveRun {
    _record_reaper: RecordReaper,
    _launch_reaper: common::LaunchedRunReaper,
    home: common::HermeticHome,
    _repo: tempfile::TempDir,
    _fake_bin: common::FakeBin,
    root: PathBuf,
    phase: PhaseId,
    trigger: PathBuf,
    record: SupervisorRecord,
}

impl LiveRun {
    fn start(phase: PhaseId) -> Self {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        init_supervised_repo(&root, phase);
        let trigger = root.join(".git").join("devflow-test-trigger");
        let fake_bin = common::fake_bin_dir(&triggered_agent(&trigger));
        let launch_reaper = common::LaunchedRunReaper::new(&root, phase, vec![]);
        let existing = std::env::var_os("PATH").unwrap_or_default();
        let path = format!("{}:{}", fake_bin.path.display(), existing.to_string_lossy());
        let output = common::hermetic_devflow(&home)
            .args([
                "start",
                "--phase",
                &phase.to_string(),
                "--agent",
                "claude",
                "--mode",
                "supervise",
                "--no-worktree",
                "--until",
                "plan",
            ])
            .arg(&root)
            .current_dir(&root)
            .env("PATH", path)
            .output()
            .expect("run devflow start");
        assert!(
            output.status.success(),
            "start failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let record = devflow_core::workflow::load_state(&root, phase)
            .expect("start saved state")
            .supervisor
            .expect("start records state.supervisor");
        Self {
            _record_reaper: RecordReaper(record.clone()),
            _launch_reaper: launch_reaper,
            home,
            _repo: repo,
            _fake_bin: fake_bin,
            root,
            phase,
            trigger,
            record,
        }
    }

    /// `devflow stop --phase N` plus `extra`, run from the project root.
    fn stop(&self, extra: &[&str]) -> std::process::Output {
        common::hermetic_devflow(&self.home)
            .args(["stop", "--phase", &self.phase.to_string()])
            .args(extra)
            .current_dir(&self.root)
            .output()
            .expect("run devflow stop")
    }

    fn state_file(&self) -> PathBuf {
        devflow_core::workflow::state_path(&self.root, self.phase)
    }

    fn probe(&self) -> Liveness {
        supervisor::probe(&self.record.socket_path).expect("probe the recorded socket")
    }
}

/// `stop`'s stdout and stderr, for an assertion message.
fn describe(output: &std::process::Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// V-17 (999.138): a live run whose agent holds no per-phase lock is ended
/// through its supervisor's socket: the agent group is gone, the exit file
/// reads 143, no next stage is launched, the socket is Gone, and the state
/// records the stop.
#[test]
fn stop_ends_a_lock_free_live_run_through_the_socket() {
    let run = LiveRun::start(PhaseId::new(140));
    assert!(
        matches!(
            devflow_core::lock::holder_status(&run.root, run.phase),
            devflow_core::lock::HolderStatus::NoHolder
        ),
        "precondition: the running agent holds no per-phase lock"
    );
    assert!(
        matches!(run.probe(), Liveness::Alive(_)),
        "precondition: the run is ALIVE"
    );
    let pgid = run.record.agent_pgid;
    assert!(
        !group_members(pgid).is_empty(),
        "precondition: the agent group is running"
    );

    let output = run.stop(&[]);
    let text = describe(&output);
    assert!(output.status.success(), "stop must succeed: {text}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&format!(
            "stopped phase {} through its supervisor",
            run.phase
        )),
        "stop says it stopped the phase through its supervisor: {text}"
    );
    assert!(
        holds_within(Duration::from_secs(5), || group_members(pgid).is_empty()),
        "the agent group {pgid} is gone"
    );
    let exit = std::fs::read_to_string(devflow_core::agent_result::exit_code_path(
        &run.root, run.phase,
    ))
    .expect("the supervisor records the agent's exit code");
    assert_eq!(exit.trim(), "143", "the agent ended by the stop");
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(
        stage_launched_count(&run.root, run.phase),
        1,
        "no next stage is launched after the stop"
    );
    assert_eq!(run.probe(), Liveness::Gone, "the socket is gone");
    let state = devflow_core::workflow::load_state(&run.root, run.phase).unwrap();
    assert!(state.stopped, "state.stopped after the stop");
    assert!(
        state
            .stop_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("devflow stop")),
        "the stop reason names `devflow stop`: {:?}",
        state.stop_reason
    );
}

/// The V-17 control: the same fixture with no `stop`, whose agent is
/// released by its trigger, completes and advances, so a following
/// `stage_launched` appears. V-17's "no next stage" is therefore the stop's
/// doing, not a fixture that never advances.
#[test]
fn without_stop_a_lock_free_run_completes_and_advances() {
    let run = LiveRun::start(PhaseId::new(141));
    assert_eq!(stage_launched_count(&run.root, run.phase), 1);
    std::fs::write(&run.trigger, "go\n").unwrap();
    assert!(
        holds_within(RUN_WAIT, || stage_launched_count(&run.root, run.phase) >= 2),
        "a released agent must complete and launch the next stage:\n{}",
        std::fs::read_to_string(devflow_core::events::events_path(&run.root)).unwrap_or_default()
    );
}

/// D-09's repair verb: a live run whose state file will not load is ended
/// through its one sidecar-found socket. `stop` exits 0, says the stop was
/// not recorded because the state does not load (naming the file), and
/// leaves the state bytes unchanged. V-17 is its loadable-state control.
#[test]
fn stop_ends_a_live_run_whose_state_will_not_load() {
    let run = LiveRun::start(PhaseId::new(142));
    let pgid = run.record.agent_pgid;
    assert!(
        !group_members(pgid).is_empty(),
        "precondition: agent running"
    );
    let state_file = run.state_file();
    std::fs::write(&state_file, b"{ this is not json").unwrap();
    let before = std::fs::read(&state_file).unwrap();

    let output = run.stop(&[]);
    let text = describe(&output);
    assert!(output.status.success(), "stop must succeed: {text}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!(
            "stopped phase {} through its supervisor",
            run.phase
        )),
        "stop says it stopped the phase: {text}"
    );
    assert!(
        stdout.contains(&state_file.display().to_string())
            && stdout.contains("does not load")
            && stdout.contains("was not recorded"),
        "stop says `stopped` was not recorded and names the state file: {text}"
    );
    assert!(
        holds_within(Duration::from_secs(5), || group_members(pgid).is_empty()),
        "the agent group {pgid} is gone"
    );
    assert_eq!(run.probe(), Liveness::Gone, "the socket is gone");
    assert_eq!(
        std::fs::read(&state_file).unwrap(),
        before,
        "the state that will not load is left byte for byte"
    );
}
