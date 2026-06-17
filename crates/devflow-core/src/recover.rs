//! State recovery and stale-state detection.
//!
//! `devflow recover` reads the existing state file, determines if the
//! tmux agent session is still running, and either reports status or
//! offers to clean up / restart.

use crate::state::State;
use crate::workflow::{self, load_state, WorkflowError};
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
    /// Whether the tmux agent session is still running.
    pub agent_running: bool,
    /// Whether the state is considered stale (>24h without tmux session).
    pub is_stale: bool,
    /// Human-readable age of the state.
    pub age: String,
    /// Whether a lock file is present.
    pub lock_held: Option<String>,
}

/// Load state, inspect the tmux session, and produce a recovery status.
pub fn inspect(project_root: &Path) -> Result<RecoveryStatus, RecoverError> {
    let state = match load_state(project_root) {
        Ok(s) => s,
        Err(WorkflowError::MissingState(_)) => {
            return Err(RecoverError::NothingToRecover);
        }
        Err(err) => return Err(err.into()),
    };

    let agent_running = if let Some(session) = &state.tmux_session {
        crate::tmux::agent_running(session).unwrap_or(false)
    } else {
        false
    };

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
    // Also remove lock if it exists
    let lock_path = project_root.join(".devflow").join("lock");
    if lock_path.exists() {
        std::fs::remove_file(&lock_path)?;
    }
    Ok(())
}

/// Check whether a state is stale: >24h old with no running tmux session.
pub fn is_stale_state(state: &State) -> bool {
    let age_secs = match state_age_secs(&state.started_at) {
        Some(a) => a,
        None => return false,
    };

    if age_secs < STALE_THRESHOLD.as_secs() {
        return false;
    }

    // Only stale if the agent session is gone
    #[allow(clippy::collapsible_if)]
    if let Some(session) = &state.tmux_session {
        if crate::tmux::agent_running(session).unwrap_or(false) {
            return false;
        }
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
