//! Every CLI subcommand handler and the display/rendering helpers they
//! share: `start`, the gate/status/logs/history family, worktree listing,
//! recovery, and `devflow doctor`'s project-aware reconciliation core.
//!
//! D-07: this is deliberately one flat file, not a `commands/`
//! subdirectory. Mapping Phase 18's plans onto clusters showed this
//! cluster absorbed only 2 of 7 plans (pipeline absorbed 3), so a
//! per-subcommand directory buys zero measured wave reduction — and it
//! tends to re-centralise the shared display helpers this file already
//! keeps flat into a `common.rs`, recreating exactly the contention the
//! split is meant to remove.

use crate::CliError;
use crate::config_parse::GATE_ESCALATION_THRESHOLD_SECS;
use crate::parallel::ensure_phase_worktree;
use crate::pipeline_gate::print_dry_run;
use crate::pipeline_launch::{launch_stage, single_active_phase};
use crate::pipeline_outcomes::render_gate_context;
use crate::preflight::{agent_program, ensure_agent_binary};
use crate::staleness::run_git_stdout;
use devflow_core::agent;
use devflow_core::agent_result;
use devflow_core::agents;
use devflow_core::config::{DEVELOP, FEATURE_PREFIX};
use devflow_core::events;
use devflow_core::gates::{GateAction, GateResponse, Gates, OpenGate};
use devflow_core::git::GitFlow;
use devflow_core::history;
use devflow_core::mode::Mode;
use devflow_core::recover;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::workflow;
use devflow_core::worktree;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn resolve_gate_target(
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn start(
    project_root: &Path,
    phase: u32,
    agent: AgentKind,
    mode: Mode,
    force: bool,
    worktree: bool,
    dry_run: bool,
    until: Option<Stage>,
) -> Result<(), CliError> {
    let mut state = State::new(phase, agent, mode, project_root.to_path_buf());
    state.stop_until = until;

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

// ---------------------------------------------------------------------------
// reference / cleanup / list / status / recover
// ---------------------------------------------------------------------------

/// Create or refresh the static reference worktree.
pub(crate) fn reference(
    project_root: &Path,
    branch: Option<String>,
    refresh: bool,
) -> Result<(), CliError> {
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

/// Parse the phase number encoded in a `.worktrees/phase-NN[-agent]` path.
/// Used only as a fallback join key when no persisted `State.worktree_path`
/// matches the worktree entry (review: Codex MEDIUM — worktree->phase join).
/// Returns `None` for paths that don't follow this naming (e.g. the static
/// `reference` worktree), which correctly excludes it from the liveness
/// guard — a snapshot has no owning phase/agent to be alive.
fn phase_from_worktree_path(worktrees_dir: &Path, path: &Path) -> Option<u32> {
    let name = path.strip_prefix(worktrees_dir).ok()?.to_str()?;
    let rest = name.strip_prefix("phase-")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Join a `git worktree list` entry to its owning phase `State`, preferring
/// the persisted `worktree_path` (set by `start`/`parallel`) and falling back
/// to worktree-directory-name or branch-name matching only when no
/// `worktree_path` match exists (review: Codex MEDIUM). Returns `None` when
/// no owning state can be found at all (e.g. the phase already shipped and
/// its state was cleared) — callers treat that as "no liveness signal",
/// not as an implicit "safe to remove."
fn state_for_worktree<'a>(
    states: &'a [State],
    worktrees_dir: &Path,
    wt: &worktree::WorktreeInfo,
) -> Option<&'a State> {
    if let Some(state) = states
        .iter()
        .find(|s| s.worktree_path.as_deref() == Some(wt.path.as_path()))
    {
        return Some(state);
    }
    if let Some(phase) = phase_from_worktree_path(worktrees_dir, &wt.path)
        && let Some(state) = states.iter().find(|s| s.phase == phase)
    {
        return Some(state);
    }
    if let Some(branch) = &wt.branch {
        return states
            .iter()
            .find(|s| *branch == format!("{FEATURE_PREFIX}phase-{:02}", s.phase));
    }
    None
}

/// Bounded-backoff retry around `worktree::remove`, absorbing the transient
/// `Directory not empty` race that can occur even after a phase is confirmed
/// dead (a lingering fd/writer from the just-exited agent). NOT a substitute
/// for the liveness guard above — only reached once a phase is confirmed
/// dead (agent dead AND monitor not active). `git worktree prune` is
/// deliberately not used here: it only clears metadata for already-absent
/// directories and would orphan leftover files on disk (Pitfall 3).
fn remove_worktree_with_retry(
    project_root: &Path,
    path: &Path,
    force: bool,
) -> Result<(), worktree::WorktreeError> {
    const ATTEMPTS: u32 = 3;
    const BASE_DELAY_MS: u64 = 50;
    let mut last_err = None;
    for attempt in 0..ATTEMPTS {
        match worktree::remove(project_root, path, force) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                if attempt + 1 < ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(
                        BASE_DELAY_MS * 2u64.pow(attempt),
                    ));
                }
            }
        }
    }
    Err(last_err.expect("loop runs ATTEMPTS >= 1 times"))
}

