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

use crate::agent_result::{IdleTimeoutCommit, IdleTimeoutRecord};
use crate::git::hermetic_command;
use crate::phase_id::PhaseId;
use crate::state::{AgentKind, State};
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
/// Raised 30s -> 120s on 2026-08-03 by direct measurement; see
/// [`IDLE_TIMEOUT_FLOOR_SECS`] for the trials and the reasoning. The previous
/// value's "~4.2x margin" was computed against a workload that never entered a
/// long foreground tool call, and did not transfer to one.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 120;

/// The floor an idle timeout can never be configured below (D-02/D-04, 31-02).
///
/// **Raised 30s -> 120s on 2026-08-03, and the reasoning that set 30s was
/// wrong — read this before touching it again.**
///
/// The original ≥30s floor cited "~4.2x margin" against an every-line signal
/// whose observed max was 7.09s. Both numbers were real; the inference was not.
/// Phase 30d measured *backgrounded* 10s/22s sleeps, where the agent is never
/// sitting inside a long foreground tool call. Under one, the CLI emits
/// `tool_progress` keepalives on a **fixed 30.00s interval**, so a healthy,
/// hard-working child produces a 30.00s gap between stream lines — dead level
/// with a 30s timeout, and on the wrong side of it, since the timer starts when
/// the previous line is *processed* while the keepalive arrives 30s after it
/// was *sent*, plus pipe latency.
///
/// Measured 2026-08-03, CLI 2.1.220, five workload-controlled trials across two
/// unrelated workload types (each verified to have actually run — elapsed >=
/// the workload duration, no `tool_use_error`), plus a negative control:
///
/// | workload                  | gaps > 5s              |
/// |---------------------------|------------------------|
/// | 90s busy loop x3          | ~26.4, **30.00**, ~30.0 |
/// | `cargo test --workspace` x2 | ~26.4, **30.00**, ~16  |
/// | control (no long call)    | max 2.2                |
///
/// Variance across all five: ±0.02s. `cargo test --workspace` is not a contrived
/// case — it sits inside DevFlow's own post-merge gate, so the old floor would
/// have killed healthy Code stages on the common path.
///
/// 120s is 4x the measured cadence: it survives **three** consecutive missed
/// keepalives. That headroom is the point — the hazard is not a slightly larger
/// gap but a *dropped* keepalive, which doubles the interval outright. 90s
/// (two missed) is the lowest defensible value; do not go below it.
///
/// Do NOT lower it, and note that no configuration can. Phase 30d measured a
/// 12-second bound killing a LIVE, HEALTHY run in 2 of 7 trials.
///
/// **What the five trials do not establish:** one machine, idle, one CLI
/// version, two workload types. They show the 30.00s cadence is real and
/// reproducible; they do not prove the interval is fixed across load, hardware,
/// or CLI versions. That is precisely why this floor sits well above the
/// observed maximum rather than near it.
///
/// Because the default IS the floor, the value can only ever be raised.
pub const IDLE_TIMEOUT_FLOOR_SECS: u64 = 120;

/// How many consecutive idle windows the monitor will wait through while a
/// background task is known to be outstanding, before treating the silence as
/// a hang after all.
///
/// At the 120s default this is a 20-minute ceiling that applies ONLY when the
/// stream has told us work is in flight; a stage with no open task is still
/// judged on the first window. The bound exists because an open task is not
/// proof of progress — a wedged subagent never reports a terminal status, and
/// an unbounded wait would turn a false kill into an immortal run.
pub const MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN: u32 = 10;

/// The environment variable that raises the idle timeout above its floor.
pub const IDLE_TIMEOUT_ENV: &str = "DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS";

/// How [`parse_idle_timeout_secs`] arrived at the timeout now in force.
///
/// A distinct enum rather than the plain `clamped: bool` the plan sketched:
/// there are FOUR distinguishable resolutions, not two, and the loud operator
/// notice needs to name the value that was configured — which a bool cannot
/// carry. `ValidateOutcome` in `pipeline_outcomes.rs` makes the same argument
/// for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdleTimeoutResolution {
    /// Nothing was configured; the default — which is the floor — is in force.
    Default,
    /// A configured value at or above the floor is in force verbatim.
    Configured,
    /// A configured value BELOW the floor was raised to it (D-04).
    Clamped {
        /// What the operator asked for, for the notice to name.
        configured: u64,
    },
    /// A value was set but could not be parsed; the default is in force.
    ///
    /// Loud for the same reason the clamp is. An operator who meant `600` and
    /// typed `60O` silently gets the 120s default, and a legitimately slow stage then dies
    /// on a timeout nobody chose. `parse_gate_max_unattended_age` substitutes
    /// silently in this case and is the anti-pattern here, not the precedent.
    Unparseable {
        /// The raw value, echoed back so the typo is visible.
        raw: String,
    },
}

/// A resolved idle timeout together with how it was arrived at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdleTimeoutSetting {
    /// The window that must elapse with NO line on the child's stdout.
    pub timeout: Duration,
    /// How that value was reached — observable to the caller as a VALUE, not
    /// only as a log line, so a test can assert on it directly.
    pub resolution: IdleTimeoutResolution,
}

impl IdleTimeoutSetting {
    /// Whether the floor clamp engaged.
    pub fn clamped(&self) -> bool {
        matches!(self.resolution, IdleTimeoutResolution::Clamped { .. })
    }

    /// The loud, operator-facing notice this resolution owes, if any.
    ///
    /// `None` for the two unremarkable cases. `Some` exactly when a value the
    /// operator supplied is NOT the value in force — the case that must never
    /// pass silently.
    pub fn notice(&self) -> Option<String> {
        self.notice_for(IDLE_TIMEOUT_ENV)
    }

    /// [`Self::notice`] named for the SPECIFIC environment variable that
    /// produced this resolution.
    ///
    /// The per-agent idle policy (D-08, round 3) resolves different variables
    /// per agent; the notice must name the knob the operator actually set, or
    /// a clamped/typo'd `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` would be
    /// reported as if `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` were the culprit.
    pub fn notice_for(&self, env: &str) -> Option<String> {
        match &self.resolution {
            IdleTimeoutResolution::Default | IdleTimeoutResolution::Configured => None,
            IdleTimeoutResolution::Clamped { configured } => Some(format!(
                "{env}={configured} is below the {IDLE_TIMEOUT_FLOOR_SECS}s floor \
                 and was CLAMPED; {}s is in force. A shorter window kills healthy runs: a 12s \
                 bound terminated a live, healthy run in 2 of 7 measured trials.",
                self.timeout.as_secs()
            )),
            IdleTimeoutResolution::Unparseable { raw } => Some(format!(
                "{env}={raw:?} could not be parsed as a whole number of seconds; \
                 the {}s default is in force. If you meant to RAISE the timeout, this did not \
                 do it.",
                self.timeout.as_secs()
            )),
        }
    }
}

/// Resolve a raw idle-timeout override into the value actually in force.
///
/// Pure — no environment access — so it is unit-testable directly rather than
/// by mutating process-global env. That shape is copied from
/// `devflow-cli`'s four `parse_*` timeout readers; their BEHAVIOUR is
/// deliberately not copied, because none of them clamps against a floor and
/// none logs when a fallback engages. There is no clamp-and-log precedent
/// anywhere in this workspace; this is the first (D-04).
pub fn parse_idle_timeout_secs(raw: Option<String>) -> IdleTimeoutSetting {
    let floor = Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS);

    // An unset variable and an EMPTY one are the same intent: nothing chosen.
    // Only a non-empty value that fails to parse is a typo worth shouting at.
    let Some(trimmed) = raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return IdleTimeoutSetting {
            timeout: floor,
            resolution: IdleTimeoutResolution::Default,
        };
    };

    let Ok(configured) = trimmed.parse::<u64>() else {
        return IdleTimeoutSetting {
            timeout: floor,
            resolution: IdleTimeoutResolution::Unparseable {
                raw: trimmed.to_string(),
            },
        };
    };

    if configured < IDLE_TIMEOUT_FLOOR_SECS {
        IdleTimeoutSetting {
            timeout: floor,
            resolution: IdleTimeoutResolution::Clamped { configured },
        }
    } else {
        IdleTimeoutSetting {
            timeout: Duration::from_secs(configured),
            resolution: IdleTimeoutResolution::Configured,
        }
    }
}

