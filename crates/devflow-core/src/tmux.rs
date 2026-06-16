//! Tmux integration for launching and inspecting agent sessions.

use crate::state::State;
use std::process::Command;

/// Errors produced by tmux operations.
#[derive(Debug, thiserror::Error)]
pub enum TmuxError {
    /// Spawning tmux failed.
    #[error("failed to execute tmux: {0}")]
    Io(#[from] std::io::Error),
    /// Tmux returned a non-success status.
    #[error("tmux command failed: {0}")]
    Command(String),
    /// Project path could not be represented as UTF-8.
    #[error("project path is not valid UTF-8")]
    NonUtf8Path,
}

/// Launch an agent in a detached tmux session and return the session name.
pub fn launch_agent(state: &State) -> Result<String, TmuxError> {
    let session = state.tmux_session_name();
    if agent_running(&session)? {
        return Ok(session);
    }

    let root = state.project_root.to_str().ok_or(TmuxError::NonUtf8Path)?;
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &session, "sh"])
        .status()?;
    if !status.success() {
        return Err(TmuxError::Command(format!(
            "new-session exited with {status}"
        )));
    }

    let command = state.agent.launch_command(root);
    let status = Command::new("tmux")
        .args(["send-keys", "-t", &session, &command, "C-m"])
        .status()?;
    if !status.success() {
        return Err(TmuxError::Command(format!(
            "send-keys exited with {status}"
        )));
    }

    Ok(session)
}

/// Return whether a tmux session exists.
pub fn agent_running(session_name: &str) -> Result<bool, TmuxError> {
    let status = Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .status()?;
    Ok(status.success())
}

/// Capture the current contents of a tmux pane.
pub fn capture_output(session_name: &str) -> Result<String, TmuxError> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-t", session_name])
        .output()?;
    if !output.status.success() {
        return Err(TmuxError::Command(format!(
            "capture-pane exited with {}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
