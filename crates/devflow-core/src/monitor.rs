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
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;
use tracing::{debug, info, warn};

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
    /// A child spawned with piped stdio did not expose one of its pipes.
    #[error("supervised child exposed no {0} pipe")]
    NoChildPipe(&'static str),
}

/// Idle-timeout default in seconds (D-02): the measured constraint-8 floor.
///
/// Plan 31-02 supplies the configurable-and-clamped reader that can only raise
/// this. Until then `spawn_monitor` passes this literal to the monitor process.
///
/// Do NOT "correct" this upward on the assumption it is tight. The ≥30s floor
/// was derived from the *milestone* signal (pooled max 13.73s); against the
/// every-line signal this monitor actually uses, the observed max is 7.09s, so
/// 30s is ~4.2x margin.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30;

/// Which supervision shape [`spawn_monitor`] should launch.
///
/// This is a MODE selection on one supervisor, not two monitors: both arms
/// write the same capture, exit-code and agent-pid files under `.devflow/`,
/// and both end by advancing the same stage machine. Nothing downstream needs
/// to know which arm ran.
pub enum MonitorLaunch {
    /// Phase 31: a Rust supervisor that owns BOTH of the child's pipes,
    /// delivers `prompt` as a JSON user turn on the child's stdin, and holds
    /// that stdin open past the child's first turn so a task-notification turn
    /// can still be delivered (constraint 4).
    PipeOwning {
        /// The stage prompt, delivered on the child's stdin rather than argv.
        prompt: String,
    },
    /// The pre-31 detached `sh` script: stdin is `/dev/null`, stdout is
    /// redirected to the capture file by the shell, and the script waits on
    /// the agent then runs `devflow advance`. Every non-Claude adapter, every
    /// stage not yet widened by D-09/D-10's rollout, and the checkpoint-resume
    /// relaunch all run through here, unchanged.
    Legacy,
}

/// Spawn a background monitor that owns the agent for the given workflow state.
///
/// The monitor is a detached process that:
/// 1. Launches the agent (`program` + `args`) with stdout captured to the
///    phase stdout file, recording the agent PID to the agent-pid file
/// 2. Waits for the agent to exit and records its exit code to the exit file
/// 3. Runs `devflow advance --phase N` to advance the workflow through its
///    remaining stages
///
/// `launch` selects the supervision shape — see [`MonitorLaunch`].
///
/// Returns the PID of the spawned monitor.
pub fn spawn_monitor(
    state: &State,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    launch: MonitorLaunch,
) -> Result<u32, MonitorError> {
    spawn_monitor_inner(state, program, args, envs, launch, true)
}

