//! Workflow state persistence helpers.

use crate::state::State;
use std::path::{Path, PathBuf};
use tracing::debug;

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
    debug!("saving state: phase={} stage={}", state.phase, state.stage);
    let dir = devflow_dir(&state.project_root);
    std::fs::create_dir_all(&dir)?;
    let contents = serde_json::to_string_pretty(state)?;
    write_state_atomic(&dir.join("state.json"), &contents)?;
    Ok(())
}

/// Write state through a sibling temporary file so readers never observe a
/// truncated or partially written `state.json`.
fn write_state_atomic(path: &Path, contents: &str) -> Result<(), WorkflowError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use crate::stage::Stage;
    use crate::state::Agent;

    fn state_in(root: &Path, stage: Stage) -> State {
        let mut state = State::new(1, Agent::Claude, Mode::Auto, root.to_path_buf());
        state.stage = stage;
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
        let state = state_in(dir.path(), Stage::Code);
        save_state(&state).expect("save");

        let loaded = load_state(dir.path()).expect("load");
        assert_eq!(loaded.stage, Stage::Code);
        assert_eq!(loaded.phase, 1);
        assert_eq!(loaded.agent, Agent::Claude);
        assert_eq!(loaded.mode, Mode::Auto);
    }

    #[test]
    fn save_state_writes_atomically_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), Stage::Validate);

        save_state(&state).expect("save");

        let path = state_path(dir.path());
        assert!(path.exists());
        let loaded = load_state(dir.path()).expect("load");
        assert_eq!(loaded.stage, Stage::Validate);
        assert_eq!(loaded.phase, state.phase);
        assert!(!path.with_extension("tmp").exists());
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
        let state = state_in(dir.path(), Stage::Validate);
        save_state(&state).unwrap();
        assert!(state_path(dir.path()).exists());

        clear_state(dir.path()).expect("clear");
        assert!(!state_path(dir.path()).exists());
        // Clearing when nothing is present is a no-op success.
        clear_state(dir.path()).expect("clear again");
    }
}
