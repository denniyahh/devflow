//! End-to-end proof for `devflow gate sweep` (23b, the acting half of bound
//! gate lifetime): an aged, abandoned gate is answered with a rejection a
//! real live poller picks up and resolves to abort — no process is ever
//! signalled.
//!
//! `commands::gate_sweep` is `pub(crate)` inside the `devflow` BINARY crate
//! (no `lib.rs`, so integration tests cannot link against it at all), so —
//! like every other devflow-cli integration test (`phase7_cli.rs`,
//! `release_check.rs`) — this drives the real compiled binary via
//! `Command::new`, not an in-process call.

use devflow_core::gates::{GateAction, GateFile, Gates};
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
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
/// commit, hooks disabled — the same shape `pipeline_launch.rs`'s
/// `code_unknown_does_not_transition_to_validate` test uses to reach an
/// `AgentStatus::Unknown` Code outcome (no exit-code capture file written).
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
/// production code): how long this test's patience with the spawned child
/// extends before the bounded wait below panics instead of hanging CI
/// indefinitely. Defaults comfortably above `Gates::poll_response`'s 60s
/// backoff cap; tighten via the env var in CI if 90s is too generous.
fn e2e_child_timeout() -> Duration {
    let secs: u64 = std::env::var("DEVFLOW_E2E_CHILD_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);
    Duration::from_secs(secs)
}

/// Wait for `child` to exit, `try_wait`-polling on a short interval rather
/// than blocking indefinitely on `wait()`. On expiry, reaps the child (via
/// a bounded `wait()` — the child's own deliberately short
/// `DEVFLOW_GATE_TIMEOUT_SECS` bounds this, so it is never an unbounded
/// block) so a failing test never leaks a process into the CI runner, then
/// `panic!`s naming the elapsed budget, the child's pid, and what was still
/// true on disk — a diagnosable failure instead of a silent CI hang. Never
/// sends a signal to the child — no process-termination call appears
/// anywhere in this file — the only path off this wait is the child
/// exiting on its own.
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

/// Overwrite an already-written gate file's `timestamp` so it reads as
/// `age_secs` old — the deterministic way to make a gate look abandoned
/// without sleeping in the test. Round-trips through the same [`GateFile`]
/// shape `Gates::write_gate` itself writes.
fn backdate_gate(root: &Path, phase: PhaseId, stage: Stage, age_secs: u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let gate = GateFile {
        phase,
        stage,
        context: "abandoned run".to_string(),
        timestamp: now.saturating_sub(age_secs).to_string(),
    };
    std::fs::write(
        Gates::gate_path(root, phase, stage),
        serde_json::to_string_pretty(&gate).unwrap(),
    )
    .unwrap();
}

/// An age past any plausible `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` default.
///
/// Deliberately a wide margin rather than "just past the threshold": this is
/// an integration test, so it cannot read `config_parse`'s private default,
/// and the previous literal (7h, chosen against a six-hour default) silently
/// stopped reaching the reap path when that default moved to three days. The
/// boundary itself is covered by the unit tests in `commands.rs`; what this
/// test exists to prove is the live-poller seam, which needs only that the
/// gate be unambiguously aged.
const AGED_WELL_PAST_DEFAULT_THRESHOLD_SECS: u64 = 30 * 24 * 60 * 60;

/// The plan's core claim: a gate older than the default threshold is answered
/// with a rejection, and a real `Gates::poll_response` thread — the exact
/// live-poller seam `run_gate_with_timeout` blocks on in production — picks
/// it up and resolves to `GateAction::Abort`.
#[test]
fn sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(91);
    let stage = Stage::Ship;

    Gates::write_gate(root, phase, stage, "approve merge?").unwrap();
    backdate_gate(root, phase, stage, AGED_WELL_PAST_DEFAULT_THRESHOLD_SECS);

    std::thread::scope(|scope| {
        let poller = scope.spawn(move || Gates::poll_response(root, phase, stage, 30));

        let output = Command::new(devflow_bin())
            .args(["gate", "sweep", "--root"])
            .arg(root)
            .output()
            .expect("run devflow gate sweep");
        assert!(
            output.status.success(),
            "devflow gate sweep failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let response = poller
            .join()
            .expect("poller thread")
            .expect("a live Gates::poll_response must observe the sweep's response within 30s");
        assert!(
            !response.approved,
            "a reap must never write an approval (T-23-41)"
        );
        assert!(
            matches!(GateAction::from_response(&response), GateAction::Abort(_)),
            "an aged gate's reap must resolve to Abort, not a Code loop-back"
        );
    });
}

/// The fail-safe counterpart: a gate younger than the threshold is left
/// completely untouched by an invoked sweep — no response file, no state
/// change.
#[test]
fn sweep_leaves_a_fresh_gate_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(92);
    let stage = Stage::Validate;

    // Freshly written — `unix_now()`'s age is ~0s, well under the 6h default.
    Gates::write_gate(root, phase, stage, "review gaps").unwrap();

    let output = Command::new(devflow_bin())
        .args(["gate", "sweep", "--root"])
        .arg(root)
        .output()
        .expect("run devflow gate sweep");
    assert!(
        output.status.success(),
        "devflow gate sweep failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let open = Gates::list_open(root);
    assert_eq!(
        open.len(),
        1,
        "a fresh gate must remain open after an invoked sweep"
    );
    assert_eq!(open[0].phase, phase);
    assert_eq!(open[0].stage, stage);
    assert!(
        !Gates::response_path(root, phase, stage).exists(),
        "a fresh gate must get no response file at all"
    );
}

/// CLI discoverability: `--max-age-secs` and `--dry-run` must be documented
/// in the subcommand's own `--help`.
#[test]
fn sweep_help_documents_max_age_and_dry_run() {
    let output = Command::new(devflow_bin())
        .args(["gate", "sweep", "--help"])
        .output()
        .expect("run devflow gate sweep --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--max-age-secs"),
        "--help missing --max-age-secs:\n{stdout}"
    );
    assert!(
        stdout.contains("--dry-run"),
        "--help missing --dry-run:\n{stdout}"
    );
}

