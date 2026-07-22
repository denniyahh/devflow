//! Workflow state persistence helpers.
//!
//! State is per-phase: each active phase persists to
//! `.devflow/state-{phase:02}.json` (mirroring the per-phase lock naming), so
//! `devflow parallel` sibling phases never clobber one another
//! (13-DEFERRED-CR-03). A legacy single-slot `.devflow/state.json` from an
//! older binary is migrated to its per-phase name on first read.

use crate::state::State;
use std::io::Write;
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
const CORRUPT_LEGACY_STATE_HINT: &str = "devflow recover --clean";

/// Return the `.devflow` directory for a project.
pub fn devflow_dir(project_root: &Path) -> PathBuf {
    project_root.join(".devflow")
}

/// Locate the shallowest `.devflow` path component within `dir`, if any, and
/// return the path up to and including it.
///
/// Walking `dir`'s path *components* — rather than string-matching each of
/// `dir.ancestors()`'s paths — is what makes this resolve a **relative**
/// `dir` whose `.devflow` is the leaf or near-leaf component (e.g.
/// `.devflow/captures`, or the bare `.devflow`) correctly: the ancestor-tail
/// approach hits an empty final ancestor `""` on those inputs and mishandles
/// the leaf-is-`.devflow` case, whereas component-walking handles both
/// cleanly. Shallowest-first (the first match while walking root-to-leaf) is
/// the deterministic tie-break if `.devflow` appears twice in the path.
fn find_devflow_marker(dir: &Path) -> Option<PathBuf> {
    let mut acc = PathBuf::new();
    for component in dir.components() {
        acc.push(component.as_os_str());
        if component.as_os_str() == std::ffi::OsStr::new(".devflow") {
            return Some(acc);
        }
    }
    None
}

/// Create `dir` (and any missing parents), then self-protect any `.devflow`
/// directory found in its path by writing `<that-dir>/.gitignore` containing
/// `*` — so a downstream user's routine `git add . && git commit` never
/// sweeps DevFlow's runtime artifacts (agent stdout, gate context, state)
/// into their repository, independent of whether their own root
/// `.gitignore` mentions `.devflow` at all (closes 19a-WR-01,
/// 19-CONTEXT.md D-14).
///
/// This is deliberately a different function from the pure path accessor
/// [`devflow_dir`] above: `devflow_dir(project_root)` takes a **project
/// root** and returns `project_root/.devflow` with zero filesystem I/O — it
/// is invoked from read-only paths (`doctor`, `status`) and from tests that
/// assert on the returned path, so giving *it* side effects would be exactly
/// the class of behavioral change this phase exists to avoid. This function,
/// `ensure_devflow_dir(dir)`, instead takes the **directory to create**,
/// which may itself be `.devflow`, a subdirectory of it, or something with no
/// `.devflow` ancestor at all. Do not confuse the two.
///
/// Contract:
/// 1. `create_dir_all(dir)` — create `dir` and all missing parents.
/// 2. Find the shallowest `.devflow` path component (see
///    [`find_devflow_marker`]).
/// 3. If found, write `<marker>/.gitignore` with the bytes `*\n`, using
///    `create_new(true)` so an existing file — whatever its content — is left
///    untouched; a lost race against a concurrent creator surfaces as
///    `AlreadyExists`, which this function maps to `Ok(())`. Any other I/O
///    error propagates via `?`.
/// 4. If no `.devflow` component exists, this function is exactly equivalent
///    to `create_dir_all`.
///
/// Returns `std::io::Result<()>`, not a crate-specific error enum: this
/// plan's seven conversion sites each live in a different module with their
/// own error enum (`WorkflowError`, `GateError`, `MonitorError`,
/// `ResultError`, `ShipError`, `LockError`), and every one already carries an
/// `Io(#[from] std::io::Error)` variant, so `?` converts at every call site
/// with zero signature churn.
///
/// **Deleted-marker note:** if `.devflow/.gitignore` is deleted after
/// creation, subsequent calls will not recreate it — the protection is
/// established once per directory lifetime. Recreating a deleted marker
/// would violate the rule that this function must never overwrite an
/// existing `.gitignore` a user or another tool may own, and it cannot
/// distinguish "user deleted it" from "never created."
pub fn ensure_devflow_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    let Some(marker_dir) = find_devflow_marker(dir) else {
        return Ok(());
    };

    let gitignore = marker_dir.join(".gitignore");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&gitignore)
    {
        Ok(mut f) => {
            f.write_all(b"*\n")?;
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

/// Return the persisted state path for a phase of a project.
pub fn state_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("{STATE_FILE_PREFIX}{phase:02}.json"))
}

