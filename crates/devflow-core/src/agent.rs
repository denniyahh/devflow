//! Agent process launching — spawns coding agents as child processes.
//!
//! All agents run in non-interactive mode (`claude -p`, `codex exec`).
//! They produce structured output and exit when done — never block on input.

use crate::agent_result::{self, AgentCapture};
use crate::state::AgentKind;
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
/// `workdir` is the directory the agent process runs in — the project root in
/// normal mode, or a git worktree path when worktree mode is active. Capture
/// and state files are keyed off the main project root separately.
///
/// The agent runs in non-interactive mode with structured output.
/// Caller is responsible for waiting on the child.
pub fn launch_agent(
    agent: &dyn crate::agents::AgentAdapter,
    phase: u32,
    prompt: &str,
    workdir: &Path,
    extra_writable_roots: &[std::path::PathBuf],
) -> Result<(Child, u32), AgentError> {
    let root = workdir.to_str().ok_or(AgentError::NonUtf8Path)?;

    let (program, args) = agent.exec_command(phase, prompt, extra_writable_roots);

    let child = Command::new(program)
        .args(&args)
        .envs(agent.extra_env())
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
///
/// The PID typically comes from parsing an on-disk file, so hostile or
/// corrupted values must be rejected, not reinterpreted: `kill(0, sig)`
/// signals the caller's own process group (a "0" PID file would read as
/// permanently alive), and a value above `i32::MAX` would wrap negative
/// through an `as libc::pid_t` cast — `kill(-1, 0)` probes every process
/// the caller may signal and virtually always succeeds.
pub fn agent_running(pid: u32) -> bool {
    // kill(pid, 0) is the standard POSIX way to check process existence
    // without sending an actual signal.
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    pid > 0 && unsafe { libc::kill(pid, 0) == 0 }
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
    // Read stdout pipe. Read raw bytes and convert lossily instead of
    // failing the whole capture on invalid UTF-8 — never silently drop
    // output that may carry the DEVFLOW_RESULT marker.
    let mut buf = Vec::new();
    if let Some(ref mut pipe) = child.stdout {
        let _ = pipe.read_to_end(&mut buf);
    }
    // Cow: borrows `buf` unchanged when it is valid UTF-8 (the common case),
    // allocating only when replacement characters are actually needed.
    let stdout = String::from_utf8_lossy(&buf);

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
    let _ = std::fs::write(&stdout_path, stdout.as_bytes());

    // Write exit code to file.
    let exit_path = agent_result::exit_code_path(project_root, phase);
    let _ = std::fs::write(&exit_path, exit_code.to_string());

    Ok(AgentCapture { exit_code })
}

/// Generate a human-readable label for the agent session.
pub fn agent_label(agent: AgentKind, pid: u32) -> String {
    format!("{}-{}", agent, pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

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
    fn agent_running_rejects_corrupt_pid_values() {
        // "0" from a truncated PID file: kill(0, 0) would signal our own
        // process group and report alive.
        assert!(!agent_running(0));
        // Above i32::MAX: `as libc::pid_t` would wrap to -1, and
        // kill(-1, 0) probes every signalable process — almost always "alive".
        assert!(!agent_running(u32::MAX));
        assert!(!agent_running(i32::MAX as u32 + 1));
    }

    #[test]
    fn agent_label_combines_agent_and_pid() {
        assert_eq!(agent_label(AgentKind::Claude, 42), "claude-42");
        assert_eq!(agent_label(AgentKind::OpenCode, 7), "opencode-7");
    }

    #[test]
    fn capture_agent_output_writes_stdout_file() {
        let dir = tempfile::tempdir().unwrap();
        let child = Command::new("sh")
            .args(["-c", "printf 'hello\n'"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let capture = capture_agent_output(child, 7, dir.path()).unwrap();

        assert_eq!(capture.exit_code, 0);
        assert_eq!(
            std::fs::read_to_string(agent_result::stdout_path(dir.path(), 7)).unwrap(),
            "hello\n"
        );
    }

    #[test]
    fn capture_agent_output_records_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let child = Command::new("sh")
            .args(["-c", "exit 42"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let capture = capture_agent_output(child, 8, dir.path()).unwrap();

        assert_eq!(capture.exit_code, 42);
        assert_eq!(
            std::fs::read_to_string(agent_result::exit_code_path(dir.path(), 8)).unwrap(),
            "42"
        );
    }

    #[test]
    fn capture_agent_output_handles_empty_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let child = Command::new("sh")
            .args(["-c", "true"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let capture = capture_agent_output(child, 9, dir.path()).unwrap();

        assert_eq!(capture.exit_code, 0);
        assert_eq!(
            std::fs::read_to_string(agent_result::stdout_path(dir.path(), 9)).unwrap(),
            ""
        );
    }
}
