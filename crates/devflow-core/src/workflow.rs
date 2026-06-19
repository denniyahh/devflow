//! Workflow state persistence helpers.

use crate::state::{State, Step};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Result of advancing workflow state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceResult {
    /// State after the advance attempt.
    pub state: State,
    /// Whether the persisted state was changed.
    pub changed: bool,
    /// Human-readable summary of the transition.
    pub message: String,
}

/// Errors produced while reading or writing workflow state.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    /// Filesystem operation failed.
    #[error("state I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse or serialization failed.
    #[error("state JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// No active state file exists.
    #[error("no active DevFlow state at {0}")]
    MissingState(PathBuf),
}

/// Return the `.devflow` directory for a project.
pub fn devflow_dir(project_root: &Path) -> PathBuf {
    project_root.join(".devflow")
}

/// Return the persisted state path for a project.
pub fn state_path(project_root: &Path) -> PathBuf {
    devflow_dir(project_root).join("state.json")
}

/// Save workflow state to `.devflow/state.json`.
pub fn save_state(state: &State) -> Result<(), WorkflowError> {
    debug!("saving state: phase={} step={}", state.phase, state.step);
    let dir = devflow_dir(&state.project_root);
    std::fs::create_dir_all(&dir)?;
    let contents = serde_json::to_string_pretty(state)?;
    std::fs::write(dir.join("state.json"), contents)?;
    Ok(())
}

