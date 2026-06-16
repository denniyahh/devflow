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
