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

use crate::git::hermetic_command;
use crate::state::State;
use std::path::Path;
use std::process::Stdio;
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
    let workdir_path = state
        .worktree_path
        .as_deref()
        .unwrap_or(&state.project_root);
    let workdir = workdir_path.to_str().ok_or(MonitorError::NonUtf8Path)?;

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

    // 27-REVIEW WR-03: built through `hermetic_command`, not a bare
    // `Command::new("sh")`. This is the spawn that launches the coding agent
    // itself, and the comment below is precisely the hazard: whatever
    // environment this `sh` carries rides down into the agent and into every
    // git command the agent runs. An inherited `GIT_DIR` here would silently
    // retarget the phase's real commits at a repository the operator never
    // named — the worst case this phase exists to prevent, on its
    // highest-consequence call site.
    //
    // Ordering is load-bearing: `hermetic_command` does its `env_remove`s at
    // construction, and `.envs(...)` below runs after, so an adapter that
    // deliberately sets one of these variables still wins. Deliberate
    // configuration survives; inherited pollution does not. That is what
    // keeps Codex's unsigned-commit override (`GIT_CONFIG_*`) working.
    let child = hermetic_command("sh", workdir_path)
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
    /// A one-line identity/state summary of a pid, for failure diagnostics.
    /// `Name`/`State`/`PPid` come from `/proc/<pid>/status`; the cmdline
    /// distinguishes a shell that exec'd its command from one that forked it.
    /// Test-only; never used in a decision.
    fn proc_snapshot(pid: u32) -> String {
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            return format!("GONE (no /proc/{pid})");
        };
        let field = |key: &str| {
            status
                .lines()
                .find(|l| l.starts_with(key))
                .map(|l| l.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
                .unwrap_or_else(|| "?".into())
        };
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline"))
            .map(|raw| {
                let joined = raw
                    .split(|&b| b == 0)
                    .filter(|a| !a.is_empty())
                    .map(|a| String::from_utf8_lossy(a).into_owned())
                    .collect::<Vec<_>>()
                    .join(" ");
                if joined.is_empty() {
                    "<empty>".to_string()
                } else {
                    joined
                }
            })
            .unwrap_or_else(|e| format!("<unreadable: {e}>"));
        format!(
            "ALIVE Name={} State={} PPid={} cmdline=[{cmdline}]",
            field("Name:"),
            field("State:"),
            field("PPid:")
        )
    }

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

        // Snapshot both processes before signalling. This assertion fails in
        // containerised CI and cannot be reproduced locally, and a bare
        // "still running" message discards everything that could explain it
        // — the same antipattern that made 999.47 expensive to diagnose.
        let monitor_before = proc_snapshot(monitor_pid);
        let agent_before = proc_snapshot(agent_pid);

        // SIGTERM the monitor, as an operator (or lock.rs's stale-holder
        // reclaim path) would to abort a run.
        let kill_rc = unsafe { libc::kill(monitor_pid as libc::pid_t, libc::SIGTERM) };
        let kill_err = if kill_rc == 0 {
            "ok".to_string()
        } else {
            format!("errno {}", std::io::Error::last_os_error())
        };

        // The agent should be killed promptly by the monitor's trap —
        // poll rather than sleep a fixed amount to keep this fast and
        // avoid flaking under load. (Window widened to 5s: at 2s this
        // still flaked under a fully parallel workspace test run.)
        //
        // 2026-07-26: this was widened 5s -> 15s for the containerised CI
        // job and STILL failed, then reverted to 5s. That widening was a
        // mistake: 15s is far beyond any plausible trap-and-kill latency,
        // so the agent is not being reaped SLOWLY, it is not being reaped.
        // Buying silence with a bigger number would have hidden a real
        // defect behind a green check — the exact false negative this
        // repository keeps getting bitten by.
        //
        // The trap mechanism itself is verified working: DevFlow's real
        // monitor script shape was run under both `bash` and `dash` (the
        // container's /bin/sh is dash, the Fedora host's is bash) and both
        // killed the backgrounded agent correctly. So the defect is in how
        // the agent is spawned or identified under container timing, not in
        // the shell trap — see 999.47, whose confirmed transient fork/exec
        // window is the prime suspect for the same class of failure here.
        //
        // Leave this red until that is fixed. Do NOT widen it again.
        let mut still_running = true;
        for _ in 0..250 {
            if !crate::agent::agent_running(agent_pid) {
                still_running = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let monitor_after = proc_snapshot(monitor_pid);
        let agent_after = proc_snapshot(agent_pid);
        let pidfile =
            std::fs::read_to_string(crate::agent_result::agent_pid_path(dir.path(), state.phase))
                .unwrap_or_else(|e| format!("<unreadable: {e}>"));

        assert!(
            !still_running,
            "agent (pid {agent_pid}) was orphaned — still running after monitor SIGTERM\n\
             \x20 monitor pid:      {monitor_pid}\n\
             \x20 kill(TERM) rc:    {kill_rc} ({kill_err})\n\
             \x20 monitor before:   {monitor_before}\n\
             \x20 monitor after:    {monitor_after}\n\
             \x20 agent pid:        {agent_pid}\n\
             \x20 agent before:     {agent_before}\n\
             \x20 agent after:      {agent_after}\n\
             \x20 pidfile contents: {}\n\
             Read the monitor's `after` line first. GONE means the shell died \
             without running its trap — most likely SIGTERM arrived before \
             `trap` was installed, or it was killed rather than handling the \
             signal, either way leaving the agent unreaped. STILL ALIVE means \
             the trap never fired or `kill $apid` failed, so compare the agent \
             pid against the pidfile and check the agent's PPid: if PPid is not \
             the monitor, `$!` did not name the process we are polling. If the \
             agent's Name is `sh` rather than `sleep`, the agent shell forked \
             rather than exec'd, so killing it leaves its own child behind.",
            pidfile.trim()
        );
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

    /// Build the fixture repositories through the scrubbing constructor, as
    /// every other test module in this phase does (`version.rs:1102`).
    ///
    /// A bare `Command::new("git")` here would itself inherit an ambient
    /// hostile `GIT_DIR` — so under this phase's own acceptance command
    /// (`GIT_DIR=<throwaway>/.git cargo test -p devflow-core ...`) the
    /// fixture setup would target the throwaway repository instead of
    /// `root`, and the test below would fail for a reason that has nothing
    /// to do with the behavior it is guarding.
    fn git(root: &Path, args: &[&str]) {
        let ok = crate::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn init_repo(root: &Path) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
    }

    /// 27-REVIEW WR-03: the `sh` this function spawns owns the coding
    /// agent, and whatever environment rides down with it reaches every git
    /// command the agent runs (`sh` -> agent -> agent's git children). This
    /// proves the scrub with a real spawned agent process, not by
    /// inspecting the `Command` object: the agent shells out to
    /// `git rev-parse --absolute-git-dir`, and the resolved path must be
    /// the caller's own workdir, never a hostile `GIT_DIR` pointed at an
    /// unrelated foreign repository.
    ///
    /// Mirrors `tag_reads_resolve_caller_root_under_a_hostile_git_dir`
    /// (version.rs, 27-03/WR-01): `GIT_DIR` is never set on this test
    /// process itself (Rust 2024 `unsafe`, unsound under threaded tests —
    /// Phase 25 D-14), only on one freshly spawned child re-invoking this
    /// binary filtered to this test.
    #[test]
    fn spawn_monitor_agent_git_calls_resolve_workdir_not_a_hostile_git_dir() {
        const INNER_ROOT: &str = "DEVFLOW_27_MONITOR_INNER_ROOT";

        if let Ok(root) = std::env::var(INNER_ROOT) {
            // Inner mode: GIT_DIR points at a foreign repository unrelated
            // to `root`, scoped to this child process only.
            let root = std::path::PathBuf::from(root);
            let state = state_in(&root);
            let args = vec![
                "-c".to_string(),
                "git rev-parse --absolute-git-dir".to_string(),
            ];

            spawn_monitor(&state, "sh", &args, &[]).unwrap();
            wait_for_agent_pid(&root, state.phase).expect("monitor should record the agent pid");

            let stdout_path = crate::agent_result::stdout_path(&root, state.phase);
            let mut captured = String::new();
            for _ in 0..100 {
                if let Ok(contents) = std::fs::read_to_string(&stdout_path)
                    && !contents.trim().is_empty()
                {
                    captured = contents;
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }

            let resolved = std::fs::canonicalize(captured.trim())
                .expect("agent's reported git-dir must exist on disk");
            let expected =
                std::fs::canonicalize(root.join(".git")).expect("caller repo .git must exist");
            assert_eq!(
                resolved, expected,
                "agent's git call resolved to a hostile GIT_DIR's \
                 repository instead of the caller's own workdir: \
                 got {resolved:?}, want {expected:?}"
            );
            return;
        }

        // Outer mode: a real repository at `root`, and an unrelated
        // foreign repository whose .git must never leak into the agent's
        // environment.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("caller-repo");
        std::fs::create_dir_all(&root).unwrap();
        init_repo(&root);

        let foreign = tempfile::tempdir().unwrap();
        init_repo(foreign.path());

        let exe = std::env::current_exe().expect("current_exe for child re-invocation");
        let out = std::process::Command::new(&exe)
            // Substring filter, NOT `--exact`: the binary's real test name
            // is module-qualified (`monitor::tests::spawn_monitor_...`), so
            // `--exact` against the bare name matches nothing, runs zero
            // tests, and still exits 0 — a false green.
            .arg("spawn_monitor_agent_git_calls_resolve_workdir_not_a_hostile_git_dir")
            .arg("--test-threads=1")
            .env(INNER_ROOT, root.to_str().unwrap())
            .env("GIT_DIR", foreign.path().join(".git"))
            .output()
            .expect("spawn hostile child test process");

        let stdout = String::from_utf8_lossy(&out.stdout);
        // Assert the child actually RAN the test, not merely that it
        // exited 0. A filter that matches nothing exits 0 with "0 passed".
        assert!(
            stdout.contains("1 passed"),
            "child test process must have run exactly the inner test; \
             stdout:\n{stdout}"
        );
        assert!(
            out.status.success(),
            "monitor-spawned agent (hostile GIT_DIR pointed at an \
             unrelated foreign repository) must still resolve its git \
             calls against the caller's own workdir; child exit status \
             {:?}\nstdout:\n{stdout}",
            out.status
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