/// Remove phase worktrees (and the reference with --force), deleting their
/// associated feature branches, then prune and clean up merged branches.
///
/// Hard-refuses (D-06, no override flag) removal of any worktree whose owning
/// phase has a live agent (any monitor state, including Unknown/Stuck) or an
/// active monitor (Healthy/BetweenStages) — closing the race where a real
/// `cleanup --force` run could delete a worktree a live agent/monitor is
/// still writing into (review: Codex HIGH, fail-closed on a live agent).
pub(crate) fn cleanup(project_root: &Path, force: bool) -> Result<(), CliError> {
    let git = GitFlow::new(project_root);
    let worktrees_dir = worktree::worktrees_dir(project_root);
    let reference = worktree::reference_path(project_root);
    let states = workflow::list_states(project_root);

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

        let matched_state = state_for_worktree(&states, &worktrees_dir, wt);
        let phase = matched_state
            .map(|s| s.phase)
            .or_else(|| phase_from_worktree_path(&worktrees_dir, &wt.path));
        let agent_alive = phase
            .and_then(|p| agent_pid_from_file(project_root, p))
            .is_some_and(agent::agent_running);
        let monitor_pid = matched_state.and_then(|s| s.monitor_pid);
        let monitor_alive = monitor_pid.is_some_and(agent::agent_running);
        let phase_liveness = liveness(monitor_pid, monitor_alive, agent_alive);

        // Fail-closed on a live agent: refuse whenever the agent is alive
        // (regardless of monitor liveness — Unknown/Stuck included) OR the
        // monitor is actively running the stage (Healthy/BetweenStages).
        // Only Stuck/Unknown WITHOUT a live agent proceeds.
        if agent_alive || matches!(phase_liveness, Liveness::Healthy | Liveness::BetweenStages) {
            let phase_label = phase
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".to_string());
            return Err(CliError::Message(format!(
                "refusing to remove worktree {} for phase {phase_label} ({}) — run `devflow resume --phase {phase_label}` or wait for it to finish",
                wt.path.display(),
                phase_liveness.describe(),
            )));
        }

        match remove_worktree_with_retry(project_root, &wt.path, force) {
            Ok(()) => {
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
            Err(err) => {
                println!(
                    "warning: could not remove worktree {} after retrying — manually delete this directory: {err}",
                    wt.path.display()
                );
            }
        }
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

pub(crate) fn status(project_root: &Path) -> Result<(), CliError> {
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
pub(crate) fn gate_list(project_root: &Path) -> Result<(), CliError> {
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
pub(crate) fn gate_respond(
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
pub(crate) fn logs(
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
pub(crate) fn history_cmd(project_root: &Path, phase: Option<u32>) -> Result<(), CliError> {
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

pub(crate) fn list(project_root: &Path) -> Result<(), CliError> {
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

pub(crate) fn recover_cmd(
    project_root: &Path,
    do_clean: bool,
    phase: Option<u32>,
) -> Result<(), CliError> {
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
pub(crate) fn test_cmd(project_root: &Path) -> Result<(), CliError> {
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
pub(crate) fn doctor(project_root: &Path, json: bool) -> Result<(), CliError> {
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
    use crate::{Cli, Command, GateCmd};
    use clap::Parser;

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
    fn describe_worktree_dir_infers_phase_and_agent() {
        assert_eq!(
            describe_worktree_dir("phase-07-claude"),
            " — phase 7, agent claude"
        );
        assert_eq!(describe_worktree_dir("phase-08"), " — phase 8");
        assert_eq!(describe_worktree_dir("reference"), "");
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