/// The AGENT-SPECIFIC idle-timeout resolution (round-3 D-08, B3).
///
/// The 120s floor was measured against Claude's stream cadence; applying it
/// to an unmeasured agent would be a behaviour prediction. The decision is
/// therefore per-agent and explicit, never a silent inheritance:
///
/// - Claude reads `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` — byte-identical to the
///   pre-phase-41 behaviour this replaces.
/// - Antigravity reads `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` with the same
///   120s floor as a DECIDED starting point (documented in the variable's
///   OPERATIONS.md row), to be revisited after the first real cadence
///   measurement.
///
/// Both literals are spelled out here so `doc_check` keeps every variable
/// visible to the operator-doc parity gate (same rule as the single-variable
/// wrapper above).
pub fn idle_timeout_setting_for(agent: AgentKind) -> IdleTimeoutSetting {
    let raw = match agent {
        AgentKind::Antigravity => std::env::var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS").ok(),
        _ => std::env::var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS").ok(),
    };
    parse_idle_timeout_secs(raw)
}

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
        // sufficient because the adapters routed through this arm — Claude
        // and Antigravity (round-3) — declare no extra env at all
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

        // D-04: resolve and clamp the idle timeout HERE, in the parent, and
        // hand the monitor the already-resolved integer.
        //
        // The placement is the whole point. `spawn_monitor` runs inside
        // `devflow start`, attached to the operator's terminal; the monitor is
        // a detached process whose stdio is all `Stdio::null()`, so a warning
        // logged there scrolls into nothing. A silent clamp is the exact
        // failure class this project keeps paying for, so the notice goes to
        // BOTH `tracing::warn!` and stdout — the log for the record, stdout
        // for the human who is watching right now.
        let idle = idle_timeout_setting_for(state.agent);
        // The notice names the variable the operator actually set (D-08): the
        // literal is deliberately spelled here so `doc_check` keeps BOTH
        // variables visible to the operator-doc parity gate.
        let idle_env = match state.agent {
            AgentKind::Antigravity => "DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS",
            _ => IDLE_TIMEOUT_ENV,
        };
        if let Some(notice) = idle.notice_for(idle_env) {
            warn!("{notice}");
            println!("{notice}");
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
            .arg(idle.timeout.as_secs().to_string())
            .arg("--agent")
            .arg(state.agent.to_string())
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

/// Constraint 4's close rule as a pure, line-fed state machine: stdin may be
/// released only once a `DEVFLOW_RESULT` marker has appeared inside a
/// TOP-LEVEL `result` event **and** the background-task list has drained.
///
/// An `AND` of two arms, neither sufficient alone:
///
/// - **Marker arm.** Satisfied only by
///   [`crate::agent_result::event_is_top_level_result_marker`] — a composition
///   of the existing `is_top_level` predicate and the existing marker parser,
///   never a looser text search. The CLI echoes the operator's prompt back
///   into the same stdout, and DevFlow's own stage prompts discuss
///   `DEVFLOW_RESULT` markers at length, so marker text alone is not evidence
///   (T-31-01; the same echo produced the checkpoint false positive 30-05
///   fixed).
/// - **Drain arm.** Satisfied when no `background_tasks_changed` event has
///   ever announced anything (vacuous — the common single-plan case) or when
///   the most recent one carried an empty list.
///
/// **The drain alone is never a stop signal.** 30c/30d measured the
/// drain-to-final-`result` lag at 4.54–11.51s across 14 trials; closing at the
/// drain would have truncated the final orchestrator turn in all seven 30d
/// trials.
///
/// **Never count `result` events.** Constraint 7: the CLI coalesces
/// completions, so a wave whose children finish together produces one `result`
/// for several of them — a shape superficially indistinguishable from "one
/// child delivered, one lost". The drained list is the only thing separating
/// those two. Per 30-04 the drain arm is *defensive rather than load-bearing*
/// (n=2 Mode B trials delivered everything without it); that is the recorded
/// reason to keep it cheaply, not a reason to drop it.
///
/// **A line that does not parse as JSON is ignored by this rule** — it can
/// neither satisfy nor block either arm — but it is still teed verbatim to the
/// capture file by the reader thread. A torn line therefore cannot silently
/// decide anything, and cannot be silently lost either.
/// The three states a `background_tasks_changed` announcement can leave the
/// close rule in. Kept as a named enum rather than `Option<usize>` (999.75 /
/// DEN-96, fixed 2026-08-04): a plain `Option` cannot distinguish "no
/// announcement has ever arrived" from "an announcement arrived but its
/// `tasks` field was not a readable array", and both used to collapse onto
/// `None`. `should_close()` treats `None` as permission to close — so an
/// unparseable *first* announcement closed stdin exactly when a background
/// task was actually pending, which is the 999.64 orphan shape reachable
/// through the guard built to prevent it.
#[derive(Default, PartialEq, Eq, Debug, Clone, Copy)]
enum BackgroundTaskState {
    /// No `background_tasks_changed` event has been observed at all. Vacuously
    /// drained: a stage that never backgrounds anything must still be able to
    /// close on its marker, or every non-backgrounding stage would hang for
    /// the full idle timeout.
    #[default]
    NeverAnnounced,
    /// The last announcement carried a readable `tasks` array of this length.
    /// `Pending(0)` is a real drain; `Pending(n>0)` blocks closing.
    Pending(usize),
    /// An announcement arrived — `type: "system"`,
    /// `subtype: "background_tasks_changed"` — but its `tasks` field was not a
    /// readable JSON array. Distinct from `NeverAnnounced` specifically so it
    /// does NOT satisfy `should_close()`: the CLI said tasks might exist and
    /// this rule could not read the count, so the safe assumption is that
    /// something is still pending, not that nothing ever was.
    Unreadable,
}

/// The task statuses that end a background task's life.
///
/// Deliberately an allow-list of TERMINAL states rather than a deny-list of
/// active ones. An unrecognised status leaves the task OPEN, which blocks
/// closing and extends the idle window — the same conservative direction
/// [`BackgroundTaskState::Unreadable`] already takes. Being wrong here delays
/// a stage; being wrong the other way orphans its work (999.64).
const TERMINAL_TASK_STATUSES: &[&str] = &[
    "completed",
    "killed",
    "failed",
    "stopped",
    "cancelled",
    "canceled",
    "error",
];

pub struct CloseRule {
    marker_seen: bool,
    background_tasks: BackgroundTaskState,
    /// Task ids announced via the per-task event vocabulary and not yet seen
    /// to reach a terminal status.
    ///
    /// **Why this exists alongside [`Self::background_tasks`].** The drain arm
    /// above reads `background_tasks_changed`, and production does not emit
    /// that event *for sub-agent dispatch*. Measured on a real Phase 35.1 Plan
    /// capture (2026-08-08) — a sub-agent (researcher) dispatch — `task_started`
    /// ×1, `task_progress` ×61, `task_notification` ×1, `task_updated` ×1, and
    /// `background_tasks_changed` ×0. Every occurrence of `background_tasks_changed`
    /// in this repository's *source* is a test fixture synthesising it, which is
    /// why the blindness never surfaced in the suite. This is 999.83's "the
    /// fixture's shape doesn't match what production actually emits", with the
    /// capture to prove it.
    ///
    /// Phase 35.3's measurement (HARDEN-06, 2026-08-12, CLI 2.1.228) later refined
    /// that: the current CLI DOES emit `background_tasks_changed` — but only for
    /// backgrounded shells, with `task_type: "local_bash"` — while sub-agent
    /// dispatch emits only the per-task vocabulary. So the drain arm is not dead;
    /// it is scoped to the backgrounded-shell path, and `open_tasks` is what
    /// covers the sub-agent path.
    ///
    /// The two signals are ANDed, never substituted: whichever one says work
    /// is pending wins.
    open_tasks: std::collections::HashSet<String>,
    /// Which event shape carries the `DEVFLOW_RESULT` marker this rule closes
    /// on — the AGENT-AWARE half of the close rule (round-3 B1).
    ///
    /// Claude emits `type: "result"` with a STRING `result` field;
    /// Antigravity emits `event: "result"` with `result.response`. The
    /// predicate is selected at construction via [`CloseRule::for_agent`];
    /// [`Default`] keeps the Claude predicate so every pre-existing
    /// construction site is unchanged.
    marker_predicate: fn(&serde_json::Value) -> bool,
}

impl Default for CloseRule {
    fn default() -> Self {
        Self::for_agent(AgentKind::Claude)
    }
}

impl CloseRule {
    /// The close rule for a specific agent.
    ///
    /// Selects the marker predicate by agent: Claude keeps
    /// [`crate::agent_result::event_is_top_level_result_marker`]; Antigravity
    /// uses [`crate::agent_result::event_is_top_level_antigravity_result_marker`]
    /// (round-3 B1). Without the agent-aware predicate, an Antigravity stream's
    /// `event: "result"` object never sets `marker_seen`, stdin is never
    /// released, and every real stage idle-times-out before its capture is read.
    ///
    /// **Vacuously-satisfied drain arms (Antigravity, B1).** The
    /// `background_tasks` / `open_tasks` arms read `type: "system"` subtypes
    /// (`background_tasks_changed`, `task_started`, `task_notification`, ...)
    /// that the Antigravity CLI never emits — its event-key schema has no
    /// `type` field at all. An Antigravity rule therefore stays at
    /// `NeverAnnounced` / empty forever and [`CloseRule::should_close`] reduces
    /// to the marker predicate. This is STATED, not silently inherited: it is a
    /// documented property of the Antigravity transport, asserted by the
    /// close-rule tests (close_rule_antigravity_*).
    pub fn for_agent(agent: AgentKind) -> Self {
        let marker_predicate = match agent {
            AgentKind::Antigravity => {
                crate::agent_result::event_is_top_level_antigravity_result_marker
            }
            _ => crate::agent_result::event_is_top_level_result_marker,
        };
        Self {
            marker_seen: false,
            background_tasks: BackgroundTaskState::NeverAnnounced,
            open_tasks: std::collections::HashSet::new(),
            marker_predicate,
        }
    }

    /// Fold one raw stdout line into the rule.
    pub fn observe(&mut self, line: &str) {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        if (self.marker_predicate)(&event) {
            self.marker_seen = true;
        }
        if event.get("type").and_then(serde_json::Value::as_str) == Some("system")
            && event.get("subtype").and_then(serde_json::Value::as_str)
                == Some("background_tasks_changed")
        {
            self.background_tasks = match event.get("tasks").and_then(serde_json::Value::as_array) {
                Some(tasks) => BackgroundTaskState::Pending(tasks.len()),
                // The announcement exists but its `tasks` field could not be
                // read as an array. Distinct from "never announced" — see the
                // enum doc comment. This is the fix: previously this arm did
                // nothing, leaving `pending_background_tasks` at its prior
                // value, which on the FIRST announcement was `None` —
                // indistinguishable from vacuous drain, and so treated as
                // permission to close exactly when it should not have been.
                None => BackgroundTaskState::Unreadable,
            };
        }

        self.observe_task_event(&event);
    }

    /// Fold the per-task event vocabulary the CLI actually emits into
    /// [`Self::open_tasks`].
    ///
    /// `task_started` and `task_progress` both open a task — progress is
    /// treated as opening, not merely as a heartbeat, so a capture joined
    /// mid-flight (a resumed monitor, a rotated capture) still learns that
    /// work is outstanding instead of concluding the stage is quiet.
    fn observe_task_event(&mut self, event: &serde_json::Value) {
        if event.get("type").and_then(serde_json::Value::as_str) != Some("system") {
            return;
        }
        let Some(subtype) = event.get("subtype").and_then(serde_json::Value::as_str) else {
            return;
        };
        let Some(task_id) = event.get("task_id").and_then(serde_json::Value::as_str) else {
            return;
        };

        match subtype {
            "task_started" | "task_progress" => {
                self.open_tasks.insert(task_id.to_string());
            }
            "task_notification" | "task_updated" => {
                // `task_notification` carries `status` at the top level;
                // `task_updated` carries it inside `patch`. Read both rather
                // than assuming one shape.
                let status = event
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| {
                        event
                            .get("patch")
                            .and_then(|patch| patch.get("status"))
                            .and_then(serde_json::Value::as_str)
                    });
                match status {
                    Some(status) if TERMINAL_TASK_STATUSES.contains(&status) => {
                        self.open_tasks.remove(task_id);
                    }
                    // A status we do not recognise, or none at all, leaves the
                    // task open. See TERMINAL_TASK_STATUSES.
                    _ => {
                        self.open_tasks.insert(task_id.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    /// Whether any background task is known to be outstanding.
    ///
    /// The idle-timeout arm consults this: a parent that is correctly quiet
    /// while a subagent works is not idle, and killing it is the defect this
    /// predicate exists to prevent.
    #[must_use]
    pub fn has_open_background_tasks(&self) -> bool {
        !self.open_tasks.is_empty()
            || matches!(
                self.background_tasks,
                BackgroundTaskState::Pending(1..) | BackgroundTaskState::Unreadable
            )
    }

    /// Whether both arms hold and the child's stdin may be released.
    pub fn should_close(&self) -> bool {
        self.marker_seen
            && matches!(
                self.background_tasks,
                BackgroundTaskState::NeverAnnounced | BackgroundTaskState::Pending(0)
            )
            && self.open_tasks.is_empty()
    }
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

/// The AGENT-AWARE first-turn wire shape for a `--input-format stream-json`
/// child (round-3 D-02, antigravity notice (b)).
///
/// Antigravity's CLI rejects Claude's `{"type":"user",...}` turn — the round-2
/// live capture records `stream input message is missing the "event" field`
/// for exactly that shape — so the Antigravity turn is
/// `{"event":"user","message":{...}}` (the `event`-key schema, matching every
/// other event the CLI emits). Every other agent keeps today's
/// [`user_turn_line`] shape; the Claude path is byte-identical.
pub fn user_turn_line_for(agent: AgentKind, prompt: &str) -> String {
    match agent {
        AgentKind::Antigravity => serde_json::json!({
            "event": "user",
            "message": { "role": "user", "content": prompt },
        })
        .to_string(),
        _ => user_turn_line(prompt),
    }
}

/// Supervise a `stream-json` child, owning both of its pipes, until the close
/// rule is satisfied and the child exits. Returns the child's exit code, which
/// is also written to the phase exit file.
///
/// Agent-aware transport (round-3): the first turn is written via
/// [`user_turn_line_for`] and the close rule via [`CloseRule::for_agent`], so
/// the write/read/close triple matches whichever CLI is being supervised.
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
    phase: PhaseId,
    workdir: &Path,
    prompt: &str,
    idle_timeout: Duration,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    agent: AgentKind,
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
    let turn = user_turn_line_for(agent, prompt);
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
        // `read_until` + `from_utf8_lossy`, NOT `BufRead::lines()` (peer review
        // 2026-08-03, CRITICAL). `lines()` yields `Err(InvalidData)` on a single
        // non-UTF-8 byte, and the previous code treated any read error as EOF —
        // so one bad byte silently truncated the capture and dropped every later
        // line INCLUDING the terminal `DEVFLOW_RESULT` marker. That is precisely
        // the boundary-truncation class constraint 9 exists for, manufactured by
        // the supervisor itself rather than by a dying writer.
        //
        // Decoding is now lossy and NON-fatal: undecodable bytes become U+FFFD
        // and the line still reaches the capture and the close rule. A genuine
        // I/O error still ends the loop, because that one really is EOF.
        let mut reader_buf = BufReader::new(child_stdout);
        let mut raw = Vec::new();
        loop {
            raw.clear();
            match reader_buf.read_until(b'\n', &mut raw) {
                Ok(0) => break, // real EOF
                Ok(_) => {}
                Err(err) => {
                    warn!("stdout read error, treating as EOF: {err}");
                    break;
                }
            }
            while raw.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                raw.pop();
            }
            let line = String::from_utf8_lossy(&raw).into_owned();
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

    // Constraint 4's close rule lives in `CloseRule` so it can be unit-tested
    // by feeding it lines, with no child process per case.
    let mut rule = CloseRule::for_agent(agent);
    let mut close_signalled = false;
    let mut idle_extensions: u32 = 0;

    loop {
        match line_rx.recv_timeout(idle_timeout) {
            Ok(line) => {
                if close_signalled {
                    continue;
                }
                rule.observe(&line);
                if rule.should_close() {
                    let _ = close_tx.send(());
                    close_signalled = true;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // AFTER a deliberate close, silence is EXPECTED, not a hang
                // (peer review 2026-08-03, CRITICAL). The close rule fires only
                // once the agent has emitted its terminal marker AND background
                // tasks have drained — at which point it has said everything it
                // intends to say and is merely winding down. Firing the idle
                // timeout here wrote an authoritative `IdleTimeout` verdict OVER
                // a completed, successful stage; and because `evaluate_layer1`
                // reads that side channel FIRST, by design, so that nothing can
                // shadow a real timeout, the bogus verdict outranked the real
                // success and could not be recovered from. The mechanism that
                // protects a true timeout is what made a false one fatal.
                //
                // Break instead: the reap path below already bounds a child that
                // will not exit, via `terminate_and_verify`.
                if close_signalled {
                    info!(
                        "no output for {idle_timeout:?} after the close rule released stdin; \
                         the stage already reported — proceeding to reap, NOT recording a timeout"
                    );
                    break;
                }
                // No outer wall-clock bound exists anywhere in this loop, and
                // none may be added (D-03). `recv_timeout` measures the gap
                // since the LAST LINE, so a healthy 47-minute stage that keeps
                // emitting is never touched — every line the reader thread
                // forwards resets the window naturally, which is D-01's
                // every-line signal rather than a milestone-only one. There is
                // no single wall-clock value that is safe for both a hang and
                // a legitimately long stage, which is why constraint 5
                // rejected one.
                //
                // That reasoning assumed healthy ⇒ keeps emitting. A stage
                // that BACKGROUNDS work breaks the assumption: the parent is
                // correctly silent while a subagent runs, and the subagent's
                // `task_progress` heartbeat is bursty — a real Phase 35.1 Plan
                // capture showed organic gaps of 25s, 28s, 42s and 44.5s while
                // the researcher was demonstrably alive, then one gap that
                // reached the 120s floor and got the whole run killed. The
                // drain gate should have covered this and could not: it reads
                // `background_tasks_changed`, which production never emits.
                //
                // So consult the task state before firing. While work is known
                // to be outstanding, silence is expected and this arm extends
                // instead of killing. The extension is BOUNDED — a subagent
                // that wedges leaves its task open forever, so an unbounded
                // wait would trade a false kill for an immortal run, which is
                // the failure this project already knows by name.
                if rule.has_open_background_tasks()
                    && idle_extensions < MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN
                {
                    idle_extensions += 1;
                    info!(
                        "no output for {idle_timeout:?}, but background work is still \
                         outstanding — extending ({idle_extensions}/\
                         {MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN}) instead of recording a timeout"
                    );
                    continue;
                }
                if rule.has_open_background_tasks() {
                    warn!(
                        "background work still outstanding after \
                         {MAX_IDLE_EXTENSIONS_WITH_TASKS_OPEN} idle extensions \
                         ({idle_timeout:?} each) — treating as a hang, not as progress"
                    );
                }
                fire_idle_timeout(project_root, phase, workdir, child_pid, idle_timeout);
                break;
            }
        }
    }

    // Guarantee stdin is released before waiting. A child still holding an
    // open stdin may never exit, and `child.wait()` would then block forever.
    drop(close_tx);

    let status = child.wait()?;
    // A signal-killed child has NO exit code — `status.code()` is `None`, and
    // the previous `unwrap_or(-1)` threw the signal away (peer review
    // 2026-08-03, found independently by both reviewers and by the 31-04 plan
    // review as W1). That silently defeated the classification 31-04 took care
    // to preserve: `evaluate_layer2` and
    // `reconcile_stream_success_against_exit_code` map **137** to
    // `ResourceKilled` (routed to `GateInfra` — an infrastructure fault) and
    // **127** to `AgentUnavailable`. Recording `-1` matched neither, so a real
    // OOM kill arrived as a generic `Failed` and routed to `GateReview`, asking
    // an operator to code-review a stage that was killed by the kernel.
    //
    // `128 + signal` is the shell convention those constants already encode:
    // SIGKILL(9) -> 137, SIGTERM(15) -> 143. `-1` is now reachable only when a
    // status is neither exited nor signalled, which POSIX does not define.
    let code = status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map_or(-1, |signal| 128 + signal)
    });
    std::fs::write(&exit_file, format!("{code}\n"))?;

    let _ = writer.join();
    let _ = reader.join();

    info!("supervised child {child_pid} exited with code {code}");
    Ok(code)
}

/// The idle-timeout firing sequence, in the ONE order it may run (D-05).
///
/// 1. Enumerate the commits the agent made.
/// 2. Write the authoritative verdict to its side-channel file, and fsync it.
/// 3. **Only then** terminate the child.
/// 4. Append a loud entry to the monitor's own log.
///
/// Step 3 must not precede step 2, and reversing them is not a stylistic
/// choice. Between "the child is dead" and "an authoritative result exists"
/// there is a window in which the verdict cascade sees a dead process, no
/// Layer-1 answer, and some commits on the branch — and Layer 2 scores exactly
/// that as `Success`. That is 999.64 reborn inside its own fix. A bare kill
/// with no record is the other half of the same failure: exit code 137 reads
/// as `ResourceKilled`, blaming an OOM that never happened.
///
/// **Nothing here rolls back, resets, or reverts a commit** (D-07, T-31-09).
/// The commit log is READ and never written. A timeout can be a false
/// positive, and destroying real work on a false positive is unrecoverable —
/// this repo treats irreversible operations as needing review, not tests.
///
/// Scoped to the `PipeOwning` arm alone: `Legacy` keeps today's behaviour, and
/// Codex/OpenCode/Pi keep theirs. The 120-second floor was measured against
/// Claude's stream cadence (a fixed 30.00s `tool_progress` keepalive). The
/// per-agent resolution is explicit, not inherited (round-3 D-08):
/// [`idle_timeout_setting_for`] reads `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` for
/// Claude unchanged and `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` for
/// Antigravity with the same floor as a DECIDED starting point — revisited
/// after the first real cadence measurement, never a silent application of an
/// unmeasured policy (the thing constraint 1 forbids).
///
/// Every step is best-effort and none can abort the sequence. A failure to
/// enumerate, write, or log must still leave the child terminated and the
/// stage machine advancing to a never-silent gate; the operator loses detail,
/// never the verdict.
fn fire_idle_timeout(
    project_root: &Path,
    phase: PhaseId,
    workdir: &Path,
    child_pid: u32,
    idle: Duration,
) {
    let idle_secs = idle.as_secs();
    warn!("idle timeout: no output from the supervised child for {idle_secs}s");

    // 0. Ask WHY the stream went quiet before recording a verdict about the
    //    silence. A quota denial silences the agent — it has nothing left to
    //    say — and the capture already carries the answer in a form
    //    `detect_claude_stream_rate_limit` classifies as `RateLimited`, which
    //    `outcome_policy` routes to auto-resume. Writing an idle-timeout record
    //    here would bury that: `parse_idle_timeout_side_channel` is
    //    `evaluate_layer1`'s first statement and returns unconditionally
    //    (T-31-06), so the record outranks the better classification sitting in
    //    the same capture, and a resumable pause is reported as "TERMINAL and
    //    not retried automatically".
    //
    //    Observed 2026-08-08 on a real Code stage that hit a `seven_day`
    //    `out_of_credits` denial: the operator was told the stream had been
    //    silent for 120s. Running out of quota is the likeliest way a long
    //    unattended run stops.
    //
    //    The child is still terminated below — it is wedged either way. Only
    //    the VERDICT changes, and it changes by omission: with no record
    //    written, the cascade reaches the rate-limit classifier and returns the
    //    right answer. Never silent — this is logged loudly and lands in the
    //    monitor log alongside the kill.
    if crate::agent_result::capture_shows_rate_limit_denial(project_root, phase) {
        warn!(
            "idle timeout after {idle_secs}s, but the capture carries an explicit quota \
             denial — NOT recording an idle-timeout verdict, so the rate-limit \
             classifier decides and the run stays resumable"
        );
        append_monitor_log(
            project_root,
            phase,
            &format!(
                "[idle-timeout] suppressed after {idle_secs}s: capture carries a quota \
                 denial; classified as rate-limited (resumable), not as a hang"
            ),
        );
        terminate_child_group(child_pid);
        return;
    }

    // 1. Enumerate. A failure degrades to an empty list plus a note; it never
    //    aborts, because a missing commit list must not cost the verdict.
    let (commits, enumeration_note) = enumerate_phase_commits(workdir, phase);

    // 2. Write, flush, fsync. This completing is the ONLY thing that stops
    //    Layer 2 from later scoring partial commits as Success.
    let write_error =
        write_idle_timeout_record(project_root, phase, idle_secs, child_pid, &commits)
            .err()
            .map(|err| err.to_string());
    if let Some(err) = &write_error {
        warn!("idle timeout: could not persist the verdict: {err}");
    }

    // 3. Only now is it safe to kill.
    let terminated = terminate_child_group(child_pid);

    // 4. Loud, durable, and readable after the fact.
    let named: Vec<String> = commits
        .iter()
        .map(|commit| {
            let short: String = commit.sha.chars().take(7).collect();
            format!("{short} {}", commit.subject)
        })
        .collect();
    let mut entry = format!(
        "[idle-timeout] no output for {idle_secs}s; terminated agent pid {child_pid} \
         (verified dead: {terminated}). {} commit(s) on the phase branch, NONE rolled back{}{}",
        named.len(),
        if named.is_empty() {
            String::new()
        } else {
            format!(": {}", named.join("; "))
        },
        enumeration_note
            .map(|note| format!(" [commit enumeration degraded: {note}]"))
            .unwrap_or_default(),
    );
    if let Some(err) = write_error {
        entry.push_str(&format!(" [verdict file could not be written: {err}]"));
    }
    warn!("{entry}");
    append_monitor_log(project_root, phase, &entry);
}

/// Enumerate the commits on this phase's feature branch, as
/// `(commits, degradation note)`.
///
/// Same range construction `evaluate_layer2`'s commit COUNT uses
/// (`{develop}..{feature_prefix}phase-NN`) — the same question asked with
/// `git log` instead of `rev-list --count`, so the two can never disagree
/// about which commits are the agent's.
///
/// Never returns an error. Every failure path yields an empty list and a note
/// naming what went wrong: the operator losing the commit NAMES is bad, the
/// operator losing the VERDICT is the failure this whole plan exists to
/// prevent.
fn enumerate_phase_commits(
    workdir: &Path,
    phase: PhaseId,
) -> (Vec<IdleTimeoutCommit>, Option<String>) {
    let git_flow = crate::config::GitFlowConfig::default();
    let branch = format!("{}phase-{}", git_flow.feature_prefix, phase.padded());
    let range = format!("{}..{branch}", git_flow.develop);

    let output = match crate::git::git_command(workdir)
        .args(["log", "--format=%H %s", &range])
        .output()
    {
        Ok(output) => output,
        Err(err) => return (Vec::new(), Some(format!("git log could not run: {err}"))),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return (
            Vec::new(),
            Some(format!("git log {range} failed: {stderr}")),
        );
    }

    let commits = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // `%H %s` — a sha, one space, then the subject, which may itself
            // contain spaces. `split_once` is therefore correct and `split`
            // is not. A subject-less commit still yields an empty subject
            // rather than being dropped.
            let (sha, subject) = line.split_once(' ').unwrap_or((line, ""));
            Some(IdleTimeoutCommit {
                sha: sha.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect();

    (commits, None)
}

/// Write the idle-timeout verdict and get it onto the platter before returning.
///
/// `sync_all` is not decoration: D-05's guarantee is that the result exists
/// before anything can race it, and a buffered write that is still in the page
/// cache when the process is signalled has not achieved that.
fn write_idle_timeout_record(
    project_root: &Path,
    phase: PhaseId,
    idle_secs: u64,
    child_pid: u32,
    commits: &[IdleTimeoutCommit],
) -> std::io::Result<()> {
    let record = IdleTimeoutRecord {
        status: crate::agent_result::AgentStatus::IdleTimeout
            .as_wire_str()
            .to_string(),
        idle_secs,
        agent_pid: child_pid,
        written_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        commits: commits.to_vec(),
    };
    let json = serde_json::to_string(&record)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

    let path = crate::agent_result::idle_timeout_path(project_root, phase);
    if let Some(parent) = path.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }
    let mut file = std::fs::File::create(&path)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    file.sync_all()
}

/// Terminate the supervised child's whole process group, returning the
/// VERIFIED fact of whether the leader is dead.
///
/// Acts on `child_pid`, which came from the in-memory `Child` handle — never
/// on the on-disk pid file (T-31-07). That distinction is what makes the
/// negative-pid signal below safe at all: while this monitor still holds the
/// unwaited `Child`, the kernel cannot recycle that pid, so it cannot come to
/// mean some unrelated process between spawn and now. A pid re-read from disk
/// carries no such guarantee.
///
/// Three steps, and the middle one is borrowed whole rather than reimplemented:
///
/// 1. `SIGTERM` to the GROUP. `.process_group(0)` at spawn made the child its
///    own group leader, so its pid IS its pgid and `-pid` reaches its whole
///    subtree — the tool subprocesses a coding agent leaves behind, which a
///    leader-only signal would orphan. It cannot reach this monitor: the
///    monitor stayed in its own inherited group, which is precisely what
///    `.process_group(0)` bought (T-31-05).
/// 2. [`crate::agent::terminate_and_verify`] for the leader — reused, not
///    rewritten. It owns the `SIGTERM` → poll → `SIGKILL` → re-poll
///    escalation and returns a verified liveness fact instead of an
///    assumption. 999.44 measured 15 of 15 orphaned wrappers surviving
///    `SIGTERM`, so the escalation is not optional.
/// 3. `SIGKILL` to the group, sweeping any survivor the leader's own
///    escalation did not cover. Unconditional by design: at this point the run
///    is over, everything in the group is the agent's subtree, and a `kill` to
///    an empty group is a no-op `ESRCH`.
///
/// The `signed > 1` guard is load-bearing twice over. `kill(-1, sig)` signals
/// every process the caller may signal, and `kill(0, sig)` signals the
/// caller's own group — the two catastrophic cases `agent::terminate` already
/// documents, reachable here through the negation rather than through a
/// hostile pid file.
fn terminate_child_group(child_pid: u32) -> bool {
    let Ok(signed) = libc::pid_t::try_from(child_pid) else {
        warn!("idle timeout: child pid {child_pid} does not fit pid_t; not signalling");
        return false;
    };
    if signed <= 1 {
        warn!("idle timeout: refusing to signal group for pid {signed}");
        return false;
    }

    // SAFETY: `signed > 1`, so `-signed < -1` and the two catastrophic
    // targets (`0` = our own group, `-1` = everything) are both excluded.
    unsafe {
        libc::kill(-signed, libc::SIGTERM);
    }

    let dead = crate::agent::terminate_and_verify(
        child_pid,
        crate::agent::TERMINATE_VERIFY_WAIT,
        crate::agent::TERMINATE_VERIFY_POLL,
    );

    // SAFETY: same guard as above.
    unsafe {
        libc::kill(-signed, libc::SIGKILL);
    }

    dead
}

/// Append one line to the monitor's own log, creating it if needed.
///
/// Best-effort: the monitor's stdio is null, so this file is the only place a
/// "log loudly" obligation can actually land, but failing to write it must
/// never abort a termination sequence already in progress.
fn append_monitor_log(project_root: &Path, phase: PhaseId, entry: &str) {
    let path = crate::agent_result::monitor_log_path(project_root, phase);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{entry}");
    }
}

/// Poll for the agent PID that the monitor records, for up to ~1 second.
///
/// Returns the PID once the monitor has launched the agent, or `None` if it
/// does not appear in time (the monitor still runs; only the display PID is lost).
pub fn wait_for_agent_pid(project_root: &Path, phase: PhaseId) -> Option<u32> {
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
        let mut state = State::new(
            PhaseId::new(4),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state
    }

    // ---- close-rule fixtures ------------------------------------------
    //
    // Key names, nesting and event types are taken from the real archived
    // capture at
    // `.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl`
    // (lines 5, 8, 19, 44 and 54), not invented: `tasks` is an array of
    // objects with `task_id`/`task_type`/`description`, the drained event is
    // the same event with `tasks":[]`, and a coalesced completion carries
    // `origin.kind == "task-notification"` on an ordinary `result`. Volumes
    // and identifiers are generalized; shapes are not.
    //
    // CLI-version pin (999.83 / HARDEN-06, measured 2026-08-12 on Claude
    // 2.1.228): the Phase 30 capture above predates the current CLI. Phase 35.3's
    // drill measured that the current CLI emits `background_tasks_changed` ONLY
    // for backgrounded shells — with `task_type: "local_bash"`, not `local_agent` —
    // while sub-agent dispatch emits the per-task vocabulary (`task_started` with
    // `task_type: "local_agent"`, then `task_updated`/`task_notification`) and zero
    // `background_tasks_changed`. `bg_tasks_line` below therefore reproduces a
    // combination the current CLI never produces: `local_agent` inside
    // `background_tasks_changed` was a Phase 30 behavior. It is kept as the legacy
    // drain shape; the current per-task vocabulary is covered by the `REAL_TASK_*`
    // constants and `observe_task_event`.

    const INIT_LINE: &str = r#"{"type":"system","subtype":"init","cwd":"/tmp/work","session_id":"s-1","tools":["Task","Bash"],"uuid":"u-init"}"#;

    /// A `system`/`background_tasks_changed` event announcing `count` tasks.
    /// `count == 0` is the DRAINED shape (v3 line 44). The `local_agent`
    /// task_type is the Phase 30 shape; the current CLI uses `local_bash` here
    /// (see the CLI-version pin in the block comment above).
    fn bg_tasks_line(count: usize) -> String {
        let tasks: Vec<String> = (0..count)
            .map(|i| {
                format!(
                    r#"{{"task_id":"t{i}","task_type":"local_agent","description":"child {i}"}}"#
                )
            })
            .collect();
        format!(
            r#"{{"type":"system","subtype":"background_tasks_changed","tasks":[{}],"uuid":"u-bg{count}","session_id":"s-1"}}"#,
            tasks.join(",")
        )
    }

    /// A top-level `result` event. `marker` is the `result` field's text —
    /// the agent's own final message, where a `DEVFLOW_RESULT:` line lives.
    fn result_line(marker: &str) -> String {
        format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"num_turns":3,"stop_reason":"end_turn","session_id":"s-1","uuid":"u-res","result":"{marker}"}}"#
        )
    }

    /// The v3 line-54 shape: ONE `result` closing out work that several
    /// children contributed to, tagged with the task-notification origin.
    fn coalesced_result_line(marker: &str) -> String {
        format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"num_turns":2,"stop_reason":"end_turn","origin":{{"kind":"task-notification"}},"session_id":"s-1","uuid":"u-res-coalesced","result":"{marker}"}}"#
        )
    }

    /// Same envelope, forwarded from a subagent rather than authored by the
    /// orchestrator session.
    fn subagent_result_line(marker: &str) -> String {
        result_line(marker).replacen('{', r#"{"parent_tool_use_id":"toolu_child","#, 1)
    }

    /// A success marker as it appears INSIDE a `result` string field — the
    /// quotes are escaped because the field is itself JSON.
    const MARKER: &str = r#"All done.\nDEVFLOW_RESULT: {\"status\":\"success\",\"commits\":3}"#;
    const NO_MARKER: &str = "Acknowledged; nothing to report.";

    fn observe_all(lines: &[String]) -> CloseRule {
        let mut rule = CloseRule::default();
        for line in lines {
            rule.observe(line);
        }
        rule
    }

    /// The per-task events the CLI ACTUALLY emits, copied verbatim (minus
    /// truncated payloads) from a real Phase 35.1 Plan capture on
    /// 2026-08-08. Not synthesised.
    ///
    /// That capture contained `task_started` ×1, `task_progress` ×61,
    /// `task_notification` ×1, `task_updated` ×1 — and `background_tasks_changed`
    /// ×0. The drain arm reads only the last of those, which is why it was
    /// blind in production while every fixture-fed test passed (999.83).
    const REAL_TASK_STARTED: &str = r#"{"type":"system","subtype":"task_started","task_id":"a5c0bae42941134b0","tool_use_id":"toolu_017H7RUmWejPcfm5Dc1whhdi","description":"Research Phase 35.1","subagent_type":"gsd-phase-researcher","task_type":"local_agent"}"#;
    const REAL_TASK_PROGRESS: &str = r#"{"type":"system","subtype":"task_progress","task_id":"a5c0bae42941134b0","tool_use_id":"toolu_017H7RUmWejPcfm5Dc1whhdi","description":"Reading CONTEXT.md","subagent_type":"gsd-phase-researcher"}"#;
    const REAL_TASK_UPDATED_TERMINAL: &str = r#"{"type":"system","subtype":"task_updated","task_id":"a5c0bae42941134b0","patch":{"status":"completed","end_time":1786159637614}}"#;

    /// A marker as it appears in a top-level `result` event.
    const RESULT_WITH_MARKER: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#;

    /// The regression this whole change exists for: a backgrounding stage must
    /// be recognised as busy from the events production really sends.
    #[test]
    fn open_tasks_are_learned_from_the_events_production_actually_emits() {
        let busy = observe_all(&[
            INIT_LINE.to_string(),
            REAL_TASK_STARTED.to_string(),
            REAL_TASK_PROGRESS.to_string(),
            RESULT_WITH_MARKER.to_string(),
        ]);
        assert!(
            busy.has_open_background_tasks(),
            "a started task must read as outstanding — this is what stops the \
             idle-timeout arm from killing a healthy backgrounding stage"
        );
        assert!(
            !busy.should_close(),
            "stdin must stay open while a subagent runs, even once the marker \
             has landed (999.64 orphan shape)"
        );

        let drained = observe_all(&[
            INIT_LINE.to_string(),
            REAL_TASK_STARTED.to_string(),
            REAL_TASK_PROGRESS.to_string(),
            RESULT_WITH_MARKER.to_string(),
            REAL_TASK_UPDATED_TERMINAL.to_string(),
        ]);
        assert!(
            !drained.has_open_background_tasks(),
            "a terminal status must close the task"
        );
        assert!(
            drained.should_close(),
            "marker seen and every task drained — the stage may close"
        );
    }

    /// Negative control on the conservative direction: an unknown status must
    /// not be mistaken for completion.
    #[test]
    fn an_unrecognised_task_status_leaves_the_task_open() {
        let rule = observe_all(&[
            REAL_TASK_STARTED.to_string(),
            RESULT_WITH_MARKER.to_string(),
            r#"{"type":"system","subtype":"task_updated","task_id":"a5c0bae42941134b0","patch":{"status":"reticulating_splines"}}"#.to_string(),
        ]);
        assert!(
            rule.has_open_background_tasks(),
            "an unrecognised status must leave the task open — being wrong this \
             way delays a stage, being wrong the other way orphans its work"
        );
        assert!(!rule.should_close());
    }

    /// Negative control on the other side: the fix must not make ordinary,
    /// non-backgrounding stages hang. Every stage that never dispatches a
    /// subagent has to close exactly as before.
    #[test]
    fn a_stage_that_backgrounds_nothing_is_unaffected() {
        let rule = observe_all(&[INIT_LINE.to_string(), RESULT_WITH_MARKER.to_string()]);
        assert!(
            !rule.has_open_background_tasks(),
            "no task events means no outstanding work"
        );
        assert!(
            rule.should_close(),
            "a non-backgrounding stage must still close on its marker alone"
        );
    }

    /// Constraint 4 is an `AND`, and neither arm is sufficient alone. Both
    /// halves are asserted here because a rule that accidentally became an
    /// `OR` still passes any test that only ever feeds it both.
    #[test]
    fn close_rule_requires_both_marker_and_drained_background_tasks() {
        // Arm A: the drain lands, but no marker ever appears in a top-level
        // result. Closing here truncates the run before its verdict exists.
        // The torn line carrying marker TEXT is the negative control: a line
        // that does not parse as JSON must not be able to satisfy the marker
        // arm through the back door.
        let drained_but_unmarked = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(1),
            bg_tasks_line(0),
            r#"{"type":"result","result":"DEVFLOW_RESULT: {\"status\":\"succ"#.to_string(),
            "progress: still working".to_string(),
            result_line(NO_MARKER),
        ]);
        assert!(
            !drained_but_unmarked.should_close(),
            "the drain alone must never close stdin: 30c/30d measured the \
             drain-to-final-result lag at 4.54-11.51s across 14 trials, and \
             closing at the drain would have truncated the final orchestrator \
             turn in all seven 30d trials"
        );

        // Arm B: the marker lands while a child is still pending.
        let marked_but_pending =
            observe_all(&[INIT_LINE.to_string(), bg_tasks_line(1), result_line(MARKER)]);
        assert!(
            !marked_but_pending.should_close(),
            "a marker while a background task is still announced must not \
             close stdin — the pending child's task-notification turn would \
             have nowhere to be delivered"
        );
    }

    /// 999.75 / DEN-96, fixed 2026-08-04. The FIRST `background_tasks_changed`
    /// announcement carries an unparseable `tasks` field (`null`, not an
    /// array). Before the fix, an unreadable announcement left the field at
    /// its prior value — which on the first announcement was the same `None`
    /// used for "nothing was ever announced", so `should_close()` treated it
    /// as a vacuous drain and closed stdin with a task genuinely pending. This
    /// is the 999.64 orphan shape, reachable through the guard built to
    /// prevent it.
    #[test]
    fn unreadable_first_announcement_does_not_satisfy_the_drain_arm() {
        let unreadable_first = observe_all(&[
            INIT_LINE.to_string(),
            r#"{"type":"system","subtype":"background_tasks_changed","tasks":null}"#.to_string(),
            result_line(MARKER),
        ]);
        assert!(
            !unreadable_first.should_close(),
            "an unreadable FIRST announcement must not be indistinguishable \
             from never-announced — closing here would release stdin while \
             the CLI has said a task exists whose count could not be read"
        );

        // Negative control: the identical sequence, but with NO announcement
        // at all, must still close — this is the ordinary non-backgrounding
        // stage, and the fix must not regress it into hanging for the idle
        // timeout on every run that never backgrounds anything.
        let never_announced = observe_all(&[INIT_LINE.to_string(), result_line(MARKER)]);
        assert!(
            never_announced.should_close(),
            "a stage that never announces background tasks at all must still \
             close on its marker alone — conflating NeverAnnounced with \
             Unreadable would hang every ordinary stage for the full idle \
             timeout"
        );

        // A LATER unreadable announcement, after a real pending count was
        // already known, must also block — the fix must not accidentally
        // treat Unreadable as forgiving once real state exists.
        let unreadable_after_pending = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(1),
            r#"{"type":"system","subtype":"background_tasks_changed","tasks":"not-an-array"}"#
                .to_string(),
            result_line(MARKER),
        ]);
        assert!(
            !unreadable_after_pending.should_close(),
            "an unreadable announcement following a real pending count must \
             still block closing, not silently forget the pending task"
        );
    }

    /// The common case: a single-plan stage that never dispatches anything.
    /// The drain arm is satisfied VACUOUSLY, because nothing was ever
    /// announced — an implementation that waited for a literal empty-list
    /// event would hang every such stage until the idle timeout.
    ///
    /// The interleaved noise lines also pin the other half of the rule's
    /// tolerance: a torn JSON line and a prose line are ignored for the rule
    /// (they can neither satisfy nor block it) while still being teed to the
    /// capture by the reader thread.
    #[test]
    fn close_rule_is_vacuously_drained_when_no_background_tasks_event_appears() {
        let rule = observe_all(&[
            INIT_LINE.to_string(),
            "starting up".to_string(),
            r#"{"type":"assist"#.to_string(),
            result_line(MARKER),
        ]);
        assert!(
            rule.should_close(),
            "a stage that never announced a background task is drained by \
             definition; only the marker arm has anything to satisfy"
        );
    }

    /// Constraint 7. The CLI COALESCES completions: two children can finish
    /// into one `result` event, and two announced tasks can drain to an empty
    /// list in a single `background_tasks_changed`. Counting `result` events
    /// therefore silently undercounts any wave whose completions cluster —
    /// and that shape is superficially indistinguishable from "one child
    /// delivered, one lost". The drained list is the only thing separating
    /// them, so the rule asserts on the list state and never on a count.
    ///
    /// Per 30-04 the drain arm is DEFENSIVE rather than load-bearing: n=2
    /// Mode B trials delivered everything without it. That is the documented
    /// reason to keep it cheaply — "defensive" is not "removable".
    #[test]
    fn coalesced_completions_do_not_undercount_children() {
        let rule = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(2),
            // BOTH children drain in ONE event...
            bg_tasks_line(0),
            // ...and complete into ONE result.
            coalesced_result_line(MARKER),
        ]);
        assert!(
            rule.should_close(),
            "two announced children, one drain event and one coalesced result \
             must still close — a rule that matched result events against \
             child count would stall here forever"
        );

        // Negative control: the SAME single coalesced result with the drain
        // withheld must NOT close. Without this, the assertion above is also
        // satisfied by a rule that simply closes on any result event, and the
        // test would be measuring nothing.
        let undrained = observe_all(&[
            INIT_LINE.to_string(),
            bg_tasks_line(2),
            coalesced_result_line(MARKER),
        ]);
        assert!(
            !undrained.should_close(),
            "control: it is the drained list that decides, not the arrival of \
             a result event"
        );
    }

    /// T-31-01. The CLI echoes the operator's prompt back into the same
    /// stdout, and DevFlow's own stage prompts discuss `DEVFLOW_RESULT`
    /// markers at length — so marker TEXT is not evidence of a verdict. Only
    /// a marker inside an event that is both `type: "result"` and top-level
    /// counts, reusing the one provenance predicate rather than inventing a
    /// second notion of trustworthiness.
    #[test]
    fn marker_inside_a_non_top_level_result_does_not_satisfy_the_close_rule() {
        let subagent = observe_all(&[INIT_LINE.to_string(), subagent_result_line(MARKER)]);
        assert!(
            !subagent.should_close(),
            "a subagent-origin result carrying a marker must not close the \
             stream — same provenance hole constraint 9 item 2 closed for the \
             stage verdict"
        );

        // Control: the identical envelope WITHOUT the planted parent id is
        // top-level and legitimately closes. Without this the assertion above
        // would also pass against a rule that never closes at all.
        let top_level = observe_all(&[INIT_LINE.to_string(), result_line(MARKER)]);
        assert!(
            top_level.should_close(),
            "control: the same event without a parent id is authoritative"
        );
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
        let phase = PhaseId::new(4);
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
            AgentKind::Claude,
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

    /// Build a stub that goes silent for longer than the idle window, with or
    /// without first announcing a background task.
    ///
    /// The two arms differ in exactly one line. That is the point: it is the
    /// announcement, and nothing else, that decides whether the silence is
    /// read as work or as a hang.
    fn silent_stub(announce_task: bool) -> String {
        let announce = if announce_task {
            format!("printf '%s\\n' '{REAL_TASK_STARTED}'")
        } else {
            String::from(": # no background task announced")
        };
        format!(
            r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{INIT_LINE}'
{announce}
sleep 3
printf '%s\n' '{REAL_TASK_UPDATED_TERMINAL}'
printf '%s\n' '{RESULT_WITH_MARKER}'
exit 0
"#
        )
    }

    /// The defect this change fixes, end to end: a parent that is correctly
    /// quiet while a subagent works must not be killed.
    ///
    /// Measured shape it reproduces — a real Phase 35.1 Plan run went silent
    /// for exactly the 120s window while `gsd-phase-researcher` was live, and
    /// DevFlow killed it. The CLI then reported the task as `killed` and the
    /// tool use as "user rejected", which reads like an external failure and
    /// is in fact our own signal coming back at us.
    #[test]
    fn idle_timeout_does_not_fire_while_a_background_task_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(51);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let code = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), silent_stub(true)],
            &[],
            AgentKind::Claude,
        )
        .expect("the monitor must supervise a backgrounding stub to completion");

        assert_eq!(code, 0, "stub should exit cleanly, not be killed");
        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "an idle-timeout verdict was recorded for a stage whose subagent was \
             demonstrably alive — this is the false kill the extension exists to \
             prevent, and because evaluate_layer1 reads that side channel FIRST \
             the bogus verdict would outrank the stage's real success"
        );
    }

    /// A quota denial must not be recorded as a hang.
    ///
    /// End-to-end counterpart of `capture_shows_rate_limit_denial`'s unit
    /// tests: the stub announces an explicit `rejected` denial and then goes
    /// silent, exactly as a real agent does when it runs out of credits. No
    /// idle-timeout verdict may be written, because that record would outrank
    /// the rate-limit classifier and turn a resumable pause into "TERMINAL and
    /// not retried automatically".
    ///
    /// Note the stub has NO open background task — this is the arm the
    /// drain-gate extension does not cover, and the arm a real quota denial
    /// lands in.
    #[test]
    fn a_quota_denial_is_not_recorded_as_an_idle_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(53);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let denial = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1786222800,"rateLimitType":"seven_day","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false},"uuid":"u-rl","session_id":"s-rl"}"#;
        let script = format!(
            r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{INIT_LINE}'
printf '%s\n' '{denial}'
sleep 3
exit 0
"#
        );

        let _ = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Claude,
        );

        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "a quota denial was recorded as an idle timeout — the operator is told the \
             stream went silent when the truth is 'out of credits', and the run is \
             marked terminal instead of resumable"
        );
    }

    /// Negative control for the test above. Same stub, same silence, one line
    /// removed — the guard must still fire when nothing is outstanding, or it
    /// has simply been disabled rather than made accurate.
    #[test]
    fn idle_timeout_still_fires_when_no_background_task_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(52);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let _ = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(1),
            "sh",
            &["-c".to_string(), silent_stub(false)],
            &[],
            AgentKind::Claude,
        );

        assert!(
            crate::agent_result::idle_timeout_path(root, phase).exists(),
            "silence with no outstanding work must still be recorded as a hang; \
             if this never fires, the idle guard has been removed, not fixed"
        );
    }

    /// Peer review 2026-08-03, CRITICAL: `BufRead::lines()` yields
    /// `Err(InvalidData)` on one non-UTF-8 byte, and the reader treated any read
    /// error as EOF — silently truncating the capture and dropping every later
    /// line, INCLUDING the terminal marker. The supervisor manufactured exactly
    /// the boundary-truncation failure constraint 9 exists to defend against.
    ///
    /// **What this does NOT establish:** that the real `claude` CLI ever emits
    /// non-UTF-8 on this stream. It emits JSON, which should be valid UTF-8. This
    /// pins the supervisor's robustness, not a demonstrated CLI behaviour.
    #[test]
    fn non_utf8_byte_does_not_truncate_the_capture() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(11);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // A raw 0xFF is invalid UTF-8 in any position. It sits BETWEEN two good
        // lines, so a reader that dies on it loses the marker that follows.
        let script = r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"utf8-1"}'
