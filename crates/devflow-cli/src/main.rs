use clap::{Parser, Subcommand};
use devflow_core::config::Config;
use devflow_core::git::GitFlow;
use devflow_core::state::{Agent, State, Step};
use devflow_core::{monitor, tmux, version, workflow};
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
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
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
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Config(#[from] devflow_core::config::ConfigError),
    #[error(transparent)]
    Workflow(#[from] devflow_core::workflow::WorkflowError),
    #[error(transparent)]
    Git(#[from] devflow_core::git::GitError),
    #[error(transparent)]
    Tmux(#[from] devflow_core::tmux::TmuxError),
    #[error(transparent)]
    Version(#[from] devflow_core::version::VersionError),
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
            project,
        } => start(&project_root(project)?, phase, agent, use_monitor),
        Command::Check { project } => check(&project_root(project)?),
        Command::Status { project } => status(&project_root(project)?),
        Command::Ship { project } => ship(&project_root(project)?),
        Command::Init { project, force } => init(&project_root(project)?, force),
        Command::Config { project } => show_config(&project_root(project)?),
    }
}

fn start(project_root: &Path, phase: u32, agent: Agent, use_monitor: bool) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    let mut state = State::new(phase, agent, project_root.to_path_buf());

    if config.automation.auto_branch {
        let git = GitFlow::new(project_root, config.git_flow.clone());
        match git.feature_start(phase) {
            Ok(branch) => println!("created feature branch: {branch}"),
            Err(err) => println!("warning: could not create feature branch: {err}"),
        }
    }

    state.step = Step::Executing;
    match tmux::launch_agent(&state) {
        Ok(session) => {
            state.tmux_session = Some(session.clone());
            println!("launched {} in tmux session: {session}", agent.name());
        }
        Err(err) => {
            state.tmux_session = Some(state.tmux_session_name());
            println!("warning: could not launch tmux agent: {err}");
        }
    }

    if use_monitor {
        match monitor::spawn_monitor(&state) {
            Ok(pid) => {
                state.monitor_pid = Some(pid);
                println!("monitor spawned (pid {pid}) — will auto-advance when agent exits");
            }
            Err(err) => println!("warning: could not spawn monitor: {err}"),
        }
    }

    workflow::save_state(&state)?;
    println!("started phase {} at {}", state.phase, state.started_at);
    Ok(())
}

fn check(project_root: &Path) -> Result<(), CliError> {
    let config = Config::load(project_root)?;
    let state = workflow::load_state(project_root)?;

    if state.step == Step::Executing {
        if let Some(session) = &state.tmux_session {
            match tmux::agent_running(session) {
                Ok(true) => {
                    println!("agent still running in tmux session: {session}");
                    return Ok(());
                }
                Ok(false) => println!("agent session ended: {session}"),
                Err(err) => println!("warning: could not inspect tmux session: {err}"),
            }
        }
    }

    let result = workflow::advance_state(state, &config)?;
    println!("{}", result.message);
    println!("current step: {}", result.state.step);
    Ok(())
}

fn status(project_root: &Path) -> Result<(), CliError> {
    match workflow::load_state(project_root) {
        Ok(state) => {
            println!("step: {}", state.step);
            println!("phase: {}", state.phase);
            println!("agent: {}", state.agent.name());
            println!("started_at: {}", state.started_at);
            println!("project_root: {}", state.project_root.display());
            match &state.tmux_session {
                Some(session) => {
                    let running = tmux::agent_running(session).unwrap_or(false);
                    println!("tmux_session: {session}");
                    println!("agent_running: {running}");
                }
                None => println!("tmux_session: none"),
            }
        }
        Err(devflow_core::workflow::WorkflowError::MissingState(_)) => {
            println!("step: idle");
            println!("project_root: {}", project_root.display());
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
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

fn default_config_yaml() -> &'static str {
    "version:\n  scheme: semver\n  file: pyproject.toml\n  field: project.version\n  build_number: git\nautomation:\n  auto_branch: true\n  auto_verify: true\n  auto_docs: true\n  auto_version: patch\n  auto_ship: false\n  auto_cleanup: true\ngit_flow:\n  main: main\n  develop: develop\n  feature_prefix: feature/\n"
}
