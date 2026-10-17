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
use crate::stage::Stage;
use crate::state::{AgentKind, State};
use crate::supervisor::SocketPath;
use crate::supervisor::runtime::{AgentOutcome, Supervisor};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Errors produced by monitor operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
    /// The supervisor socket path was refused before anything was bound.
    #[error("supervisor socket path refused: {0}")]
    SocketPath(#[from] crate::supervisor::SocketPathError),
    /// The supervisor socket or its sidecar could not be published.
    #[error("could not publish the supervisor socket: {0}")]
    Publish(#[from] crate::supervisor::PublishError),
    /// The agent did not lead its own process group, so a group signal
    /// could reach a process that is not the agent's (R-3). The agent was
    /// killed and reaped.
    #[error("agent {pid} is not its own process-group leader (pgid {pgid})")]
    PgidMismatch {
        /// The agent's pid.
        pid: u32,
        /// The process group the agent was found in (`-1` when unreadable).
        pgid: i32,
    },
    /// A supervision shape that this build cannot run yet, or a supervisor
    /// that never published its socket (it and what it started were ended).
    #[error("supervision shape not ready: {0}")]
    NotReady(String),
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

/// Name of the environment variable that carries the `__monitor` argv (the
/// arguments after `__monitor`, as a JSON array of strings) to a test
/// binary's monitor harness entry. Set only by
/// [`MonitorExec::HarnessEntry`]; no release build reads it.
#[cfg(feature = "test-support")]
pub const MONITOR_ARGV_ENV: &str = "DEVFLOW_MONITOR_ARGV";

/// Which executable a stream launch's detached monitor re-execs (50-10).
///
/// In the `devflow` binary this is always [`MonitorExec::Subcommand`]: the
/// binary itself, run as its hidden `__monitor` subcommand. Inside a test
/// binary, `current_exe()` is libtest, which rejects `__monitor`'s arguments,
/// so a test build can instead name an `#[ignore]`d test that runs the same
/// monitor body.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorExec {
    /// `<exe> __monitor <args…>`.
    Subcommand {
        /// The `devflow` executable to re-exec.
        exe: PathBuf,
    },
    /// `<exe> <test_name> --exact --include-ignored --test-threads=1`, with
    /// the `__monitor` argv in [`MONITOR_ARGV_ENV`]. Test builds only: the
    /// variant does not exist without the `test-support` feature.
    #[cfg(feature = "test-support")]
    HarnessEntry {
        /// The test binary to re-exec.
        exe: PathBuf,
        /// The harness entry's full test name.
        test_name: String,
    },
}

impl MonitorExec {
    /// The executable this monitor runs, whichever way it is entered.
    fn exe(&self) -> &Path {
        match self {
            Self::Subcommand { exe } => exe,
            #[cfg(feature = "test-support")]
            Self::HarnessEntry { exe, .. } => exe,
        }
    }
}

/// Everything a stage launch resolves once, in one place, before it spawns a
/// monitor (50-10): the machine-global cache directory its registration (and,
/// from 50-11, its socket) lives under, and the executable its monitor runs.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchContext {
    /// The DevFlow cache directory for this launch.
    pub cache_dir: PathBuf,
    /// The monitor executable and how it is entered.
    pub exec: MonitorExec,
}

impl LaunchContext {
    /// A launch context from its two parts.
    pub fn new(cache_dir: PathBuf, exec: MonitorExec) -> Self {
        Self { cache_dir, exec }
    }
}

/// Strip `DEVFLOW_GATE_NOTIFY_CMD` from a detached monitor chain
/// (49-REVIEW.md WR-07). Every gate in the chain then resolves the command
/// from the run's state file at the moment it fires, so editing
/// `gate_notify_cmd` there changes or disables the hook for a run in flight,
/// and the command (which may carry a webhook token) never reaches the coding
/// agent's environment. Also applied wherever an agent process is spawned
/// directly (the pipe-owning agent and the delivery canary). The foreground
/// `devflow` process keeps the operator's own variable for the gates it fires.
pub(crate) fn strip_notify_env(child_cmd: &mut std::process::Command) {
    child_cmd.env_remove(crate::gates::GATE_NOTIFY_CMD_ENV);
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
/// `launch` selects the supervision shape — see [`MonitorLaunch`]. `ctx`
/// names the monitor executable: the `PipeOwning` arm re-execs it as
/// [`LaunchContext::exec`] says, and the `Legacy` arm's `advance` tail runs
/// the same executable.
///
/// Returns the PID of the spawned monitor.
pub fn spawn_monitor(
    state: &State,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    launch: MonitorLaunch,
    ctx: &LaunchContext,
) -> Result<u32, MonitorError> {
    spawn_monitor_inner(
        state,
        program,
        args,
        envs,
        launch,
        ctx,
        MonitorTail::Advance,
    )
}

/// What the Legacy monitor runs after it records the agent's exit code.
enum MonitorTail {
    /// `devflow advance` for this phase and stage — every production launch.
    Advance,
    /// Test-only stand-in for `advance` (empty for none), so a test neither
    /// re-execs the test binary as `devflow advance` nor loses the window in
    /// which the monitor outlives its agent.
    #[cfg(test)]
    Script(String),
}

fn spawn_monitor_inner(
    state: &State,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    launch: MonitorLaunch,
    ctx: &LaunchContext,
    tail: MonitorTail,
) -> Result<u32, MonitorError> {
    let project_root = state
        .project_root
        .to_str()
        .ok_or(MonitorError::NonUtf8Path)?;

    let binary = ctx
        .exec
        .exe()
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
        // `tail` is not consulted on this arm: the `__monitor`
        // subcommand always advances after reaping, and `spawn_monitor` is the
        // only caller of this function — it hardcodes `Advance`. Adding a
        // `--no-advance` flag for a case nothing exercises would be an
        // untested branch; add it when a caller actually needs it.
        let _ = tail;

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
        let mut monitor_argv = pipe_owning_monitor_args(
            project_root,
            state,
            workdir,
            prompt_file,
            idle.timeout.as_secs(),
        );
        monitor_argv.push(program.to_string());
        monitor_argv.extend(args.iter().cloned());
        let mut child_cmd = monitor_command(&ctx.exec, &binary, workdir_path, &monitor_argv)?;
        child_cmd.envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));

        strip_notify_env(&mut child_cmd);

        let child = child_cmd
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
    //
    // A single `kill` from the trap is NOT enough on its own
    // (monitor-sigterm-orphans-agent, .planning/debug, 2026-09-22). Two
    // windows lose it, and both orphaned the agent (PPid=1) in the tests
    // that hold them open:
    //
    // - Window A (dash, i.e. Debian's /bin/sh and CI's): the forked agent
    //   child keeps the monitor's *caught* TERM handler until dash's
    //   post-fork trap reset. A TERM landing first is recorded, then
    //   discarded (the trap it would run is gone), and the child execs an
    //   agent that never saw it. bash does not lose it.
    // - Window F (dash and bash): a TERM reaching the monitor after its fork
    //   but before `apid=$!` ran `cleanup` with an empty `apid`.
    //
    // So `cleanup` creates a stop marker BEFORE it kills, and the agent child
    // checks that marker right before `exec`. The check runs after the
    // child's trap reset, so a TERM it lost is always followed by a check
    // that sees the marker — no timing assumption. `echo`, not `:`, creates
    // the marker: a failed redirect on the special builtin `:` would exit the
    // shell before the `kill`. `${apid:-$!}` closes window F, since `$!` is
    // set as soon as the fork returns. The marker is removed in Rust before
    // the spawn, never by the script, so the agent launch stays the monitor's
    // first fork and a stale marker cannot stop the next agent.
    //
    // Once `wait` has reaped the agent, `cleanup` must leave it alone (WR-01,
    // 48-REVIEW.md). The trap stays armed through the advance tail, and the
    // shell defers it until that foreground command returns — after
    // `advance` has launched the next stage's monitor, which removed the
    // marker. Writing it then stops that next agent, and the `kill` targets a
    // pid reaped long ago. `reaped` gates both; clearing `apid` would not,
    // because `$!` still names the same pid. A TERM landing between `wait`
    // returning and `reaped=1` still writes the marker, but the trap then
    // exits before `advance` runs, so the next launch removes it as stale.
    let stop_path = crate::agent_result::stop_marker_path(&state.project_root, state.phase);
    if let Err(err) = std::fs::remove_file(&stop_path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        return Err(err.into());
    }
    let stop_file = stop_path.to_str().ok_or(MonitorError::NonUtf8Path)?;
    let advance_tail = match tail {
        MonitorTail::Advance => advance_tail(&binary, project_root, state),
        #[cfg(test)]
        MonitorTail::Script(script) if script.is_empty() => String::new(),
        #[cfg(test)]
        MonitorTail::Script(script) => format!("; {script}"),
    };
    let script = format!(
        "apid=''; reaped=''; cleanup() {{ if [ -z \"$reaped\" ]; then echo > {stop_file}; \
         [ -n \"${{apid:-$!}}\" ] && kill \"${{apid:-$!}}\" 2>/dev/null; fi; exit 0; }}; \
         trap cleanup TERM INT; \
         cd {workdir} || exit 1; \
         {{ [ -e {stop_file} ] && exit 143; exec \"$@\"; }} > {stdout_file} 2>{stderr_file} & \
         apid=$!; echo $apid > {pid_file}; \
         wait $apid; rc=$?; reaped=1; echo $rc > {exit_file}{advance_tail}",
        stop_file = shell_escape(stop_file),
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
    let mut child_cmd = hermetic_command("sh", workdir_path);
    child_cmd
        .arg("-c")
        .arg(&script)
        .arg("sh")
        .arg(program)
        .args(args)
        // Adapter-scoped env (e.g. Codex's unsigned-commit override) rides
        // the whole monitor chain: sh → agent → its git children (13-06).
        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    strip_notify_env(&mut child_cmd);

    let child = child_cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let pid = child.id();
    info!("monitor spawned with pid {pid}");
    Ok(pid)
}

fn pipe_owning_monitor_args(
    project_root: &str,
    state: &State,
    workdir: &str,
    prompt_file: &str,
    idle_timeout_secs: u64,
) -> Vec<String> {
    vec![
        "__monitor".to_string(),
        "--project".to_string(),
        project_root.to_string(),
        "--phase".to_string(),
        state.phase.to_string(),
        "--stage".to_string(),
        state.stage.to_string(),
        "--workdir".to_string(),
        workdir.to_string(),
        "--prompt-file".to_string(),
        prompt_file.to_string(),
        "--idle-timeout-secs".to_string(),
        idle_timeout_secs.to_string(),
        "--agent".to_string(),
        state.agent.to_string(),
        "--".to_string(),
    ]
}

/// The `PipeOwning` re-exec command for `exec`, before the adapter env is
/// applied. `monitor_argv` starts with `__monitor` and ends with the child's
/// program and argv; `binary` is `exec`'s executable as UTF-8.
fn monitor_command(
    exec: &MonitorExec,
    binary: &str,
    workdir: &Path,
    monitor_argv: &[String],
) -> Result<std::process::Command, MonitorError> {
    let mut command = hermetic_command(binary, workdir);
    match exec {
        MonitorExec::Subcommand { .. } => {
            command.args(monitor_argv);
        }
        // libtest cannot parse `__monitor`'s arguments, so they travel in the
        // environment and the test binary runs exactly the named entry.
        #[cfg(feature = "test-support")]
        MonitorExec::HarnessEntry { test_name, .. } => {
            let after_subcommand = match monitor_argv.split_first() {
                Some((first, rest)) if first == "__monitor" => rest,
                _ => monitor_argv,
            };
            let encoded = serde_json::to_string(after_subcommand).map_err(std::io::Error::other)?;
            command
                .arg(test_name)
                .args(["--exact", "--include-ignored", "--test-threads=1"])
                .env(MONITOR_ARGV_ENV, encoded);
        }
    }
    Ok(command)
}

fn advance_tail(binary: &str, project_root: &str, state: &State) -> String {
    format!(
        "; {binary} advance {project_root} --phase {phase} --stage {stage}",
        binary = shell_escape(binary),
        project_root = shell_escape(project_root),
        phase = state.phase,
        stage = state.stage,
    )
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
    let capture = std::fs::File::create(&stdout_file)?;

    let mut agent_cmd = hermetic_command(program, workdir);
    // The coding agent never needs the gate notify command (WR-07).
    strip_notify_env(&mut agent_cmd);
    let mut child = agent_cmd
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

    let threads = drive_stream_io(&mut child, capture, agent, prompt, idle_timeout, || {
        fire_idle_timeout(project_root, phase, workdir, child_pid, idle_timeout);
    })?;

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

    threads.join();

    info!("supervised child {child_pid} exited with code {code}");
    Ok(code)
}

/// The stream shape's writer and reader threads, joined once the agent has
/// been reaped.
struct StreamThreads {
    writer: std::thread::JoinHandle<()>,
    reader: std::thread::JoinHandle<()>,
}

impl StreamThreads {
    fn join(self) {
        let _ = self.writer.join();
        let _ = self.reader.join();
    }
}

/// Drive a `stream-json` child's two pipes until its stream is over, with
/// the threading model [`run_pipe_owning_monitor`]'s doc describes: a writer
/// thread that delivers the first turn and holds stdin open, a reader thread
/// that tees stdout into `capture`, and this thread applying the close rule
/// and the idle timeout.
///
/// Returns once stdout reaches EOF, the idle timeout has fired (after
/// `on_idle_timeout` ran), or the stream stayed quiet after the close rule
/// released stdin. Stdin is released before it returns, so the caller may
/// then wait for the child. Shared by [`run_pipe_owning_monitor`] and
/// [`run_supervised_stage`]'s stream shape, so the two cannot drift apart.
fn drive_stream_io(
    child: &mut std::process::Child,
    mut capture: std::fs::File,
    agent: AgentKind,
    prompt: &str,
    idle_timeout: Duration,
    on_idle_timeout: impl FnOnce(),
) -> Result<StreamThreads, MonitorError> {
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
                on_idle_timeout();
                break;
            }
        }
    }

    // Guarantee stdin is released before waiting. A child still holding an
    // open stdin may never exit, and `child.wait()` would then block forever.
    drop(close_tx);

    Ok(StreamThreads { writer, reader })
}

/// How [`run_supervised_stage`] wires the agent's stdio: the two supervision
/// shapes of one supervisor body.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StageIo {
    /// The capture shape (the `MonitorLaunch::Legacy` script's replacement):
    /// stdin is `/dev/null`, stdout and stderr go straight to the phase's
    /// capture and stderr files.
    Capture,
    /// The stream shape (`MonitorLaunch::PipeOwning`): the supervisor owns
    /// both pipes, delivers `prompt` as the first turn on stdin, tees stdout
    /// to the capture file, applies the close rule and fires the idle
    /// timeout after `idle_timeout` of silence.
    Stream {
        /// The stage prompt, delivered on the agent's stdin.
        prompt: String,
        /// How long the stream may be silent before the idle timeout fires.
        idle_timeout: Duration,
    },
}

impl StageIo {
    /// The launch label a ping reply and the sidecar carry: `capture` or
    /// `stream`.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Stream { .. } => "stream",
        }
    }
}

/// Everything [`run_supervised_stage`] needs to supervise one stage's agent.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StageSpec {
    /// The project root the capture, stderr, exit and pid files live under.
    pub project_root: PathBuf,
    /// The phase the stage runs for.
    pub phase: PhaseId,
    /// The stage the agent runs, when one is known.
    pub stage: Option<Stage>,
    /// The agent's working directory (the phase worktree, or the project
    /// root when worktree mode is off).
    pub workdir: PathBuf,
    /// The agent program.
    pub program: String,
    /// The agent's argv after the program, passed literally.
    pub args: Vec<String>,
    /// The adapter environment, applied after `hermetic_command`'s scrub so
    /// deliberate configuration survives it (process env, never argv).
    pub envs: Vec<(String, String)>,
    /// Which coding agent this is.
    pub agent: AgentKind,
    /// The supervision shape.
    pub io: StageIo,
    /// Where the supervisor publishes its socket.
    pub socket: SocketPath,
    /// The per-launch id the sidecar records.
    pub launch_id: String,
}

impl StageSpec {
    /// A spec with no stage, no extra argv and no adapter env, whose agent
    /// runs in `project_root`. Set the public fields for the rest.
    pub fn new(
        project_root: PathBuf,
        phase: PhaseId,
        agent: AgentKind,
        program: String,
        io: StageIo,
        socket: SocketPath,
        launch_id: String,
    ) -> Self {
        Self {
            workdir: project_root.clone(),
            project_root,
            phase,
            stage: None,
            program,
            args: Vec::new(),
            envs: Vec::new(),
            agent,
            io,
            socket,
            launch_id,
        }
    }
}

/// A supervised stage whose agent has exited.
#[derive(Debug)]
pub struct StageRun {
    /// How the agent's run ended. Only [`AgentOutcome::Completed`] may
    /// advance the pipeline.
    pub outcome: AgentOutcome,
    /// The still-published supervisor. The caller owns
    /// [`Supervisor::hand_off`] and [`Supervisor::finish`].
    pub supervisor: Supervisor,
}

/// How long [`confirm_own_group`] waits for a just-spawned agent's
/// `setpgid` to become visible before it declares a mismatch.
const PGID_SETTLE_WAIT: Duration = Duration::from_millis(100);