printf 'raw-\377-bytes\n'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"utf8-1","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}'
exit 0
"#;

        let code = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(20),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("the monitor must survive a non-UTF-8 byte on the child's stdout");
        assert_eq!(code, 0, "stub should exit cleanly");

        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        assert!(
            capture.contains(r#""type":"result""#),
            "the terminal result event was lost: a non-UTF-8 byte earlier in the \
             stream truncated the capture. This is the regression:\n{capture}"
        );
        assert!(
            capture.contains("raw-"),
            "the undecodable line itself must still be teed (lossily), since the \
             capture is the verbatim record:\n{capture}"
        );
        let result = crate::agent_result::evaluate_layer1(root, phase)
            .expect("Layer 1 must still decide a capture that contained a bad byte");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::Success,
            "verdict after lossy decode: {result:?}"
        );
    }

    /// Peer review 2026-08-03, CRITICAL: after the close rule released stdin the
    /// supervisor kept timing out on silence and fired `fire_idle_timeout`,
    /// writing an authoritative `IdleTimeout` verdict OVER a stage that had
    /// already reported success. `evaluate_layer1` reads that side channel first
    /// — by design, so nothing can shadow a real timeout — so the bogus verdict
    /// won and was unrecoverable.
    ///
    /// The timeout here (600ms) is injected short deliberately; the child sleeps
    /// well past it AFTER the marker. **What this does NOT establish:** that the
    /// 120s production floor is right — that rests on the keepalive measurement
    /// in `31-IDLE-GAP-MEASUREMENTS.md`, not on this test.
    #[test]
    fn no_idle_timeout_is_recorded_when_the_child_is_merely_slow_to_exit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(12);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let script = r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"slow-1"}'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"slow-1","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}'
