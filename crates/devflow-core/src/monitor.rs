//! Background monitor daemon.
//!
//! Spawns a detached child process that *owns* the coding agent: it launches
//! the agent, captures its stdout and exit code into `.devflow/`, and — when
//! the agent exits — runs `devflow check` to advance the state machine.
//!
//! Owning the agent is the key fix over a CLI-scoped capture thread: because
//! the monitor outlives `devflow start`, the agent's stdout keeps flowing into
//! the capture file and its exit code is still reaped after the CLI exits.
//!
//! This is the core automation primitive — no cron, no scheduler,
//! no agent cooperation needed.

use crate::state::State;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

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

/// Spawn a background monitor that owns the agent for the given workflow state.
///
/// The monitor is a detached shell process that:
/// 1. Launches the agent (`program` + `args`) with stdout redirected to the
///    phase stdout file, recording the agent PID to the agent-pid file
/// 2. Waits for the agent to exit and records its exit code to the exit file
/// 3. Runs `devflow check` to advance the workflow through its remaining steps
///
/// Returns the PID of the spawned monitor.
pub fn spawn_monitor(state: &State, program: &str, args: &[String]) -> Result<u32, MonitorError> {
    let project_root = state
        .project_root
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?;

    let binary = std::env::current_exe()
        .map_err(|_| MonitorError::NoBinaryPath)?
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?
        .to_string();

    let stdout_file = crate::agent_result::stdout_path(&state.project_root, state.phase);
    let exit_file = crate::agent_result::exit_code_path(&state.project_root, state.phase);
    let pid_file = crate::agent_result::agent_pid_path(&state.project_root, state.phase);

    // Ensure the capture directory exists before the detached process runs.
    if let Some(parent) = stdout_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let stdout_file = stdout_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
    let exit_file = exit_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
    let pid_file = pid_file.to_str().ok_or(MonitorError::NonUtf8Path)?;

    // The agent runs in its worktree when worktree mode is active; otherwise it
    // runs in the project root. Capture/state files and the `devflow check`
    // calls below always use the main project root, regardless of cwd.
    let workdir = state
        .worktree_path
        .as_deref()
        .unwrap_or(&state.project_root)
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?;

    // Build the shell-escaped agent command.
    let mut agent_cmd = shell_escape(program);
    for arg in args {
        agent_cmd.push(' ');
        agent_cmd.push_str(&shell_escape(arg));
    }

    // Shell script that launches the agent in the background, captures its
    // stdout and exit code, then advances the workflow. Because this process
    // is the agent's parent, capture survives the CLI exiting.
    //
    // stderr is discarded so it cannot corrupt the (possibly JSON) stdout
    // capture that DevFlow parses for DEVFLOW_RESULT.
    //
    // Traps SIGTERM and SIGINT for clean shutdown.
    let script = format!(
        "cleanup() {{ exit 0; }}; trap cleanup TERM INT; \
         cd {workdir} || exit 1; \
         {agent_cmd} > {stdout_file} 2>/dev/null & \
         apid=$!; echo $apid > {pid_file}; \
         wait $apid; echo $? > {exit_file}; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}; \
         {binary} check {project_root}",
        agent_cmd = agent_cmd,
        workdir = shell_escape(workdir),
        stdout_file = shell_escape(stdout_file),
        exit_file = shell_escape(exit_file),
        pid_file = shell_escape(pid_file),
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

/// Poll for the agent PID that the monitor records, for up to ~1 second.
///
/// Returns the PID once the monitor has launched the agent, or `None` if it
/// does not appear in time (the monitor still runs; only the display PID is lost).
pub fn wait_for_agent_pid(project_root: &Path, phase: u32) -> Option<u32> {
    let path = crate::agent_result::agent_pid_path(project_root, phase);
    for _ in 0..50 {
        if let Ok(contents) = std::fs::read_to_string(&path)
            && let Ok(pid) = contents.trim().parse::<u32>()
        {
            return Some(pid);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Escape a string for safe use in a single-quoted shell context.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Agent, State};

    fn state_in(root: &Path) -> State {
        let mut state = State::new(4, Agent::Claude, root.to_path_buf());
        state.step = crate::state::Step::Executing;
        state
    }

    #[test]
    fn shell_escape_wraps_basic_strings() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("/tmp/devflow"), "'/tmp/devflow'");
    }

    #[test]
    fn shell_escape_handles_single_quotes() {
        assert_eq!(shell_escape("can't"), "'can'\\''t'");
        assert_eq!(shell_escape("a'b'c"), "'a'\\''b'\\''c'");
    }

    #[test]
    fn shell_escape_handles_empty_string() {
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn wait_for_agent_pid_returns_pid_when_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            crate::agent_result::agent_pid_path(dir.path(), 4),
            "12345\n",
        )
        .unwrap();

        assert_eq!(wait_for_agent_pid(dir.path(), 4), Some(12345));
    }

    #[test]
    fn wait_for_agent_pid_returns_none_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(wait_for_agent_pid(dir.path(), 4), None);
    }

    #[test]
    fn wait_for_agent_pid_returns_none_for_garbage_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            crate::agent_result::agent_pid_path(dir.path(), 4),
            "not-a-pid",
        )
        .unwrap();

        assert_eq!(wait_for_agent_pid(dir.path(), 4), None);
    }

    #[test]
    fn spawn_monitor_captures_agent_pid_and_output() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path());
        // Stub agent: write a known marker to stdout, then exit cleanly.
        let args = vec!["-c".to_string(), "echo MONITOR_READY".to_string()];

        let monitor_pid = spawn_monitor(&state, "sh", &args).unwrap();
        assert!(monitor_pid > 0);

        // Observable side effect #1: the monitor records the agent PID to its
        // pid file with valid numeric content.
        let agent_pid = wait_for_agent_pid(dir.path(), state.phase)
            .expect("monitor should record the agent pid");
        assert!(agent_pid > 0);

        // Observable side effect #2: the agent's stdout is captured to the
        // phase stdout file (proving the monitor actually ran the agent).
        let stdout_path = crate::agent_result::stdout_path(dir.path(), state.phase);
        let mut captured = String::new();
        for _ in 0..100 {
            if let Ok(contents) = std::fs::read_to_string(&stdout_path)
                && contents.contains("MONITOR_READY")
            {
                captured = contents;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            captured.contains("MONITOR_READY"),
            "expected MONITOR_READY in captured stdout, got {captured:?}"
        );
    }
}
