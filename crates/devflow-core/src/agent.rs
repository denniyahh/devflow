//! Agent process launching — spawns coding agents as child processes.
//!
//! All agents run in non-interactive mode (`claude -p`, `codex exec`).
//! They produce structured output and exit when done — never block on input.

use crate::agent_result::{self, AgentCapture};
use crate::state::Agent;
use std::io::Read;
use std::path::Path;
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
pub fn launch_agent(
    agent: &dyn crate::agents::Agent,
    phase: u32,
    project_root: &Path,
) -> Result<(Child, u32), AgentError> {
    let root = project_root.to_str().ok_or(AgentError::NonUtf8Path)?;

    let (program, args) = agent.exec_command(phase);

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

/// Capture agent stdout and exit code, writing both to `.devflow/` files.
///
/// Ownership of the Child is transferred into this function — the caller
/// must not drop or wait on the child after calling this.
///
/// This is used by both blocking mode (called directly) and monitor mode
/// (wrapped in `thread::spawn`).
pub fn capture_agent_output(
    mut child: Child,
    phase: u32,
    project_root: &Path,
) -> std::io::Result<AgentCapture> {
    // Read stdout pipe.
    let mut stdout = String::new();
    if let Some(ref mut pipe) = child.stdout {
        let _ = pipe.read_to_string(&mut stdout);
    }

    // Wait for exit code.
    let exit_code = match child.wait() {
        Ok(status) => status.code().unwrap_or(-1),
        Err(_) => -1,
    };

    // Write stdout to file.
    let stdout_path = agent_result::stdout_path(project_root, phase);
    if let Some(parent) = stdout_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&stdout_path, &stdout);

    // Write exit code to file.
    let exit_path = agent_result::exit_code_path(project_root, phase);
    let _ = std::fs::write(&exit_path, exit_code.to_string());

    Ok(AgentCapture {
        stdout: stdout.clone(),
        exit_code,
    })
}

/// Generate a human-readable label for the agent session.
pub fn agent_label(agent: Agent, pid: u32) -> String {
    format!("{}-{}", agent, pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_running_detects_self() {
        // The current process is, by definition, running.
        assert!(agent_running(std::process::id()));
    }

    #[test]
    fn agent_running_false_for_dead_pid() {
        // A PID near the top of the range is essentially never live.
        assert!(!agent_running(0x7FFF_FFFE));
    }

    #[test]
    fn agent_label_combines_agent_and_pid() {
        assert_eq!(agent_label(Agent::Claude, 42), "claude-42");
        assert_eq!(agent_label(Agent::OpenCode, 7), "opencode-7");
    }
}
