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
use devflow_core::stage::Stage;
use std::path::Path;
use std::process::Command;

fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Overwrite an already-written gate file's `timestamp` so it reads as
/// `age_secs` old — the deterministic way to make a gate look abandoned
/// without sleeping in the test. Round-trips through the same [`GateFile`]
/// shape `Gates::write_gate` itself writes.
fn backdate_gate(root: &Path, phase: u32, stage: Stage, age_secs: u64) {
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

/// The plan's core claim: a gate older than the default six-hour threshold
/// is answered with a rejection, and a real `Gates::poll_response` thread —
/// the exact live-poller seam `run_gate_with_timeout` blocks on in
/// production — picks it up and resolves to `GateAction::Abort`.
#[test]
fn sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = 91;
    let stage = Stage::Ship;

    Gates::write_gate(root, phase, stage, "approve merge?").unwrap();
    backdate_gate(root, phase, stage, 7 * 60 * 60); // 7h, past the 6h default.

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
    let phase = 92;
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