fn spawn_monitor_inner(
    state: &State,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    launch: MonitorLaunch,
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

    if let MonitorLaunch::PipeOwning { prompt } = launch {
        // `run_advance` is not consulted on this arm: the `__monitor`
        // subcommand always advances after reaping, and `spawn_monitor` is the
        // only caller of this function — it hardcodes `true`. Adding a
        // `--no-advance` flag for a case nothing exercises would be an
        // untested branch; add it when a caller actually needs it.
        let _ = run_advance;

        // The adapter's extra env rides down by INHERITANCE here (set via
        // `.envs(...)` on the `__monitor` process below), and that is only
        // sufficient because the sole adapter routed through this arm —
        // Claude — declares no extra env at all
        // (`codex_disables_signing_via_env_others_do_not` asserts this).
        // Widening this arm to an adapter that DOES set env requires
        // threading it explicitly to `run_pipe_owning_monitor`: the inner
        // `hermetic_command` scrubs `GIT_CONFIG_COUNT`, which neutralises any
        // inherited `GIT_CONFIG_KEY_n` pair (Codex's unsigned-commit
        // override is exactly that shape). Loud rather than silent, and in
        // the CLI process where an operator can actually see it.
        if !envs.is_empty() {
            warn!(
                "pipe-owning monitor: {} adapter env var(s) will not survive the \
                 inner hermetic_command scrub — thread them explicitly before \
                 routing an env-setting adapter through this arm",
                envs.len()
            );
        }

        // The prompt travels as a FILE, not argv: argv has a hard length
        // ceiling and DevFlow stage prompts routinely exceed what is safe to
        // pass positionally.
        let prompt_file = crate::agent_result::prompt_path(&state.project_root, state.phase);
        std::fs::write(&prompt_file, &prompt)?;
        let prompt_file = prompt_file.to_str().ok_or(MonitorError::NonUtf8Path)?;

        // Re-exec THIS binary as its hidden `__monitor` subcommand. The
        // monitor must outlive `devflow start`/`advance`, so it has to be a
        // distinct OS process; re-exec needs no daemonization primitive beyond
        // `spawn()`-without-`wait()`, which is exactly what the `sh` monitor
        // below already relies on.
        //
        // Ordering is load-bearing for the same reason the Legacy arm's
        // comment gives: `hermetic_command` does its `env_remove`s at
        // construction and `.envs(...)` runs after, so deliberate
        // configuration survives while inherited pollution does not.
        let child = hermetic_command(&binary, workdir_path)
            .arg("__monitor")
            .arg("--project")
            .arg(project_root)
            .arg("--phase")
            .arg(state.phase.to_string())
            .arg("--workdir")
            .arg(workdir)
            .arg("--prompt-file")
            .arg(prompt_file)
            .arg("--idle-timeout-secs")
            .arg(DEFAULT_IDLE_TIMEOUT_SECS.to_string())
            .arg("--")
            .arg(program)
            .args(args)
            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let pid = child.id();
        info!("pipe-owning monitor spawned with pid {pid}");
        return Ok(pid);
    }

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

/// The single place the stdin wire shape is constructed: one line of JSON
/// carrying the initial user turn for a `--input-format stream-json` child.
///
/// Shape (`{"type":"user","message":{"role":"user","content":<prompt>}}`) is
/// reproduced from the three archived Phase 30 harnesses, which all wrote
/// exactly this and got a working turn back.
///
/// Built with `serde_json` rather than `format!` so the prompt is ESCAPED, not
/// interpolated. A stage prompt is arbitrary text containing quotes, newlines
/// and backslashes; interpolating it would produce a torn JSON line the CLI
/// rejects, and a prompt could then alter the surrounding document's structure.
pub fn user_turn_line(prompt: &str) -> String {
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": prompt },
    })
    .to_string()
}

