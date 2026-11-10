use clap::{Parser, Subcommand};
use devflow_core::mode::Mode;
use devflow_core::stage::Stage;
use devflow_core::state::AgentKind;
use std::path::PathBuf;

#[cfg(test)]
mod test_support;

mod staleness;

mod preflight;

mod pipeline_launch;
use pipeline_launch::{resume_with_fallback, run_monitor};

mod pipeline_outcomes;

mod pipeline_gate;
use pipeline_gate::ship_override;

mod phase_worktree;

mod commands;
use commands::{
    cleanup, doctor, evidence, gate_list, gate_respond, gate_show, gate_sweep, history_cmd, logs,
    recover_cmd, start, status, stop, stop_launch,
};
use devflow_core::phase_id::PhaseId;

mod config_parse;

#[derive(Debug, Parser)]
#[command(
    name = "devflow",
    version,
    about = "An opinionated, GSD-native take on AI-driven development automation"
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
        phase: PhaseId,
        /// Agent to launch.
        #[arg(long, default_value = "claude")]
        agent: AgentKind,
        /// Pipeline mode: `auto` runs to Ship unattended; `supervise` gates at Validate.
        #[arg(long)]
        mode: Mode,
        /// Overwrite the feature branch if it already exists.
        #[arg(long)]
        force: bool,
        /// Run the agent directly in the primary checkout instead of an
        /// isolated worktree (not recommended for unattended runs).
        #[arg(long)]
        no_worktree: bool,
        /// Print the pipeline that would run without launching anything.
        #[arg(long)]
        dry_run: bool,
        /// Run the pipeline through `<stage>` and halt cleanly before
        /// advancing further (e.g. `--until plan` runs Define+Plan then
        /// stops before Code). `ship` is rejected — the pipeline already
        /// stops there.
        #[arg(long)]
        until: Option<Stage>,
        /// Pre-authorize the Ship gate so this run can reach a completed
        /// Ship stage unattended (D-04/D-05/D-06, 23-09; provenance widened
        /// by D-12, `28-CONTEXT.md`). The Ship gate still fires and is
        /// still answered through the normal gate protocol — this only
        /// supplies the approval automatically, attributed to `--yes-ship`
        /// in the gate ledger. This flag ORs with a standing `yes_ship =
        /// true` in `devflow.toml` (or `DEVFLOW_YES_SHIP`) rather than
        /// replacing it — passing it here always wins, and a run whose
        /// authorization came from config instead prints a notice naming
        /// `devflow.toml` as the source, so a persisted default is never a
        /// silent one.
        #[arg(long)]
        yes_ship: bool,
        /// A second agent that takes over the Code stage once if the agent is
        /// rate-limited there (D-01, `51-CONTEXT.md`). Failover happens only at
        /// Code; at any other stage a rate limit waits at the stage's gate.
        ///
        /// Persisted in the run state (`state-NN.json`) before the first
        /// supervisor spawns, and checked here as far as launch can tell: it
        /// must differ from `--agent`, its binary must be installed and its
        /// driver healthy, and in `--mode auto` it must be `claude` or
        /// `antigravity` (the unattended launch shape). The full preflight runs
        /// again at the handoff itself.
        ///
        /// Overrides a `fallback_agent` key in `devflow.toml` when both are
        /// given. No environment variable selects it.
        #[arg(long)]
        fallback_agent: Option<AgentKind>,
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
        phase: Option<PhaseId>,
        /// Stage the monitor launched; omitted only by monitors from older binaries.
        #[arg(long)]
        stage: Option<Stage>,
    },
    /// Internal: the stage supervisor's own process entry point (50-11).
    ///
    /// `monitor::spawn_monitor` re-execs this binary as this subcommand,
    /// detached and in its own process group, for both launch shapes.
    /// Everything after `--` is the supervised agent's program and argv.
    /// Hidden for the same reason `advance` is: it is an implementation
    /// detail of the monitor chain, never an operator verb.
    #[command(name = "__monitor", hide = true)]
    Monitor {
        /// Project root whose `.devflow/` capture files this monitor writes.
        #[arg(long)]
        project: PathBuf,
        /// Phase whose stage machine to advance once the child is reaped.
        #[arg(long)]
        phase: PhaseId,
        /// Stage the monitor launched; passed to advance after the child exits.
        #[arg(long)]
        stage: Option<Stage>,
        /// Directory the supervised child runs in (the phase worktree when
        /// worktree mode is active, else the project root).
        #[arg(long)]
        workdir: PathBuf,
        /// Which agent's transport the monitor supervises (selects the
        /// first-turn schema and the close-rule marker predicate).
        #[arg(long)]
        agent: AgentKind,
        /// Supervision shape: `capture` (stdin null, stdout to the capture file) or `stream`.
        #[arg(long, value_enum)]
        launch: pipeline_launch::LaunchShape,
        /// Socket path this supervisor publishes, under the DevFlow cache dir.
        #[arg(long)]
        socket: PathBuf,
        /// Per-launch id the launcher waits to see in this supervisor's ping reply.
        #[arg(long)]
        launch_id: String,
        /// The inherited descriptor holding this launch's liveness lock.
        #[arg(long)]
        liveness_fd: Option<i32>,
        /// Adapter env var NAME to re-apply to the agent (repeatable; the value stays off argv).
        #[arg(long = "keep-env")]
        keep_env: Vec<String>,
        /// File holding the stage prompt (stream shape; a file, not argv, for length).
        #[arg(long)]
        prompt_file: Option<PathBuf>,
        /// Seconds of stream silence before the idle timeout fires (stream shape).
        #[arg(long)]
        idle_timeout_secs: Option<u64>,
        /// The supervised child's program and arguments, after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        argv: Vec<String>,
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
        phase: PhaseId,
        /// Hand the saved run to a different agent driver; omit to keep its saved agent.
        #[arg(long)]
        agent: Option<AgentKind>,
        /// Re-arm the run's one-hop fallback for the Code stage (D-01,
        /// `51-CONTEXT.md`): sets it to this agent and clears a spent mark, so a
        /// run whose fallback was used can fail over once more. Accepted on a run
        /// that is not live (stopped, or whose supervisor has ended);
        /// refused over a live run like any resume.
        ///
        /// Checked as on `devflow start`: it must differ from the agent the run
        /// resumes under, its binary must be installed and its driver healthy, and
        /// in `--mode auto` it must be `claude` or `antigravity`. Omit it to keep
        /// the saved fallback as it is, spent or not. `resume` does not re-read the
        /// `devflow.toml` key; only `start` does.
        #[arg(long)]
        fallback_agent: Option<AgentKind>,
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
        phase: Option<PhaseId>,
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
        phase: Option<PhaseId>,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Remove phase worktrees and their feature branches.
    Cleanup {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Force-remove dirty worktrees.
        #[arg(long)]
        force: bool,
    },
    /// Show current workflow state.
    Status {
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
        phase: Option<PhaseId>,
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
    /// Manually drive a phase through Ship when the monitor that would have
    /// consumed its already-written Ship gate response is dead.
    ///
    /// A second, out-of-process trigger of the SAME terminal effect
    /// (`finish_workflow`) the live poll loop would have run (20e, D-01) —
    /// requires `state.stage == Stage::Ship` and an open Ship gate holding an
    /// answer bound to it, not yet claimed; `--force` never skips an earlier
    /// stage, the foreground holder, the refusal while a supervisor of the
    /// phase is live, the answer claim, or those existence checks (D-02).
    Ship {
        /// Phase to ship.
        #[arg(long)]
        phase: PhaseId,
        /// Accepted for explicit, auditable operator intent. Does NOT skip
        /// the stage, holder, live-supervisor, gate-existence, or answer-claim
        /// checks (D-02) — see `pipeline_gate::ship_override`'s doc comment for
        /// exact scope.
        #[arg(long)]
        force: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// End a running phase cleanly: answers its open gate with a rejection
    /// when a live waiter holds it (a Ship gate with no waiter stores the
    /// rejection for `devflow ship`); otherwise addresses the run's supervisor
    /// socket — `shutdown` when ALIVE, the guarded backstop when STALE. When
    /// no socket answers, it signals the phase's foreground holder (a `start`
    /// or `resume` before its spawn, `devflow ship`, a hand-run `advance`)
    /// once its identity is confirmed. `--launch <id>` tears down only the
    /// socket with that launch id (the form `devflow doctor` prints for an
    /// orphan). Never signals a pid recorded in state. Idempotent: safe to
    /// run against an already-stopped, never-started, or already-dead
    /// phase.
    Stop {
        /// Phase to stop.
        #[arg(long)]
        phase: PhaseId,
        /// Tear down only the supervisor socket with this launch id; answers
        /// no gate, signals no holder and writes no state.
        #[arg(long)]
        launch: Option<String>,
        /// Project root. Overrides the positional argument when supplied.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Report DevFlow's own structural record of whether a phase shipped
    /// (23-06) — a read-only oracle sourced from the append-only event log,
    /// not from any agent-authored attestation document. See
    /// `devflow_core::ship_evidence` for the full contract.
    Evidence {
        /// Phase to report on.
        #[arg(long)]
        phase: PhaseId,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Exit non-zero unless DevFlow's own record shows this phase
        /// shipped — declarable as a Layer 0 `external_verify` probe.
        #[arg(long)]
        require_shipped: bool,
        /// Project root.
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum GateCmd {
    /// List gates awaiting a response.
    List {
        /// List open gates across every root this machine has registered
        /// (`devflow start` registers a launched phase), instead of only
        /// the current project.
        #[arg(long = "all-roots")]
        all_roots: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Approve an open gate — the workflow advances.
    Approve {
        /// Phase whose gate to approve.
        phase: PhaseId,
        /// Stage of the gate (auto-resolved when the phase has exactly one
        /// open gate).
        #[arg(long = "stage")]
        stage: Option<Stage>,
        /// Optional free-text note recorded with the approval.
        #[arg(long)]
        note: Option<String>,
        /// The gate id `gate list` showed. Without it the command answers the
        /// gate open now; with it, a gate opened since is refused.
        #[arg(long = "gate-id")]
        gate_id: Option<String>,
        /// Project root.
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Reject an open gate — loops back to Code, or aborts the phase when
    /// the note contains "abort".
    Reject {
        /// Phase whose gate to reject.
        phase: PhaseId,
        /// Stage of the gate (auto-resolved when the phase has exactly one
        /// open gate).
        #[arg(long = "stage")]
        stage: Option<Stage>,
        /// Required note explaining the rejection (include "abort" to end
        /// the phase instead of looping back to Code).
        #[arg(long)]
        note: String,
        /// The gate id `gate list` showed. Without it the command answers the
        /// gate open now; with it, a gate opened since is refused.
        #[arg(long = "gate-id")]
        gate_id: Option<String>,
        /// Project root.
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Print an open gate's full, untruncated context (21a) — `gate list`
    /// truncates context to 300 chars, as the gate banner does; this reads it in
    /// full, control-char sanitized.
    Show {
        /// Phase whose gate to show.
        phase: PhaseId,
        /// Stage of the gate (auto-resolved when the phase has exactly one
        /// open gate).
        #[arg(long = "stage")]
        stage: Option<Stage>,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Answer or report aged, unattended gates across every registered root
    /// (23b) — bounds an abandoned run's lifetime without `kill(1)` and
    /// without a supervisor. On-demand only: nothing schedules this for you.
    Sweep {
        /// Age threshold in seconds — a gate older than this is reaped.
        /// Defaults to `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` (three days,
        /// held equal to `DEVFLOW_GATE_TIMEOUT_SECS` so a sweep cannot reap a
        /// gate a live monitor is still polling).
        #[arg(long = "max-age-secs")]
        max_age_secs: Option<u64>,
        /// Report what would be reaped without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Restrict the sweep to one project root instead of every root
        /// this machine has registered (`registry::load_roots`).
        #[arg(long)]
        root: Option<PathBuf>,
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
            no_worktree,
            dry_run,
            until,
            yes_ship,
            fallback_agent,
            project,
        } => {
            // Worktree is the default; `--no-worktree` is the only switch.
            let worktree = !no_worktree;
            // D-07: `--until ship` is a semantic no-op — `handle_ship_outcome`
            // calls `finish_workflow` directly and never calls `transition`,
            // so the pipeline already stops at Ship today regardless of this
            // flag. Reject before any stage runs rather than silently
            // accepting a flag that would never actually intercept anything.
            if until == Some(Stage::Ship) {
                return Err(CliError::Message(
                    "--until ship is a no-op: Ship is already the pipeline's terminal \
                     stage and never advances further"
                        .to_string(),
                ));
            }
            start(
                &project_root(project)?,
                phase,
                agent,
                mode,
                force,
                worktree,
                dry_run,
                until,
                yes_ship,
                commands::FallbackSource::FlagOrConfig(fallback_agent),
            )
        }
        Command::Advance {
            project,
            phase,
            stage,
        } => pipeline_launch::advance_command(&project_root(project)?, phase, stage),
        Command::Monitor {
            project,
            phase,
            stage,
            workdir,
            agent,
            launch,
            socket,
            launch_id,
            liveness_fd,
            keep_env,
            prompt_file,
            idle_timeout_secs,
            argv,
        } => run_monitor(
            &pipeline_launch::MonitorInvocation {
                project_root: project_root(project)?,
                phase,
                stage,
                workdir,
                agent,
                launch,
                socket,
                launch_id,
                liveness_fd,
                keep_env,
                prompt_file,
                idle_timeout_secs,
                argv,
            },
            true,
        ),
        Command::Resume {
            phase,
            agent,
            fallback_agent,
            project,
        } => resume_with_fallback(&project_root(project)?, phase, agent, fallback_agent),
        Command::Gate { action } => match action {
            GateCmd::List { all_roots, project } => gate_list(&project_root(project)?, all_roots),
            GateCmd::Approve {
                phase,
                stage,
                note,
                gate_id,
                project,
            } => gate_respond(&project_root(project)?, phase, stage, true, note, gate_id),
            GateCmd::Reject {
                phase,
                stage,
                note,
                gate_id,
                project,
            } => gate_respond(
                &project_root(project)?,
                phase,
                stage,
                false,
                Some(note),
                gate_id,
            ),
            GateCmd::Show {
                phase,
                stage,
                project,
            } => gate_show(&project_root(project)?, phase, stage),
            GateCmd::Sweep {
                max_age_secs,
                dry_run,
                root,
            } => gate_sweep(max_age_secs, dry_run, root),
        },
        Command::Logs {
            phase,
            follow,
            stderr,
            project,
        } => logs(&project_root(project)?, phase, follow, stderr),
        Command::History { phase, project } => history_cmd(&project_root(project)?, phase),
        Command::Cleanup { project, force } => cleanup(&project_root(project)?, force),
        Command::Status { project } => status(&project_root(project)?),
        Command::Recover {
            project,
            clean,
            phase,
        } => recover_cmd(&project_root(project)?, clean, phase),
        Command::Doctor { json, project } => doctor(&project_root(project)?, json),
        Command::Ship {
            phase,
            force,
            project,
        } => ship_override(&project_root(project)?, phase, force),
        Command::Stop {
            phase,
            launch,
            root,
            project,
        } => {
            let root = project_root(root.unwrap_or(project))?;
            match launch {
                Some(launch_id) => stop_launch(&root, phase, &launch_id),
                None => stop(&root, phase),
            }
        }
        Command::Evidence {
            phase,
            json,
            require_shipped,
            root,
        } => evidence(
            &project_root(root.unwrap_or_else(|| PathBuf::from(".")))?,
            phase,
            json,
            require_shipped,
        ),
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The `fallback_agent` a `devflow start` command line parses to.
    fn start_fallback(args: &[&str]) -> Result<Option<AgentKind>, clap::Error> {
        let cli = Cli::try_parse_from(args)?;
        let Command::Start { fallback_agent, .. } = cli.command else {
            panic!(
                "`devflow start` parsed to another command: {:?}",
                cli.command
            );
        };
        Ok(fallback_agent)
    }

    #[test]
    fn resume_accepts_a_fallback_agent_flag() {
        let parse = |extra: &[&'static str]| {
            let mut args = vec!["devflow", "resume", "--phase", "51"];
            args.extend_from_slice(extra);
            let cli = Cli::try_parse_from(args)?;
            let Command::Resume { fallback_agent, .. } = cli.command else {
                panic!(
                    "`devflow resume` parsed to another command: {:?}",
                    cli.command
                );
            };
            Ok::<_, clap::Error>(fallback_agent)
        };
        assert_eq!(
            parse(&["--fallback-agent", "antigravity"]).expect("a known agent parses"),
            Some(AgentKind::Antigravity)
        );
        assert!(
            parse(&["--fallback-agent", ""]).is_err(),
            "an empty fallback is a parse error"
        );
        assert_eq!(
            parse(&[]).expect("the flag is optional"),
            None,
            "omitting the flag re-arms nothing"
        );
    }

    #[test]
    fn start_accepts_a_fallback_agent_flag() {
        let base = [
            "devflow",
            "start",
            "--phase",
            "51",
            "--agent",
            "claude",
            "--mode",
            "supervise",
        ];
        let with = |extra: &[&'static str]| {
            let mut args = base.to_vec();
            args.extend_from_slice(extra);
            start_fallback(&args)
        };
        assert_eq!(
            with(&["--fallback-agent", "codex"]).expect("a known agent parses"),
            Some(AgentKind::Codex)
        );
        assert!(
            with(&["--fallback-agent", ""]).is_err(),
            "an empty fallback is a parse error"
        );
        assert!(
            with(&["--fallback-agent", "nosuch"]).is_err(),
            "an unknown fallback is a parse error"
        );
        assert_eq!(
            with(&[]).expect("the flag is optional"),
            None,
            "omitting the flag leaves no fallback"
        );
    }
}