/// Load workflow state from `.devflow/state.json`.
pub fn load_state(project_root: &Path) -> Result<State, WorkflowError> {
    let path = state_path(project_root);
    debug!("loading state from {}", path.display());
    if !path.exists() {
        return Err(WorkflowError::MissingState(path));
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// Remove persisted state if present.
pub fn clear_state(project_root: &Path) -> Result<(), WorkflowError> {
    let path = state_path(project_root);
    if path.exists() {
        debug!("clearing state at {}", path.display());
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Advance state once and skip configured optional steps.
pub fn advance_state(
    mut state: State,
    config: &crate::config::Config,
) -> Result<AdvanceResult, WorkflowError> {
    if state.step == Step::Executing
        && state
            .agent_result
            .as_ref()
            .is_some_and(|result| result.status == crate::agent_result::AgentStatus::Failed)
    {
        save_state(&state)?;
        return Ok(AdvanceResult {
            state,
            changed: false,
            message: String::from("agent failed; staying at executing"),
        });
    }

    let before = state.step;
    let before_phase = state.phase;

    // Emit step_exited event
    tracing::event!(
        tracing::Level::INFO,
        step_exited = before.to_string(),
        phase = before_phase,
        "exiting step"
    );

    let message = if state.advance().is_some() {
        state.advance_skipping(config);
        if state.step == Step::Cleaning && config.should_skip(&Step::Cleaning) {
            state.step = Step::Idle;
        }

        // Emit step_entered event
        tracing::event!(
            tracing::Level::INFO,
            step_entered = state.step.to_string(),
            phase = state.phase,
            "entered step"
        );

        info!(
            "state transition: {} → {} (phase {})",
            before, state.step, state.phase
        );
        format!("advanced from {before} to {}", state.step)
    } else {
        state.step = Step::Idle;
        // Emit step_entered for idle
        tracing::event!(
            tracing::Level::INFO,
            step_entered = "idle",
            phase = state.phase,
            "workflow complete"
        );
        info!("workflow complete for phase {}; returned to idle", state.phase);
        String::from("workflow complete; returned to idle")
    };

    let changed = before != state.step;
    if state.step == Step::Idle {
        clear_state(&state.project_root)?;
    } else {
        save_state(&state)?;
    }

    Ok(AdvanceResult {
        state,
        changed,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_result::{AgentResult, AgentStatus};
    use crate::config::Config;
    use crate::state::Agent;

    fn state_in(root: &Path, step: Step) -> State {
        let mut state = State::new(1, Agent::Claude, root.to_path_buf());
        state.step = step;
        state
    }

    #[test]
    fn paths_are_under_devflow_dir() {
        let root = Path::new("/repo");
        assert_eq!(devflow_dir(root), Path::new("/repo/.devflow"));
        assert_eq!(state_path(root), Path::new("/repo/.devflow/state.json"));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), Step::Executing);
        save_state(&state).expect("save");

        let loaded = load_state(dir.path()).expect("load");
        assert_eq!(loaded.step, Step::Executing);
        assert_eq!(loaded.phase, 1);
        assert_eq!(loaded.agent, Agent::Claude);
    }

    #[test]
    fn load_missing_state_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_state(dir.path()).unwrap_err();
        assert!(matches!(err, WorkflowError::MissingState(_)));
    }

    #[test]
    fn clear_removes_state_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), Step::Verifying);
        save_state(&state).unwrap();
        assert!(state_path(dir.path()).exists());

        clear_state(dir.path()).expect("clear");
        assert!(!state_path(dir.path()).exists());
        // Clearing when nothing is present is a no-op success.
        clear_state(dir.path()).expect("clear again");
    }

    #[test]
    fn advance_state_saves_and_reports_transition() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), Step::Branching);
        let config = Config::default();

        let result = advance_state(state, &config).expect("advance");
        assert!(result.changed);
        assert_eq!(result.state.step, Step::Planning);
        assert!(result.message.contains("branching"));
        assert!(result.message.contains("planning"));
        // Non-idle state is persisted.
        assert_eq!(load_state(dir.path()).unwrap().step, Step::Planning);
    }

    #[test]
    fn advance_state_skips_disabled_steps() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.automation.auto_verify = false;
        config.automation.auto_docs = false;

        let state = state_in(dir.path(), Step::Executing);
        let result = advance_state(state, &config).expect("advance");
        // Verifying + Docsing are skipped, landing on Shipping.
        assert_eq!(result.state.step, Step::Shipping);
    }

    #[test]
    fn advance_state_from_terminal_returns_to_idle_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), Step::Cleaning);
        save_state(&state).unwrap();
        let config = Config::default();

        let result = advance_state(state, &config).expect("advance");
        assert_eq!(result.state.step, Step::Idle);
        assert!(result.message.contains("complete"));
        // Idle state clears persisted file.
        assert!(!state_path(dir.path()).exists());
    }

    #[test]
    fn advance_state_clears_when_cleanup_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.automation.auto_cleanup = false;

        let state = state_in(dir.path(), Step::Shipping);
        let result = advance_state(state, &config).expect("advance");
        // Cleaning is disabled, so the workflow finishes at Idle.
        assert_eq!(result.state.step, Step::Idle);
        assert!(!state_path(dir.path()).exists());
    }

    #[test]
    fn advance_state_with_success_agent_result_advances_normally() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_in(dir.path(), Step::Executing);
        state.agent_result = Some(AgentResult {
            status: AgentStatus::Success,
            exit_code: Some(0),
            reason: None,
            commits: Some(1),
            summary: None,
        });
        let config = Config::default();

        let result = advance_state(state, &config).expect("advance");

        assert!(result.changed);
        assert_eq!(result.state.step, Step::Verifying);
        assert_eq!(load_state(dir.path()).unwrap().step, Step::Verifying);
    }

    #[test]
    fn advance_state_with_failed_agent_result_stays_executing() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_in(dir.path(), Step::Executing);
        state.agent_result = Some(AgentResult {
            status: AgentStatus::Failed,
            exit_code: Some(1),
            reason: Some("tests failed".into()),
            commits: Some(0),
            summary: None,
        });
        let config = Config::default();

        let result = advance_state(state, &config).expect("advance");

        assert!(!result.changed);
        assert_eq!(result.state.step, Step::Executing);
        assert!(result.message.contains("agent failed"));
        assert_eq!(load_state(dir.path()).unwrap().step, Step::Executing);
    }
}