/// Supervise one stage's agent under an addressed [`Supervisor`], in either
/// supervision shape, and return once the agent has exited.
///
/// The order is the contract:
/// 1. [`install_signal_pipe`](crate::supervisor::runtime::install_signal_pipe),
///    so SIGTERM and SIGINT to this process become stop events, never a
///    death that orphans the agent (D-05, 999.134).
/// 2. Spawn the agent through `hermetic_command` (the adapter env re-applied
///    after its scrub, the gate notify command stripped) with
///    `process_group(0)`, then require that it leads its own process group
///    before anything is recorded (R-3). A mismatch kills and reaps it and
///    returns [`MonitorError::PgidMismatch`].
/// 3. Hand the agent's `Child` to an `AgentGroupGuard`, which ends the
///    agent's group on any error or panic until `serve` succeeds. Write the
///    agent pid file and the sidecar, then [`Supervisor::serve`]: the
///    sidecar exists before the socket name appears (R-7), and it is the
///    launcher's only record of the agent if this process is SIGKILLed
///    before it publishes (RV-03; `serve` rewrites it with the same content).
/// 4. The stream shape only: drive the agent's pipes through the same
///    helper `run_pipe_owning_monitor` uses (first turn, capture tee, close
///    rule, idle timeout). An idle-timeout group kill is a natural outcome,
///    `Completed(128 + n)` with the idle-timeout verdict file, never a stop.
/// 5. [`Supervisor::wait_agent`], then the exit file: the agent's code for
///    [`AgentOutcome::Completed`], `143` for [`AgentOutcome::Stopped`].
///
/// The caller owns [`Supervisor::hand_off`] and [`Supervisor::finish`], and
/// advances only on [`AgentOutcome::Completed`].
pub fn run_supervised_stage(spec: StageSpec) -> Result<StageRun, MonitorError> {
    crate::supervisor::runtime::install_signal_pipe()?;

    let stdout_file = crate::agent_result::stdout_path(&spec.project_root, spec.phase);
    let exit_file = crate::agent_result::exit_code_path(&spec.project_root, spec.phase);
    let pid_file = crate::agent_result::agent_pid_path(&spec.project_root, spec.phase);
    if let Some(parent) = stdout_file.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }

    let (mut child, stream_capture) = spawn_stage_agent(&spec)?;
    let pid = child.id();

    if let Err(pgid) = confirm_own_group(pid) {
        abandon_agent(&mut child);
        return Err(MonitorError::PgidMismatch { pid, pgid });
    }
    // Armed only now: before the pgid check the agent's pid may not number
    // its group. Until `serve` succeeds, any error or panic ends the group.
    let guard = AgentGroupGuard::arm(child);
    std::fs::write(&pid_file, format!("{pid}\n"))?;
    let sidecar = stage_sidecar(&spec, pid);
    crate::supervisor::write_sidecar(&spec.socket, &sidecar)?;
    #[cfg(test)]
    monitor_test_hold("after_sidecar");
    let supervisor = Supervisor::serve(spec.socket.clone(), sidecar)?;
    let mut child = guard.disarm();

    let threads = match (&spec.io, stream_capture) {
        (
            StageIo::Stream {
                prompt,
                idle_timeout,
            },
            Some(capture),
        ) => {
            let idle = *idle_timeout;
            let driven = drive_stream_io(&mut child, capture, spec.agent, prompt, idle, || {
                fire_idle_timeout(&spec.project_root, spec.phase, &spec.workdir, pid, idle);
            });
            match driven {
                Ok(threads) => Some(threads),
                Err(err) => {
                    abandon_agent(&mut child);
                    return Err(err);
                }
            }
        }
        _ => None,
    };

    let outcome = match supervisor.wait_agent(&mut child) {
        Ok(outcome) => outcome,
        Err(err) => {
            abandon_agent(&mut child);
            return Err(err.into());
        }
    };
    let code = match outcome {
        AgentOutcome::Completed(code) => code,
        AgentOutcome::Stopped => crate::supervisor::runtime::STOPPED_EXIT_CODE,
    };
    std::fs::write(&exit_file, format!("{code}\n"))?;
    if let Some(threads) = threads {
        threads.join();
    }
    info!(
        "supervised {} agent {pid} ended: {outcome:?}",
        spec.io.label()
    );
    Ok(StageRun {
        outcome,
        supervisor,
    })
}

/// Owns a just-spawned agent's unreaped `Child` from the moment its pgid
/// check passes until [`Supervisor::serve`] succeeds, so a pid-file write
/// error, a sidecar or publish error, or a panic in that span never leaves
/// the agent running when `__monitor` exits. Dropped while armed, it
/// SIGKILLs the agent's group and only then reaps the leader: because it
/// holds the unreaped leader, the pgid cannot be recycled before its own
/// signal. [`disarm`](Self::disarm) hands the `Child` back once the
/// [`Supervisor`] owns teardown.
struct AgentGroupGuard {
    /// The agent, unreaped; `None` only once disarmed.
    child: Option<std::process::Child>,
    /// The agent's pgid: its pid, checked by `confirm_own_group`.
    pgid: u32,
}

impl AgentGroupGuard {
    /// Take ownership of `child`, which leads its own group.
    fn arm(child: std::process::Child) -> Self {
        let pgid = child.id();
        Self {
            child: Some(child),
            pgid,
        }
    }

    /// Hand the agent back: the [`Supervisor`] now owns its teardown.
    fn disarm(mut self) -> std::process::Child {
        self.child
            .take()
            .expect("an armed AgentGroupGuard holds its child until disarm")
    }
}

impl Drop for AgentGroupGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            warn!(
                "ending agent group {}: its supervisor failed before publishing",
                self.pgid
            );
            signal_guarded_group(self.pgid, libc::SIGKILL);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// How long [`monitor_test_hold`] waits for its release file.
#[cfg(test)]
const MONITOR_TEST_HOLD_WAIT: Duration = Duration::from_secs(30);

/// A test-only hold point: when `DEVFLOW_TEST_HOLD` is `<point>:<path>` for
/// this `point`, wait up to [`MONITOR_TEST_HOLD_WAIT`] for `<path>` to
/// exist, so a test can land a fault in a chosen window. Compiled out of
/// non-test builds (pinned by
/// `monitor::tests::the_test_hold_is_compiled_out_of_non_test_builds`).
#[cfg(test)]
fn monitor_test_hold(point: &str) {
    let Ok(hold) = std::env::var("DEVFLOW_TEST_HOLD") else {
        return;
    };
    let Some((at, path)) = hold.split_once(':') else {
        return;
    };
    if at != point {
        return;
    }
    let release = Path::new(path);
    let deadline = std::time::Instant::now() + MONITOR_TEST_HOLD_WAIT;
    while !release.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Spawn `spec`'s agent in its own process group (`process_group(0)`, R-3),
/// through `hermetic_command` with the adapter env re-applied after its
/// scrub and the gate notify command stripped. Returns the child and, for
/// the stream shape, the capture file the reader thread tees into.
///
/// The capture and stderr files truncate at open, as the Legacy script's
/// `>` and `2>` do, and stay separate so stderr never corrupts the parsed
/// stdout capture. The capture shape hands the capture file to the agent as
/// its stdout, with stdin `/dev/null`; the stream shape pipes both.
fn spawn_stage_agent(
    spec: &StageSpec,
) -> Result<(std::process::Child, Option<std::fs::File>), MonitorError> {
    let capture = std::fs::File::create(crate::agent_result::stdout_path(
        &spec.project_root,
        spec.phase,
    ))?;
    let stderr = std::fs::File::create(crate::agent_result::stderr_path(
        &spec.project_root,
        spec.phase,
    ))?;
    let mut agent_cmd = hermetic_command(&spec.program, &spec.workdir);
    strip_notify_env(&mut agent_cmd);
    agent_cmd
        .args(&spec.args)
        .envs(spec.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stderr(Stdio::from(stderr))
        .process_group(0);
    let stream_capture = match spec.io {
        StageIo::Capture => {
            agent_cmd.stdin(Stdio::null()).stdout(Stdio::from(capture));
            None
        }
        StageIo::Stream { .. } => {
            agent_cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
            Some(capture)
        }
    };
    Ok((agent_cmd.spawn()?, stream_capture))
}

/// Require that `pid` leads its own process group: `getpgid(pid) == pid`.
/// A just-spawned child's `setpgid` has normally landed by the time `spawn`
/// returns; if it has not, re-read `/proc/<pid>/stat` field 5 (falling back
/// to `getpgid` where `/proc` is unreadable) for at most
/// [`PGID_SETTLE_WAIT`]. Returns the pgid last seen on a mismatch (`-1` when
/// it could not be read).
fn confirm_own_group(pid: u32) -> Result<(), i32> {
    let Ok(signed) = libc::pid_t::try_from(pid) else {
        return Err(-1);
    };
    // SAFETY: getpgid has no memory preconditions; a bad pid returns -1.
    let read_pgid = || proc_stat_pgrp(pid).unwrap_or_else(|| unsafe { libc::getpgid(signed) });
    // SAFETY: as above.
    let mut pgid = unsafe { libc::getpgid(signed) };
    let deadline = std::time::Instant::now() + PGID_SETTLE_WAIT;
    while pgid != signed && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        pgid = read_pgid();
    }
    if pgid == signed { Ok(()) } else { Err(pgid) }
}

/// Field 5 (`pgrp`) of `/proc/<pid>/stat`, or `None` where it cannot be read.
fn proc_stat_pgrp(pid: u32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // `comm` (field 2) may hold spaces or parentheses: split after the last
    // `)`. Field 3 is then index 0, so field 5 is index 2.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(2)?.parse().ok()
}

/// SIGKILL an agent this function's caller spawned (its group, when it leads
/// one) and reap it, on a path that gives up before the supervisor owns it.
/// The pid comes from the in-memory `Child`, never from disk.
fn abandon_agent(child: &mut std::process::Child) {
    // SAFETY: getpgrp has no preconditions and cannot fail.
    let own_group = unsafe { libc::getpgrp() };
    if let Ok(signed) = libc::pid_t::try_from(child.id())
        && signed > 1
        && signed != own_group
    {
        // SAFETY: `signed > 1` and not our own group. A child that does not
        // lead a group makes this ESRCH: no group can be numbered by a pid
        // that is still alive as a non-leader.
        unsafe { libc::kill(-signed, libc::SIGKILL) };
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The sidecar for `spec`'s agent, spawned as `agent_pid` (its own group
/// leader), served by this process.
fn stage_sidecar(spec: &StageSpec, agent_pid: u32) -> crate::supervisor::Sidecar {
    let supervisor_pid = std::process::id();
    crate::supervisor::Sidecar {
        launch_id: spec.launch_id.clone(),
        project_root: spec.project_root.clone(),
        phase: spec.phase,
        stage: spec.stage.as_ref().map(ToString::to_string),
        agent: spec.agent.to_string(),
        launch: spec.io.label().to_string(),
        agent_pid,
        agent_pgid: agent_pid,
        agent_start_time: crate::agent::process_start_time(agent_pid).unwrap_or(0),
        boot_id: crate::supervisor::boot_id().unwrap_or_default(),
        supervisor_pid,
        supervisor_start_time: crate::agent::process_start_time(supervisor_pid).unwrap_or(0),
        created_at: crate::supervisor::rfc3339_now(),
    }
}

/// How long a launcher waits for a new supervisor to answer a ping with its
/// launch id before it ends what it started (50-24).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
const PUBLISH_WAIT: Duration = Duration::from_secs(10);

/// How often [`wait_for_publish`] polls the child and probes the socket.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
const PUBLISH_POLL: Duration = Duration::from_millis(20);

/// How long the not-ready teardown waits for its SIGSTOP to show in
/// `/proc` before it lists the child's children.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
const STOP_SETTLE_WAIT: Duration = Duration::from_millis(100);

/// The `/proc/<pid>/stat` fields the not-ready teardown picks its targets
/// by, from one read. 50-09's `agent::ProcStat` carries no parent pid, so
/// this is a separate, private parse.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChildStat {
    /// Field 4, the parent pid.
    ppid: u32,
    /// Field 5, the process group id.
    pgid: i32,
    /// Field 22, the start time in clock ticks since boot, as
    /// [`crate::agent::is_same_process`] compares it.
    start_time: u64,
}

/// Parse one `/proc/<pid>/stat` line into [`ChildStat`]. `comm` (field 2)
/// may hold spaces and parentheses, so the line is split after its last
/// `)`: index 0 is then field 3, so the parent pid (field 4) is index 1,
/// the process group (field 5) index 2 and the start time (field 22)
/// index 19. A missing or unparsable field is `None`, never a guess.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn parse_proc_stat_line(line: &str) -> Option<ChildStat> {
    let rest = &line[line.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    Some(ChildStat {
        ppid: fields.get(1)?.parse().ok()?,
        pgid: fields.get(2)?.parse().ok()?,
        start_time: fields.get(19)?.parse().ok()?,
    })
}

/// [`ChildStat`] for `pid` from one read of `/proc/<pid>/stat`, or `None`
/// when it cannot be read or parsed.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn read_child_stat(pid: u32) -> Option<ChildStat> {
    parse_proc_stat_line(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// Every process whose parent pid is `parent`, each with the stat it was
/// chosen by. An entry that cannot be read is skipped.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn direct_children(parent: u32) -> Vec<(u32, ChildStat)> {
    proc_table()
        .into_iter()
        .filter(|(_, stat)| stat.ppid == parent)
        .collect()
}

/// Every readable `/proc/<pid>/stat`, each from one read. An unlistable
/// `/proc` is an empty table (logged); an unreadable entry is skipped.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn proc_table() -> Vec<(u32, ChildStat)> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        warn!("could not list /proc");
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| Some((pid, read_child_stat(pid)?)))
        .collect()
}

/// Signal process group `pgid`: the one place this module's launch paths
/// send a group signal. It refuses a pgid that does not fit `pid_t`, is `0`
/// or `1`, or is this process's own group, because `kill(0, …)` signals our
/// own group and `kill(-1, …)` every process we may signal (the runtime's
/// rule, T-50-18). Callers pass only a group they started or one whose
/// leader they have just re-validated.
fn signal_guarded_group(pgid: u32, signal: libc::c_int) -> bool {
    let Ok(signed) = libc::pid_t::try_from(pgid) else {
        warn!("process group {pgid} does not fit pid_t; not signalling");
        return false;
    };
    if signed <= 1 {
        warn!("refusing to signal process group {signed}");
        return false;
    }
    // SAFETY: getpgrp has no preconditions and cannot fail.
    if signed == unsafe { libc::getpgrp() } {
        warn!("refusing to signal this process's own group {signed}");
        return false;
    }
    // SAFETY: `signed > 1`, so `-signed < -1`: neither our own group (`0`)
    // nor every process (`-1`).
    unsafe { libc::kill(-signed, signal) == 0 }
}

/// Send `signal` to `child` alone, by the pid of the `Child` this process
/// spawned and has not reaped (so the pid cannot name another process).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn signal_own_child(child: &std::process::Child, signal: libc::c_int) -> bool {
    match libc::pid_t::try_from(child.id()) {
        // SAFETY: `pid > 1` names one unreaped child of this process.
        Ok(pid) if pid > 1 => unsafe { libc::kill(pid, signal) == 0 },
        _ => false,
    }
}

/// Poll `child` until it exits (reaping it) or `wait` elapses.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn exit_within(
    child: &mut std::process::Child,
    wait: Duration,
) -> Option<std::process::ExitStatus> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(PUBLISH_POLL);
    }
}

/// End a supervisor that did not publish in time, and what it started,
/// using only its own `Child` and values read from `/proc` while it is
/// stopped, never a pid from disk. Returns how it ended.
///
/// 1. SIGTERM: a supervisor that reached `serve` treats it as a stop (D-05)
///    and ends its agent group itself. Wait up to
///    `2 × agent::TERMINATE_VERIFY_WAIT`.
/// 2. Still running: SIGSTOP it, so it cannot spawn or reap meanwhile, and
///    list its direct children with their group and start time.
/// 3. SIGKILL its own group (it leads it, `process_group(0)`) and reap it.
/// 4. SIGKILL the group of each listed child that leads its own group
///    (R-3's agent shape) and still has the listed start time. A bare kill
///    of the child's group would orphan such an agent.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn end_unpublished_child(child: &mut std::process::Child, wait: Duration) -> String {
    signal_own_child(child, libc::SIGTERM);
    if let Some(status) = exit_within(child, 2 * crate::agent::TERMINATE_VERIFY_WAIT) {
        return format!(
            "did not publish within {wait:?} and was ended by SIGTERM ({status}); \
             0 child process groups ended"
        );
    }
    let pid = child.id();
    signal_own_child(child, libc::SIGSTOP);
    let deadline = std::time::Instant::now() + STOP_SETTLE_WAIT;
    while !crate::agent::process_stat_fields(pid).is_some_and(|stat| stat.state == 'T')
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let children = direct_children(pid);
    signal_guarded_group(pid, libc::SIGKILL);
    // By pid as well, in case it leads no group: a pid still in use as a
    // live non-leader names no group, so the group signal reached nothing.
    let _ = child.kill();
    let _ = child.wait();
    let ended = children
        .iter()
        .filter(|(child_pid, stat)| {
            i64::from(stat.pgid) == i64::from(*child_pid)
                && crate::agent::is_same_process(*child_pid, stat.start_time)
                && signal_guarded_group(*child_pid, libc::SIGKILL)
        })
        .count();
    format!(
        "did not publish within {wait:?} and was ended (SIGTERM ignored for {grace:?}, then \
         SIGSTOP and SIGKILL of its group); {ended} child process group(s) ended",
        grace = 2 * crate::agent::TERMINATE_VERIFY_WAIT
    )
}

/// Wait up to `wait` for the supervisor `child` (spawned by this process
/// with `process_group(0)`) to publish `socket` with `launch_id`.
///
/// Polls every [`PUBLISH_POLL`]: a ping reply with this launch id returns
/// it and leaves the child running; Gone, Stale, a probe error or another
/// launch id count as not yet published. If the child exits first it is
/// reaped; if the wait expires it is ended with what it started
/// ([`end_unpublished_child`]). Either way nothing can answer on this
/// launch's address any more (its only server was the reaped child), so its
/// launch files are removed, and the result is [`MonitorError::NotReady`]
/// naming the child, how it ended and how many child groups were ended.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn wait_for_publish(
    child: &mut std::process::Child,
    socket: &SocketPath,
    launch_id: &str,
    wait: Duration,
) -> Result<crate::supervisor::protocol::PingReply, MonitorError> {
    let pid = child.id();
    let deadline = std::time::Instant::now() + wait;
    let how = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break format!(
                "exited by itself before publishing ({status}); 0 child process groups ended"
            );
        }
        if let Ok(crate::supervisor::Liveness::Alive(reply)) =
            crate::supervisor::probe(socket.as_path())
            && reply.launch_id == launch_id
        {
            return Ok(reply);
        }
        if std::time::Instant::now() >= deadline {
            break end_unpublished_child(child, wait);
        }
        std::thread::sleep(PUBLISH_POLL);
    };
    let recorded = end_recorded_agent_group(socket, launch_id);
    let removed = if recorded.keep_sidecar {
        match std::fs::remove_file(socket.as_path()) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    } else {
        crate::supervisor::remove_launch_files(socket)
    };
    if let Err(error) = removed {
        warn!(
            "could not remove the launch files of {}: {error}",
            socket.as_path().display()
        );
    }
    Err(MonitorError::NotReady(format!(
        "supervisor {pid} {how}; {}",
        recorded.text
    )))
}