# Everything has been said; the close rule fires here. Now wind down slowly,
# well past the injected idle window, emitting nothing.
sleep 3
exit 0
"#;

        let code = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_millis(600),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("a slow-exiting child that already reported is not a failure");

        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "an idle-timeout verdict was written for a stage that had ALREADY \
             emitted its terminal marker and drained its tasks — silence after a \
             deliberate close is expected, not a hang"
        );
        assert_eq!(code, 0, "the child exited cleanly, if slowly");

        let result = crate::agent_result::evaluate_layer1(root, phase)
            .expect("Layer 1 must decide this capture");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::Success,
            "a completed stage must not be reported as a timeout: {result:?}"
        );
    }

    /// Peer review 2026-08-03 (found independently by BOTH reviewers and by the
    /// 31-04 plan review as W1): `status.code()` is `None` for a signal-killed
    /// child, and `unwrap_or(-1)` discarded the signal. `-1` matches neither the
    /// 137 nor the 127 arm, so a kernel OOM kill arrived as a generic `Failed`
    /// and routed to `GateReview` — asking a human to code-review a stage the
    /// kernel killed — instead of `GateInfra`.
    ///
    /// This asserts on what the monitor ACTUALLY writes for a real SIGKILL. The
    /// pre-existing arbitration test hardcoded `"137\n"` into its fixture, so it
    /// passed green against this defect the entire time — which is why this test
    /// spawns a child and kills it rather than writing the file itself.
    #[test]
    fn a_signal_killed_child_records_128_plus_signal_not_minus_one() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(13);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // SIGKILL itself: no exit code exists, only a termination signal.
        let script = r#"
