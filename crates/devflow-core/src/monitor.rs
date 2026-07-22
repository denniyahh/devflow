//! Background monitor daemon.
//!
//! Spawns a detached child process that *owns* the coding agent: it launches
//! the agent, captures its stdout and exit code into `.devflow/`, and — when
//! the agent exits — runs `devflow advance` to advance the stage machine.
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
use tracing::{debug, info};

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
/// 3. Runs `devflow advance --phase N` to advance the workflow through its
///    remaining stages
///
/// Returns the PID of the spawned monitor.
pub fn spawn_monitor(
    state: &State,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
) -> Result<u32, MonitorError> {
    spawn_monitor_inner(state, program, args, envs, true)
}

/// Spawn a monitor that owns the agent and records its capture files but does
/// NOT advance the stage machine when the agent exits. Used by `sequentagent`,
/// which drives its own synchronous handoff loop: the CLI blocks on the exit
/// file (see [`wait_for_agent_exit`]) while the monitor guarantees capture
/// survives even if the CLI dies.
pub fn spawn_monitor_no_advance(
    state: &State,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
) -> Result<u32, MonitorError> {
    spawn_monitor_inner(state, program, args, envs, false)
}

fn spawn_monitor_inner(
    state: &State,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    run_advance: bool,
) -> Result<u32, MonitorError> {
    let project_root = state
        .project_root
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?;

    let binary = std::env::current_exe()
        .map_err(|_| MonitorError::NoBinaryPath)?
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?
        .to_string();

    info!(
        "spawning monitor for phase {}: {program} {}",
        state.phase,
        args.join(" ")
    );

    let stdout_file = crate::agent_result::stdout_path(&state.project_root, state.phase);
    let stderr_file = crate::agent_result::stderr_path(&state.project_root, state.phase);
    let exit_file = crate::agent_result::exit_code_path(&state.project_root, state.phase);
    let pid_file = crate::agent_result::agent_pid_path(&state.project_root, state.phase);

    // Ensure the capture directory exists before the detached process runs.
    if let Some(parent) = stdout_file.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }

    let stdout_file = stdout_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
    let stderr_file = stderr_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
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

    // Shell script that launches the agent in the background, captures its
    // stdout and exit code, then advances the workflow. Because this process
    // is the agent's parent, capture survives the CLI exiting.
    //
    // stderr is captured to a separate file so it cannot corrupt the (possibly
    // JSON) stdout capture that DevFlow parses for DEVFLOW_RESULT. Inspect
    // .devflow/phase-NN-stderr.log for agent error output on failures.
    //
    // `devflow advance --phase N` evaluates the agent result, moves the stage
    // machine forward, and (for an agent stage) spawns the next monitor
    // itself. The phase is recorded here at spawn time so advance's identity
    // never depends on a shared state singleton (13-DEFERRED-CR-03): under
    // `devflow parallel`, each phase's monitor advances exactly its own
    // stage machine.
    //
    // Traps SIGTERM and SIGINT for clean shutdown. WR-08 (13-REVIEW.md):
    // the trap must also kill the backgrounded agent ($apid) — previously
    // it only exited the monitor shell itself, orphaning the agent so it
    // kept running/committing unsupervised with nothing left to call
    // `devflow advance` once it finished. `apid` is initialized to empty
    // before the trap is installed so a signal arriving before the agent is
    // even backgrounded doesn't reference an unset variable.
    let advance_tail = if run_advance {
        format!(
            "; {binary} advance {project_root} --phase {phase}",
            binary = shell_escape(&binary),
            project_root = shell_escape(project_root),
            phase = state.phase,
        )
    } else {
        String::new()
    };
    let script = format!(
        "apid=''; cleanup() {{ [ -n \"$apid\" ] && kill \"$apid\" 2>/dev/null; exit 0; }}; \
         trap cleanup TERM INT; \
         cd {workdir} || exit 1; \
         \"$@\" > {stdout_file} 2>{stderr_file} & \
         apid=$!; echo $apid > {pid_file}; \
         wait $apid; echo $? > {exit_file}{advance_tail}",
        workdir = shell_escape(workdir),
        stdout_file = shell_escape(stdout_file),
        stderr_file = shell_escape(stderr_file),
        exit_file = shell_escape(exit_file),
        pid_file = shell_escape(pid_file),
    );

    let child = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .arg("sh")
        .arg(program)
        .args(args)
        // Adapter-scoped env (e.g. Codex's unsigned-commit override) rides
        // the whole monitor chain: sh → agent → its git children (13-06).
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let pid = child.id();
    info!("monitor spawned with pid {pid}");
    Ok(pid)
}

