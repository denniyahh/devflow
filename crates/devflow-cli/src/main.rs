use clap::{Parser, Subcommand};
use devflow_core::agent;
use devflow_core::config::{DEVELOP, FEATURE_PREFIX, GitFlowConfig};
use devflow_core::gates::{self, GateAction, Gates};
use devflow_core::git::GitFlow;
use devflow_core::hooks::{self, HookContext};
use devflow_core::mode::{self, Mode};
use devflow_core::prompt::{self, FixType};
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{agent_result, agents, lock, monitor, recover, worktree};
use devflow_core::{
    agent_result::{AgentStatus, Verdict},
    workflow,
};
use std::path::{Path, PathBuf};
use tracing::info;

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
        /// Clean up the stale state instead of just inspecting.
        #[arg(long)]
        clean: bool,
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

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Workflow(#[from] devflow_core::workflow::WorkflowError),
    #[error(transparent)]
    Recover(#[from] devflow_core::recover::RecoverError),
    #[error(transparent)]
    Git(#[from] devflow_core::git::GitError),
    #[error(transparent)]
    Agent(#[from] devflow_core::agent::AgentError),
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
            tracing_subscriber::fmt().json().init();
        }
        _ => {
            tracing_subscriber::fmt::init();
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
        Command::Advance { project } => advance(&project_root(project)?),
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
        Command::Recover { project, clean } => recover_cmd(&project_root(project)?, clean),
        Command::Test { project } => test_cmd(&project_root(project)?),
        Command::Doctor { json, project } => doctor(&project_root(project)?, json),
    }
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
fn phase_artifact_on_develop(project_root: &Path, phase: u32, suffix: &str) -> bool {
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
    if let Err(err) = launch_stage(&state, None) {
        if let Err(clear_err) = workflow::clear_state(project_root) {
            eprintln!("warning: could not clear state after failed launch: {clear_err}");
        }
        return Err(err);
    }
    println!(
        "started phase {} in {mode} mode at {} — monitor will auto-advance",
        state.phase, state.started_at
    );
    Ok(())
}

/// The sandbox writable roots a worktree-hosted agent needs to commit: the
/// main repo's common `.git/` (objects, refs) and the linked worktree's
/// admin dir (`index.lock`, `HEAD`) — resolved from the worktree's `.git`
/// gitdir pointer when readable, with the creation-convention path as
/// fallback (13-06 dogfood finding).
fn worktree_writable_roots(project_root: &Path, worktree: &Path) -> Vec<PathBuf> {
    let git_dir = project_root.join(".git");
    let admin = std::fs::read_to_string(worktree.join(".git"))
        .ok()
        .and_then(|s| {
            s.trim()
                .strip_prefix("gitdir:")
                .map(|p| PathBuf::from(p.trim()))
        })
        .unwrap_or_else(|| {
            git_dir
                .join("worktrees")
                .join(worktree.file_name().unwrap_or_default())
        });
    vec![git_dir, admin]
}

/// Spawn the background monitor that owns the agent for `state.stage`. The
/// monitor calls `devflow advance` when the agent exits. An optional
/// `prompt_override` is used for Code loop-backs (fix prompts).
fn launch_stage(state: &State, prompt_override: Option<String>) -> Result<(), CliError> {
    let prompt = prompt_override.unwrap_or_else(|| prompt::stage_prompt(state.stage, state.phase));
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

    agent_result::cleanup_phase_files(&state.project_root, state.phase);
    let pid = monitor::spawn_monitor(state, program, &args, &adapter.extra_env())
        .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    println!(
        "stage {} → launched {} (monitor pid {pid})",
        state.stage,
        adapter.name()
    );
    Ok(())
}

/// Advance the stage machine after a monitored agent for `state.stage` exits.
/// Invoked by the monitor process; not normally run by a human.
fn advance(project_root: &Path) -> Result<(), CliError> {
    // CR-03 (13-REVIEW.md): the lock is scoped per-phase, not per-project.
    // advance() holds it across a gate's multi-day blocking wait, and every
    // successful run ends at a mandatory Ship gate — a project-wide lock
    // would starve `devflow parallel`'s sibling phases with no retry. Load
    // state before acquiring so the lock can be keyed on this phase; this is
    // safe because it's a plain read and same-phase races still contend on
    // the same phase-scoped lock file below.
    let mut state = workflow::load_state(project_root)?;
    let _lock = match lock::acquire(project_root, state.phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };

    let git_flow = GitFlowConfig::default();
    let result = agent_result::evaluate_agent_result(project_root, &state, &git_flow)
        .map_err(|err| CliError::Message(format!("could not evaluate agent result: {err}")))?;
    let stage = state.stage;
    println!("stage {stage} finished with status {:?}", result.status);
    if let Some(reason) = &result.reason {
        println!("  detail: {reason}");
    }

    let failed = matches!(
        result.status,
        AgentStatus::Failed | AgentStatus::RateLimited
    );
    if failed {
        return match stage {
            // Validate failures drive the Code↔Validate loop (or a gate).
            Stage::Validate => handle_validate_outcome(project_root, &mut state, false),
            // Ship distinguishes an agent crash (AgentFailed) from a review
            // rejection (ReviewFailed, `review:`-prefixed reason).
            Stage::Ship => handle_ship_failure(project_root, &mut state, result.reason),
            // Every other non-Validate failure is never silent (WR-11): it
            // always fires a gate + notify instead of returning a bare error.
            _ => handle_stage_failure(project_root, &mut state, stage, result.reason),
        };
    }

    // Success (or Unknown — advance with the warning already printed above).
    match stage {
        Stage::Define => transition(project_root, &mut state, Stage::Plan),
        Stage::Plan => transition(project_root, &mut state, Stage::Code),
        Stage::Code => transition(project_root, &mut state, Stage::Validate),
        Stage::Validate => {
            // 13b verdict-vs-ran: the Validate prompt now REQUIRES a verdict,
            // so ONLY an explicit `verdict: pass` advances to Ship. A missing
            // verdict is a fail-safe (gate/loop), NOT a silent pass — closes
            // the composition bug where a marker-less/verdict-less Validate
            // could otherwise reach Ship.
            let passed = matches!(result.verdict, Some(Verdict::Pass));
            handle_validate_outcome(project_root, &mut state, passed)
        }
        Stage::Ship => handle_ship_outcome(project_root, &mut state),
    }
}

/// Decide what happens after a Validate stage (passed or failed), honoring the
/// active mode's gate policy and the consecutive-failure threshold.
fn handle_validate_outcome(
    project_root: &Path,
    state: &mut State,
    passed: bool,
) -> Result<(), CliError> {
    if !passed {
        state.consecutive_failures += 1;
        workflow::save_state(state)?;
    }

    if state
        .mode
        .should_gate(Stage::Validate, state.consecutive_failures)
    {
        let context = if passed {
            "Validation passed — approve to ship?".to_string()
        } else {
            format!(
                "Validation failed {} time(s) — human review needed.",
                state.consecutive_failures
            )
        };
        return match run_gate(project_root, state, Stage::Validate, &context)? {
            GateAction::Advance => transition(project_root, state, Stage::Ship),
            GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }

    if passed {
        transition(project_root, state, Stage::Ship)
    } else {
        loop_back_to_code(project_root, state, FixType::GapsOnly)
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
    const MAX: usize = 300;
    if reason.chars().count() <= MAX {
        return reason.to_string();
    }
    let head: String = reason.chars().take(MAX).collect();
    format!("{head}… [truncated; full output in .devflow/]")
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
            launch_stage(state, None)
        }
        GateAction::LoopBack(_) => {
            // Retry the SAME failed stage — Code is not a valid recovery
            // target before planning exists for a Define/Plan failure
            // (Codex 13-01 MEDIUM). Only Ship's ReviewFailed path (handled
            // separately in `handle_ship_failure`) actually loops to Code.
            let _ = Gates::cleanup(project_root, state.phase, stage);
            launch_stage(state, None)
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

/// Fire the hooks for `from → to`, persist the new stage, and launch its agent.
fn transition(project_root: &Path, state: &mut State, to: Stage) -> Result<(), CliError> {
    let from = state.stage;
    let git_flow = GitFlowConfig::default();
    for hook in hooks::hooks_for_transition(from, to) {
        let ctx = HookContext {
            phase: state.phase,
            project_root: project_root.to_path_buf(),
            stage: to,
            git_flow: git_flow.clone(),
        };
        if let Err(err) = hook.run(&ctx) {
            println!("warning: hook {hook:?} failed: {err}");
        }
    }
    state.stage = to;
    state.consecutive_failures = 0;
    state.gate_pending = false;
    workflow::save_state(state)?;
    launch_stage(state, None)
}

/// Loop the pipeline back to Code with the given fix prompt (`GapsOnly` for a
/// Validate rejection, `AuditFix` for a Ship `review:` rejection).
fn loop_back_to_code(project_root: &Path, state: &mut State, fix: FixType) -> Result<(), CliError> {
    let prompt = prepare_loop_back_to_code(project_root, state, fix)?;
    launch_stage(state, Some(prompt))
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
    println!(
        "looping back to Code (validate failures: {})",
        state.consecutive_failures
    );
    Ok(prompt::fix_prompt(fix, state.phase))
}

/// Run the terminal hooks (version bump + branch cleanup) and clear state.
fn finish_workflow(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    let git_flow = GitFlowConfig::default();
    for hook in hooks::hooks_after_ship() {
        let ctx = HookContext {
            phase: state.phase,
            project_root: project_root.to_path_buf(),
            stage: Stage::Ship,
            git_flow: git_flow.clone(),
        };
        if let Err(err) = hook.run(&ctx) {
            println!("warning: hook {hook:?} failed: {err}");
        }
    }
    let _ = Gates::cleanup(project_root, state.phase, Stage::Validate);
    let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
    workflow::clear_state(project_root)?;
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
    gates::fire_gate_notify(state.phase, stage, context, unexpected);
    match Gates::poll_response(project_root, state.phase, stage, gate_timeout_secs()) {
        Some(response) => {
            state.gate_pending = false;
            workflow::save_state(state)?;
            Gates::ack(project_root, state.phase, stage)?;
            Ok(GateAction::from_response(&response))
        }
        None => Err(CliError::Message(format!(
            "gate for stage {stage} timed out awaiting a response"
        ))),
    }
}

/// Abort the workflow with a reason, clearing state.
fn abort(project_root: &Path, state: &State, reason: &str) -> Result<(), CliError> {
    println!("workflow aborted for phase {}: {reason}", state.phase);
    // See CR-01: without this, a stale response/ack for this phase+stage
    // survives on disk and is silently reused if the gate fires again later.
    let _ = Gates::cleanup(project_root, state.phase, state.stage);
    let _ = workflow::clear_state(project_root);
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

/// Launch one agent, block until it exits, and return its self-reported result
/// (parsed from the DEVFLOW_RESULT marker, if present). Used by sequentagent,
/// where the rebase handoff between agents requires a synchronous run.
fn run_agent_blocking(
    project_root: &Path,
    phase: u32,
    agent: AgentKind,
    workdir: &Path,
) -> Result<Option<agent_result::AgentResult>, CliError> {
    agent_result::cleanup_phase_files(project_root, phase);
    let adapter = agents::adapter_for(agent);
    let prompt = prompt::stage_prompt(Stage::Code, phase);
    // sequentagent always runs in a worktree — the main repo's `.git/` and
    // the worktree admin dir must stay writable for sandboxed agents to
    // commit (13-06 dogfood finding).
    let roots = if workdir == project_root {
        Vec::new()
    } else {
        worktree_writable_roots(project_root, workdir)
    };
    let (child, pid) = agent::launch_agent(&*adapter, phase, &prompt, workdir, &roots)?;
    println!(
        "launched {} (pid {pid}) in {}",
        adapter.name(),
        workdir.display()
    );
    let capture = agent::capture_agent_output(child, phase, project_root)
        .map_err(|err| CliError::Message(format!("failed to capture agent output: {err}")))?;
    println!("agent {agent} exited with code {}", capture.exit_code);
    // capture_agent_output already wrote stdout to the same file
    // evaluate_layer1 reads, so delegate to it directly rather than
    // re-implementing a subset of its precedence here — this is the single
    // code path that knows how to find a Codex agent's DEVFLOW_RESULT
    // marker inside its JSONL `--json` event stream (parse_codex_event_result),
    // which the previous parse_devflow_result-only chain never called.
    Ok(agent_result::evaluate_layer1(project_root, phase))
}

/// Integrate an agent branch into the shared base, pushing if a remote exists.
fn integrate_agent_branch(git: &GitFlow, base: &str, agent_branch: &str) -> Result<(), CliError> {
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
    let _ = devflow_core::ship::delete_cron_instructions(project_root);
    let (agent_a, agent_b) = split_two_agents(agents)?;
    let git = GitFlow::new(project_root);
    let base = format!("{FEATURE_PREFIX}phase-{phase:02}");

    // 1. Ensure the shared base branch exists off develop, without leaving the
    //    main checkout on it.
    git.ensure_branch(&base, DEVELOP)?;

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
                        "Agent A rate-limited with zero commits; paused and wrote .devflow/cron-instructions.json"
                    );
                    return Ok(());
                }
                println!("Agent A rate-limited; handing off to agent B");
            }
            _ => {}
        }
    }
    integrate_agent_branch(&git, &base, &branch_a)?;

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
    integrate_agent_branch(&git, &base, &branch_b)?;
    let _ = devflow_core::ship::delete_cron_instructions(project_root);

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
        println!("wrote .devflow/cron-instructions.json");
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

fn status(project_root: &Path) -> Result<(), CliError> {
    let mut current_worktree: Option<PathBuf> = None;
    match workflow::load_state(project_root) {
        Ok(state) => {
            let gate = if state.gate_pending {
                "pending"
            } else {
                "none"
            };
            println!(
                "stage: {} | mode: {} | gate: {}",
                state.stage, state.mode, gate
            );
            println!("phase: {}", state.phase);
            println!("agent: {}", agents::adapter_for(state.agent).name());
            if state.consecutive_failures > 0 {
                println!("validate failures: {}", state.consecutive_failures);
            }
            println!("started_at: {}", state.started_at);
            println!("project_root: {}", state.project_root.display());
            if let Some(ref wt) = state.worktree_path {
                println!("worktree: {}", wt.display());
            }
            current_worktree = state.worktree_path.clone();
            match agent_pid_from_file(project_root, state.phase) {
                Some(pid) => {
                    println!("agent_pid: {pid}");
                    println!("agent_running: {}", agent::agent_running(pid));
                }
                None => println!("agent_pid: none"),
            }
        }
        Err(devflow_core::workflow::WorkflowError::MissingState(_)) => {
            println!("stage: idle");
            println!("project_root: {}", project_root.display());
        }
        Err(err) => return Err(err.into()),
    }
    print_open_branches(project_root);
    print_worktrees(project_root, current_worktree.as_deref());
    if let Some(hint) = cron_instruction_hint(project_root) {
        println!("\n{hint}");
    }
    Ok(())
}

/// Read the launched agent PID the monitor recorded for `phase`, if present.
fn agent_pid_from_file(project_root: &Path, phase: u32) -> Option<u32> {
    let path = agent_result::agent_pid_path(project_root, phase);
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn cron_instruction_hint(project_root: &Path) -> Option<String> {
    devflow_core::ship::cron_instructions_path(project_root)
        .exists()
        .then(|| {
            format!(
                "Cron instruction pending: hermes cron create --from-devflow {}",
                project_root.display()
            )
        })
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
    if project.exists() {
        project
            .canonicalize()
            .map_err(|err| CliError::Message(format!("failed to resolve project path: {err}")))
    } else {
        Err(CliError::Message(format!(
            "project path does not exist: {}",
            project.display()
        )))
    }
}

fn recover_cmd(project_root: &Path, do_clean: bool) -> Result<(), CliError> {
    if do_clean {
        recover::clean(project_root)?;
        println!("cleaned up abandoned workflow state");
        return Ok(());
    }

    let status = match recover::inspect(project_root) {
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

    println!("phase: {}", status.state.phase);
    println!("stage: {}", status.state.stage);
    println!("mode: {}", status.state.mode);
    println!("agent: {}", agents::adapter_for(status.state.agent).name());
    println!("started: {}", status.state.started_at);
    println!("age: {}", status.age);
    match agent_pid_from_file(project_root, status.state.phase) {
        Some(pid) => {
            let running = agent::agent_running(pid);
            println!("agent_pid: {pid}");
            println!("agent_running: {running}");
            if !running {
                println!("\nagent is not running — the monitor may have already advanced");
            }
        }
        None => println!("agent_pid: none"),
    }

    if status.is_stale {
        println!("\nstate is stale — run `devflow recover --clean` to clear it");
    }

    Ok(())
}

/// Run the local quality gate: cargo test, clippy, and fmt --check.
fn test_cmd(project_root: &Path) -> Result<(), CliError> {
    let checks = [
        ("cargo test", "cargo test"),
        ("cargo clippy", "cargo clippy -- -D warnings"),
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

/// Audit the environment and report what's installed, missing, or broken.
fn doctor(_project_root: &Path, json: bool) -> Result<(), CliError> {
    use std::process::Command;

    struct Check {
        name: String,
        status: String,
        version: Option<String>,
        install_hint: Option<String>,
    }

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

    if json {
        let mut out = String::from("[\n");
        for (i, c) in checks.iter().enumerate() {
            out.push_str("  {\n");
            out.push_str(&format!("    \"name\": {:?},\n", c.name));
            out.push_str(&format!("    \"status\": {:?},\n", c.status));
            if let Some(v) = &c.version {
                out.push_str(&format!("    \"version\": {:?},\n", v));
            } else {
                out.push_str("    \"version\": null,\n");
            }
            if let Some(h) = &c.install_hint {
                out.push_str(&format!("    \"install_hint\": {:?}\n", h));
            } else {
                out.push_str("    \"install_hint\": null\n");
            }
            out.push('}');
            if i + 1 < checks.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("]\n");
        print!("{out}");
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
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-global env vars (`set_var`/
    /// `remove_var` are process-wide and `cargo test` runs in parallel by
    /// default) so they don't race each other.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

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
    fn cron_instruction_hint_includes_hermes_command() {
        let dir = tempfile::tempdir().unwrap();
        let instructions = devflow_core::ship::build_cron_instructions(
            dir.path(),
            7,
            "2026-06-18T15:45:30Z",
            "claude,codex",
        );
        devflow_core::ship::write_cron_instructions(dir.path(), &instructions).unwrap();

        let hint = cron_instruction_hint(dir.path()).unwrap();

        assert_eq!(
            hint,
            format!(
                "Cron instruction pending: hermes cron create --from-devflow {}",
                dir.path().display()
            )
        );
    }

    /// Build a real git repo (main + develop, with a Cargo.toml committed) so
    /// the terminal-path hooks (`VersionBump`, `BranchCleanup`) exercised below
    /// have real git plumbing to operate on rather than an empty directory.
    fn init_repo(root: &Path) {
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?} failed");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "devflow@example.com"]);
        git(&["config", "user.name", "DevFlow Tests"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "tag.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("Cargo.toml"), "[package]\nversion = \"2.0.0\"\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        git(&["branch", "-M", "main"]);
        git(&["checkout", "-q", "-b", "develop"]);
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

        advance(root).unwrap();

        let err = workflow::load_state(root).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::response_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
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

        handle_validate_outcome(root, &mut state, false).unwrap();

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        // CR-01: the forced gate's request file (along with its response and
        // ack) must be cleaned up once it resolves to Abort — previously
        // only the terminal Ship-success path cleaned up gate files, leaving
        // this one on disk to be silently reused by a later gate.
        assert!(
            !Gates::gate_path(root, phase, Stage::Validate).exists(),
            "forced gate's files must be cleaned up once it resolves to Abort"
        );
        let err = workflow::load_state(root).unwrap_err();
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
                advance(root).unwrap();
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

        handle_validate_outcome(root, &mut state, false).unwrap();

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

    /// 13-06 dogfood regression: a multi-KB parser-derived reason reached
    /// the operator's desktop notification verbatim. Gate contexts must cap
    /// the reason to a readable headline.
    #[test]
    fn truncate_reason_caps_long_reasons_and_keeps_short_ones() {
        assert_eq!(truncate_reason("short reason"), "short reason");
        let long = "x".repeat(5000);
        let capped = truncate_reason(&long);
        assert!(capped.chars().count() < 400);
        assert!(capped.ends_with("[truncated; full output in .devflow/]"));
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
        assert!(workflow::load_state(root).is_ok());
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
}
