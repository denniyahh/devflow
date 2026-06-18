//! Workflow state persistence helpers.

use crate::state::{State, Step};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    let dir = devflow_dir(&state.project_root);
    std::fs::create_dir_all(&dir)?;
    let contents = serde_json::to_string_pretty(state)?;
    std::fs::write(dir.join("state.json"), contents)?;
    Ok(())
}

/// Load workflow state from `.devflow/state.json`.
pub fn load_state(project_root: &Path) -> Result<State, WorkflowError> {
    let path = state_path(project_root);
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
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Advance state once and skip configured optional steps.
pub fn advance_state(
    mut state: State,
    config: &crate::config::Config,
) -> Result<AdvanceResult, WorkflowError> {
    let before = state.step;
    let message = if state.advance().is_some() {
        state.advance_skipping(config);
        if state.step == Step::Cleaning && config.should_skip(&Step::Cleaning) {
            state.step = Step::Idle;
        }
        format!("advanced from {before} to {}", state.step)
    } else {
        state.step = Step::Idle;
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
        assert_eq!(result.state.step, Step::Executing);
        assert!(result.message.contains("branching"));
        assert!(result.message.contains("executing"));
        // Non-idle state is persisted.
        assert_eq!(load_state(dir.path()).unwrap().step, Step::Executing);
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
}
