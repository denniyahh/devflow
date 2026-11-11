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

use devflow_core::gates::{GateId, GateResponse, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State, SupervisorRecord};
use devflow_core::supervisor::{self, Liveness};
use devflow_core::test_support::{answer_open_gate, gate_answer_files};
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
/// `Gates::await_answer`'s 60s backoff cap.
fn e2e_child_timeout() -> Duration {
    let secs: u64 = std::env::var("DEVFLOW_E2E_CHILD_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);
    Duration::from_secs(secs)
}

/// `pid`'s start time, read once it is readable: a freshly forked child may not have
/// exec'd yet (999.47).
fn wait_for_start_time(pid: u32) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(start) = devflow_core::agent::process_start_time(pid) {
            return start;
        }
        assert!(Instant::now() < deadline, "no start time for pid {pid}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Hold `phase`'s holder file in this test process, its record naming `pid`. The record
/// is rewritten through a separate truncating open, never the hold's own descriptor, whose
/// offset sits past the claim's record. Bind the hold to a named local: `_` drops it.
fn hold_phase_with_record(
    root: &Path,
    phase: PhaseId,
    pid: u32,
    start: Option<u64>,
) -> devflow_core::liveness::Hold {
    let hold = devflow_core::liveness::claim_phase_holder(root, phase).unwrap();
    let record = match start {
        Some(start) => format!("{pid} {start}\n"),
        None => format!("{pid}\n"),
    };
    std::fs::write(
        devflow_core::liveness::phase_holder_path(root, phase),
        record,
    )
    .unwrap();
    hold
}

/// Carries the fixture's project root to a holder child. Not a DevFlow setting.
const HOLDER_ROOT_ENV: &str = "P50_1_29_ROOT";

/// The child half of a test whose holder claims the file itself: claim, deliberately with
/// no D-18 handler, so this models a holder the signal ends, and wait to be signalled.
fn hold_the_phase_as_a_child(phase: PhaseId) {
    let root = PathBuf::from(std::env::var_os(HOLDER_ROOT_ENV).expect("the parent sets the root"));
    let _hold = devflow_core::liveness::claim_phase_holder(&root, phase).unwrap();
    std::thread::sleep(Duration::from_secs(60));
    panic!("the holder child was never signalled");
}

/// This test binary re-run on `test_name`, once the holder names it.
fn spawn_holder_child(test_name: &str, root: &Path, phase: PhaseId) -> Child {
    let path_dir = tempfile::tempdir().unwrap();
    let mut child = devflow_core::test_support::spawn_test_in_child(
        test_name,
        path_dir.path(),
        &[(HOLDER_ROOT_ENV, root.as_os_str())],
    );
    let holder_path = devflow_core::liveness::phase_holder_path(root, phase);
    let named = |pid: u32| {
        devflow_core::liveness::live_holder(&holder_path)
            .ok()
            .flatten()
            .is_some_and(|(holder, _)| holder == pid)
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    while !named(child.id()) {
        if Instant::now() > deadline {
            common::kill_and_reap(&mut child);
            panic!("the holder child never claimed the phase");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child
}

#[test]
fn stop_marks_stopped_after_the_signalled_holder_exits() {
    use std::os::unix::process::ExitStatusExt;
    const NAME: &str = "stop_marks_stopped_after_the_signalled_holder_exits";
    let phase = PhaseId::new(103);
    if devflow_core::test_support::in_child_test(NAME) {
        hold_the_phase_as_a_child(phase);
        return;
    }
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();
    let mut holder = spawn_holder_child(NAME, root, phase);

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
    assert_eq!(
        holder.wait().expect("reap holder").signal(),
        Some(libc::SIGTERM),
        "the signal ends a holder with no handler"
    );
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
fn stop_writes_no_state_while_the_holder_survives_the_signal() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(104);
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();
    let state_path = root.join(format!(".devflow/state-{}.json", phase.padded()));
    let before = std::fs::read(&state_path).unwrap();
    let mut holder = Command::new("sh")
        .args(["-c", "trap '' TERM; echo ready; exec sleep 60"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn TERM-ignoring holder");
    // A TERM that lands before the trap kills the shell, which is not this case.
    let mut ready = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(holder.stdout.as_mut().expect("the holder's stdout")),
        &mut ready,
    )
    .expect("read the holder's ready line");
    assert_eq!(ready, "ready\n");
    let start = devflow_core::agent::process_start_time(holder.id()).expect("holder start time");
    let _hold = hold_phase_with_record(root, phase, holder.id(), Some(start));

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
    assert!(stderr.contains(&holder.id().to_string()), "{stderr}");
    assert!(stderr.contains("not marked"), "{stderr}");
    assert!(stderr.contains("next safe point"), "{stderr}");
    assert_eq!(std::fs::read(&state_path).unwrap(), before);
    assert!(holder.try_wait().expect("poll holder").is_none());
    common::kill_and_reap(&mut holder);
}

/// T-50-38 (renamed from `stop_never_signals_the_recorded_monitor_pid`):
/// `stop` never signals a pid read from state; it addresses the socket. The
/// state plants a live test-owned process as the agent group of a
/// `supervisor` record whose socket file does not exist, so the probe reads
/// GONE. That process is a confirmed live agent with no live supervisor
/// (D-19), so `stop` names it and the manual kill, exits non-zero and records
/// nothing; the planted process is still alive afterwards. Also covers
/// T-23-51's unit test (removed in 50-22): a pid recorded in state alone is
/// never a signalling target.
#[test]
fn stop_never_signals_a_recorded_pid_it_addresses_the_socket() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(105);
    let mut planted = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn the planted process");
    let start = devflow_core::agent::process_start_time(planted.id()).expect("start time");
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.supervisor = Some(SupervisorRecord::new(
        home.cache_dir().join("sock").join("missing.sock"),
        "launch-missing".to_string(),
        planted.id(),
        start,
        supervisor::boot_id().unwrap_or_default(),
    ));
    devflow_core::workflow::save_state(&state).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    let text = describe(&output);
    assert!(!output.status.success(), "{text}");
    assert!(!text.contains("nothing is running"), "{text}");
    assert!(
        text.contains(&planted.id().to_string())
            && text.contains(&format!("kill -TERM -- -{}", planted.id())),
        "stop names the live agent and the manual kill: {text}"
    );
    assert!(
        planted
            .try_wait()
            .expect("poll the planted process")
            .is_none(),
        "the planted process named only by state is never signalled"
    );
    assert!(
        !devflow_core::workflow::load_state(root, phase)
            .unwrap()
            .stopped,
        "no stop is recorded beside a running agent: {text}"
    );
    common::kill_and_reap(&mut planted);
}

#[test]
fn stop_refuses_to_signal_a_holder_whose_start_time_does_not_match() {
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
    let wrong_start = wait_for_start_time(holder.id()) + 1;
    let _hold = hold_phase_with_record(root, phase, holder.id(), Some(wrong_start));

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

/// Saves a fresh state for `phase`, starts a live `sleep` as the process a
/// holder record can name, and returns the state path, its bytes, and the holder.
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

/// A Validate gate with its gate file and an approval's claim but no answer, the shape a
/// partial close leaves: `stop` says a consumer took it and writes no second answer.
#[test]
fn stop_says_a_claimed_gate_was_not_answered_again() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(112);
    let (_state_path, _before, mut holder) = gated_phase_with_foreign_holder(root, phase);
    let mut state = devflow_core::workflow::load_state(root, phase).unwrap();
    state.stage = Stage::Validate;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();
    let gate = Gates::open_gate(root, phase, Stage::Validate, "finish")
        .unwrap()
        .gate_id;
    let response = GateResponse {
        approved: true,
        note: None,
        responded_by: Some("human".into()),
    };
    Gates::answer(root, phase, Stage::Validate, &gate, &response).unwrap();
    assert!(matches!(
        Gates::claim(root, phase, Stage::Validate, &gate).unwrap(),
        devflow_core::gates::Claim::Claimed
    ));
    std::fs::remove_file(Gates::answer_path(root, phase, Stage::Validate, &gate)).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("was already answered and its answer claimed by its consumer")
            && stdout.contains(gate.as_str()),
        "stdout: {stdout}"
    );
    assert!(
        !Gates::answer_path(root, phase, Stage::Validate, &gate).exists(),
        "stop writes no second answer"
    );
    common::kill_and_reap(&mut holder);
}

/// A held gate lock is named as the reason a gate went unanswered, never reported as a
/// missing waiter, and `stop` goes on to the arms that reach the run without the gate.
#[test]
fn stop_says_a_held_gate_lock_left_its_gate_unanswered() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(113);
    let (_state_path, _before, mut holder) = gated_phase_with_foreign_holder(root, phase);
    let mut state = devflow_core::workflow::load_state(root, phase).unwrap();
    state.stage = Stage::Code;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();
    let (_opened, registration) = Gates::open_registered(root, phase, Stage::Code, "g").unwrap();
    let gate_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(
            root.join(".devflow")
                .join(format!(".gates-{}.lock", phase.padded())),
        )
        .unwrap();
    gate_lock.lock().unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gate was not answered") && stdout.contains(".gates-"),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains("no confirmed live waiter"),
        "a held lock is not a missing waiter: {stdout}"
    );
    assert!(gate_answer_files(root, phase, Stage::Code).is_empty());
    registration.finish();
    common::kill_and_reap(&mut holder);
}

/// With no registered waiter, a gate gets no answer, and a holder record with
/// no start time cannot be confirmed: `stop` signals nothing and fails rather than
/// return success.
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
    let _hold = hold_phase_with_record(root, phase, holder.id(), None);
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
        gate_answer_files(root, phase, Stage::Code).is_empty(),
        "an unconfirmable holder must not receive a stale abort response"
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
    use std::os::unix::process::ExitStatusExt;
    const NAME: &str = "stop_does_not_treat_a_stale_gate_as_the_live_holders_wait";
    let phase = PhaseId::new(112);
    if devflow_core::test_support::in_child_test(NAME) {
        hold_the_phase_as_a_child(phase);
        return;
    }
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    state.gate_pending = false;
    devflow_core::workflow::save_state(&state).unwrap();

    let mut holder = spawn_holder_child(NAME, root, phase);
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
    // `stop` saw the holder's lock released, which comes just before its exit status.
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = holder.try_wait().expect("poll holder") {
            break status;
        }
        if Instant::now() > deadline {
            common::kill_and_reap(&mut holder);
            panic!("stop must signal the live holder instead of trusting the stale gate");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        status.signal(),
        Some(libc::SIGTERM),
        "SIGTERM must end the live holder"
    );
    assert!(
        gate_answer_files(root, phase, Stage::Code).is_empty(),
        "a stale gate must not receive a response"
    );
    assert!(
        devflow_core::workflow::load_state(root, phase)
            .unwrap()
            .stopped,
        "the actually stopped holder must allow state to be marked"
    );
}

/// Finding E: an answer is evidence of nothing unless a process will act on it.
/// With no registered waiter, `gate approve` writes no response, names the repair
/// and never claims that the workflow will advance.
#[test]
fn gate_approve_with_no_registered_waiter_does_not_claim_advance() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(110);
    Gates::write_gate(root, phase, Stage::Validate, "approve the finish").unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["gate", "approve", &phase.to_string(), "--project"])
        .arg(root)
        .output()
        .expect("run devflow gate approve");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(gate_answer_files(root, phase, Stage::Validate).is_empty());
    assert!(
        !stdout.contains("workflow will advance") && !stderr.contains("workflow will advance"),
        "a no-waiter refusal must not claim an outcome: {stdout}{stderr}"
    );
    assert!(
        stderr.contains("no registered waiter") && stderr.contains("no response was written"),
        "a no-waiter refusal must name the evidence limit: {stderr}"
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
            let open_gate_waiter = Gates::list_open(root)
                .into_iter()
                .find(|gate| gate.phase == phase && gate.stage == Stage::Code)
                .and_then(|gate| GateId::parse(&gate.gate_id).ok())
                .map(|gate| Gates::waiter_status(root, phase, Stage::Code, &gate));
            let gate_present = Gates::gate_path(root, phase, Stage::Code).exists();
            let response_present = !gate_answer_files(root, phase, Stage::Code).is_empty();
            // Reap before panicking — see doc comment above for why this
            // stays bounded without ever signalling the child.
            let _ = child.wait();
            panic!(
                "devflow advance (pid {pid}) did not exit within {deadline:?}; \
                 on disk: open_gate_waiter={open_gate_waiter:?} gate_present={gate_present} \
                 response_present={response_present}"
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The id of the gate open at `stage`.
fn open_gate_id(root: &Path, phase: PhaseId, stage: Stage) -> GateId {
    let listed = Gates::list_open(root)
        .into_iter()
        .find(|gate| gate.phase == phase && gate.stage == stage)
        .expect("the gate is listed");
    GateId::parse(&listed.gate_id).expect("the gate has a gate id")
}

/// Whether a process holds `gate`'s waiter registration.
fn gate_registration_is_live(root: &Path, phase: PhaseId, stage: Stage, gate: &GateId) -> bool {
    Gates::waiter_status(root, phase, stage, gate)
        .is_ok_and(|status| status == devflow_core::gates::WaiterStatus::Live)
}

/// Task 1's core proof: a real, separate `devflow advance` process parked on
/// a Code gate is ended by `devflow stop` writing a rejection file — no
/// signal sent — unwinds through its own `abort()` path, finishes its gate
/// registration, and leaves a `workflow_aborted` audit event. The orphan
/// class `23-ORPHAN-FORENSICS.md` documented, now remedied.
#[test]
fn stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(95);

    init_repo(root, phase);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.verification_baseline_captured = true;
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

    // Wait for the child to write the Code gate (proves it reached
    // `run_gate_with_timeout`) and for that gate's waiter registration to read
    // live (proves it is now waiting for a response).
    let gate_path = Gates::gate_path(root, phase, Stage::Code);
    wait_for(
        || gate_path.exists(),
        10,
        "the child to write the Code gate",
    );
    let gate_id = open_gate_id(root, phase, Stage::Code);
    wait_for(
        || gate_registration_is_live(root, phase, Stage::Code, &gate_id),
        10,
        "the child's registration at the Code gate",
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
        !Gates::waiter_path(root, phase, Stage::Code, &gate_id).exists(),
        "the waiter must have finished its registration, removing the file: proof the process \
         unwound cleanly through its own code, since one ended from outside leaves it in place"
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

    Gates::write_gate(root, phase, Stage::Validate, "approve the finish?").unwrap();
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Validate;
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
/// one wrote. The gate's waiter is registered by this test process, so `stop`
/// answers it.
#[test]
fn stop_is_idempotent_against_an_already_answered_gate() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(99);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Validate;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();
    let (_opened, registration) =
        Gates::open_registered(root, phase, Stage::Validate, "approve validation?").unwrap();

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
    let answers = gate_answer_files(root, phase, Stage::Validate);
    assert_eq!(answers.len(), 1, "the first stop writes one answer");
    let response_path = &answers[0];
    let first_bytes =
        std::fs::read(response_path).expect("response file must exist after first stop");

    let second = run_stop();
    assert!(
        second.status.success(),
        "second stop failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_bytes = std::fs::read(response_path).expect("response file must still exist");
    assert_eq!(
        first_bytes, second_bytes,
        "the second stop must not modify the response file the first one wrote"
    );
    registration.finish();
}

/// The id `gate list` prints is the id `gate approve --gate-id` takes, and an id
/// for a gate that has since been replaced answers nothing. The approval that
/// follows with the listed id is the control: the refusal was the id comparison,
/// not a missing waiter.
#[test]
fn gate_approve_takes_the_id_gate_list_shows_and_refuses_a_replaced_one() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(111);
    let g1 = Gates::open_gate(root, phase, Stage::Validate, "first")
        .unwrap()
        .gate_id;
    Gates::close(root, phase, Stage::Validate).unwrap();
    let (opened, registration) =
        Gates::open_registered(root, phase, Stage::Validate, "second").unwrap();
    let g2 = opened.gate_id;
    let approve_with = |gate_id: &str| {
        common::hermetic_devflow(&home)
            .args(["gate", "approve", &phase.to_string(), "--gate-id", gate_id])
            .arg("--project")
            .arg(root)
            .output()
            .expect("run devflow gate approve")
    };

    let list = common::hermetic_devflow(&home)
        .args(["gate", "list"])
        .arg(root)
        .output()
        .expect("run devflow gate list");
    let listed = String::from_utf8_lossy(&list.stdout);
    assert!(listed.contains(g2.as_str()), "{listed}");

    let refused = approve_with(g1.as_str());
    let refusal = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "a replaced id must be refused");
    assert!(
        refusal.contains(g1.as_str()) && refusal.contains(g2.as_str()),
        "the refusal names both gates: {refusal}"
    );
    assert!(gate_answer_files(root, phase, Stage::Validate).is_empty());

    let answered = approve_with(g2.as_str());
    assert!(
        answered.status.success(),
        "{}",
        String::from_utf8_lossy(&answered.stderr)
    );
    assert_eq!(gate_answer_files(root, phase, Stage::Validate).len(), 1);
    registration.finish();
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

    Gates::write_gate(root, phase, Stage::Validate, "approve the finish?").unwrap();
    let response_path = answer_open_gate(
        root,
        phase,
        Stage::Validate,
        &GateResponse {
            approved: false,
            note: Some("hand-written rejection".into()),
            responded_by: Some("human".into()),
        },
    )
    .unwrap();
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
/// POSITIONAL argument, the way `start`, `resume`, `status` and
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

/// What a [`triggered_agent`] does once released: one empty commit, the
/// success marker, exit 0.
const SUCCEEDS: &str = r#"git commit -q --allow-empty -m "fake stage work"
printf 'work done\nDEVFLOW_RESULT: {"status":"success","commits":1}\n'
exit 0"#;

/// What a [`triggered_agent`] does once released: a failed first stage (Plan), which the
/// never-silent path parks at a human gate.
const FAILS_TO_A_GATE: &str = r#"printf 'DEVFLOW_RESULT: {"status":"failed","reason":"gate fixture"}\n'
exit 0"#;

/// A fake agent that keeps running until `trigger` exists, then runs
/// `released` ([`SUCCEEDS`] or [`FAILS_TO_A_GATE`]).
fn triggered_agent(trigger: &Path, released: &str) -> String {
    fake_claude(&format!(
        "while [ ! -e '{trigger}' ]; do sleep 0.05; done\n{released}",
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

/// The pids of the live members of process group `pgid`. Zombies do not
/// count: a CI job container's PID 1 never reaps an orphaned one
/// (`test_support::live_group_members`).
fn group_members(pgid: u32) -> Vec<u32> {
    devflow_core::test_support::live_group_members(pgid)
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
/// `devflow start --mode supervise --until code`, so a released first stage (Plan)
/// hands off to one more. Its reapers are declared
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
    /// A run whose agent succeeds once released.
    fn start(phase: PhaseId) -> Self {
        Self::start_with(phase, SUCCEEDS)
    }

    /// A run whose agent runs `released` once its trigger exists.
    fn start_with(phase: PhaseId, released: &str) -> Self {
        let home = common::HermeticHome::new();
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().canonicalize().unwrap();
        init_supervised_repo(&root, phase);
        let trigger = root.join(".git").join("devflow-test-trigger");
        let fake_bin = common::fake_bin_dir(&triggered_agent(&trigger, released));
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
                "code",
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

/// The agent's work once released: ignore SIGTERM (its `sleep` children
/// inherit that), mark that it does, then idle about 60 s.
const IGNORES_TERM: &str = "trap '' TERM
: > .git/devflow-test-ignores-term
i=0
while [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done
exit 0";

/// A stopped supervisor records its stop under the state write lock, waiting
/// up to 11 s for it, before it sends the final `stopped` line, and its kill
/// of an agent that ignores SIGTERM first takes the 3 s SIGTERM grace. So
/// `stop`'s wait for that line must cover both: here this test holds the
/// write lock for 12.5 s after the shutdown is sent, past an 11 s wait and within
/// the supervisor's 14 s. A wait too short reads the still-answering
/// supervisor as one that "did not answer the shutdown" although it stopped.
#[test]
fn stop_waits_out_a_supervisor_whose_stop_record_waits_on_the_write_lock() {
    const HOLD_AFTER_SHUTDOWN: Duration = Duration::from_millis(12_500);
    let run = LiveRun::start_with(PhaseId::new(156), IGNORES_TERM);
    std::fs::write(&run.trigger, "").unwrap();
    assert!(
        holds_within(Duration::from_secs(10), || run
            .root
            .join(".git/devflow-test-ignores-term")
            .exists()),
        "precondition: the agent ignores SIGTERM"
    );
    let (held, is_held) = std::sync::mpsc::channel();
    let (hold_root, hold_phase) = (run.root.clone(), run.phase);
    let hold = std::thread::spawn(move || {
        devflow_core::workflow::update_state_with(&hold_root, hold_phase, |_| {
            held.send(()).expect("the test waits for the hold");
            std::thread::sleep(HOLD_AFTER_SHUTDOWN);
            Ok::<_, devflow_core::workflow::WorkflowError>((None, ()))
        })
        .expect("the hold writes nothing");
    });
    is_held.recv().expect("the write lock is held");
    let stop = common::hermetic_devflow(&run.home)
        .args(["stop", "--phase", &run.phase.to_string()])
        .current_dir(&run.root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn devflow stop");
    let output = stop.wait_with_output().expect("wait for devflow stop");
    hold.join().expect("the hold's thread");
    let text = describe(&output);
    assert!(
        output.status.success(),
        "stop must report the stop that happened: {text}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&format!(
            "stopped phase {} through its supervisor",
            run.phase
        )),
        "{text}"
    );
    assert!(
        devflow_core::workflow::load_state(&run.root, run.phase)
            .expect("the state loads")
            .stopped,
        "the stop is recorded: {text}"
    );
}

/// V-17 (999.138): a live run with no foreground holder is ended
/// through its supervisor's socket: the agent group is gone, the exit file
/// reads 143, no next stage is launched, the socket is Gone, and the state
/// records the stop.
#[test]
fn stop_ends_a_live_run_with_no_foreground_holder_through_the_socket() {
    let run = LiveRun::start(PhaseId::new(140));
    assert_eq!(
        devflow_core::liveness::live_holder(&devflow_core::liveness::phase_holder_path(
            &run.root, run.phase
        ))
        .unwrap(),
        None,
        "precondition: no foreground process holds the phase"
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
        text.contains("agent group ended"),
        "a stop taken while the agent runs says its group ended: {text}"
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

/// SIGKILL a run's supervisor by the pid in its sidecar, after confirming
/// that pid is still the recorded process, and return once it is gone and
/// its socket probes STALE. Returns the sidecar.
fn kill_supervisor(run: &LiveRun) -> supervisor::Sidecar {
    let sidecar = supervisor::read_sidecar(&run.record.socket_path.with_extension("json"))
        .expect("read the sidecar");
    let pid = sidecar.supervisor_pid;
    assert!(
        devflow_core::agent::is_same_process(pid, sidecar.supervisor_start_time),
        "the sidecar's supervisor {pid} is the recorded process"
    );
    let signed = libc::pid_t::try_from(pid).expect("pid fits pid_t");
    assert!(signed > 1, "refusing to signal pid {signed}");
    // SAFETY: `signed > 1` names the supervisor just identity-checked.
    assert_eq!(
        unsafe { libc::kill(signed, libc::SIGKILL) },
        0,
        "SIGKILL {pid}"
    );
    assert!(
        holds_within(RUN_WAIT, || !devflow_core::agent::agent_running(pid)),
        "the supervisor {pid} is gone after SIGKILL"
    );
    assert!(
        holds_within(Duration::from_secs(10), || matches!(
            supervisor::probe(&run.record.socket_path),
            Ok(Liveness::Stale)
        )),
        "a SIGKILLed supervisor leaves its socket STALE"
    );
    sidecar
}

/// A STALE socket goes through the 50-09 backstop: the supervisor of a live
/// run is SIGKILLed, its agent group outlives it, and `stop` ends that group
/// through the validated pidfd path, reports the members it ended, removes
/// the socket and sidecar, and records `stopped`.
#[test]
fn stop_tears_down_a_stale_supervisor_through_the_backstop() {
    let run = LiveRun::start(PhaseId::new(143));
    let pgid = run.record.agent_pgid;
    kill_supervisor(&run);
    assert!(
        !group_members(pgid).is_empty(),
        "precondition: the orphaned agent group {pgid} outlives its supervisor"
    );

    let output = run.stop(&[]);
    let text = describe(&output);
    assert!(output.status.success(), "stop must succeed: {text}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ended its agent group") && stdout.contains(&pgid.to_string()),
        "stop reports the group and the members it ended: {text}"
    );
    assert!(
        holds_within(Duration::from_secs(5), || group_members(pgid).is_empty()),
        "the agent group {pgid} is gone"
    );
    let socket = &run.record.socket_path;
    assert!(
        !socket.exists() && !socket.with_extension("json").exists(),
        "the socket and sidecar are removed"
    );
    assert_eq!(run.probe(), Liveness::Gone);
    assert!(
        devflow_core::workflow::load_state(&run.root, run.phase)
            .unwrap()
            .stopped,
        "the performed stop is recorded"
    );
}

/// `devflow status` on `run`.
fn status_of(run: &LiveRun) -> std::process::Output {
    common::hermetic_devflow(&run.home)
        .arg("status")
        .current_dir(&run.root)
        .output()
        .expect("run devflow status")
}

/// D-19, on real processes: a supervisor SIGKILLed whose socket and sidecar are then lost,
/// so the probe reads GONE while its agent runs. `status` and `stop` both name the agent and
/// the manual kill, `stop` exits non-zero and records nothing, and the agent is untouched.
/// Control: once the group has ended, `status` suggests `devflow resume` and `stop` reports
/// nothing running and records `stopped`.
#[test]
fn stop_and_status_name_an_agent_whose_supervisor_is_gone() {
    let run = LiveRun::start(PhaseId::new(149));
    let pgid = run.record.agent_pgid;
    let phase = run.phase;
    kill_supervisor(&run);
    let socket = &run.record.socket_path;
    std::fs::remove_file(socket).expect("remove the socket");
    std::fs::remove_file(socket.with_extension("json")).expect("remove the sidecar");
    assert!(
        !group_members(pgid).is_empty(),
        "precondition: the agent group {pgid} outlives its supervisor"
    );
    assert_eq!(
        run.probe(),
        Liveness::Gone,
        "precondition: the probe reads GONE"
    );

    let status = status_of(&run);
    let text = describe(&status);
    assert!(status.status.success(), "{text}");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains(&pgid.to_string()) && stdout.contains(&format!("kill -TERM -- -{pgid}")),
        "status names the live agent and the manual kill: {text}"
    );
    assert!(
        !stdout.lines().any(|line| line.contains("→ devflow resume")),
        "status names no resume that would refuse: {text}"
    );

    let stop = run.stop(&[]);
    let text = describe(&stop);
    assert!(!stop.status.success(), "{text}");
    assert!(
        text.contains(&pgid.to_string()) && text.contains("999.170"),
        "stop names the agent and the backlog entry: {text}"
    );
    assert!(!text.contains("nothing is running"), "{text}");
    assert!(
        !devflow_core::workflow::load_state(&run.root, phase)
            .unwrap()
            .stopped,
        "no stop is recorded beside a running agent: {text}"
    );
    assert!(
        !group_members(pgid).is_empty(),
        "stop signals nothing: the agent group {pgid} still runs"
    );

    let signed = libc::pid_t::try_from(pgid).expect("pgid fits pid_t");
    assert!(signed > 1, "refusing to signal group {signed}");
    // SAFETY: `signed > 1` is the group this test's run recorded, whose members were just
    // listed; the kill is what an operator would do.
    assert_eq!(unsafe { libc::kill(-signed, libc::SIGKILL) }, 0);
    assert!(
        holds_within(RUN_WAIT, || group_members(pgid).is_empty()),
        "the agent group {pgid} ends"
    );

    let status = status_of(&run);
    let text = describe(&status);
    assert!(
        String::from_utf8_lossy(&status.stdout)
            .contains(&format!("devflow resume --phase {phase}")),
        "control: with the agent gone, status suggests resume: {text}"
    );
    let stop = run.stop(&[]);
    let text = describe(&stop);
    assert!(stop.status.success(), "control: {text}");
    assert!(
        String::from_utf8_lossy(&stop.stdout).contains("nothing is running"),
        "control: {text}"
    );
    assert!(
        devflow_core::workflow::load_state(&run.root, phase)
            .unwrap()
            .stopped,
        "control: the stop is recorded: {text}"
    );
}

/// A socket standing in for a supervisor that accepted a stop and has not confirmed it: it
/// answers one ping, then one shutdown with `accepted` only, and holds that connection until
/// its peer closes it.
fn serve_accepted_only(listener: std::os::unix::net::UnixListener, phase: PhaseId) {
    use std::io::{BufRead as _, Read as _, Write as _};
    let ping = serde_json::json!({
        "ok": true,
        "launch_id": "launch-pending",
        "phase": phase.to_string(),
        "stage": "code",
        "launch": "stream",
        "owner": "advancing",
        "agent_pid": 1,
        "agent_pgid": 1,
        "agent_start_time": 0,
        "boot_id": null,
        "supervisor_pid": 1,
    });
    for stream in listener.incoming() {
        let mut stream = stream.expect("accept a connection");
        let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone the stream"));
        let mut request = String::new();
        if reader.read_line(&mut request).is_err() {
            continue;
        }
        if request.contains("\"ping\"") {
            let _ = writeln!(stream, "{ping}");
            continue;
        }
        let _ = writeln!(
            stream,
            r#"{{"ok":true,"accepted":true,"owner":"advancing"}}"#
        );
        let _ = reader.read(&mut [0_u8; 1]);
        return;
    }
}

/// D-13 item 1 on the real binary's stdout, since libtest captures a passing test's print: a
/// stop the supervisor accepted and did not confirm within `stop`'s wait reads as pending.
#[test]
fn stop_prints_that_an_unconfirmed_accepted_stop_is_pending() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(150);
    let socket_dir = tempfile::Builder::new()
        .prefix("dfs")
        .tempdir_in("/tmp")
        .unwrap();
    let socket = socket_dir.path().join("p.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind the socket");
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.supervisor = Some(SupervisorRecord::new(
        socket.clone(),
        "launch-pending".to_string(),
        1,
        0,
        "boot".to_string(),
    ));
    devflow_core::workflow::save_state(&state).unwrap();
    let server = std::thread::spawn(move || serve_accepted_only(listener, phase));

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    server.join().expect("the scripted supervisor");

    let text = describe(&output);
    assert!(output.status.success(), "{text}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("accepted the stop")
            && stdout.contains("next safe point")
            && stdout.contains("`devflow status`"),
        "{text}"
    );
    assert!(!text.contains("nothing was signalled"), "{text}");
    assert!(
        devflow_core::workflow::load_state(root, phase)
            .unwrap()
            .stopped,
        "{text}"
    );
}

/// R-10: a supervise-mode run parked at a gate, holding its registration,
/// whose supervisor is SIGKILLed. `stop` writes no gate response (no live
/// waiter), tears the STALE socket down to GONE (the agent had already
/// exited, so nothing is signalled), names `devflow resume --phase N` (R-8),
/// and the dead waiter's registration reads absent.
#[test]
fn stop_after_a_supervisor_killed_at_a_gate_reaps_and_reports_gone() {
    let run = LiveRun::start_with(PhaseId::new(144), FAILS_TO_A_GATE);
    std::fs::write(&run.trigger, "go\n").unwrap();
    let gate = Gates::gate_path(&run.root, run.phase, Stage::Plan);
    assert!(
        holds_within(RUN_WAIT, || gate.exists()),
        "precondition: the run parks at the Plan gate:\n{}",
        std::fs::read_to_string(devflow_core::events::events_path(&run.root)).unwrap_or_default()
    );
    let gate_id = open_gate_id(&run.root, run.phase, Stage::Plan);
    assert!(
        holds_within(RUN_WAIT, || gate_registration_is_live(
            &run.root,
            run.phase,
            Stage::Plan,
            &gate_id
        )),
        "precondition: the run holds the gate's registration"
    );
    kill_supervisor(&run);

    let output = run.stop(&[]);
    let text = describe(&output);
    assert!(output.status.success(), "stop must succeed: {text}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&format!(
            "the phase is resumable: run `devflow resume --phase {}`",
            run.phase
        )),
        "a teardown that signalled nothing says the phase is resumable (R-8): {text}"
    );
    assert!(
        gate_answer_files(&run.root, run.phase, Stage::Plan).is_empty(),
        "no gate response is written without a live waiter"
    );
    assert_eq!(run.probe(), Liveness::Gone, "the socket is torn down");
    assert_eq!(
        Gates::waiter_status(&run.root, run.phase, Stage::Plan, &gate_id).unwrap(),
        devflow_core::gates::WaiterStatus::Absent,
        "the killed waiter's registration reads absent"
    );
}

/// R-6: a probe error is reported and never reaches the backstop. The
/// recorded socket path holds a regular file; the sidecar beside it records
/// a live test-owned group with its real identity, which a backstop run
/// would end. `stop` exits non-zero naming the path problem, the group is
/// untouched, and nothing is removed or recorded.
#[test]
fn stop_reports_a_probe_error_and_never_runs_the_backstop() {
    use std::os::unix::process::CommandExt as _;
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let phase = PhaseId::new(145);
    let sock_dir = home.cache_dir().join("sock");
    std::fs::create_dir_all(&sock_dir).unwrap();
    let socket = sock_dir.join("not-a-socket.sock");
    std::fs::write(&socket, "a regular file\n").unwrap();
    let mut group = Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .expect("spawn a test-owned group");
    let pgid = group.id();
    let start = devflow_core::agent::process_start_time(pgid).expect("start time");
    let boot = supervisor::boot_id().expect("a readable boot id");
    let sidecar = serde_json::json!({
        "launch_id": "aaaa0000aaaa000e",
        "project_root": root,
        "phase": phase,
        "stage": "code",
        "agent": "claude",
        "launch": "stream",
        "agent_pid": pgid,
        "agent_pgid": pgid,
        "agent_start_time": start,
        "boot_id": boot,
        "supervisor_pid": 1,
        "supervisor_start_time": 0,
        "created_at": "2026-10-18T00:00:00Z",
    });
    std::fs::write(socket.with_extension("json"), sidecar.to_string()).unwrap();
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.clone());
    state.supervisor = Some(SupervisorRecord::new(
        socket.clone(),
        "aaaa0000aaaa000e".to_string(),
        pgid,
        start,
        boot,
    ));
    devflow_core::workflow::save_state(&state).unwrap();
    let state_file = devflow_core::workflow::state_path(&root, phase);
    let before = std::fs::read(&state_file).unwrap();

    let output = common::hermetic_devflow(&home)
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(&root)
        .output()
        .expect("run devflow stop");
    let text = describe(&output);
    assert!(
        !output.status.success(),
        "a probe error is a refusal: {text}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&socket.display().to_string()) && stderr.contains("not a socket"),
        "the refusal names the path and its problem: {text}"
    );
    assert!(
        group.try_wait().expect("poll the group").is_none(),
        "the group the sidecar records is untouched"
    );
    assert!(socket.is_file() && socket.with_extension("json").is_file());
    assert_eq!(std::fs::read(&state_file).unwrap(), before);
    common::signal_group(pgid, libc::SIGKILL);
    let _ = group.wait();
}

/// What a planted orphan socket has beside it.
enum OrphanSidecar {
    /// The live run's sidecar, round-tripped through serde with this launch
    /// id and the identity of a test child already reaped.
    Parsed,
    /// A sidecar file that does not parse.
    Broken,
    /// No sidecar file.
    Missing,
}

/// Plant a STALE socket for the run's root and phase with launch id `id`
/// (a listener bound and dropped) and the sidecar `kind` describes. Returns
/// the socket path.
fn plant_stale_orphan(run: &LiveRun, id: &str, kind: OrphanSidecar) -> PathBuf {
    let socket =
        supervisor::SocketPath::for_launch(&run.home.cache_dir(), &run.root, run.phase, id)
            .expect("the orphan socket path fits sun_path");
    drop(supervisor::publish(&socket).expect("publish the orphan socket"));
    let path = socket.as_path().to_path_buf();
    assert!(
        matches!(supervisor::probe(&path), Ok(Liveness::Stale)),
        "the planted orphan {id} is STALE"
    );
    match kind {
        OrphanSidecar::Parsed => {
            let mut reaped = Command::new("sleep")
                .arg("60")
                .spawn()
                .expect("spawn a child to reap");
            let start =
                devflow_core::agent::process_start_time(reaped.id()).expect("its start time");
            common::kill_and_reap(&mut reaped);
            let live = supervisor::read_sidecar(&run.record.socket_path.with_extension("json"))
                .expect("the live run's sidecar");
            let mut value = serde_json::to_value(&live).unwrap();
            value["launch_id"] = serde_json::json!(id);
            value["agent_pid"] = serde_json::json!(reaped.id());
            value["agent_pgid"] = serde_json::json!(reaped.id());
            value["agent_start_time"] = serde_json::json!(start);
            std::fs::write(socket.sidecar_path(), value.to_string()).unwrap();
            supervisor::read_sidecar(&socket.sidecar_path()).expect("the planted sidecar parses");
        }
        OrphanSidecar::Broken => std::fs::write(socket.sidecar_path(), "{").unwrap(),
        OrphanSidecar::Missing => {}
    }
    path
}

/// The socket and sidecar of `socket` both exist.
fn launch_files_present(socket: &Path) -> bool {
    socket.exists() && socket.with_extension("json").exists()
}

/// The run's supervisor answers ping with its own launch id.
fn alive_as(run: &LiveRun) -> bool {
    matches!(run.probe(), Liveness::Alive(reply) if reply.launch_id == run.record.launch_id)
}

/// D-06 / T-50-53: `stop --launch <id>` tears down exactly the socket with
/// that launch id. A non-matching id and a phase mismatch change nothing;
/// the matching id removes the superseded orphan A and leaves the live run B
/// running, its state unstopped and no gate answered.
#[test]
fn stop_with_a_launch_id_tears_down_only_that_socket() {
    let run = LiveRun::start(PhaseId::new(146));
    let orphan = plant_stale_orphan(&run, "0000000000000aaa", OrphanSidecar::Parsed);
    let other_phase = PhaseId::new(147).to_string();

    let nope = run.stop(&["--launch", "nope"]);
    assert!(nope.status.success(), "{}", describe(&nope));
    assert!(
        String::from_utf8_lossy(&nope.stdout).contains("nothing to stop"),
        "a non-matching id says nothing matched: {}",
        describe(&nope)
    );
    assert!(launch_files_present(&orphan) && alive_as(&run));

    let mismatch = common::hermetic_devflow(&run.home)
        .args([
            "stop",
            "--phase",
            &other_phase,
            "--launch",
            "0000000000000aaa",
        ])
        .current_dir(&run.root)
        .output()
        .expect("run devflow stop");
    assert!(
        !mismatch.status.success(),
        "a phase mismatch is refused: {}",
        describe(&mismatch)
    );
    assert!(launch_files_present(&orphan) && alive_as(&run));

    let output = run.stop(&["--launch", "0000000000000aaa"]);
    let text = describe(&output);
    assert!(output.status.success(), "{text}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("launch 0000000000000aaa"),
        "stop names the launch it tore down: {text}"
    );
    assert!(
        !orphan.exists() && !orphan.with_extension("json").exists(),
        "A's socket and sidecar are removed: {text}"
    );
    assert!(alive_as(&run), "B still answers with its own launch id");
    assert!(
        !group_members(run.record.agent_pgid).is_empty(),
        "B's agent group is alive"
    );
    let state = devflow_core::workflow::load_state(&run.root, run.phase).unwrap();
    assert!(!state.stopped, "a targeted stop records nothing");
    assert!(
        gate_answer_files(&run.root, run.phase, Stage::Plan).is_empty(),
        "a targeted stop answers no gate"
    );
}

/// The control for the targeted form: the same fixture with a bare
/// `devflow stop --phase N` stops the run the state records (B) and leaves
/// the superseded socket A, so the two forms address different sockets.
#[test]
fn bare_stop_addresses_the_state_record_and_leaves_a_superseded_socket() {
    let run = LiveRun::start(PhaseId::new(148));
    let orphan = plant_stale_orphan(&run, "0000000000000aaa", OrphanSidecar::Parsed);

    let output = run.stop(&[]);
    assert!(output.status.success(), "{}", describe(&output));
    assert_eq!(run.probe(), Liveness::Gone, "B is stopped");
    assert!(
        devflow_core::workflow::load_state(&run.root, run.phase)
            .unwrap()
            .stopped
    );
    assert!(
        launch_files_present(&orphan),
        "the superseded socket A is untouched"
    );
}

/// D-06: `recover --clean` is a file cleaner and refuses a run whose
/// supervisor is ALIVE, deleting nothing.
#[test]
fn recover_clean_refuses_while_the_supervisor_is_alive() {
    let run = LiveRun::start(PhaseId::new(149));
    let gate = devflow_core::gates::Gates::write_gate(
        &run.root,
        run.phase,
        Stage::Validate,
        "planted gate",
    )
    .expect("plant a gate file");
    let gate_bytes = std::fs::read(&gate).unwrap();
    let state_bytes = std::fs::read(run.state_file()).unwrap();

    let output = common::hermetic_devflow(&run.home)
        .args(["recover", "--clean", "--phase", &run.phase.to_string()])
        .current_dir(&run.root)
        .output()
        .expect("run devflow recover");
    let text = describe(&output);
    assert!(!output.status.success(), "a live run is refused: {text}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains(&format!("devflow stop --phase {}", run.phase)),
        "the refusal names the stop verb: {text}"
    );
    assert_eq!(std::fs::read(run.state_file()).unwrap(), state_bytes);
    assert_eq!(std::fs::read(&gate).unwrap(), gate_bytes);
    assert!(alive_as(&run), "the run is untouched");
}

/// The `devflow stop ... --launch <id>` line `stop` printed for `id`, as
/// arguments after the program name.
fn printed_launch_line(output: &std::process::Output, id: &str) -> Vec<String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let wanted = format!("--launch {id}");
    let line = stderr
        .lines()
        .find(|line| line.trim_end().ends_with(&wanted))
        .unwrap_or_else(|| panic!("no printed line ends with `{wanted}`:\n{stderr}"));
    let command = &line[line
        .find("devflow stop")
        .expect("the line holds the command")..];
    command
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect()
}

/// T-50-65: several live sockets of one root and phase with a state file
/// that does not load are a refusal listing each `--launch` form, acting on
/// neither. The printed line for B, run as printed, then stops B and leaves
/// the corrupt state bytes unchanged.
#[test]
fn stop_refuses_several_sockets_when_the_state_will_not_load() {
    let run = LiveRun::start(PhaseId::new(150));
    let orphan = plant_stale_orphan(&run, "0000000000000aaa", OrphanSidecar::Parsed);
    std::fs::write(run.state_file(), b"{ this is not json").unwrap();
    let corrupt = std::fs::read(run.state_file()).unwrap();
    let b_id = run.record.launch_id.clone();

    let refused = run.stop(&[]);
    let text = describe(&refused);
    assert!(!refused.status.success(), "several sockets: {text}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    for id in [b_id.as_str(), "0000000000000aaa"] {
        assert!(
            stderr.contains(&format!("devflow stop --phase {} --launch {id}", run.phase)),
            "the refusal lists the --launch form for {id}: {text}"
        );
    }
    assert!(alive_as(&run) && launch_files_present(&orphan), "{text}");

    let args = printed_launch_line(&refused, &b_id);
    let output = common::hermetic_devflow(&run.home)
        .args(&args)
        .current_dir(&run.root)
        .output()
        .expect("run the printed line");
    assert!(
        output.status.success(),
        "the printed repair works: {args:?}\n{}",
        describe(&output)
    );
    assert_eq!(run.probe(), Liveness::Gone, "B is stopped");
    assert!(launch_files_present(&orphan), "A is untouched");
    assert_eq!(std::fs::read(run.state_file()).unwrap(), corrupt);
}

/// RV-05b (V-39): with both the state and B's sidecar unparseable, the
/// refusal names B by the launch id in its file name; a non-matching id and
/// a wrong phase change nothing; the printed `--launch` line stops B and its
/// agent group, leaving A and the corrupt state bytes alone.
#[test]
fn stop_with_a_launch_id_stops_a_live_socket_whose_sidecar_will_not_parse() {
    let run = LiveRun::start(PhaseId::new(151));
    let orphan = plant_stale_orphan(&run, "0000000000000aaa", OrphanSidecar::Parsed);
    let sidecar_path = run.record.socket_path.with_extension("json");
    let sidecar = supervisor::read_sidecar(&sidecar_path).expect("B's sidecar");
    let (pgid, start) = (sidecar.agent_pgid, sidecar.agent_start_time);
    let b_id = run.record.launch_id.clone();
    assert!(
        run.record
            .socket_path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|stem| stem.ends_with(&format!("-{b_id}"))),
        "B's launch id is the last segment of its file name"
    );
    std::fs::write(run.state_file(), b"{ this is not json").unwrap();
    std::fs::write(&sidecar_path, b"{ nor is this").unwrap();
    let corrupt = std::fs::read(run.state_file()).unwrap();

    let refused = run.stop(&[]);
    let text = describe(&refused);
    assert!(!refused.status.success(), "{text}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    let b_line = format!("devflow stop --phase {} --launch {b_id}", run.phase);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains(&b_line) && line.contains("sidecar does not parse")),
        "B is listed by its file name's launch id, marked unparseable: {text}"
    );
    assert!(
        stderr.contains(&format!(
            "devflow stop --phase {} --launch 0000000000000aaa",
            run.phase
        )),
        "{text}"
    );
    assert!(alive_as(&run) && launch_files_present(&orphan));

    let nope = run.stop(&["--launch", "nope"]);
    assert!(nope.status.success(), "{}", describe(&nope));
    assert!(String::from_utf8_lossy(&nope.stdout).contains("nothing to stop"));
    assert!(alive_as(&run), "a non-matching id leaves B running");

    let wrong_phase = common::hermetic_devflow(&run.home)
        .args([
            "stop",
            "--phase",
            &PhaseId::new(152).to_string(),
            "--launch",
            &b_id,
        ])
        .current_dir(&run.root)
        .output()
        .expect("run devflow stop");
    assert!(
        !wrong_phase.status.success(),
        "B's file name names phase {}: {}",
        run.phase,
        describe(&wrong_phase)
    );
    assert!(alive_as(&run), "a wrong phase leaves B running");

    assert!(
        devflow_core::agent::is_same_process(pgid, start),
        "precondition: B's agent leader {pgid} is the recorded process"
    );
    let args = printed_launch_line(&refused, &b_id);
    let output = common::hermetic_devflow(&run.home)
        .args(&args)
        .current_dir(&run.root)
        .output()
        .expect("run the printed line");
    assert!(output.status.success(), "{args:?}\n{}", describe(&output));
    assert_eq!(run.probe(), Liveness::Gone, "B is stopped");
    assert!(
        holds_within(Duration::from_secs(5), || group_members(pgid).is_empty()),
        "B's agent group {pgid} is gone"
    );
    assert!(launch_files_present(&orphan), "A is untouched");
    assert_eq!(std::fs::read(run.state_file()).unwrap(), corrupt);
}

/// RV-05b (V-40): a STALE socket with no usable sidecar has no identity to
/// validate, so `--launch` refuses it with nothing signalled or removed,
/// for a sidecar that does not parse and for a missing one. Control: a STALE
/// socket whose sidecar parses, in the same fixture, is torn down.
#[test]
fn stop_with_a_launch_id_refuses_a_stale_socket_without_a_usable_sidecar() {
    let run = LiveRun::start(PhaseId::new(153));
    let a = plant_stale_orphan(&run, "0000000000000aaa", OrphanSidecar::Parsed);
    let c = plant_stale_orphan(&run, "0000000000000ccc", OrphanSidecar::Broken);
    let d = plant_stale_orphan(&run, "0000000000000ddd", OrphanSidecar::Missing);

    for (id, socket, why) in [
        ("0000000000000ccc", &c, "does not parse"),
        ("0000000000000ddd", &d, "missing"),
    ] {
        let output = run.stop(&["--launch", id]);
        let text = describe(&output);
        assert!(!output.status.success(), "{id} is refused: {text}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("STALE") && stderr.contains(why) && stderr.contains("signals nothing"),
            "{id}: the refusal says STALE, why the sidecar is unusable, and that nothing is \
             signalled: {text}"
        );
        assert!(socket.exists(), "{id}'s socket is kept");
        assert!(alive_as(&run), "B is untouched");
    }
    assert_eq!(std::fs::read(c.with_extension("json")).unwrap(), b"{");
    assert!(!d.with_extension("json").exists());

    let output = run.stop(&["--launch", "0000000000000aaa"]);
    assert!(output.status.success(), "{}", describe(&output));
    assert!(!a.exists() && !a.with_extension("json").exists());
    assert!(alive_as(&run), "B is untouched");
}

/// Plant a STALE socket and a parsed sidecar for `(project_root, phase,
/// id)` under `home`'s cache, recording a test child already reaped as its
/// agent group. Returns the socket path.
fn plant_unattached_orphan(
    home: &common::HermeticHome,
    project_root: &Path,
    phase: PhaseId,
    id: &str,
) -> PathBuf {
    let socket = supervisor::SocketPath::for_launch(&home.cache_dir(), project_root, phase, id)
        .expect("the orphan socket path fits sun_path");
    drop(supervisor::publish(&socket).expect("publish the orphan socket"));
    let mut reaped = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn a child to reap");
    let start = devflow_core::agent::process_start_time(reaped.id()).expect("its start time");
    common::kill_and_reap(&mut reaped);
    let sidecar = serde_json::json!({
        "launch_id": id,
        "project_root": project_root,
        "phase": phase,
        "stage": "code",
        "agent": "claude",
        "launch": "stream",
        "agent_pid": reaped.id(),
        "agent_pgid": reaped.id(),
        "agent_start_time": start,
        "boot_id": supervisor::boot_id().expect("a readable boot id"),
        "supervisor_pid": 1,
        "supervisor_start_time": 0,
        "created_at": "2026-10-18T00:00:00Z",
    });
    std::fs::write(socket.sidecar_path(), sidecar.to_string()).unwrap();
    supervisor::read_sidecar(&socket.sidecar_path()).expect("the planted sidecar parses");
    socket.as_path().to_path_buf()
}

/// 50-12's repair for a deleted root's orphan works by launch id alone,
/// from any directory, with no project state: `devflow doctor` prints
/// `devflow stop --phase N --launch <id>` for it, and that line, run from an
/// unrelated directory, tears it down. Control: an orphan whose sidecar
/// names another project that still exists is refused there and kept.
#[test]
fn stop_with_a_launch_id_reaches_a_deleted_roots_orphan_from_any_directory() {
    let home = common::HermeticHome::new();
    let dirs = tempfile::tempdir().unwrap();
    let base = dirs.path().canonicalize().unwrap();
    let (gone_root, other_root, elsewhere) = (
        base.join("gone"),
        base.join("other"),
        base.join("elsewhere"),
    );
    for dir in [&gone_root, &other_root, &elsewhere] {
        std::fs::create_dir(dir).unwrap();
    }
    let phase = PhaseId::new(154);
    let deleted = plant_unattached_orphan(&home, &gone_root, phase, "0000000000000eee");
    let foreign = plant_unattached_orphan(&home, &other_root, phase, "0000000000000fff");
    std::fs::remove_dir(&gone_root).unwrap();
    let repair = format!("devflow stop --phase {phase} --launch 0000000000000eee");

    let doctor = common::hermetic_devflow(&home)
        .arg("doctor")
        .current_dir(&elsewhere)
        .output()
        .expect("run devflow doctor");
    assert!(
        String::from_utf8_lossy(&doctor.stdout).contains(&repair),
        "doctor prints `{repair}` for the deleted root's orphan: {}",
        describe(&doctor)
    );

    let refused = common::hermetic_devflow(&home)
        .args([
            "stop",
            "--phase",
            &phase.to_string(),
            "--launch",
            "0000000000000fff",
        ])
        .current_dir(&elsewhere)
        .output()
        .expect("run devflow stop");
    assert!(
        !refused.status.success()
            && String::from_utf8_lossy(&refused.stderr).contains("belongs to project"),
        "another existing project's orphan is refused: {}",
        describe(&refused)
    );
    assert!(launch_files_present(&foreign), "the refused orphan is kept");

    let args: Vec<&str> = repair.split_whitespace().skip(1).collect();
    let output = common::hermetic_devflow(&home)
        .args(&args)
        .current_dir(&elsewhere)
        .output()
        .expect("run the doctor's repair");
    assert!(output.status.success(), "{}", describe(&output));
    assert!(
        !deleted.exists() && !deleted.with_extension("json").exists(),
        "the deleted root's orphan is torn down: {}",
        describe(&output)
    );
}

/// `--launch` with the launch the state itself records runs the plain stop,
/// every arm included: the run is stopped through its socket and the plain
/// stop's own reason is recorded beside the supervisor's, which a targeted
/// stop (it writes no state) never adds.
#[test]
fn stop_with_the_recorded_launch_id_runs_the_plain_stop() {
    let run = LiveRun::start(PhaseId::new(155));
    let output = run.stop(&["--launch", &run.record.launch_id]);
    assert!(output.status.success(), "{}", describe(&output));
    assert_eq!(run.probe(), Liveness::Gone, "the recorded run is stopped");
    let state = devflow_core::workflow::load_state(&run.root, run.phase).unwrap();
    assert!(state.stopped, "the run is recorded stopped");
    assert!(
        state
            .stop_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("stopped via `devflow stop`")),
        "the plain stop's own reason is recorded: {:?}",
        state.stop_reason
    );
}