/// Path of the legacy single-slot state file written by pre-14a binaries.
pub(crate) fn legacy_state_path(project_root: &Path) -> PathBuf {
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
            "legacy state at {} is unparsable — leaving it in place; remove it with `{CORRUPT_LEGACY_STATE_HINT}`",
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

    #[test]
    fn migrate_legacy_state_warning_names_recovery_command() {
        assert!(CORRUPT_LEGACY_STATE_HINT.contains("recover --clean"));
    }

    #[test]
    fn ensure_devflow_dir_writes_star_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");
        ensure_devflow_dir(&target).expect("ensure_devflow_dir");
        assert!(target.is_dir());
        let contents = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(contents.trim(), "*");
    }

    #[test]
    fn ensure_devflow_dir_is_idempotent_and_preserves_existing_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");
        ensure_devflow_dir(&target).expect("first call");
        let first = std::fs::read(target.join(".gitignore")).unwrap();

        ensure_devflow_dir(&target).expect("second call");
        let second = std::fs::read(target.join(".gitignore")).unwrap();
        assert_eq!(first, second, "second call must leave the file byte-identical");
    }

    #[test]
    fn ensure_devflow_dir_preserves_foreign_gitignore_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join(".gitignore"), "# owned by something else\n").unwrap();

        ensure_devflow_dir(&target).expect("must not fail on a foreign .gitignore");

        let contents = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(contents, "# owned by something else\n");
    }

    #[test]
    fn ensure_devflow_dir_on_nested_subpath_marks_the_devflow_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow").join("history").join("phase-01");
        ensure_devflow_dir(&target).expect("nested ensure_devflow_dir");

        assert!(target.is_dir());
        let marker = dir.path().join(".devflow").join(".gitignore");
        assert!(marker.is_file(), "gitignore must land at the .devflow ancestor");
        assert!(
            !target.join(".gitignore").exists(),
            "gitignore must not land at the leaf directory"
        );
    }

    /// Antigravity review edge case: a relative path whose `.devflow` is the
    /// leaf or near-leaf component. Exercises the component-detection logic
    /// directly rather than a real filesystem call, so the assertion does not
    /// depend on the test process's (global, shared-across-threads) cwd.
    #[test]
    fn ensure_devflow_dir_on_relative_devflow_leaf_path_marks_it() {
        assert_eq!(
            find_devflow_marker(Path::new(".devflow/captures")),
            Some(PathBuf::from(".devflow")),
            "leaf-adjacent relative path must mark .devflow, not captures/"
        );
        assert_eq!(
            find_devflow_marker(Path::new(".devflow")),
            Some(PathBuf::from(".devflow")),
            "bare relative .devflow path must mark itself"
        );
    }

    #[test]
    fn ensure_devflow_dir_without_a_devflow_ancestor_only_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("plain").join("sub");
        ensure_devflow_dir(&target).expect("ensure_devflow_dir");

        assert!(target.is_dir());
        assert!(!dir.path().join(".gitignore").exists());
        assert!(!dir.path().join("plain").join(".gitignore").exists());
        assert!(!target.join(".gitignore").exists());
    }

    #[test]
    fn ensure_devflow_dir_concurrent_calls_both_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".devflow");

        let t1 = {
            let target = target.clone();
            std::thread::spawn(move || ensure_devflow_dir(&target))
        };
        let t2 = {
            let target = target.clone();
            std::thread::spawn(move || ensure_devflow_dir(&target))
        };
        assert!(t1.join().unwrap().is_ok(), "first concurrent call must succeed");
        assert!(t2.join().unwrap().is_ok(), "second concurrent call must succeed");

        let contents = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(contents.trim(), "*");
    }

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
