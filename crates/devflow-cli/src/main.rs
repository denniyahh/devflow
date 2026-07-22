use clap::{Parser, Subcommand};
use devflow_core::agent;
use devflow_core::config::{DEVELOP, FEATURE_PREFIX, GitFlowConfig, capture_retention};
use devflow_core::gates::{self, GateAction, GateResponse, Gates, OpenGate};
use devflow_core::git::GitFlow;
use devflow_core::hooks::{self, HookContext};
use devflow_core::mode::{self, Mode};
use devflow_core::prompt::{self, FixType};
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{
    agent_result, agents, events, history, lock, monitor, outcome_policy, recover, worktree,
};
use devflow_core::{
    agent_result::{AgentStatus, Verdict},
    outcome_policy::Action,
    workflow,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[cfg(test)]
mod test_support;

mod staleness;
use staleness::{enforce_build_staleness, run_git_stdout};

mod preflight;
use preflight::{agent_program, ensure_agent_binary, run_preflight, worktree_writable_roots};

/// A pending gate becomes visually urgent after thirty minutes without an
/// answer. The banner remains visible before and after this threshold.
const GATE_ESCALATION_THRESHOLD_SECS: u64 = 30 * 60;

/// Parse `DEVFLOW_GATE_TIMEOUT_SECS`'s raw value, falling back to 7 days on
/// an absent or unparsable value. Pure (no env access) so it's unit-testable
/// without mutating process-global env.
fn parse_gate_timeout(raw: Option<String>) -> u64 {
    const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
    raw.and_then(|s| s.parse().ok()).unwrap_or(SEVEN_DAYS)
}

/// How long a background gate poll waits for a human response, configurable
/// via `DEVFLOW_GATE_TIMEOUT_SECS` (defaults to 7 days).
fn gate_timeout_secs() -> u64 {
    parse_gate_timeout(std::env::var("DEVFLOW_GATE_TIMEOUT_SECS").ok())
}

/// Parse `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, falling back to 120s. Pure
/// (no env access) so it's unit-testable without mutating process-global env.
fn parse_checkout_lock_timeout(raw: Option<String>) -> std::time::Duration {
    const DEFAULT_SECS: u64 = 120;
    std::time::Duration::from_secs(raw.and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_SECS))
}

