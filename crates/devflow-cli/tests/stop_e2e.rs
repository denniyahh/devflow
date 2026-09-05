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

use devflow_core::gates::{GateResponse, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

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
    let mut child = Command::new(devflow_bin())
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

    let output = Command::new(devflow_bin())
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(96);

    Gates::write_gate(root, phase, Stage::Ship, "approve merge?").unwrap();
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();

    let output = Command::new(devflow_bin())
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(97);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stopped = true;
    state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
    devflow_core::workflow::save_state(&state).unwrap();

    let output = Command::new(devflow_bin())
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(98);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stop_until = Some(Stage::Plan);
    devflow_core::workflow::save_state(&state).unwrap();

    let output = Command::new(devflow_bin())
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
    let output = Command::new(devflow_bin())
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(99);

    Gates::write_gate(root, phase, Stage::Ship, "approve merge?").unwrap();
    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();

    let run_stop = || {
        Command::new(devflow_bin())
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

    let output = Command::new(devflow_bin())
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(101);

    let output = Command::new(devflow_bin())
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
    let stopped = Command::new(devflow_bin())
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
    let refused = Command::new(devflow_bin())
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
    let forced = Command::new(devflow_bin())
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
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(103);

    let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    devflow_core::workflow::save_state(&state).unwrap();

    let output = Command::new(devflow_bin())
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
    let first = Command::new(devflow_bin())
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
    let second = Command::new(devflow_bin())
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
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-project-root");
    let phase = PhaseId::new(105);

    assert!(
        !missing.exists(),
        "fixture error: the offending path must NOT exist"
    );

    let output = Command::new(devflow_bin())
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
    let phase = PhaseId::new(106);

    // Spelling 1 — empty POSITIONAL.
    let positional = Command::new(devflow_bin())
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
    let flag = Command::new(devflow_bin())
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

    let output = Command::new(devflow_bin())
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
