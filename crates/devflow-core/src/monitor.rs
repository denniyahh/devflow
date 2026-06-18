//! Background monitor daemon.
//!
//! Spawns a detached child process that watches a spawned agent process.
//! When the agent exits, it automatically runs `devflow check` to advance
//! the state machine.
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
    /// No agent PID to monitor.
    #[error("no agent PID recorded — cannot spawn monitor")]
    NoAgentPid,
}

/// Spawn a background monitor for the given workflow state.
///
/// The monitor is a detached child process that:
/// 1. Polls every 30s to check if the agent process is still alive
/// 2. When the agent exits, runs `devflow check` to advance the workflow
/// 3. Exits
///
/// Returns the PID of the spawned monitor.
pub fn spawn_monitor(state: &State) -> Result<u32, MonitorError> {
    let agent_pid = state.agent_pid.ok_or(MonitorError::NoAgentPid)?;

    let project_root = state
        .project_root
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?;

    let binary = std::env::current_exe()
        .map_err(|_| MonitorError::NoBinaryPath)?
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?
        .to_string();

    // Shell script that watches the agent PID. Uses kill -0 to check
    // process existence (POSIX standard, no signal sent). When the agent
    // exits, runs `devflow check` multiple times to advance through all
    // remaining workflow steps.
    //
    // Traps SIGTERM and SIGINT for clean shutdown.
    let script = format!(
        "cleanup() {{ exit 0; }}; trap cleanup TERM INT; \
         while kill -0 {agent_pid} 2>/dev/null; do sleep 30; done; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}",
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
