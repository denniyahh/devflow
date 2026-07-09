use clap::{Parser, Subcommand};
use devflow_core::agent;
use devflow_core::config::{DEVELOP, FEATURE_PREFIX, GitFlowConfig};
use devflow_core::gates::{GateAction, Gates};
use devflow_core::git::GitFlow;
use devflow_core::hooks::{self, HookContext};
use devflow_core::mode::{self, Mode};
use devflow_core::prompt::{self, FixType};
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{agent_result, agents, lock, monitor, recover, worktree};
use devflow_core::{agent_result::AgentStatus, workflow};
use std::path::{Path, PathBuf};

/// How long a background gate poll waits for a human response (7 days).
const GATE_TIMEOUT_SECS: u64 = 7 * 24 * 60 * 60;

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
        /// Run the agent in an isolated git worktree at `.worktrees/phase-NN/`.
        #[arg(long)]
        worktree: bool,
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
            worktree,
            dry_run,
            project,
        } => start(
            &project_root(project)?,
            phase,
            agent,
            mode,
            force,
            worktree,
            dry_run,
        ),
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

    // Pre-start divergence check: runs on current HEAD before any git mutation.
    if let Ok((_ahead, behind)) = GitFlow::new(project_root).divergence_from_develop() {
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

    workflow::save_state(&state)?;
    launch_stage(&state, None)?;
    println!(
        "started phase {} in {mode} mode at {} — monitor will auto-advance",
        state.phase, state.started_at
    );
    Ok(())
}

/// Spawn the background monitor that owns the agent for `state.stage`. The
/// monitor calls `devflow advance` when the agent exits. An optional
/// `prompt_override` is used for Code loop-backs (fix prompts).
fn launch_stage(state: &State, prompt_override: Option<String>) -> Result<(), CliError> {
    let prompt = prompt_override.unwrap_or_else(|| prompt::stage_prompt(state.stage, state.phase));
    let adapter = agents::adapter_for(state.agent);
    let (program, args) = adapter.exec_command(state.phase, &prompt);

    agent_result::cleanup_phase_files(&state.project_root, state.phase);
    let pid = monitor::spawn_monitor(state, program, &args)
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
    let _lock = match lock::acquire(project_root) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };

    let mut state = workflow::load_state(project_root)?;
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
            // Other stages have no auto-loop — halt and leave state for recovery.
            _ => Err(CliError::Message(format!(
                "stage {stage} failed: {}",
                result
                    .reason
                    .unwrap_or_else(|| "no details available".into())
            ))),
        };
    }

    // Success (or Unknown — advance with the warning already printed above).
    match stage {
        Stage::Define => transition(project_root, &mut state, Stage::Plan),
        Stage::Plan => transition(project_root, &mut state, Stage::Code),
        Stage::Code => transition(project_root, &mut state, Stage::Validate),
        Stage::Validate => handle_validate_outcome(project_root, &mut state, true),
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
            GateAction::LoopBack(_) => loop_back_to_code(project_root, state),
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }

    if passed {
        transition(project_root, state, Stage::Ship)
    } else {
        loop_back_to_code(project_root, state)
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
        GateAction::LoopBack(_) => loop_back_to_code(project_root, state),
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
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

/// Loop the pipeline back to Code with a gaps-only fix prompt.
fn loop_back_to_code(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    state.stage = Stage::Code;
    state.gate_pending = false;
    workflow::save_state(state)?;
    println!(
        "looping back to Code (validate failures: {})",
        state.consecutive_failures
    );
    let _ = project_root;
    launch_stage(
        state,
        Some(prompt::fix_prompt(FixType::GapsOnly, state.phase)),
    )
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
    match Gates::poll_response(project_root, state.phase, stage, GATE_TIMEOUT_SECS) {
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
    let (child, pid) = agent::launch_agent(&*adapter, phase, &prompt, workdir)?;
    println!(
        "launched {} (pid {pid}) in {}",
        adapter.name(),
        workdir.display()
    );
    let capture = agent::capture_agent_output(child, phase, project_root)
        .map_err(|err| CliError::Message(format!("failed to capture agent output: {err}")))?;
    println!("agent {agent} exited with code {}", capture.exit_code);
    Ok(
        agent_result::parse_devflow_result(&capture.stdout).or_else(|| {
            agent_result::detect_rate_limit(&capture.stdout).map(|retry| {
                agent_result::AgentResult {
                    status: AgentStatus::RateLimited,
                    exit_code: Some(capture.exit_code),
                    reason: Some(format!("rate limited until {retry}")),
                    commits: None,
                    summary: None,
                }
            })
        }),
    )
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
                let version = String::from_utf8_lossy(&out.stderr)
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
        assert!(
            Gates::gate_path(root, phase, Stage::Validate).exists(),
            "forced gate request must be written at the failure threshold"
        );
        let err = workflow::load_state(root).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
    }
}