/// How long a caller waits out a sibling phase's short critical section on
/// the project-wide checkout lock before giving up, configurable via
/// `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` (defaults to 120s) — generous
/// relative to the seconds the lock is held for, tiny relative to a gate
/// wait.
fn checkout_lock_timeout() -> std::time::Duration {
    parse_checkout_lock_timeout(std::env::var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS").ok())
}

#[derive(Debug, Parser)]
#[command(
    name = "devflow",
    version,
    about = "Agent-agnostic, GSD-native development workflow automation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Begin the workflow for a phase: Define → Plan → Code → Validate → Ship.
    Start {
        /// Phase number to work on.
        #[arg(long)]
        phase: u32,
        /// Agent to launch.
        #[arg(long, default_value = "claude")]
        agent: AgentKind,
        /// Pipeline mode: `auto` runs to Ship unattended; `supervise` gates at Validate.
        #[arg(long)]
        mode: Mode,
        /// Overwrite the feature branch if it already exists.
        #[arg(long)]
        force: bool,
        /// Deprecated: a worktree is now created by default; this flag is a
        /// no-op kept for one release for backward compatibility.
        #[arg(long, hide = true)]
        worktree: bool,
        /// Run the agent directly in the primary checkout instead of an
        /// isolated worktree (not recommended for unattended runs).
        #[arg(long)]
        no_worktree: bool,
        /// Print the pipeline that would run without launching anything.
        #[arg(long)]
        dry_run: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Internal: advance the stage machine after a monitored agent exits.
    #[command(hide = true)]
    Advance {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Phase whose stage machine to advance. Recorded by the monitor at
        /// spawn time so advance never depends on a shared state singleton.
        #[arg(long)]
        phase: Option<u32>,
    },
    /// Resume a phase from its saved stage after a rate limit or infrastructure pause.
    ///
    /// Unlike `start`, this loads the persisted per-phase state and
    /// relaunches its saved stage — it does NOT create a new branch/worktree
    /// or reset the workflow to Define (review consensus #5); agent and mode
    /// come from the saved state.
    Resume {
        /// Phase to resume.
        #[arg(long)]
        phase: u32,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Inspect and answer human gates (the pause points where the workflow
    /// waits for approval).
    Gate {
        #[command(subcommand)]
        action: GateCmd,
    },
    /// Print or follow an agent's captured output for a phase.
    Logs {
        /// Phase to show (defaults to the single active phase, else the
        /// most recently written capture file).
        #[arg(long)]
        phase: Option<u32>,
        /// Keep watching for new output until the agent exits.
        #[arg(long, short = 'f')]
        follow: bool,
        /// Show the agent's stderr capture instead of stdout.
        #[arg(long)]
        stderr: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Show a phase's chronological events and retained attempt evidence.
    History {
        /// Phase to show (defaults to the single active phase).
        phase: Option<u32>,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Run multiple phases concurrently, each in its own worktree + monitor.
    Parallel {
        /// Comma-separated phase numbers, e.g. `7,8`.
        #[arg(long)]
        phases: String,
        /// Comma-separated agents matched positionally to phases (default claude).
        #[arg(long)]
        agents: Option<String>,
        /// Pipeline mode for every phase.
        #[arg(long, default_value = "auto")]
        mode: Mode,
        /// Recreate worktrees if they already exist.
        #[arg(long)]
        force: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Run two agents sequentially on one phase, each in its own worktree.
    ///
    /// Agent A runs first; its work is integrated into `feature/phase-NN`, then
    /// agent B rebases onto the updated base and runs. Rebase conflicts are
    /// surfaced for manual resolution — the worktree boundary is the isolation.
    Sequentagent {
        /// Phase number to work on.
        #[arg(long)]
        phase: u32,
        /// Exactly two comma-separated agents, e.g. `claude,codex`.
        #[arg(long)]
        agents: String,
        /// Recreate agent worktrees/branches if they already exist.
        #[arg(long)]
        force: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Create or refresh a static reference worktree at `.worktrees/reference/`.
    Reference {
        /// Branch to check out (defaults to develop).
        #[arg(long)]
        branch: Option<String>,
        /// Update an existing reference snapshot in place.
        #[arg(long)]
        refresh: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Remove phase worktrees and their feature branches.
    Cleanup {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Also remove the reference worktree and force-remove dirty worktrees.
        #[arg(long)]
        force: bool,
    },
    /// Show current workflow state.
    Status {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// List all feature branches with divergence from develop.
    List {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Recover or inspect stale/abandoned workflow state.
    Recover {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Clean up stale state instead of just inspecting. Only stale
        /// phases are swept; combine with --phase to clear a specific
        /// phase regardless of staleness.
        #[arg(long)]
        clean: bool,
        /// Restrict the command to one phase.
        #[arg(long)]
        phase: Option<u32>,
    },
    /// Run local quality checks: cargo test, clippy, and fmt --check.
    Test {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Audit the environment and report what's installed, missing, or broken.
    Doctor {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Project root (optional — doctor works without a project too).
        #[arg(default_value = ".")]
        project: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum GateCmd {
    /// List gates awaiting a response.
    List {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Approve an open gate — the workflow advances.
    Approve {
        /// Phase whose gate to approve.
        phase: u32,
        /// Optional stage or legacy project path (`approve 15 ship` or
        /// `approve 15 /repo`).
        #[arg(value_name = "STAGE_OR_PROJECT")]
        stage: Option<String>,
        /// Legacy positional project path when a stage precedes it.
        #[arg(value_name = "PROJECT")]
        legacy_project: Option<PathBuf>,
        /// Stage of the gate (auto-resolved when the phase has exactly one
        /// open gate).
        #[arg(long = "stage")]
        stage_option: Option<Stage>,
        /// Optional free-text note recorded with the approval.
        #[arg(long)]
        note: Option<String>,
        /// Project root.
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Reject an open gate — loops back to Code, or aborts the phase when
    /// the note contains "abort".
    Reject {
        /// Phase whose gate to reject.
        phase: u32,
        /// Optional stage or legacy project path (`reject 15 ship` or
        /// `reject 15 /repo`).
        #[arg(value_name = "STAGE_OR_PROJECT")]
        stage: Option<String>,
        /// Legacy positional project path when a stage precedes it.
        #[arg(value_name = "PROJECT")]
        legacy_project: Option<PathBuf>,
        /// Stage of the gate (auto-resolved when the phase has exactly one
        /// open gate).
        #[arg(long = "stage")]
        stage_option: Option<Stage>,
        /// Required note explaining the rejection (include "abort" to end
        /// the phase instead of looping back to Code).
        #[arg(long)]
        note: String,
        /// Project root.
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    #[error(transparent)]
    Workflow(#[from] devflow_core::workflow::WorkflowError),
    #[error(transparent)]
    Recover(#[from] devflow_core::recover::RecoverError),
    #[error(transparent)]
    Git(#[from] devflow_core::git::GitError),
    #[error(transparent)]
    Worktree(#[from] devflow_core::worktree::WorktreeError),
    #[error(transparent)]
    Gate(#[from] devflow_core::gates::GateError),
    #[error(transparent)]
    Ship(#[from] devflow_core::ship::ShipError),
    #[error("{0}")]
    Message(String),
}

fn main() {
    match std::env::var("DEVFLOW_LOG_FORMAT").as_deref() {
        Ok("json") => {
            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
            tracing_subscriber::fmt()
                .json()
                .with_writer(std::io::stderr)
                .with_env_filter(filter)
                .init();
        }
        _ => {
            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_env_filter(filter)
                .init();
        }
    }
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    match cli.command {
        Command::Start {
            phase,
            agent,
            mode,
            force,
            worktree: _worktree,
            no_worktree,
            dry_run,
            project,
        } => {
            // Worktree is now the default; the deprecated `--worktree` flag is
            // an intentionally ignored no-op (see field doc comment above).
            // `--no-worktree` is the only switch that changes behavior.
            let worktree = !no_worktree;
            start(
                &project_root(project)?,
                phase,
                agent,
                mode,
                force,
                worktree,
                dry_run,
            )
        }
        Command::Advance { project, phase } => advance(&project_root(project)?, phase),
        Command::Resume { phase, project } => resume(&project_root(project)?, phase),
        Command::Gate { action } => match action {
            GateCmd::List { project } => gate_list(&project_root(project)?),
            GateCmd::Approve {
                phase,
                stage,
                legacy_project,
                stage_option,
                note,
                project,
            } => {
                let (stage, project) =
                    resolve_gate_target(stage, legacy_project, stage_option, project)?;
                gate_respond(&project_root(project)?, phase, stage, true, note)
            }
            GateCmd::Reject {
                phase,
                stage,
                legacy_project,
                stage_option,
                note,
                project,
            } => {
                let (stage, project) =
                    resolve_gate_target(stage, legacy_project, stage_option, project)?;
                gate_respond(&project_root(project)?, phase, stage, false, Some(note))
            }
        },
        Command::Logs {
            phase,
            follow,
            stderr,
            project,
        } => logs(&project_root(project)?, phase, follow, stderr),
        Command::History { phase, project } => history_cmd(&project_root(project)?, phase),
        Command::Parallel {
            phases,
            agents,
            mode,
            force,
            project,
        } => parallel(
            &project_root(project)?,
            &phases,
            agents.as_deref(),
            mode,
            force,
        ),
        Command::Sequentagent {
            phase,
            agents,
            force,
            project,
        } => sequentagent(&project_root(project)?, phase, &agents, force),
        Command::Reference {
            branch,
            refresh,
            project,
        } => reference(&project_root(project)?, branch, refresh),
        Command::Cleanup { project, force } => cleanup(&project_root(project)?, force),
        Command::Status { project } => status(&project_root(project)?),
        Command::List { project } => list(&project_root(project)?),
        Command::Recover {
            project,
            clean,
            phase,
        } => recover_cmd(&project_root(project)?, clean, phase),
        Command::Test { project } => test_cmd(&project_root(project)?),
        Command::Doctor { json, project } => doctor(&project_root(project)?, json),
    }
}

fn resolve_gate_target(
    positional: Option<String>,
    legacy_project: Option<PathBuf>,
    stage_option: Option<Stage>,
    project: PathBuf,
) -> Result<(Option<Stage>, PathBuf), CliError> {
    let Some(positional) = positional else {
        return Ok((stage_option, project));
    };
    if let Ok(positional_stage) = positional.parse::<Stage>() {
        if let Some(flagged_stage) = stage_option
            && flagged_stage != positional_stage
        {
            return Err(CliError::Message(format!(
                "conflicting stages: positional {positional_stage} and --stage {flagged_stage}"
            )));
        }
        let target = legacy_project.unwrap_or(project);
        return Ok((Some(stage_option.unwrap_or(positional_stage)), target));
    }
    if legacy_project.is_some() {
        return Err(CliError::Message(format!(
            "unsupported stage `{positional}`; expected define, plan, code, validate, or ship"
        )));
    }
    if project.as_path() != Path::new(".") {
        return Err(CliError::Message(
            "project was supplied both positionally and with --project".into(),
        ));
    }
    Ok((stage_option, PathBuf::from(positional)))
}

// ---------------------------------------------------------------------------
// start / pipeline driving
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
/// Whether phase `{NN}`'s GSD planning artifact (a `.planning/phases/{NN}-*/`
/// file ending in `suffix`, e.g. `-CONTEXT.md`) exists on `develop` — the
/// branch phase worktrees fork from. Fail-open on git errors (missing
/// develop, not a repo): pre-flight must never block a run the later, more
/// specific checks would allow.
pub(crate) fn phase_artifact_on_develop(project_root: &Path, phase: u32, suffix: &str) -> bool {
    let prefix = format!(".planning/phases/{phase:02}-");
    let output = std::process::Command::new("git")
        .args([
            "ls-tree",
            "-r",
            "--name-only",
            "develop",
            "--",
            ".planning/phases/",
        ])
        .current_dir(project_root)
        .output();
    let Ok(out) = output else { return true };
    if !out.status.success() {
        return true;
    }
    String::from_utf8_lossy(&out.stdout).lines().any(|path| {
        path.strip_prefix(&prefix)
            .is_some_and(|rest| rest.contains('/') && rest.ends_with(suffix))
    })
}

fn start(
    project_root: &Path,
    phase: u32,
    agent: AgentKind,
    mode: Mode,
    force: bool,
    worktree: bool,
    dry_run: bool,
) -> Result<(), CliError> {
    let mut state = State::new(phase, agent, mode, project_root.to_path_buf());

    if dry_run {
        print_dry_run(&state);
        return Ok(());
    }

    // 14-CR-05: fail on a missing agent binary BEFORE any branch/worktree is
    // scaffolded (launch_stage re-checks for the advance-time launch paths).
    ensure_agent_binary(agent_program(agent))?;

    // 13-06 dogfood pre-flight (Codex leg): a fresh headless Codex run can
    // never pass Define — GSD's discuss-phase is an interview, and Codex's
    // exec mode cannot answer it (`request_user_input is unavailable in
    // Default mode`). Fail in one second with instructions instead of after
    // a burned agent run and a dead-end gate. Checked on `develop` (the
    // branch worktrees fork from), so the result does not depend on what the
    // primary checkout happens to have checked out.
    if agent == AgentKind::Codex {
        if !phase_artifact_on_develop(project_root, phase, "-CONTEXT.md") {
            return Err(CliError::Message(format!(
                "phase {phase} has no CONTEXT.md on develop, and codex cannot run an \
                 interactive discussion headless. Run /gsd-discuss-phase {phase} \
                 interactively first (any agent), or use --agent claude."
            )));
        }
        if !phase_artifact_on_develop(project_root, phase, "-PLAN.md") {
            println!(
                "warning: phase {phase} has no PLAN.md on develop — headless codex \
                 planning is untested and may need input; pre-writing plans is safer"
            );
        }
    }

    // Pre-start divergence check: runs on current HEAD before any git
    // mutation. WR-10 (13-REVIEW.md): only meaningful for the --no-worktree
    // (branch-in-place) flow, where `start` actually branches from the main
    // checkout's current HEAD. In worktree mode (the default) the agent's
    // work always forks fresh from `develop` via `worktree::add`, independent
    // of whatever happens to be checked out in the main repo — checking the
    // main checkout's divergence there is unrelated to what's about to
    // happen and can either hard-fail on a stale unrelated branch or
    // silently no-op if the main checkout happens to be on develop.
    if !worktree && let Ok((_ahead, behind)) = GitFlow::new(project_root).divergence_from_develop()
    {
        if behind > 50 {
            return Err(CliError::Message(format!(
                "develop is {behind} commits ahead — your branch is too far behind. \
                 Rebase onto develop first, or use --force to override."
            )));
        }
        if behind > 10 {
            println!("warning: develop is {behind} commits ahead — consider rebasing first");
        }
    }

    if worktree {
        let wt = ensure_phase_worktree(project_root, phase, force)?;
        println!(
            "created worktree: {} (branch {FEATURE_PREFIX}phase-{phase:02})",
            wt.display(),
        );
        state.worktree_path = Some(wt);
    } else {
        let git = GitFlow::new(project_root);
        let result = if force {
            git.feature_start_force(phase)
        } else {
            git.feature_start(phase)
        };
        match result {
            Ok(branch) => println!("created feature branch: {branch}"),
            Err(err) => {
                if !force {
                    return Err(CliError::Message(format!(
                        "{err}\nUse --force to overwrite the existing branch."
                    )));
                }
                return Err(err.into());
            }
        }
    }

    // WR-11 (13-REVIEW.md), revised: state must be on disk BEFORE the monitor
    // exists. launch_stage spawns the detached monitor, which runs `devflow
    // advance` the moment the agent exits — and advance begins with
    // load_state. Launching first (the previous WR-11 order) raced a
    // fast-exiting agent against this save: the monitor's advance found no
    // state.json, died silently into /dev/null, and the save below then wrote
    // an in-progress state nothing would ever advance. Save first; if the
    // launch fails, clear the just-saved state so `devflow status`/`recover`
    // don't report a phantom run (the failure WR-11 originally targeted).
    workflow::save_state(&state)?;
    events::emit(
        project_root,
        phase,
        "workflow_started",
        workflow_started_payload(&state),
    );
    if let Err(err) = launch_stage(&mut state, None, None) {
        if let Err(clear_err) = workflow::clear_state(project_root, phase) {
            eprintln!("warning: could not clear state after failed launch: {clear_err}");
        }
        return Err(err);
    }
    println!(
        "started phase {} in {mode} mode at {} — monitor will auto-advance",
        state.phase, state.started_at
    );
    println!("  watch live: devflow logs -f --phase {phase}");
    Ok(())
}

// ---------------------------------------------------------------------------
// 17d: build provenance + self-dogfood staleness gate (D-17-D-21).
// ---------------------------------------------------------------------------

/// D-21: the `workflow_started` event payload, including build provenance —
/// factored out of `start()` so the payload shape is directly unit-testable
/// without spawning a real agent (`start()` calls `launch_stage` immediately
/// after emitting this event).
fn workflow_started_payload(state: &State) -> serde_json::Value {
    serde_json::json!({
        "agent": state.agent.to_string(),
        "mode": state.mode.to_string(),
        "worktree": state.worktree_path.as_ref().map(|p| p.display().to_string()),
        "version": env!("CARGO_PKG_VERSION"),
        "commit": env!("DEVFLOW_BUILD_COMMIT"),
        "dirty": env!("DEVFLOW_BUILD_DIRTY"),
        // WR-02: filename only, never the full path (leaks home dir/username
        // into OPERATIONS.md's tail-and-paste file); to_string_lossy (not
        // to_str) so non-UTF-8 names still yield a string, not null.
        "exe_path": std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
    })
}

/// The post-preflight body of [`launch_stage`]: self-dogfood build-staleness
/// enforcement, capture archival/rollover, and spawning the monitor.
/// Extracted (18f, D-18f) so `run_preflight`'s `Advance` arm can call it
/// directly and skip the just-adjudicated preflight check, while every
/// other caller keeps going through [`launch_stage`]'s full path (readiness
/// resolution, `ensure_agent_binary`, then `run_preflight`).
///
/// Recomputes `prompt`/`adapter`/`roots`/`program`/`args` from `state` and
/// `prompt_override` — deliberately NOT threaded through as parameters.
/// They are pure functions of `state` and the prompt override; recomputing
/// them here (rather than widening `run_preflight`'s signature to carry
/// them from `launch_stage`'s earlier resolution) keeps this function
/// callable entirely on its own, which is exactly what `run_preflight`'s
/// `Advance` arm needs. This does not duplicate `worktree_writable_roots`'s
/// logic — both call sites call the same shared helper.
fn launch_stage_inner(
    state: &mut State,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<(), CliError> {
    // WR-04 (18-fix): clear the prior stage's monitor pid up front, before
    // any fallible step below (`ensure_agent_binary`, `enforce_build_staleness`)
    // can return early via `?`. Without this, a failed relaunch left
    // `state.stage` already advanced (by `transition()`, before this
    // function was ever called) alongside a stale `monitor_pid` still
    // naming the PREVIOUS stage's (now-dead) monitor — `liveness()` then
    // misreports `Stuck → devflow resume`, even when the real remedy is
    // unrelated (e.g. rebuild after a staleness block). The real pid is
    // set again below once `monitor::spawn_monitor` actually succeeds.
    state.monitor_pid = None;
    workflow::save_state(state)?;

    let prompt = prompt_override.unwrap_or_else(|| {
        prompt::stage_prompt_for_project(state.stage, state.phase, &state.project_root)
    });
    let adapter = agents::adapter_for(state.agent);
    // In worktree mode the agent's cwd is the linked worktree, but git
    // metadata for commits lives under the main repo's `.git/` — sandboxed
    // agents need it (and the worktree admin dir, which Codex read-only-
    // mounts otherwise) writable (13-06 dogfood finding).
    let roots = state
        .worktree_path
        .as_deref()
        .map(|wt| worktree_writable_roots(&state.project_root, wt))
        .unwrap_or_default();
    let (program, args) = adapter.exec_command(state.phase, &prompt, &roots);
    ensure_agent_binary(program)?;

    let project_root = state.project_root.clone();

    // 17d (Task 2, D-17-D-19): self-dogfood build-staleness gate — also
    // before spawn_monitor, so a stale DevFlow-on-itself run never even
    // reaches the agent.
    enforce_build_staleness(
        &project_root,
        state,
        env!("DEVFLOW_BUILD_COMMIT"),
        env!("DEVFLOW_BUILD_DIRTY") == "true",
    )?;

    if let Some(stamp) = agent_result::archive_phase_files(
        &state.project_root,
        state
            .worktree_path
            .as_deref()
            .unwrap_or(&state.project_root),
        state.phase,
        capture_retention(&state.project_root),
    )
    .map_err(|err| {
        CliError::Message(format!(
            "could not archive phase {} capture before rollover: {err}",
            state.phase
        ))
    })? {
        events::emit(
            &state.project_root,
            state.phase,
            "capture_archived",
            serde_json::json!({
                "stage": archived_stage.unwrap_or(state.stage).to_string(),
                "to_stage": state.stage.to_string(),
                "stamp": stamp,
            }),
        );
    }
    let pid = monitor::spawn_monitor(state, program, &args, &adapter.extra_env())
        .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    // `transition()` calls `workflow::save_state` BEFORE `launch_stage`, so a
    // pid recorded only in memory here is lost unless it is written again
    // (18b).
    state.monitor_pid = Some(pid);
    workflow::save_state(state)?;
    events::emit(
        &state.project_root,
        state.phase,
        "stage_launched",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "agent": state.agent.to_string(),
            "monitor_pid": pid,
        }),
    );
    println!(
        "stage {} → launched {} (monitor pid {pid})",
        state.stage,
        adapter.name()
    );
    Ok(())
}

/// Spawn the background monitor that owns the agent for `state.stage`. The
/// monitor calls `devflow advance` when the agent exits. An optional
/// `prompt_override` is used for Code loop-backs (fix prompts).
///
/// Resolves the prompt/adapter/roots/program, validates the agent binary,
/// then runs the readiness gate ([`run_preflight`]) before delegating to
/// [`launch_stage_inner`] for the actual spawn. Every EXISTING caller of
/// this function keeps getting the full path including preflight — the
/// ONLY caller of `launch_stage_inner` directly is `run_preflight`'s own
/// `Advance` arm (18f, D-18f), which is skipping a check it just
/// adjudicated for this one relaunch, not granting a standing bypass
/// (T-18-28: the skip must never leak beyond the single stage a human
/// approved).
fn launch_stage(
    state: &mut State,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<(), CliError> {
    let adapter = agents::adapter_for(state.agent);
    let prompt = prompt_override.clone().unwrap_or_else(|| {
        prompt::stage_prompt_for_project(state.stage, state.phase, &state.project_root)
    });
    let roots = state
        .worktree_path
        .as_deref()
        .map(|wt| worktree_writable_roots(&state.project_root, wt))
        .unwrap_or_default();
    let (program, _args) = adapter.exec_command(state.phase, &prompt, &roots);
    ensure_agent_binary(program)?;

    // 17c (Task 1, D-13-D-16): a scoped readiness gate runs before any agent
    // time is spent — a failing check surfaces as a named preflight gate +
    // notify (never a hard exit, D-15), not here.
    //
    // CR-01 (17-08 gap closure): `run_preflight` returns `Ok(false)` when a
    // failing check was ALREADY resolved via a full retried launch (or an
    // abort) — this frame must not run any more launch steps in that case,
    // or the agent gets spawned a second time for the same stage.
    let project_root = state.project_root.clone();
    if !run_preflight(&project_root, state, adapter.as_ref())? {
        return Ok(());
    }

    launch_stage_inner(state, prompt_override, archived_stage)
}

/// Resume a rate-limited or infra-paused phase from its saved stage (review
/// consensus #5). Loads the persisted `.devflow/state-{NN}.json` and
/// relaunches its saved stage via [`launch_stage`] — unlike `start`, this
/// does NOT call `State::new`, `feature_start`, or `ensure_phase_worktree`:
/// the branch/worktree already exist and agent/mode are read from the saved
/// state, so neither needs to be passed as a flag and the workflow is never
/// reset to Define.
fn resume(project_root: &Path, phase: u32) -> Result<(), CliError> {
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    let mut state = workflow::load_state(project_root, phase)?;
    launch_stage(&mut state, None, None)
}

/// The single active phase: `Ok(Some)` when exactly one is active, `Ok(None)`
/// when none, and an error naming the candidates when several are — shared by
/// `advance`'s legacy fallback and `logs`'s default-phase resolution so the
/// ambiguity rule and message live in one place.
fn single_active_phase(project_root: &Path) -> Result<Option<u32>, CliError> {
    let states = workflow::list_states(project_root);
    match states.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one.phase)),
        many => Err(CliError::Message(format!(
            "multiple active phases ({}) — pass --phase to pick one",
            many.iter()
                .map(|s| s.phase.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Resolve which phase a bare `devflow advance` (no `--phase`) refers to:
/// only unambiguous when exactly one phase is active. Exists for monitors
/// spawned by a pre-14a binary that doesn't pass `--phase`.
fn resolve_sole_active_phase(project_root: &Path) -> Result<u32, CliError> {
    single_active_phase(project_root)?
        .ok_or_else(|| CliError::Message("no active DevFlow state — nothing to advance".into()))
}

/// Advance the stage machine after a monitored agent for `state.stage` exits.
/// Invoked by the monitor process; not normally run by a human.
fn advance(project_root: &Path, phase: Option<u32>) -> Result<(), CliError> {
    // 13-DEFERRED-CR-03 fix shape #2: the phase is threaded in by the monitor
    // (recorded at spawn time), so advance's identity never depends on a
    // shared state singleton — under `devflow parallel`, each monitor
    // advances exactly its own phase. The Option fallback only serves
    // monitors spawned by an older binary.
    let phase = match phase {
        Some(phase) => phase,
        None => match resolve_sole_active_phase(project_root) {
            Ok(phase) => phase,
            Err(err) => {
                // 14-CR-06: a legacy monitor's bare `advance` failing here
                // would otherwise be invisible (its output goes to
                // /dev/null) and its phase silently stalls — record the
                // failure in events.jsonl. Phase 0 is the "could not
                // attribute a phase" sentinel; no real phase is 0.
                events::emit(
                    project_root,
                    0,
                    "advance_failed",
                    serde_json::json!({ "reason": err.to_string() }),
                );
                return Err(err);
            }
        },
    };
    // CR-03 (13-REVIEW.md): the lock is scoped per-phase, not per-project.
    // advance() holds it across a gate's multi-day blocking wait, and every
    // successful run ends at a mandatory Ship gate — a project-wide lock
    // would starve `devflow parallel`'s sibling phases with no retry.
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    // Load under the lock: with per-phase state files keyed by the same
    // phase as the lock, there is no cross-phase TOCTOU left by
    // construction — a concurrent advance of another phase touches a
    // different file and a duplicate advance of THIS phase is excluded by
    // the lock itself.
    let mut state = workflow::load_state(project_root, phase)?;

    let git_flow = GitFlowConfig::default();
    let result = agent_result::evaluate_agent_result(project_root, &state, &git_flow)
        .map_err(|err| CliError::Message(format!("could not evaluate agent result: {err}")))?;
    let stage = state.stage;
    println!("stage {stage} finished with status {:?}", result.status);
    if let Some(reason) = &result.reason {
        println!("  detail: {reason}");
    }
    events::emit(
        project_root,
        phase,
        "advance_evaluated",
        serde_json::json!({
            "stage": stage.to_string(),
            "status": result.status.as_wire_str(),
            "verdict": result.verdict.map(|v| format!("{v:?}").to_ascii_lowercase()),
            "decided_by_layer": result.decided_by_layer,
            "reason": result.reason.as_deref().map(truncate_reason),
        }),
    );

    // D-01/D-06: dispatch on the exhaustive outcome_policy::decide_action
    // table (no wildcard arm upstream) so a new/unhandled AgentStatus variant
    // is a compile error here rather than a silent advance. Replaces the old
    // `matches!(Failed | RateLimited)` boolean, which let Unknown fall
    // through into the success arm below.
    match outcome_policy::decide_action(stage, result.status) {
        Action::Advance => match stage {
            Stage::Define => transition(project_root, &mut state, Stage::Plan),
            Stage::Plan => transition(project_root, &mut state, Stage::Code),
            Stage::Code => transition(project_root, &mut state, Stage::Validate),
            Stage::Validate => {
                // 13b verdict-vs-ran + 18e: the Validate prompt now REQUIRES
                // a verdict, so ONLY an explicit `verdict: pass` advances to
                // Ship. A missing verdict is a fail-safe (gate/loop), NOT a
                // silent pass — closes the composition bug where a
                // marker-less/verdict-less Validate could otherwise reach
                // Ship. `classify_validate_outcome` additionally resolves
                // the `external_verify` three-way matrix (D-18e): agreement
                // advances, disagreement/no-verdict gates immediately.
                handle_validate_outcome(
                    project_root,
                    &mut state,
                    classify_validate_outcome(&result),
                )
            }
            Stage::Ship => handle_ship_outcome(project_root, &mut state),
        },
        Action::GateReview => match stage {
            // Validate failures drive the Code↔Validate loop (or a gate).
            Stage::Validate => {
                handle_validate_outcome(project_root, &mut state, ValidateOutcome::Failed)
            }
            // Ship distinguishes an agent crash (AgentFailed) from a review
            // rejection (ReviewFailed, `review:`-prefixed reason).
            Stage::Ship => handle_ship_failure(project_root, &mut state, result.reason),
            // Every other non-Validate failure (incl. Unknown, D-06) is
            // never silent (WR-11): it always fires a gate + notify instead
            // of returning a bare error or silently advancing.
            _ => handle_stage_failure(project_root, &mut state, stage, result.reason),
        },
        // ResourceKilled/AgentUnavailable: a dedicated infra path, identical
        // for every stage (including Validate/Ship) — MUST NOT route through
        // handle_validate_outcome/handle_ship_failure, which would bump
        // consecutive_failures (review consensus #4, D-08).
        Action::GateInfra => handle_infra_outcome(project_root, &mut state, stage, result.reason),
        // RateLimited: auto-resume via the primary loop's single-agent cron
        // path (D-09), bounded by the shared infra-failure ceiling (D-08).
        Action::AutoResume => {
            handle_rate_limited_outcome(project_root, &mut state, phase, stage, result.reason)
        }
    }
}

/// Route a `GateInfra` outcome (ResourceKilled/AgentUnavailable) — bumps
/// `state.infra_failures` (saturating, never `consecutive_failures`),
/// persists, then either aborts at the ceiling or fires the never-silent
/// gate via [`handle_stage_failure`]. Deliberately never calls
/// `handle_validate_outcome`/`handle_ship_failure` on any stage (review
/// consensus #4) — those increment `consecutive_failures`, which would
/// conflate an infrastructure fault with an agent-caused failure (D-08).
fn handle_infra_outcome(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    state.infra_failures = state.infra_failures.saturating_add(1);
    workflow::save_state(state)?;
    gate_or_abort_infra(project_root, state, stage, reason)
}

/// The ceiling check + gate-or-abort half of the infra path, shared by
/// [`handle_infra_outcome`] and the `AutoResume` arm's infra-ceiling branch
/// (which bumps `infra_failures` itself before calling this, so the counter
/// is never bumped twice for the same outcome).
fn gate_or_abort_infra(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    if state.infra_failures >= mode::MAX_INFRA_FAILURES {
        return abort(
            project_root,
            state,
            &format!(
                "infrastructure failures reached the ceiling ({} of {}) — aborting rather than gating again",
                state.infra_failures,
                mode::MAX_INFRA_FAILURES
            ),
        );
    }
    handle_stage_failure(project_root, state, stage, reason)
}

/// Route a `RateLimited` outcome from the PRIMARY advance() monitor loop
/// (D-09): writes a single-agent cron-instructions resume record (`devflow
/// resume --phase N`) and returns without firing a blocking gate — unlike
/// `sequentagent`'s existing rate-limit handling, this path never called the
/// cron machinery before this plan (Pitfall 3). Shares the same
/// `infra_failures` ceiling as [`handle_infra_outcome`] (D-08's intentional
/// shared infra counter): once bumping would reach the ceiling, auto-resume
/// stops and the outcome instead routes through the infra gate/abort path.
/// Never touches `consecutive_failures`.
fn handle_rate_limited_outcome(
    project_root: &Path,
    state: &mut State,
    phase: u32,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let retry_after = retry_after_from_reason(reason.as_deref());
    let projected_infra_failures = state.infra_failures.saturating_add(1);
    if projected_infra_failures >= mode::MAX_INFRA_FAILURES {
        return handle_infra_outcome(project_root, state, stage, reason);
    }
    state.infra_failures = projected_infra_failures;
    workflow::save_state(state)?;

    let instructions =
        devflow_core::ship::build_single_agent_cron_instructions(project_root, phase, &retry_after);
    devflow_core::ship::write_cron_instructions(project_root, &instructions)?;
    // CR-03: an unparseable retry hint (e.g. the `"usage limit"` fallback for
    // a 429 with no retry_after) leaves the schedule empty — and it must stay
    // empty, since an empty cron expression would degrade into an
    // every-minute resume. That means auto-resume cannot happen, so returning
    // here would exit the detached monitor with the phase stalled and no
    // operator signal at all (the println below is read by nobody). Route
    // through the same gate/notify path the infra ceiling uses so the phase is
    // never silently stalled (WR-11/D-15). `infra_failures` is already bumped
    // above, so `gate_or_abort_infra` — which never bumps — is the correct
    // entry point.
    if instructions.hermes_cron.schedule.is_empty() {
        return gate_or_abort_infra(
            project_root,
            state,
            stage,
            Some(format!(
                "rate limited with no parseable retry time ({retry_after}) — auto-resume cron not scheduled; resume manually"
            )),
        );
    }
    println!(
        "rate limited — wrote {}",
        devflow_core::ship::cron_instructions_path(project_root, phase)
            .strip_prefix(project_root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| {
                devflow_core::ship::cron_instructions_path(project_root, phase)
                    .display()
                    .to_string()
            })
    );
    events::emit(
        project_root,
        phase,
        "rate_limit_resume_scheduled",
        serde_json::json!({
            "stage": stage.to_string(),
            "retry_after": retry_after,
            "infra_failures": state.infra_failures,
        }),
    );
    Ok(())
}

/// The three-way outcome of a Validate stage evaluation (18e, D-18e).
///
/// Distinct from a plain `bool`: an `external_verify`-declared Validate has
/// THREE distinguishable outcomes, not two — the probe and the agent's
/// self-reported verdict can independently agree, disagree, or leave one
/// signal missing. Collapsing disagreement or "no verdict at all" onto
/// `Failed` would route them through the counter-based auto-loop, a DELAYED
/// gate indistinguishable from an ordinary retry to the operator watching
/// it — the binding operator decision requires an IMMEDIATE one instead
/// (T-18-19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidateOutcome {
    /// The two independent signals agree (or no `external_verify` is
    /// declared and the agent reported `verdict: pass`): advance to Ship.
    Passed,
    /// An ordinary Validate failure — the pre-existing fail-safe, unchanged:
    /// loop back to Code, or gate once `consecutive_failures` reaches the
    /// ceiling.
    Failed,
    /// The probe passed but the agent's verdict disagrees, or no verdict
    /// arrived at all. Gates for a human IMMEDIATELY, never touching
    /// `consecutive_failures`. The payload names which two signals
    /// disagreed, for the `[never-silent]` gate context.
    Ambiguous(String),
}

/// Classify a Validate-stage `AgentResult` into its three-way outcome
/// (D-18e, the binding operator decision reproduced in 18-05-PLAN.md).
///
/// Pure function over `&AgentResult` — no I/O — so the whole decision
/// matrix is directly unit-testable. `Some(Verdict::Pass)` is matched FIRST
/// and wins regardless of which layer decided the result: it is the "two
/// independent signals agreeing" arm and must not be shadowed by the
/// external-verify-specific arms below it.
fn classify_validate_outcome(result: &agent_result::AgentResult) -> ValidateOutcome {
    let external = result.decided_by_layer == Some(0) && result.status == AgentStatus::Success;
    match (external, result.verdict) {
        (_, Some(Verdict::Pass)) => ValidateOutcome::Passed,
        (true, Some(Verdict::Gaps)) => ValidateOutcome::Ambiguous(
            "external verification passed but the agent reported gaps".to_string(),
        ),
        (true, None) => ValidateOutcome::Ambiguous(
            "external verification passed but no agent verdict arrived".to_string(),
        ),
        _ => ValidateOutcome::Failed,
    }
}

/// The two ordinary Validate outcomes left once `ValidateOutcome::Ambiguous`
/// has been handled and returned on its own (WR-03, 18-fix). Deliberately a
/// distinct, two-variant type: matching on THIS below is exhaustive without
/// a third, panic-capable arm — the compiler enforces that
/// `handle_validate_outcome`'s tail can never see an ambiguous outcome,
/// instead of that invariant being proven by hand-tracing control flow (the
/// pre-fix shape's `unreachable!()`, which was sound but fragile: a future
/// edit to either the `forced` computation or the early-return `if` could
/// have silently reintroduced reachability).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidateResult {
    Passed,
    Failed,
}

/// Decide what happens after a Validate stage, honoring the active mode's
/// gate policy, the consecutive-failure threshold, and (18e) the immediate
/// gate an ambiguous `external_verify` outcome forces regardless of either.
fn handle_validate_outcome(
    project_root: &Path,
    state: &mut State,
    outcome: ValidateOutcome,
) -> Result<(), CliError> {
    // 18e / T-18-19: an ambiguous outcome must gate IMMEDIATELY — it is
    // being adjudicated right now, not retried, so it must never fall
    // through to the counter-based `should_gate` check below and must never
    // touch `consecutive_failures`. Handled in its own arm, up front, and
    // converted to `ValidateResult` for the two variants that share the
    // rest of this function's logic (WR-03).
    let result = match outcome {
        ValidateOutcome::Ambiguous(detail) => {
            let context = format!(
                "[never-silent] validate ambiguous: {}",
                truncate_reason(&detail)
            );
            return match run_gate(project_root, state, Stage::Validate, &context)? {
                GateAction::Advance => transition(project_root, state, Stage::Ship),
                GateAction::LoopBack(_) => {
                    loop_back_to_code(project_root, state, FixType::GapsOnly)
                }
                GateAction::Abort(reason) => abort(project_root, state, &reason),
            };
        }
        ValidateOutcome::Passed => ValidateResult::Passed,
        ValidateOutcome::Failed => ValidateResult::Failed,
    };

    if result == ValidateResult::Failed {
        // Now that the counter genuinely accumulates (18d), an unbounded
        // loop could otherwise overflow it and wrap to 0, silently
        // restoring the unreachable-ceiling bug in a slower form.
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        workflow::save_state(state)?;
    }

    if state
        .mode
        .should_gate(Stage::Validate, state.consecutive_failures)
    {
        let context = match result {
            ValidateResult::Passed => "Validation passed — approve to ship?".to_string(),
            ValidateResult::Failed => format!(
                "Validation failed {} time(s) — human review needed.",
                state.consecutive_failures
            ),
        };
        return match run_gate(project_root, state, Stage::Validate, &context)? {
            GateAction::Advance => transition(project_root, state, Stage::Ship),
            GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }

    match result {
        ValidateResult::Passed => transition(project_root, state, Stage::Ship),
        ValidateResult::Failed => loop_back_to_code(project_root, state, FixType::GapsOnly),
    }
}

/// Decide what happens after the Ship stage completes — always gated.
fn handle_ship_outcome(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    match run_gate(
        project_root,
        state,
        Stage::Ship,
        "Ship complete — approve merge?",
    )? {
        GateAction::Advance => finish_workflow(project_root, state),
        GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}

/// Handle a non-Validate stage failure (Define/Plan/Code, or a Ship agent
/// crash routed in via [`handle_ship_failure`]). WR-11: this path must never
/// be silent — it unconditionally fires a gate + notify via [`run_gate`]
/// (independent of `Mode::should_gate`; `run_gate` marks it as an unexpected
/// gate and notifies accordingly), then lets the operator retry, loop back,
/// or abort. Deliberately kept separate from `handle_validate_outcome`: it
/// does not touch `consecutive_failures` and never auto-loops.
/// Cap a failure reason before it enters a gate context (and from there the
/// operator's notification). Reasons are agent- or parser-derived and can
/// embed arbitrary output — 13-06 dogfood finding: a multi-KB raw JSONL line
/// reached the desktop notification verbatim. Full detail stays available in
/// `.devflow/phase-NN-stdout`; the gate only needs a readable headline.
fn truncate_reason(reason: &str) -> String {
    render_gate_context(reason, 300)
}

/// Render agent-controlled gate text as one bounded, terminal-safe line.
fn render_gate_context(context: &str, max_chars: usize) -> String {
    const TRUNCATED: &str = "… [truncated; full output in .devflow/]";
    let sanitized: String = context
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    if sanitized.chars().count() <= max_chars {
        return sanitized;
    }

    let suffix_len = TRUNCATED.chars().count().min(max_chars);
    let head_len = max_chars.saturating_sub(suffix_len);
    let head: String = sanitized.chars().take(head_len).collect();
    let suffix: String = TRUNCATED.chars().take(suffix_len).collect();
    format!("{head}{suffix}")
}

fn handle_stage_failure(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let context = format!(
        "[never-silent] stage {stage} failed: {} — human review needed (retry, loop-to-code, or abort)",
        truncate_reason(&reason.unwrap_or_else(|| "no details available".into()))
    );
    match run_gate(project_root, state, stage, &context)? {
        GateAction::Advance => {
            // CR-01: clean up the stale gate/response/ack before retrying so
            // the retry cannot silently consume the prior response.
            let _ = Gates::cleanup(project_root, state.phase, stage);
            state.gate_pending = false;
            launch_stage(state, None, Some(stage))
        }
        GateAction::LoopBack(_) => {
            // Retry the SAME failed stage — Code is not a valid recovery
            // target before planning exists for a Define/Plan failure
            // (Codex 13-01 MEDIUM). Only Ship's ReviewFailed path (handled
            // separately in `handle_ship_failure`) actually loops to Code.
            let _ = Gates::cleanup(project_root, state.phase, stage);
            launch_stage(state, None, Some(stage))
        }
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}

/// Handle the Ship stage's failure outcome, distinguishing an agent crash
/// (`AgentFailed`) from a review rejection (`ReviewFailed`). A `review:`-
/// prefixed reason (trimmed, case-folded) is the agent-reported convention
/// for "the change was reviewed and rejected" — that loops back to Code with
/// the `/gsd-audit-fix` prompt rather than firing a gate (consensus #7).
/// Anything else is treated as an agent crash and routed through the generic
/// never-silent gate path.
fn handle_ship_failure(
    project_root: &Path,
    state: &mut State,
    reason: Option<String>,
) -> Result<(), CliError> {
    if is_ship_review_failure(&reason) {
        return loop_back_to_code(project_root, state, FixType::AuditFix);
    }
    handle_stage_failure(project_root, state, Stage::Ship, reason)
}

/// Whether a Ship-stage failure `reason` is a review rejection (`review:`
/// prefix, trimmed + case-folded) rather than an agent crash. This string
/// convention is an inherent limitation of the agent-reported DEVFLOW_RESULT
/// contract (T-13-04) — verified live against a real agent in 13-06.
fn is_ship_review_failure(reason: &Option<String>) -> bool {
    reason
        .as_deref()
        .map(|r| r.trim().to_ascii_lowercase().starts_with("review:"))
        .unwrap_or(false)
}

/// Run a batch of hooks against the primary checkout, serialized across
/// phases by the coarse project lock (13-DEFERRED-CR-03 fix shape #3): the
/// hooks commit/tag/delete branches in the shared main checkout, and two
/// phases doing that concurrently race git's `index.lock`/`HEAD`. Held for
/// seconds — never across a gate wait. Hook failures stay fail-soft (warn
/// and continue), as before.
///
/// 14-CR-02: a lock timeout SKIPS the batch instead of running it
/// unserialized — mutating the shared checkout concurrently is the exact
/// race this lock exists to prevent, and the hooks are individually
/// fail-soft for ordinary transitions. The return value lets terminal
/// completion fail closed and preserve state when the batch was skipped or
/// a required hook failed.
/// Which tree a hook batch operates on.
///
/// The Validate→Ship transition batch (`DocsUpdate`) authors material *about
/// the branch being shipped*, so it must write into that phase's worktree —
/// otherwise its output is stranded on the base branch, uncommitted and
/// divorced from the commits it describes (found live: Phase 17's changelog
/// entry landed on `develop` while every one of its commits sat on
/// `feature/phase-17`).
///
/// The terminal batch (`Merge`, `VersionBump`, `ChangelogAppend`,
/// `BranchCleanup`) is the exact opposite: it merges the feature branch INTO
/// the base branch, tags the base branch, and deletes the feature branch.
/// Those are primary-checkout operations and retargeting them at the
/// worktree would be a correctness regression. `ChangelogAppend` moved here
/// in 17-12 (WR-04) — a release record naming a version only becomes true
/// once `VersionBump` has tagged it, so the changelog entry belongs on the
/// base branch alongside the tag, not in the worktree. Do not restore
/// 17-10's worktree targeting to this hook.
///
/// Falls back to `project_root` whenever no worktree is configured, so
/// `--no-worktree` runs are unaffected.
fn hook_context_root(project_root: &Path, state: &State, terminal_batch: bool) -> PathBuf {
    if terminal_batch {
        return project_root.to_path_buf();
    }
    state
        .worktree_path
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| project_root.to_path_buf())
}

fn run_checkout_hooks(
    project_root: &Path,
    state: &State,
    batch: &[hooks::Hook],
    stage: Stage,
) -> bool {
    if batch.is_empty() {
        return true;
    }
    let _checkout_lock = match lock::acquire_project_blocking(project_root, checkout_lock_timeout())
    {
        Ok(guard) => guard,
        Err(err) => {
            println!(
                "warning: could not acquire the checkout lock ({err}) — \
                 SKIPPING hooks {batch:?} rather than mutating the checkout \
                 unserialized. Re-run them once the holder finishes."
            );
            events::emit(
                project_root,
                state.phase,
                "checkout_lock_timeout",
                serde_json::json!({ "stage": stage.to_string(), "error": err.to_string() }),
            );
            for hook in batch {
                events::emit(
                    project_root,
                    state.phase,
                    "hook_run",
                    serde_json::json!({
                        "hook": format!("{hook:?}"),
                        "ok": false,
                        "skipped": "checkout lock timeout",
                    }),
                );
            }
            return false;
        }
    };
    let git_flow = GitFlowConfig::default();
    let mut all_succeeded = true;
    let terminal_batch = batch == hooks::hooks_after_ship().as_slice();
    let hook_root = hook_context_root(project_root, state, terminal_batch);
    // Hoisted out of the loop (GAP-7): these fields are loop-invariant, and
    // VersionBump needs to hand shipped_version forward to ChangelogAppend
    // within the same batch run, which a fresh per-iteration context would
    // discard.
    let mut ctx = HookContext {
        phase: state.phase,
        project_root: hook_root.clone(),
        stage,
        git_flow: git_flow.clone(),
        shipped_version: None,
    };
    for hook in batch {
        let outcome = hook.run(&mut ctx);
        if let Err(ref err) = outcome {
            println!("warning: hook {hook:?} failed: {err}");
            all_succeeded = false;
        }
        events::emit(
            project_root,
            state.phase,
            "hook_run",
            serde_json::json!({
                "hook": format!("{hook:?}"),
                "ok": outcome.is_ok(),
            }),
        );
        // Terminal finalization is ordered and fail-fast. In particular, a
        // failed version/tag operation must not delete the feature branch and
        // destroy the evidence needed for a safe retry.
        if terminal_batch && outcome.is_err() {
            break;
        }
    }
    all_succeeded
}

/// Fire the hooks for `from → to`, persist the new stage, and launch its agent.
///
/// `infra_failures` resets unconditionally on every successful transition
/// (CR-01, 17-06 gap closure). Without this, an infra-fault ceiling meant to
/// bound a *stuck loop* (D-08, [`mode::MAX_INFRA_FAILURES`]) instead
/// accumulates across a phase's entire lifetime — several well-spaced,
/// cleanly-resolved infra faults would falsely reach the ceiling and
/// hard-abort a long-running but otherwise healthy phase.
///
/// `consecutive_failures` clears on every transition EXCEPT Code→Validate
/// (18d, [`mode::transition_resets_consecutive_failures`]): that hop is
/// crossed on every single Code↔Validate retry cycle, so unconditionally
/// clearing it there made [`mode::MAX_CONSECUTIVE_FAILURES`] unreachable for
/// the exact loop it bounds. The two counters deliberately no longer share a
/// single reset condition.
fn transition(project_root: &Path, state: &mut State, to: Stage) -> Result<(), CliError> {
    let from = state.stage;
    let _ = run_checkout_hooks(
        project_root,
        state,
        &hooks::hooks_for_transition(from, to),
        to,
    );
    state.stage = to;
    if mode::transition_resets_consecutive_failures(from, to) {
        state.consecutive_failures = 0;
    }
    state.infra_failures = 0;
    state.gate_pending = false;
    workflow::save_state(state)?;
    events::emit(
        project_root,
        state.phase,
        "transition",
        serde_json::json!({
            "from": from.to_string(),
            "to": to.to_string(),
        }),
    );
    launch_stage(state, None, Some(from))
}

/// Loop the pipeline back to Code with the given fix prompt (`GapsOnly` for a
/// Validate rejection, `AuditFix` for a Ship `review:` rejection).
fn loop_back_to_code(project_root: &Path, state: &mut State, fix: FixType) -> Result<(), CliError> {
    let from = state.stage;
    let prompt = prepare_loop_back_to_code(project_root, state, fix)?;
    launch_stage(state, Some(prompt), Some(from))
}

/// The state-mutating half of `loop_back_to_code`, split out so it's
/// unit-testable without spawning a real agent process (`launch_stage`
/// invokes the actual configured agent CLI). Cleans up the stale gate for
/// the stage the gate fired on (CR-01), moves `state` to Code, persists it,
/// and returns the fix prompt the caller should launch with.
fn prepare_loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
) -> Result<String, CliError> {
    // Capture the stage the gate actually fired on before it's mutated below,
    // so cleanup targets the right stage's gate files (see CR-01: a stale
    // response/ack left on disk after a loop-back is silently reused by a
    // later gate for the same phase+stage).
    let gate_stage = state.stage;
    let _ = Gates::cleanup(project_root, state.phase, gate_stage);
    state.stage = Stage::Code;
    state.gate_pending = false;
    workflow::save_state(state)?;
    events::emit(
        project_root,
        state.phase,
        "loop_back",
        serde_json::json!({
            "from": gate_stage.to_string(),
            "consecutive_failures": state.consecutive_failures,
        }),
    );
    println!(
        "looping back to Code (validate failures: {})",
        state.consecutive_failures
    );
    Ok(prompt::fix_prompt(fix, state.phase))
}

/// Run the terminal hooks (version bump + branch cleanup) and clear state.
fn finish_workflow(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    loop {
        if run_checkout_hooks(project_root, state, &hooks::hooks_after_ship(), Stage::Ship) {
            break;
        }
        // The original Ship approval has already been consumed. Reopen an
        // actionable gate and keep this monitor waiting so a terminal-hook
        // failure cannot turn into an invisible stalled Ship state.
        let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
        let context = format!(
            "[finalization failed] phase {} terminal hooks did not complete. Resolve the git/version error, then approve to retry; reject to loop back or abort.",
            state.phase
        );
        match run_gate(project_root, state, Stage::Ship, &context)? {
            GateAction::Advance => {
                let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
            }
            GateAction::LoopBack(_) => {
                return loop_back_to_code(project_root, state, FixType::AuditFix);
            }
            GateAction::Abort(reason) => return abort(project_root, state, &reason),
        }
    }
    let _ = Gates::cleanup(project_root, state.phase, Stage::Validate);
    let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
    workflow::clear_state(project_root, state.phase)?;
    events::emit(
        project_root,
        state.phase,
        "workflow_finished",
        serde_json::Value::Null,
    );
    println!("phase {} shipped — workflow complete", state.phase);
    Ok(())
}

/// Write a gate file and block (in the detached monitor) until a response or
/// the long poll timeout. Acks the response so the Hermes poller can clean up.
fn run_gate(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    context: &str,
) -> Result<GateAction, CliError> {
    state.gate_pending = true;
    workflow::save_state(state)?;
    Gates::write_gate(project_root, state.phase, stage, context)?;
    println!(
        "gate written: .devflow/gates/{:02}-{stage}.json — awaiting response",
        state.phase
    );
    // A gate is "unexpected" when the active mode would not normally fire
    // one for this stage (e.g. a Define/Plan/Code failure in Auto mode) —
    // WR-11's never-silent path gates unconditionally, independent of mode.
    let unexpected = !state.mode.should_gate(stage, state.consecutive_failures);
    if unexpected {
        info!(
            "never-silent gate: {stage} failed in {:?} mode — surfacing an unattended gate this mode would not normally fire",
            state.mode
        );
    }
    events::emit(
        project_root,
        state.phase,
        "gate_fired",
        serde_json::json!({
            "stage": stage.to_string(),
            "unexpected": unexpected,
            "context": context,
        }),
    );
    gates::fire_gate_notify(state.phase, stage, context, unexpected);
    events::emit(
        project_root,
        state.phase,
        "notify_fired",
        serde_json::json!({ "stage": stage.to_string(), "unexpected": unexpected }),
    );
    match Gates::poll_response(project_root, state.phase, stage, gate_timeout_secs()) {
        Some(response) => {
            state.gate_pending = false;
            workflow::save_state(state)?;
            Gates::ack(project_root, state.phase, stage)?;
            let action = GateAction::from_response(&response);
            events::emit(
                project_root,
                state.phase,
                "gate_resolved",
                serde_json::json!({
                    "stage": stage.to_string(),
                    "approved": response.approved,
                    "action": match &action {
                        GateAction::Advance => "advance",
                        GateAction::LoopBack(_) => "loop_back",
                        GateAction::Abort(_) => "abort",
                    },
                    "responded_by": response.responded_by,
                }),
            );
            Ok(action)
        }
        None => {
            events::emit(
                project_root,
                state.phase,
                "gate_timeout",
                serde_json::json!({ "stage": stage.to_string() }),
            );
            Err(CliError::Message(format!(
                "gate for stage {stage} timed out awaiting a response"
            )))
        }
    }
}

/// Abort the workflow with a reason, clearing state.
fn abort(project_root: &Path, state: &State, reason: &str) -> Result<(), CliError> {
    println!("workflow aborted for phase {}: {reason}", state.phase);
    // See CR-01: without this, a stale response/ack for this phase+stage
    // survives on disk and is silently reused if the gate fires again later.
    let _ = Gates::cleanup(project_root, state.phase, state.stage);
    let _ = workflow::clear_state(project_root, state.phase);
    events::emit(
        project_root,
        state.phase,
        "workflow_aborted",
        serde_json::json!({ "reason": truncate_reason(reason) }),
    );
    Ok(())
}

/// Print the full pipeline that a `start` would run, without launching anything.
fn print_dry_run(state: &State) {
    println!(
        "dry run — phase {} | agent {} | mode {}",
        state.phase, state.agent, state.mode
    );
    println!("\nstage pipeline:");
    let mut stage = Some(Stage::Define);
    while let Some(s) = stage {
        let command = s.gsd_command().replace("{N}", &state.phase.to_string());
        let gate = if state.mode.should_gate(s, 0) {
            " [GATE]".to_string()
        } else if state.mode.should_gate(s, mode::MAX_CONSECUTIVE_FAILURES) {
            format!(" [GATE after {} failures]", mode::MAX_CONSECUTIVE_FAILURES)
        } else {
            String::new()
        };
        println!("  {s:<9} {command}{gate}");
        if let Some(next) = s.next() {
            let transition_hooks = hooks::hooks_for_transition(s, next);
            if !transition_hooks.is_empty() {
                println!("            ↳ hooks: {transition_hooks:?}");
            }
        }
        stage = s.next();
    }
    println!("\nafter ship: {:?}", hooks::hooks_after_ship());
}

/// Create the phase worktree at `.worktrees/phase-NN/` on `feature/phase-NN`.
fn ensure_phase_worktree(
    project_root: &Path,
    phase: u32,
    force: bool,
) -> Result<PathBuf, CliError> {
    let wt = worktree::phase_path(project_root, phase);
    let branch = format!("{FEATURE_PREFIX}phase-{phase:02}");

    if force {
        if wt.exists() {
            worktree::remove(project_root, &wt, true)?;
        }
        let _ = GitFlow::new(project_root).delete_branch(&branch, true);
    }

    match worktree::add(project_root, &wt, &branch, DEVELOP, true) {
        Ok(()) => Ok(wt),
        Err(devflow_core::worktree::WorktreeError::Exists(path)) => {
            Err(CliError::Message(format!(
                "worktree already exists at {} — use --force to recreate it",
                path.display()
            )))
        }
        Err(err) => Err(err.into()),
    }
}

// ---------------------------------------------------------------------------
// parallel / sequentagent
// ---------------------------------------------------------------------------

/// Parse `--phases` and optional `--agents` into positional (phase, agent)
/// pairs. Agents default to `claude` when fewer are given than phases; an error
/// is returned when more agents than phases are supplied.
fn parse_phase_agent_pairs(
    phases: &str,
    agents: Option<&str>,
) -> Result<Vec<(u32, AgentKind)>, CliError> {
    let phases: Vec<u32> = phases
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<u32>()
                .map_err(|_| CliError::Message(format!("invalid phase number `{p}`")))
        })
        .collect::<Result<_, _>>()?;
    if phases.is_empty() {
        return Err(CliError::Message("no phases given".into()));
    }

    let agents: Vec<AgentKind> = match agents {
        Some(list) => list
            .split(',')
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.parse::<AgentKind>()
                    .map_err(|err| CliError::Message(err.to_string()))
            })
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    if agents.len() > phases.len() {
        return Err(CliError::Message(format!(
            "got {} agents for {} phases — provide at most one agent per phase",
            agents.len(),
            phases.len()
        )));
    }

    Ok(phases
        .into_iter()
        .enumerate()
        .map(|(i, phase)| (phase, agents.get(i).copied().unwrap_or(AgentKind::Claude)))
        .collect())
}

/// Spawn one monitored worktree run per phase, concurrently.
fn parallel(
    project_root: &Path,
    phases: &str,
    agents: Option<&str>,
    mode: Mode,
    force: bool,
) -> Result<(), CliError> {
    let pairs = parse_phase_agent_pairs(phases, agents)?;
    println!("launching {} phase(s) in parallel worktrees", pairs.len());
    for (phase, agent) in pairs {
        println!("\n=== phase {phase} ({agent}) ===");
        // Worktree mode keeps each run isolated so the phases run together.
        start(project_root, phase, agent, mode, force, true, false)?;
    }
    Ok(())
}

/// Parse exactly two comma-separated agents for `sequentagent`.
fn split_two_agents(agents: &str) -> Result<(AgentKind, AgentKind), CliError> {
    let parsed: Vec<AgentKind> = agents
        .split(',')
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .map(|a| {
            a.parse::<AgentKind>()
                .map_err(|err| CliError::Message(err.to_string()))
        })
        .collect::<Result<_, _>>()?;
    if parsed.len() != 2 {
        return Err(CliError::Message(format!(
            "sequentagent requires exactly two agents (e.g. claude,codex), got {}",
            parsed.len()
        )));
    }
    Ok((parsed[0], parsed[1]))
}

/// Launch one agent via a no-advance monitor, block until it exits, and
/// return its self-reported result (parsed from the DEVFLOW_RESULT marker,
/// if present). Used by sequentagent, where the rebase handoff between
/// agents requires a synchronous run.
///
/// 14b: this used to be a CLI-owned `launch_agent` + `capture_agent_output`
/// pipe — the last synchronous execution path. It now rides the same
/// monitor-owned execution as everything else (stderr separated from the
/// parseable stdout capture, exit code reaped even if this CLI dies) and
/// simply blocks on the exit file the monitor writes.
fn run_agent_blocking(
    project_root: &Path,
    phase: u32,
    agent: AgentKind,
    workdir: &Path,
) -> Result<Option<agent_result::AgentResult>, CliError> {
    if let Some(stamp) = agent_result::archive_phase_files(
        project_root,
        workdir,
        phase,
        capture_retention(project_root),
    )
    .map_err(|err| {
        CliError::Message(format!(
            "could not archive phase {phase} capture before rollover: {err}"
        ))
    })? {
        events::emit(
            project_root,
            phase,
            "capture_archived",
            serde_json::json!({"stage": "code", "stamp": stamp}),
        );
    }
    let adapter = agents::adapter_for(agent);
    let prompt = prompt::stage_prompt_for_project(Stage::Code, phase, project_root);
    // sequentagent always runs in a worktree — the main repo's `.git/` and
    // the worktree admin dir must stay writable for sandboxed agents to
    // commit (13-06 dogfood finding).
    let roots = if workdir == project_root {
        Vec::new()
    } else {
        worktree_writable_roots(project_root, workdir)
    };
    let (program, args) = adapter.exec_command(phase, &prompt, &roots);
    ensure_agent_binary(program)?;
    // Synthetic, never-persisted state: the monitor only reads project_root,
    // phase, and worktree_path from it — sequentagent does not participate
    // in the stage machine.
    let mut state = State::new(phase, agent, Mode::Auto, project_root.to_path_buf());
    state.stage = Stage::Code;
    if workdir != project_root {
        state.worktree_path = Some(workdir.to_path_buf());
    }
    let monitor_pid =
        monitor::spawn_monitor_no_advance(&state, program, &args, &adapter.extra_env())
            .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    println!(
        "launched {} (monitor pid {monitor_pid}) in {}",
        adapter.name(),
        workdir.display()
    );
    // 14-CR-09: the sync path used to stream agent stderr to this terminal;
    // the monitor captures it instead — tell the operator where to watch.
    println!("  watch live: devflow logs -f --phase {phase} [--stderr]");
    let exit_code = monitor::wait_for_agent_exit(project_root, phase, monitor_pid)
        .map_err(|err| CliError::Message(format!("agent run did not complete: {err}")))?;
    println!("agent {agent} exited with code {exit_code}");
    // The monitor wrote stdout to the same file evaluate_layer1 reads, so
    // delegate to it directly rather than re-implementing a subset of its
    // precedence here — this is the single code path that knows how to find
    // a Codex agent's DEVFLOW_RESULT marker inside its JSONL `--json` event
    // stream (parse_codex_event_result).
    let result = agent_result::evaluate_layer1(project_root, phase);
    // Layer-1 silence is not success: a crashed agent (nonzero exit, no
    // marker, no envelope) yields None here, and sequentagent's callers
    // treat None as "proceed to integrate". Mirror Layer 2's exit-code gate
    // so a crash never fast-forwards a half-finished branch into the base.
    if result.is_none() && exit_code != 0 {
        return Ok(Some(agent_result::AgentResult {
            status: AgentStatus::Failed,
            exit_code: Some(exit_code),
            reason: Some(format!(
                "agent exited with code {exit_code} without reporting a result"
            )),
            commits: None,
            summary: None,
            verdict: None,
            // Mirrors Layer 2's exit-code gate per this block's own comment
            // (review consensus #3).
            decided_by_layer: Some(2),
        }));
    }
    Ok(result)
}

/// Integrate an agent branch into the shared base, pushing if a remote
/// exists. Serialized on the coarse checkout lock: branch fast-forwards and
/// pushes mutate shared refs that a concurrently finishing phase's hooks
/// also touch (13-DEFERRED-CR-03 sequentagent re-check).
fn integrate_agent_branch(
    project_root: &Path,
    git: &GitFlow,
    base: &str,
    agent_branch: &str,
) -> Result<(), CliError> {
    // 14-CR-07: this hard-fails on a lock timeout by design (a shared-ref
    // mutation must never run unlocked), which can leave an earlier agent's
    // branch integrated and this one not — so the error carries resume
    // guidance instead of leaving the operator to guess (re-running with
    // --force would re-run agents on top of already-integrated work).
    let _checkout_lock = lock::acquire_project_blocking(project_root, checkout_lock_timeout())
        .map_err(|err| {
            CliError::Message(format!(
                "could not lock checkout to integrate {agent_branch} into {base}: {err}. \
                 Earlier integrations into {base} are already in place; once the lock \
                 holder finishes, integrate manually with \
                 `git fetch . {agent_branch}:{base}` — do NOT re-run sequentagent --force."
            ))
        })?;
    git.fast_forward_branch(base, agent_branch)?;
    println!("integrated {agent_branch} into {base}");
    if git.has_remote() {
        match git.push(base) {
            Ok(()) => println!("pushed {base} to origin"),
            Err(err) => println!("warning: could not push {base}: {err}"),
        }
    }
    Ok(())
}

/// Run two agents sequentially on one phase, each in its own worktree, with a
/// rebase handoff between them. See the `Sequentagent` command docs.
fn sequentagent(
    project_root: &Path,
    phase: u32,
    agents: &str,
    force: bool,
) -> Result<(), CliError> {
    // 14b: hold this phase's lock for the whole run — a monitored pipeline
    // run and a sequentagent run for the same phase share capture files and
    // branches, and previously nothing excluded them from colliding.
    let _phase_lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "phase {phase} is already being driven by another devflow process (pid {pid})"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    if let Err(err) = devflow_core::ship::delete_cron_instructions(project_root, phase) {
        println!("warning: could not remove stale cron-instructions file: {err}");
    }
    let (agent_a, agent_b) = split_two_agents(agents)?;
    // 14-CR-05: both binaries must resolve before any branch/worktree is
    // scaffolded — agent B's absence should not surface after A's full run.
    ensure_agent_binary(agent_program(agent_a))?;
    ensure_agent_binary(agent_program(agent_b))?;
    let git = GitFlow::new(project_root);
    let base = format!("{FEATURE_PREFIX}phase-{phase:02}");

    // 1. Ensure the shared base branch exists off develop, without leaving the
    //    main checkout on it. Ref creation is serialized on the checkout lock
    //    like every other shared-ref mutation.
    {
        let _checkout_lock = lock::acquire_project_blocking(project_root, checkout_lock_timeout())
            .map_err(|err| CliError::Message(format!("could not lock checkout: {err}")))?;
        git.ensure_branch(&base, DEVELOP)?;
    }

    // 2. Create one worktree per agent, both off the current base tip.
    let branch_a = format!("{base}-{agent_a}");
    let branch_b = format!("{base}-{agent_b}");
    let wt_a = worktree::phase_agent_path(project_root, phase, &agent_a.to_string());
    let wt_b = worktree::phase_agent_path(project_root, phase, &agent_b.to_string());

    if force {
        for (wt, branch) in [(&wt_a, &branch_a), (&wt_b, &branch_b)] {
            if wt.exists() {
                worktree::remove(project_root, wt, true)?;
            }
            let _ = git.delete_branch(branch, true);
        }
    }

    add_or_explain(project_root, &wt_a, &branch_a, &base)?;
    add_or_explain(project_root, &wt_b, &branch_b, &base)?;
    println!("worktree A: {} ({branch_a})", wt_a.display());
    println!("worktree B: {} ({branch_b})", wt_b.display());

    // 3. Run agent A; stop before touching B if it fails.
    println!("\n=== agent A: {agent_a} ===");
    if let Some(result) = run_agent_blocking(project_root, phase, agent_a, &wt_a)? {
        match result.status {
            AgentStatus::Failed => {
                return Err(CliError::Message(format!(
                    "agent A ({agent_a}) failed: {}",
                    result.reason.unwrap_or_else(|| "no details".into())
                )));
            }
            AgentStatus::RateLimited => {
                let retry_after = retry_after_from_reason(result.reason.as_deref());
                write_rate_limit_cron(project_root, phase, &retry_after, agents)?;
                let commits = count_commits_between(project_root, &base, &branch_a)?;
                if commits == 0 {
                    println!(
                        "Agent A rate-limited with zero commits; paused — resume record at {}",
                        devflow_core::ship::cron_instructions_path(project_root, phase).display()
                    );
                    return Ok(());
                }
                println!("Agent A rate-limited; handing off to agent B");
            }
            _ => {}
        }
    }
    integrate_agent_branch(project_root, &git, &base, &branch_a)?;

    // 4. Rebase B onto the updated base; surface conflicts for manual fixing.
    git.rebase_in(&wt_b, &base).map_err(|err| {
        CliError::Message(format!(
            "rebase of {branch_b} onto {base} hit conflicts — resolve them in {} \
             then re-run sequentagent: {err}",
            wt_b.display()
        ))
    })?;
    println!("rebased {branch_b} onto {base}");

    // 5. Run agent B and integrate.
    println!("\n=== agent B: {agent_b} ===");
    if let Some(result) = run_agent_blocking(project_root, phase, agent_b, &wt_b)?
        && matches!(
            result.status,
            AgentStatus::Failed | AgentStatus::RateLimited
        )
    {
        let label = if result.status == AgentStatus::RateLimited {
            "rate-limited"
        } else {
            "failed"
        };
        return Err(CliError::Message(format!(
            "agent B ({agent_b}) {label}: {}",
            result.reason.unwrap_or_else(|| "no details".into())
        )));
    }
    integrate_agent_branch(project_root, &git, &base, &branch_b)?;
    // WR-02: the phase has shipped — a surviving cron-instructions file would
    // mislead `devflow status`/a Hermes cron into re-running it. A failed
    // delete must be visible, not swallowed, for the same reason.
    if let Err(err) = devflow_core::ship::delete_cron_instructions(project_root, phase) {
        println!("warning: could not remove cron-instructions file: {err}");
    }

    println!("\nsequentagent complete — both agents integrated into {base}");
    Ok(())
}

fn retry_after_from_reason(reason: Option<&str>) -> String {
    reason
        .and_then(|s| s.strip_prefix("rate limited until "))
        .unwrap_or("unknown")
        .to_string()
}

fn write_rate_limit_cron(
    project_root: &Path,
    phase: u32,
    retry_after: &str,
    agents: &str,
) -> Result<(), CliError> {
    let instructions =
        devflow_core::ship::build_cron_instructions(project_root, phase, retry_after, agents);
    devflow_core::ship::write_cron_instructions(project_root, &instructions)?;
    if instructions.hermes_cron.schedule.is_empty() {
        println!("no parseable retry time — auto-resume cron not scheduled; resume manually");
    } else {
        // 14-CR-08: name the file that was actually written (per-phase),
        // not the retired single-slot path.
        println!(
            "wrote {}",
            devflow_core::ship::cron_instructions_path(project_root, phase)
                .strip_prefix(project_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| {
                    devflow_core::ship::cron_instructions_path(project_root, phase)
                        .display()
                        .to_string()
                })
        );
    }
    Ok(())
}

fn count_commits_between(project_root: &Path, base: &str, branch: &str) -> Result<u32, CliError> {
    let range = format!("{base}..{branch}");
    let output = std::process::Command::new("git")
        .args(["rev-list", "--count", &range])
        .current_dir(project_root)
        .output()
        .map_err(|err| CliError::Message(format!("could not count commits on {branch}: {err}")))?;
    if !output.status.success() {
        return Err(CliError::Message(format!(
            "could not count commits on {branch}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|err| CliError::Message(format!("invalid commit count for {branch}: {err}")))
}

/// Add a worktree, turning the "already exists" error into actionable advice.
fn add_or_explain(
    project_root: &Path,
    path: &Path,
    branch: &str,
    base: &str,
) -> Result<(), CliError> {
    match worktree::add(project_root, path, branch, base, true) {
        Ok(()) => Ok(()),
        Err(devflow_core::worktree::WorktreeError::Exists(p)) => Err(CliError::Message(format!(
            "worktree already exists at {} — use --force to recreate it",
            p.display()
        ))),
        Err(err) => Err(err.into()),
    }
}

// ---------------------------------------------------------------------------
// reference / cleanup / list / status / recover
// ---------------------------------------------------------------------------

/// Create or refresh the static reference worktree.
fn reference(project_root: &Path, branch: Option<String>, refresh: bool) -> Result<(), CliError> {
    let branch = branch.unwrap_or_else(|| DEVELOP.to_string());
    let path = worktree::reference_path(project_root);

    // Detached snapshot: `branch` may already be checked out in the main
    // worktree, so we pin a detached HEAD at its tip rather than checking it out.
    if path.exists() {
        if !refresh {
            println!(
                "reference exists at {} (use --refresh to update it)",
                path.display()
            );
            return Ok(());
        }
        worktree::remove(project_root, &path, true)?;
        worktree::add_detached(project_root, &path, &branch)?;
        println!(
            "refreshed reference worktree at {} (snapshot of {branch})",
            path.display()
        );
    } else {
        worktree::add_detached(project_root, &path, &branch)?;
        println!(
            "created reference worktree at {} (snapshot of {branch})",
            path.display()
        );
    }
    Ok(())
}

/// Remove phase worktrees (and the reference with --force), deleting their
/// associated feature branches, then prune and clean up merged branches.
fn cleanup(project_root: &Path, force: bool) -> Result<(), CliError> {
    let git = GitFlow::new(project_root);
    let worktrees_dir = worktree::worktrees_dir(project_root);
    let reference = worktree::reference_path(project_root);

    let worktrees = worktree::list(project_root)?;
    let mut removed = 0usize;
    for wt in &worktrees {
        // Only touch worktrees under `.worktrees/` (never the main checkout).
        if !wt.path.starts_with(&worktrees_dir) {
            continue;
        }
        if wt.path == reference && !force {
            println!("keeping reference worktree (use --force to remove it)");
            continue;
        }
        worktree::remove(project_root, &wt.path, force)?;
        print!("removed worktree {}", wt.path.display());
        match &wt.branch {
            Some(branch) if branch.starts_with(FEATURE_PREFIX) => {
                match git.delete_branch(branch, force) {
                    Ok(()) => println!(" + deleted branch {branch}"),
                    Err(err) => println!(" (branch {branch} kept: {err})"),
                }
            }
            _ => println!(),
        }
        removed += 1;
    }

    worktree::prune(project_root)?;
    if removed == 0 {
        println!("no worktrees to clean up");
    }
    match git.cleanup_merged() {
        Ok(merged) => {
            for branch in merged {
                println!("deleted merged branch {branch}");
            }
        }
        Err(err) => println!("warning: could not prune merged branches: {err}"),
    }
    Ok(())
}

/// A phase's monitor/agent liveness, distinguishing a dead monitor (nothing
/// will call `devflow advance` when the agent exits) from a normal
/// between-stages moment (18b — "who watches the watcher").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Liveness {
    /// Monitor and agent are both alive — the stage is actively running.
    Healthy,
    /// Monitor is alive, agent has exited — normal between-stages moment;
    /// the monitor will advance the phase shortly.
    BetweenStages,
    /// The recorded monitor is dead. Whether or not the agent is also dead,
    /// nothing will call `devflow advance` for this phase — it needs a
    /// manual `devflow resume`.
    Stuck,
    /// No monitor PID has been recorded for this state — either none has
    /// been spawned yet, or the state was written by a binary predating
    /// this field. Never reported as a problem.
    Unknown,
}

impl Liveness {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Liveness::Healthy => "healthy",
            Liveness::BetweenStages => "between stages",
            Liveness::Stuck => "stuck — needs devflow resume",
            Liveness::Unknown => "unknown (no monitor recorded)",
        }
    }
}

/// Pure liveness predicate — no I/O. `monitor_pid` is matched `None` first
/// so a state written by a pre-18b binary (carrying no `monitor_pid`) can
/// never be misclassified as `Stuck` (T-18-11).
fn liveness(monitor_pid: Option<u32>, monitor_alive: bool, agent_alive: bool) -> Liveness {
    match monitor_pid {
        None => Liveness::Unknown,
        Some(_) => match (monitor_alive, agent_alive) {
            (true, true) => Liveness::Healthy,
            (true, false) => Liveness::BetweenStages,
            (false, _) => Liveness::Stuck,
        },
    }
}

fn status(project_root: &Path) -> Result<(), CliError> {
    // 13-DEFERRED-CR-03 acceptance: enumerate every active phase, not just
    // the last one started.
    let states = workflow::list_states(project_root);
    let mut current_worktree: Option<PathBuf> = None;
    if states.is_empty() {
        println!("stage: idle");
        println!("project_root: {}", project_root.display());
    } else {
        // 14-CR-10: one pass over events.jsonl for every phase's last event,
        // instead of a full-file scan per phase.
        let mut last_events = events::last_events_by_phase(project_root);
        println!("project_root: {}", project_root.display());
        println!(
            "active phases: {}",
            states
                .iter()
                .map(|s| s.phase.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        for state in &states {
            let gate = if state.gate_pending {
                "pending"
            } else {
                "none"
            };
            println!("\nphase {}:", state.phase);
            println!(
                "  stage: {} | mode: {} | gate: {}",
                state.stage, state.mode, gate
            );
            println!("  agent: {}", agents::adapter_for(state.agent).name());
            if state.consecutive_failures > 0 {
                println!("  validate failures: {}", state.consecutive_failures);
            }
            println!(
                "  started: {} ({})",
                state.started_at,
                recover::format_age(&state.started_at)
            );
            if let Some(ref wt) = state.worktree_path {
                println!("  worktree: {}", wt.display());
            }
            current_worktree = current_worktree.or_else(|| state.worktree_path.clone());
            let agent_pid = agent_pid_from_file(project_root, state.phase);
            match agent_pid {
                Some(pid) => {
                    println!(
                        "  agent_pid: {pid} (running: {})",
                        agent::agent_running(pid)
                    );
                }
                None => println!("  agent_pid: none"),
            }
            match state.monitor_pid {
                Some(pid) => {
                    println!(
                        "  monitor_pid: {pid} (running: {})",
                        agent::agent_running(pid)
                    );
                }
                None => println!("  monitor_pid: none"),
            }
            let agent_alive = agent_pid.is_some_and(agent::agent_running);
            let monitor_alive = state.monitor_pid.is_some_and(agent::agent_running);
            let phase_liveness = liveness(state.monitor_pid, monitor_alive, agent_alive);
            println!("  liveness: {}", phase_liveness.describe());
            if phase_liveness == Liveness::Stuck {
                println!("    → devflow resume --phase {}", state.phase);
            }
            if let Some(event) = last_events.remove(&state.phase) {
                let ago = event
                    .get("ts")
                    .and_then(|t| t.as_u64())
                    .map(|t| format!(" ({})", recover::format_age(&t.to_string())))
                    .unwrap_or_default();
                println!("  last action: {}{ago}", events::describe(&event));
            }
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Some(banner) = render_pending_gate_banner(&Gates::list_open(project_root), now) {
        println!("\n{banner}");
    }
    print_open_branches(project_root);
    print_worktrees(project_root, current_worktree.as_deref());
    for hint in cron_instruction_hints(project_root) {
        println!("\n{hint}");
    }
    Ok(())
}

/// Build the persistent status-side signal for gates awaiting an operator.
/// Context is agent-controlled, so it must use the same bounded rendering as
/// gate notifications and failure events.
fn render_pending_gate_banner(open: &[OpenGate], now: u64) -> Option<String> {
    if open.is_empty() {
        return None;
    }

    let mut banner = String::from("==================== PENDING GATE ====================\n");
    for gate in open {
        let timestamp = gate.timestamp.parse::<u64>().ok();
        let escalated = timestamp
            .and_then(|timestamp| now.checked_sub(timestamp))
            .is_some_and(|age| age >= GATE_ESCALATION_THRESHOLD_SECS);
        let marker = if escalated { "!!! ESCALATED" } else { "!!!" };
        let context = render_gate_context(&gate.context, 300);
        let stage = gate.stage.to_string();
        banner.push_str(&format!(
            "{marker}: phase {} {stage} ({})\n  {context}\n  approve: devflow gate approve {} --stage {stage}\n  reject:  devflow gate reject {} --stage {stage} --note <reason>\n",
            gate.phase,
            recover::format_age(&gate.timestamp),
            gate.phase,
            gate.phase,
        ));
    }
    banner.push_str("======================================================");
    Some(banner)
}

/// List every gate awaiting a human response.
fn gate_list(project_root: &Path) -> Result<(), CliError> {
    let open = Gates::list_open(project_root);
    if open.is_empty() {
        println!("no open gates");
        return Ok(());
    }
    println!("{:<6} {:<9} {:<9} CONTEXT", "PHASE", "STAGE", "AGE");
    for gate in &open {
        let context = render_gate_context(&gate.context, 100);
        println!(
            "{:<6} {:<9} {:<9} {context}",
            gate.phase,
            gate.stage.to_string(),
            recover::format_age(&gate.timestamp),
        );
    }
    println!(
        "\nanswer with: devflow gate approve <phase> [--note ...] | \
         devflow gate reject <phase> --note ... (note with \"abort\" ends the phase)"
    );
    Ok(())
}

/// Answer an open gate from the CLI — the dogfood-facing replacement for
/// hand-writing `.devflow/gates/NN-stage.response.json` (15a).
fn gate_respond(
    project_root: &Path,
    phase: u32,
    stage: Option<Stage>,
    approved: bool,
    note: Option<String>,
) -> Result<(), CliError> {
    let stage = match stage {
        Some(stage) => stage,
        None => {
            let open: Vec<_> = Gates::list_open(project_root)
                .into_iter()
                .filter(|g| g.phase == phase)
                .collect();
            match open.as_slice() {
                [] => {
                    return Err(CliError::Message(format!(
                        "no open gate for phase {phase} — see `devflow gate list`"
                    )));
                }
                [one] => one.stage,
                many => {
                    return Err(CliError::Message(format!(
                        "phase {phase} has several open gates ({}) — pass --stage",
                        many.iter()
                            .map(|g| g.stage.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        }
    };
    let responded_by = std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .unwrap_or_else(|| "devflow-cli".into());
    let response = GateResponse {
        approved,
        note,
        responded_by: Some(responded_by),
    };
    let path = Gates::respond(project_root, phase, stage, &response)?;
    events::emit(
        project_root,
        phase,
        "gate_response_written",
        serde_json::json!({
            "stage": stage.to_string(),
            "approved": approved,
            "via": "cli",
        }),
    );
    let outcome = match GateAction::from_response(&response) {
        GateAction::Advance => "workflow will advance",
        GateAction::LoopBack(_) => "workflow will loop back to Code",
        GateAction::Abort(_) => "phase will abort",
    };
    println!(
        "{} gate for phase {phase} {stage} — {outcome} once the waiting monitor polls it \
         (response at {})",
        if approved { "approved" } else { "rejected" },
        path.display()
    );
    Ok(())
}

/// Print (or follow) a phase's captured agent output.
fn logs(
    project_root: &Path,
    phase: Option<u32>,
    follow: bool,
    stderr: bool,
) -> Result<(), CliError> {
    let phase = match phase {
        Some(p) => p,
        None => default_logs_phase(project_root)?,
    };
    let path = if stderr {
        agent_result::stderr_path(project_root, phase)
    } else {
        agent_result::stdout_path(project_root, phase)
    };
    if !path.exists() && !follow {
        return Err(CliError::Message(format!(
            "no capture file for phase {phase} at {}",
            path.display()
        )));
    }
    eprintln!("== phase {phase}: {} ==", path.display());
    let mut offset = print_capture_from(&path, 0)?;
    if !follow {
        return Ok(());
    }
    // Follow until the agent's exit code lands AND one further quiescent
    // poll produced no new bytes — the natural end of a run. (An operator
    // can always Ctrl-C sooner.)
    let exit_path = agent_result::exit_code_path(project_root, phase);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        // 14-CR-03: a stage transition archives and recreates the capture
        // file (launch_stage → archive_phase_files), so a shrunken file
        // means the next stage started — reset to the top instead of
        // seeking past EOF forever and silently skipping its output.
        let base = rollover_offset(&path, offset);
        if base != offset {
            eprintln!("== capture restarted (next stage) — following from the top ==");
        }
        let new_offset = print_capture_from(&path, base)?;
        // Quiescent only if no rollover happened AND no new bytes appeared.
        if exit_path.exists() && base == offset && new_offset == offset {
            if let Ok(code) = std::fs::read_to_string(&exit_path) {
                eprintln!("== agent exited with code {} ==", code.trim());
            }
            return Ok(());
        }
        offset = new_offset;
    }
}

/// Render the read-only cross-attempt view for one phase.
fn history_cmd(project_root: &Path, phase: Option<u32>) -> Result<(), CliError> {
    let phase = match phase {
        Some(phase) => phase,
        None => single_active_phase(project_root)?.ok_or_else(|| {
            CliError::Message("no active phase — pass a phase number to `devflow history`".into())
        })?,
    };
    println!(
        "{}",
        history::render_timeline(&history::attempt_timeline(project_root, phase))
    );
    Ok(())
}

/// Detect capture-file rollover for `logs --follow` (14-CR-03): a file
/// shorter than the follower's offset was deleted and recreated by the next
/// stage's monitor, so following must restart from 0. A missing file (the
/// mid-rollover gap) keeps the current offset — the recreated file's shorter
/// length triggers the reset on a later poll if output restarted.
fn rollover_offset(path: &Path, offset: u64) -> u64 {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() < offset => 0,
        _ => offset,
    }
}

/// Print the capture file's contents from `offset`, returning the new offset.
/// A missing file is treated as empty (it may not exist yet under --follow).
fn print_capture_from(path: &Path, offset: u64) -> Result<u64, CliError> {
    let stdout = std::io::stdout();
    write_capture_from(path, offset, &mut stdout.lock())
}

fn write_capture_from(
    path: &Path,
    offset: u64,
    output: &mut impl std::io::Write,
) -> Result<u64, CliError> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return Ok(offset);
    };
    file.seek(SeekFrom::Start(offset))
        .map_err(|err| CliError::Message(format!("could not seek capture file: {err}")))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|err| CliError::Message(format!("could not read capture file: {err}")))?;
    if !buf.is_empty() {
        let _ = output.write_all(&buf);
        let _ = output.flush();
    }
    Ok(offset + buf.len() as u64)
}

/// Pick the phase `devflow logs` should show when none is given: the single
/// active phase, else the phase with the most recently modified capture file.
fn default_logs_phase(project_root: &Path) -> Result<u32, CliError> {
    if let Some(phase) = single_active_phase(project_root)? {
        return Ok(phase);
    }
    // No active state: fall back to the newest capture file on disk.
    let devflow = workflow::devflow_dir(project_root);
    let mut newest: Option<(std::time::SystemTime, u32)> = None;
    if let Ok(entries) = std::fs::read_dir(&devflow) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(phase) = name
                .strip_prefix("phase-")
                .and_then(|rest| rest.strip_suffix("-stdout"))
                .and_then(|num| num.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if newest.is_none_or(|(when, _)| modified > when) {
                newest = Some((modified, phase));
            }
        }
    }
    newest.map(|(_, phase)| phase).ok_or_else(|| {
        CliError::Message("no active phase and no capture files — nothing to show".into())
    })
}

/// Read the launched agent PID the monitor recorded for `phase`, if present.
fn agent_pid_from_file(project_root: &Path, phase: u32) -> Option<u32> {
    let path = agent_result::agent_pid_path(project_root, phase);
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn cron_instruction_hints(project_root: &Path) -> Vec<String> {
    devflow_core::ship::list_cron_instructions(project_root)
        .iter()
        .map(|instructions| {
            format!(
                "Cron instruction pending (phase {}): hermes cron create --from-devflow {}",
                instructions.phase,
                project_root.display()
            )
        })
        .collect()
}

/// Print active phase worktrees with branch and inferred phase/agent.
fn print_worktrees(project_root: &Path, current: Option<&Path>) {
    let worktrees_dir = worktree::worktrees_dir(project_root);
    let worktrees = match worktree::list(project_root) {
        Ok(w) => w,
        Err(_) => return,
    };
    let active: Vec<_> = worktrees
        .iter()
        .filter(|w| w.path.starts_with(&worktrees_dir))
        .collect();
    if active.is_empty() {
        return;
    }
    println!("\nactive worktrees:");
    for wt in active {
        let label = wt
            .path
            .file_name()
            .map(|n| describe_worktree_dir(&n.to_string_lossy()))
            .unwrap_or_default();
        let branch = wt.branch.as_deref().unwrap_or("(detached)");
        let marker = if current == Some(wt.path.as_path()) {
            " *"
        } else {
            ""
        };
        println!("  {} [{branch}]{label}{marker}", wt.path.display());
    }
}

/// Turn a worktree dir name like `phase-07-claude` into ` — phase 7, agent claude`.
fn describe_worktree_dir(name: &str) -> String {
    let Some(rest) = name.strip_prefix("phase-") else {
        return String::new();
    };
    match rest.split_once('-') {
        Some((phase, agent)) => {
            format!(" — phase {}, agent {agent}", phase.trim_start_matches('0'))
        }
        None => format!(" — phase {}", rest.trim_start_matches('0')),
    }
}

fn list(project_root: &Path) -> Result<(), CliError> {
    let git = GitFlow::new(project_root);
    let branches = git.list_feature_branches()?;
    if branches.is_empty() {
        println!("no open feature branches");
        return Ok(());
    }
    println!(
        "{:<25} {:>6} {:>7}  LAST COMMIT",
        "BRANCH", "AHEAD", "BEHIND"
    );
    for b in &branches {
        println!(
            "{:<25} {:>6} {:>7}  {}",
            b.name, b.ahead, b.behind, b.last_commit
        );
    }
    Ok(())
}

fn print_open_branches(project_root: &Path) {
    let git = GitFlow::new(project_root);
    let branches = match git.list_feature_branches() {
        Ok(b) => b,
        Err(_) => return,
    };
    if branches.is_empty() {
        return;
    }
    println!("\nopen branches:");
    for b in &branches {
        let staleness = if b.behind > 0 {
            format!(" ({} behind develop)", b.behind)
        } else {
            String::new()
        };
        println!("  {} — {} ahead{staleness}", b.name, b.ahead);
    }
}

fn project_root(project: PathBuf) -> Result<PathBuf, CliError> {
    if !project.exists() {
        return Err(CliError::Message(format!(
            "project path does not exist: {}",
            project.display()
        )));
    }

    let start = project
        .canonicalize()
        .map_err(|err| CliError::Message(format!("failed to resolve project path: {err}")))?;
    let mut probe = start.as_path();
    loop {
        if probe.join(".devflow").is_dir() {
            return Ok(probe.to_path_buf());
        }
        match probe.parent() {
            Some(parent) => probe = parent,
            None => return Ok(start),
        }
    }
}

fn recover_cmd(project_root: &Path, do_clean: bool, phase: Option<u32>) -> Result<(), CliError> {
    if do_clean {
        let warnings = match phase {
            // Explicit phase: clear it regardless of staleness (14-CR-01's
            // escape hatch for a wedged-but-fresh run).
            Some(phase) => recover::clean_phase(project_root, phase)?,
            // Implicit sweep: stale phases only.
            None => recover::clean(project_root)?,
        };
        for warning in &warnings {
            println!("warning: {warning}");
        }
        match phase {
            Some(phase) => println!("cleaned up workflow state for phase {phase}"),
            None => println!("cleaned up stale workflow state"),
        }
        return Ok(());
    }

    let statuses = match recover::inspect_all(project_root) {
        Ok(s) => s,
        Err(recover::RecoverError::NothingToRecover) => {
            println!("no state to recover — project is idle");
            return Ok(());
        }
        Err(err) => {
            return Err(CliError::Message(format!(
                "recover inspection failed: {err}"
            )));
        }
    };

    let mut any_stale = false;
    for status in &statuses {
        if let Some(only) = phase
            && status.state.phase != only
        {
            continue;
        }
        println!("phase: {}", status.state.phase);
        println!("  stage: {}", status.state.stage);
        println!("  mode: {}", status.state.mode);
        println!(
            "  agent: {}",
            agents::adapter_for(status.state.agent).name()
        );
        println!("  started: {} ({})", status.state.started_at, status.age);
        match agent_pid_from_file(project_root, status.state.phase) {
            Some(pid) => {
                let running = agent::agent_running(pid);
                println!("  agent_pid: {pid} (running: {running})");
                if !running {
                    println!("  agent is not running — the monitor may have already advanced");
                }
            }
            None => println!("  agent_pid: none"),
        }
        if status.is_stale {
            any_stale = true;
            println!("  state is stale");
        }
    }

    if any_stale {
        println!(
            "\nstale state found — `devflow recover --clean` clears stale phases only; \
             use `--clean --phase N` for a specific phase"
        );
    }

    Ok(())
}

/// Run the local quality gate: cargo test, clippy, and fmt --check.
fn test_cmd(project_root: &Path) -> Result<(), CliError> {
    let checks = [
        ("cargo test", "cargo test"),
        (
            "cargo clippy",
            "cargo clippy --workspace --all-targets -- -D warnings",
        ),
        ("cargo fmt --check", "cargo fmt --check"),
    ];
    let mut failures = Vec::new();
    for (label, cmd) in checks {
        println!("=== {label} ===");
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(project_root)
            .status()
            .map_err(|err| CliError::Message(format!("could not run `{cmd}`: {err}")))?;
        if status.success() {
            println!("  ✓ {label}");
        } else {
            println!("  ✗ {label}");
            failures.push(label);
        }
    }
    if failures.is_empty() {
        println!("\nall checks passed");
        Ok(())
    } else {
        Err(CliError::Message(format!(
            "quality checks failed: {}",
            failures.join(", ")
        )))
    }
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

/// One tool/environment check from `doctor`'s pre-existing audit (git,
/// cargo, agent CLIs, `RUST_LOG`, ...). Module-level (WR-01, 18-fix) so
/// `checks_json_value` and `doctor_json_body` can compose it into
/// `doctor --json`'s single output document without living inside `doctor`
/// itself.
pub(crate) struct Check {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) version: Option<String>,
    pub(crate) install_hint: Option<String>,
}

/// Audit the environment and report what's installed, missing, or broken.
fn doctor(project_root: &Path, json: bool) -> Result<(), CliError> {
    use std::process::Command;

    fn cmd_check(name: &str, cmd: &str, version_arg: &str, install_hint: &str) -> Check {
        match Command::new(cmd).arg(version_arg).output() {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("unknown")
                    .trim()
                    .to_string();
                Check {
                    name: name.into(),
                    status: "ok".into(),
                    version: Some(version),
                    install_hint: None,
                }
            }
            Ok(out) => {
                let detail = String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .next()
                    .unwrap_or("unknown")
                    .trim()
                    .to_string();
                Check {
                    name: name.into(),
                    status: "warn".into(),
                    version: Some(detail),
                    install_hint: Some(format!(
                        "`{cmd} {version_arg}` exited non-zero — reinstall or check PATH"
                    )),
                }
            }
            Err(_) => Check {
                name: name.into(),
                status: "missing".into(),
                version: None,
                install_hint: Some(install_hint.into()),
            },
        }
    }

    fn bool_check(name: &str, ok: bool, version: &str, install_hint: &str) -> Check {
        Check {
            name: name.into(),
            status: if ok { "ok".into() } else { "missing".into() },
            version: Some(version.into()),
            install_hint: if ok { None } else { Some(install_hint.into()) },
        }
    }

    let devflow_version = env!("CARGO_PKG_VERSION");

    // RUST_LOG environment check: validate the value is a parsable log directive.
    let (rust_log_status, rust_log_version, rust_log_hint) = match std::env::var("RUST_LOG") {
        Ok(ref val) if val.is_empty() => (
            "warn",
            Some("empty (logging disabled)".into()),
            Some("Set RUST_LOG=info for better diagnostics".into()),
        ),
        Ok(val) => {
            let all_valid = val.split(',').all(|directive| {
                let directive = directive.trim();
                if let Some((_target, level)) = directive.split_once('=') {
                    matches!(level.trim(), "error" | "warn" | "info" | "debug" | "trace")
                } else {
                    matches!(directive, "error" | "warn" | "info" | "debug" | "trace")
                }
            });
            if all_valid {
                ("ok", Some(val), None)
            } else {
                (
                    "warn",
                    Some(val),
                    Some("RUST_LOG value may be invalid — expected error, warn, info, debug, or trace".into()),
                )
            }
        }
        Err(_) => (
            "missing",
            Some("not set — defaulting to info".into()),
            Some("Set RUST_LOG=info for better diagnostics".into()),
        ),
    };

    let checks: Vec<Check> = vec![
        cmd_check(
            "git",
            "git",
            "--version",
            "Install from https://git-scm.com/downloads",
        ),
        bool_check("sh (POSIX shell)", cfg!(unix), "built-in", "Unsupported OS"),
        cmd_check(
            "cargo/rust",
            "cargo",
            "--version",
            "curl https://sh.rustup.rs -sSf | sh",
        ),
        cmd_check(
            "gh CLI",
            "gh",
            "--version",
            "brew install gh / apt install gh",
        ),
        cmd_check(
            "claude",
            "claude",
            "--version",
            "npm i -g @anthropic-ai/claude-code",
        ),
        cmd_check("codex", "codex", "--version", "npm i -g @openai/codex"),
        cmd_check(
            "opencode",
            "opencode",
            "--version",
            "cargo install opencode",
        ),
        Check {
            name: format!("devflow v{devflow_version}"),
            status: "ok".into(),
            version: Some(devflow_version.into()),
            install_hint: None,
        },
        Check {
            name: "RUST_LOG".into(),
            status: rust_log_status.into(),
            version: rust_log_version,
            install_hint: rust_log_hint,
        },
    ];

    let facts = collect_phase_facts(project_root);

    if json {
        // WR-01 (18-fix): a single top-level JSON document — `{"environment":
        // [...], "reconciliation": [...]}` — instead of the pre-fix
        // behavior of printing the tool checks as one top-level `[...]`
        // array and then printing a SECOND, independent top-level array
        // right after it. That concatenation is not valid single-document
        // JSON for any parser that isn't NDJSON-aware (`json.load` raised
        // "Extra data").
        let body = doctor_json_body(&checks, &facts);
        println!(
            "{}",
            serde_json::to_string_pretty(&body).expect("doctor --json body must serialize")
        );
    } else {
        for c in &checks {
            let icon = match c.status.as_str() {
                "ok" => "✓",
                "missing" => "✗",
                "warn" => "⚠",
                _ => "?",
            };
            let version_str = c.version.as_deref().unwrap_or("-");
            print!("  {:<20} {:<20} {}", c.name, version_str, icon);
            #[allow(clippy::collapsible_if)]
            if c.status == "missing" || c.status == "warn" {
                if let Some(hint) = &c.install_hint {
                    print!(" — {}", hint);
                }
            }
            println!();
        }
        print!("{}", render_reconciliation_text(&facts));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// doctor reconciliation (18a)
// ---------------------------------------------------------------------------

/// Severity of a reconciliation finding, matching the existing `Check.status`
/// convention (lowercase strings) so both `doctor` renderers stay consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Ok,
    Warn,
    Problem,
}

impl Severity {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Severity::Ok => "ok",
            Severity::Warn => "warn",
            Severity::Problem => "problem",
        }
    }
}

/// The read-only facts `doctor` gathers for one active phase before
/// reconciling them. Collected by `collect_phase_facts` (all I/O); consumed
/// with zero I/O by `reconcile_phase`.
pub(crate) struct PhaseFacts {
    pub(crate) phase: u32,
    pub(crate) stage: Stage,
    pub(crate) gate_pending: bool,
    pub(crate) agent_pid: Option<u32>,
    pub(crate) agent_alive: bool,
    /// The monitor pid recorded in `State.monitor_pid` (18b). `None` means
    /// no monitor has been spawned for this state yet, or the state was
    /// written by a binary predating the field — never treated as a problem.
    pub(crate) monitor_pid: Option<u32>,
    pub(crate) monitor_alive: bool,
    /// The most recent event's `event` field value, for display context.
    pub(crate) last_event: Option<String>,
    /// The `stage` field of the most recent `stage_launched` event; `None`
    /// when the last event recorded for this phase is not a launch.
    pub(crate) last_launched_stage: Option<Stage>,
    pub(crate) open_gate_stages: Vec<Stage>,
    pub(crate) feature_branch_exists: bool,
}

/// One diagnostic finding for a phase, with a copy-pasteable repair command
/// when one exists. Never carries a filesystem path or username (T-18-01) —
/// only phase numbers, stage names, and pids identify the disagreement.
pub(crate) struct PhaseFinding {
    pub(crate) phase: u32,
    pub(crate) severity: Severity,
    pub(crate) detail: String,
    pub(crate) repair: Option<String>,
}

/// `gate_pending` is set but no gate file is open for this phase — the gate
/// answer path is stuck. `doctor` only reports this; it never repairs it
/// (T-18-02).
fn check_gate_pending_without_gate(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if !facts.gate_pending || !facts.open_gate_stages.is_empty() {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: gate_pending is true at stage {} but no gate file is open",
            facts.phase, facts.stage
        ),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}

/// An open gate file exists but `gate_pending` is false — an unanswered
/// operator question that `status`/`doctor` isn't surfacing as pending.
fn check_orphan_gate(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.gate_pending || facts.open_gate_stages.is_empty() {
        return None;
    }
    let gate_stage = facts.open_gate_stages[0];
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: gate open for stage {} but state.gate_pending is false",
            facts.phase, gate_stage
        ),
        repair: Some(format!(
            "devflow gate approve {} --stage {}",
            facts.phase, gate_stage
        )),
    })
}

/// The recorded agent pid is not alive while the phase sits at an
/// agent-driven stage — the "who watches the watcher" class of silent death
/// CONTEXT.md cites (two incidents, ~4h lost, found only via `ps`).
fn check_dead_agent(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let pid = facts.agent_pid?;
    if facts.agent_alive || !facts.stage.is_agent_stage() {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: agent pid {pid} recorded but not running at stage {}",
            facts.phase, facts.stage
        ),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}

/// The recorded monitor pid is dead — nothing will call `devflow advance`
/// for this phase, whether or not the agent is also dead (an agent that
/// outlived its monitor is orphaned too, since nothing will advance it when
/// it exits either). Reuses `liveness` rather than re-deriving the matrix,
/// so the two copies can never drift (18b, T-18-11's `Unknown` guard applies
/// here transitively — an unrecorded monitor is silently `Unknown`, never a
/// finding).
fn check_dead_monitor(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if liveness(facts.monitor_pid, facts.monitor_alive, facts.agent_alive) != Liveness::Stuck {
        return None;
    }
    let pid = facts.monitor_pid?;
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: monitor pid {pid} recorded but not running at stage {}",
            facts.phase, facts.stage
        ),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}

/// The last `stage_launched` event named a different stage than
/// `state.stage`. A `Warn`, not a `Problem` — a healthy pipeline legitimately
/// has one stage in flight between the launch event and the next
/// transition; exact equality is agreement, never an off-by-one mismatch.
fn check_stage_event_drift(facts: &PhaseFacts) -> Option<PhaseFinding> {
    let launched = facts.last_launched_stage?;
    if launched == facts.stage {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Warn,
        detail: format!(
            "phase {}: last stage_launched event named {launched} but state.stage is {}",
            facts.phase, facts.stage
        ),
        repair: None,
    })
}

/// The phase's feature branch does not exist even though its stage is past
/// `Define`. A `Warn` — a not-yet-pushed or manually deleted branch is
/// recoverable without state surgery.
fn check_missing_branch(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if facts.feature_branch_exists || facts.stage == Stage::Define {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Warn,
        detail: format!(
            "phase {}: feature/phase-{:02} does not exist but stage is {}",
            facts.phase, facts.phase, facts.stage
        ),
        repair: None,
    })
}

/// Pure reconciliation core: diffs `state.stage` against the latest event,
/// live agent pid, open gates, and branch existence, evaluating checks in a
/// fixed order so the returned findings never depend on how `facts` was
/// assembled (ordering edge). Takes no path, performs no I/O, and mutates
/// nothing (T-18-02) — directly unit-testable without a repository.
fn reconcile_phase(facts: &PhaseFacts) -> Vec<PhaseFinding> {
    [
        check_gate_pending_without_gate(facts),
        check_orphan_gate(facts),
        check_dead_agent(facts),
        check_dead_monitor(facts),
        check_stage_event_drift(facts),
        check_missing_branch(facts),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Gather the read-only facts `reconcile_phase` needs for every active
/// phase, sorted by phase ascending so output ordering never depends on
/// directory-read order (ordering edge). Every call here is a read-only
/// primitive already used elsewhere (`status`, `recover::inspect_all`) —
/// none of it is reimplemented.
fn collect_phase_facts(project_root: &Path) -> Vec<PhaseFacts> {
    let states = workflow::list_states(project_root);
    // 14-CR-10: one pass over events.jsonl for every phase's last event,
    // matching status()'s optimization, not a per-phase rescan.
    let mut last_events = events::last_events_by_phase(project_root);
    let open_gates = Gates::list_open(project_root);

    let mut facts: Vec<PhaseFacts> = states
        .into_iter()
        .map(|state| build_phase_facts(project_root, state, &mut last_events, &open_gates))
        .collect();

    facts.sort_by_key(|f| f.phase);
    facts
}

/// Build one phase's [`PhaseFacts`] from already-fetched state, events, and
/// gates — the per-phase half of `collect_phase_facts`, split out to keep
/// that function short.
fn build_phase_facts(
    project_root: &Path,
    state: State,
    last_events: &mut std::collections::HashMap<u32, serde_json::Value>,
    open_gates: &[OpenGate],
) -> PhaseFacts {
    let phase = state.phase;
    let agent_pid = agent_pid_from_file(project_root, phase);
    let agent_alive = agent_pid.is_some_and(agent::agent_running);
    let monitor_pid = state.monitor_pid;
    let monitor_alive = monitor_pid.is_some_and(agent::agent_running);
    let last_event = last_events.remove(&phase);
    let last_launched_stage = last_event.as_ref().and_then(last_launched_stage_from_event);
    let last_event_name = last_event
        .as_ref()
        .and_then(|e| e.get("event"))
        .and_then(|e| e.as_str())
        .map(str::to_string);
    let open_gate_stages = open_gates
        .iter()
        .filter(|g| g.phase == phase)
        .map(|g| g.stage)
        .collect();
    let branch_ref = format!("refs/heads/feature/phase-{phase:02}");
    let feature_branch_exists =
        run_git_stdout(project_root, &["rev-parse", "--verify", &branch_ref]).is_some();

    PhaseFacts {
        phase,
        stage: state.stage,
        gate_pending: state.gate_pending,
        agent_pid,
        agent_alive,
        monitor_pid,
        monitor_alive,
        last_event: last_event_name,
        last_launched_stage,
        open_gate_stages,
        feature_branch_exists,
    }
}

/// Derive the stage named by an event's `stage` field, but only when the
/// event's `event` field is `"stage_launched"` — any other event kind (or
/// an unparsable stage name) yields `None`, never a panic.
fn last_launched_stage_from_event(event: &serde_json::Value) -> Option<Stage> {
    if event.get("event").and_then(|e| e.as_str()) != Some("stage_launched") {
        return None;
    }
    event
        .get("stage")
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<Stage>().ok())
}

/// The findings to display for one phase: real findings when any exist,
/// otherwise a single synthetic `ok` finding — the display-only counterpart
/// to `reconcile_phase`'s "zero findings" agreement case, shared by both
/// the text and `--json` renderers.
fn findings_for_display(facts: &PhaseFacts) -> Vec<PhaseFinding> {
    let findings = reconcile_phase(facts);
    if !findings.is_empty() {
        return findings;
    }
    vec![PhaseFinding {
        phase: facts.phase,
        severity: Severity::Ok,
        detail: format!("phase {}: ok", facts.phase),
        repair: None,
    }]
}

/// Build `doctor`'s per-phase reconciliation section (after the existing
/// tool/env checks), read-only: it never calls `workflow::save_state`,
/// `events::emit`, `Gates::cleanup`/`Gates::write`, or any `recover::clean*`
/// function (T-18-02). A pure string builder (not a direct `println!`) so
/// it's directly assertable in tests without capturing process stdout.
fn render_reconciliation_text(facts: &[PhaseFacts]) -> String {
    let mut out = String::from("\nreconciliation:\n");
    if facts.is_empty() {
        out.push_str("  no active phases — nothing to reconcile\n");
        return out;
    }
    for phase_facts in facts {
        for finding in findings_for_display(phase_facts) {
            out.push_str(&format!("  {}\n", finding.detail));
            if let Some(repair) = &finding.repair {
                out.push_str(&format!("    repair: {repair}\n"));
            }
        }
    }
    out
}

/// Build the `--json` reconciliation array as a `serde_json::Value` (WR-01,
/// 18-fix). No longer prints its own top-level `[...]` document — `doctor()`
/// nests this under `"reconciliation"` in the single object
/// `doctor_json_body` composes alongside `checks_json_value`'s
/// `"environment"` array.
fn render_reconciliation_json(facts: &[PhaseFacts]) -> serde_json::Value {
    // Pair each finding with its originating phase's last recorded event, so
    // a `--json` consumer gets that context without re-reading events.jsonl.
    let findings: Vec<(&PhaseFacts, PhaseFinding)> = facts
        .iter()
        .flat_map(|pf| findings_for_display(pf).into_iter().map(move |f| (pf, f)))
        .collect();
    serde_json::Value::Array(
        findings
            .iter()
            .map(|(phase_facts, finding)| {
                serde_json::json!({
                    "phase": finding.phase,
                    "severity": finding.severity.label(),
                    "detail": finding.detail,
                    "repair": finding.repair,
                    "last_event": phase_facts.last_event,
                })
            })
            .collect(),
    )
}

/// Build `doctor --json`'s `"environment"` array from the pre-existing
/// tool/env checks (WR-01, 18-fix). Extracted so it can be composed with
/// `render_reconciliation_json`'s array into ONE JSON document instead of
/// being printed as its own top-level array.
fn checks_json_value(checks: &[Check]) -> serde_json::Value {
    serde_json::Value::Array(
        checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "status": c.status,
                    "version": c.version,
                    "install_hint": c.install_hint,
                })
            })
            .collect(),
    )
}

/// Compose `doctor --json`'s single JSON document (WR-01, 18-fix). Pre-fix,
/// `doctor()` printed the tool checks as one top-level `[...]` array and
/// then printed `render_reconciliation_json`'s array as a SECOND,
/// independent top-level array right after it — invalid single-document
/// JSON for any parser that isn't NDJSON-aware (`json.load` raised "Extra
/// data" against a live fixture with one active phase). There is now
/// exactly one top-level value: `{"environment": [...], "reconciliation":
/// [...]}`.
fn doctor_json_body(checks: &[Check], facts: &[PhaseFacts]) -> serde_json::Value {
    serde_json::json!({
        "environment": checks_json_value(checks),
        "reconciliation": render_reconciliation_json(facts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn project_root_walks_up_to_nearest_devflow_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let nested = root.join(".worktrees/phase-16/deep");
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        assert_eq!(project_root(nested).unwrap(), root.canonicalize().unwrap());

        let idle = dir.path().join("idle/nested");
        std::fs::create_dir_all(&idle).unwrap();
        assert_eq!(
            project_root(idle.clone()).unwrap(),
            idle.canonicalize().unwrap()
        );

        let missing = dir.path().join("missing");
        let error = project_root(missing).unwrap_err().to_string();
        assert!(error.contains("project path does not exist"));
    }

    #[test]
    fn gate_approve_arg_parsing_accepts_positional_stage() {
        let cli = Cli::try_parse_from(["devflow", "gate", "approve", "15", "ship"]).unwrap();
        let Command::Gate {
            action: GateCmd::Approve { stage, project, .. },
        } = cli.command
        else {
            panic!("expected gate approve command");
        };

        assert_eq!(stage.as_deref(), Some("ship"));
        assert_eq!(project, PathBuf::from("."));

        let flagged =
            Cli::try_parse_from(["devflow", "gate", "approve", "15", "--stage", "ship"]).unwrap();
        let Command::Gate {
            action:
                GateCmd::Approve {
                    stage,
                    stage_option,
                    ..
                },
        } = flagged.command
        else {
            panic!("expected flagged gate approve command");
        };
        assert_eq!(stage, None);
        assert_eq!(stage_option, Some(Stage::Ship));

        let bare = Cli::try_parse_from(["devflow", "gate", "approve", "15"]).unwrap();
        let Command::Gate {
            action:
                GateCmd::Approve {
                    stage,
                    stage_option,
                    ..
                },
        } = bare.command
        else {
            panic!("expected bare gate approve command");
        };
        assert_eq!(stage, None);
        assert_eq!(stage_option, None);

        let legacy =
            Cli::try_parse_from(["devflow", "gate", "approve", "15", "/tmp/example-project"])
                .unwrap();
        let Command::Gate {
            action:
                GateCmd::Approve {
                    stage,
                    legacy_project,
                    stage_option,
                    project,
                    ..
                },
        } = legacy.command
        else {
            panic!("expected legacy gate approve command");
        };
        let (stage, project) =
            resolve_gate_target(stage, legacy_project, stage_option, project).unwrap();
        assert_eq!(stage, None);
        assert_eq!(project, PathBuf::from("/tmp/example-project"));
    }

    #[test]
    fn pairs_default_missing_agents_to_claude() {
        let pairs = parse_phase_agent_pairs("7,8", Some("codex")).unwrap();
        assert_eq!(pairs, vec![(7, AgentKind::Codex), (8, AgentKind::Claude)]);
    }

    #[test]
    fn pairs_match_agents_positionally() {
        let pairs = parse_phase_agent_pairs("7, 8", Some("claude, codex")).unwrap();
        assert_eq!(pairs, vec![(7, AgentKind::Claude), (8, AgentKind::Codex)]);
    }

    #[test]
    fn pairs_default_all_to_claude_without_agents() {
        let pairs = parse_phase_agent_pairs("3,4", None).unwrap();
        assert_eq!(pairs, vec![(3, AgentKind::Claude), (4, AgentKind::Claude)]);
    }

    #[test]
    fn pairs_reject_more_agents_than_phases() {
        let err = parse_phase_agent_pairs("7", Some("claude,codex")).unwrap_err();
        assert!(matches!(err, CliError::Message(_)));
    }

    #[test]
    fn pairs_reject_invalid_phase() {
        assert!(parse_phase_agent_pairs("7,x", None).is_err());
        assert!(parse_phase_agent_pairs("", None).is_err());
    }

    #[test]
    fn describe_worktree_dir_infers_phase_and_agent() {
        assert_eq!(
            describe_worktree_dir("phase-07-claude"),
            " — phase 7, agent claude"
        );
        assert_eq!(describe_worktree_dir("phase-08"), " — phase 8");
        assert_eq!(describe_worktree_dir("reference"), "");
    }

    #[test]
    fn split_two_agents_requires_exactly_two() {
        assert_eq!(
            split_two_agents("claude, codex").unwrap(),
            (AgentKind::Claude, AgentKind::Codex)
        );
        assert!(split_two_agents("claude").is_err());
        assert!(split_two_agents("claude,codex,opencode").is_err());
        assert!(split_two_agents("claude,bogus").is_err());
    }

    #[test]
    fn retry_after_from_reason_strips_prefix() {
        assert_eq!(
            retry_after_from_reason(Some("rate limited until 2026-06-18T15:45:30Z")),
            "2026-06-18T15:45:30Z"
        );
        assert_eq!(retry_after_from_reason(Some("usage limit")), "unknown");
        assert_eq!(retry_after_from_reason(None), "unknown");
    }

    #[test]
    fn cron_instruction_hints_include_hermes_command_per_phase() {
        let dir = tempfile::tempdir().unwrap();
        for phase in [7, 9] {
            let instructions = devflow_core::ship::build_cron_instructions(
                dir.path(),
                phase,
                "2026-06-18T15:45:30Z",
                "claude,codex",
            );
            devflow_core::ship::write_cron_instructions(dir.path(), &instructions).unwrap();
        }

        let hints = cron_instruction_hints(dir.path());

        assert_eq!(hints.len(), 2);
        assert_eq!(
            hints[0],
            format!(
                "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
                dir.path().display()
            )
        );
        assert!(hints[1].contains("(phase 9)"));
    }

    #[test]
    fn parse_checkout_lock_timeout_defaults_and_parses() {
        assert_eq!(
            parse_checkout_lock_timeout(None),
            std::time::Duration::from_secs(120)
        );
        assert_eq!(
            parse_checkout_lock_timeout(Some("5".into())),
            std::time::Duration::from_secs(5)
        );
        assert_eq!(
            parse_checkout_lock_timeout(Some("nope".into())),
            std::time::Duration::from_secs(120)
        );
    }

    /// 14-CR-02: when the checkout lock cannot be acquired, the hook batch
    /// must be SKIPPED — never run unserialized against the shared checkout
    /// — and the skip must be recorded in events.jsonl. `ChangelogAppend`
    /// would observably create `CHANGELOG.md` if the batch ran; it moved
    /// from the Validate→Ship batch into `hooks_after_ship()` in 17-12
    /// (WR-04), so this test now drives that batch instead — none of its
    /// hooks execute here regardless (the lock check short-circuits before
    /// the first hook runs), so no real merge/version state is needed.
    /// Env-mutating, so serialized under ENV_MUTEX; the "0" timeout only
    /// affects a concurrent test if it is actually contended, which none are
    /// (no other test holds the project lock).
    #[test]
    fn checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A live holder (this process) keeps the lock contended; the stale-
        // holder reclaim cannot fire.
        let _held = lock::acquire_project(root).expect("hold checkout lock");
        unsafe {
            std::env::set_var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS", "0");
        }

        let state = State::new(33, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            std::env::remove_var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS");
        }

        assert!(
            !root.join("CHANGELOG.md").exists(),
            "hooks must not run while the checkout lock is held elsewhere"
        );
        let last = devflow_core::events::last_event_for_phase(root, 33)
            .expect("skip must be recorded in events.jsonl");
        assert_eq!(last["event"], "hook_run");
        assert_eq!(last["ok"], false);
        assert_eq!(last["skipped"], "checkout lock timeout");
    }

    #[test]
    fn terminal_hook_failure_stops_before_branch_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = 34;
        let branch = "feature/phase-34";
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["branch", branch, "develop"]);
        // Force VersionBump to fail after Merge succeeds.
        std::fs::remove_file(root.join("Cargo.toml")).unwrap();
        std::fs::create_dir(root.join("Cargo.toml")).unwrap();

        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let succeeded = run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);

        assert!(!succeeded);
        assert!(
            GitFlow::new(root).branch_exists(branch),
            "a failed terminal batch must preserve the branch for retry"
        );
    }

    /// GAP-8 (17-VALIDATION.md): GAP-7 fixed `HookContext.shipped_version`
    /// forwarding `hooks_after_ship`'s `VersionBump` tag to `ChangelogAppend`
    /// within the same batch — but only the `devflow-core::hooks` unit tests
    /// exercised it directly by hand-rolling their own context and looping
    /// over `hooks_after_ship()`. `run_checkout_hooks` is the ONLY production
    /// caller of that batch, and it must construct the `HookContext` once,
    /// above the hook loop, for the forwarding to survive into production.
    /// This test drives `run_checkout_hooks` itself (not a hand-rolled loop)
    /// against a repo with no version file, and asserts the changelog
    /// heading names the actual tagged version rather than falling back to
    /// the "unreleased" literal.
    #[test]
    fn run_checkout_hooks_keeps_changelog_in_sync_with_tag_when_no_version_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo_no_version_file(root);

        let phase = 47;
        let branch = format!("feature/phase-{phase:02}");
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["branch", &branch, "develop"]);
        std::fs::write(root.join(".gitignore"), ".devflow/\n").unwrap();
        git(&["checkout", &branch]);
        std::fs::write(root.join("feature.txt"), "phase work\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "phase work"]);
        git(&["checkout", "develop"]);

        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let succeeded = run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);
        assert!(
            succeeded,
            "after-ship batch must succeed against a clean repo"
        );

        let all_tags = std::process::Command::new("git")
            .arg("tag")
            .current_dir(root)
            .output()
            .unwrap();
        let all_tags = String::from_utf8_lossy(&all_tags.stdout);
        assert_eq!(all_tags.lines().count(), 1, "expected exactly one tag");
        let tag = all_tags.trim().to_string();
        let tag_version = tag
            .strip_prefix('v')
            .expect("tag should be prefixed with v")
            .to_string();

        let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
        let changelog_version = changelog
            .lines()
            .find(|l| l.starts_with("## "))
            .and_then(|l| l.trim_start_matches("## ").split(' ').next())
            .unwrap()
            .to_string();

        assert_ne!(
            changelog_version, "unreleased",
            "changelog heading must name the tagged version, not fall back to the literal"
        );
        assert_eq!(
            changelog_version, tag_version,
            "changelog heading must match the git tag ({tag}) produced by the same \
             run_checkout_hooks call, even with no version file"
        );
    }

    #[test]
    fn default_logs_phase_prefers_single_active_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(6, AgentKind::Claude, Mode::Auto, dir.path().to_path_buf());
        workflow::save_state(&state).unwrap();

        assert_eq!(default_logs_phase(dir.path()).unwrap(), 6);
    }

    #[test]
    fn default_logs_phase_is_ambiguous_with_two_active_states() {
        let dir = tempfile::tempdir().unwrap();
        for phase in [6, 7] {
            let state = State::new(
                phase,
                AgentKind::Claude,
                Mode::Auto,
                dir.path().to_path_buf(),
            );
            workflow::save_state(&state).unwrap();
        }

        let err = default_logs_phase(dir.path()).unwrap_err();
        assert!(err.to_string().contains("--phase"));
    }

    #[test]
    fn default_logs_phase_falls_back_to_newest_capture_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(agent_result::stdout_path(dir.path(), 3), "old").unwrap();
        // Ensure a strictly newer mtime on the second capture.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(agent_result::stdout_path(dir.path(), 5), "new").unwrap();

        assert_eq!(default_logs_phase(dir.path()).unwrap(), 5);
    }

    #[test]
    fn default_logs_phase_errors_with_nothing_to_show() {
        let dir = tempfile::tempdir().unwrap();
        assert!(default_logs_phase(dir.path()).is_err());
    }

    /// 18b: a state with no recorded monitor is never reported as stuck,
    /// regardless of the (unreliable, since no monitor was ever recorded)
    /// liveness bits passed alongside it.
    #[test]
    fn liveness_unknown_when_no_monitor_recorded() {
        assert_eq!(liveness(None, false, false), Liveness::Unknown);
        assert_eq!(liveness(None, false, true), Liveness::Unknown);
        assert_eq!(liveness(None, true, false), Liveness::Unknown);
        assert_eq!(liveness(None, true, true), Liveness::Unknown);
    }

    /// 18b: the full four-row matrix for a recorded monitor pid. A dead
    /// agent with a dead monitor OR a live monitor with a dead agent are
    /// different states — only the former is `Stuck` (nothing will call
    /// `devflow advance`); the latter is a normal between-stages moment. An
    /// agent that outlived its monitor is also `Stuck` — orphaned, since
    /// nothing will advance it when it exits either.
    #[test]
    fn liveness_matrix_covers_all_four_rows() {
        let pid = Some(4242);
        assert_eq!(liveness(pid, true, true), Liveness::Healthy);
        assert_eq!(liveness(pid, true, false), Liveness::BetweenStages);
        assert_eq!(liveness(pid, false, false), Liveness::Stuck);
        assert_eq!(liveness(pid, false, true), Liveness::Stuck);
    }

    /// 18b: a corrupt pid (0, or above `i32::MAX`) must never read as alive
    /// — `liveness` relies entirely on `agent::agent_running`'s existing
    /// hardening (no second probe is written), so it can only ever produce
    /// `Stuck` or `Unknown` for a corrupt pid, never a false `Healthy`.
    #[test]
    fn liveness_treats_zero_and_overflow_pids_as_dead() {
        assert!(!agent::agent_running(0));
        assert!(!agent::agent_running(u32::MAX));
    }

    /// 18b: persisting `monitor_pid` for one phase must not disturb a
    /// concurrently-active sibling phase's `monitor_pid` (concurrency edge).
    #[test]
    fn monitor_pid_persisted_for_one_phase_does_not_disturb_a_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut phase7 = State::new(7, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        phase7.monitor_pid = Some(111);
        workflow::save_state(&phase7).unwrap();

        let mut phase8 = State::new(8, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        phase8.monitor_pid = Some(222);
        workflow::save_state(&phase8).unwrap();

        let reloaded7 = workflow::load_state(root, 7).unwrap();
        let reloaded8 = workflow::load_state(root, 8).unwrap();
        assert_eq!(reloaded7.monitor_pid, Some(111));
        assert_eq!(reloaded8.monitor_pid, Some(222));
    }

    /// 18b: after `launch_stage` spawns a monitor, the persisted state file
    /// for that phase carries the monitor's pid — `transition()` saves state
    /// BEFORE calling `launch_stage`, so the pid must be saved again inside
    /// `launch_stage` or it is lost.
    #[test]
    fn launch_stage_persists_monitor_pid_for_reload() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 65;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = launch_stage(&mut state, None, None);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
        result.unwrap();

        assert!(
            state.monitor_pid.is_some(),
            "launch_stage must record the monitor pid on the in-memory state"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.monitor_pid, state.monitor_pid,
            "the monitor pid recorded by launch_stage must be persisted to disk, \
             since transition() saves state before launch_stage runs"
        );
    }

    /// 18b (idempotency edge): running `devflow status` twice must produce
    /// byte-identical `.devflow/` state — the new monitor liveness probe is
    /// purely a read, same as the existing agent liveness probe it sits
    /// beside. Also exercises the `u32::MAX` boundary pid (precision edge,
    /// via `agent::agent_running`'s existing hardening) so the probe can
    /// only ever report `Stuck`, never a false `Healthy`.
    #[test]
    fn status_reading_monitor_liveness_writes_no_state_and_no_event() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 66;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.monitor_pid = Some(u32::MAX);
        workflow::save_state(&state).unwrap();

        let state_path = workflow::state_path(root, phase);
        let before_len = std::fs::metadata(&state_path).unwrap().len();
        let before_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
        let events_log = events::events_path(root);
        let before_lines = std::fs::read_to_string(&events_log)
            .unwrap_or_default()
            .lines()
            .count();

        status(root).unwrap();
        status(root).unwrap();

        let after_len = std::fs::metadata(&state_path).unwrap().len();
        let after_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
        let after_lines = std::fs::read_to_string(&events_log)
            .unwrap_or_default()
            .lines()
            .count();

        assert_eq!(
            before_len, after_len,
            "status must not rewrite the state file"
        );
        assert_eq!(
            before_modified, after_modified,
            "status must not touch the state file's mtime"
        );
        assert_eq!(
            before_lines, after_lines,
            "status must not append to events.jsonl"
        );
    }

    /// 15a: `devflow gate approve` resolves the stage automatically when a
    /// phase has exactly one open gate and writes a response the workflow's
    /// poller will consume.
    #[test]
    fn gate_respond_auto_resolves_single_open_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Gates::write_gate(root, 15, Stage::Ship, "approve merge?").unwrap();

        gate_respond(root, 15, None, true, Some("lgtm".into())).unwrap();

        let polled = Gates::poll_response(root, 15, Stage::Ship, 1).expect("response readable");
        assert!(polled.approved);
        assert_eq!(polled.note.as_deref(), Some("lgtm"));
        let event = devflow_core::events::last_event_for_phase(root, 15).unwrap();
        assert_eq!(event["event"], "gate_response_written");
        assert_eq!(event["stage"], "ship");
    }

    #[test]
    fn gate_respond_requires_stage_when_ambiguous_and_errors_when_none_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let err = gate_respond(root, 15, None, true, None).unwrap_err();
        assert!(err.to_string().contains("no open gate"), "{err}");

        Gates::write_gate(root, 15, Stage::Validate, "a").unwrap();
        Gates::write_gate(root, 15, Stage::Ship, "b").unwrap();
        let err = gate_respond(root, 15, None, false, Some("nope".into())).unwrap_err();
        assert!(err.to_string().contains("--stage"), "{err}");

        // Explicit --stage disambiguates.
        gate_respond(root, 15, Some(Stage::Validate), false, Some("gaps".into())).unwrap();
        assert!(
            Gates::response_path(root, 15, Stage::Validate).exists(),
            "explicit-stage rejection must land"
        );
        assert!(!Gates::response_path(root, 15, Stage::Ship).exists());
    }

    /// 14-CR-03: a capture file SHORTER than the follower's offset means the
    /// next stage's monitor deleted and recreated it — the follower must
    /// restart from 0, not seek past EOF forever.
    #[test]
    fn rollover_offset_resets_on_shrunken_capture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capture");
        std::fs::write(&path, "abc").unwrap();

        // File (3 bytes) shorter than offset 10 → rollover → 0.
        assert_eq!(rollover_offset(&path, 10), 0);
        // File longer than or equal to the offset → keep the offset.
        assert_eq!(rollover_offset(&path, 3), 3);
        assert_eq!(rollover_offset(&path, 2), 2);
        // Missing file (mid-rollover gap) → keep the offset for now.
        assert_eq!(rollover_offset(&dir.path().join("gone"), 7), 7);
    }

    #[test]
    fn print_capture_from_tracks_offsets_across_appends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capture");
        std::fs::write(&path, "hello ").unwrap();
        let mut output = Vec::new();

        let offset = write_capture_from(&path, 0, &mut output).unwrap();
        assert_eq!(offset, 6);
        assert_eq!(output, b"hello ");

        // Nothing new: offset unchanged.
        output.clear();
        assert_eq!(write_capture_from(&path, offset, &mut output).unwrap(), 6);
        assert!(output.is_empty());

        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"world").unwrap();
        drop(f);
        assert_eq!(write_capture_from(&path, offset, &mut output).unwrap(), 11);
        assert_eq!(output, b"world");

        // Missing file is treated as "no new bytes yet".
        output.clear();
        assert_eq!(
            write_capture_from(Path::new("/nonexistent/x"), 4, &mut output).unwrap(),
            4
        );
        assert!(output.is_empty());
    }

    /// `advance()` over a Ship-stage success with an approved Ship gate must run
    /// the terminal `finish_workflow` path (after-ship hooks + gate cleanup +
    /// state cleared) — the only non-spawning branch of `advance`'s orchestration
    /// (11-VALIDATION.md 12f). The gate response is pre-seeded on disk so
    /// `run_gate`'s poll returns immediately instead of blocking.
    #[test]
    fn advance_ship_success_runs_finish_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 21;
        let branch = format!("feature/phase-{phase:02}");
        let branch_created = std::process::Command::new("git")
            .args(["branch", &branch, "develop"])
            .current_dir(root)
            .status()
            .unwrap()
            .success();
        assert!(branch_created);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        // Seed a DEVFLOW_RESULT success marker so `evaluate_agent_result` resolves
        // at Layer 1 without needing the exit-code/commit-count fallback.
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();

        // Pre-write an approved Ship gate response so `run_gate` returns
        // `GateAction::Advance` immediately instead of polling.
        let response_path = Gates::response_path(root, phase, Stage::Ship);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":true,"note":null,"responded_by":"test"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::response_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
    }

    #[test]
    fn terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["checkout", "-q", "-b", "feature/phase-22"]);
        std::fs::write(root.join("conflict.txt"), "feature\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "feature change"]);
        git(&["checkout", "-q", "develop"]);
        std::fs::write(root.join("conflict.txt"), "develop\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "develop change"]);

        let mut state = State::new(22, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let root_owned = root.to_path_buf();
        let handle = std::thread::spawn(move || {
            let mut state = workflow::load_state(&root_owned, 22).unwrap();
            finish_workflow(&root_owned, &mut state)
        });
        let gate_path = Gates::gate_path(root, 22, Stage::Ship);
        for _ in 0..100 {
            if gate_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            gate_path.exists(),
            "finalization failure must reopen Ship gate"
        );
        assert!(workflow::load_state(root, 22).unwrap().gate_pending);
        Gates::respond(
            root,
            22,
            Stage::Ship,
            &GateResponse {
                approved: false,
                note: Some("abort after merge conflict".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        handle.join().unwrap().unwrap();

        assert_ne!(
            events::last_event_for_phase(root, 22)
                .and_then(|event| event["event"].as_str().map(str::to_owned))
                .as_deref(),
            Some("workflow_finished")
        );
        let tags = std::process::Command::new("git")
            .arg("tag")
            .current_dir(root)
            .output()
            .unwrap();
        assert!(tags.stdout.is_empty());
    }

    /// 13-DEFERRED-CR-03 acceptance: two phases advancing their Ship stages
    /// CONCURRENTLY must each finish their own stage machine — per-phase
    /// state files prevent cross-phase clobbering, and the coarse checkout
    /// lock serializes both `finish_workflow`s' git operations on the shared
    /// primary checkout. Gate responses are pre-seeded so neither advance
    /// blocks polling on its *first* Ship gate.
    ///
    /// 17-09 gap closure (GAP-2): both phases compute their next version from
    /// the same starting git state, and on some runs genuinely race to
    /// create the same version tag — confirmed directly during this plan's
    /// RED phase via temporary debug instrumentation, which caught both
    /// threads inside `version_bump` with the identical computed version
    /// (`2.0.1`) within ~1.8ms of each other, and the loser's `git tag`
    /// failing with git's own "reference already exists". That failure
    /// reopens the loser's Ship gate for human review (`finish_workflow`'s
    /// retry loop) — but only ONE gate response was ever pre-written per
    /// phase (consumed by its first gate open), so the reopened gate has
    /// nothing to consume. Unbounded, `Gates::poll_response` then polls the
    /// 7-day production default (`DEVFLOW_GATE_TIMEOUT_SECS`) with no
    /// response ever arriving — that is the wedge this plan closes.
    ///
    /// The binding constraint is "never hangs," not "always both succeed."
    /// This test does not try to make the race loser also succeed (that
    /// would require re-answering a gate reactively and still not rule out
    /// a second, equally rare collision) — instead it bounds the reopened
    /// gate's poll to a few seconds via `DEVFLOW_GATE_TIMEOUT_SECS`
    /// (overridden ONLY for this test's poll, under the established
    /// `ENV_MUTEX` guard — the 7-day production default is never touched)
    /// and asserts the loser's documented behavior: a bounded timeout error,
    /// state left intact (not cleared), and an actionable Ship gate still on
    /// disk awaiting a human. The common case (no collision) still asserts
    /// both phases finish independently, exactly as before.
    #[test]
    fn concurrent_ship_advances_finish_both_phases_independently() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX. Bounds a reopened Ship gate's
        // poll to a few seconds instead of the 7-day production default.
        // Every OTHER test that reaches `run_gate` pre-writes its response
        // before calling in, so `poll_response` finds it on the very first
        // read regardless of this value — only a *reopened*, unanswered
        // gate (this test's race-loser path) ever actually waits it out.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phases = [31u32, 32u32];
        for &phase in &phases {
            let branch = format!("feature/phase-{phase:02}");
            let branch_created = std::process::Command::new("git")
                .args(["branch", &branch, "develop"])
                .current_dir(root)
                .status()
                .unwrap()
                .success();
            assert!(branch_created);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();
            std::fs::write(
                agent_result::stdout_path(root, phase),
                "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
            )
            .unwrap();
            let response_path = Gates::response_path(root, phase, Stage::Ship);
            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":true,"note":null,"responded_by":"test"}"#,
            )
            .unwrap();
        }

        let results: Vec<(u32, Result<(), CliError>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = phases
                .iter()
                .map(|&phase| (phase, scope.spawn(move || advance(root, Some(phase)))))
                .collect();
            handles
                .into_iter()
                .map(|(phase, handle)| (phase, handle.join().expect("advance thread")))
                .collect()
        });

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_gate_timeout {
                Some(value) => std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", value),
                None => std::env::remove_var("DEVFLOW_GATE_TIMEOUT_SECS"),
            }
        }

        let succeeded = results.iter().filter(|(_, r)| r.is_ok()).count();
        assert!(
            succeeded == 1 || succeeded == 2,
            "at least one phase must finish independently of the other; got {succeeded}/2 successes"
        );

        for (phase, result) in &results {
            match result {
                Ok(()) => {
                    assert!(
                        matches!(
                            workflow::load_state(root, *phase),
                            Err(workflow::WorkflowError::MissingState(_))
                        ),
                        "phase {phase} must be finished (state cleared)"
                    );
                    assert!(!Gates::gate_path(root, *phase, Stage::Ship).exists());
                    let last = devflow_core::events::last_event_for_phase(root, *phase)
                        .expect("events recorded for phase");
                    assert_eq!(
                        last["event"], "workflow_finished",
                        "phase {phase}'s own event stream must end in workflow_finished"
                    );
                }
                Err(err) => {
                    // The documented loser behavior (GAP-2): a version-tag
                    // race lost by VersionBump reopens the Ship gate for a
                    // human; with no second response pre-written, the
                    // bounded poll above times out rather than hanging.
                    assert!(
                        err.to_string().contains("timed out"),
                        "phase {phase}'s only non-success outcome must be a bounded gate \
                         timeout, not some other failure: {err}"
                    );
                    let state = workflow::load_state(root, *phase)
                        .expect("a timed-out gate leaves state intact, not cleared");
                    assert!(
                        state.gate_pending,
                        "phase {phase} must leave an actionable, still-open gate for a human"
                    );
                    assert!(
                        Gates::gate_path(root, *phase, Stage::Ship).exists(),
                        "phase {phase}'s reopened Ship gate file must remain on disk"
                    );
                }
            }
        }
    }

    /// Reaching `MAX_CONSECUTIVE_FAILURES` on a failed Validate must force a
    /// gate (even in Auto mode, which otherwise auto-loops), and an `abort`
    /// gate response must end the workflow (state cleared) without spawning a
    /// new stage (11-VALIDATION.md 12f). The gate response is pre-seeded so the
    /// poll inside `run_gate` returns immediately.
    #[test]
    fn validate_failure_threshold_forces_gate_then_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 22;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        // Pre-write a rejected response whose note says "abort" so
        // `GateAction::from_response` resolves to `Abort` rather than a
        // loop-back-to-Code.
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: requirements changed","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        // CR-01: the forced gate's request file (along with its response and
        // ack) must be cleaned up once it resolves to Abort — previously
        // only the terminal Ship-success path cleaned up gate files, leaving
        // this one on disk to be silently reused by a later gate.
        assert!(
            !Gates::gate_path(root, phase, Stage::Validate).exists(),
            "forced gate's files must be cleaned up once it resolves to Abort"
        );
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
    }

    /// Seed a Validate-stage DEVFLOW_RESULT marker (with the given verdict
    /// JSON fragment, or `None` to omit the key entirely) and drive `advance()`
    /// on a scoped thread, busy-polling for the Validate gate file to appear
    /// so its `context` text — the only externally observable signal of the
    /// `passed` value `advance()` computed from the verdict — can be read
    /// before resolving the gate with an Abort response. Forcing a gate for
    /// every case (rather than letting a `passed=true` case fall through to a
    /// bare `transition`) is deliberate: `transition`/`loop_back_to_code` both
    /// call `launch_stage`, which spawns the real configured agent CLI and
    /// must never fire from a unit test (see `ship_review_failed_loops_to_code`).
    fn drive_validate_advance_and_read_gate_context(
        root: &Path,
        phase: u32,
        consecutive_failures: u32,
        verdict_json: Option<&str>,
    ) -> String {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = consecutive_failures;
        workflow::save_state(&state).unwrap();

        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let marker = match verdict_json {
            Some(verdict) => {
                format!(r#"DEVFLOW_RESULT: {{"status":"success","verdict":"{verdict}"}}"#)
            }
            None => r#"DEVFLOW_RESULT: {"status":"success"}"#.to_string(),
        };
        std::fs::write(agent_result::stdout_path(root, phase), marker).unwrap();

        let gate_path = Gates::gate_path(root, phase, Stage::Validate);
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        let mut context = String::new();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                advance(root, Some(phase)).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if gate_path.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "advance() must force a Validate gate, not advance silently"
            );

            context = std::fs::read_to_string(&gate_path).unwrap();

            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });

        context
    }

    /// 13b verdict-vs-ran: a Validate agent that ran successfully but found
    /// gaps (`verdict: "gaps"`) must NOT advance to Ship — `advance()`'s
    /// Validate arm must compute `passed = false` and route through
    /// `handle_validate_outcome`'s failure path (gate/loop), never Ship.
    #[test]
    fn validate_gaps_does_not_advance_to_ship() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            60,
            mode::MAX_CONSECUTIVE_FAILURES - 1,
            Some("gaps"),
        );
        assert!(
            context.contains("Validation failed"),
            "a gaps verdict must be treated as a failed validation, not a pass: {context}"
        );
    }

    /// 13b verdict-vs-ran (consensus #1): because the Validate prompt now
    /// REQUIRES a verdict, its absence must be treated as a fail-safe
    /// (gate/loop), NOT a silent pass — this is the composition fix that
    /// closes the marker-less/verdict-less Validate → Ship false-advance.
    #[test]
    fn validate_missing_verdict_does_not_advance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            61,
            mode::MAX_CONSECUTIVE_FAILURES - 1,
            None,
        );
        assert!(
            context.contains("Validation failed"),
            "a missing verdict must be treated as a failed validation, not a pass: {context}"
        );
    }

    /// A Validate result with an explicit `verdict: "pass"` must advance to
    /// Ship — `consecutive_failures` is pre-seeded at the gate threshold
    /// itself (rather than `threshold - 1`) because a `passed=true` result
    /// never increments the counter, so the gate must already be at the
    /// threshold to force it open without falling through to a real
    /// `transition`/`launch_stage` spawn.
    #[test]
    fn validate_pass_advances() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            62,
            mode::MAX_CONSECUTIVE_FAILURES,
            Some("pass"),
        );
        assert!(
            context.contains("Validation passed"),
            "an explicit pass verdict must advance to Ship: {context}"
        );
    }

    /// Regression test for CR-01: `abort()` must clean up the gate's
    /// response/ack files for the stage the gate actually fired on. Without
    /// that cleanup, a later gate for the same phase+stage would find the
    /// old, already-consumed response still on disk and `poll_response`
    /// would resolve from it instantly instead of waiting for a fresh human
    /// decision.
    #[test]
    fn abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 23;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        // Pre-write a rejected response whose note says "abort" so
        // `GateAction::from_response` resolves to `Abort`.
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: requirements changed","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        // The gate, response, and ack files for the stage the gate fired on
        // (Validate) must all be gone after the Abort path runs.
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
        assert!(
            !Gates::response_path(root, phase, Stage::Validate).exists(),
            "stale response file must not survive an aborted gate"
        );
        assert!(!Gates::ack_path(root, phase, Stage::Validate).exists());

        // Simulate the phase reaching the same gate again later (e.g. after
        // a restart) — write a fresh request but no new response. If cleanup
        // had not happened, `poll_response` would instantly return the old,
        // already-consumed response instead of blocking for a fresh human
        // decision.
        Gates::write_gate(root, phase, Stage::Validate, "re-fired gate").unwrap();
        let started = std::time::Instant::now();
        let got = Gates::poll_response(root, phase, Stage::Validate, 1);
        assert!(
            got.is_none(),
            "poll_response must not instantly resolve from a stale response after cleanup"
        );
        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }

    /// D-18e's "two independent signals agreeing" arm: a probe pass plus an
    /// explicit `verdict: pass` classify as `ValidateOutcome::Passed` and
    /// drive straight through to Ship — no forced gate (Auto mode,
    /// `consecutive_failures == 0`), no counter touched. PATH is
    /// neutralized under `ENV_MUTEX` (matching
    /// `consecutive_failures_reaches_ceiling_across_cycles`) so
    /// `transition`'s own `launch_stage` call cannot spawn a real agent CLI;
    /// its resulting `Err` (agent binary not found) is discarded, since
    /// `transition` mutates `state.stage` to `Ship` before that call and the
    /// mutation survives regardless of the launch outcome.
    #[test]
    fn external_verify_agreement_advances_to_ship() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 90;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Pass),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert_eq!(outcome, ValidateOutcome::Passed);

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(root, &mut state, outcome);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.stage, Stage::Ship);
        assert_eq!(
            state.consecutive_failures, 0,
            "an agreeing outcome must never touch the failure counter"
        );
    }

    /// D-18e's disagreement arm: the probe passes but the agent reports
    /// `verdict: gaps`. Must classify `Ambiguous` and gate IMMEDIATELY on
    /// the FIRST cycle — never touching `consecutive_failures` — which is
    /// what distinguishes this from `Failed`'s counter-based delayed gate
    /// and is the precise thing the binding operator decision (D-18e,
    /// T-18-19) requires. Resolved via an Abort response so no agent is
    /// ever launched.
    #[test]
    fn external_verify_disagreement_gates_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 91;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Gaps),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "an ambiguous outcome must gate on cycle one without touching the counter"
        );
        assert!(
            !Gates::gate_path(root, phase, Stage::Validate).exists(),
            "the immediate gate must resolve (and clean up) via the same abort path as any other gate"
        );
    }

    /// D-18e's ambiguous arm: the probe passes but NO agent verdict arrived
    /// at all. Same immediate-gate contract as the disagreement case above
    /// — `consecutive_failures` must stay 0.
    #[test]
    fn external_verify_no_verdict_gates_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 92;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: None,
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "an ambiguous outcome must gate on cycle one without touching the counter"
        );
    }

    /// D-01/D-06 regression: a Code-stage `Unknown` outcome (Layer 3's
    /// "process gone but commits exist" case) must route through
    /// `handle_stage_failure`'s never-silent gate, never
    /// `transition(.., Stage::Validate)`. Drives a real `advance()` on a
    /// scoped thread, polling for the Code gate file (not a Validate one) to
    /// prove the dispatch never took the success/Advance arm.
    #[test]
    fn code_unknown_does_not_transition_to_validate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = 72;
        let branch = format!("feature/phase-{phase:02}");
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()
                    .unwrap()
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["checkout", "-q", "-b", &branch, "develop"]);
        std::fs::write(root.join("work.txt"), "wip\n").unwrap();
        git(&["add", "work.txt"]);
        git(&["commit", "-q", "-m", "wip commit"]);
        git(&["checkout", "-q", "develop"]);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let code_gate = Gates::gate_path(root, phase, Stage::Code);
        let validate_gate = Gates::gate_path(root, phase, Stage::Validate);
        let response_path = Gates::response_path(root, phase, Stage::Code);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                advance(root, Some(phase)).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if code_gate.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "an Unknown Code outcome must fire a never-silent gate, not advance silently"
            );
            assert!(
                !validate_gate.exists(),
                "an Unknown Code outcome must never transition to Validate"
            );

            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });
    }

    /// D-08/consensus #4: a `ResourceKilled` outcome on a non-Validate stage
    /// bumps `infra_failures` and leaves `consecutive_failures` untouched —
    /// `handle_infra_outcome` (the `GateInfra` arm) never routes through
    /// `handle_validate_outcome`. A rejected/abort response is pre-seeded so
    /// the never-silent gate resolves immediately without a spawn thread.
    #[test]
    fn resource_killed_on_code_bumps_infra_failures_not_consecutive_failures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 73;
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "137").unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.consecutive_failures = 1;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        // abort() clears state entirely — assert against the terminal error
        // rather than a field, and confirm no Validate gate ever appeared.
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
    }

    /// D-08/consensus #4 (Validate-stage case): a `ResourceKilled` outcome on
    /// the VALIDATE stage still bumps `infra_failures` and leaves
    /// `consecutive_failures` unchanged — proving `GateInfra`
    /// (`handle_infra_outcome`) bypasses `handle_validate_outcome` even on
    /// the one stage that normally owns `consecutive_failures`. The rejected
    /// gate response resolves the never-silent gate to `Abort` immediately
    /// (no spawn thread needed); `consecutive_failures` is asserted on the
    /// in-memory `state`, which `abort()` never mutates (it only clears the
    /// on-disk state file and gate artifacts).
    #[test]
    fn resource_killed_on_validate_bumps_infra_not_consecutive_failures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 74;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = 2;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_infra_outcome(
            root,
            &mut state,
            Stage::Validate,
            Some("agent process was killed (exit code 137, likely OOM)".into()),
        )
        .unwrap();

        assert_eq!(state.infra_failures, 1);
        assert_eq!(
            state.consecutive_failures, 2,
            "consecutive_failures must be untouched by the infra path"
        );
    }

    /// D-08: reaching `MAX_INFRA_FAILURES` infra outcomes aborts rather than
    /// gating again.
    #[test]
    fn infra_ceiling_aborts_instead_of_gating() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 75;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        handle_infra_outcome(root, &mut state, Stage::Code, Some("killed".into())).unwrap();

        assert_eq!(state.infra_failures, mode::MAX_INFRA_FAILURES);
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "at the ceiling, the run must abort rather than gate again"
        );
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
    }

    /// CR-01 regression (17-06 gap closure): `transition()` resets
    /// `infra_failures` to 0 alongside `consecutive_failures` — both in the
    /// in-memory `State` and the persisted `state.json` — and a subsequent
    /// infra fault after a clean transition starts counting from 1, not the
    /// pre-transition count. PATH is neutralized under `ENV_MUTEX` (pointed
    /// at a directory containing ONLY a `git` symlink, so
    /// `agent_binary_available`'s PATH scan has zero possible matches) before
    /// calling `transition()`, because this host genuinely has
    /// `claude`/`codex`/`opencode` on PATH — without neutralizing it,
    /// `transition()`'s downstream `launch_stage` would try to actually spawn
    /// a real agent CLI subprocess, which this test must never do. The
    /// resulting `Err` from `ensure_agent_binary` is expected and ignored:
    /// the counter reset happens earlier in `transition()` and is unaffected
    /// by that downstream failure.
    ///
    /// 19i: PATH must NOT be pointed at an empty directory. `set_var`
    /// mutates the whole process's environment, and Rust's default test
    /// runner executes tests in parallel threads within that one process —
    /// an empty PATH here previously made every OTHER concurrently running,
    /// unguarded git-spawning test fail with `Os { NotFound }` (confirmed
    /// live: both duplicate CI runs for the same commit hit this race).
    /// `agent_free_git_only_path_dir` keeps `git` resolvable for every other
    /// thread while still hiding agent CLIs from this one.
    #[test]
    fn transition_resets_infra_failures() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 80;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = transition(root, &mut state, Stage::Validate);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(
            state.infra_failures, 0,
            "transition() must reset infra_failures in-memory, not just consecutive_failures"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.infra_failures, 0,
            "transition() must persist the infra_failures reset to state.json"
        );

        // A fresh infra fault after the clean transition starts counting
        // from 1, not resuming the pre-transition MAX_INFRA_FAILURES - 1
        // count toward a false premature abort.
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_infra_outcome(root, &mut state, Stage::Validate, Some("killed".into())).unwrap();

        assert_eq!(state.infra_failures, 1);
    }

    /// WR-04 (18-fix): an early failure in `launch_stage_inner` — before
    /// `monitor::spawn_monitor` ever runs — must not leave a stale
    /// `monitor_pid` behind. Pre-fix, `state.monitor_pid` still named the
    /// PREVIOUS stage's (now-dead) monitor after `ensure_agent_binary`
    /// returned early via `?`, and `liveness()`/`doctor` then misreported
    /// `Stuck → devflow resume` — the wrong remedy for what's actually an
    /// agent-binary/staleness failure. PATH is neutralized to a `git`-only
    /// directory under `ENV_MUTEX`, mirroring `transition_resets_infra_failures`,
    /// so `ensure_agent_binary("claude")` reliably fails without touching a
    /// real agent CLI and without racing other PATH-mutating tests.
    #[test]
    fn launch_stage_inner_clears_monitor_pid_on_early_failure() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 93;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        // A stale pid from a prior stage's now-dead monitor — this is what
        // must be cleared, not carried forward into the new stage.
        state.monitor_pid = Some(999_999);
        workflow::save_state(&state).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let result = launch_stage_inner(&mut state, None, None);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(
            result.is_err(),
            "ensure_agent_binary must fail against the neutralized, agent-free PATH"
        );
        assert_eq!(
            state.monitor_pid, None,
            "an early launch failure must clear the stale monitor_pid in-memory, not carry it \
             forward from the previous stage"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.monitor_pid, None,
            "the monitor_pid clear must be persisted to state.json, not just in-memory"
        );
    }

    /// 18d — the RED-then-GREEN core of the Code↔Validate safety-gate
    /// reachability fix. Drives `MAX_CONSECUTIVE_FAILURES` real
    /// fail/Code→Validate cycles via `handle_validate_outcome` (the +1) and
    /// `transition()` (previously an unconditional reset to 0). Before the
    /// fix, `consecutive_failures` oscillates 0/1 and never reaches the
    /// ceiling; after the fix it accumulates and forces the gate.
    ///
    /// `state.stage` is forced back to `Stage::Code` before every
    /// `transition()` call so each loop iteration exercises the exact
    /// `(Code, Validate)` hop under test, independent of which internal
    /// branch `handle_validate_outcome` took on that cycle (ordinary
    /// loop-back vs. the forced gate on the final cycle) — mirrors what
    /// `prepare_loop_back_to_code` does for real on every retry.
    ///
    /// A gate response is re-seeded at the top of every loop iteration (not
    /// just once before the loop) so it survives `prepare_loop_back_to_code`'s
    /// `Gates::cleanup(.., Stage::Validate)` — which fires on every ordinary
    /// loop-back cycle once `state.stage` is `Validate` and would otherwise
    /// delete a response written only once up front before the final,
    /// gate-triggering cycle ever gets to read it. With it re-seeded every
    /// iteration, the forced gate on the final cycle resolves immediately via
    /// `Gates::poll_response` finding an already-written file, instead of
    /// waiting out the (default 7-day) gate timeout. PATH is neutralized
    /// under `ENV_MUTEX` so neither `handle_validate_outcome`'s loop-back nor
    /// `transition()`'s own `launch_stage` call risk spawning a real agent
    /// CLI, following `transition_resets_infra_failures`' established
    /// approach.
    #[test]
    fn consecutive_failures_reaches_ceiling_across_cycles() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 81;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        assert!(
            state
                .mode
                .should_gate(Stage::Validate, state.consecutive_failures),
            "reaching the ceiling must force the Auto-mode Validate gate"
        );
        assert_eq!(
            state.infra_failures, 0,
            "infra_failures must still reset unconditionally on the same hop the consecutive reset now skips"
        );
    }

    /// Combined 18d+18e scenario (18-RESEARCH.md Pitfall 1) — the only test
    /// that proves both fixes hold TOGETHER, not each in isolation: 18e's
    /// Layer-0 discard is what makes an `external_verify` Validate fail for
    /// the wrong reason, and 18d's counter reset is what made that failure
    /// loop unbounded — fixing either alone leaves the other's failure mode
    /// partially masked. Arm A (18e dominates) proves an `Ambiguous` outcome
    /// gates on the FIRST cycle, never touching `consecutive_failures`. Arm
    /// B (18d dominates) proves a genuine, non-ambiguous failure still
    /// reaches `MAX_CONSECUTIVE_FAILURES` and forces the gate — the case
    /// that, before 18d, ran forever.
    #[test]
    fn external_verify_cycles_reach_ceiling_without_unbounded_loop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Arm A: an Ambiguous outcome gates on cycle one, never touching
        // consecutive_failures. Arm B: a genuine failure still reaches
        // MAX_CONSECUTIVE_FAILURES and forces the gate.
        arm_a_ambiguous_outcome_gates_on_cycle_one(root, 93);
        arm_b_genuine_failures_reach_the_ceiling(root, 94);
    }

    /// Arm A (18e dominates): an ambiguous `external_verify` outcome gates
    /// immediately — no Code↔Validate loop ever starts, so 18d's counter is
    /// irrelevant here and must stay untouched. Asserting that prevents a
    /// future refactor from quietly routing ambiguity back through the
    /// counter-based auto-loop.
    fn arm_a_ambiguous_outcome_gates_on_cycle_one(root: &Path, phase: u32) {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Gaps),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "18e's ambiguous gate must fire on cycle one, never touching 18d's counter"
        );
    }

    /// Arm B (18d dominates): a genuine, non-ambiguous `ValidateOutcome::Failed`
    /// driven through repeated Code↔Validate cycles reaches
    /// `MAX_CONSECUTIVE_FAILURES` and forces the gate. PATH is neutralized
    /// under `ENV_MUTEX` (matching `consecutive_failures_reaches_ceiling_across_cycles`)
    /// so neither `handle_validate_outcome`'s loop-back nor `transition`'s
    /// own `launch_stage` risk spawning a real agent CLI.
    fn arm_b_genuine_failures_reach_the_ceiling(root: &Path, phase: u32) {
        let _guard = ENV_MUTEX.lock().unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        assert!(
            state
                .mode
                .should_gate(Stage::Validate, state.consecutive_failures),
            "a genuine repeated failure must still reach the reachable ceiling (18d)"
        );
    }

    /// 18d precision edge: `consecutive_failures` must saturate at `u32::MAX`
    /// rather than wrap to 0 on overflow, so a long-running stuck loop can't
    /// silently restore the unreachable-ceiling bug in a slower, harder-to-
    /// diagnose form. At `u32::MAX`, `should_gate` is already true, so the
    /// failure resolves via the forced-gate path — pre-seed a response so
    /// `run_gate`'s poll doesn't wait out the timeout.
    #[test]
    fn consecutive_failures_increment_saturates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 82;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = u32::MAX;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert_eq!(state.consecutive_failures, u32::MAX);
    }

    /// 18d idempotency edge: a repeated Code→Validate transition leaves
    /// `consecutive_failures` unchanged rather than zeroing it. `state.stage`
    /// is reset to `Code` before each call so both calls exercise the exact
    /// hop under test.
    #[test]
    fn repeated_code_to_validate_transition_is_idempotent_on_the_counter() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 83;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.consecutive_failures = 2;
        workflow::save_state(&state).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = transition(root, &mut state, Stage::Validate);
        state.stage = Stage::Code;
        let _ = transition(root, &mut state, Stage::Validate);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, 2);
    }

    /// 18d concurrency edge: two concurrently-active phases' `consecutive_failures`
    /// counters are independent — a Code→Validate hop on one phase must not
    /// reset a sibling phase's counter.
    #[test]
    fn consecutive_failures_are_independent_across_phases() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut state_a = State::new(84, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state_a.stage = Stage::Code;
        state_a.consecutive_failures = 1;
        workflow::save_state(&state_a).unwrap();

        let mut state_b = State::new(85, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state_b.stage = Stage::Code;
        state_b.consecutive_failures = 2;
        workflow::save_state(&state_b).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = transition(root, &mut state_a, Stage::Validate);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let reloaded_a = workflow::load_state(root, 84).unwrap();
        let reloaded_b = workflow::load_state(root, 85).unwrap();

        assert_eq!(
            reloaded_a.consecutive_failures, 1,
            "the Code->Validate hop must not reset consecutive_failures"
        );
        assert_eq!(
            reloaded_b.consecutive_failures, 2,
            "an untouched sibling phase's counter must be unaffected"
        );
    }

    /// D-09: a primary-loop `RateLimited` outcome writes the single-agent
    /// cron-instructions record (`devflow resume --phase N`) and returns
    /// without firing a blocking gate.
    #[test]
    fn primary_loop_rate_limited_writes_single_agent_cron_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 76;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let instructions = devflow_core::ship::load_cron_instructions(root, phase).unwrap();
        assert_eq!(instructions.resume.command, "devflow");
        assert_eq!(
            instructions.resume.args,
            ["resume", "--phase", &phase.to_string()]
        );
        assert!(
            instructions
                .hermes_cron
                .command
                .contains(&format!("devflow resume --phase {phase}"))
        );

        // No blocking gate — state persists, stage unchanged, not gate-pending.
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.stage, Stage::Code);
        assert!(!reloaded.gate_pending);
        assert_eq!(reloaded.infra_failures, 1);
        assert_eq!(reloaded.consecutive_failures, 0);
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
    }

    /// D-08/D-09: the RateLimited path at `infra_failures ==
    /// MAX_INFRA_FAILURES - 1` bumps to the ceiling and stops auto-resuming —
    /// it routes to the infra gate/abort path instead of writing a resume
    /// record (bounded resume, no soft-loop).
    #[test]
    fn rate_limited_at_infra_ceiling_stops_resuming_and_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 77;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(
            matches!(err, workflow::WorkflowError::MissingState(_)),
            "the infra ceiling must abort, clearing state"
        );
        assert!(
            devflow_core::ship::load_cron_instructions(root, phase).is_err(),
            "must not schedule an auto-resume once the infra ceiling stops resumption"
        );
    }

    /// CR-03: a rate-limit reason whose retry hint is unparseable (e.g. the
    /// `"usage limit"` fallback `detect_claude_rate_limit` produces for a 429
    /// with no retry_after) yields an EMPTY cron schedule — auto-resume is
    /// impossible. That must not return `Ok(())` silently (the detached
    /// monitor would exit with the phase stalled and zero operator signal);
    /// it must fire the same never-silent gate + notify the infra path uses
    /// (WR-11/D-15), and must never invent a schedule.
    #[test]
    fn rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 81;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // Pre-seed an Abort response so `run_gate`'s poll resolves immediately.
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_rate_limited_outcome(
            root,
            &mut state,
            phase,
            Stage::Code,
            Some("rate limited until usage limit".into()),
        )
        .unwrap();

        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            events.contains("gate_fired"),
            "an unparseable retry hint must raise a gate, not stall the phase silently: {events}"
        );
        assert!(
            events.contains("notify_fired"),
            "the operator must be notified that a manual resume is needed: {events}"
        );
        assert!(
            !events.contains("rate_limit_resume_scheduled"),
            "nothing was scheduled — emitting a resume-scheduled event would be a false signal: {events}"
        );

        // The unparseable hint must never become a schedule (an empty cron
        // expression would otherwise degrade into an every-minute resume).
        let instructions = devflow_core::ship::load_cron_instructions(root, phase).unwrap();
        assert!(instructions.hermes_cron.schedule.is_empty());
    }

    /// D-10: `advance_evaluated` emits `status` via `AgentStatus::as_wire_str()`
    /// (never the Debug-lowercase formatter that collapses `ResourceKilled`
    /// into `resourcekilled`) and carries the `decided_by_layer` evidence
    /// field.
    #[test]
    fn advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = 78;
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "137").unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        // advance_evaluated isn't the last event once the infra gate/abort
        // path runs, so read the raw log and find it by name rather than
        // using `last_event_for_phase`.
        let contents = std::fs::read_to_string(events::events_path(root)).unwrap();
        let event = contents
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|e| e["event"] == "advance_evaluated")
            .expect("advance_evaluated event recorded");
        assert_eq!(event["status"], "resource_killed");
        assert_ne!(event["status"], "resourcekilled");
        assert_eq!(event["decided_by_layer"], 2);
    }

    /// `parse_gate_timeout` is a pure function — no env mutation needed, so
    /// this test cannot race any other test.
    #[test]
    fn parse_gate_timeout_env_override() {
        const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
        assert_eq!(parse_gate_timeout(Some("42".into())), 42);
        assert_eq!(parse_gate_timeout(Some("bad".into())), SEVEN_DAYS);
        assert_eq!(parse_gate_timeout(None), SEVEN_DAYS);
    }

    /// 13-06 dogfood regression (Codex leg): a fresh headless Codex run can
    /// never pass Define, so `start --agent codex` pre-flights on the
    /// phase's CONTEXT.md existing on develop.
    #[test]
    fn phase_artifact_on_develop_detects_context_and_fails_open() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("spawn git");
            assert!(out.status.success(), "git {args:?} failed");
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "t@e.st"]);
        run(&["config", "user.name", "t"]);
        run(&["config", "commit.gpgsign", "false"]);
        run(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::create_dir_all(root.join(".planning/phases/03-widget")).unwrap();
        std::fs::write(root.join(".planning/phases/03-widget/03-CONTEXT.md"), "ctx").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);
        run(&["branch", "develop"]);

        assert!(phase_artifact_on_develop(root, 3, "-CONTEXT.md"));
        assert!(!phase_artifact_on_develop(root, 3, "-PLAN.md"));
        assert!(!phase_artifact_on_develop(root, 4, "-CONTEXT.md"));

        // Fail-open: outside a repo (or with no develop branch) the
        // pre-flight must not block.
        let empty = tempfile::tempdir().unwrap();
        assert!(phase_artifact_on_develop(empty.path(), 3, "-CONTEXT.md"));
    }

    // -----------------------------------------------------------------
    // 17d: build provenance + self-dogfood staleness gate (D-17-D-21, Task 2)
    // -----------------------------------------------------------------

    /// D-21: the `workflow_started` payload carries every provenance field,
    /// tested directly without spawning a real agent. No `build_timestamp`
    /// field any more (CR-02, 17-11) — it was removed from `build.rs`
    /// entirely, not just this payload. Also pins the WR-02 redaction: the
    /// `exe_path` field must never carry a directory component (the
    /// operator's home directory / OS username), since `OPERATIONS.md`
    /// documents `events.jsonl` as a file that's safe to tail and paste.
    #[test]
    fn workflow_started_payload_carries_build_provenance() {
        let state = State::new(66, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
        let payload = workflow_started_payload(&state);
        assert_eq!(payload["agent"], "claude");
        assert_eq!(payload["mode"], "auto");
        assert!(payload["version"].as_str().is_some());
        assert!(payload["commit"].is_string());
        assert!(payload["dirty"].is_string());
        assert!(
            payload.get("build_timestamp").is_none(),
            "build_timestamp was removed (CR-02) and must not reappear"
        );
        assert!(
            payload.get("exe_path").is_some(),
            "WR-02: exe_path key must still exist — a future refactor must not \
             satisfy the redaction assertion by deleting the field"
        );
        assert!(payload["exe_path"].is_string() || payload["exe_path"].is_null());
        if let Some(exe_path) = payload["exe_path"].as_str() {
            assert!(
                !exe_path.contains('/') && !exe_path.contains('\\'),
                "WR-02: exe_path must be a bare filename with no directory \
                 separator — OPERATIONS.md documents events.jsonl as safe to \
                 tail and paste, so a full absolute path here leaks the \
                 operator's home directory and OS username; got {exe_path:?}"
            );
        }
    }

    /// The Validate→Ship content hook (`DocsUpdate`) authors material about
    /// the branch being shipped, so it must run in that phase's worktree;
    /// the terminal batch merges/tags/deletes against the primary checkout
    /// and must NOT be retargeted. `ChangelogAppend` moved into the terminal
    /// batch in 17-12 (WR-04) for exactly this reason — it now targets
    /// `project_root`, not the worktree.
    ///
    /// Found live: `ChangelogAppend` wrote Phase 17's release note into
    /// `develop`'s CHANGELOG.md while all of its commits sat on
    /// `feature/phase-17`, stranding the entry on the wrong branch.
    #[test]
    fn content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let worktree = root.join(".worktrees/phase-70");
        std::fs::create_dir_all(&worktree).unwrap();

        let mut state = State::new(70, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.worktree_path = Some(worktree.clone());

        assert_eq!(
            hook_context_root(root, &state, false),
            worktree,
            "content hooks must write into the phase's worktree"
        );
        assert_eq!(
            hook_context_root(root, &state, true),
            root.to_path_buf(),
            "terminal hooks merge/tag/delete against the primary checkout"
        );

        // --no-worktree runs, and a worktree recorded but already removed,
        // both fall back to the project root rather than writing nowhere.
        let mut no_worktree = state.clone();
        no_worktree.worktree_path = None;
        assert_eq!(hook_context_root(root, &no_worktree, false), root);

        let mut missing = state.clone();
        missing.worktree_path = Some(root.join(".worktrees/gone"));
        assert_eq!(hook_context_root(root, &missing, false), root);
    }

    /// 13-06 dogfood regression: a multi-KB parser-derived reason reached
    /// the operator's desktop notification verbatim. Gate contexts must cap
    /// the reason to a readable headline.
    #[test]
    fn truncate_reason_caps_long_reasons_and_keeps_short_ones() {
        assert_eq!(truncate_reason("short reason"), "short reason");
        let long = "x".repeat(5000);
        let capped = truncate_reason(&long);
        assert!(capped.chars().count() <= 300);
        assert!(capped.ends_with("[truncated; full output in .devflow/]"));
    }

    #[test]
    fn gate_context_rendering_neutralizes_all_controls_and_obeys_limit() {
        let rendered = render_gate_context("line 1\n\u{1b}[2J\tline 2\u{7}", 100);
        assert!(!rendered.chars().any(char::is_control));
        assert_eq!(rendered, "line 1  [2J line 2 ");

        let bounded = render_gate_context(&"x".repeat(500), 100);
        assert_eq!(bounded.chars().count(), 100);
        assert!(bounded.ends_with("[truncated; full output in .devflow/]"));
    }

    #[test]
    fn status_shows_pending_gate_prominently() {
        let dir = tempfile::tempdir().unwrap();
        let context = format!("first line\n\u{1b}[2J{}", "sensitive detail ".repeat(80));
        Gates::write_gate(dir.path(), 16, Stage::Ship, &context).unwrap();
        let open = Gates::list_open(dir.path());

        let banner = render_pending_gate_banner(&open, u64::MAX).unwrap();

        assert!(banner.contains("PENDING GATE"));
        assert!(banner.contains("phase 16"));
        assert!(banner.contains("ship"));
        assert!(banner.contains("devflow gate approve 16 --stage ship"));
        assert!(banner.contains("devflow gate reject 16 --stage ship"));
        assert!(banner.contains("[truncated; full output in .devflow/]"));
        assert!(!banner.contains(&context));
        assert!(!banner.contains('\u{1b}'));
        assert!(banner.contains("ESCALATED"));
    }

    /// A Ship-stage AgentFailed result (no `review:` prefix) must write a
    /// gate file and block for a response — not silently return an `Err`
    /// with nothing surfaced (WR-11; the pre-Task-2 catch-all never wrote a
    /// gate at all for this case). Runs `handle_ship_failure` on a scoped
    /// thread and busy-polls for the gate file to appear while the call is
    /// still blocked in `run_gate`'s poll, then unblocks it with an Abort
    /// response so the thread can finish without spawning a real monitor
    /// (Abort resolves via `abort()`, which never calls `launch_stage`).
    #[test]
    fn ship_agent_failed_fires_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 40;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let gate_path = Gates::gate_path(root, phase, Stage::Ship);
        let response_path = Gates::response_path(root, phase, Stage::Ship);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                handle_ship_failure(root, &mut state, Some("agent crashed".into())).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if gate_path.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "handle_ship_failure must write a gate file, not silently return an Err"
            );

            // Unblock the poll with an Abort response so the spawned thread
            // finishes (abort() cleans up on its own; no monitor spawned).
            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });
    }

    /// A Ship-stage result whose reason starts with `review:` must loop back
    /// to Code instead of firing a gate — it does not go through `run_gate`
    /// at all, so no gate file is ever written for this path.
    ///
    /// Exercises `is_ship_review_failure` (the exact dispatch predicate
    /// `handle_ship_failure` uses) plus `prepare_loop_back_to_code` (the
    /// state-mutating half of `loop_back_to_code`) directly, rather than the
    /// full `handle_ship_failure` → `loop_back_to_code` → `launch_stage`
    /// chain: `launch_stage` spawns the real configured agent CLI (e.g. real
    /// `claude -p ... --dangerously-skip-permissions` if it's on `$PATH`),
    /// which must never fire from a unit test.
    #[test]
    fn ship_review_failed_loops_to_code() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 41;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let reason = Some("review: please fix naming".to_string());
        assert!(is_ship_review_failure(&reason));

        prepare_loop_back_to_code(root, &mut state, FixType::AuditFix).unwrap();

        assert_eq!(state.stage, Stage::Code);
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        // Not finished — finish_workflow would have cleared state entirely.
        assert!(workflow::load_state(root, phase).is_ok());
    }

    /// The ReviewFailed loop-back must select `FixType::AuditFix`
    /// (`/gsd-audit-fix`), not the Validate path's `FixType::GapsOnly`
    /// (consensus #7 / OpenCode HIGH #2).
    #[test]
    fn ship_review_failed_uses_audit_fix() {
        assert!(is_ship_review_failure(&Some(
            "review: needs changes".into()
        )));
        assert!(is_ship_review_failure(&Some("  Review: nitpick".into())));
        assert!(!is_ship_review_failure(&Some("agent crashed".into())));
        assert!(!is_ship_review_failure(&None));

        let prompt = prompt::fix_prompt(FixType::AuditFix, 11);
        assert!(prompt.contains("/gsd-audit-fix"));
        assert!(!prompt.contains("--gaps-only"));
    }

    /// A Code-stage failure must fire a gate AND run the configured notify
    /// hook — with `DEVFLOW_NON_SILENT_GATE=1` since Auto mode would not
    /// normally gate a Code failure (unexpected/never-silent gate). The
    /// notify sentinel is a side effect distinct from the gate file itself,
    /// so it survives even though `Gates::cleanup` removes the gate/
    /// response/ack once the gate resolves. This test sets
    /// `DEVFLOW_GATE_NOTIFY_CMD`, so it's serialized under `ENV_MUTEX`.
    #[test]
    fn non_validate_failure_fires_gate_and_hook() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let sentinel = root.join("notify-sentinel");

        // SAFETY: serialized under ENV_MUTEX — no other thread in this
        // process sets/removes DEVFLOW_GATE_NOTIFY_CMD concurrently. Note
        // this only prevents races between env-*mutating* tests: any other
        // concurrently-running test that calls `run_gate` (most of them do)
        // will also read whatever we set here and may itself fire our
        // sentinel command with its own `unexpected` value. So we assert
        // only that the hook fired at all (sentinel created), not its exact
        // content — the exact DEVFLOW_NON_SILENT_GATE propagation is already
        // covered contamination-free by gates.rs's
        // `notify_hook_sets_non_silent_flag` (calls the pure
        // `run_notify_command` directly, no global env involved).
        unsafe {
            std::env::set_var(
                "DEVFLOW_GATE_NOTIFY_CMD",
                format!("touch {}", sentinel.display()),
            );
        }

        let phase = 42;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // A Code-stage failure in Auto mode is exactly the "unexpected" case
        // `run_gate` computes (`!should_gate(..)`) and passes to
        // `fire_gate_notify` — asserted here as a pure, race-free check.
        assert!(
            !state
                .mode
                .should_gate(Stage::Code, state.consecutive_failures)
        );

        // Pre-write an Abort response so the call resolves without spawning
        // a monitor (the notify hook already fired by the time `run_gate`
        // starts polling, so this doesn't affect what we're asserting).
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        let result =
            handle_stage_failure(root, &mut state, Stage::Code, Some("build failed".into()));

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            std::env::remove_var("DEVFLOW_GATE_NOTIFY_CMD");
        }

        result.unwrap();
        assert!(
            sentinel.exists(),
            "handle_stage_failure must fire the configured notify hook, not silently skip it"
        );
    }

    /// CR-01 regression: after a stage failure's gate resolves via Advance
    /// and the retry (also a stage failure) fires a fresh gate, the SECOND
    /// gate's poll must not instantly resolve from the FIRST gate's
    /// already-consumed response/ack — `handle_stage_failure` must clean
    /// those up before the retry launches.
    #[test]
    fn stage_failure_retry_cleans_stale_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = 43;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // Pre-write an Abort response so the first failure resolves
        // immediately without spawning a monitor.
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_stage_failure(root, &mut state, Stage::Code, Some("first failure".into())).unwrap();

        // abort() must have cleaned up the gate/response/ack for Code.
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
        assert!(!Gates::response_path(root, phase, Stage::Code).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Code).exists());

        // Simulate the phase reaching the same gate again later (e.g. a
        // fresh retry after abort would normally clear state, but re-fire
        // here directly to prove the CR-01 stale-response reuse regression
        // is closed): write a fresh gate but no new response.
        Gates::write_gate(root, phase, Stage::Code, "re-fired gate").unwrap();
        let started = std::time::Instant::now();
        let got = Gates::poll_response(root, phase, Stage::Code, 1);
        assert!(
            got.is_none(),
            "poll_response must not instantly resolve from a stale response after cleanup"
        );
        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }

    /// Unit tests for the pure `doctor` reconciliation core (18a). Each test
    /// builds a `PhaseFacts` directly — no repository, no I/O — proving
    /// `reconcile_phase` is a predicate over facts alone.
    #[cfg(test)]
    mod doctor_reconciliation {
        use super::*;

        /// A fully-agreeing baseline: `reconcile_phase` over this returns
        /// zero findings. Each test overrides only the field(s) needed to
        /// trigger the one check it's proving.
        fn agreeing_facts(phase: u32) -> PhaseFacts {
            PhaseFacts {
                phase,
                stage: Stage::Code,
                gate_pending: false,
                agent_pid: Some(4242),
                agent_alive: true,
                monitor_pid: Some(4343),
                monitor_alive: true,
                last_event: Some("stage_launched".into()),
                last_launched_stage: Some(Stage::Code),
                open_gate_stages: Vec::new(),
                feature_branch_exists: true,
            }
        }

        #[test]
        fn reconcile_phase_returns_no_findings_when_all_agree() {
            let facts = agreeing_facts(1);
            assert!(reconcile_phase(&facts).is_empty());
        }

        #[test]
        fn reconcile_phase_flags_gate_pending_without_open_gate() {
            let facts = PhaseFacts {
                gate_pending: true,
                open_gate_stages: Vec::new(),
                ..agreeing_facts(2)
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("gate_pending is true"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some("devflow resume --phase 2")
            );
        }

        #[test]
        fn reconcile_phase_flags_orphan_open_gate() {
            let facts = PhaseFacts {
                gate_pending: false,
                open_gate_stages: vec![Stage::Validate],
                ..agreeing_facts(3)
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("gate open for stage validate"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some("devflow gate approve 3 --stage validate")
            );
        }

        #[test]
        fn reconcile_phase_flags_dead_agent_at_agent_stage() {
            let facts = PhaseFacts {
                agent_pid: Some(999_999),
                agent_alive: false,
                ..agreeing_facts(4)
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Problem);
            assert!(findings[0].detail.contains("agent pid 999999"));
            assert_eq!(
                findings[0].repair.as_deref(),
                Some("devflow resume --phase 4")
            );
        }

        #[test]
        fn reconcile_phase_flags_stage_event_drift() {
            let facts = PhaseFacts {
                stage: Stage::Validate,
                last_launched_stage: Some(Stage::Code),
                ..agreeing_facts(5)
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Warn);
            assert!(
                findings[0]
                    .detail
                    .contains("last stage_launched event named code")
            );
            assert!(findings[0].repair.is_none());
        }

        #[test]
        fn reconcile_phase_flags_missing_feature_branch() {
            let facts = PhaseFacts {
                stage: Stage::Plan,
                last_launched_stage: Some(Stage::Plan),
                feature_branch_exists: false,
                ..agreeing_facts(6)
            };
            let findings = reconcile_phase(&facts);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].severity, Severity::Warn);
            assert!(findings[0].detail.contains("feature/phase-06"));
            assert!(findings[0].repair.is_none());
        }

        /// 18b: a dead monitor with a dead agent is `Stuck` — nothing will
        /// call `devflow advance` for this phase — and reports a `Problem`
        /// finding with a `devflow resume --phase N` repair.
        #[test]
        fn reconcile_reports_stuck_when_monitor_and_agent_are_both_dead() {
            let facts = PhaseFacts {
                monitor_pid: Some(5150),
                monitor_alive: false,
                agent_pid: Some(4242),
                agent_alive: false,
                ..agreeing_facts(8)
            };
            let findings = reconcile_phase(&facts);
            let monitor_finding = findings
                .iter()
                .find(|f| f.detail.contains("monitor pid"))
                .expect("expected a monitor finding when monitor and agent are both dead");
            assert_eq!(monitor_finding.severity, Severity::Problem);
            assert!(monitor_finding.detail.contains("monitor pid 5150"));
            assert_eq!(
                monitor_finding.repair.as_deref(),
                Some("devflow resume --phase 8")
            );
        }

        /// 18b (T-18-11): an unrecorded monitor is unknown, not a problem —
        /// a state file written by a pre-18b binary must never render as
        /// stuck.
        #[test]
        fn reconcile_is_silent_when_monitor_pid_is_unrecorded() {
            let facts = PhaseFacts {
                monitor_pid: None,
                monitor_alive: false,
                ..agreeing_facts(9)
            };
            assert!(
                reconcile_phase(&facts).is_empty(),
                "an unrecorded monitor must never produce a finding"
            );
        }

        /// 18b: a live monitor with a dead agent is a normal between-stages
        /// moment (the monitor hasn't advanced the phase yet), not a monitor
        /// finding. `check_dead_agent`'s own pre-existing finding for the
        /// dead agent pid is unrelated to this check and out of this plan's
        /// scope.
        #[test]
        fn reconcile_is_silent_when_monitor_alive_and_agent_dead() {
            let facts = PhaseFacts {
                monitor_pid: Some(5150),
                monitor_alive: true,
                agent_alive: false,
                ..agreeing_facts(10)
            };
            let findings = reconcile_phase(&facts);
            assert!(
                findings.iter().all(|f| !f.detail.contains("monitor pid")),
                "a live monitor with a dead agent must not produce a monitor finding"
            );
        }

        /// Several checks trigger simultaneously; the returned findings must
        /// come back in the fixed order `reconcile_phase` evaluates checks
        /// in, not in whatever order the facts happen to be populated.
        #[test]
        fn reconcile_phase_ordering_is_input_order_independent() {
            let facts = PhaseFacts {
                gate_pending: true,
                agent_pid: Some(999_999),
                agent_alive: false,
                monitor_pid: Some(999_998),
                monitor_alive: false,
                last_launched_stage: Some(Stage::Validate),
                open_gate_stages: Vec::new(),
                feature_branch_exists: false,
                ..agreeing_facts(7)
            };
            let findings = reconcile_phase(&facts);
            let severities: Vec<Severity> = findings.iter().map(|f| f.severity).collect();
            assert_eq!(
                severities,
                vec![
                    Severity::Problem, // check_gate_pending_without_gate
                    Severity::Problem, // check_dead_agent
                    Severity::Problem, // check_dead_monitor
                    Severity::Warn,    // check_stage_event_drift
                    Severity::Warn,    // check_missing_branch
                ]
            );
            assert!(findings[0].detail.contains("gate_pending is true"));
            assert!(findings[1].detail.contains("agent pid 999999"));
            assert!(findings[2].detail.contains("monitor pid 999998"));
            assert!(
                findings[3]
                    .detail
                    .contains("last stage_launched event named validate")
            );
            assert!(findings[4].detail.contains("feature/phase-07"));
        }

        /// `doctor`'s idle-project path (Task 2, 18a): the exact code path
        /// `doctor(root, false)` runs for its reconciliation section is
        /// `collect_phase_facts` + `render_reconciliation_text` — asserted
        /// directly here rather than capturing process stdout, since this
        /// codebase has no stdout-capture dependency and this phase adds no
        /// new ones (18-RESEARCH.md).
        #[test]
        fn doctor_reports_no_active_phases_when_idle() {
            let dir = tempfile::tempdir().unwrap();
            let facts = collect_phase_facts(dir.path());
            assert!(facts.is_empty());
            assert!(render_reconciliation_text(&facts).contains("no active phases"));
        }

        #[test]
        fn doctor_reports_gate_pending_without_gate_file() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = 90;
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            state.gate_pending = true;
            workflow::save_state(&state).unwrap();

            let facts = collect_phase_facts(root);
            assert_eq!(facts.len(), 1);
            let text = render_reconciliation_text(&facts);
            assert!(text.contains(&format!("phase {phase}: gate_pending is true")));
            assert!(text.contains(&format!("repair: devflow resume --phase {phase}")));
        }

        /// WR-01 (18-fix): `doctor --json` must emit ONE JSON document, not
        /// two concatenated top-level arrays. Exercises the exact
        /// composition `doctor()`'s `--json` path uses (`doctor_json_body`),
        /// then round-trips it through `serde_json::to_string`/`from_str` —
        /// the failure mode this reproduces (pre-fix) is a single-document
        /// parser (`json.load`, `JSON.parse`) raising "Extra data" on the
        /// old two-array output; `jq` tolerated it (NDJSON-style streaming),
        /// which is why it went unnoticed.
        #[test]
        fn doctor_json_is_a_single_object_with_environment_and_reconciliation() {
            let checks = vec![Check {
                name: "git".into(),
                status: "ok".into(),
                version: Some("2.40.0".into()),
                install_hint: None,
            }];

            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = 92;
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            state.gate_pending = true; // mismatched: no gate file — produces a finding
            workflow::save_state(&state).unwrap();
            let facts = collect_phase_facts(root);

            let body = doctor_json_body(&checks, &facts);
            let serialized = serde_json::to_string(&body).unwrap();
            let reparsed: serde_json::Value = serde_json::from_str(&serialized)
                .expect("doctor --json must be single-document JSON, not two concatenated arrays");

            assert!(
                reparsed.get("environment").is_some(),
                "must carry the tool checks under \"environment\": {reparsed}"
            );
            assert!(
                reparsed.get("reconciliation").is_some(),
                "must carry the reconciliation findings under \"reconciliation\": {reparsed}"
            );
            assert!(reparsed["environment"].is_array());
            assert!(reparsed["reconciliation"].is_array());
            let reconciliation = reparsed["reconciliation"].as_array().unwrap();
            assert!(
                !reconciliation.is_empty(),
                "the mismatched gate_pending fixture must produce at least one finding"
            );
            assert!(
                reconciliation.iter().any(|f| f["detail"]
                    .as_str()
                    .unwrap_or("")
                    .contains("gate_pending is true")),
                "must carry the gate_pending finding: {reconciliation:?}"
            );
        }

        /// T-18-02: running `doctor` twice against a mismatched fixture must
        /// leave `.devflow/` byte-identical — no state rewrite, no event
        /// append, no gate file appears or disappears.
        #[test]
        fn doctor_is_read_only_on_a_mismatched_project() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = 91;
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            state.gate_pending = true; // mismatched: no gate file will exist
            workflow::save_state(&state).unwrap();
            events::emit(
                root,
                phase,
                "stage_launched",
                serde_json::json!({"stage": "code"}),
            );

            let state_path = workflow::state_path(root, phase);
            let before_len = std::fs::metadata(&state_path).unwrap().len();
            let before_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
            let events_log = events::events_path(root);
            let before_lines = std::fs::read_to_string(&events_log)
                .unwrap()
                .lines()
                .count();

            doctor(root, false).unwrap();
            doctor(root, false).unwrap();

            let after_len = std::fs::metadata(&state_path).unwrap().len();
            let after_modified = std::fs::metadata(&state_path).unwrap().modified().unwrap();
            let after_lines = std::fs::read_to_string(&events_log)
                .unwrap()
                .lines()
                .count();

            assert_eq!(
                before_len, after_len,
                "doctor must not rewrite the state file"
            );
            assert_eq!(
                before_modified, after_modified,
                "doctor must not touch the state file's mtime"
            );
            assert_eq!(
                before_lines, after_lines,
                "doctor must not append to events.jsonl"
            );
        }
    }
}