set -u
IFS= read -r _turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"sig-1"}'
kill -9 $$
"#;

        let code = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_secs(20),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("the monitor must reap a signal-killed child");

        assert_eq!(
            code, 137,
            "SIGKILL(9) must be recorded as 128+9=137, the value \
             `evaluate_layer2` and `reconcile_stream_success_against_exit_code` \
             map to ResourceKilled/GateInfra. -1 means the signal was discarded."
        );
        let exit = std::fs::read_to_string(crate::agent_result::exit_code_path(root, phase))
            .expect("the monitor must record the exit code");
        assert_eq!(exit.trim(), "137", "exit file contents: {exit:?}");
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
            crate::agent_result::agent_pid_path(dir.path(), PhaseId::new(4)),
            "12345\n",
        )
        .unwrap();

        assert_eq!(wait_for_agent_pid(dir.path(), PhaseId::new(4)), Some(12345));
    }

    #[test]
    fn wait_for_agent_pid_returns_none_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(wait_for_agent_pid(dir.path(), PhaseId::new(4)), None);
    }

    #[test]
    fn wait_for_agent_pid_returns_none_for_garbage_content() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            crate::agent_result::agent_pid_path(dir.path(), PhaseId::new(4)),
            "not-a-pid",
        )
        .unwrap();

        assert_eq!(wait_for_agent_pid(dir.path(), PhaseId::new(4)), None);
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

    // ---- idle timeout (31-02, D-01..D-08) --------------------------------

    /// D-04: a value below the floor is raised to it, and the fact is
    /// observable to the CALLER as a value — not only as a log line a test
    /// would have to capture stdout to see.
    #[test]
    fn idle_timeout_secs_clamps_below_floor_and_logs() {
        let setting = parse_idle_timeout_secs(Some("5".to_string()));

        assert_eq!(setting.timeout, Duration::from_secs(120));
        assert!(setting.clamped(), "the clamp must be observable as a value");
        assert_eq!(
            setting.resolution,
            IdleTimeoutResolution::Clamped { configured: 5 }
        );

        // The notice must NAME the configured value, the floor, and the value
        // actually in force — a clamp that says only "clamped" leaves the
        // operator guessing which of the three numbers won.
        let notice = setting.notice().expect("a clamp owes a loud notice");
        for fragment in ["5", "120", IDLE_TIMEOUT_ENV] {
            assert!(
                notice.contains(fragment),
                "notice must name {fragment:?}; got: {notice}"
            );
        }
    }

    /// The floor raises, it never lowers: a value above it survives verbatim
    /// and reports no clamp.
    #[test]
    fn idle_timeout_secs_accepts_values_above_floor() {
        let setting = parse_idle_timeout_secs(Some("300".to_string()));

        assert_eq!(setting.timeout, Duration::from_secs(300));
        assert!(!setting.clamped());
        assert_eq!(setting.resolution, IdleTimeoutResolution::Configured);
        assert_eq!(
            setting.notice(),
            None,
            "an honoured value is unremarkable and must not shout"
        );

        // Boundary: exactly the floor is CONFIGURED, not CLAMPED. An
        // off-by-one here would report a clamp that never happened and train
        // operators to ignore the notice.
        let exact = parse_idle_timeout_secs(Some("120".to_string()));
        assert_eq!(exact.resolution, IdleTimeoutResolution::Configured);
        assert!(!exact.clamped());
    }

    /// Absent, empty, and unparseable all resolve to the floor. The three are
    /// NOT equivalent in loudness: nothing configured is silent, a typo is not.
    #[test]
    fn idle_timeout_secs_defaults_to_the_floor() {
        let floor = Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS);

        for raw in [None, Some(String::new()), Some("   ".to_string())] {
            let setting = parse_idle_timeout_secs(raw.clone());
            assert_eq!(setting.timeout, floor, "raw {raw:?} must yield the floor");
            assert_eq!(setting.resolution, IdleTimeoutResolution::Default);
            assert_eq!(setting.notice(), None, "nothing chosen is not an error");
        }

        for raw in ["banana", "60O", "-5", "30.5"] {
            let setting = parse_idle_timeout_secs(Some(raw.to_string()));
            assert_eq!(setting.timeout, floor, "raw {raw:?} must yield the floor");
            assert_eq!(
                setting.resolution,
                IdleTimeoutResolution::Unparseable {
                    raw: raw.to_string()
                }
            );
            assert!(
                setting.notice().is_some(),
                "a typo that silently halves an intended timeout must be loud: {raw:?}"
            );
        }
    }

    /// D-01/D-03: every line resets the window, and there is no outer
    /// wall-clock bound. A child that keeps talking for FOUR times the idle
    /// timeout is never terminated.
    ///
    /// The timeout is injected short (400ms) rather than using the 120s
    /// production default — this measures the RESET MECHANISM, and does so at
    /// a scale the suite can afford. **What it does not establish:** that 120s
    /// is the right production value. That rests on the 2026-08-03 keepalive
    /// measurement recorded on [`IDLE_TIMEOUT_FLOOR_SECS`], not on this test.
    #[test]
    fn idle_timer_resets_on_every_stream_line() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(6);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // 12 lines x 100ms = 1.2s of talking against a 400ms window. Any
        // implementation that resets on milestones only, or that imposes an
        // outer bound, kills this child before it finishes.
        let script = r#"
