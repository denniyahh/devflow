//! Monitor integration tests that need no running supervisor.
//!
//! Boundary note (50-11): a library integration-test binary cannot re-exec
//! the `devflow` binary's hidden `__monitor` subcommand, so no test here can
//! start a supervisor. The end-to-end capture-result check that lived here
//! (`monitor_owns_fake_agent_and_records_devflow_result`) moved to
//! `crates/devflow-cli/tests/supervisor_e2e.rs` as
//! `capture_supervisor_owns_a_fake_agent_and_records_devflow_result`, which
//! drives the real binary. What remains is the advance-side state loading.

use devflow_core::phase_id::PhaseId;
use devflow_core::workflow::{WorkflowError, load_state};

#[test]
fn advance_state_loading_fails_cleanly_for_missing_and_corrupt_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    assert!(matches!(
        load_state(root, PhaseId::new(7)),
        Err(WorkflowError::MissingState(_))
    ));

    std::fs::create_dir_all(root.join(".devflow")).unwrap();
    std::fs::write(root.join(".devflow/state-07.json"), "{\"stage\":").unwrap();

    assert!(matches!(
        load_state(root, PhaseId::new(7)),
        Err(WorkflowError::Json(_))
    ));

    // A corrupt LEGACY state.json must not wedge loading either — it is left
    // in place (unmigratable) and per-phase reads proceed independently.
    std::fs::write(root.join(".devflow/state.json"), "{\"stage\":").unwrap();
    assert!(matches!(
        load_state(root, PhaseId::new(8)),
        Err(WorkflowError::MissingState(_))
    ));
}