/// Task 3's strongest proof: a **real, separate `devflow advance` process**
/// parked on a gate is ended by a file write, unwinds through its own
/// `abort()` path, releases its per-phase lock, and leaves an audit record —
/// the orphan class `23-ORPHAN-FORENSICS.md` documented, now remedied
/// without `kill(1)`.
///
/// Setup mirrors `pipeline_launch.rs`'s `code_unknown_does_not_transition_to_validate`
/// test exactly (a Code-stage phase with no exit-code capture file yields
/// `AgentStatus::Unknown`, which `handle_stage_failure`'s never-silent gate
/// answers with a Code gate) — except the target here is spawned as a real
/// OS process via `Command::new`, not driven in-process on a thread.
#[test]
fn sweep_ends_a_real_advance_process_through_its_own_abort_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(95);

    init_repo(root, phase);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    devflow_core::workflow::save_state(&state).unwrap();

    // The child's OWN gate timeout — deliberately short (comfortably above
    // this setup's own latency, comfortably below this test's outer
    // patience) so that even a totally failed reap still lets
    // `wait_for_child_exit`'s cleanup `wait()` return promptly rather than
    // block for `gate_timeout_secs()`'s multi-day production default.
    // Set per-`Command` via `.env(...)`, not via a process-global env
    // mutation (999.37) — so this cannot race any other test.
    let mut child = Command::new(devflow_bin())
        .args(["advance", "--phase", &phase.to_string()])
        .arg(root)
        .env("DEVFLOW_GATE_TIMEOUT_SECS", "15")
        .spawn()
        .expect("spawn devflow advance");

    // Wait for the child to acquire the per-phase lock (proves it is
    // genuinely inside `advance()`, holding it across the gate wait) and to
    // write the Code gate (proves it reached `run_gate_with_timeout` and is
    // now polling for a response) — the two on-disk facts the plan's
    // objective names as the "live poller" evidence.
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
        "lock must still be held once the gate is written — this is the \
         proof that a live poller is genuinely blocking on it"
    );

    // The reap itself: the same `Gates::reap` primitive Task 1/2's tests
    // exercise against a thread-based live poller. This test's job is to
    // prove the OTHER end of that same write — a real, separate process
    // consuming it.
    Gates::reap(
        root,
        phase,
        Stage::Code,
        "abort: reaped by devflow gate sweep (unattended gate exceeded max age)",
        "devflow-reap",
    )
    .expect("reap the aged Code gate");

    let status = wait_for_child_exit(&mut child, root, phase, e2e_child_timeout());

    assert!(
        status.success(),
        "devflow advance must exit cleanly once its own abort() path runs, got {status:?}"
    );
    assert!(
        devflow_core::lock::holder(root, phase).is_none(),
        "LockGuard's Drop must have released the per-phase lock — proof the \
         process unwound cleanly through its own code, not by being \
         terminated from outside"
    );

    let events = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap();
    assert!(
        events
            .lines()
            .any(|line| line.contains("\"workflow_aborted\"")),
        "the target process must have run its own abort() path, recording \
         workflow_aborted — the whole claim this plan rests on\nevents:\n{events}"
    );

    // The negative that separates this plan from a reaper that terminates
    // processes: neither the sweep's write path (`Gates::reap`, exercised
    // above) nor this test itself ever sent the child a signal. Checked
    // mechanically by this file's own acceptance grep for signalling call
    // forms (must be zero), not asserted at runtime — there is no API
    // surface here that could send one.
}