/// Supervise a `stream-json` child, owning both of its pipes, until the close
/// rule is satisfied and the child exits. Returns the child's exit code, which
/// is also written to the phase exit file.
///
/// This runs INSIDE the detached `__monitor` process, not in the CLI.
///
/// Threading model (constraint 4 / T-31-04). Three participants:
/// - a **writer thread** owning the child's stdin: it writes the initial user
///   turn, then BLOCKS on a channel rather than returning. It drops stdin only
///   when told to, because constraint 4's `AND` can never be honoured if stdin
///   is already gone — a task-notification turn arriving after the child's
///   first turn would have nowhere to be delivered.
/// - a **reader thread** owning the child's stdout: it tees each line verbatim
///   to the capture file and forwards it to the supervisor. Dropping its
///   sender at EOF is what surfaces `Disconnected` below.
/// - the **supervisor** (this function's own thread), which applies the close
///   rule and reaps.
///
/// The write and the read MUST be on independent threads. Writing the prompt
/// synchronously before reading stdout is the textbook two-pipe deadlock: it
/// passes every short-prompt smoke test and hangs on exactly the context-heavy
/// production stages that matter (the Linux pipe buffer is commonly 64KiB and
/// a DevFlow stage prompt can exceed that in one write).
#[allow(clippy::too_many_arguments)]
pub fn run_pipe_owning_monitor(
    project_root: &Path,
    phase: u32,
    workdir: &Path,
    prompt: &str,
    idle_timeout: Duration,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
) -> Result<i32, MonitorError> {
    let stdout_file = crate::agent_result::stdout_path(project_root, phase);
    let stderr_file = crate::agent_result::stderr_path(project_root, phase);
    let exit_file = crate::agent_result::exit_code_path(project_root, phase);
    let pid_file = crate::agent_result::agent_pid_path(project_root, phase);
    if let Some(parent) = stdout_file.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }

    // stderr goes to its own file so it cannot corrupt the JSONL stdout
    // capture DevFlow parses — the same separation the Legacy script's
    // `2>{stderr_file}` provides.
    let stderr_handle = std::fs::File::create(&stderr_file)?;
    // One handle, opened once, truncating at open and appending line by line.
    // Truncate-at-open reproduces the Legacy arm's `>` redirection exactly, so
    // a capture from a previous attempt can never be mixed into this one's
    // (the launch path archives the prior capture first, but relying on that
    // to make an append-mode open safe would be an unstated coupling).
    let mut capture = std::fs::File::create(&stdout_file)?;

    let mut child = hermetic_command(program, workdir)
        .args(args)
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr_handle))
        // T-31-05: make the child its own process-group leader so a later
        // group signal cannot reach this monitor's own ancestors. Verified
        // source shows the pre-31 `spawn_monitor` had NO session or group
        // configuration at all — detachment came only from the parent not
        // waiting — so this closes a gap rather than preserving one.
        // Full `setsid()` session detachment is deliberately NOT done: no
        // forensics record cites a SIGHUP-related monitor loss, so there is
        // no evidence it buys anything. `pre_exec` calling `libc::setsid()`
        // is the one-line follow-on if such a loss ever surfaces.
        .process_group(0)
        .spawn()?;

    // Recorded immediately, before any pipe work: `wait_for_agent_pid` polls
    // for this and the rest of DevFlow's liveness reporting depends on it.
    let child_pid = child.id();
    std::fs::write(&pid_file, format!("{child_pid}\n"))?;

    let mut child_stdin = child
        .stdin
        .take()
        .ok_or(MonitorError::NoChildPipe("stdin"))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or(MonitorError::NoChildPipe("stdout"))?;

    let (close_tx, close_rx) = mpsc::channel::<()>();
    let turn = user_turn_line(prompt);
    let writer = std::thread::spawn(move || {
        let wrote = child_stdin
            .write_all(turn.as_bytes())
            .and_then(|()| child_stdin.write_all(b"\n"))
            .and_then(|()| child_stdin.flush());
        if let Err(err) = wrote {
            warn!("could not write the initial user turn to the child's stdin: {err}");
            return;
        }
        // Deliberately NOT dropping stdin here — see this function's doc.
        // Either signal (an explicit close, or the supervisor dropping its
        // sender) means the same thing: stop holding the pipe open.
        let _ = close_rx.recv();
        drop(child_stdin);
    });

    let (line_tx, line_rx) = mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(child_stdout).lines() {
            let Ok(line) = line else {
                // A read error is EOF for supervision purposes.
                break;
            };
            // Tee VERBATIM before any interpretation: the whole Layer 1
            // cascade reads this file, and a line the close rule ignores
            // (unparseable noise, interleaved prose) must still reach it.
            if let Err(err) = writeln!(capture, "{line}") {
                warn!("could not append to the capture file: {err}");
            }
            let _ = capture.flush();
            if line_tx.send(line).is_err() {
                break;
            }
        }
        // Dropping `line_tx` here is what surfaces `Disconnected` below.
    });

    // --- Close rule (constraint 4): an AND of two arms, neither sufficient ---
    let mut marker_seen = false;
    let mut pending_background_tasks: Option<usize> = None;
    let mut close_signalled = false;

    loop {
        match line_rx.recv_timeout(idle_timeout) {
            Ok(line) => {
                if close_signalled {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                    if crate::agent_result::event_is_top_level_result_marker(&event) {
                        marker_seen = true;
                    }
                    if event.get("type").and_then(serde_json::Value::as_str) == Some("system")
                        && event.get("subtype").and_then(serde_json::Value::as_str)
                            == Some("background_tasks_changed")
                        && let Some(tasks) =
                            event.get("tasks").and_then(serde_json::Value::as_array)
                    {
                        pending_background_tasks = Some(tasks.len());
                    }
                }
                // Vacuously satisfied when nothing was ever announced — the
                // common single-plan case.
                let drained = matches!(pending_background_tasks, None | Some(0));
                if marker_seen && drained {
                    // The drain ALONE is never a stop signal: 30d measured the
                    // drain-to-final-`result` lag at 4.54–11.51s across 14
                    // trials, and closing at the drain would have truncated
                    // the final orchestrator turn in all seven 30d trials.
                    let _ = close_tx.send(());
                    close_signalled = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Plan 31-02 owns what this arm must actually do: write the
                // authoritative IdleTimeout result to its OWN side-channel
                // file first, then terminate the child's process group via
                // `crate::agent::terminate_and_verify` (D-05/D-06, and
                // RESEARCH Pitfall 3 on why the side channel cannot be the
                // stdout capture). Falling through to reaping here is a
                // placeholder, not a partial implementation of that.
                break;
            }
        }
    }

    // Guarantee stdin is released before waiting. A child still holding an
    // open stdin may never exit, and `child.wait()` would then block forever.
    drop(close_tx);

    let status = child.wait()?;
    let code = status.code().unwrap_or(-1);
    std::fs::write(&exit_file, format!("{code}\n"))?;

    let _ = writer.join();
    let _ = reader.join();

    info!("supervised child {child_pid} exited with code {code}");
    Ok(code)
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

    /// The Phase 31 tracer: ONE Claude-shaped stage driven end to end through
    /// the pipe-owning supervisor.
    ///
    /// The stub behaves like the real CLI on the two axes under test and no
    /// others: it takes its initial turn from stdin, and it keeps stdin open
    /// as a channel it can still be spoken to on. It is a `sh` script because
    /// the wire behaviour is the subject, not the binary.
    ///
    /// **The early-close negative control is the point of the probe files.**
    /// A stub that merely blocks on stdin EOF before exiting cannot fail:
    /// whether the monitor closes stdin immediately after the write or only
    /// after the close rule is satisfied, the stub still eventually sees EOF
    /// and still exits 0. So the stub instead SAMPLES stdin liveness at a
    /// moment when a correct monitor provably has not closed it — after the
    /// drain, before any marker — and records `EARLY` if it is already gone.
    /// Two files that must disagree: `eof` must exist at the end, `early`
    /// must never exist.
    ///
    /// **The prompt sentinel is a negative control on JSON escaping.** The
    /// sentinel sits on the SECOND line of a multi-line prompt containing a
    /// double quote. `user_turn_line` escapes it, so the whole prompt arrives
    /// as one physical line and the stub's single `read` sees the sentinel. A
    /// `format!`-interpolated implementation would emit a torn two-line
    /// document, the stub's `read` would return only the first line, and the
    /// sentinel check would fail — which is exactly what should happen.
    #[test]
    fn pipe_owning_monitor_delivers_prompt_via_stdin_and_captures_stream() {
        const SENTINEL: &str = "TRACER-PROMPT-SENTINEL";

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 4u32;
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let eof_file = root.join("stdin-eof");
        let early_file = root.join("stdin-closed-early");

        // A quote on line one, the sentinel on line two — see the doc above.
        let prompt = format!("first line with a \" quote\n{SENTINEL}");

        let script = format!(
            r#"
set -u
IFS= read -r turn || {{ echo "NO_INITIAL_TURN_ON_STDIN" >&2; exit 91; }}
case "$turn" in
  *{SENTINEL}*) ;;
  *) echo "INITIAL_TURN_MISSING_PROMPT: $turn" >&2; exit 92 ;;
esac

# Probe: block on stdin until EOF, then record it. stdout is redirected so
# this subshell does not hold the capture pipe open after the main shell exits.
#
# `exec 3<&0` then `cat <&3` is load-bearing, not a flourish: POSIX assigns
# /dev/null to a BACKGROUNDED list's stdin before any explicit redirection
# when job control is off. A bare `( cat > /dev/null ) &` therefore reads EOF
# instantly and reports an early close that never happened. The explicit
# `<&3` is applied after that default and overrides it.
exec 3<&0
( cat <&3 > /dev/null; printf 'EOF\n' > '{eof}' ) > /dev/null 2>&1 &

printf '%s\n' '{{"type":"system","subtype":"init","session_id":"tracer-1"}}'
printf '%s\n' '{{"type":"system","subtype":"background_tasks_changed","tasks":[{{"task_id":"t1","task_type":"local_agent"}}]}}'
printf '%s\n' '{{"type":"system","subtype":"background_tasks_changed","tasks":[]}}'

# The drain has landed but no marker has. A correct monitor is still holding
# stdin open here; sample it and record the violation if it is not.
sleep 0.5
if [ -f '{eof}' ]; then printf 'EARLY\n' > '{early}'; fi

printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"session_id":"tracer-1","result":"DEVFLOW_RESULT: {{\"status\":\"success\",\"commits\":2}}"}}'