/// Poll for the agent PID that the monitor records, for up to ~1 second.
///
/// Returns the PID once the monitor has launched the agent, or `None` if it
/// does not appear in time (the monitor still runs; only the display PID is lost).
pub fn wait_for_agent_pid(project_root: &Path, phase: u32) -> Option<u32> {
    let path = crate::agent_result::agent_pid_path(project_root, phase);
    debug!("polling for agent PID for phase {phase}");
    for _ in 0..50 {
        if let Ok(contents) = std::fs::read_to_string(&path)
            && let Ok(pid) = contents.trim().parse::<u32>()
        {
            return Some(pid);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    debug!("agent PID not found for phase {phase} after polling");
    None
}

/// Block until the monitor records the agent's exit code, returning it.
///
/// Used by callers that need a synchronous run on top of monitor-owned
/// execution (sequentagent's rebase handoff). Polls the exit file; if the
/// monitor process disappears without ever writing it (killed, crashed),
/// returns an error instead of hanging forever. There is deliberately no
/// time-based cap — agent runs are legitimately long (tens of minutes) and
/// monitor liveness is the meaningful bound.
pub fn wait_for_agent_exit(
    project_root: &Path,
    phase: u32,
    monitor_pid: u32,
) -> Result<i32, MonitorError> {
    let exit_path = crate::agent_result::exit_code_path(project_root, phase);
    loop {
        if let Ok(contents) = std::fs::read_to_string(&exit_path)
            && let Ok(code) = contents.trim().parse::<i32>()
        {
            return Ok(code);
        }
        if !crate::agent::agent_running(monitor_pid) {
            // One final read: the monitor may have written the file and
            // exited between our read above and the liveness check.
            if let Ok(contents) = std::fs::read_to_string(&exit_path)
                && let Ok(code) = contents.trim().parse::<i32>()
            {
                return Ok(code);
            }
            return Err(MonitorError::Io(std::io::Error::other(format!(
                "monitor (pid {monitor_pid}) exited without recording an exit code for phase {phase}"
            ))));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Escape a string for safe use in a single-quoted shell context.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use crate::stage::Stage;
    use crate::state::{AgentKind, State};

    fn state_in(root: &Path) -> State {
        let mut state = State::new(4, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
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

        let monitor_pid = spawn_monitor(&state, "sh", &args, &[]).unwrap();
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

    /// WR-08 (13-REVIEW.md): sending SIGTERM/SIGINT to the monitor must also
    /// terminate the agent it owns. Before the fix, `cleanup()` only exited
    /// the monitor shell, leaving the agent orphaned and running/committing
    /// unsupervised with nothing left to call `devflow advance` for it.
    #[test]
    fn sigterm_to_monitor_also_kills_the_agent() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path());
        // Stub agent that runs long enough to observe: sleeps well past the
        // window this test needs to send SIGTERM and check liveness.
        let args = vec!["-c".to_string(), "sleep 30".to_string()];

        let monitor_pid = spawn_monitor(&state, "sh", &args, &[]).unwrap();
        let agent_pid = wait_for_agent_pid(dir.path(), state.phase)
            .expect("monitor should record the agent pid");
        assert!(
            crate::agent::agent_running(agent_pid),
            "agent should be running before SIGTERM"
        );

        // SIGTERM the monitor, as an operator (or lock.rs's stale-holder
        // reclaim path) would to abort a run.
        unsafe {
            libc::kill(monitor_pid as libc::pid_t, libc::SIGTERM);
        }

        // The agent should be killed promptly by the monitor's trap —
        // poll rather than sleep a fixed amount to keep this fast and
        // avoid flaking under load. (Window widened to 5s: at 2s this
        // still flaked under a fully parallel workspace test run.)
        let mut still_running = true;
        for _ in 0..250 {
            if !crate::agent::agent_running(agent_pid) {
                still_running = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !still_running,
            "agent (pid {agent_pid}) was orphaned — still running after monitor SIGTERM"
        );
    }

    /// 14b: sequentagent's synchronous handoff = no-advance monitor + a
    /// blocking wait on the exit file. The monitor still owns capture; the
    /// caller gets the real exit code back.
    #[test]
    fn no_advance_monitor_plus_wait_returns_exit_code_and_captures() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path());
        let args = vec!["-c".to_string(), "echo SEQ_READY; exit 3".to_string()];

        let monitor_pid = spawn_monitor_no_advance(&state, "sh", &args, &[]).unwrap();
        let code = wait_for_agent_exit(dir.path(), state.phase, monitor_pid)
            .expect("exit code must be reaped");

        assert_eq!(code, 3);
        let captured =
            std::fs::read_to_string(crate::agent_result::stdout_path(dir.path(), state.phase))
                .unwrap_or_default();
        assert!(
            captured.contains("SEQ_READY"),
            "stdout captured: {captured:?}"
        );
    }

    /// A dead monitor that never wrote the exit file must yield an error, not
    /// an infinite hang.
    #[test]
    fn wait_for_agent_exit_errors_when_monitor_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        // PID that is essentially certain not to be alive; no exit file.
        let err = wait_for_agent_exit(dir.path(), 4, 0x7FFF_FFFE).unwrap_err();
        assert!(err.to_string().contains("without recording an exit code"));
    }

    #[test]
    fn spawn_monitor_runs_agent_in_worktree_but_captures_in_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join(".worktrees/phase-04");
        std::fs::create_dir_all(&worktree).unwrap();
        let mut state = state_in(dir.path());
        state.worktree_path = Some(worktree.clone());

        // Stub agent: print its cwd so the test proves the monitor changed
        // directories before launching the agent.
        let args = vec!["-c".to_string(), "pwd; echo WORKTREE_READY".to_string()];

        let monitor_pid = spawn_monitor(&state, "sh", &args, &[]).unwrap();
        assert!(monitor_pid > 0);

        let agent_pid = wait_for_agent_pid(dir.path(), state.phase)
            .expect("monitor should record the agent pid in the main project");
        assert!(agent_pid > 0);

        let stdout_path = crate::agent_result::stdout_path(dir.path(), state.phase);
        let mut captured = String::new();
        for _ in 0..100 {
            if let Ok(contents) = std::fs::read_to_string(&stdout_path)
                && contents.contains("WORKTREE_READY")
            {
                captured = contents;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(
            captured.contains(&worktree.display().to_string()),
            "agent did not run in worktree cwd; captured stdout: {captured:?}"
        );
        assert!(
            stdout_path.exists(),
            "stdout capture missing in main .devflow"
        );
        assert!(
            !crate::agent_result::stdout_path(&worktree, state.phase).exists(),
            "stdout capture should not be written under the worktree"
        );
    }

    #[test]
    fn spawn_monitor_treats_agent_args_as_literal_argv() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path());
        let payload = "value; touch INJECTED";
        let args = vec![
            "-c".to_string(),
            "printf '%s\\n' \"$0\"; echo ARGV_SAFE".to_string(),
            payload.to_string(),
        ];

        spawn_monitor(&state, "sh", &args, &[]).unwrap();
        wait_for_agent_pid(dir.path(), state.phase).expect("monitor should record the agent pid");

        let stdout_path = crate::agent_result::stdout_path(dir.path(), state.phase);
        let mut captured = String::new();
        for _ in 0..100 {
            if let Ok(contents) = std::fs::read_to_string(&stdout_path)
                && contents.contains("ARGV_SAFE")
            {
                captured = contents;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(
            captured.contains(payload),
            "literal argv missing: {captured:?}"
        );
        assert!(captured.contains("ARGV_SAFE"));
        assert!(!dir.path().join("INJECTED").exists());
    }
}
