use clap::{Parser, Subcommand};
use devflow_core::agent;
use devflow_core::config::Config;
use devflow_core::git::GitFlow;
use devflow_core::state::{Agent, State, Step};
use devflow_core::verify;
use devflow_core::{lock, monitor, recover, version, workflow, worktree};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "devflow",
    version,
    about = "Agent-agnostic development workflow automation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Begin workflow for a phase.
    Start {
        /// Phase number to work on.
        #[arg(long)]
        phase: u32,
        /// Agent to launch.
        #[arg(long, default_value = "claude")]
        agent: Agent,
        /// Spawn a background monitor that auto-advances when the agent exits.
        #[arg(long)]
        monitor: bool,
        /// Overwrite the feature branch if it already exists.
        #[arg(long)]
        force: bool,
        /// Run the agent in an isolated git worktree at `.worktrees/phase-NN/`.
        #[arg(long)]
        worktree: bool,
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
    /// Poll state and advance if the agent is done.
    Check {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
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
    /// Create a release branch and bump the configured version.
    Ship {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Bootstrap `.devflow.yaml` and `.devflow/`.
    Init {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Overwrite an existing `.devflow.yaml`.
        #[arg(long)]
        force: bool,
    },
    /// Show effective config.
    Config {
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
    /// Run the configured verification command (e.g., cargo test).
    Verify {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Run the configured lint command (e.g., cargo clippy).
    Lint {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Run the configured docs command and optionally auto-commit.
    Docs {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Config(#[from] devflow_core::config::ConfigError),
    #[error(transparent)]
    Workflow(#[from] devflow_core::workflow::WorkflowError),
    #[error(transparent)]
    Recover(#[from] devflow_core::recover::RecoverError),
    #[error(transparent)]
    Git(#[from] devflow_core::git::GitError),
    #[error(transparent)]
    Agent(#[from] devflow_core::agent::AgentError),
    #[error(transparent)]
    Version(#[from] devflow_core::version::VersionError),
    #[error(transparent)]
    Verify(#[from] devflow_core::verify::VerifyError),
    #[error(transparent)]
    Worktree(#[from] devflow_core::worktree::WorktreeError),
    #[error("{0}")]
    Message(String),
}

fn main() {
    tracing_subscriber::fmt::init();
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
            monitor: use_monitor,
            force,
            worktree,
            project,
        } => start(
            &project_root(project)?,
            phase,
            agent,
            use_monitor,
            force,
            worktree,
        ),
        Command::Parallel {
            phases,
            agents,
            force,
            project,
        } => parallel(&project_root(project)?, &phases, agents.as_deref(), force),
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
        Command::Check { project } => check(&project_root(project)?),
        Command::Status { project } => status(&project_root(project)?),
        Command::List { project } => list(&project_root(project)?),
        Command::Ship { project } => ship(&project_root(project)?),
        Command::Init { project, force } => init(&project_root(project)?, force),
        Command::Config { project } => show_config(&project_root(project)?),
        Command::Recover { project, clean } => recover_cmd(&project_root(project)?, clean),
        Command::Verify { project } => verify_cmd(&project_root(project)?),
        Command::Lint { project } => lint_cmd(&project_root(project)?),
        Command::Docs { project } => docs_cmd(&project_root(project)?),
    }
}

fn start(
    project_root: &Path,
    phase: u32,
    agent: Agent,
    use_monitor: bool,
    force: bool,
    worktree: bool,
) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    let mut state = State::new(phase, agent, project_root.to_path_buf());

    if worktree {
        // Worktree mode: create an isolated checkout instead of mutating the
        // main working copy. The agent's cwd becomes the worktree path.
        let wt = ensure_phase_worktree(project_root, &config, phase, force)?;
        println!(
            "created worktree: {} (branch {}phase-{:02})",
            wt.display(),
            config.git_flow.feature_prefix,
            phase
        );
        state.worktree_path = Some(wt);
    } else if config.automation.auto_branch {
        let git = GitFlow::new(project_root, config.git_flow.clone());
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

    // Clean up old stdout/exit files from a prior run of the same phase.
    devflow_core::agent_result::cleanup_phase_files(project_root, phase);

    // Pre-start divergence check: warn if develop has advanced significantly.
    let git = GitFlow::new(project_root, config.git_flow.clone());
    match git.divergence_from_develop() {
        Ok((_ahead, behind)) => {
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
        Err(_) => {
            // If divergence check fails (e.g., no git repo), just continue.
        }
    }

    state.step = Step::Executing;
    let adapter = devflow_core::agents::adapter_for(state.agent);

    if use_monitor {
        // Monitor mode: the monitor daemon *owns* the agent so stdout/exit
        // capture survives this CLI process exiting. The CLI does not launch
        // or wait on the agent itself.
        let (program, args) = adapter.exec_command(state.phase);
        match monitor::spawn_monitor(&state, program, &args) {
            Ok(mon_pid) => {
                state.monitor_pid = Some(mon_pid);
                // The monitor records the agent PID; poll briefly so status
                // and `devflow check` can report it.
                if let Some(agent_pid) = monitor::wait_for_agent_pid(project_root, phase) {
                    state.agent_pid = Some(agent_pid);
                    state.agent_label = Some(agent::agent_label(agent, agent_pid));
                    println!("launched {} (pid {agent_pid})", adapter.name());
                }
                println!("monitor spawned (pid {mon_pid}) — will auto-advance when agent exits");
            }
            Err(err) => {
                return Err(CliError::Message(format!("could not spawn monitor: {err}")));
            }
        }
    } else {
        // Blocking mode: the CLI launches the agent and captures output directly.
        // The agent runs in its worktree when set; capture stays in project_root.
        let workdir = state
            .worktree_path
            .clone()
            .unwrap_or_else(|| project_root.to_path_buf());
        let (child, pid) = agent::launch_agent(&*adapter, state.phase, &workdir)?;
        state.agent_pid = Some(pid);
        state.agent_label = Some(agent::agent_label(agent, pid));
        println!("launched {} (pid {pid})", adapter.name());
        println!("waiting for agent to complete (no monitor — blocking)...");
        match agent::capture_agent_output(child, phase, project_root) {
            Ok(capture) => {
                println!("agent (pid {pid}) exited with code {}", capture.exit_code);
                // Parse DEVLOW_RESULT and store in state.
                let result = devflow_core::agent_result::parse_devlow_result(&capture.stdout);
                state.agent_result = result;
                state.agent_stdout_path =
                    Some(devflow_core::agent_result::stdout_path(project_root, phase));
                if !capture.stdout.is_empty() {
                    let preview: String = capture
                        .stdout
                        .chars()
                        .rev()
                        .take(2000)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    println!("\n--- agent output (last 2000 chars) ---\n{preview}");
                    if capture.stdout.len() > 2000 {
                        println!("... ({} total chars truncated)", capture.stdout.len());
                    }
                }
                println!("agent exited — run `devflow check` to advance");
            }
            Err(err) => {
                println!("error capturing agent output (pid {pid}): {err}");
            }
        }
    }

    workflow::save_state(&state)?;
    println!("started phase {} at {}", state.phase, state.started_at);
    Ok(())
}

/// Create the phase worktree at `.worktrees/phase-NN/` on `feature/phase-NN`.
///
/// With `force`, an existing worktree directory and its branch are removed
/// first so the worktree can be recreated cleanly from `develop`.
fn ensure_phase_worktree(
    project_root: &Path,
    config: &Config,
    phase: u32,
    force: bool,
) -> Result<PathBuf, CliError> {
    let wt = worktree::phase_path(project_root, phase);
    let branch = format!("{}phase-{:02}", config.git_flow.feature_prefix, phase);

    if force {
        if wt.exists() {
            worktree::remove(project_root, &wt, true)?;
        }
        // Best-effort: drop the branch so it can be recreated from develop.
        let _ = GitFlow::new(project_root, config.git_flow.clone()).delete_branch(&branch, true);
    }

    match worktree::add(project_root, &wt, &branch, &config.git_flow.develop, true) {
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

/// Parse `--phases` and optional `--agents` into positional (phase, agent)
/// pairs. Agents default to `claude` when fewer are given than phases; an error
/// is returned when more agents than phases are supplied.
fn parse_phase_agent_pairs(
    phases: &str,
    agents: Option<&str>,
) -> Result<Vec<(u32, Agent)>, CliError> {
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

    let agents: Vec<Agent> = match agents {
        Some(list) => list
            .split(',')
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.parse::<Agent>()
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
        .map(|(i, phase)| (phase, agents.get(i).copied().unwrap_or(Agent::Claude)))
        .collect())
}

/// Spawn one monitored worktree run per phase, concurrently.
fn parallel(
    project_root: &Path,
    phases: &str,
    agents: Option<&str>,
    force: bool,
) -> Result<(), CliError> {
    let pairs = parse_phase_agent_pairs(phases, agents)?;
    println!("launching {} phase(s) in parallel worktrees", pairs.len());
    for (phase, agent) in pairs {
        println!("\n=== phase {phase} ({agent}) ===");
        // Monitor mode keeps each run non-blocking so the phases run together.
        start(project_root, phase, agent, true, force, true)?;
    }
    Ok(())
}

/// Parse exactly two comma-separated agents for `sequentagent`.
fn split_two_agents(agents: &str) -> Result<(Agent, Agent), CliError> {
    let parsed: Vec<Agent> = agents
        .split(',')
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .map(|a| {
            a.parse::<Agent>()
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
/// (parsed from the DEVLOW_RESULT marker, if present).
fn run_agent_blocking(
    project_root: &Path,
    phase: u32,
    agent: Agent,
    workdir: &Path,
) -> Result<Option<devflow_core::agent_result::AgentResult>, CliError> {
    devflow_core::agent_result::cleanup_phase_files(project_root, phase);
    let adapter = devflow_core::agents::adapter_for(agent);
    let (child, pid) = agent::launch_agent(&*adapter, phase, workdir)?;
    println!(
        "launched {} (pid {pid}) in {}",
        adapter.name(),
        workdir.display()
    );
    let capture = agent::capture_agent_output(child, phase, project_root)
        .map_err(|err| CliError::Message(format!("failed to capture agent output: {err}")))?;
    println!("agent {agent} exited with code {}", capture.exit_code);
    Ok(devflow_core::agent_result::parse_devlow_result(
        &capture.stdout,
    ))
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
    let config = Config::load(project_root)?;
    let (agent_a, agent_b) = split_two_agents(agents)?;
    let git = GitFlow::new(project_root, config.git_flow.clone());
    let base = format!("{}phase-{:02}", config.git_flow.feature_prefix, phase);

    // 1. Ensure the shared base branch exists off develop, without leaving the
    //    main checkout on it.
    git.ensure_branch(&base, &config.git_flow.develop)?;

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
    if let Some(result) = run_agent_blocking(project_root, phase, agent_a, &wt_a)?
        && result.status == devflow_core::agent_result::AgentStatus::Failed
    {
        return Err(CliError::Message(format!(
            "agent A ({agent_a}) failed: {}",
            result.reason.unwrap_or_else(|| "no details".into())
        )));
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
        && result.status == devflow_core::agent_result::AgentStatus::Failed
    {
        return Err(CliError::Message(format!(
            "agent B ({agent_b}) failed: {}",
            result.reason.unwrap_or_else(|| "no details".into())
        )));
    }
    integrate_agent_branch(&git, &base, &branch_b)?;

    println!("\nsequentagent complete — both agents integrated into {base}");
    Ok(())
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

/// Create or refresh the static reference worktree (CONTEXT Q3: manual only).
fn reference(project_root: &Path, branch: Option<String>, refresh: bool) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    let branch = branch.unwrap_or_else(|| config.git_flow.develop.clone());
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
    let config = Config::load(project_root)?;
    let git = GitFlow::new(project_root, config.git_flow.clone());
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
            Some(branch) if branch.starts_with(&config.git_flow.feature_prefix) => {
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

fn check(project_root: &Path) -> Result<(), CliError> {
    let _lock = match lock::acquire(project_root) {
        Ok(guard) => Some(guard),
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running — \
                 if this is stale, run `devflow recover --clean`"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    let config = Config::load(project_root)?;
    let state = workflow::load_state(project_root)?;

    if state.step == Step::Executing {
        if let Some(pid) = state.agent_pid {
            if agent::agent_running(pid) {
                println!("agent still running (pid {pid})");
                return Ok(());
            }
            println!("agent process ended (pid {pid})");
        } else {
            println!("no agent PID recorded — advancing state");
        }

        // Three-layer agent result evaluation.
        match devflow_core::agent_result::evaluate_agent_result(
            project_root,
            &state,
            &config.git_flow,
        ) {
            Ok(result) => {
                match result.status {
                    devflow_core::agent_result::AgentStatus::Success => {
                        println!("agent reported success");
                        if let Some(ref reason) = result.reason {
                            println!("  detail: {reason}");
                        }
                    }
                    devflow_core::agent_result::AgentStatus::Failed => {
                        return Err(CliError::Message(format!(
                            "phase {} failed: {}",
                            state.phase,
                            result
                                .reason
                                .unwrap_or_else(|| "no details available".into())
                        )));
                    }
                    devflow_core::agent_result::AgentStatus::Unknown => {
                        // Layer 3 fallback — advance with warning.
                        if let Some(ref reason) = result.reason {
                            println!("warning: could not verify agent completion — {reason}");
                        } else {
                            println!("warning: could not verify agent completion status");
                        }
                    }
                }
            }
            Err(err) => {
                println!("warning: could not evaluate agent result: {err}");
            }
        }
    }

    let mut result = workflow::advance_state(state, &config)?;
    println!("{}", result.message);

    // Auto-run verify/lint/docs as state advances through automated steps.
    loop {
        match result.state.step {
            Step::Verifying if config.automation.auto_verify => {
                println!("--- running verify & lint ---");
                match run_verify(&config, project_root) {
                    Ok(()) => {
                        println!("verify & lint passed");
                    }
                    Err(err) => {
                        if config.automation.continue_on_error {
                            println!("verify/lint failed but continue_on_error is set: {err}");
                        } else {
                            return Err(err);
                        }
                    }
                }
                result = workflow::advance_state(result.state, &config)?;
                println!("{}", result.message);
            }
            Step::Docsing if config.automation.auto_docs => {
                println!("--- running docs ---");
                match run_docs(&config, project_root) {
                    Ok(()) => {
                        println!("docs passed");
                    }
                    Err(err) => {
                        if config.automation.continue_on_error {
                            println!("docs failed but continue_on_error is set: {err}");
                        } else {
                            return Err(err);
                        }
                    }
                }
                result = workflow::advance_state(result.state, &config)?;
                println!("{}", result.message);
            }
            _ => break,
        }
    }

    println!("current step: {}", result.state.step);
    Ok(())
}

/// Run the configured verify + lint commands. Fails on first non-zero exit.
fn run_verify(config: &Config, project_root: &Path) -> Result<(), CliError> {
    println!("verify: {}", config.automation.verify_command);
    let verify_result = verify::run_or_fail(&config.automation.verify_command, project_root)?;
    if !verify_result.stdout.is_empty() {
        println!("{}", verify_result.stdout.trim());
    }

    println!("lint: {}", config.automation.lint_command);
    let lint_result = verify::run_or_fail(&config.automation.lint_command, project_root)?;
    if !lint_result.stdout.is_empty() {
        println!("{}", lint_result.stdout.trim());
    }
    if !lint_result.stderr.is_empty() {
        eprintln!("{}", lint_result.stderr.trim());
    }
    Ok(())
}

/// Run the configured docs command. Optionally auto-commits.
fn run_docs(config: &Config, project_root: &Path) -> Result<(), CliError> {
    println!("docs: {}", config.automation.docs_command);
    let docs_result = verify::run_or_fail(&config.automation.docs_command, project_root)?;
    if !docs_result.stdout.is_empty() {
        println!("{}", docs_result.stdout.trim());
    }
    if !docs_result.stderr.is_empty() {
        eprintln!("{}", docs_result.stderr.trim());
    }

    if config.automation.docs_auto_commit {
        match GitFlow::new(project_root, devflow_core::config::GitFlowConfig::default())
            .commit_all("docs: auto-update from devflow")
        {
            Ok(()) => println!("auto-committed doc changes"),
            Err(err) => println!("warning: could not auto-commit docs: {err}"),
        }
    }
    Ok(())
}

fn status(project_root: &Path) -> Result<(), CliError> {
    let mut current_worktree: Option<PathBuf> = None;
    match workflow::load_state(project_root) {
        Ok(state) => {
            println!("step: {}", state.step);
            println!("phase: {}", state.phase);
            println!(
                "agent: {}",
                devflow_core::agents::adapter_for(state.agent).name()
            );
            println!("started_at: {}", state.started_at);
            println!("project_root: {}", state.project_root.display());
            if let Some(ref wt) = state.worktree_path {
                println!("worktree: {}", wt.display());
            }
            current_worktree = state.worktree_path.clone();
            match state.agent_pid {
                Some(pid) => {
                    let running = agent::agent_running(pid);
                    println!("agent_pid: {pid}");
                    println!("agent_running: {running}");
                }
                None => println!("agent_pid: none"),
            }
        }
        Err(devflow_core::workflow::WorkflowError::MissingState(_)) => {
            println!("step: idle");
            println!("project_root: {}", project_root.display());
        }
        Err(err) => return Err(err.into()),
    }
    // Show open feature branches
    print_open_branches(project_root);
    // Show active git worktrees
    print_worktrees(project_root, current_worktree.as_deref());
    Ok(())
}

/// Print active phase worktrees with branch and inferred phase/agent.
///
/// Tolerates git errors (no repo) the same way `print_open_branches` does.
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
    let config = Config::load(project_root)?;
    let git = GitFlow::new(project_root, config.git_flow.clone());
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
    let config = match Config::load(project_root) {
        Ok(c) => c,
        Err(_) => return,
    };
    let git = GitFlow::new(project_root, config.git_flow.clone());
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

fn ship(project_root: &Path) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    let current = match version::read_version(project_root, &config.version) {
        Ok(version) => version,
        Err(err) => {
            return Err(CliError::Message(format!(
                "could not read configured version: {err}"
            )));
        }
    };
    let next = version::bump(&current, &config.automation.auto_version)?;
    let written = version::write_version(project_root, &config.version, &next)?;
    let git = GitFlow::new(project_root, config.git_flow.clone());
    match git.release_start(&next) {
        Ok(branch) => println!("created release branch: {branch}"),
        Err(err) => println!("warning: could not create release branch: {err}"),
    }
    println!("bumped version: {current} -> {next}");
    println!("updated: {}", written.display());
    Ok(())
}

fn init(project_root: &Path, force: bool) -> Result<(), CliError> {
    std::fs::create_dir_all(project_root.join(".devflow"))
        .map_err(|err| CliError::Message(format!("failed to create .devflow: {err}")))?;
    let config_path = project_root.join(".devflow.yaml");
    if config_path.exists() && !force {
        println!("config already exists: {}", config_path.display());
        return Ok(());
    }
    std::fs::write(&config_path, default_config_yaml())
        .map_err(|err| CliError::Message(format!("failed to write config: {err}")))?;
    println!("initialized DevFlow config: {}", config_path.display());
    Ok(())
}

fn show_config(project_root: &Path) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    print!("{}", config.to_yaml());
    Ok(())
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
    println!("step: {}", status.state.step);
    println!(
        "agent: {}",
        devflow_core::agents::adapter_for(status.state.agent).name()
    );
    println!("started: {}", status.state.started_at);
    println!("age: {}", status.age);
    match status.state.agent_pid {
        Some(pid) => {
            let running = agent::agent_running(pid);
            println!("agent_pid: {pid}");
            println!("agent_running: {running}");
            if !running {
                println!("\nagent is not running — run `devflow check` to advance");
            }
        }
        None => println!("agent_pid: none"),
    }

    if status.is_stale {
        println!("\nstate is stale — run `devflow recover --clean` to clear it");
    }

    Ok(())
}

/// Standalone verify command: runs verify + lint from config.
fn verify_cmd(project_root: &Path) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    println!("verify: {}", config.automation.verify_command);
    let verify_result = verify::run_or_fail(&config.automation.verify_command, project_root)?;
    if !verify_result.stdout.is_empty() {
        println!("{}", verify_result.stdout.trim());
    }

    println!("lint: {}", config.automation.lint_command);
    let lint_result = verify::run_or_fail(&config.automation.lint_command, project_root)?;
    if !lint_result.stdout.is_empty() {
        println!("{}", lint_result.stdout.trim());
    }
    if !lint_result.stderr.is_empty() {
        eprintln!("{}", lint_result.stderr.trim());
    }
    println!("verify & lint: ok");
    Ok(())
}

/// Standalone lint command: runs only the lint command from config.
fn lint_cmd(project_root: &Path) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    println!("lint: {}", config.automation.lint_command);
    let result = verify::run_or_fail(&config.automation.lint_command, project_root)?;
    if !result.stdout.is_empty() {
        println!("{}", result.stdout.trim());
    }
    if !result.stderr.is_empty() {
        eprintln!("{}", result.stderr.trim());
    }
    println!("lint: ok");
    Ok(())
}

/// Standalone docs command: runs the docs command from config and optionally auto-commits.
fn docs_cmd(project_root: &Path) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    println!("docs: {}", config.automation.docs_command);
    let result = verify::run_or_fail(&config.automation.docs_command, project_root)?;
    if !result.stdout.is_empty() {
        println!("{}", result.stdout.trim());
    }
    if !result.stderr.is_empty() {
        eprintln!("{}", result.stderr.trim());
    }

    if config.automation.docs_auto_commit {
        match GitFlow::new(project_root, devflow_core::config::GitFlowConfig::default())
            .commit_all("docs: auto-update from devflow")
        {
            Ok(()) => println!("auto-committed doc changes"),
            Err(err) => println!("warning: could not auto-commit docs: {err}"),
        }
    }
    println!("docs: ok");
    Ok(())
}

fn default_config_yaml() -> &'static str {
    "version:\n  scheme: semver\n  file: pyproject.toml\n  field: project.version\n  build_number: git\nautomation:\n  auto_branch: true\n  auto_verify: true\n  auto_docs: true\n  auto_version: patch\n  auto_ship: false\n  auto_cleanup: true\n  verify_command: \"cargo test\"\n  lint_command: \"cargo clippy -- -D warnings\"\n  docs_command: \"cargo doc --no-deps 2>&1\"\n  continue_on_error: false\n  docs_auto_commit: false\ngit_flow:\n  main: main\n  develop: develop\n  feature_prefix: feature/\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_default_missing_agents_to_claude() {
        let pairs = parse_phase_agent_pairs("7,8", Some("codex")).unwrap();
        assert_eq!(pairs, vec![(7, Agent::Codex), (8, Agent::Claude)]);
    }

    #[test]
    fn pairs_match_agents_positionally() {
        let pairs = parse_phase_agent_pairs("7, 8", Some("claude, codex")).unwrap();
        assert_eq!(pairs, vec![(7, Agent::Claude), (8, Agent::Codex)]);
    }

    #[test]
    fn pairs_default_all_to_claude_without_agents() {
        let pairs = parse_phase_agent_pairs("3,4", None).unwrap();
        assert_eq!(pairs, vec![(3, Agent::Claude), (4, Agent::Claude)]);
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
            (Agent::Claude, Agent::Codex)
        );
        assert!(split_two_agents("claude").is_err());
        assert!(split_two_agents("claude,codex,opencode").is_err());
        assert!(split_two_agents("claude,bogus").is_err());
    }
}