# Bounded wait for EOF: a monitor that never closes stdin must fail the
# assertions below, not hang the suite.
i=0
while [ $i -lt 100 ] && [ ! -f '{eof}' ]; do
  sleep 0.1
  i=$((i+1))
done
exit 0
"#,
            eof = eof_file.display(),
            early = early_file.display(),
        );

        let code = run_pipe_owning_monitor(
            root,
            phase,
            root,
            &prompt,
            Duration::from_secs(20),
            "sh",
            &["-c".to_string(), script],
            &[],
        )
        .expect("pipe-owning monitor should supervise the stub to completion");

        let stderr = std::fs::read_to_string(crate::agent_result::stderr_path(root, phase))
            .unwrap_or_default();
        assert_eq!(
            code, 0,
            "stub exited {code}; 91 = no initial turn arrived on stdin, \
             92 = the turn arrived but did not carry the prompt (a JSON \
             escaping regression tears it across lines). stderr: {stderr:?}"
        );

        assert!(
            !early_file.exists(),
            "the monitor closed the child's stdin BEFORE the close rule was \
             satisfied — the drain had landed but no DEVFLOW_RESULT marker had. \
             Constraint 4's AND cannot be honoured once stdin is gone: a \
             task-notification turn would have nowhere to be delivered."
        );
        assert!(
            eof_file.exists(),
            "the monitor never closed the child's stdin at all; the close rule \
             should have fired once the marker arrived with the task list drained"
        );

        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        for expected in [
            r#""subtype":"init""#,
            r#""task_id":"t1""#,
            r#""tasks":[]"#,
            r#""type":"result""#,
        ] {
            assert!(
                capture.contains(expected),
                "capture is missing {expected}; got:\n{capture}"
            );
        }
        assert!(
            crate::agent_result::capture_is_claude_stream(&capture),
            "the capture must classify as a Claude stream-json document — \
             this is what makes 30b's stream parser reachable at all:\n{capture}"
        );

        let result = crate::agent_result::evaluate_layer1(root, phase)
            .expect("Layer 1 must decide this capture");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::Success,
            "Layer 1 verdict from the stream capture: {result:?}"
        );

        let exit = std::fs::read_to_string(crate::agent_result::exit_code_path(root, phase))
            .expect("the monitor must record the child's exit code");
        assert_eq!(exit.trim(), "0", "exit file contents: {exit:?}");
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

        let monitor_pid = spawn_monitor(&state, "sh", &args, &[], MonitorLaunch::Legacy).unwrap();
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

        let monitor_pid = spawn_monitor(&state, "sh", &args, &[], MonitorLaunch::Legacy).unwrap();
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

        let monitor_pid = spawn_monitor(&state, "sh", &args, &[], MonitorLaunch::Legacy).unwrap();
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

            spawn_monitor(&state, "sh", &args, &[], MonitorLaunch::Legacy).unwrap();
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

        spawn_monitor(&state, "sh", &args, &[], MonitorLaunch::Legacy).unwrap();
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
