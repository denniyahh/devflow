//! State recovery and stale-state detection.
//!
//! `devflow recover` reads the existing state file, determines if the
//! agent process is still running, and either reports status or
//! offers to clean up / restart.

use crate::state::State;
use crate::workflow::{self, WorkflowError};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum age before a state is considered stale (>24h).
pub const STALE_THRESHOLD: Duration = Duration::from_secs(24 * 60 * 60);

/// Errors produced by recover operations.
#[derive(Debug, thiserror::Error)]
pub enum RecoverError {
    /// No state file exists — nothing to recover.
    #[error("no state to recover — project is idle")]
    NothingToRecover,
    /// Filesystem operation failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// State loading failed.
    #[error("{0}")]
    Workflow(#[from] WorkflowError),
}

/// Result of inspecting an existing workflow state.
#[derive(Debug)]
pub struct RecoveryStatus {
    /// The loaded workflow state.
    pub state: State,
    /// Whether the agent process is still running.
    pub agent_running: bool,
    /// Whether the state is considered stale (>24h without agent).
    pub is_stale: bool,
    /// Human-readable age of the state.
    pub age: String,
    /// Whether a lock file is present (shows holder PID).
    pub lock_held: Option<String>,
}

/// Inspect every active phase state, producing one recovery status per phase
/// (sorted by phase number). Errors with [`RecoverError::NothingToRecover`]
/// when no phase has persisted state.
pub fn inspect_all(project_root: &Path) -> Result<Vec<RecoveryStatus>, RecoverError> {
    let states = workflow::list_states(project_root);
    if states.is_empty() {
        return Err(RecoverError::NothingToRecover);
    }
    Ok(states
        .into_iter()
        .map(|state| inspect_state(project_root, state))
        .collect())
}

fn inspect_state(project_root: &Path, state: State) -> RecoveryStatus {
    let agent_running = agent_pid_for(&state).is_some_and(crate::agent::agent_running);
    let is_stale = is_stale_state(&state);
    let age = format_age(state.started_at.as_str());
    let lock_held = crate::lock::holder(project_root, state.phase).map(|(pid, _)| pid);

    RecoveryStatus {
        state,
        agent_running,
        is_stale,
        age,
        lock_held,
    }
}

/// Clean up stale or abandoned workflow state.
///
/// 14-CR-01: only STALE phases are swept — a phase whose agent is still
/// running, or whose state is simply fresh, is kept (with a warning naming
/// the explicit `--phase` escape hatch), so cleaning one dead phase under
/// `devflow parallel` can never orphan a healthy sibling. Also removes an
/// unparsable legacy `state.json` (14-CR-04 — this reset is the one
/// sanctioned place), lock files whose holder is dead (the sweep lives in
/// [`crate::lock::remove_stale_locks`], which refuses to delete a live
/// holder's lock), and cron-instruction records for phases that no longer
/// have state — self-describing "auto-re-run this phase" records that must
/// not survive an operator-driven reset. Returns warnings for anything kept
/// or that could not be removed.
pub fn clean(project_root: &Path) -> Result<Vec<String>, RecoverError> {
    let mut warnings = Vec::new();
    for state in workflow::list_states(project_root) {
        let phase = state.phase;
        if agent_pid_for(&state).is_some_and(crate::agent::agent_running) {
            warnings.push(format!(
                "kept phase {phase} — its agent is still running (clear explicitly with --phase {phase})"
            ));
            continue;
        }
        if !is_stale_state(&state) {
            warnings.push(format!(
                "kept phase {phase} — state is not stale yet (clear explicitly with --phase {phase})"
            ));
            continue;
        }
        workflow::clear_state(project_root, phase)?;
    }
    match workflow::remove_corrupt_legacy_state(project_root) {
        Ok(true) => warnings.push("removed unparsable legacy state.json".into()),
        Ok(false) => {}
        Err(err) => warnings.push(format!("could not remove corrupt legacy state.json: {err}")),
    }
    warnings.append(&mut crate::lock::remove_stale_locks(project_root));
    // Drop cron records only for phases without surviving state, so a kept
    // phase's pending re-run record is preserved.
    for instructions in crate::ship::list_cron_instructions(project_root) {
        if workflow::state_path(project_root, instructions.phase).exists() {
            continue;
        }
        if let Err(err) = crate::ship::delete_cron_instructions(project_root, instructions.phase) {
            warnings.push(format!(
                "could not remove cron-instructions for phase {}: {err}",
                instructions.phase
            ));
        }
    }
    Ok(warnings)
}

/// Explicitly clean ONE phase, regardless of staleness — the operator's
/// escape hatch for a wedged-but-fresh run. Clears its state and cron
/// record; warns (but proceeds) when the recorded agent still looks alive.
pub fn clean_phase(project_root: &Path, phase: u32) -> Result<Vec<String>, RecoverError> {
    let mut warnings = Vec::new();
    if let Ok(state) = workflow::load_state(project_root, phase)
        && agent_pid_for(&state).is_some_and(crate::agent::agent_running)
    {
        warnings.push(format!(
            "phase {phase}'s agent appears to still be running — cleared anyway (explicit --phase)"
        ));
    }
    workflow::clear_state(project_root, phase)?;
    if let Err(err) = crate::ship::delete_cron_instructions(project_root, phase) {
        warnings.push(format!("could not remove cron-instructions: {err}"));
    }
    warnings.append(&mut crate::lock::remove_stale_locks(project_root));
    Ok(warnings)
}

/// Check whether a state is stale: >24h old with no running agent.
pub fn is_stale_state(state: &State) -> bool {
    let age_secs = match state_age_secs(&state.started_at) {
        Some(a) => a,
        None => return false,
    };

    if age_secs < STALE_THRESHOLD.as_secs() {
        return false;
    }

    // Only stale if the agent process is gone
    if let Some(pid) = agent_pid_for(state)
        && crate::agent::agent_running(pid)
    {
        return false;
    }

    true
}

/// Read the launched agent PID the monitor recorded for this state's phase, if
/// the pid file is present and parseable.
fn agent_pid_for(state: &State) -> Option<u32> {
    let path = crate::agent_result::agent_pid_path(&state.project_root, state.phase);
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Compute the age of a state's `started_at` timestamp in seconds.
fn state_age_secs(started_at: &str) -> Option<u64> {
    let started: u64 = started_at.parse().ok()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.checked_sub(started)
}

/// Format a unix-seconds timestamp's age as a human-readable string
/// ("5m ago"). Public since 14c: `devflow status` reuses it for elapsed
/// time and event recency.
pub fn format_age(started_at: &str) -> String {
    match state_age_secs(started_at) {
        Some(s) if s < 60 => format!("{s}s ago"),
        Some(s) if s < 3600 => format!("{}m ago", s / 60),
        Some(s) if s < 86400 => format!("{}h ago", s / 3600),
        Some(s) => format!("{}d ago", s / 86400),
        None => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use crate::state::{AgentKind, State};

    /// Build a state in `root` whose `started_at` is `age_secs` in the past,
    /// optionally writing the monitor's agent-pid file with `agent_pid`.
    fn state_aged(root: &Path, age_secs: u64, agent_pid: Option<u32>) -> State {
        state_aged_phase(root, 1, age_secs, agent_pid)
    }

    fn state_aged_phase(root: &Path, phase: u32, age_secs: u64, agent_pid: Option<u32>) -> State {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.started_at = now.saturating_sub(age_secs).to_string();
        if let Some(pid) = agent_pid {
            let path = crate::agent_result::agent_pid_path(root, state.phase);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, pid.to_string()).unwrap();
        }
        state
    }

    /// A PID that is essentially certain not to map to a live process.
    const DEAD_PID: u32 = 0x7FFF_FFFE;

    #[test]
    fn fresh_state_is_not_stale() {
        // One hour old, well under the 24h threshold.
        let dir = tempfile::tempdir().unwrap();
        let state = state_aged(dir.path(), 3600, None);
        assert!(!is_stale_state(&state));
    }

    #[test]
    fn old_state_with_no_agent_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_aged(dir.path(), STALE_THRESHOLD.as_secs() + 60, None);
        assert!(is_stale_state(&state));
    }

    #[test]
    fn old_state_with_dead_agent_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_aged(dir.path(), STALE_THRESHOLD.as_secs() + 60, Some(DEAD_PID));
        assert!(is_stale_state(&state));
    }

