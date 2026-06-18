//! Agent process launching — spawns coding agents as child processes.
//!
//! All agents run in non-interactive mode (`claude -p`, `codex exec`).
//! They produce structured output and exit when done — never block on input.

use crate::state::{Agent, State};
use std::process::{Child, Command, Stdio};

/// Errors produced by agent operations.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Spawning the agent process failed.
    #[error("failed to spawn agent: {0}")]
    Io(#[from] std::io::Error),
    /// Project path is not valid UTF-8.
    #[error("project path is not valid UTF-8")]
    NonUtf8Path,
    /// Agent binary not found in PATH.
    #[error("agent binary `{0}` not found — is it installed?")]
    NotFound(String),
}

/// Launch an agent as a child process and return the spawned child + its PID.
///
/// The agent runs in non-interactive mode with structured output.
/// Caller is responsible for waiting on the child.
pub fn launch_agent(state: &State) -> Result<(Child, u32), AgentError> {
    let root = state
        .project_root
        .to_str()
        .ok_or(AgentError::NonUtf8Path)?;

    let (program, args) = state.agent.exec_command(root, state.phase);

    let child = Command::new(program)
        .args(&args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                AgentError::NotFound(program.to_string())
            } else {
                AgentError::Io(err)
            }
        })?;

    let pid = child.id();
    Ok((child, pid))
}

/// Check whether a process with the given PID is still running.
pub fn agent_running(pid: u32) -> bool {
    // kill(pid, 0) is the standard POSIX way to check process existence
    // without sending an actual signal.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Generate a human-readable label for the agent session.
pub fn agent_label(agent: Agent, pid: u32) -> String {
    format!("{}-{}", agent, pid)
}
