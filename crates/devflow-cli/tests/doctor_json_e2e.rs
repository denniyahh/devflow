//! `devflow doctor --json`'s `reconciliation[].gate_waiter`, against the real
//! binary: the machine-readable fact a script reads to learn which gate a phase
//! has open and whether a registered waiter is live.
//!
//! This is an integration test because a unit-test harness's
//! `env!("CARGO_BIN_EXE_devflow")` resolves to the harness itself, as
//! `doctor_antigravity.rs` explains. The test process holds the gate's
//! registration, so the child's probe reads another process's lock, the way an
//! operator's `doctor` reads a supervisor's.

mod common;

use devflow_core::gates::Gates;
use devflow_core::mode::Mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use std::path::Path;

/// The `gate_waiter` of every `reconciliation` entry for phase 1, from one real
/// `devflow doctor --json <root>`.
fn phase_one_gate_waiters(home: &common::HermeticHome, root: &Path) -> Vec<serde_json::Value> {
    let out = common::hermetic_devflow(home)
        .args(["doctor", "--json"])
        .arg(root)
        .output()
        .expect("spawn devflow doctor --json");
    assert!(
        out.status.success(),
        "doctor --json must exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|error| {
        panic!(
            "doctor --json must print one JSON document ({error}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    let entries = document["reconciliation"]
        .as_array()
        .unwrap_or_else(|| panic!("no reconciliation array: {document}"));
    let waiters: Vec<serde_json::Value> = entries
        .iter()
        .filter(|entry| entry["phase"] == 1)
        .map(|entry| {
            entry
                .get("gate_waiter")
                .unwrap_or_else(|| panic!("a phase-1 entry has no gate_waiter: {entry}"))
                .clone()
        })
        .collect();
    assert!(
        !waiters.is_empty(),
        "the state saved for phase 1 must produce an entry: {document}"
    );
    waiters
}

/// Every phase-1 entry names the open Define gate: `live` while this process holds
/// its registration, `absent` once it finishes. `finish` removes the registration
/// file, so the second run reads it missing, with no wait on a lock's release.
/// `devflow status` is refused `--json`, so a consumer of the wrong command fails
/// loudly.
#[test]
fn doctor_json_names_the_open_gates_waiter() {
    let home = common::HermeticHome::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(1);
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Define;
    state.gate_pending = true;
    devflow_core::workflow::save_state(&state).unwrap();
    let (opened, registration) =
        Gates::open_registered(root, phase, Stage::Define, "approve the definition").unwrap();
    let gate_id = opened.gate_id.as_str();

    let expected =
        |status: &str| serde_json::json!({"stage": "define", "gate_id": gate_id, "status": status});
    for waiter in phase_one_gate_waiters(&home, root) {
        assert_eq!(waiter, expected("live"), "while the registration is held");
    }

    registration.finish();
    for waiter in phase_one_gate_waiters(&home, root) {
        assert_eq!(waiter, expected("absent"), "after the registration ended");
    }

    let refused = common::hermetic_devflow(&home)
        .args(["status", "--json"])
        .arg(root)
        .output()
        .expect("spawn devflow status --json");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success(),
        "status must refuse --json: {}",
        String::from_utf8_lossy(&refused.stdout)
    );
    assert!(
        stderr.contains("unexpected argument '--json'"),
        "the refusal must be clap's: {stderr}"
    );
}
