//! Background monitor daemon.
//!
//! Spawns a detached child process that watches a tmux session.
//! When the session dies (agent exited), it automatically runs
//! `devflow check` to advance the state machine.
//!
//! This is the key automation primitive — no cron, no scheduler,
//! no agent cooperation needed.

use crate::state::State;
use std::process::{Command, Stdio};

/// Errors produced by monitor operations.
#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    /// Spawning the monitor process failed.
    #[error("failed to spawn monitor: {0}")]
    Io(#[from] std::io::Error),
    /// Project path is not valid UTF-8.
    #[error("project path is not valid UTF-8")]
    NonUtf8Path,
    /// Could not determine the current executable path.
    #[error("could not determine devflow binary path")]
    NoBinaryPath,
}

/// Spawn a background monitor for the given workflow state.
///
/// The monitor is a detached child process that:
/// 1. Waits for the tmux session to disappear
/// 2. Runs `devflow check <project>` to advance the workflow
/// 3. Exits
///
/// Returns the PID of the spawned monitor.
pub fn spawn_monitor(state: &State) -> Result<u32, MonitorError> {
    let session_name = state.tmux_session_name();
    let session = state.tmux_session.as_deref().unwrap_or(&session_name);

    let project_root = state
        .project_root
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?;

    let binary = std::env::current_exe()
        .map_err(|_| MonitorError::NoBinaryPath)?
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?
        .to_string();

    // Shell script that watches the tmux session and auto-advances.
    // Uses a polling loop: check every 30s if the session still exists.
    // When it disappears, run `devflow check` to advance the state machine,
    // then `devflow check` again to run through remaining steps (verify, docs, ship, clean).
    //
    // Traps SIGTERM and SIGINT for clean shutdown (no orphaned state).
    let script = format!(
        "cleanup() {{ exit 0; }}; trap cleanup TERM INT; \
         while tmux has-session -t {session} 2>/dev/null; do sleep 30; done; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}",
        session = shell_escape(session),
        binary = shell_escape(&binary),
        project_root = shell_escape(project_root),
    );

    let child = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let pid = child.id();
    Ok(pid)
}

/// Escape a string for safe use in a single-quoted shell context.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
