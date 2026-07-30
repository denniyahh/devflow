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
use pipeline_launch::{advance, resume};

mod pipeline_outcomes;

mod pipeline_gate;
use pipeline_gate::ship_override;

mod parallel;
use parallel::parallel;

mod commands;
use commands::{
    cleanup, doctor, evidence, gate_list, gate_respond, gate_show, gate_sweep, history_cmd, list,
    logs, recover_cmd, reference, release_check, release_execute, resolve_gate_target, start,
    status, stop, sync_cmd, test_cmd,
};

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
        /// Run the pipeline through `<stage>` and halt cleanly before
        /// advancing further (e.g. `--until plan` runs Define+Plan then
        /// stops before Code). `ship` is rejected — the pipeline already
        /// stops there.
        #[arg(long)]
        until: Option<Stage>,
        /// Pre-authorize the Ship gate so this run can reach a completed
        /// Ship stage unattended (D-04/D-05/D-06, 23-09). The Ship gate
        /// still fires and is still answered through the normal gate
        /// protocol — this only supplies the approval automatically,
        /// attributed to `--yes-ship` in the gate ledger. Must be typed on
        /// every invocation: it cannot be set in `devflow.toml` or any
        /// environment variable (D-05), so an unattended auto-merge can
        /// never become a standing, silent default.
        #[arg(long)]
        yes_ship: bool,
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
    /// Cut a release: `--check` runs the read-only preflight (self-pin,
    /// develop/main divergence, crates.io publish order, and tag-signing
    /// viability); `--execute --yes-release` runs the real release-cut
    /// sequence (version bump, signed tag, sync back, crates.io publish).
    ///
    /// Shipping ends at merge to `develop`; releasing ends at merge to `main`
    /// plus a full version release. `--yes-release` authorizes the entire
    /// bump/tag/sync/publish sequence as one typed act, must be typed on
    /// every invocation, and cannot be set in `devflow.toml` or any
    /// environment variable, so an unattended release can never become a
    /// standing default. `--check` and `--execute` are mutually exclusive.
    Release {
        /// Run the read-only preflight checks only. Mutually exclusive with
        /// `--execute`.
        #[arg(long)]
        check: bool,
        /// Run the real release-cut sequence (version bump, signed tag, sync
        /// back, crates.io publish), halting cleanly at the `develop`->`main`
        /// human gate if it has not yet been merged. Requires
        /// `--yes-release`. Mutually exclusive with `--check`.
        #[arg(long)]
        execute: bool,
        /// Authorizes `--execute` to run the release-cut sequence. This is a
        /// separate authorization from `--yes-ship` — neither implies the
        /// other. Must be typed on every invocation: it cannot be set in
        /// `devflow.toml` or any environment variable, so an unattended
        /// release can never become a standing, silent default.
        #[arg(long)]
        yes_release: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Sync `main` back into `develop` after a release.
    ///
    /// Direct-pushes `origin/develop` (D-08) — refuses on a dirty working
    /// tree or when not checked out on `develop`, and is a no-op when
    /// `origin/main` is already an ancestor of `develop`. The Rust port of
    /// `scripts/sync-main-to-develop.sh`; the same `sync_main_to_develop`
    /// function this subcommand calls is also the release executor's own
    /// sync step (D-07, 999.52).
    Sync {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Manually drive a phase through Ship when the monitor that would have
    /// consumed its already-written Ship gate response is dead.
    ///
    /// A second, out-of-process trigger of the SAME terminal effect
    /// (`finish_workflow`) the live poll loop would have run (20e, D-01) —
    /// requires `state.stage == Stage::Ship` and an existing Ship gate
    /// request+response pair with no prior ack; `--force` never skips an
    /// earlier stage, the lock, or those existence checks (D-02).
    Ship {
        /// Phase to ship.
        #[arg(long)]
        phase: u32,
        /// Accepted for explicit, auditable operator intent. Does NOT skip
        /// the stage, lock, gate-existence, or ack checks (D-02) — see
        /// `pipeline_gate::ship_override`'s doc comment for exact scope.
        #[arg(long)]
        force: bool,
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// End a running phase cleanly (23c): answers its open gate with a
    /// rejection if one is open — the target unwinds through its own abort
    /// path, no signal sent — otherwise signals the process recorded in its
    /// per-phase lock file (`.devflow/lock-{phase:02}`), never
    /// `state.monitor_pid` (the PID `devflow status` displays, and the
    /// wrong one — see `commands::stop`'s doc comment). Idempotent: safe to
    /// run against an already-stopped, never-started, or already-dead
    /// phase.
    Stop {
        /// Phase to stop.
        #[arg(long)]
        phase: u32,
        /// Project root. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Report DevFlow's own structural record of whether a phase shipped
    /// (23-06) — a read-only oracle sourced from the append-only event log,
    /// not from any agent-authored attestation document. See
    /// `devflow_core::ship_evidence` for the full contract.
    Evidence {
        /// Phase to report on.
        #[arg(long)]
        phase: u32,
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
    /// Print an open gate's full, untruncated context (21a) — `gate list`
    /// truncates context to 100 chars for the table view; this reads it in
    /// full, control-char sanitized.
    Show {
        /// Phase whose gate to show.
        phase: u32,
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
        /// Defaults to `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` (six hours).
        #[arg(long = "max-age-secs")]
        max_age_secs: Option<u64>,
        /// Report what would be reaped without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Restrict the sweep to one project root instead of every root
        /// this machine has registered (`registry::load_roots`). Does NOT
        /// scope `--reap-strays`: that pass is machine-wide by construction
        /// (a stray has no project root to scope by), and the
        /// reachability safety filter it uses is likewise always
        /// machine-wide regardless of this flag (CR-01, 25-15).
        #[arg(long)]
        root: Option<PathBuf>,
        /// Also discover and clear devflow processes shaped like a monitor
        /// wrapper or an `advance` child (999.44), by scanning the OS
        /// process table directly rather than trusting a lock file the
        /// process itself wrote. Matches only two structural argv shapes,
        /// owned by the calling user, older than the minimum age, and NOT
        /// named by any registered root's state file or lock file (CR-01,
        /// 25-15) — a pid a live registry entry still reaches is never
        /// touched. Discovery itself stays registry-independent, so this
        /// still catches a process whose project root no longer exists on
        /// disk (`devflow stop`/the default sweep above cannot see
        /// either). Off by default: preview with `--dry-run` before
        /// authorising it for real.
        #[arg(long)]
        reap_strays: bool,
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
            until,
            yes_ship,
            project,
        } => {
            // Worktree is now the default; the deprecated `--worktree` flag is
            // an intentionally ignored no-op (see field doc comment above).
            // `--no-worktree` is the only switch that changes behavior.
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
            )
        }
        Command::Advance { project, phase } => advance(&project_root(project)?, phase),
        Command::Resume { phase, project } => resume(&project_root(project)?, phase),
        Command::Gate { action } => match action {
            GateCmd::List { all_roots, project } => gate_list(&project_root(project)?, all_roots),
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
            GateCmd::Show {
                phase,
                stage,
                project,
            } => gate_show(&project_root(project)?, phase, stage),
            GateCmd::Sweep {
                max_age_secs,
                dry_run,
                root,
                reap_strays,
            } => gate_sweep(max_age_secs, dry_run, root, reap_strays),
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
        Command::Release {
            check,
            execute,
            yes_release,
            project,
        } => {
            // D-03: --yes-release is read ONLY here, from the parsed CLI
            // flag — never from `devflow.toml`, an environment variable, or
            // persisted `State`. If it were ever sourced from any of those,
            // one typed authorization would silently become a standing
            // default, the exact failure `--yes-ship`'s own design forbids.
            // The two branches resolve their project root by DIFFERENT rules,
            // on purpose (D-13): `--check` is read-only and legitimately walks
            // up to the owning `.devflow` from a subdirectory, while
            // `--execute` is irreversible and must never act on a repository
            // the operator did not name (C-06). The asymmetry is a decision,
            // not an oversight.
            if check && execute {
                Err(CliError::Message(
                    "devflow release: --check and --execute are mutually exclusive — pick one"
                        .to_string(),
                ))
            } else if check {
                release_check(&project_root(project)?)
            } else if execute && !yes_release {
                Err(CliError::Message(
                    "devflow release --execute requires --yes-release: this authorizes the \
                     entire version bump, signed tag, sync, and crates.io publish sequence as \
                     one typed act, and must be typed on every invocation — it cannot be set in \
                     devflow.toml or any environment variable."
                        .to_string(),
                ))
            } else if execute {
                release_execute(&mutating_project_root(project)?)
            } else {
                Err(CliError::Message(
                    "devflow release: pass --check for the read-only preflight, or \
                     --execute --yes-release to run the release-cut sequence"
                        .to_string(),
                ))
            }
        }
        Command::Sync { project } => sync_cmd(&mutating_project_root(project)?),
        Command::Ship {
            phase,
            force,
            project,
        } => ship_override(&project_root(project)?, phase, force),
        Command::Stop { phase, root } => stop(
            &project_root(root.unwrap_or_else(|| PathBuf::from(".")))?,
            phase,
        ),
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

/// Resolve the project root for a **mutating** command — one that pushes,
/// tags, or publishes.
///
/// Do not "simplify" this back into [`project_root`]. That resolver walks *up*
/// to the nearest `.devflow` ancestor, which is correct for pipeline commands
/// that genuinely need to find their owning project from a subdirectory. For
/// `release --execute` and `sync` it is a defect (26-REVIEW.md **C-06**): a
/// phase worktree (`.worktrees/phase-NN/`) has no `.devflow` while the parent
/// checkout does, so the upward walk silently substituted a repository the
/// operator never named — and then ran an *irreversible* `git push`, `git tag`,
/// and `cargo publish` against it. Worse, the executor's four entry guards all
/// tested the substituted root, so a dirty worktree beside a clean parent made
/// the executor MORE likely to proceed.
///
/// **D-13** requires a mutating command to refuse when the resolved root
/// differs from the directory it was invoked in, naming both paths. This
/// resolves via `git rev-parse --show-toplevel` instead — the latitude D-13
/// explicitly grants — because it makes a silent redirect structurally
/// impossible rather than merely detected, and because it is what these
/// commands actually need: neither `release.rs` nor `sync.rs` reads `.devflow`,
/// `devflow_dir`, or `events::emit` at all. They want a **git repository
/// root**; the pipeline resolver's `.devflow` marker is not that, and is
/// deliberately NOT consulted here.
///
/// There is intentionally **no bypass flag and no environment variable**: an
/// escape hatch would recreate C-06 for precisely the operator most likely to
/// be in a hurry.
fn mutating_project_root(project: PathBuf) -> Result<PathBuf, CliError> {
    if !project.exists() {
        return Err(CliError::Message(format!(
            "project path does not exist: {}",
            project.display()
        )));
    }

    let start = project
        .canonicalize()
        .map_err(|err| CliError::Message(format!("failed to resolve project path: {err}")))?;

    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&start)
        .output()
        .map_err(|err| {
            CliError::Message(format!(
                "could not determine the git repository root for {}: {err}. A mutating release \
                 command acts only on the repository it was invoked in and never searches \
                 upward for one.",
                start.display()
            ))
        })?;
    if !output.status.success() {
        // Never a fallback to the upward walk — falling back IS the silent
        // redirect this function removes.
        return Err(CliError::Message(format!(
            "{} is not inside a git repository. A mutating release command pushes, tags, and \
             publishes, so it acts only on the repository it was invoked in and never searches \
             upward for one.",
            start.display()
        )));
    }

    let reported = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // Canonicalize BOTH sides before comparing. On a symlinked checkout (and
    // on macOS's `/tmp`), comparing a raw path against a resolved one produces
    // a spurious refusal — and a spurious refusal in a release tool teaches
    // operators to route around the guard.
    let toplevel = std::path::Path::new(&reported)
        .canonicalize()
        .map_err(|err| {
            CliError::Message(format!(
                "failed to resolve the git repository root reported as {reported}: {err}"
            ))
        })?;

    if toplevel != start {
        return Err(CliError::Message(format!(
            "refusing to act on a repository you did not name: you invoked this from {}, but \
             that is inside the git repository rooted at {}. A mutating release command pushes, \
             tags, and publishes, so it never substitutes a root for the one you named. Either \
             `cd {}` and re-run, or pass that path as the command's [PROJECT] argument.",
            start.display(),
            toplevel.display(),
            toplevel.display()
        )));
    }

    Ok(toplevel)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a throwaway git repository. Hermetic (999.37): pinning cwd alone
    /// does not stop an inherited `GIT_DIR` from retargeting the real
    /// repository — which would retarget the very resolution under test.
    fn init_repo(root: &std::path::Path) {
        let output = devflow_core::test_support::git_command(root)
            .args(["init", "-q"])
            .output()
            .expect("spawn git init");
        assert!(
            output.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn mutating_project_root_accepts_the_repository_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        init_repo(&root);

        assert_eq!(
            mutating_project_root(root.clone()).unwrap(),
            root.canonicalize().unwrap()
        );
    }

    #[test]
    fn mutating_project_root_refuses_a_subdirectory_and_names_both_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        init_repo(&root);
        let nested = root.join("crates/devflow-cli");
        std::fs::create_dir_all(&nested).unwrap();

        let error = mutating_project_root(nested.clone())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&nested.canonicalize().unwrap().display().to_string()),
            "the refusal must name the invoking path: {error}"
        );
        assert!(
            error.contains(&root.canonicalize().unwrap().display().to_string()),
            "the refusal must name the resolved repository root: {error}"
        );
        assert!(
            error.contains("cd "),
            "the refusal must offer the `cd` remedy: {error}"
        );
        assert!(
            error.contains("[PROJECT]"),
            "the refusal must offer the explicit-target remedy: {error}"
        );
    }

    #[test]
    fn mutating_project_root_refuses_outside_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("not-a-repo");
        std::fs::create_dir_all(&bare).unwrap();

        let result = mutating_project_root(bare.clone());
        assert!(
            result.is_err(),
            "a directory outside any git repository must not resolve"
        );
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("not inside a git repository"),
            "the refusal must say so plainly: {error}"
        );
    }

    /// The C-06 property at the unit level: the marker that caused the
    /// redirect no longer participates at all.
    #[test]
    fn mutating_project_root_does_not_consult_devflow() {
        let dir = tempfile::tempdir().unwrap();
        let ancestor = dir.path().join("owning-project");
        std::fs::create_dir_all(ancestor.join(".devflow")).unwrap();
        let nested = ancestor.join("worktrees/phase-99");
        std::fs::create_dir_all(&nested).unwrap();
        init_repo(&nested);

        assert_eq!(
            mutating_project_root(nested.clone()).unwrap(),
            nested.canonicalize().unwrap(),
            "the nested repository must win — the `.devflow` ancestor is not consulted"
        );
    }

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
}