/// How long the ending step waits for a SIGKILLed recorded agent group to
/// empty.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
const RECORDED_GROUP_GONE_WAIT: Duration = Duration::from_secs(2);

/// What the ending step did about the agent group a launch's sidecar
/// records.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
struct RecordedGroupEnd {
    /// The sentence the `NotReady` message carries.
    text: String,
    /// Keep the sidecar, so `doctor` still lists the launch: the backstop
    /// refused the recorded group, or it did not empty after its SIGKILL.
    keep_sidecar: bool,
}

/// End the agent group the sidecar beside `socket` records, once the
/// supervisor that wrote it is reaped (RV-03). A `__monitor` SIGKILLed
/// after its sidecar write and before it published ran neither its
/// `AgentGroupGuard` nor its signal handler, and its agent (which leads its
/// own group, R-3) was reparented away, so the sidecar is the only record of
/// it.
///
/// This reads a pgid from disk, so it is held to D-04's rule for the
/// backstop: the group is signalled only when the sidecar is this launch's,
/// [`backstop::assess`](crate::supervisor::backstop::assess) returns
/// `LeaderPresentSignal` (boot id current, leader present with the recorded
/// start time and leading the recorded group) and
/// [`agent::is_same_process`](crate::agent::is_same_process) re-confirms
/// the leader immediately before the SIGKILL. No sidecar, another launch
/// id or any other decision signals nothing, and the text says which.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn end_recorded_agent_group(socket: &SocketPath, launch_id: &str) -> RecordedGroupEnd {
    use crate::supervisor::backstop::BackstopDecision;
    let done = |text: String| RecordedGroupEnd {
        text,
        keep_sidecar: false,
    };
    let sidecar = match crate::supervisor::read_sidecar(&socket.sidecar_path()) {
        Ok(sidecar) => sidecar,
        Err(crate::supervisor::SidecarError::Io(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            return done(format!(
                "no sidecar for launch {launch_id}; no agent group could be identified"
            ));
        }
        Err(error) => {
            return done(format!(
                "the sidecar for launch {launch_id} is unusable ({error}); nothing was signalled"
            ));
        }
    };
    if sidecar.launch_id != launch_id {
        return done(format!(
            "the sidecar names launch {}, not {launch_id}; nothing was signalled",
            sidecar.launch_id
        ));
    }
    let pgid = sidecar.agent_pgid;
    let decision = crate::supervisor::backstop::assess(&sidecar, None);
    let name = decision_name(&decision);
    match decision {
        BackstopDecision::LeaderPresentSignal => {
            if !crate::agent::is_same_process(pgid, sidecar.agent_start_time) {
                return done(format!(
                    "the leader of the agent group {pgid} its sidecar records changed before \
                     the signal; nothing was signalled"
                ));
            }
            if !signal_guarded_group(pgid, libc::SIGKILL) {
                return done(format!(
                    "the agent group {pgid} its sidecar records could not be signalled"
                ));
            }
            if group_empties_within(pgid, RECORDED_GROUP_GONE_WAIT) {
                done(format!("ended the agent group {pgid} its sidecar records"))
            } else {
                RecordedGroupEnd {
                    text: format!(
                        "ended the agent group {pgid} its sidecar records, but it still had \
                         members after {RECORDED_GROUP_GONE_WAIT:?}; the sidecar is kept"
                    ),
                    keep_sidecar: true,
                }
            }
        }
        BackstopDecision::EmptyGroup
        | BackstopDecision::LeaderRecycled
        | BackstopDecision::PreviousBoot => done(format!(
            "the agent group {pgid} its sidecar records reads {name}; nothing was signalled"
        )),
        BackstopDecision::RefuseLeaderGone(_)
        | BackstopDecision::RefuseMismatch(_)
        | BackstopDecision::RefuseUnsafeTarget
        | BackstopDecision::RefuseIdentityUnavailable => RecordedGroupEnd {
            text: format!(
                "the agent group {pgid} its sidecar records reads {name} with {members} \
                 member(s); nothing was signalled and the sidecar is kept",
                members = group_member_count(pgid)
            ),
            keep_sidecar: true,
        },
    }
}

/// The variant name of a backstop decision, for the `NotReady` text.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn decision_name(decision: &crate::supervisor::backstop::BackstopDecision) -> &'static str {
    use crate::supervisor::backstop::BackstopDecision;
    match decision {
        BackstopDecision::RefuseUnsafeTarget => "RefuseUnsafeTarget",
        BackstopDecision::RefuseIdentityUnavailable => "RefuseIdentityUnavailable",
        BackstopDecision::RefuseMismatch(_) => "RefuseMismatch",
        BackstopDecision::PreviousBoot => "PreviousBoot",
        BackstopDecision::EmptyGroup => "EmptyGroup",
        BackstopDecision::LeaderPresentSignal => "LeaderPresentSignal",
        BackstopDecision::LeaderRecycled => "LeaderRecycled",
        BackstopDecision::RefuseLeaderGone(_) => "RefuseLeaderGone",
    }
}

/// How many processes `/proc` lists in group `pgid` right now.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn group_member_count(pgid: u32) -> usize {
    proc_table()
        .iter()
        .filter(|(_, stat)| i64::from(stat.pgid) == i64::from(pgid))
        .count()
}