    #[test]
    fn old_state_with_live_agent_is_not_stale() {
        // Our own PID is guaranteed to be running.
        let dir = tempfile::tempdir().unwrap();
        let own_pid = std::process::id();
        let state = state_aged(dir.path(), STALE_THRESHOLD.as_secs() + 60, Some(own_pid));
        assert!(!is_stale_state(&state));
    }

    #[test]
    fn unparseable_timestamp_is_never_stale() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(1, AgentKind::Claude, Mode::Auto, dir.path().to_path_buf());
        state.started_at = "not-a-number".into();
        assert!(!is_stale_state(&state));
        assert_eq!(state_age_secs(&state.started_at), None);
    }

    #[test]
    fn state_age_secs_parses_epoch() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let started = (now - 120).to_string();
        let age = state_age_secs(&started).expect("age");
        // Allow a small window for clock drift during the test.
        assert!((118..=125).contains(&age), "unexpected age: {age}");
    }

    #[test]
    fn format_age_buckets_by_magnitude() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ago = |secs: u64| format_age(&(now - secs).to_string());
        assert!(ago(30).ends_with("s ago"));
        assert!(ago(120).ends_with("m ago"));
        assert!(ago(7200).ends_with("h ago"));
        assert!(ago(2 * 86400).ends_with("d ago"));
        assert_eq!(format_age("garbage"), "unknown");
    }

    #[test]
    fn inspect_all_missing_state_reports_nothing_to_recover() {
        let dir = std::env::temp_dir().join(format!("devflow-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let err = inspect_all(&dir).expect_err("should have no state");
        assert!(matches!(err, RecoverError::NothingToRecover));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 13-DEFERRED-CR-03 acceptance: recover must enumerate ALL active
    /// phases, not just the last one started.
    #[test]
    fn inspect_all_enumerates_every_active_phase() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged(dir.path(), 60, None)).unwrap();
        let mut other = state_aged(dir.path(), 60, None);
        other.phase = 2;
        workflow::save_state(&other).unwrap();

        let statuses = inspect_all(dir.path()).expect("two phases active");
        assert_eq!(
            statuses.iter().map(|s| s.state.phase).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    /// 14-CR-01: `recover --clean` must never delete a phase whose agent is
    /// still running — under `devflow parallel`, cleaning a stale phase must
    /// not orphan a healthy sibling.
    #[test]
    fn clean_keeps_phase_with_live_agent() {
        let dir = tempfile::tempdir().unwrap();
        // Stale-aged but the recorded agent (our own pid) is alive.
        let live = state_aged_phase(
            dir.path(),
            1,
            STALE_THRESHOLD.as_secs() + 60,
            Some(std::process::id()),
        );
        workflow::save_state(&live).unwrap();
        // Genuinely stale sibling: old and dead.
        let stale = state_aged_phase(
            dir.path(),
            2,
            STALE_THRESHOLD.as_secs() + 60,
            Some(DEAD_PID),
        );
        workflow::save_state(&stale).unwrap();

        let warnings = clean(dir.path()).expect("clean");

        let remaining: Vec<u32> = workflow::list_states(dir.path())
            .iter()
            .map(|s| s.phase)
            .collect();
        assert_eq!(remaining, vec![1], "live phase must survive, stale cleared");
        assert!(
            warnings.iter().any(|w| w.contains("phase 1")),
            "keeping a live phase must be reported: {warnings:?}"
        );
    }

    /// 14-CR-01: a fresh (not yet stale) phase is also kept — only stale
    /// phases are swept implicitly; anything else needs explicit `--phase`.
    #[test]
    fn clean_keeps_fresh_phase() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged_phase(dir.path(), 3, 60, None)).unwrap();

        let warnings = clean(dir.path()).expect("clean");

        assert_eq!(workflow::list_states(dir.path()).len(), 1);
        assert!(warnings.iter().any(|w| w.contains("--phase 3")));
    }

    #[test]
    fn clean_clears_stale_phase_state() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged_phase(
            dir.path(),
            2,
            STALE_THRESHOLD.as_secs() + 60,
            Some(DEAD_PID),
        ))
        .unwrap();

        clean(dir.path()).expect("clean");

        assert!(workflow::list_states(dir.path()).is_empty());
    }

    /// 14-CR-04: a corrupt legacy `state.json` (old binary killed mid-write)
    /// can never be migrated or matched by a per-phase clear — the operator
    /// reset is the one sanctioned place to remove it.
    #[test]
    fn clean_removes_corrupt_legacy_state_json() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(".devflow/state.json");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, "{\"stage\":").unwrap();

        clean(dir.path()).expect("clean");

        assert!(
            !legacy.exists(),
            "recover --clean must remove an unparsable legacy state.json"
        );
    }

    /// 14-CR-01: explicit `--phase` cleanup clears exactly that phase, even
    /// when it is fresh, and leaves siblings alone.
    #[test]
    fn clean_phase_clears_only_the_named_phase() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged_phase(dir.path(), 4, 60, None)).unwrap();
        workflow::save_state(&state_aged_phase(dir.path(), 5, 60, None)).unwrap();

        clean_phase(dir.path(), 4).expect("clean_phase");

        let remaining: Vec<u32> = workflow::list_states(dir.path())
            .iter()
            .map(|s| s.phase)
            .collect();
        assert_eq!(remaining, vec![5]);
    }
}
