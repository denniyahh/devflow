//! Workflow state persistence helpers.
//!
//! State is per-phase: each active phase persists to
//! `.devflow/state-{phase:02}.json` (mirroring the per-phase lock naming), so
//! `devflow parallel` sibling phases never clobber one another
//! (13-DEFERRED-CR-03). A legacy single-slot `.devflow/state.json` from an
//! older binary is migrated to its per-phase name on first read.

use crate::state::State;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

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

/// Filename prefix shared by every per-phase state file. Owned here so
/// listing/migration never hardcode the naming scheme.
const STATE_FILE_PREFIX: &str = "state-";

/// Return the `.devflow` directory for a project.
pub fn devflow_dir(project_root: &Path) -> PathBuf {
    project_root.join(".devflow")
}

/// Return the persisted state path for a phase of a project.
pub fn state_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("{STATE_FILE_PREFIX}{phase:02}.json"))
}

/// Path of the legacy single-slot state file written by pre-14a binaries.
fn legacy_state_path(project_root: &Path) -> PathBuf {
    devflow_dir(project_root).join("state.json")
}

/// One-shot migration: move a legacy `.devflow/state.json` to its per-phase
/// name. Called before any read so an upgrade mid-run keeps its state. A
/// per-phase file that already exists wins — the legacy file is then stale
/// and removed without overwriting.
fn migrate_legacy_state(project_root: &Path) {
    let legacy = legacy_state_path(project_root);
    let Ok(contents) = std::fs::read_to_string(&legacy) else {
        return;
    };
    let Ok(state) = serde_json::from_str::<State>(&contents) else {
        warn!(
            "legacy state at {} is unparsable — leaving it in place",
            legacy.display()
        );
        return;
    };
    let target = state_path(project_root, state.phase);
    if target.exists() {
        debug!(
            "per-phase state already exists for phase {} — dropping stale legacy file",
            state.phase
        );
    } else if let Err(err) = std::fs::rename(&legacy, &target) {
        warn!(
            "could not migrate legacy state to {}: {err}",
            target.display()
        );
        return;
    } else {
        debug!("migrated legacy state.json to {}", target.display());
        return;
    }
    let _ = std::fs::remove_file(&legacy);
}

/// Save workflow state to `.devflow/state-{NN}.json`, keyed by `state.phase`.
pub fn save_state(state: &State) -> Result<(), WorkflowError> {
    debug!("saving state: phase={} stage={}", state.phase, state.stage);
    let path = state_path(&state.project_root, state.phase);
    let contents = serde_json::to_string_pretty(state)?;
    write_state_atomic(&path, &contents)?;
    Ok(())
}