/// Poll until no process is left in group `pgid` (`kill(-pgid, 0)` fails
/// with ESRCH), up to `wait`. A pgid that is not a safe group target never
/// counts as empty.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "spawn_monitor calls it from 50-11")
)]
fn group_empties_within(pgid: u32, wait: Duration) -> bool {
    let Ok(signed) = libc::pid_t::try_from(pgid) else {
        return false;
    };
    if signed <= 1 {
        return false;
    }
    let deadline = std::time::Instant::now() + wait;
    loop {
        // SAFETY: signal 0 only checks for existence; `signed > 1`.
        let rc = unsafe { libc::kill(-signed, 0) };
        if rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(PUBLISH_POLL);
    }
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
    let (commits, enumeration_note) = enumerate_phase_commits(project_root, workdir, phase);

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
/// (`{base}..{feature_prefix}phase-NN`) — the same question asked with
/// `git log` instead of `rev-list --count`, so the two can never disagree
/// about which commits are the agent's.
///
/// **`project_root` and `workdir` are deliberately distinct (45-01, review
/// round 2).** The git-flow config resolves from `project_root` — the
/// canonical checkout — while the `git log` still runs in `workdir`.
/// Resolving from `workdir` would read a phase worktree, which need not carry
/// a `devflow.toml` that exists only in the main checkout, and would silently
/// fall back to the default trunk. Against a mismatched trunk this range
/// either over- or under-reports the phase's commits, which is exactly the
/// false-evidence shape the never-silent commit gate depends on not producing.
///
/// Never returns an error. Every failure path yields an empty list and a note
/// naming what went wrong: the operator losing the commit NAMES is bad, the
/// operator losing the VERDICT is the failure this whole plan exists to
/// prevent.
fn enumerate_phase_commits(
    project_root: &Path,
    workdir: &Path,
    phase: PhaseId,
) -> (Vec<IdleTimeoutCommit>, Option<String>) {
    // CR-45-06: `git_flow_for_project` re-resolves from AMBIENT
    // configuration, and `DEVFLOW_BASE_BRANCH` lives in the environment of
    // whichever shell ran `devflow start`. A monitor re-launched by
    // `devflow resume` from a clean shell therefore ranged from `develop`
    // against a run based on something else — over- or under-reporting the
    // phase's commits in the record that IS the operator's timeout evidence.
    // Round 2's fix made this resolve from `project_root` rather than
    // `workdir`, which is a different bug and left this one open.
    //
    // The state file is the same source of truth `git_flow_for_run` reads;
    // consulting it here rather than threading a base through
    // `run_pipe_owning_monitor`'s public signature keeps the fix local to
    // the site that needs it. A missing or unreadable state degrades to the
    // ambient resolution — the pre-existing behaviour, never an abort,
    // because losing the verdict is worse than losing the commit names.
    let persisted_base = crate::workflow::load_state(project_root, phase)
        .ok()
        .and_then(|state| state.base_branch);
    let git_flow = crate::config::git_flow_for_run(project_root, persisted_base.as_deref());
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

    #[test]
    fn legacy_advance_tail_passes_the_launched_stage() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_in(dir.path());
        state.stage = Stage::Code;
        let root = dir.path().to_str().unwrap();

        let tail = advance_tail("devflow", root, &state);
        assert!(tail.contains(" advance "));
        assert!(tail.contains(&format!("--phase {} --stage code", state.phase)));
    }

    #[test]
    fn pipe_owning_monitor_args_pass_the_stage() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_in(dir.path());
        state.stage = Stage::Code;
        let args = pipe_owning_monitor_args("/project", &state, "/workdir", "/prompt", 60);
        let stage = args.iter().position(|arg| arg == "--stage").unwrap();
        let separator = args.iter().position(|arg| arg == "--").unwrap();

        assert_eq!(args[stage + 1], "code");
        assert!(stage < separator, "stage must be parsed before child argv");
    }

    /// WR-07 (49-REVIEW.md): the chain's command never carries the variable.
    #[test]
    fn strip_notify_env_removes_the_variable_from_the_child() {
        let mut cmd = std::process::Command::new("true");
        cmd.env(crate::gates::GATE_NOTIFY_CMD_ENV, "touch x");
        strip_notify_env(&mut cmd);
        let entry = cmd
            .get_envs()
            .find(|(key, _)| *key == crate::gates::GATE_NOTIFY_CMD_ENV);
        assert_eq!(
            entry,
            Some((
                std::ffi::OsStr::new(crate::gates::GATE_NOTIFY_CMD_ENV),
                None
            ))
        );
    }

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

    /// The launch context every pre-50-10 `spawn_monitor` test passes: this
    /// test binary as the monitor executable, exactly what `current_exe()`
    /// resolved to inside `spawn_monitor` before 50-10, and a cache dir under
    /// the test's own project tempdir.
    fn test_launch_context(state: &State) -> LaunchContext {
        LaunchContext::new(
            state.project_root.join("cache"),
            MonitorExec::Subcommand {
                exe: std::env::current_exe().unwrap(),
            },
        )
    }

    /// T-50-26's source scan: whether the first `HarnessEntry {` after
    /// `enum MonitorExec` in `source` carries `cfg(feature = "test-support")`
    /// on its own line or on one of the two lines above it. The source is cut
    /// at its first `mod tests {` line and `//` lines are dropped first, so
    /// neither this test nor a comment can satisfy or fail the scan. `None`
    /// when there is no such enum or variant.
    fn harness_entry_is_feature_gated(source: &str) -> Option<bool> {
        let code: Vec<&str> = source
            .lines()
            .take_while(|line| line.trim() != concat!("mod ", "tests {"))
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        let enum_at = code
            .iter()
            .position(|line| line.contains(concat!("enum ", "MonitorExec")))?;
        let variant_at = enum_at
            + code[enum_at..]
                .iter()
                .position(|line| line.contains(concat!("HarnessEntry", " {")))?;
        let gate = concat!("cfg(feature = ", "\"test-support\")");
        let window_start = variant_at.saturating_sub(2).max(enum_at);
        Some(
            code[window_start..=variant_at]
                .iter()
                .any(|line| line.contains(gate)),
        )
    }

    /// T-50-26: the test-only monitor entry cannot exist in a release build,
    /// because its variant is compiled only under `test-support`.
    #[test]
    fn the_harness_entry_variant_is_declared_only_under_the_test_support_feature() {
        let ungated = concat!(
            "pub enum ",
            "MonitorExec {\n",
            "    Subcommand { exe: PathBuf },\n",
            "    /// docs\n",
            "    HarnessEntry",
            " { exe: PathBuf, test_name: String },\n",
            "}\n"
        );
        assert_eq!(
            harness_entry_is_feature_gated(ungated),
            Some(false),
            "planted control: an ungated HarnessEntry must be reported ungated"
        );
        let gated = concat!(
            "pub enum ",
            "MonitorExec {\n",
            "    Subcommand { exe: PathBuf },\n",
            "    #[cfg(feature = ",
            "\"test-support\")]\n",
            "    HarnessEntry",
            " { exe: PathBuf, test_name: String },\n",
            "}\n"
        );
        assert_eq!(
            harness_entry_is_feature_gated(gated),
            Some(true),
            "planted control: a gated HarnessEntry must be reported gated"
        );

        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/monitor.rs");
        let source = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            harness_entry_is_feature_gated(&source),
            Some(true),
            "{}: MonitorExec::HarnessEntry must be declared under cfg(feature = \"test-support\")",
            path.display()
        );
    }

    // ---- 45-01 Task 3: idle-timeout commit evidence resolves its trunk
    //      from the project, not from the phase worktree it runs `git log` in.

    fn commit_range_git(root: &Path, args: &[&str]) {
        let out = crate::test_support::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A repo with `develop`, a `workspace/example` branch one commit ahead
    /// of it, and `feature/phase-04` one commit ahead of THAT. The middle
    /// commit is the discriminator: it is reachable from the configured base,
    /// so a correctly-scoped range excludes it and a default-trunk range
    /// includes it.
    fn idle_commit_fixture(root: &Path) {
        commit_range_git(root, &["init", "-q", "-b", "develop"]);
        commit_range_git(root, &["config", "user.email", "t@e.st"]);
        commit_range_git(root, &["config", "user.name", "t"]);
        commit_range_git(root, &["config", "commit.gpgsign", "false"]);
        commit_range_git(root, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "x").unwrap();
        commit_range_git(root, &["add", "-A"]);
        commit_range_git(root, &["commit", "-q", "-m", "init"]);

        commit_range_git(root, &["checkout", "-q", "-b", "workspace/example"]);
        std::fs::write(root.join("base-only.txt"), "x").unwrap();
        commit_range_git(root, &["add", "-A"]);
        commit_range_git(root, &["commit", "-q", "-m", "base-only commit"]);

        commit_range_git(root, &["checkout", "-q", "-b", "feature/phase-04"]);
        std::fs::write(root.join("agent-work.txt"), "x").unwrap();
        commit_range_git(root, &["add", "-A"]);
        commit_range_git(root, &["commit", "-q", "-m", "agent-work commit"]);
    }

    /// codex (external review, 2026-09-02, CR-45-06): this enumeration read
    /// AMBIENT configuration, and `DEVFLOW_BASE_BRANCH` lives in the
    /// environment of whichever shell ran `devflow start`. A monitor
    /// re-launched by `devflow resume` from a clean shell therefore ranged
    /// from `develop` against a run based elsewhere, and this list IS the
    /// operator's idle-timeout evidence — the false-evidence shape the
    /// never-silent commit gate depends on not producing. Round 2's fix
    /// (resolve from `project_root`, not `workdir`) is a different bug and
    /// left this one open.
    #[test]
    fn enumerate_phase_commits_prefers_the_runs_persisted_base() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        idle_commit_fixture(root);
        // No `devflow.toml` and no environment variable: ambient resolution
        // yields `develop`, which is precisely the resumed-shell case.
        assert!(
            std::env::var("DEVFLOW_BASE_BRANCH")
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "DEVFLOW_BASE_BRANCH is exported — it outranks the ambient default"
        );

        let subjects = |root: &Path| -> Vec<String> {
            let (commits, note) = enumerate_phase_commits(root, root, PhaseId::new(4));
            assert!(note.is_none(), "enumeration must not degrade: {note:?}");
            commits.into_iter().map(|c| c.subject).collect()
        };

        // NEGATIVE CONTROL, asserted FIRST: with no state on disk the
        // enumeration falls back to ambient resolution and over-reports —
        // `base-only commit` belongs to the base, not to the agent. This is
        // the bug, and it pins that the assertion below is measuring the
        // persisted base rather than passing for any reason at all.
        let ambient = subjects(root);
        assert!(
            ambient.iter().any(|s| s == "base-only commit"),
            "without persisted state the range runs from develop and includes base commits: \
             {ambient:?}"
        );

        // The run recorded its base at `start`.
        let mut state = crate::state::State::new(
            PhaseId::new(4),
            crate::state::AgentKind::Claude,
            crate::mode::Mode::Auto,
            root.to_path_buf(),
        );
        state.base_branch = Some("workspace/example".to_string());
        crate::workflow::save_state(&state).expect("save state");

        let persisted = subjects(root);
        assert!(
            persisted.iter().any(|s| s == "agent-work commit"),
            "a commit reachable only from the feature branch must be listed: {persisted:?}"
        );
        assert!(
            !persisted.iter().any(|s| s == "base-only commit"),
            "a commit already on the run's own base must NOT be reported as agent work: \
             {persisted:?}"
        );
    }

    #[test]
    fn enumerate_phase_commits_ranges_from_the_configured_base() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        idle_commit_fixture(root);
        std::fs::write(
            root.join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();
        assert!(
            std::env::var("DEVFLOW_BASE_BRANCH")
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "DEVFLOW_BASE_BRANCH is exported — it outranks devflow.toml"
        );

        let (commits, note) = enumerate_phase_commits(root, root, PhaseId::new(4));
        assert!(note.is_none(), "enumeration must not degrade: {note:?}");
        let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();

        assert!(
            subjects.contains(&"agent-work commit"),
            "a commit reachable only from the feature branch must be listed: {subjects:?}"
        );
        // NEGATIVE CONTROL: a commit present on the configured base must NOT
        // be listed. Without this half the test passes against a range built
        // from the default trunk, which lists both.
        assert!(
            !subjects.contains(&"base-only commit"),
            "a commit already on the configured base must not be listed: {subjects:?}"
        );
    }

    /// Review round 2's monitor-root finding: the config must resolve from
    /// the canonical project root, not from the phase worktree the `git log`
    /// runs in. A phase worktree need not carry a `devflow.toml` that exists
    /// only in the main checkout, and the fallback is silent.
    #[test]
    fn enumerate_phase_commits_resolves_config_from_the_project_root() {
        // The project root: carries the config AND the git history.
        let project = tempfile::tempdir().unwrap();
        let project_root = project.path();
        idle_commit_fixture(project_root);
        std::fs::write(
            project_root.join("devflow.toml"),
            "base_branch = \"workspace/example\"\n",
        )
        .unwrap();
        assert!(
            std::env::var("DEVFLOW_BASE_BRANCH")
                .map(|v| v.is_empty())
                .unwrap_or(true),
            "DEVFLOW_BASE_BRANCH is exported — it outranks devflow.toml"
        );

        // The workdir: a linked worktree of the same repository that carries
        // NO `devflow.toml` — the shape of a phase worktree whose base branch
        // does not track the operator's local config.
        let wt = tempfile::tempdir().unwrap();
        let workdir = wt.path().join("phase-04");
        commit_range_git(
            project_root,
            &[
                "worktree",
                "add",
                "--detach",
                workdir.to_str().unwrap(),
                "feature/phase-04",
            ],
        );
        assert!(
            !workdir.join("devflow.toml").exists(),
            "the worktree must not carry the config, or the test proves nothing"
        );

        let (commits, note) = enumerate_phase_commits(project_root, &workdir, PhaseId::new(4));
        assert!(note.is_none(), "enumeration must not degrade: {note:?}");
        let configured: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
        assert!(
            !configured.contains(&"base-only commit"),
            "config resolved from the project root must scope the range: {configured:?}"
        );

        // NEGATIVE CONTROL: swap the two arguments so the config lookup hits
        // the worktree instead. Without the swap the test cannot distinguish
        // "resolved from the right root" from "both roots gave the same
        // answer", which is the whole of the concern.
        let (swapped, _) = enumerate_phase_commits(&workdir, project_root, PhaseId::new(4));
        let swapped: Vec<&str> = swapped.iter().map(|c| c.subject.as_str()).collect();
        assert_ne!(
            configured, swapped,
            "resolving config from the worktree must give a DIFFERENT range"
        );
        assert!(
            swapped.contains(&"base-only commit"),
            "the worktree has no config, so the swapped range falls back to the \
             default trunk and over-reports: {swapped:?}"
        );
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

        let monitor_pid = spawn_monitor(
            &state,
            "sh",
            &args,
            &[],
            MonitorLaunch::Legacy,
            &test_launch_context(&state),
        )
        .unwrap();
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

        let monitor_pid = spawn_monitor(
            &state,
            "sh",
            &args,
            &[],
            MonitorLaunch::Legacy,
            &test_launch_context(&state),
        )
        .unwrap();
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
        // 2026-09-22 diagnosis (monitor-sigterm-orphans-agent,
        // .planning/debug): under dash (the container's /bin/sh; the Fedora
        // host's is bash), a TERM that lands in the agent child after its
        // fork but before dash's post-fork trap reset is caught by the
        // inherited handler and then discarded, and the child execs an agent
        // that never saw it. The CI failures' `agent before` snapshot shows
        // the child still pre-exec, consistent with that window; this test
        // only races it. The `sigterm_before_*` tests below hold it (and the
        // monitor-side `apid=$!` window) open deterministically; the Legacy
        // script's stop marker closes the first and `${apid:-$!}` the second.
        //
        // Do NOT widen this window: a failure here is a lost TERM, not a slow
        // one.
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

    // ---- monitor-sigterm-orphans-agent (.planning/debug, 2026-09-22): a TERM
    //      that lands inside one of the Legacy monitor script's spawn windows.
    //
    // Window A (dash only; Debian's /bin/sh, so CI's): the forked agent child
    // keeps the monitor's *caught* TERM handler until dash's fork-reset clears
    // the trap. A TERM that lands first is recorded by that handler and then
    // discarded (`dotrap` skips a signal whose trap is now NULL), and the child
    // goes on to exec the agent, which never sees a TERM and is orphaned when
    // the monitor exits. bash does not lose it, which is why the Fedora host
    // never reproduced `sigterm_to_monitor_also_kills_the_agent`'s CI failures.
    //
    // Window F (dash and bash): a TERM that reaches the monitor after its fork
    // but before `apid=$!` runs `cleanup` with an empty `apid`, so it kills
    // nothing.
    //
    // Both windows are microseconds wide, so these tests hold them open
    // deterministically instead of racing them. An LD_PRELOAD `fork()` shim,
    // compiled here with `cc` and handed to the monitor through
    // `spawn_monitor`'s own `envs`, holds ONE side of the monitor's first fork
    // (the agent launch) until the test creates a release file. The shim
    // blocks every signal for the hold, so a TERM sent inside it stays pending
    // (kernel-visible in /proc) until the hold ends, and each test asserts that
    // the TERM really landed inside the window instead of assuming it.

    /// The `fork()` interposer. Only the first fork, on the configured side,
    /// in a process carrying these variables holds: that side unlinks the
    /// `HOLDFORK_ONCE` file, and every later fork (the exec'd agent inherits
    /// `LD_PRELOAD`) finds it gone.
    #[cfg(target_os = "linux")]
    const HOLDFORK_SHIM_C: &str = r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static void record_pending_term(const char *path) {
    sigset_t pending;
    if (sigpending(&pending) == 0 && sigismember(&pending, SIGTERM)) {
        int fd = open(path, O_WRONLY | O_CREAT, 0600);
        if (fd >= 0) close(fd);
    }
}

pid_t fork(void) {
    pid_t (*real_fork)(void) = (pid_t (*)(void))dlsym(RTLD_NEXT, "fork");
    pid_t pid = real_fork();
    if (pid < 0) return pid;
    int saved_errno = errno;
    const char *side = getenv("HOLDFORK_SIDE");
    const char *once = getenv("HOLDFORK_ONCE");
    const char *release = getenv("HOLDFORK_RELEASE");
    const char *term_seen = getenv("HOLDFORK_TERM_SEEN");
    if (side && once && release && term_seen
        && strcmp(side, pid == 0 ? "child" : "parent") == 0
        && unlink(once) == 0) {
        /* Every signal is blocked for the hold, so one sent inside it stays
           pending instead of running a handler whose only trace would be a
           cut-short sleep, which a process that is descheduled when the
           signal lands never sees. A pending TERM is recorded each tick and
           once more at release; restoring the mask then delivers it to the
           handler this process already had, still before any of the shell's
           own post-fork code. Hold until released: 10 ms ticks, capped at
           20 s so a crashed test cannot wedge a process forever. */
        sigset_t all, saved;
        sigfillset(&all);
        sigprocmask(SIG_BLOCK, &all, &saved);
        for (int tick = 0; tick < 2000 && access(release, F_OK) != 0; tick++) {
            struct timespec step = {0, 10 * 1000 * 1000};
            nanosleep(&step, NULL);
            record_pending_term(term_seen);
        }
        record_pending_term(term_seen);
        sigprocmask(SIG_SETMASK, &saved, NULL);
    }
    errno = saved_errno;
    return pid;
}
"#;

    /// Which side of the monitor's first `fork()` the shim holds.
    #[cfg(target_os = "linux")]
    #[derive(Clone, Copy)]
    enum HoldSide {
        /// The agent child, before any of the shell's post-fork code runs.
        Child,
        /// The monitor, after the fork returned and before `apid=$!`.
        Parent,
    }

    /// A Legacy monitor whose first fork is held open by the shim. Dropping it
    /// releases the hold and SIGKILLs every still-tracked process that is
    /// still recognisably ours, so a failing assertion leaks neither a held
    /// monitor nor a 30-second agent.
    #[cfg(target_os = "linux")]
    struct HeldMonitor {
        state: State,
        monitor_pid: u32,
        release: std::path::PathBuf,
        term_seen: std::path::PathBuf,
        tracked: Vec<u32>,
    }

    #[cfg(target_os = "linux")]
    impl HeldMonitor {
        fn spawn(root: &Path, side: HoldSide) -> Self {
            let shim = compile_holdfork_shim(root);
            let once = root.join("holdfork-once");
            std::fs::write(&once, "").unwrap();
            let release = root.join("holdfork-release");
            let term_seen = root.join("holdfork-term-seen");
            let side = match side {
                HoldSide::Child => "child",
                HoldSide::Parent => "parent",
            };
            let envs = [
                ("LD_PRELOAD", shim.to_str().unwrap()),
                ("HOLDFORK_SIDE", side),
                ("HOLDFORK_ONCE", once.to_str().unwrap()),
                ("HOLDFORK_RELEASE", release.to_str().unwrap()),
                ("HOLDFORK_TERM_SEEN", term_seen.to_str().unwrap()),
            ]
            .map(|(key, value)| (key.to_string(), value.to_string()));
            let state = state_in(root);
            let args = vec!["-c".to_string(), "sleep 30".to_string()];
            let monitor_pid = spawn_monitor(
                &state,
                "sh",
                &args,
                &envs,
                MonitorLaunch::Legacy,
                &test_launch_context(&state),
            )
            .unwrap();
            Self {
                state,
                monitor_pid,
                release,
                term_seen,
                tracked: vec![monitor_pid],
            }
        }

        fn release(&self) {
            std::fs::write(&self.release, "").unwrap();
        }

        /// Whether the shim saw a TERM pending during its hold.
        fn hold_saw_a_pending_term(&self) -> bool {
            self.term_seen.exists()
        }

        fn sigterm_monitor(&self) {
            let rc = unsafe { libc::kill(self.monitor_pid as libc::pid_t, libc::SIGTERM) };
            assert_eq!(
                rc,
                0,
                "kill(monitor, TERM): {}",
                std::io::Error::last_os_error()
            );
        }

        /// Reap the monitor (this test process is its parent) and require that
        /// it left through its trap: `cleanup` ends in `exit 0`, whereas a
        /// shell killed by the TERM itself reports a signal. Once reaped, its
        /// pid may be reused, so it is no longer tracked.
        fn reap_monitor_expecting_trap_exit(&mut self) {
            let pid = self.monitor_pid as libc::pid_t;
            let mut reaped = None;
            for _ in 0..250 {
                let mut status: libc::c_int = 0;
                let rc = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
                if rc == pid {
                    reaped = Some(status);
                    break;
                }
                assert_eq!(
                    rc,
                    0,
                    "waitpid(monitor): {}",
                    std::io::Error::last_os_error()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            let status = reaped.expect("the monitor did not exit within 5s of SIGTERM");
            self.tracked.retain(|&tracked| tracked != self.monitor_pid);
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "the monitor must leave through its trap (`cleanup` → `exit 0`); \
                 wait status {status:#x}"
            );
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for HeldMonitor {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.release, "");
            for &pid in &self.tracked {
                let cmd = cmdline_of(pid);
                let ours = cmd.contains("trap cleanup TERM INT") || cmd.contains("sleep 30");
                if ours && crate::agent::agent_running(pid) {
                    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn compile_holdfork_shim(dir: &Path) -> std::path::PathBuf {
        let source = dir.join("holdfork.c");
        std::fs::write(&source, HOLDFORK_SHIM_C).unwrap();
        let shim = dir.join("holdfork.so");
        let out = std::process::Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(&shim)
            .arg(&source)
            .arg("-ldl")
            .output()
            .expect(
                "these tests compile an LD_PRELOAD fork() shim and need `cc`, \
                 the C toolchain cargo already links with on Linux",
            );
        assert!(
            out.status.success(),
            "compiling the fork() shim failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        shim
    }

    /// `/proc/<pid>/cmdline` with its NULs as spaces; empty once the pid is
    /// gone or a zombie.
    #[cfg(target_os = "linux")]
    fn cmdline_of(pid: u32) -> String {
        std::fs::read(format!("/proc/{pid}/cmdline"))
            .map(|raw| {
                raw.split(|&byte| byte == 0)
                    .filter(|arg| !arg.is_empty())
                    .map(|arg| String::from_utf8_lossy(arg).into_owned())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    }

    /// Whether `pid` has exec'd the stub agent (`sh -c 'sleep 30'`, or
    /// `sleep 30` itself where the shell execs its last command).
    #[cfg(target_os = "linux")]
    fn runs_the_agent(pid: u32) -> bool {
        let cmd = cmdline_of(pid);
        !cmd.contains("trap cleanup TERM INT") && cmd.contains("sleep 30")
    }

    /// Whether SIGTERM is set in the named signal mask of `pid`'s
    /// `/proc/<pid>/status` (`SigCgt`, `SigPnd`, `ShdPnd`, ...). Panics when
    /// the field cannot be read: an absent field must not read as "clear".
    #[cfg(target_os = "linux")]
    fn sigterm_in_mask(pid: u32, field: &str) -> bool {
        signal_mask(pid, field) & (1 << (libc::SIGTERM - 1)) != 0
    }

    /// The named signal mask of `pid`'s `/proc/<pid>/status` (`SigCgt`,
    /// `SigBlk`, ...). Panics when the field cannot be read: an absent field
    /// must not read as "clear".
    #[cfg(target_os = "linux")]
    fn signal_mask(pid: u32, field: &str) -> u64 {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
            .expect("/proc/<pid>/status must be readable for a live child");
        let hex = status
            .lines()
            .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
            .unwrap_or_else(|| panic!("/proc/{pid}/status has no {field} line"))
            .trim();
        u64::from_str_radix(hex, 16).expect("signal masks are hex")
    }

    #[cfg(target_os = "linux")]
    fn sigterm_is_caught(pid: u32) -> bool {
        sigterm_in_mask(pid, "SigCgt")
    }

    /// Whether a TERM has reached `pid` while it was held. The shim blocks
    /// signals for the hold, so the TERM sits pending (`ShdPnd` for a
    /// process-directed `kill`) until the hold ends, and the shim records it.
    #[cfg(target_os = "linux")]
    fn held_process_received_sigterm(held: &HeldMonitor, pid: u32) -> bool {
        held.hold_saw_a_pending_term()
            || sigterm_in_mask(pid, "ShdPnd")
            || sigterm_in_mask(pid, "SigPnd")
    }

    #[cfg(target_os = "linux")]
    fn within_5s(mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..250 {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        done()
    }

    /// Window A. Fails under dash before the fix (bash does not lose the TERM,
    /// so on a bash `/bin/sh` this passes either way).
    #[cfg(target_os = "linux")]
    #[test]
    fn sigterm_before_the_agent_child_resets_its_traps_still_kills_the_agent() {
        let dir = tempfile::tempdir().unwrap();
        let mut held = HeldMonitor::spawn(dir.path(), HoldSide::Child);
        let agent_pid = wait_for_agent_pid(dir.path(), held.state.phase)
            .expect("the monitor records the agent pid right after its fork");
        held.tracked.push(agent_pid);

        // The injection landed where this test claims it did: the agent child
        // is held before its trap reset, still running the monitor's script
        // and still catching TERM with the handler it inherited.
        let held_cmd = cmdline_of(agent_pid);
        assert!(
            held_cmd.contains("trap cleanup TERM INT"),
            "the hold did not land on the agent fork; child already runs [{held_cmd}]"
        );
        assert!(
            sigterm_is_caught(agent_pid),
            "the held agent child does not catch TERM, so it is not in the pre-reset window"
        );

        held.sigterm_monitor();
        held.reap_monitor_expecting_trap_exit();
        // `cleanup` has run, so its `kill` was sent while the child was held.
        // It is pending there, and the handler the child inherited catches it
        // when the hold ends, before the child's trap reset.
        assert!(
            within_5s(|| held_process_received_sigterm(&held, agent_pid)),
            "the held agent child never received the monitor's TERM, so the kill did \
             not land inside the window"
        );

        held.release();
        let died = within_5s(|| !crate::agent::agent_running(agent_pid));
        assert!(
            died,
            "agent (pid {agent_pid}) survived a TERM that reached it before its trap \
             reset, orphaned: {}",
            proc_snapshot(agent_pid)
        );
    }

    /// Opposite-result control for the test above: the same shim and the same
    /// hold, but released BEFORE the TERM, so the TERM finds an exec'd agent.
    /// Passes before and after the fix; if it ever fails, the harness, not the
    /// monitor, is what keeps the agent alive.
    #[cfg(target_os = "linux")]
    #[test]
    fn sigterm_after_a_held_agent_child_execs_kills_the_agent() {
        let dir = tempfile::tempdir().unwrap();
        let mut held = HeldMonitor::spawn(dir.path(), HoldSide::Child);
        let agent_pid = wait_for_agent_pid(dir.path(), held.state.phase)
            .expect("the monitor records the agent pid right after its fork");
        held.tracked.push(agent_pid);
        let held_cmd = cmdline_of(agent_pid);
        assert!(
            held_cmd.contains("trap cleanup TERM INT"),
            "the hold did not land on the agent fork; child already runs [{held_cmd}]"
        );

        held.release();
        assert!(
            within_5s(|| runs_the_agent(agent_pid)),
            "the released agent child never exec'd the agent: [{}]",
            cmdline_of(agent_pid)
        );
        held.sigterm_monitor();
        held.reap_monitor_expecting_trap_exit();

        let died = within_5s(|| !crate::agent::agent_running(agent_pid));
        assert!(
            died,
            "agent (pid {agent_pid}) survived a TERM sent after it exec'd: {}",
            proc_snapshot(agent_pid)
        );
        assert!(
            !held.hold_saw_a_pending_term(),
            "a TERM landed inside the hold; this control must only TERM after exec"
        );
    }

    /// Window F. Fails under dash and bash before the fix.
    #[cfg(target_os = "linux")]
    #[test]
    fn sigterm_before_the_monitor_records_the_agent_pid_still_kills_the_agent() {
        let dir = tempfile::tempdir().unwrap();
        let mut held = HeldMonitor::spawn(dir.path(), HoldSide::Parent);
        // The monitor is held right after its fork, so it has not written the
        // pidfile; the agent is the monitor's only child.
        let children = format!("/proc/{0}/task/{0}/children", held.monitor_pid);
        let mut agent_pid = None;
        let found = within_5s(|| {
            agent_pid = std::fs::read_to_string(&children)
                .expect("/proc/<pid>/task/<tid>/children must be readable")
                .split_whitespace()
                .next()
                .map(|pid| pid.parse::<u32>().expect("children lists numeric pids"));
            agent_pid.is_some()
        });
        assert!(found, "the held monitor never forked its agent");
        let agent_pid = agent_pid.unwrap();
        held.tracked.push(agent_pid);
        assert!(
            within_5s(|| runs_the_agent(agent_pid)),
            "the agent child of the held monitor never exec'd: [{}]",
            cmdline_of(agent_pid)
        );
        let pidfile = crate::agent_result::agent_pid_path(dir.path(), held.state.phase);
        assert!(
            !pidfile.exists(),
            "the monitor already wrote the pidfile, so the hold did not land between \
             its fork and `apid=$!`"
        );

        held.sigterm_monitor();
        let monitor_pid = held.monitor_pid;
        assert!(
            within_5s(|| held_process_received_sigterm(&held, monitor_pid)),
            "the held monitor never received the TERM"
        );
        held.release();
        held.reap_monitor_expecting_trap_exit();

        let died = within_5s(|| !crate::agent::agent_running(agent_pid));
        assert!(
            died,
            "agent (pid {agent_pid}) survived a TERM that reached the monitor before \
             it recorded the agent pid, orphaned: {}",
            proc_snapshot(agent_pid)
        );
    }

    /// The stop marker a TERM'd monitor leaves behind must not stop the NEXT
    /// agent for that phase: the Legacy launch removes a stale marker before
    /// it spawns. Without that, the agent child finds the marker, exits 143
    /// without ever running, and the stage records a kill nobody sent.
    ///
    /// The tail is empty so the monitor does not re-exec this test binary as
    /// `devflow advance`; the marker handling does not depend on it.
    #[test]
    fn a_stale_stop_marker_does_not_stop_the_next_agent() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path());
        let marker = crate::agent_result::stop_marker_path(dir.path(), state.phase);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "\n").unwrap();
        let args = vec![
            "-c".to_string(),
            "echo STALE_MARKER_AGENT_RAN; exit 7".to_string(),
        ];

        spawn_monitor_inner(
            &state,
            "sh",
            &args,
            &[],
            MonitorLaunch::Legacy,
            &test_launch_context(&state),
            MonitorTail::Script(String::new()),
        )
        .unwrap();

        let exit_path = crate::agent_result::exit_code_path(dir.path(), state.phase);
        let mut recorded = None;
        for _ in 0..250 {
            recorded = std::fs::read_to_string(&exit_path)
                .ok()
                .filter(|contents| contents.ends_with('\n'));
            if recorded.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let recorded = recorded.expect("the monitor never recorded the agent's exit code");
        assert_eq!(
            recorded.trim(),
            "7",
            "a stale stop marker stopped the next agent: its exit code is {:?}, not the \
             agent's own 7 (143 means the agent child found the marker and never exec'd)",
            recorded.trim()
        );
        let captured =
            std::fs::read_to_string(crate::agent_result::stdout_path(dir.path(), state.phase))
                .unwrap();
        assert!(
            captured.contains("STALE_MARKER_AGENT_RAN"),
            "the agent did not run under a stale stop marker; captured stdout: {captured:?}"
        );
        assert!(
            !marker.exists(),
            "the stale stop marker survived the launch that should have removed it"
        );
    }

    /// WR-01 (48-REVIEW.md): a TERM that reaches the monitor after its agent
    /// has been reaped — while the `advance` tail runs — must not act on the
    /// agent. The shell defers the trap until the foreground tail returns, and
    /// by then `advance` has launched the next stage's monitor, which removed
    /// the phase's stop marker. A `cleanup` that writes the marker anyway stops
    /// that next agent (exit 143) and `kill`s a pid reaped long ago.
    ///
    /// The tail stands in for `advance`: it announces itself, blocks until
    /// released (capped at 10 s so a failed test cannot leak it), then exits 5.
    /// `kill()` queues the TERM before it returns and the release comes after,
    /// so the TERM always lands while the tail runs; and the monitor exiting 0
    /// rather than the tail's 5 proves `cleanup` ran rather than being missed.
    #[test]
    fn a_term_after_the_agent_exits_leaves_no_stop_marker() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_in(dir.path());
        let marker = crate::agent_result::stop_marker_path(dir.path(), state.phase);
        let started = dir.path().join("tail-started");
        let release = dir.path().join("tail-release");
        let tail = format!(
            "sh -c 'echo > \"$1\"; i=0; while [ ! -e \"$2\" ] && [ $i -lt 500 ]; do \
             sleep 0.02; i=$((i+1)); done; exit 5' tail {} {}",
            shell_escape(started.to_str().unwrap()),
            shell_escape(release.to_str().unwrap()),
        );
        let args = vec!["-c".to_string(), "exit 7".to_string()];
        let monitor_pid = spawn_monitor_inner(
            &state,
            "sh",
            &args,
            &[],
            MonitorLaunch::Legacy,
            &test_launch_context(&state),
            MonitorTail::Script(tail),
        )
        .unwrap();

        let mut tail_running = false;
        for _ in 0..250 {
            if started.exists() {
                tail_running = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(tail_running, "the monitor never reached its tail");
        let recorded =
            std::fs::read_to_string(crate::agent_result::exit_code_path(dir.path(), state.phase))
                .unwrap();
        assert_eq!(
            recorded.trim(),
            "7",
            "the tail runs only after the agent was reaped and its exit recorded"
        );
        let monitor_cmd = std::fs::read(format!("/proc/{monitor_pid}/cmdline")).unwrap();
        assert!(
            String::from_utf8_lossy(&monitor_cmd).contains("trap cleanup TERM INT"),
            "the monitor shell exec'd its tail, so no trap is left to test"
        );

        let rc = unsafe { libc::kill(monitor_pid as libc::pid_t, libc::SIGTERM) };
        assert_eq!(
            rc,
            0,
            "kill(monitor, TERM): {}",
            std::io::Error::last_os_error()
        );
        std::fs::write(&release, "").unwrap();

        let pid = monitor_pid as libc::pid_t;
        let mut status = None;
        for _ in 0..600 {
            let mut raw: libc::c_int = 0;
            let waited = unsafe { libc::waitpid(pid, &mut raw, libc::WNOHANG) };
            if waited == pid {
                status = Some(raw);
                break;
            }
            assert_eq!(
                waited,
                0,
                "waitpid(monitor): {}",
                std::io::Error::last_os_error()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let status = status.expect("the monitor did not exit within 12s of its release");
        assert!(
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            "the monitor must leave through its trap (`cleanup` → `exit 0`), not finish \
             its tail (exit 5); wait status {status:#x}"
        );
        assert!(
            !marker.exists(),
            "a TERM after the agent was reaped wrote the phase's stop marker, which stops \
             the next stage's agent before it runs"
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

        let monitor_pid = spawn_monitor(
            &state,
            "sh",
            &args,
            &[],
            MonitorLaunch::Legacy,
            &test_launch_context(&state),
        )
        .unwrap();
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

            spawn_monitor(
                &state,
                "sh",
                &args,
                &[],
                MonitorLaunch::Legacy,
                &test_launch_context(&state),
            )
            .unwrap();
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

        spawn_monitor(
            &state,
            "sh",
            &args,
            &[],
            MonitorLaunch::Legacy,
            &test_launch_context(&state),
        )
        .unwrap();
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
    // D-09 (48-09): mutates `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`, `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
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

    // ---- 50-08: both supervision shapes on the 50-07 runtime ----------
    //
    // Each supervisor runs in a child test process (`spawn_test_in_child`),
    // because `run_supervised_stage` installs process-wide signal state.
    // The parent probes the child's socket, signals the child by the pid of
    // the `Child` it owns, and reads the files the child wrote.

    /// The socket the child supervisor publishes.
    const STAGE_SOCKET_ENV: &str = "DEVFLOW_TEST_STAGE_SOCKET";
    /// Where the child records `<outcome> <owner phase>`.
    const STAGE_OUTCOME_ENV: &str = "DEVFLOW_TEST_STAGE_OUTCOME";
    /// `capture` or `stream`.
    const STAGE_SHAPE_ENV: &str = "DEVFLOW_TEST_STAGE_SHAPE";
    /// The fake agent's absolute path.
    const STAGE_PROGRAM_ENV: &str = "DEVFLOW_TEST_STAGE_PROGRAM";
    /// The project root the phase files live under.
    const STAGE_ROOT_ENV: &str = "DEVFLOW_TEST_STAGE_ROOT";
    /// The agent's working directory.
    const STAGE_WORKDIR_ENV: &str = "DEVFLOW_TEST_STAGE_WORKDIR";
    /// The agent kind's label.
    const STAGE_AGENT_ENV: &str = "DEVFLOW_TEST_STAGE_AGENT";
    /// One adapter env pair as `NAME=VALUE`, or empty for none.
    const STAGE_ADAPTER_ENV: &str = "DEVFLOW_TEST_STAGE_ADAPTER_ENV";

    /// The phase every supervised-stage test runs.
    const STAGE_PHASE: PhaseId = PhaseId::new(50);
    /// The stream shape's idle timeout in these tests: far above every wait
    /// below, so it never fires unless a test asks it to.
    const STAGE_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
    /// How long the parent waits for a child supervisor to answer or exit.
    const STAGE_CHILD_WAIT: Duration = Duration::from_secs(30);
    /// The first turn the stream shape delivers.
    const STAGE_PROMPT: &str = "SUPERVISED-STAGE-PROMPT";

    /// One supervision shape, as the tests name it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Shape {
        Capture,
        Stream,
    }

    impl Shape {
        const BOTH: [Shape; 2] = [Shape::Capture, Shape::Stream];

        fn label(self) -> &'static str {
            self.io().label()
        }

        fn io(self) -> StageIo {
            match self {
                Shape::Capture => StageIo::Capture,
                Shape::Stream => StageIo::Stream {
                    prompt: STAGE_PROMPT.to_string(),
                    idle_timeout: STAGE_IDLE_TIMEOUT,
                },
            }
        }

        fn parse(label: &str) -> Self {
            match label {
                "capture" => Shape::Capture,
                "stream" => Shape::Stream,
                other => panic!("unknown shape {other}"),
            }
        }

        /// The other shape, for the label control.
        fn other(self) -> Self {
            match self {
                Shape::Capture => Shape::Stream,
                Shape::Stream => Shape::Capture,
            }
        }

        /// The non-Claude adapter each shape runs in these tests (R-11):
        /// Codex under capture, Antigravity under stream.
        fn adapter(self) -> AgentKind {
            match self {
                Shape::Capture => AgentKind::Codex,
                Shape::Stream => AgentKind::Antigravity,
            }
        }

        /// The agent script's first lines: the stream shape reads its first
        /// turn from stdin before anything else, as a real stream agent does,
        /// and exits 91 or 92 when the turn is missing or lacks the prompt.
        fn preamble(self) -> &'static str {
            match self {
                Shape::Capture => "",
                Shape::Stream => {
                    "IFS= read -r turn || { echo NO_FIRST_TURN >&2; exit 91; }\n\
                     case \"$turn\" in *SUPERVISED-STAGE-PROMPT*) ;; *) echo BAD_TURN >&2; exit 92 ;; esac\n"
                }
            }
        }

        /// The result line the shape's adapter prints on stdout.
        fn result_line(self) -> &'static str {
            match self {
                Shape::Capture => r#"DEVFLOW_RESULT: {"status":"success"}"#,
                Shape::Stream => ANTG_RESULT_MARKER_LINE,
            }
        }
    }

    /// The stderr line every waiting agent prints.
    const STAGE_STDERR_LINE: &str = "STAGE-STDERR-LINE";

    /// A fake agent for `shape`, named after its adapter: it reads its first
    /// turn (stream), prints the shape's result line on stdout and
    /// [`STAGE_STDERR_LINE`] on stderr, then waits for `go` and exits `code`.
    fn waiting_agent(fixture: &StageFixture, shape: Shape, go: &Path, code: i32) -> PathBuf {
        fixture.agent(
            &shape.adapter().to_string(),
            &format!(
                "{preamble}printf '%s\\n' '{result}'\n\
                 echo '{STAGE_STDERR_LINE}' >&2\n\
                 while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                 exit {code}",
                preamble = shape.preamble(),
                result = shape.result_line(),
                go = go.display(),
            ),
        )
    }

    /// The child half of every supervised-stage test: build the spec from
    /// the parent's env, run the stage, record `<outcome> <owner>`, finish.
    fn supervise_stage_in_child() {
        let var =
            |name: &str| std::env::var_os(name).unwrap_or_else(|| panic!("the parent sets {name}"));
        let socket =
            SocketPath::new(PathBuf::from(var(STAGE_SOCKET_ENV))).expect("socket path fits");
        let launch_id = socket
            .as_path()
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(|stem| stem.rsplit('-').next())
            .expect("the socket name ends in its launch id")
            .to_string();
        let shape = Shape::parse(var(STAGE_SHAPE_ENV).to_str().expect("utf-8 shape"));
        let agent: AgentKind = var(STAGE_AGENT_ENV)
            .to_str()
            .expect("utf-8 agent")
            .parse()
            .expect("a known agent label");
        let program = var(STAGE_PROGRAM_ENV)
            .into_string()
            .expect("utf-8 program path");
        let mut spec = StageSpec::new(
            PathBuf::from(var(STAGE_ROOT_ENV)),
            STAGE_PHASE,
            agent,
            program,
            shape.io(),
            socket,
            launch_id,
        );
        spec.workdir = PathBuf::from(var(STAGE_WORKDIR_ENV));
        spec.stage = Some(Stage::Code);
        let adapter_env = var(STAGE_ADAPTER_ENV)
            .into_string()
            .expect("utf-8 adapter env");
        if let Some((name, value)) = adapter_env.split_once('=') {
            spec.envs.push((name.to_string(), value.to_string()));
        }
        let run = run_supervised_stage(spec).expect("supervise the stage");
        let record = format!(
            "{:?} {}",
            run.outcome,
            run.supervisor.token().phase().as_str()
        );
        run.supervisor.finish();
        std::fs::write(var(STAGE_OUTCOME_ENV), record).expect("record the outcome");
    }

    /// A short tempdir under `/tmp` holding the cache, the project root and
    /// the agent's separate working directory (`<root>/wt`).
    struct StageFixture {
        cache: tempfile::TempDir,
        root: PathBuf,
        workdir: PathBuf,
    }

    impl StageFixture {
        fn new() -> Self {
            let cache = tempfile::Builder::new()
                .prefix("dfs")
                .tempdir_in("/tmp")
                .expect("create a short cache dir under /tmp");
            let root = cache.path().join("proj");
            let workdir = root.join("wt");
            std::fs::create_dir_all(&workdir).expect("create the project and its worktree");
            Self {
                cache,
                root,
                workdir,
            }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.cache.path().join(name)
        }

        /// Write an executable `#!/bin/sh` fake agent named `name`.
        fn agent(&self, name: &str, body: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.path(name);
            std::fs::write(
                &path,
                format!("#!/bin/sh\nPATH=/usr/bin:/bin\nexport PATH\n{body}\n"),
            )
            .expect("write the fake agent");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make the fake agent executable");
            path
        }

        fn read(&self, path: PathBuf) -> String {
            std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
        }

        fn stdout(&self) -> String {
            self.read(crate::agent_result::stdout_path(&self.root, STAGE_PHASE))
        }

        fn stderr(&self) -> String {
            self.read(crate::agent_result::stderr_path(&self.root, STAGE_PHASE))
        }

        fn exit_code(&self) -> String {
            self.read(crate::agent_result::exit_code_path(&self.root, STAGE_PHASE))
                .trim()
                .to_string()
        }

        fn pid_file(&self) -> String {
            self.read(crate::agent_result::agent_pid_path(&self.root, STAGE_PHASE))
                .trim()
                .to_string()
        }
    }

    /// Polls `done` every 20 ms until it holds or `within` elapses.
    fn stage_wait_until(within: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + within;
        loop {
            if done() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Kills and reaps the child test process on every exit path. Only the
    /// pid of the `Child` this test spawned is ever signalled.
    struct StageChild(Option<std::process::Child>);

    impl StageChild {
        fn id(&self) -> u32 {
            self.0.as_ref().expect("the child is still owned").id()
        }

        /// Send `signal` to the child, by the pid of the `Child` this test
        /// owns.
        fn signal(&self, signal: libc::c_int) {
            let pid = libc::pid_t::try_from(self.id()).expect("pid fits pid_t");
            assert!(pid > 1, "refusing to signal pid {pid}");
            // SAFETY: `pid > 1` names one process this test spawned and still
            // owns unreaped.
            let rc = unsafe { libc::kill(pid, signal) };
            assert_eq!(rc, 0, "signal {signal} to the child supervisor {pid}");
        }

        /// SIGKILL the child (by the pid of the `Child` this test owns) and
        /// reap it.
        fn kill_now(&mut self) {
            let mut child = self.0.take().expect("the child is still owned");
            child.kill().expect("SIGKILL the child supervisor");
            child.wait().expect("reap the child supervisor");
        }

        fn try_exited(&mut self) -> bool {
            self.0
                .as_mut()
                .expect("the child is still owned")
                .try_wait()
                .expect("poll the child")
                .is_some()
        }

        /// Wait for the child to exit on its own and collect its output.
        fn wait_exit(&mut self) -> std::process::Output {
            let exited = stage_wait_until(STAGE_CHILD_WAIT, || self.try_exited());
            let child = self.0.take().expect("the child is still owned");
            if !exited {
                let mut child = StageChild(Some(child));
                let _ = child.0.as_mut().expect("owned").kill();
                let output = child.0.take().expect("owned").wait_with_output();
                panic!("the child supervisor did not exit within {STAGE_CHILD_WAIT:?}: {output:?}");
            }
            child
                .wait_with_output()
                .expect("collect the child's output")
        }
    }

    impl Drop for StageChild {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// SIGKILLs whatever is left of the agent's process group on drop.
    struct StageGroup(u32);

    impl Drop for StageGroup {
        fn drop(&mut self) {
            let Ok(signed) = libc::pid_t::try_from(self.0) else {
                return;
            };
            // SAFETY: getpgrp has no preconditions.
            if signed > 1 && signed != unsafe { libc::getpgrp() } {
                // SAFETY: `signed > 1` and not our own group.
                unsafe { libc::kill(-signed, libc::SIGKILL) };
            }
        }
    }

    /// A child supervisor the parent launched, once its socket answered.
    struct StageLaunch {
        child: StageChild,
        group: StageGroup,
        socket: SocketPath,
        outcome: PathBuf,
        reply: crate::supervisor::protocol::PingReply,
        sidecar: crate::supervisor::Sidecar,
    }

    impl StageLaunch {
        fn agent_pid(&self) -> u32 {
            self.reply.agent_pid
        }

        fn liveness(&self) -> crate::supervisor::Liveness {
            crate::supervisor::probe(self.socket.as_path()).expect("probe the socket")
        }

        /// Wait for the child to exit, require that it ran exactly its one
        /// test, and return the outcome it recorded.
        fn finish(&mut self, test_name: &str) -> String {
            let output = self.child.wait_exit();
            crate::test_support::assert_child_ran_exactly_one_passing_test(&output, test_name);
            std::fs::read_to_string(&self.outcome).expect("the child recorded its outcome")
        }
    }

    /// Re-run `test_name` as a child supervisor of `program` in `shape`, and
    /// wait until its socket answers a ping. At that first Alive the sidecar
    /// must already be readable (R-7: the sidecar precedes the socket name).
    fn launch_stage(
        test_name: &str,
        fixture: &StageFixture,
        shape: Shape,
        agent: AgentKind,
        program: &Path,
        adapter_env: &str,
    ) -> StageLaunch {
        let socket = SocketPath::for_launch(
            fixture.cache.path(),
            &fixture.root,
            STAGE_PHASE,
            &crate::supervisor::new_launch_id(),
        )
        .expect("socket path fits sun_path");
        let outcome = fixture.path("outcome");
        let agent_label = agent.to_string();
        let mut child = StageChild(Some(crate::test_support::spawn_test_in_child(
            test_name,
            fixture.cache.path(),
            &[
                (STAGE_SOCKET_ENV, socket.as_path().as_os_str()),
                (STAGE_OUTCOME_ENV, outcome.as_os_str()),
                (STAGE_SHAPE_ENV, std::ffi::OsStr::new(shape.label())),
                (STAGE_PROGRAM_ENV, program.as_os_str()),
                (STAGE_ROOT_ENV, fixture.root.as_os_str()),
                (STAGE_WORKDIR_ENV, fixture.workdir.as_os_str()),
                (STAGE_AGENT_ENV, std::ffi::OsStr::new(&agent_label)),
                (STAGE_ADAPTER_ENV, std::ffi::OsStr::new(adapter_env)),
            ],
        )));
        let deadline = std::time::Instant::now() + STAGE_CHILD_WAIT;
        let reply = loop {
            if let Ok(crate::supervisor::Liveness::Alive(reply)) =
                crate::supervisor::probe(socket.as_path())
            {
                break reply;
            }
            let exited = child.try_exited();
            if exited || std::time::Instant::now() >= deadline {
                let output = child.0.take().expect("owned").wait_with_output();
                panic!("the child supervisor never answered a ping (exited: {exited}): {output:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let group = StageGroup(reply.agent_pgid);
        let sidecar = crate::supervisor::read_sidecar(&socket.sidecar_path())
            .expect("the sidecar is readable at the first moment the socket answers (R-7)");
        StageLaunch {
            child,
            group,
            socket,
            outcome,
            reply,
            sidecar,
        }
    }

    const CAPTURE_TRACER_TEST: &str =
        "monitor::tests::capture_arm_runs_the_agent_under_an_addressed_supervisor";

    #[test]
    fn capture_arm_runs_the_agent_under_an_addressed_supervisor() {
        if crate::test_support::in_child_test(CAPTURE_TRACER_TEST) {
            supervise_stage_in_child();
            return;
        }
        let fixture = StageFixture::new();
        let go = fixture.path("go");
        let program = fixture.agent(
            "codex",
            &format!(
                "echo 'DEVFLOW_RESULT: {{\"status\":\"success\"}}'\n\
                 echo 'CAPTURE-STDERR-LINE' >&2\n\
                 while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                 exit 7",
                go = go.display()
            ),
        );
        let mut run = launch_stage(
            CAPTURE_TRACER_TEST,
            &fixture,
            Shape::Capture,
            AgentKind::Codex,
            &program,
            "",
        );
        assert_eq!(run.reply.launch, "capture");
        assert_eq!(run.reply.owner, "agent_running");
        assert_eq!(
            run.reply.agent_pgid,
            run.agent_pid(),
            "the agent leads its group"
        );
        assert_eq!(run.sidecar.launch, "capture");
        assert_eq!(run.sidecar.agent, "codex");
        assert_eq!(run.sidecar.agent_pid, run.agent_pid());
        assert_eq!(run.sidecar.stage.as_deref(), Some("code"));

        std::fs::write(&go, "").expect("let the agent exit");
        assert_eq!(run.finish(CAPTURE_TRACER_TEST), "Completed(7) advancing");

        let stdout = fixture.stdout();
        let stderr = fixture.stderr();
        assert!(stdout.contains("DEVFLOW_RESULT:"), "stdout: {stdout:?}");
        assert!(
            !stdout.contains("CAPTURE-STDERR-LINE"),
            "stdout: {stdout:?}"
        );
        assert!(stderr.contains("CAPTURE-STDERR-LINE"), "stderr: {stderr:?}");
        assert!(!stderr.contains("DEVFLOW_RESULT"), "stderr: {stderr:?}");
        assert_eq!(fixture.pid_file(), run.agent_pid().to_string());
        assert_eq!(fixture.exit_code(), "7");
        assert_eq!(run.liveness(), crate::supervisor::Liveness::Gone);
        assert!(
            !run.socket.sidecar_path().exists(),
            "finish removes the sidecar with the socket"
        );
        drop(run.group);
    }

    const STREAM_TRACER_TEST: &str =
        "monitor::tests::stream_arm_runs_the_agent_under_an_addressed_supervisor";

    #[test]
    fn stream_arm_runs_the_agent_under_an_addressed_supervisor() {
        if crate::test_support::in_child_test(STREAM_TRACER_TEST) {
            supervise_stage_in_child();
            return;
        }
        let fixture = StageFixture::new();
        let go = fixture.path("go");
        let program = waiting_agent(&fixture, Shape::Stream, &go, 0);
        let mut run = launch_stage(
            STREAM_TRACER_TEST,
            &fixture,
            Shape::Stream,
            AgentKind::Antigravity,
            &program,
            "",
        );
        assert_eq!(run.reply.launch, "stream");
        assert_eq!(run.reply.owner, "agent_running");
        assert_eq!(run.sidecar.launch, "stream");
        assert_eq!(run.sidecar.agent, "antigravity");
        assert_eq!(run.sidecar.agent_pid, run.agent_pid());

        std::fs::write(&go, "").expect("let the agent exit");
        let outcome = run.finish(STREAM_TRACER_TEST);
        let stderr = fixture.stderr();
        assert_eq!(
            outcome, "Completed(0) advancing",
            "91/92 mean the first turn was missing or lacked the prompt; stderr: {stderr:?}"
        );
        let stdout = fixture.stdout();
        assert!(
            stdout.lines().any(|line| line == ANTG_RESULT_MARKER_LINE),
            "the capture holds the stream's result line verbatim: {stdout:?}"
        );
        assert!(!stdout.contains(STAGE_STDERR_LINE), "stdout: {stdout:?}");
        assert!(stderr.contains(STAGE_STDERR_LINE), "stderr: {stderr:?}");
        assert_eq!(fixture.pid_file(), run.agent_pid().to_string());
        assert_eq!(fixture.exit_code(), "0");
        assert_eq!(run.liveness(), crate::supervisor::Liveness::Gone);
        drop(run.group);
    }

    /// `Ok` when `agent_pid` leads its own process group and that group is
    /// not `supervisor_pid`'s; the reason otherwise. Uses the production
    /// [`confirm_own_group`] check for the first half.
    fn check_own_group(agent_pid: u32, supervisor_pid: u32) -> Result<(), String> {
        confirm_own_group(agent_pid)
            .map_err(|pgid| format!("agent {agent_pid} is in group {pgid}, not its own"))?;
        let supervisor = libc::pid_t::try_from(supervisor_pid).expect("pid fits pid_t");
        // SAFETY: getpgid has no memory preconditions.
        let supervisor_group = unsafe { libc::getpgid(supervisor) };
        if i64::from(supervisor_group) == i64::from(agent_pid) {
            return Err(format!(
                "agent {agent_pid} shares its group with supervisor {supervisor_pid}"
            ));
        }
        Ok(())
    }

    const OWN_GROUP_TEST: &str = "monitor::tests::both_arms_put_the_agent_in_its_own_group";

    #[test]
    fn both_arms_put_the_agent_in_its_own_group() {
        if crate::test_support::in_child_test(OWN_GROUP_TEST) {
            supervise_stage_in_child();
            return;
        }
        for shape in Shape::BOTH {
            let fixture = StageFixture::new();
            let go = fixture.path("go");
            let program = waiting_agent(&fixture, shape, &go, 0);
            let mut run = launch_stage(
                OWN_GROUP_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            let agent = run.agent_pid();
            check_own_group(agent, run.child.id())
                .unwrap_or_else(|reason| panic!("{shape:?}: {reason}"));
            let agent_signed = libc::pid_t::try_from(agent).expect("pid fits pid_t");
            let child_signed = libc::pid_t::try_from(run.child.id()).expect("pid fits pid_t");
            // SAFETY: getpgid has no memory preconditions.
            let (agent_group, child_group) =
                unsafe { (libc::getpgid(agent_signed), libc::getpgid(child_signed)) };
            assert_eq!(agent_group, agent_signed, "{shape:?}: pgid == pid");
            assert_ne!(
                agent_group, child_group,
                "{shape:?}: the agent's group is not the supervisor's"
            );
            std::fs::write(&go, "").expect("let the agent exit");
            assert_eq!(run.finish(OWN_GROUP_TEST), "Completed(0) advancing");
            drop(run.group);
        }

        // Control: the same check refuses a process spawned without
        // `process_group(0)`, which stays in this test process's group.
        let mut stray = StageChild(Some(
            std::process::Command::new("/bin/sleep")
                .arg("30")
                .stdin(std::process::Stdio::null())
                .spawn()
                .expect("spawn a process in this test's group"),
        ));
        let verdict = check_own_group(stray.id(), std::process::id());
        stray.kill_now();
        assert!(
            verdict.is_err(),
            "control: a process left in the launcher's group must fail the check"
        );
    }

    /// What one (shape, condition) row of V-09 must probe as.
    #[derive(Debug, Clone, Copy)]
    enum ExpectedLiveness {
        Alive(&'static str),
        Stale,
        Gone,
    }

    /// `Ok` when `got` is exactly `expected` (an Alive reply must carry the
    /// expected launch label); the mismatch otherwise.
    fn liveness_row(
        expected: ExpectedLiveness,
        got: &crate::supervisor::Liveness,
    ) -> Result<(), String> {
        use crate::supervisor::Liveness;
        match (expected, got) {
            (ExpectedLiveness::Alive(label), Liveness::Alive(reply)) if reply.launch == label => {
                Ok(())
            }
            (ExpectedLiveness::Stale, Liveness::Stale)
            | (ExpectedLiveness::Gone, Liveness::Gone) => Ok(()),
            _ => Err(format!("expected {expected:?}, got {got:?}")),
        }
    }

    /// The condition a V-09 row puts the supervisor in before probing.
    #[derive(Debug, Clone, Copy)]
    enum Condition {
        /// The agent is still running.
        Running,
        /// The supervisor child was SIGKILLed; its agent may live on.
        SupervisorKilled,
        /// The agent exited and the supervisor finished.
        Finished,
    }

    const LIVENESS_TEST: &str =
        "monitor::tests::both_arms_answer_liveness_identically_for_each_condition";

    /// Launch `shape`, put it in `condition`, and probe its socket.
    fn probe_under(shape: Shape, condition: Condition) -> crate::supervisor::Liveness {
        let fixture = StageFixture::new();
        let go = fixture.path("go");
        let program = waiting_agent(&fixture, shape, &go, 0);
        let mut run = launch_stage(
            LIVENESS_TEST,
            &fixture,
            shape,
            shape.adapter(),
            &program,
            "",
        );
        match condition {
            Condition::Running => {
                let got = run.liveness();
                std::fs::write(&go, "").expect("let the agent exit");
                assert_eq!(run.finish(LIVENESS_TEST), "Completed(0) advancing");
                got
            }
            Condition::SupervisorKilled => {
                // The `Child` the parent spawned, never a pid from disk. The
                // agent outlives it in its own group; `run.group` kills it.
                run.child.kill_now();
                run.liveness()
            }
            Condition::Finished => {
                std::fs::write(&go, "").expect("let the agent exit");
                assert_eq!(run.finish(LIVENESS_TEST), "Completed(0) advancing");
                run.liveness()
            }
        }
    }

    #[test]
    fn both_arms_answer_liveness_identically_for_each_condition() {
        if crate::test_support::in_child_test(LIVENESS_TEST) {
            supervise_stage_in_child();
            return;
        }
        let mut failures = Vec::new();
        for shape in Shape::BOTH {
            for condition in [
                Condition::Running,
                Condition::SupervisorKilled,
                Condition::Finished,
            ] {
                let expected = match condition {
                    Condition::Running => ExpectedLiveness::Alive(shape.label()),
                    Condition::SupervisorKilled => ExpectedLiveness::Stale,
                    Condition::Finished => ExpectedLiveness::Gone,
                };
                let got = probe_under(shape, condition);
                if let Err(reason) = liveness_row(expected, &got) {
                    failures.push(format!("{shape:?} {condition:?}: {reason}"));
                }
                // Control: an Alive reply does not satisfy the other shape's
                // label, so the label column is really checked.
                if let Condition::Running = condition {
                    assert!(
                        liveness_row(ExpectedLiveness::Alive(shape.other().label()), &got).is_err(),
                        "control: {shape:?}'s Alive reply must fail the {:?} label",
                        shape.other()
                    );
                }
            }
        }
        assert!(failures.is_empty(), "rows that probed wrong: {failures:#?}");
    }

    /// Whether any member of process group `pgid` still exists (zombies
    /// included). `EPERM` means a member exists that we may not signal.
    fn stage_group_exists(pgid: u32) -> bool {
        let signed = libc::pid_t::try_from(pgid).expect("pgid fits pid_t");
        assert!(signed > 1, "refusing to probe process group {signed}");
        // SAFETY: `signed > 1`; signal 0 only checks for existence.
        let rc = unsafe { libc::kill(-signed, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    /// A fake agent for `shape` that leaves a second group member running
    /// (`sleep 60`, its stdio detached so it holds no capture pipe), then
    /// waits for a file that never appears. Only a signal ends it.
    fn group_agent(fixture: &StageFixture, shape: Shape) -> PathBuf {
        fixture.agent(
            &shape.adapter().to_string(),
            &format!(
                "{preamble}sleep 60 </dev/null >/dev/null 2>&1 &\n\
                 printf '%s\\n' '{result}'\n\
                 while [ ! -e '{never}' ]; do sleep 0.05; done\n\
                 exit 0",
                preamble = shape.preamble(),
                result = shape.result_line(),
                never = fixture.path("never").display(),
            ),
        )
    }

    const STOP_SIGNAL_TEST: &str = "monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance";

    /// V-16. Do NOT widen the 5 s windows below to make this pass: a missed
    /// TERM or INT is a defect, not slowness.
    #[test]
    fn sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance() {
        if crate::test_support::in_child_test(STOP_SIGNAL_TEST) {
            supervise_stage_in_child();
            return;
        }
        for shape in Shape::BOTH {
            for (name, signal) in [("SIGTERM", libc::SIGTERM), ("SIGINT", libc::SIGINT)] {
                let fixture = StageFixture::new();
                let program = group_agent(&fixture, shape);
                let mut run = launch_stage(
                    STOP_SIGNAL_TEST,
                    &fixture,
                    shape,
                    shape.adapter(),
                    &program,
                    "",
                );
                let pgid = run.reply.agent_pgid;
                assert!(
                    stage_group_exists(pgid),
                    "{shape:?} {name}: control: the agent group runs before the signal"
                );

                // The supervisor child's own pid, from the `Child` we own.
                run.child.signal(signal);
                let outcome = run.finish(STOP_SIGNAL_TEST);
                assert_eq!(
                    outcome, "Stopped finished",
                    "{shape:?} {name}: a signal to the supervisor is a stop, never a completion"
                );
                assert!(
                    stage_wait_until(Duration::from_secs(5), || !stage_group_exists(pgid)),
                    "{shape:?} {name}: every member of the agent group must be gone: {}",
                    proc_snapshot(run.agent_pid())
                );
                assert_eq!(fixture.exit_code(), "143", "{shape:?} {name}");
                assert_eq!(
                    run.liveness(),
                    crate::supervisor::Liveness::Gone,
                    "{shape:?} {name}: the socket is gone after the stop"
                );
                drop(run.group);
            }

            // Control: SIGTERM straight to the agent, not the supervisor. The
            // agent dies of it (the test observes a real death), and the
            // supervisor reports that as a natural completion that advances.
            let fixture = StageFixture::new();
            let program = group_agent(&fixture, shape);
            let mut run = launch_stage(
                STOP_SIGNAL_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            let agent = libc::pid_t::try_from(run.agent_pid()).expect("pid fits pid_t");
            assert!(agent > 1, "refusing to signal pid {agent}");
            // SAFETY: `agent > 1` is the live agent the child supervisor
            // still holds unreaped (it is waiting on it).
            assert_eq!(unsafe { libc::kill(agent, libc::SIGTERM) }, 0);
            assert_eq!(
                run.finish(STOP_SIGNAL_TEST),
                "Completed(143) advancing",
                "{shape:?} control: a TERM to the agent alone is not a supervisor stop"
            );
            assert_eq!(fixture.exit_code(), "143", "{shape:?} control");
            drop(run.group);
        }
    }

    /// One checker per supervisor responsibility of the spike's R-A..R-M
    /// table (V-08), each returning the violation it found. Linux-only:
    /// they read `/proc`.
    #[cfg(target_os = "linux")]
    mod responsibilities {
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        /// R-A: the agent's cwd is the worktree, not the project root.
        pub(super) fn r_a_cwd(agent_pid: u32, workdir: &Path) -> Result<(), String> {
            let cwd = std::fs::read_link(format!("/proc/{agent_pid}/cwd"))
                .map_err(|err| format!("agent cwd unreadable: {err}"))?;
            let want = workdir.canonicalize().map_err(|err| err.to_string())?;
            (cwd == want)
                .then_some(())
                .ok_or_else(|| format!("agent cwd {} is not {}", cwd.display(), want.display()))
        }

        /// R-B: stdout and stderr land in separate files, and stderr never
        /// reaches the stdout capture.
        pub(super) fn r_b_streams(
            stdout: &str,
            stderr: &str,
            result_line: &str,
            stderr_line: &str,
        ) -> Result<(), String> {
            if !stdout.lines().any(|line| line == result_line) {
                return Err(format!("stdout lacks the result line: {stdout:?}"));
            }
            if stdout.contains(stderr_line) {
                return Err(format!("stderr text reached stdout: {stdout:?}"));
            }
            if !stderr.contains(stderr_line) {
                return Err(format!("stderr lacks its line: {stderr:?}"));
            }
            Ok(())
        }

        /// R-C: the pid file names the agent, and that pid runs the adapter
        /// program, not the supervisor.
        pub(super) fn r_c_pid(
            pid_file: &str,
            agent_pid: u32,
            program: &Path,
        ) -> Result<(), String> {
            if pid_file != agent_pid.to_string() {
                return Err(format!("pid file {pid_file:?} is not agent {agent_pid}"));
            }
            let cmdline = std::fs::read(format!("/proc/{agent_pid}/cmdline"))
                .map_err(|err| format!("agent cmdline unreadable: {err}"))?;
            let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
            cmdline
                .contains(&*program.to_string_lossy())
                .then_some(())
                .ok_or_else(|| format!("agent {agent_pid} runs {cmdline:?}, not the adapter"))
        }

        /// R-D: the exit file holds the exact code (`128 + n` for signal n).
        pub(super) fn r_d_exit(exit_file: &str, code: i32) -> Result<(), String> {
            (exit_file == code.to_string())
                .then_some(())
                .ok_or_else(|| format!("exit file {exit_file:?}, want {code}"))
        }

        /// R-E: natural completion is `Completed(code)` and the run is left
        /// advancing.
        pub(super) fn r_e_completes(outcome: &str, code: i32) -> Result<(), String> {
            (outcome == format!("Completed({code}) advancing"))
                .then_some(())
                .ok_or_else(|| format!("outcome {outcome:?} is not a completion with {code}"))
        }

        /// R-F: the adapter variable reached a grandchild of the agent.
        pub(super) fn r_f_env(recorded: Option<&str>, value: &str) -> Result<(), String> {
            (recorded == Some(value))
                .then_some(())
                .ok_or_else(|| format!("grandchild saw {recorded:?}, want {value:?}"))
        }

        /// R-J: a sweep for the root and phase finds the live launch with no
        /// state file on disk.
        pub(super) fn r_j_sweep(
            cache: &Path,
            root: &Path,
            phase: crate::phase_id::PhaseId,
            launch_id: &str,
        ) -> Result<(), String> {
            if crate::workflow::state_path(root, phase).exists() {
                return Err("a state file exists, so this does not test the no-state sweep".into());
            }
            let found = crate::supervisor::sweep_for_in(cache, root, phase);
            let ours: Vec<_> = found
                .iter()
                .filter(|entry| {
                    entry
                        .sidecar
                        .as_ref()
                        .is_some_and(|sidecar| sidecar.launch_id == launch_id)
                })
                .collect();
            match ours.as_slice() {
                [entry] if matches!(entry.liveness, Ok(crate::supervisor::Liveness::Alive(_))) => {
                    Ok(())
                }
                _ => Err(format!("sweep found {found:?}, want our one Alive launch")),
            }
        }

        /// R-K: a stopped run still records an exit code, 143.
        pub(super) fn r_k_stop_exit(exit_file: &str) -> Result<(), String> {
            r_d_exit(exit_file, 143)
        }

        /// R-L: the socket is 0600 and the socket directory 0700.
        pub(super) fn r_l_modes(socket: &Path) -> Result<(), String> {
            let mode = |path: &Path| {
                std::fs::metadata(path)
                    .map(|meta| meta.permissions().mode() & 0o777)
                    .map_err(|err| format!("{}: {err}", path.display()))
            };
            let socket_mode = mode(socket)?;
            let dir_mode = mode(socket.parent().ok_or("socket has no parent")?)?;
            (socket_mode == 0o600 && dir_mode == 0o700)
                .then_some(())
                .ok_or_else(|| format!("socket {socket_mode:o}, sock/ {dir_mode:o}"))
        }

        /// R-M: a stop is not a completion: the run ended `Stopped` and
        /// finished, so nothing advances.
        pub(super) fn r_m_stop_is_not_completion(outcome: &str) -> Result<(), String> {
            (outcome == "Stopped finished")
                .then_some(())
                .ok_or_else(|| format!("a stopped run reported {outcome:?}"))
        }

        /// The agent was handed no blocked signal: its leader (a shell, which
        /// blocks SIGCHLD itself while it waits) has no TERM or INT bit in
        /// `SigBlk`, and a child the shell runs, which inherits the mask the
        /// agent was handed, has an empty `SigBlk`.
        pub(super) fn sigblk_clear(agent_pid: u32, grandchild_pid: u32) -> Result<(), String> {
            let stop_bits = (1_u64 << (libc::SIGTERM - 1)) | (1_u64 << (libc::SIGINT - 1));
            let leader = super::signal_mask(agent_pid, "SigBlk");
            let inherited = super::signal_mask(grandchild_pid, "SigBlk");
            (leader & stop_bits == 0 && inherited == 0)
                .then_some(())
                .ok_or_else(|| format!("SigBlk leader {leader:016x}, inherited {inherited:016x}"))
        }
    }

    #[cfg(target_os = "linux")]
    const RESPONSIBILITIES_TEST: &str =
        "monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter";

    /// The adapter variable R-F threads: one `hermetic_command` scrubs, so
    /// it only reaches the agent when `envs` is applied after the scrub.
    #[cfg(target_os = "linux")]
    const R_F_VARIABLE: &str = "GIT_CONFIG_COUNT";
    #[cfg(target_os = "linux")]
    const R_F_VALUE: &str = "1";

    /// V-08 (R-11): the spike's responsibilities under both shapes, with a
    /// fake `codex` under capture and a fake `antigravity` under stream.
    /// R-G (detached from the spawning CLI) is proved in 50-11, where
    /// `spawn_monitor` gives the supervisor its own group.
    #[cfg(target_os = "linux")]
    #[test]
    fn both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter() {
        use responsibilities as r;
        if crate::test_support::in_child_test(RESPONSIBILITIES_TEST) {
            supervise_stage_in_child();
            return;
        }
        let mut rows: Vec<(Shape, &str, Result<(), String>)> = Vec::new();
        let adapter_env = format!("{R_F_VARIABLE}={R_F_VALUE}");
        for shape in Shape::BOTH {
            // Run 1: a natural exit 7, observed alive first.
            let fixture = StageFixture::new();
            let (go, env_out, gc_pid) = (
                fixture.path("go"),
                fixture.path("grandchild-env"),
                fixture.path("grandchild-pid"),
            );
            let program = fixture.agent(
                &shape.adapter().to_string(),
                &format!(
                    "{preamble}sh -c 'printf %s \"${R_F_VARIABLE}\" > \"$1\"; exec sleep 60' sh '{env_out}' </dev/null >/dev/null 2>&1 &\n\
                     echo $! > '{gc_pid}'\n\
                     printf '%s\\n' '{result}'\n\
                     echo '{STAGE_STDERR_LINE}' >&2\n\
                     while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                     kill \"$(cat '{gc_pid}')\"\n\
                     exit 7",
                    preamble = shape.preamble(),
                    result = shape.result_line(),
                    env_out = env_out.display(),
                    gc_pid = gc_pid.display(),
                    go = go.display(),
                ),
            );
            let mut run = launch_stage(
                RESPONSIBILITIES_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                &adapter_env,
            );
            let agent = run.agent_pid();
            assert!(
                stage_wait_until(Duration::from_secs(5), || env_out.exists()
                    && gc_pid.exists()),
                "{shape:?}: the agent's grandchild started"
            );
            let grandchild: u32 = fixture
                .read(gc_pid.clone())
                .trim()
                .parse()
                .expect("grandchild pid");
            rows.push((shape, "R-A", r::r_a_cwd(agent, &fixture.workdir)));
            rows.push((
                shape,
                "R-C",
                r::r_c_pid(&fixture.pid_file(), agent, &program),
            ));
            rows.push((
                shape,
                "R-F",
                r::r_f_env(std::fs::read_to_string(&env_out).ok().as_deref(), R_F_VALUE),
            ));
            rows.push((
                shape,
                "R-J",
                r::r_j_sweep(
                    fixture.cache.path(),
                    &fixture.root,
                    STAGE_PHASE,
                    &run.sidecar.launch_id,
                ),
            ));
            rows.push((shape, "R-L", r::r_l_modes(run.socket.as_path())));
            rows.push((shape, "SigBlk", r::sigblk_clear(agent, grandchild)));
            std::fs::write(&go, "").expect("let the agent exit");
            let outcome = run.finish(RESPONSIBILITIES_TEST);
            rows.push((
                shape,
                "R-B",
                r::r_b_streams(
                    &fixture.stdout(),
                    &fixture.stderr(),
                    shape.result_line(),
                    STAGE_STDERR_LINE,
                ),
            ));
            rows.push((shape, "R-D exit", r::r_d_exit(&fixture.exit_code(), 7)));
            rows.push((shape, "R-E", r::r_e_completes(&outcome, 7)));
            drop(run.group);

            // Run 2: the agent kills itself with SIGUSR1 (128 + 10). Under
            // capture it first merges stderr into stdout: the R-B control.
            let fixture = StageFixture::new();
            let go = fixture.path("go");
            let merge = if shape == Shape::Capture {
                "exec 2>&1\n"
            } else {
                ""
            };
            let program = fixture.agent(
                &shape.adapter().to_string(),
                &format!(
                    "{preamble}{merge}printf '%s\\n' '{result}'\n\
                     echo '{STAGE_STDERR_LINE}' >&2\n\
                     while [ ! -e '{go}' ]; do sleep 0.05; done\n\
                     kill -USR1 $$\n\
                     sleep 5\n\
                     exit 0",
                    preamble = shape.preamble(),
                    result = shape.result_line(),
                    go = go.display(),
                ),
            );
            let mut run = launch_stage(
                RESPONSIBILITIES_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            std::fs::write(&go, "").expect("let the agent signal itself");
            let outcome = run.finish(RESPONSIBILITIES_TEST);
            let by_signal = 128 + libc::SIGUSR1;
            rows.push((
                shape,
                "R-D signal",
                r::r_d_exit(&fixture.exit_code(), by_signal),
            ));
            rows.push((shape, "R-E signal", r::r_e_completes(&outcome, by_signal)));
            if shape == Shape::Capture {
                assert!(
                    r::r_b_streams(
                        &fixture.stdout(),
                        &fixture.stderr(),
                        shape.result_line(),
                        STAGE_STDERR_LINE
                    )
                    .is_err(),
                    "control: the R-B checker must reject a capture whose agent merged stderr \
                     into stdout: {:?}",
                    fixture.stdout()
                );
            }
            drop(run.group);

            // Run 3: a stop (SIGTERM to the supervisor) still records 143 and
            // is not a completion.
            let fixture = StageFixture::new();
            let program = group_agent(&fixture, shape);
            let mut run = launch_stage(
                RESPONSIBILITIES_TEST,
                &fixture,
                shape,
                shape.adapter(),
                &program,
                "",
            );
            run.child.signal(libc::SIGTERM);
            let outcome = run.finish(RESPONSIBILITIES_TEST);
            rows.push((shape, "R-K", r::r_k_stop_exit(&fixture.exit_code())));
            rows.push((shape, "R-M", r::r_m_stop_is_not_completion(&outcome)));
            drop(run.group);
        }
        let failures: Vec<String> = rows
            .iter()
            .filter_map(|(shape, row, result)| {
                result
                    .as_ref()
                    .err()
                    .map(|reason| format!("{shape:?} {row}: {reason}"))
            })
            .collect();
        assert_eq!(rows.len(), 26, "every row ran: {rows:?}");
        assert!(
            failures.is_empty(),
            "responsibilities not met: {failures:#?}"
        );
    }

    // ---- 50-24: the publish wait and its not-ready teardown -------------

    /// Kills (by the pid of the `Child` this test spawned) and reaps the
    /// child on every exit path. A child the helper already reaped is not
    /// signalled again: `Child::kill` on a reaped child sends nothing.
    struct OwnedChild(std::process::Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// A process group led by `pgid`, whose leader started at `start_time`.
    /// On drop it SIGKILLs the group, and only while the leader still has
    /// that start time, so a recycled pgid is never signalled.
    struct LeaderGroup {
        pgid: u32,
        start_time: u64,
    }

    impl LeaderGroup {
        /// Record the group `pid` leads, by its start time now.
        fn of(pid: u32) -> Self {
            let start_time = crate::agent::process_start_time(pid)
                .unwrap_or_else(|| panic!("read the start time of {pid}"));
            Self {
                pgid: pid,
                start_time,
            }
        }

        fn alive(&self) -> bool {
            crate::agent::is_same_process(self.pgid, self.start_time)
        }
    }

    impl Drop for LeaderGroup {
        fn drop(&mut self) {
            let Ok(signed) = libc::pid_t::try_from(self.pgid) else {
                return;
            };
            // SAFETY: getpgrp has no preconditions.
            if signed > 1 && signed != unsafe { libc::getpgrp() } && self.alive() {
                // SAFETY: `signed > 1`, not our own group, and its leader is
                // still the recorded process.
                unsafe { libc::kill(-signed, libc::SIGKILL) };
            }
        }
    }

    /// Whether no process is left in group `pgid` (`kill(-pgid, 0)` fails
    /// with ESRCH), polled every 20 ms up to `within`.
    fn group_gone_within(pgid: u32, within: Duration) -> bool {
        let signed = libc::pid_t::try_from(pgid).expect("pgid fits pid_t");
        assert!(signed > 1, "refusing to probe group {signed}");
        stage_wait_until(within, || {
            // SAFETY: signal 0 only checks for existence; `signed > 1`.
            let rc = unsafe { libc::kill(-signed, 0) };
            rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        })
    }

    /// The env var naming the file the never-publishing supervisor writes
    /// its sleeper's pid to (read by `sh`, not by Rust).
    const SLEEPER_PID_ENV: &str = "SLEEPER_PID_FILE";

    /// A fake supervisor that never binds: it ignores SIGTERM, starts a
    /// sleeper in its own group with util-linux `setsid` as a background job
    /// (a non-interactive `sh` job is not a group leader, so `setsid` does
    /// not fork and `$!` is the sleeper), records the sleeper's pid, then
    /// becomes `sleep 300`.
    fn never_publishing_supervisor(fixture: &StageFixture) -> PathBuf {
        fixture.agent(
            "never-publishes",
            &format!(
                "trap '' TERM\n\
                 setsid sleep 300 &\n\
                 echo $! > \"${SLEEPER_PID_ENV}.tmp\"\n\
                 mv \"${SLEEPER_PID_ENV}.tmp\" \"${SLEEPER_PID_ENV}\"\n\
                 exec sleep 300"
            ),
        )
    }

    /// Spawn `script` as `spawn_monitor` will spawn a supervisor
    /// (`process_group(0)`), wait up to 2 s for its sleeper's pid, and return
    /// the child and the sleeper's group, both guarded.
    fn spawn_never_publishing(script: &Path, pid_file: &Path) -> (OwnedChild, LeaderGroup) {
        let child = OwnedChild(
            std::process::Command::new(script)
                .env(SLEEPER_PID_ENV, pid_file)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
                .expect("spawn the never-publishing supervisor"),
        );
        assert!(
            stage_wait_until(Duration::from_secs(2), || pid_file.exists()),
            "the supervisor recorded no sleeper pid within 2 s"
        );
        let sleeper: u32 = std::fs::read_to_string(pid_file)
            .expect("read the sleeper pid")
            .trim()
            .parse()
            .expect("the sleeper pid parses");
        let group = LeaderGroup::of(sleeper);
        assert_eq!(
            crate::agent::process_stat_fields(sleeper).map(|stat| stat.pgrp),
            libc::pid_t::try_from(sleeper).ok(),
            "the sleeper leads its own group"
        );
        (child, group)
    }

    #[test]
    fn a_supervisor_that_never_publishes_is_ended_with_its_agent_group() {
        let fixture = StageFixture::new();
        let script = never_publishing_supervisor(&fixture);

        // Control: a SIGKILL of the child's own group alone orphans the
        // sleeper, which leads a group of its own.
        {
            let (mut child, sleeper) =
                spawn_never_publishing(&script, &fixture.path("control-pid"));
            let pid = libc::pid_t::try_from(child.0.id()).expect("pid fits pid_t");
            assert!(pid > 1, "refusing to signal group {pid}");
            // SAFETY: `pid > 1` leads the group this test just spawned and
            // still owns unreaped.
            assert_eq!(unsafe { libc::kill(-pid, libc::SIGKILL) }, 0);
            child.0.wait().expect("reap the control child");
            std::thread::sleep(Duration::from_millis(500));
            assert!(
                sleeper.alive(),
                "control: a bare kill of the child's group must leave its sleeper alive"
            );
            let pgid = sleeper.pgid;
            drop(sleeper);
            assert!(group_gone_within(pgid, Duration::from_secs(2)));
        }

        let (mut child, sleeper) = spawn_never_publishing(&script, &fixture.path("sleeper-pid"));
        let child_pid = child.0.id();
        let socket = SocketPath::for_launch(
            fixture.cache.path(),
            &fixture.root,
            STAGE_PHASE,
            "never-publishes",
        )
        .expect("socket path fits sun_path");
        assert!(
            Duration::from_secs(1) < PUBLISH_WAIT,
            "this test runs on a wait shorter than the production one"
        );
        let started = std::time::Instant::now();
        let result = wait_for_publish(
            &mut child.0,
            &socket,
            "never-publishes",
            Duration::from_secs(1),
        );
        let elapsed = started.elapsed();
        match &result {
            Err(MonitorError::NotReady(text)) => assert!(
                text.contains(&child_pid.to_string()),
                "the NotReady text names the child {child_pid}: {text}"
            ),
            other => panic!("expected NotReady, got {other:?}"),
        }
        if let Err(MonitorError::NotReady(text)) = &result {
            assert!(
                text.contains("1 child process group(s) ended"),
                "the teardown ended exactly the sleeper's group: {text}"
            );
        }
        let bound = Duration::from_secs(1)
            + 2 * crate::agent::TERMINATE_VERIFY_WAIT
            + Duration::from_secs(3);
        assert!(elapsed <= bound, "took {elapsed:?}, bound {bound:?}");
        assert!(
            group_gone_within(sleeper.pgid, Duration::from_secs(2)),
            "the teardown must end the sleeper's group {}",
            sleeper.pgid
        );
        assert!(
            child.0.try_wait().expect("poll the child").is_some(),
            "the helper reaped the child"
        );
    }

    #[test]
    fn proc_stat_parse_reads_ppid_pgid_and_start_time_after_the_last_paren() {
        // comm `(a) b (c)` holds spaces and parentheses; every field the
        // parse could confuse carries a distinct value (session 555, tty 0,
        // itrealvalue 0, vsize 1000).
        let head = "1234 ((a) b (c)) S 4242 777 555 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0";
        let line = format!("{head} 98765 1000 200 18446744073709551615");
        assert_eq!(
            parse_proc_stat_line(&line),
            Some(ChildStat {
                ppid: 4242,
                pgid: 777,
                start_time: 98765,
            })
        );
        assert_eq!(
            parse_proc_stat_line(head),
            None,
            "a line cut off before field 22"
        );

        let own = std::fs::read_to_string("/proc/self/stat").expect("read /proc/self/stat");
        let parsed = parse_proc_stat_line(&own).expect("parse /proc/self/stat");
        assert_eq!(parsed.ppid, std::os::unix::process::parent_id());
        assert_eq!(
            Some(parsed.start_time),
            crate::agent::process_start_time(std::process::id())
        );
        // SAFETY: getpgrp has no preconditions.
        assert_eq!(parsed.pgid, unsafe { libc::getpgrp() });
    }

    // ---- 50-24: run_supervised_stage before it publishes ----------------

    /// The cache dir the publish-wait child entry builds its socket under.
    const PUBLISH_CACHE_ENV: &str = "DEVFLOW_TEST_PUBLISH_CACHE";
    /// The project root the publish-wait child entry supervises.
    const PUBLISH_ROOT_ENV: &str = "DEVFLOW_TEST_PUBLISH_ROOT";
    /// The launch id the publish-wait child entry publishes.
    const PUBLISH_LAUNCH_ENV: &str = "DEVFLOW_TEST_PUBLISH_LAUNCH";
    /// `monitor_test_hold`'s variable, set (never read) by these tests.
    const TEST_HOLD_ENV: &str = "DEVFLOW_TEST_HOLD";
    /// The child entry the publish-wait tests re-run.
    const PUBLISH_ENTRY: &str = "monitor::tests::supervise_a_stage_for_the_publish_wait";

    /// The child half of the publish-wait tests: supervise `sh -c 'exec
    /// sleep 300'` in the capture shape on the spec the parent's env names.
    /// Returns at once without that env, so an `--include-ignored` run is
    /// harmless; panics with the error when `run_supervised_stage` fails.
    #[test]
    #[ignore = "child entry of the 50-24 publish-wait tests; returns at once without its env"]
    fn supervise_a_stage_for_the_publish_wait() {
        let (Some(cache), Some(root), Some(launch_id)) = (
            std::env::var_os(PUBLISH_CACHE_ENV),
            std::env::var_os(PUBLISH_ROOT_ENV),
            std::env::var_os(PUBLISH_LAUNCH_ENV),
        ) else {
            return;
        };
        let launch_id = launch_id.into_string().expect("utf-8 launch id");
        let root = PathBuf::from(root);
        let socket = SocketPath::for_launch(Path::new(&cache), &root, STAGE_PHASE, &launch_id)
            .expect("socket path fits sun_path");
        let mut spec = StageSpec::new(
            root,
            STAGE_PHASE,
            AgentKind::Codex,
            "/bin/sh".to_string(),
            StageIo::Capture,
            socket,
            launch_id,
        );
        spec.args = vec!["-c".to_string(), "exec sleep 300".to_string()];
        spec.stage = Some(Stage::Code);
        let run = run_supervised_stage(spec)
            .unwrap_or_else(|err| panic!("run_supervised_stage failed: {err}"));
        run.supervisor.finish();
    }

    /// Spawn this test binary for [`PUBLISH_ENTRY`] under `process_group(0)`
    /// (the argv shape `run_test_in_child` uses), with its stdout and stderr
    /// in `log`. `hold` sets `DEVFLOW_TEST_HOLD=after_sidecar:<hold>`.
    fn spawn_publish_entry(
        cache: &Path,
        fixture: &StageFixture,
        launch_id: &str,
        hold: Option<&Path>,
        log: &Path,
    ) -> OwnedChild {
        let out = std::fs::File::create(log).expect("create the entry log");
        let err = out.try_clone().expect("share the entry log");
        let mut cmd = std::process::Command::new(
            std::env::current_exe().expect("current_exe for the child entry"),
        );
        cmd.arg(PUBLISH_ENTRY)
            .arg("--exact")
            .arg("--include-ignored")
            .arg("--test-threads=1")
            .env(PUBLISH_CACHE_ENV, cache)
            .env(PUBLISH_ROOT_ENV, &fixture.root)
            .env(PUBLISH_LAUNCH_ENV, launch_id)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .process_group(0);
        match hold {
            Some(path) => cmd.env(TEST_HOLD_ENV, format!("after_sidecar:{}", path.display())),
            None => cmd.env_remove(TEST_HOLD_ENV),
        };
        OwnedChild(cmd.spawn().expect("spawn the publish-wait child entry"))
    }

    /// The agent pid `run_supervised_stage` wrote before it published.
    fn publish_agent_pid(fixture: &StageFixture) -> u32 {
        fixture
            .pid_file()
            .parse()
            .expect("the agent pid file holds a pid")
    }

    #[test]
    fn a_supervisor_that_fails_to_publish_ends_its_agent_group() {
        // Failure direction: `sock/` is a symlink, which publishing refuses.
        let fixture = StageFixture::new();
        let elsewhere = fixture.path("elsewhere");
        std::fs::create_dir(&elsewhere).expect("create the symlink target");
        let bad_cache = fixture.path("bad-cache");
        std::fs::create_dir(&bad_cache).expect("create the bad cache dir");
        std::os::unix::fs::symlink(&elsewhere, bad_cache.join("sock")).expect("plant sock/");
        let launch_id = crate::supervisor::new_launch_id();
        let socket = SocketPath::for_launch(&bad_cache, &fixture.root, STAGE_PHASE, &launch_id)
            .expect("socket path fits sun_path");
        let log = fixture.path("fail.log");
        let mut child = spawn_publish_entry(&bad_cache, &fixture, &launch_id, None, &log);
        let result = wait_for_publish(&mut child.0, &socket, &launch_id, PUBLISH_WAIT);
        let entry_log = std::fs::read_to_string(&log).unwrap_or_default();
        match &result {
            Err(MonitorError::NotReady(text)) => assert!(
                text.contains("exited by itself before publishing"),
                "the child exited before publishing: {text}\n{entry_log}"
            ),
            other => panic!("expected NotReady, got {other:?}\n{entry_log}"),
        }
        let agent = publish_agent_pid(&fixture);
        let survivor = crate::agent::process_start_time(agent).map(|start_time| LeaderGroup {
            pgid: agent,
            start_time,
        });
        assert!(
            stage_wait_until(Duration::from_secs(2), || {
                let signed = libc::pid_t::try_from(agent).expect("pid fits pid_t");
                // SAFETY: signal 0 only checks for existence.
                let rc = unsafe { libc::kill(signed, 0) };
                rc != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            }),
            "no agent survives a publish error: {agent} is alive\n{entry_log}"
        );
        drop(survivor);

        // Control: a plain cache dir publishes, and the helper returns the
        // reply and leaves the supervisor and its agent running.
        let fixture = StageFixture::new();
        let launch_id = crate::supervisor::new_launch_id();
        let socket =
            SocketPath::for_launch(fixture.cache.path(), &fixture.root, STAGE_PHASE, &launch_id)
                .expect("socket path fits sun_path");
        let log = fixture.path("ok.log");
        let mut child = spawn_publish_entry(fixture.cache.path(), &fixture, &launch_id, None, &log);
        let reply = wait_for_publish(&mut child.0, &socket, &launch_id, PUBLISH_WAIT)
            .unwrap_or_else(|err| {
                panic!(
                    "a published supervisor is returned: {err}\n{}",
                    std::fs::read_to_string(&log).unwrap_or_default()
                )
            });
        let agent = LeaderGroup::of(reply.agent_pid);
        assert_eq!(reply.launch_id, launch_id);
        assert_eq!(publish_agent_pid(&fixture), reply.agent_pid);
        assert!(
            crate::agent::is_same_process(reply.agent_pid, reply.agent_start_time),
            "the published agent is alive and is the recorded process"
        );
        assert!(
            child.0.try_wait().expect("poll the child").is_none(),
            "the published supervisor is left running"
        );
        let stopped =
            crate::supervisor::request_shutdown(socket.as_path(), Duration::from_secs(10))
                .expect("request the stop");
        assert!(
            matches!(stopped, crate::supervisor::ShutdownOutcome::Stopped { .. }),
            "the supervisor stops: {stopped:?}"
        );
        assert!(
            exit_within(&mut child.0, Duration::from_secs(5)).is_some(),
            "the child exits within 5 s of the stop"
        );
        let entry_log = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(
            entry_log.contains(&format!("test {PUBLISH_ENTRY} ... ok"))
                && entry_log.contains("1 passed"),
            "the child ran its one entry: {entry_log}"
        );
        assert!(group_gone_within(agent.pgid, Duration::from_secs(2)));
    }

    /// What one killed-before-publishing run left: the wait's text, the
    /// agent's group (guarded), and the launch's socket.
    struct KilledRun {
        text: String,
        agent: LeaderGroup,
        socket: SocketPath,
    }

    /// Launch the child entry parked after its sidecar write, read the
    /// sidecar, let `tamper` change it, SIGKILL the child by its own `Child`
    /// and run the publish wait.
    fn kill_before_publishing(
        fixture: &StageFixture,
        tamper: impl FnOnce(&SocketPath, &crate::supervisor::Sidecar),
    ) -> KilledRun {
        let launch_id = crate::supervisor::new_launch_id();
        let socket =
            SocketPath::for_launch(fixture.cache.path(), &fixture.root, STAGE_PHASE, &launch_id)
                .expect("socket path fits sun_path");
        let hold = fixture.path("never-created");
        let log = fixture.path("killed.log");
        let mut child =
            spawn_publish_entry(fixture.cache.path(), fixture, &launch_id, Some(&hold), &log);
        let mut sidecar = None;
        let parked = stage_wait_until(Duration::from_secs(5), || {
            sidecar = crate::supervisor::read_sidecar(&socket.sidecar_path()).ok();
            sidecar.is_some() || child.0.try_wait().ok().flatten().is_some()
        });
        let entry_log = || std::fs::read_to_string(&log).unwrap_or_default();
        let Some(sidecar) = sidecar.filter(|_| parked) else {
            // End an agent the child may already have started, by the pid
            // file it wrote and the start time read now, before failing.
            let _orphan = std::fs::read_to_string(crate::agent_result::agent_pid_path(
                &fixture.root,
                STAGE_PHASE,
            ))
            .ok()
            .and_then(|pid| pid.trim().parse::<u32>().ok())
            .and_then(|pid| {
                crate::agent::process_start_time(pid).map(|start_time| LeaderGroup {
                    pgid: pid,
                    start_time,
                })
            });
            panic!("no sidecar within 5 s\n{}", entry_log());
        };
        let agent = LeaderGroup {
            pgid: sidecar.agent_pgid,
            start_time: sidecar.agent_start_time,
        };
        assert_eq!(sidecar.launch_id, launch_id);
        assert_eq!(
            sidecar.agent_pgid, sidecar.agent_pid,
            "the agent leads its group"
        );
        assert!(agent.alive(), "the agent runs while the child is parked");
        tamper(&socket, &sidecar);
        child.0.kill().expect("SIGKILL the child by its own Child");
        let result = wait_for_publish(&mut child.0, &socket, &launch_id, PUBLISH_WAIT);
        let text = match result {
            Err(MonitorError::NotReady(text)) => text,
            other => panic!("expected NotReady, got {other:?}\n{}", entry_log()),
        };
        KilledRun {
            text,
            agent,
            socket,
        }
    }

    #[test]
    fn a_supervisor_killed_before_publishing_has_its_agent_group_ended_through_its_sidecar() {
        // Run 1: the sidecar is untouched, so the launcher ends its group.
        let fixture = StageFixture::new();
        let run = kill_before_publishing(&fixture, |_, _| {});
        let pgid = run.agent.pgid;
        assert!(
            run.text
                .contains(&format!("ended the agent group {pgid} its sidecar records")),
            "{}",
            run.text
        );
        assert!(
            group_gone_within(pgid, Duration::from_secs(2)),
            "the recorded agent group {pgid} is ended"
        );
        assert!(!run.socket.as_path().exists(), "no socket is left");
        assert!(!run.socket.sidecar_path().exists(), "no sidecar is left");

        // Run 2, control: without the sidecar nothing names the agent, so it
        // survives the child's SIGKILL.
        let fixture = StageFixture::new();
        let run = kill_before_publishing(&fixture, |socket, _| {
            std::fs::remove_file(socket.sidecar_path()).expect("remove the sidecar");
        });
        assert!(run.text.contains("no sidecar"), "{}", run.text);
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            run.agent.alive(),
            "control: the agent survives without a sidecar"
        );
        let pgid = run.agent.pgid;
        drop(run.agent);
        assert!(group_gone_within(pgid, Duration::from_secs(2)));

        // Run 3, identity control: a recorded start time one tick off is
        // LeaderRecycled, and nothing is signalled.
        let fixture = StageFixture::new();
        let run = kill_before_publishing(&fixture, |socket, sidecar| {
            let wrong = crate::supervisor::Sidecar {
                agent_start_time: sidecar.agent_start_time + 1,
                ..sidecar.clone()
            };
            crate::supervisor::write_sidecar(socket, &wrong).expect("rewrite the sidecar");
        });
        assert!(
            run.text.contains("LeaderRecycled") && run.text.contains("nothing was signalled"),
            "{}",
            run.text
        );
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            run.agent.alive(),
            "identity control: the agent is not signalled"
        );
        let pgid = run.agent.pgid;
        drop(run.agent);
        assert!(group_gone_within(pgid, Duration::from_secs(2)));
    }

    /// Whether non-test `monitor.rs` defines `fn monitor_test_hold(` exactly
    /// once, directly under attribute lines that include `#[cfg(test)]`.
    fn test_hold_guard_verdict(non_test: &str) -> Result<(), String> {
        let lines: Vec<&str> = non_test
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        let definitions: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.trim_start().starts_with("fn monitor_test_hold("))
            .map(|(at, _)| at)
            .collect();
        let [at] = definitions[..] else {
            return Err(format!(
                "expected exactly one `fn monitor_test_hold(` in non-test monitor.rs, found {}; \
                 if the hold is removed on purpose, delete this test with it",
                definitions.len()
            ));
        };
        let guarded = lines[..at]
            .iter()
            .rev()
            .take_while(|line| line.trim_start().starts_with("#["))
            .any(|line| line.trim() == "#[cfg(test)]");
        if guarded {
            Ok(())
        } else {
            Err(
                "`fn monitor_test_hold(` is not directly under `#[cfg(test)]`, so the hold \
                 would ship in release builds; if the hold is removed on purpose, delete this \
                 test with it"
                    .to_string(),
            )
        }
    }

    #[test]
    fn the_test_hold_is_compiled_out_of_non_test_builds() {
        let guarded = "#[cfg(test)]\nfn monitor_test_hold(point: &str) {}\n";
        assert_eq!(test_hold_guard_verdict(guarded), Ok(()));
        let guarded_and_more = "/// Doc.\n#[cfg(test)]\n#[allow(clippy::needless_pass_by_value)]\nfn monitor_test_hold(point: &str) {}\n";
        assert_eq!(test_hold_guard_verdict(guarded_and_more), Ok(()));
        let unguarded = "fn monitor_test_hold(point: &str) {}\n";
        assert!(test_hold_guard_verdict(unguarded).is_err(), "no attribute");
        let wrong_attribute = "#[allow(dead_code)]\nfn monitor_test_hold(point: &str) {}\n";
        assert!(
            test_hold_guard_verdict(wrong_attribute).is_err(),
            "only #[allow(dead_code)]"
        );
        let commented = "// #[cfg(test)]\n// fn monitor_test_hold(point: &str) {}\n";
        assert!(
            test_hold_guard_verdict(commented).is_err(),
            "a commented-out definition is zero definitions"
        );
        let twice = format!("{guarded}{guarded}");
        assert!(test_hold_guard_verdict(&twice).is_err(), "two definitions");

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/monitor.rs");
        let source =
            std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path}: {err}"));
        let lines: Vec<&str> = source.lines().collect();
        let tests_at = lines
            .iter()
            .position(|line| *line == "mod tests {")
            .unwrap_or_else(|| panic!("{path} has no `mod tests {{` line"));
        let non_test = lines[..tests_at].join("\n");
        if let Err(why) = test_hold_guard_verdict(&non_test) {
            panic!("{path}: {why}");
        }
    }

    /// `(process_group(0) calls, wait_for_publish( occurrences)` in `source`
    /// cut at its first `mod tests {` line, with every line whose first
    /// non-blank characters are `//` dropped, so neither comments nor this
    /// module's own literals count.
    fn launch_scan_counts(source: &str) -> (usize, usize) {
        let code: Vec<&str> = source
            .lines()
            .take_while(|line| !line.starts_with("mod tests {"))
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        let code = code.join("\n");
        (
            code.matches("process_group(0)").count(),
            code.matches("wait_for_publish(").count(),
        )
    }

    /// R6-G4, permanent: the agent and the `__monitor` supervisor each lead
    /// their own process group, and `spawn_monitor` waits for the publish
    /// through 50-24's `wait_for_publish` (its definition plus the call).
    #[test]
    fn spawn_monitor_groups_both_processes_and_waits_for_publish() {
        let planted = "fn f() {\n    command.process_group(0);\n    // .process_group(0)\n    \
                       wait_for_publish(&mut child);\n}\nmod tests {\n    process_group(0) \
                       process_group(0) wait_for_publish( wait_for_publish(\n}\n";
        assert_eq!(
            launch_scan_counts(planted),
            (1, 1),
            "planted control: one call and one publish wait count once each; the comment \
             line and the test module do not count"
        );

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/monitor.rs");
        let source =
            std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path}: {err}"));
        let (groups, waits) = launch_scan_counts(&source);
        assert!(
            groups >= 2,
            "{path}: non-test code must start both the agent and the `__monitor` supervisor \
             with `process_group(0)`; found {groups}"
        );
        assert!(
            waits >= 2,
            "{path}: non-test code must define `wait_for_publish(` and call it from \
             `spawn_monitor`; found {waits}"
        );
    }
}