set -u
IFS= read -r turn || exit 91
i=0
while [ $i -lt 12 ]; do
  printf '%s\n' '{"type":"system","subtype":"heartbeat","n":'"$i"'}'
  sleep 0.1
  i=$((i+1))
done
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"idle-1","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}'
exit 0
"#;

        let started = std::time::Instant::now();
        let code = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_millis(400),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("a chatty child must be supervised to completion");
        let elapsed = started.elapsed();

        assert_eq!(code, 0, "the chatty child must exit cleanly, not be killed");
        assert!(
            !crate::agent_result::idle_timeout_path(root, phase).exists(),
            "no timeout may fire while the child is still emitting lines"
        );
        assert!(
            elapsed > Duration::from_millis(400),
            "the run must outlast the idle window, else it proves nothing \
             about resetting: {elapsed:?}"
        );

        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        assert_eq!(
            capture.matches("heartbeat").count(),
            12,
            "all twelve resets must have been observed: {capture:?}"
        );
    }

    /// D-05, and the assertion the whole ordering exists for.
    ///
    /// The observation is made LIVE, by a watcher thread sampling the child's
    /// liveness at the first instant the verdict file exists — not by
    /// inspecting order after the fact, which cannot distinguish
    /// write-then-kill from kill-then-write.
    ///
    /// Its own negative control is structural: if the implementation wrote the
    /// verdict AFTER terminating, the watcher would sample a dead child and
    /// this test fails with `Some(false)`. The stub ignores `SIGTERM` so the
    /// window in which "file exists AND child alive" is observable is the full
    /// `TERMINATE_VERIFY_WAIT`, rather than a microsecond race.
    ///
    /// **What the duration of this test measures:** almost entirely
    /// `agent::TERMINATE_VERIFY_WAIT` (3s), because the stub refuses `SIGTERM`
    /// and must be escalated to `SIGKILL`. The 250ms idle window is a rounding
    /// error against it.
    #[test]
    fn idle_timeout_writes_side_channel_before_terminating_child() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let phase = PhaseId::new(7);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // One line, then silence. `trap '' TERM` widens the observation
        // window to the full SIGTERM->SIGKILL escalation.
        let script = r#"