/// Write state through a sibling temporary file so readers never observe a
/// truncated or partially written state file.
fn write_state_atomic(path: &Path, contents: &str) -> Result<(), WorkflowError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Load workflow state for a phase from `.devflow/state-{NN}.json`.
pub fn load_state(project_root: &Path, phase: u32) -> Result<State, WorkflowError> {
    migrate_legacy_state(project_root);
    let path = state_path(project_root, phase);
    debug!("loading state from {}", path.display());
    if !path.exists() {
        return Err(WorkflowError::MissingState(path));
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// Enumerate every active phase state, sorted by phase number.
///
/// Unparsable state files are skipped with a warning rather than failing the
/// whole listing — `status`/`recover` must degrade, not die, on one corrupt
/// file.
pub fn list_states(project_root: &Path) -> Vec<State> {
    migrate_legacy_state(project_root);
    let mut states = Vec::new();
    let Ok(entries) = std::fs::read_dir(devflow_dir(project_root)) else {
        return states;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(STATE_FILE_PREFIX) || !name.ends_with(".json") {
            continue;
        }
        match std::fs::read_to_string(entry.path())
            .map_err(WorkflowError::from)
            .and_then(|c| Ok(serde_json::from_str::<State>(&c)?))
        {
            Ok(state) => states.push(state),
            Err(err) => warn!("skipping unreadable state file {name}: {err}"),
        }
    }
    states.sort_by_key(|s| s.phase);
    states
}

/// Delete a legacy `state.json` that cannot be parsed — and therefore can
/// never be migrated by [`migrate_legacy_state`] or matched by
/// [`clear_state`]'s phase check (14-CR-04). Ordinary reads deliberately
/// leave such a file in place; only the operator-driven reset
/// (`recover --clean`) is sanctioned to call this. Returns whether a file
/// was removed.
pub fn remove_corrupt_legacy_state(project_root: &Path) -> Result<bool, WorkflowError> {
    let legacy = legacy_state_path(project_root);
    let Ok(contents) = std::fs::read_to_string(&legacy) else {
        return Ok(false);
    };
    if serde_json::from_str::<State>(&contents).is_ok() {
        // Parsable: the normal migration path owns it.
        return Ok(false);
    }
    std::fs::remove_file(&legacy)?;
    warn!("removed unparsable legacy state at {}", legacy.display());
    Ok(true)
}

/// Remove a phase's persisted state if present.
pub fn clear_state(project_root: &Path, phase: u32) -> Result<(), WorkflowError> {
    let path = state_path(project_root, phase);
    if path.exists() {
        debug!("clearing state at {}", path.display());
        std::fs::remove_file(path)?;
    }
    // A legacy single-slot file for this phase is the same state under its
    // old name — clearing must not leave it behind to be re-migrated.
    let legacy = legacy_state_path(project_root);
    if let Ok(contents) = std::fs::read_to_string(&legacy)
        && let Ok(state) = serde_json::from_str::<State>(&contents)
        && state.phase == phase
    {
        std::fs::remove_file(&legacy)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use crate::stage::Stage;
    use crate::state::AgentKind;

    fn state_in(root: &Path, phase: u32, stage: Stage) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = stage;
        state
    }

    #[test]
    fn paths_are_per_phase_under_devflow_dir() {
        let root = Path::new("/repo");
        assert_eq!(devflow_dir(root), Path::new("/repo/.devflow"));
        assert_eq!(
            state_path(root, 7),
            Path::new("/repo/.devflow/state-07.json")
        );
        assert_eq!(
            state_path(root, 14),
            Path::new("/repo/.devflow/state-14.json")
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), 1, Stage::Code);
        save_state(&state).expect("save");

        let loaded = load_state(dir.path(), 1).expect("load");
        assert_eq!(loaded.stage, Stage::Code);
        assert_eq!(loaded.phase, 1);
        assert_eq!(loaded.agent, AgentKind::Claude);
        assert_eq!(loaded.mode, Mode::Auto);
    }

    /// 13-DEFERRED-CR-03 scenario 1: two phases' states must coexist — the
    /// second `start` no longer clobbers the first phase's state.
    #[test]
    fn two_phases_states_coexist_without_clobbering() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), 13, Stage::Code)).unwrap();
        save_state(&state_in(dir.path(), 14, Stage::Validate)).unwrap();

        let a = load_state(dir.path(), 13).expect("phase 13 state");
        let b = load_state(dir.path(), 14).expect("phase 14 state");
        assert_eq!(a.phase, 13);
        assert_eq!(a.stage, Stage::Code);
        assert_eq!(b.phase, 14);
        assert_eq!(b.stage, Stage::Validate);
    }

    #[test]
    fn save_state_writes_atomically_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), 1, Stage::Validate);

        save_state(&state).expect("save");

        let path = state_path(dir.path(), 1);
        assert!(path.exists());
        let loaded = load_state(dir.path(), 1).expect("load");
        assert_eq!(loaded.stage, Stage::Validate);
        assert_eq!(loaded.phase, state.phase);
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn load_missing_state_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_state(dir.path(), 1).unwrap_err();
        assert!(matches!(err, WorkflowError::MissingState(_)));
    }

    #[test]
    fn clear_removes_state_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), 1, Stage::Validate);
        save_state(&state).unwrap();
        assert!(state_path(dir.path(), 1).exists());

        clear_state(dir.path(), 1).expect("clear");
        assert!(!state_path(dir.path(), 1).exists());
        // Clearing when nothing is present is a no-op success.
        clear_state(dir.path(), 1).expect("clear again");
    }

    #[test]
    fn clear_only_touches_its_own_phase() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), 13, Stage::Code)).unwrap();
        save_state(&state_in(dir.path(), 14, Stage::Ship)).unwrap();

        clear_state(dir.path(), 13).unwrap();

        assert!(!state_path(dir.path(), 13).exists());
        assert!(load_state(dir.path(), 14).is_ok(), "phase 14 must survive");
    }

    #[test]
    fn list_states_enumerates_sorted_by_phase() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), 14, Stage::Ship)).unwrap();
        save_state(&state_in(dir.path(), 3, Stage::Code)).unwrap();

        let states = list_states(dir.path());
        assert_eq!(
            states.iter().map(|s| s.phase).collect::<Vec<_>>(),
            vec![3, 14]
        );
    }

    #[test]
    fn list_states_empty_when_no_devflow_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_states(dir.path()).is_empty());
    }

    #[test]
    fn list_states_skips_corrupt_files() {
        let dir = tempfile::tempdir().unwrap();
        save_state(&state_in(dir.path(), 5, Stage::Code)).unwrap();
        std::fs::write(state_path(dir.path(), 6), "not json").unwrap();

        let states = list_states(dir.path());
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, 5);
    }

    /// Upgrade path: a legacy single-slot `state.json` written by an older
    /// binary must be readable after upgrading — migrated to its per-phase
    /// name on first load/list, with the legacy file removed.
    #[test]
    fn legacy_state_json_migrates_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), 9, Stage::Validate);
        let legacy = legacy_state_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, serde_json::to_string_pretty(&state).unwrap()).unwrap();

        let loaded = load_state(dir.path(), 9).expect("legacy state must migrate");
        assert_eq!(loaded.phase, 9);
        assert_eq!(loaded.stage, Stage::Validate);
        assert!(!legacy.exists(), "legacy file must be gone after migration");
        assert!(state_path(dir.path(), 9).exists());
    }

    #[test]
    fn legacy_state_json_migrates_on_list() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path(), 4, Stage::Code);
        let legacy = legacy_state_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, serde_json::to_string_pretty(&state).unwrap()).unwrap();

        let states = list_states(dir.path());
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].phase, 4);
        assert!(!legacy.exists());
    }

    #[test]
    fn legacy_migration_never_overwrites_existing_per_phase_state() {
        let dir = tempfile::tempdir().unwrap();
        // Newer per-phase state at Ship...
        save_state(&state_in(dir.path(), 9, Stage::Ship)).unwrap();
        // ...and a stale legacy file for the same phase still at Code.
        let legacy = legacy_state_path(dir.path());
        std::fs::write(
            &legacy,
            serde_json::to_string_pretty(&state_in(dir.path(), 9, Stage::Code)).unwrap(),
        )
        .unwrap();

        let loaded = load_state(dir.path(), 9).unwrap();
        assert_eq!(loaded.stage, Stage::Ship, "per-phase state must win");
        assert!(!legacy.exists(), "stale legacy file must be dropped");
    }
}
