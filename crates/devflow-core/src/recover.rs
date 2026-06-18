//! State recovery and stale-state detection.
//!
//! `devflow recover` reads the existing state file, determines if the
//! agent process is still running, and either reports status or
//! offers to clean up / restart.

use crate::state::State;
use crate::workflow::{self, WorkflowError, load_state};
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

/// Load state, inspect the agent process, and produce a recovery status.
pub fn inspect(project_root: &Path) -> Result<RecoveryStatus, RecoverError> {
    let state = match load_state(project_root) {
        Ok(s) => s,
        Err(WorkflowError::MissingState(_)) => {
            return Err(RecoverError::NothingToRecover);
        }
        Err(err) => return Err(err.into()),
    };

    let agent_running = state.agent_pid.is_some_and(crate::agent::agent_running);
    let is_stale = is_stale_state(&state);
    let age = format_age(state.started_at.as_str());
    let lock_held = crate::lock::holder(project_root).map(|(pid, _)| pid);

    Ok(RecoveryStatus {
        state,
        agent_running,
        is_stale,
        age,
        lock_held,
    })
}

/// Clean up a stale or abandoned workflow state.
///
/// Removes the state file and lock file (if present).
pub fn clean(project_root: &Path) -> Result<(), RecoverError> {
    workflow::clear_state(project_root)?;
    let lock_path = project_root.join(".devflow").join("lock");
    if lock_path.exists() {
        std::fs::remove_file(&lock_path)?;
    }
    Ok(())
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
    if let Some(pid) = state.agent_pid
        && crate::agent::agent_running(pid)
    {
        return false;
    }

    true
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

/// Format an age in seconds to a human-readable string.
fn format_age(started_at: &str) -> String {
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
    use crate::state::{Agent, State};
    use std::path::PathBuf;

    /// Build a state whose `started_at` is `age_secs` in the past.
    fn state_aged(age_secs: u64, agent_pid: Option<u32>) -> State {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut state = State::new(1, Agent::Claude, PathBuf::from("/tmp/project"));
        state.started_at = now.saturating_sub(age_secs).to_string();
        state.agent_pid = agent_pid;
        state
    }

    /// A PID that is essentially certain not to map to a live process.
    const DEAD_PID: u32 = 0x7FFF_FFFE;

    #[test]
    fn fresh_state_is_not_stale() {
        // One hour old, well under the 24h threshold.
        let state = state_aged(3600, None);
        assert!(!is_stale_state(&state));
    }

    #[test]
    fn old_state_with_no_agent_is_stale() {
        let state = state_aged(STALE_THRESHOLD.as_secs() + 60, None);
        assert!(is_stale_state(&state));
    }

    #[test]
    fn old_state_with_dead_agent_is_stale() {
        let state = state_aged(STALE_THRESHOLD.as_secs() + 60, Some(DEAD_PID));
        assert!(is_stale_state(&state));
    }

    #[test]
    fn old_state_with_live_agent_is_not_stale() {
        // Our own PID is guaranteed to be running.
        let own_pid = std::process::id();
        let state = state_aged(STALE_THRESHOLD.as_secs() + 60, Some(own_pid));
        assert!(!is_stale_state(&state));
    }

    #[test]
    fn unparseable_timestamp_is_never_stale() {
        let mut state = State::new(1, Agent::Claude, PathBuf::from("/tmp/project"));
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
    fn inspect_missing_state_reports_nothing_to_recover() {
        let dir = std::env::temp_dir().join(format!("devflow-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let err = inspect(&dir).expect_err("should have no state");
        assert!(matches!(err, RecoverError::NothingToRecover));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