set -u
IFS= read -r turn || exit 91
trap '' TERM
printf '%s\n' '{"type":"system","subtype":"init","session_id":"idle-2"}'
sleep 120
"#;

        let verdict = crate::agent_result::idle_timeout_path(&root, phase);
        let pid_file = crate::agent_result::agent_pid_path(&root, phase);
        let watcher = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            let mut pid: Option<u32> = None;
            while std::time::Instant::now() < deadline {
                if pid.is_none() {
                    pid = std::fs::read_to_string(&pid_file)
                        .ok()
                        .and_then(|s| s.trim().parse::<u32>().ok());
                }
                if verdict.exists() {
                    // Sample liveness at the FIRST moment the verdict exists.
                    return pid.map(crate::agent::agent_running);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            None
        });

        let code = run_pipe_owning_monitor(
            &root,
            phase,
            &root,
            "prompt",
            Duration::from_millis(250),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("a silent child must still produce a supervised outcome");

        let observed = watcher.join().expect("watcher thread panicked");
        assert_eq!(
            observed,
            Some(true),
            "the verdict must be on disk while the child is STILL ALIVE. \
             Some(false) = written after termination (the D-05 violation); \
             None = the verdict never appeared at all"
        );

        // The verdict must also be readable and correct, not merely present.
        let raw = std::fs::read_to_string(crate::agent_result::idle_timeout_path(&root, phase))
            .expect("verdict file must be readable");
        let record: IdleTimeoutRecord = serde_json::from_str(&raw).expect("verdict must parse");
        assert_eq!(record.status, "idle_timeout");
        assert_eq!(record.idle_secs, 0, "250ms truncates to 0 whole seconds");
        assert!(record.agent_pid > 1);

        // And the whole cascade must agree: Layer 1 reports the timeout.
        let result = crate::agent_result::evaluate_layer1(&root, phase)
            .expect("Layer 1 must decide a timed-out run");
        assert_eq!(
            result.status,
            crate::agent_result::AgentStatus::IdleTimeout,
            "the monitor's verdict must survive all the way to the oracle"
        );

        // The child was killed, so it has no ordinary exit code — the point is
        // that the stage machine still reaches a gate rather than hanging.
        assert!(
            crate::agent_result::exit_code_path(&root, phase).exists(),
            "the exit file must still be written so advance() is reachable"
        );
        let _ = code;

        // The loud monitor-log entry (D-04/D-07's readable-after-the-fact
        // obligation) must exist too — the monitor's stdio is null, so this
        // file is the only place it can land.
        let log = std::fs::read_to_string(crate::agent_result::monitor_log_path(&root, phase))
            .expect("the monitor must log its own timeout");
        assert!(log.contains("idle-timeout"), "log entry missing: {log:?}");
    }

    /// Minimal git repo: `develop` plus a `feature/phase-NN` branch carrying
    /// `commits` extra commits.
    fn init_repo_with_feature_commits(root: &Path, phase: PhaseId, commits: usize) {
        let git = |args: &[&str]| {
            let output = crate::git::git_command(root).args(args).output().unwrap();
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init"]);
        git(&["config", "user.email", "devflow@example.com"]);
        git(&["config", "user.name", "DevFlow Tests"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        git(&["checkout", "-b", "develop"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(&["add", "README.md"]);
        git(&["commit", "-m", "base"]);

        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        git(&["checkout", "-b", &branch]);
        for i in 0..commits {
            let name = format!("work-{i}.txt");
            std::fs::write(root.join(&name), "work\n").unwrap();
            git(&["add", &name]);
            git(&["commit", "-m", &format!("feat: agent work {i}")]);
        }
    }

    fn commit_count(root: &Path, phase: PhaseId) -> u32 {
        let range = format!("develop..feature/phase-{padded}", padded = phase.padded());
        let output = crate::git::git_command(root)
            .args(["rev-list", "--count", &range])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap()
    }

    /// D-07/T-31-09: a timeout READS the commit log and never writes to it. A
    /// timeout can be a false positive, and destroying real work on a false
    /// positive is unrecoverable.
    ///
    /// The "commits were enumerated" half is this test's negative control, and
    /// it is not optional: if enumeration silently returned nothing, "no
    /// commits were rolled back" would be trivially, vacuously true.
    #[test]
    fn idle_timeout_does_not_roll_back_commits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(8);
        init_repo_with_feature_commits(root, phase, 2);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let before = commit_count(root, phase);
        assert_eq!(before, 2, "fixture precondition");

        // No TERM trap here: the child dies promptly, keeping this test fast.
        let script = r#"
set -u
IFS= read -r turn || exit 91
printf '%s\n' '{"type":"system","subtype":"init","session_id":"idle-3"}'
sleep 120
"#;

        run_pipe_owning_monitor(
            root,
            phase,
            root,
            "prompt",
            Duration::from_millis(250),
            "sh",
            &["-c".to_string(), script.to_string()],
            &[],
            AgentKind::Claude,
        )
        .expect("a silent child must still produce a supervised outcome");

        assert_eq!(
            commit_count(root, phase),
            before,
            "an idle timeout must never roll back, reset, or revert a commit"
        );

        // NEGATIVE CONTROL: enumeration must actually have found them, else
        // the assertion above is vacuous.
        let raw = std::fs::read_to_string(crate::agent_result::idle_timeout_path(root, phase))
            .expect("verdict file must exist");
        let record: IdleTimeoutRecord = serde_json::from_str(&raw).expect("verdict must parse");
        assert_eq!(
            record.commits.len(),
            2,
            "the verdict must NAME the commits, not merely leave them alone"
        );
        for commit in &record.commits {
            assert_eq!(commit.sha.len(), 40, "full sha expected: {commit:?}");
            assert!(
                commit.subject.starts_with("feat: agent work"),
                "subject must survive enumeration: {commit:?}"
            );
        }

        // And the operator-facing reason names them.
        let result = crate::agent_result::evaluate_layer1(root, phase).unwrap();
        assert_eq!(result.commits, Some(2));
        let reason = result.reason.unwrap();
        assert!(
            reason.contains("NONE of them were rolled back"),
            "reason: {reason}"
        );
    }

    // ------------------------------------------------------------------
    // Agent-aware transport (phase 41, Task 2): user_turn_line_for,
    // agent-aware CloseRule, per-agent idle timeout. Live Antigravity shapes
    // per the round-2 review evidence (antigravity-cli 1.1.16).
    // ------------------------------------------------------------------

    const ANTG_INIT_LINE: &str = r#"{"event":"init","model":"gemini-3.7-flash-high","inputFormat":"stream-json","outputFormat":"stream-json","printTimeout":"60m"}"#;
    const ANTG_STEP_LINE: &str = r#"{"event":"step_update","index":0,"text_delta":"..."}"#;
    const ANTG_RESULT_MARKER_LINE: &str = r#"{"event":"result","result":{"status":"SUCCESS","response":"DEVFLOW_RESULT: {\"status\":\"success\"}\n"}}"#;

    #[test]
    fn user_turn_line_for_antigravity_uses_event_key() {
        let prompt = "stage prompt with \"quotes\" and\nnewlines";
        let antg = user_turn_line_for(AgentKind::Antigravity, prompt);
        let v: serde_json::Value =
            serde_json::from_str(&antg).expect("antigravity turn must be valid JSON");
        assert_eq!(
            v.get("event").and_then(serde_json::Value::as_str),
            Some("user"),
            "antigravity first turn is the event-key shape (D-02)"
        );
        assert!(
            v.get("type").is_none(),
            "no Claude type key may leak into the antigravity turn"
        );
        assert_eq!(
            v.pointer("/message/content")
                .and_then(serde_json::Value::as_str),
            Some(prompt),
            "the prompt survives escaping"
        );

        // Claude stays byte-identical to the long-standing shape.
        let claude = user_turn_line_for(AgentKind::Claude, prompt);
        assert_eq!(
            claude,
            user_turn_line(prompt),
            "Claude must be byte-identical"
        );
        let v: serde_json::Value = serde_json::from_str(&claude).unwrap();
        assert_eq!(
            v.get("type").and_then(serde_json::Value::as_str),
            Some("user")
        );
        assert!(v.get("event").is_none());
    }

    #[test]
    fn close_rule_antigravity_closes_on_event_key_marker() {
        let mut rule = CloseRule::for_agent(AgentKind::Antigravity);
        rule.observe(ANTG_INIT_LINE);
        assert!(!rule.should_close(), "init alone must not close");
        rule.observe(ANTG_STEP_LINE);
        assert!(!rule.should_close(), "progress alone must not close");
        rule.observe(ANTG_RESULT_MARKER_LINE);
        assert!(
            rule.should_close(),
            "event:result with a marker in result.response must close the antigravity stream (B1)"
        );

        // Claude-shaped lines never satisfy the antigravity rule.
        let mut rule = CloseRule::for_agent(AgentKind::Antigravity);
        rule.observe(r#"{"type":"system","subtype":"init","session_id":"s1"}"#);
        rule.observe(
            r#"{"type":"result","subtype":"success","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#,
        );
        assert!(
            !rule.should_close(),
            "a Claude stream must not satisfy the antigravity close rule"
        );

        // The Claude rule's behaviour is unchanged for Claude input (B1: the
        // Claude predicate is byte-for-byte what it always was).
        let mut claude_rule = CloseRule::for_agent(AgentKind::Claude);
        claude_rule.observe(r#"{"type":"system","subtype":"init","session_id":"s1"}"#);
        claude_rule.observe(
            r#"{"type":"result","subtype":"success","result":"DEVFLOW_RESULT: {\"status\":\"success\"}"}"#,
        );
        assert!(
            claude_rule.should_close(),
            "Claude rule closes on Claude input"
        );
    }

    /// B1: for Antigravity the `type:"system"` background-task/open-task drain
    /// arms are VACUOUSLY satisfied — the CLI never emits a `type` key at all
    /// — stated here, not silently inherited. The rule reduces to the marker
    /// predicate, and no Antigravity-shaped input can ever open a task.
    #[test]
    fn close_rule_antigravity_drain_arms_vacuously_satisfied() {
        let mut rule = CloseRule::for_agent(AgentKind::Antigravity);
        rule.observe(ANTG_INIT_LINE);
        rule.observe(ANTG_STEP_LINE);
        assert_eq!(
            rule.background_tasks,
            BackgroundTaskState::NeverAnnounced,
            "Antigravity emits no type:system background_tasks_changed events (stated, B1)"
        );
        assert!(
            rule.open_tasks.is_empty(),
            "Antigravity emits no type:system per-task events (stated, B1)"
        );
        assert!(
            !rule.has_open_background_tasks(),
            "no open tasks means the drain arms never block closing"
        );
    }

    #[test]
    fn idle_timeout_setting_for_is_agent_specific() {
        use std::sync::Mutex;
        static ENV_MUTEX: Mutex<()> = Mutex::new(());
        let _lock = ENV_MUTEX.lock().unwrap();

        // Baseline: nothing set -> both agents get the decided floor.
        unsafe {
            std::env::remove_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS");
            std::env::remove_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS");
        }
        let claude_default = idle_timeout_setting_for(AgentKind::Claude);
        let antg_default = idle_timeout_setting_for(AgentKind::Antigravity);
        assert_eq!(
            claude_default.timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS)
        );
        assert_eq!(
            antg_default.timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS),
            "the antigravity default is the DECIDED 120s floor (D-08), not inherited \
             silently — the decision is explicit and documented"
        );

        // Claude's variable moves Claude, NOT Antigravity.
        unsafe {
            std::env::set_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS", "300");
            std::env::remove_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS");
        }
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Claude).timeout,
            Duration::from_secs(300)
        );
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Antigravity).timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS),
            "Claude's variable must not move Antigravity"
        );

        // Antigravity's variable moves Antigravity, NOT Claude.
        unsafe {
            std::env::set_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS", "240");
            std::env::remove_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS");
        }
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Antigravity).timeout,
            Duration::from_secs(240)
        );
        assert_eq!(
            idle_timeout_setting_for(AgentKind::Claude).timeout,
            Duration::from_secs(IDLE_TIMEOUT_FLOOR_SECS),
            "Antigravity's variable must not move Claude"
        );

        unsafe {
            std::env::remove_var("DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS");
            std::env::remove_var("DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS");
        }
    }

    /// codex-3: the REAL PipeOwning writer path, not just the pure helper —
    /// spawn a child whose stdin is read and validated. A stub `sh` script
    /// records its first stdin line, emits the Antigravity stream (init +
    /// result with marker in `response`), then drains stdin to EOF; the
    /// monitor writes via `user_turn_line_for(Antigravity, ...)` and closes
    /// via the agent-aware rule. The recorded line must be the event-key
    /// shape — an implementation that left the writer on Claude's `type`-form
    /// FAILS here, not just in the unit helper.
    #[test]
    fn pipe_owning_writer_delivers_antigravity_event_key_turn() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let turn_file = root.join("stdin-turn.txt");
        let eof_file = root.join("stdin-eof");
        let script = format!(
            r#"
set -u
IFS= read -r turn || {{ echo "NO_INITIAL_TURN_ON_STDIN" >&2; exit 91; }}
printf '%s\n' "$turn" > '{turn}'
printf '%s\n' '{{"event":"init","model":"stub","inputFormat":"stream-json","outputFormat":"stream-json"}}'
printf '%s\n' '{{"event":"result","result":{{"status":"SUCCESS","response":"DEVFLOW_RESULT: {{\"status\":\"success\"}}"}}}}'
exec 3<&0
( cat <&3 > /dev/null; printf 'EOF\n' > '{eof}' ) > /dev/null 2>&1 &
exit 0
"#,
            turn = turn_file.display(),
            eof = eof_file.display(),
        );

        let code = run_pipe_owning_monitor(
            root,
            phase,
            root,
            "the-prompt",
            Duration::from_secs(30),
            "sh",
            &["-c".to_string(), script],
            &[],
            AgentKind::Antigravity,
        )
        .expect("pipe-owning monitor should supervise the antigravity stub");

        assert_eq!(code, 0, "stub exited {code}");
        // The close rule must have released stdin (B1): the stub's background
        // drain saw EOF. Without the agent-aware close rule this fails. The
        // drain runs in a backgrounded subshell, so poll with a bounded wait
        // (same discipline as the Phase-31 tracer test) rather than asserting
        // synchronously after the child exits.
        let mut saw_eof = false;
        for _ in 0..100 {
            if eof_file.exists() {
                saw_eof = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            saw_eof,
            "stdin was never released — the agent-aware close rule (B1) did not fire"
        );

        let turn = std::fs::read_to_string(&turn_file).unwrap();
        let v: serde_json::Value = serde_json::from_str(&turn).expect("recorded turn must be JSON");
        assert_eq!(
            v.get("event").and_then(serde_json::Value::as_str),
            Some("user"),
            "the REAL writer must deliver the event-key turn (codex-3): {turn}"
        );
        assert!(v.get("type").is_none(), "no type key: {turn}");
        assert_eq!(
            v.pointer("/message/content")
                .and_then(serde_json::Value::as_str),
            Some("the-prompt")
        );

        // The capture round-trips through the antigravity parser.
        let capture =
            std::fs::read_to_string(crate::agent_result::stdout_path(root, phase)).unwrap();
        let parsed = crate::agent_result::parse_antigravity_event_result(&capture).unwrap();
        assert_eq!(parsed.status, crate::agent_result::AgentStatus::Success);
    }
}
