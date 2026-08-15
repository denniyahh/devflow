//! A decimal-numbered phase owns its own files, distinct from the integer
//! phase it shares a major number with.
//!
//! The 999.97 widening replaced a `u32` phase identifier with [`PhaseId`].
//! Every path in DevFlow is derived from that identifier, and the failure mode
//! that motivated the fix is not a crash — it is a *silent collision*: a
//! widened parser with an unwidened path derivation resolves `35.1` to phase
//! 35's state file, gate, lock, worktree, and `.planning/phases/35-*`
//! artifacts. Phase 35 is complete, so that reads a finished phase's verdict
//! into a run that has not started.
//!
//! These assertions are therefore about *distinctness first* and exact
//! spelling second. A regression that made `padded()` drop the minor number
//! would keep every path well-formed and break every one of these.

use devflow_core::phase_id::PhaseId;
use devflow_core::stage::Stage;
use devflow_core::{agent_result, gates::Gates, workflow, worktree};
use std::path::{Path, PathBuf};

const ROOT: &str = "/tmp/devflow-decimal-phase-paths";

/// Every identifier-derived path for one phase, in one place.
fn paths_for(phase: PhaseId) -> Vec<PathBuf> {
    let root = Path::new(ROOT);
    vec![
        workflow::state_path(root, phase),
        Gates::gate_path(root, phase, Stage::Validate),
        Gates::response_path(root, phase, Stage::Validate),
        worktree::phase_path(root, phase),
        worktree::phase_agent_path(root, phase, "claude"),
        agent_result::stdout_path(root, phase),
        agent_result::history_dir(root, phase),
    ]
}

#[test]
fn a_decimal_phase_never_shares_a_path_with_its_integer_sibling() {
    let integer = paths_for(PhaseId::new(35));
    let decimal = paths_for(PhaseId::with_minor(35, 1));
    let other_decimal = paths_for(PhaseId::with_minor(35, 2));

    assert_eq!(
        integer.len(),
        decimal.len(),
        "fixture error: the two path sets must be comparable position by position"
    );

    for ((a, b), c) in integer.iter().zip(&decimal).zip(&other_decimal) {
        assert_ne!(
            a,
            b,
            "phase 35 and 35.1 derive the same path: {}",
            a.display()
        );
        assert_ne!(
            b,
            c,
            "phase 35.1 and 35.2 derive the same path: {}",
            b.display()
        );
        assert_ne!(
            a,
            c,
            "phase 35 and 35.2 derive the same path: {}",
            a.display()
        );
    }
}

/// The positive half: distinctness alone would be satisfied by garbage, so
/// pin what the decimal phase's names actually are.
#[test]
fn a_decimal_phase_spells_its_paths_with_the_minor_number() {
    let phase = PhaseId::with_minor(35, 1);
    let root = Path::new(ROOT);

    assert!(
        workflow::state_path(root, phase)
            .to_string_lossy()
            .ends_with("state-35.1.json"),
        "state path was {}",
        workflow::state_path(root, phase).display()
    );
    assert!(
        worktree::phase_path(root, phase)
            .to_string_lossy()
            .ends_with("phase-35.1"),
        "worktree path was {}",
        worktree::phase_path(root, phase).display()
    );
    assert!(
        Gates::gate_path(root, phase, Stage::Validate)
            .to_string_lossy()
            .ends_with("35.1-validate.json"),
        "gate path was {}",
        Gates::gate_path(root, phase, Stage::Validate).display()
    );
}

/// The compatibility half: an integer phase's paths must be byte-identical to
/// what the `u32` era produced, or every run already on disk is orphaned.
#[test]
fn an_integer_phase_keeps_its_pre_widening_paths() {
    let root = Path::new(ROOT);

    assert!(
        workflow::state_path(root, PhaseId::new(7))
            .to_string_lossy()
            .ends_with("state-07.json"),
        "single-digit phases must stay zero-padded, got {}",
        workflow::state_path(root, PhaseId::new(7)).display()
    );
    assert!(
        worktree::phase_path(root, PhaseId::new(35))
            .to_string_lossy()
            .ends_with("phase-35"),
        "got {}",
        worktree::phase_path(root, PhaseId::new(35)).display()
    );
    assert!(
        agent_result::history_dir(root, PhaseId::new(17))
            .to_string_lossy()
            .ends_with("phase-17"),
        "got {}",
        agent_result::history_dir(root, PhaseId::new(17)).display()
    );
}
