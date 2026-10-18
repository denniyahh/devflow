//! Pipeline seam A (D-06): launching a stage's agent and driving the
//! `advance` decision after a monitored agent exits. Extracted mechanically
//! (19-08, D-09 pure move) out of `main.rs` — every function below is
//! byte-identical to its pre-move body modulo an added `pub(crate)` and
//! adjusted `use` paths.
//!
//! **This module participates in the pipeline's three-way module cycle
//! (19-RESEARCH.md Pattern 1), and that is intentional:** [`launch_stage`]
//! calls [`crate::preflight::run_preflight`] on the way in, while
//! `run_preflight`'s `Advance` arm calls [`launch_stage_inner`] back
//! directly (18-07, D-18f) — the bidirectional preflight/launch pair is
//! unchanged by this split, just repointed to a named module. Closing the
//! OTHER side of the cycle, `pipeline_gate::transition` (and
//! `loop_back_to_code`) call [`launch_stage`] at their last line — that
//! call is what actually drives the state machine forward after a stage
//! transition, and is the edge that closes `launch → outcomes → gate →
//! launch` back to this module. Rust permits cyclic module references
//! (only the crate dependency graph must be acyclic), so this compiles
//! cleanly. All three pipeline modules import from each other directly —
//! see `pipeline_gate`'s module doc comment for the explicit caveat that
//! this cycle is NOT a wave-parallelism promise for future pipeline work.

use crate::CliError;
use crate::config_parse::gate_timeout_secs;
use crate::pipeline_gate::transition;
use crate::pipeline_gate::{abort, run_gate_with_timeout};
use crate::pipeline_outcomes::{
    ValidateOutcome, classify_validate_outcome, handle_infra_outcome, handle_rate_limited_outcome,
    handle_ship_failure, handle_ship_outcome, handle_stage_failure, handle_validate_outcome,
    truncate_reason,
};
use crate::preflight::{
    agent_program, ensure_agent_binary, generic_preflight_checks, run_preflight,
    worktree_writable_roots,
};
use devflow_core::config::capture_retention;
use devflow_core::gates::{GateAction, Gates};
use devflow_core::mode::Mode;
use devflow_core::outcome_policy::{self, Action};
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt;
use devflow_core::stage::Stage;
use devflow_core::state::SupervisorRecord;
use devflow_core::state::{AgentKind, ApprovedCheckpoint, CheckpointApproval, State};
use devflow_core::supervisor::runtime::{AgentOutcome, OwnerPhase, SafePoint, StopToken};
use devflow_core::verify::CheckpointDeclaration;
use devflow_core::{
    agent_result, agents, canary, events, gsd_config, lock, mode, monitor, verify, workflow,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// `advance` waits for a post-spawn lock holder because it normally finishes
/// in seconds; after ten chosen (not measured) minutes the holder is likely at
/// a gate or wedged, and doctor then resume is the explicit recovery path.
pub(crate) const ADVANCE_LOCK_WAIT: Duration = Duration::from_secs(600);

/// The post-preflight body of [`launch_stage`]: capture archival/rollover
/// and spawning the monitor. (25b, D-03: this function no longer performs
/// self-dogfood build-staleness enforcement — that check is now adjudicated
/// once, in `commands::start`, before this function's first caller ever
/// runs. See the module-level history at the removed call site below.)
/// Extracted (18f, D-18f) so `run_preflight`'s `Advance` arm can call it
/// directly and skip the just-adjudicated preflight check, while every
/// other caller keeps going through [`launch_stage`]'s full path (readiness
/// resolution, `ensure_agent_binary`, then `run_preflight`).
///
/// 35.2 (999.89 / HARDEN-03, P-01/P-02): stamp the run-owned nonce and
/// re-observe the artifact baseline into a window scoped to THIS Validate
/// dispatch.
///
/// The stamp and the re-observation are ONE mechanism — do not separate.
/// Extracted so the co-location is testable without spawning an agent.
fn stamp_validate_dispatch_window(state: &mut State) {
    if state.stage != Stage::Validate {
        return;
    }
    let evidence_root = state
        .worktree_path
        .as_deref()
        .unwrap_or(&state.project_root);
    state.verification_run_nonce =
        Some(state.verification_run_nonce.unwrap_or(0).saturating_add(1));
    state.last_verification_fingerprint =
        devflow_core::agent_result::phase_verification_fingerprint(evidence_root, state.phase);
    state.last_verification_mtime_nanos =
        devflow_core::agent_result::phase_verification_mtime_nanos(evidence_root, state.phase);
    state.verification_baseline_captured = true;
}

/// Recomputes `prompt`/`adapter`/`roots`/`program`/`args` from `state` and
/// `prompt_override` — deliberately NOT threaded through as parameters.
/// They are pure functions of `state` and the prompt override; recomputing
/// them here (rather than widening `run_preflight`'s signature to carry
/// them from `launch_stage`'s earlier resolution) keeps this function
/// callable entirely on its own, which is exactly what `run_preflight`'s
/// `Advance` arm needs. This does not duplicate `worktree_writable_roots`'s
/// logic — both call sites call the same shared helper.
pub(crate) fn launch_stage_inner(
    state: &mut State,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<(), CliError> {
    let driver = agents::driver_for(state.agent);
    let prompt = prompt_override.unwrap_or_else(|| {
        driver.render_prompt(&prompt::StageIntent::for_stage_in_project(
            state.stage,
            state.phase,
            Some(&state.project_root),
        ))
    });
    // In worktree mode the agent's cwd is the linked worktree, but git
    // metadata for commits lives under the main repo's `.git/` — sandboxed
    // agents need it (and the worktree admin dir, which Codex read-only-
    // mounts otherwise) writable (13-06 dogfood finding).
    let roots = state
        .worktree_path
        .as_deref()
        .map(|wt| worktree_writable_roots(&state.project_root, wt))
        .unwrap_or_default();
    // D-09/D-10 sequencing gate. `ClaudeDriver::build_command` is itself
    // unconditional and stage-blind (constraint 1 forbids predicting at launch
    // time which stages background work); the choice of which stages have been
    // widened to it *yet* is a rollout-order choice, made here at the call
    // site, which constraint 1 permits.
    //
    // Evaluated ONCE and reused by the canary gate below, so a single predicate
    // governs both the launch shape and the guard that protects it — the guard
    // must fire on exactly the launches whose premise it checks, and two
    // separate evaluations of "is this the stream path?" would be free to drift.
    let stream_launch = stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);

    // D-11 (31-04): the opt-out is loud, on three channels. Fires only when the
    // opt-out is what made the difference — forcing legacy on a stage the
    // rollout has not reached changes nothing, and a notice there would imply
    // it did. Placed ahead of the canary gate so the operator learns what the
    // legacy path costs before anything else happens.
    if !stream_launch && stream_launch_enabled(state.agent, state.stage, false) {
        announce_forced_legacy_launch(state);
    }

    // D-15: refuse before any launch work if the undocumented CLI behaviour
    // this transport depends on is not backed by observed behaviour. Placed
    // ahead of `spawn_agent_and_record` so a refusal costs no archival rollover
    // and spawns no monitor.
    // Resolved into owned values BEFORE the gate takes `&mut State`. The
    // canary's child runs where the stage's agent would (the worktree when
    // there is one), and its throwaway capture lands beside the run's other
    // runtime files — never on the phase capture the Layer 1 cascade reads.
    let canary_workdir = state
        .worktree_path
        .as_deref()
        .unwrap_or(&state.project_root)
        .to_path_buf();
    let canary_capture_dir = state.project_root.join(".devflow");
    // D-07 (round-3 B2): the launcher is selected BY AGENT. The widened
    // predicate arms the canary gate for Antigravity runs, and a hardcoded
    // ClaudeCanaryLauncher there would spend a Claude invocation per
    // Antigravity run — and refuse to launch when `claude` is absent or
    // unauthenticated.
    let canary_launcher = canary_launcher_for(state.agent, canary_workdir);
    canary_gate(state, stream_launch, move || {
        canary::run_delivery_canary(canary_launcher.as_ref(), &canary_capture_dir)
    })?;

    let (program, args, launch) = resolve_launch_shape(
        state.agent,
        driver.as_ref(),
        state.phase,
        prompt,
        &roots,
        stream_launch,
    );

    // 28-03 (D-03/D-04): every ORDINARY fresh stage launch starts the
    // checkpoint-resume budget over, including a human-approved gate retry
    // (which also routes through this function). Only `launch_stage_inner`
    // resets this counter, and only `relaunch_checkpoint_session` increments
    // it — that pairing is what makes `mode::MAX_CHECKPOINT_RESUMES` bound
    // one stage's resume attempts, not a phase's entire lifetime (the same
    // distinction `MAX_INFRA_FAILURES`'s doc comment draws for
    // `infra_failures`). Persisted below by `spawn_agent_and_record`'s own
    // `save_state` calls — no extra save needed here.
    state.checkpoint_resumes = 0;

    stamp_validate_dispatch_window(state);

    spawn_agent_and_record(
        state,
        program,
        &args,
        &driver.environment(),
        archived_stage,
        launch,
    )
}

/// Resolve a stage launch into `(program, argv, monitor arm)`.
///
/// Extracted from [`launch_stage_inner`] unchanged (31-04) so the shape a
/// launch resolves to is assertable without spawning a process. The body is the
/// pre-extraction `if/else if/else` verbatim; `stream_launch` is the caller's
/// already-computed [`stream_launch_enabled`] reading, threaded in
/// rather than recomputed so one predicate still governs the launch shape, the
/// canary gate, and the D-11 notice.
fn resolve_launch_shape(
    agent: AgentKind,
    driver: &dyn agents::AgentDriver,
    phase: PhaseId,
    prompt: String,
    roots: &[std::path::PathBuf],
    stream_launch: bool,
) -> (&'static str, Vec<String>, monitor::MonitorLaunch) {
    if stream_launch {
        let (program, args) = driver.build_command(phase, &prompt, roots);
        (program, args, monitor::MonitorLaunch::PipeOwning { prompt })
    } else if agent == AgentKind::Claude {
        // Claude on a stage the rollout has not reached, or a run that took
        // D-11's opt-out: the explicitly named pre-31 builder, NOT
        // `exec_command` — which now returns the stream-json shape for every
        // stage.
        let (program, args) = agents::ClaudeDriver::exec_command_single_document(&prompt);
        (program, args, monitor::MonitorLaunch::Legacy)
    } else {
        let (program, args) = driver.build_command(phase, &prompt, roots);
        (program, args, monitor::MonitorLaunch::Legacy)
    }
}

/// Resolve a checkpoint resume into `(program, argv)`.
///
/// Extracted from [`relaunch_checkpoint_session`] unchanged (47-03), for the
/// same reason 31-04 extracted [`resolve_launch_shape`]: so the instruction a
/// resume actually DELIVERS is assertable without spawning a process. The
/// instruction rides positionally in `argv[1]` (`ClaudeDriver::exec_resume_command`);
/// this path has no stdin turn.
///
/// It builds the instruction itself rather than taking it as a parameter, so a
/// test driving this function exercises the same prompt builder a real resume
/// delivers. The caller still builds its own copy for the
/// `checkpoint_auto_decided` event, which must be emitted BEFORE this call so a
/// spawn failure still leaves the attempt on record (47-CONTEXT.md D-10).
/// `checkpoint_auto_decide_prompt` is byte-deterministic, and the caller
/// asserts in debug builds that the two copies agree.
fn resume_launch_shape(phase: PhaseId, session_id: &str) -> (&'static str, Vec<String>) {
    let instruction = prompt::checkpoint_auto_decide_prompt(phase);
    agents::ClaudeDriver::exec_resume_command(session_id, &instruction)
}

/// Where a forced legacy launch's authorization came from, for the provenance
/// record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyLaunchSource {
    /// `DEVFLOW_CLAUDE_LEGACY_LAUNCH` is set in THIS process's environment.
    Environment,
    /// The persisted `state.legacy_claude_launch`, written at `start`/`resume`
    /// time by the `--legacy-claude-launch` flag or by the environment variable
    /// as it stood then.
    PersistedState,
}

impl LegacyLaunchSource {
    fn as_str(self) -> &'static str {
        match self {
            LegacyLaunchSource::Environment => "env:DEVFLOW_CLAUDE_LEGACY_LAUNCH",
            LegacyLaunchSource::PersistedState => "state:legacy_claude_launch",
        }
    }
}

/// Which source is reporting the opt-out *in this process*.
///
/// Deliberately re-derived at launch time rather than persisted as a second
/// field. The limit is worth stating: a stage launched from the DETACHED
/// monitor's `advance` tail does not inherit the operator's shell environment,
/// so it reports `PersistedState` even for a run originally authorized by the
/// environment variable. That is accurate for the process doing the reporting —
/// the persisted flag really is what it read — and the `start`-time notice
/// already named the environment as the origin.
fn legacy_launch_source() -> LegacyLaunchSource {
    if devflow_core::config::claude_legacy_launch() {
        LegacyLaunchSource::Environment
    } else {
        LegacyLaunchSource::PersistedState
    }
}

/// The D-11 notice, as one string, so its required content is assertable
/// without capturing stdout.
///
/// **The 999.64 sentence is required, not decorative (adversarial review B3).**
/// The opt-out was questioned as a silent way to disable the D-15 delivery
/// guard; it is not — `MonitorLaunch::Legacy` runs the child with stdin at
/// `/dev/null`, so the task-notification mechanism the canary tests
/// structurally does not exist there and running it would spend a real agent
/// invocation answering a question the launch never asks. What the operator
/// genuinely gives up is different and worse: the legacy path is *where a
/// multi-plan wave orphans delegated work* — that is 999.64 itself. Taking the
/// opt-out is an explicit acceptance of the limitation this phase exists to
/// remove, and it has to say so in plain words.
fn forced_legacy_launch_notice(stage: Stage, source: LegacyLaunchSource) -> String {
    format!(
        "legacy launch: DevFlow is forcing the pre-31 single-document Claude launch for \
         stage {stage} (source: {}). This path cannot deliver background-task \
         notifications, so a multi-plan wave may ORPHAN delegated work (999.64, unfixed \
         on this path). The stream-json transport, the pipe-owning monitor, the idle \
         timeout and the delivery canary are all inactive for this launch. Unset the \
         opt-out to return to the Phase 31 transport.",
        source.as_str()
    )
}

/// Announce a forced legacy launch on all three channels.
///
/// **Three, not fewer.** D-11's "logged loudly" has to survive an unattended
/// run where nobody is watching stdout: `println!` for an operator who is
/// present, the monitor log because the detached monitor's own stdio is null,
/// and `.devflow/events.jsonl` so the run's permanent record shows how often
/// the escape hatch is actually reached. An escape hatch used routinely erodes
/// what it protects, and only the ledger makes "routinely" visible.
fn announce_forced_legacy_launch(state: &State) {
    let source = legacy_launch_source();
    let notice = forced_legacy_launch_notice(state.stage, source);

    println!("warning: {notice}");
    append_monitor_log(
        &state.project_root,
        state.phase,
        &format!("[devflow] {notice}"),
    );

    events::emit(
        &state.project_root,
        state.phase,
        "claude_legacy_launch_forced",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "source": source.as_str(),
            "notice": truncate_reason(&notice),
        }),
    );
}

/// Append one line to the phase's monitor log, creating it if needed.
///
/// Best-effort: recording the notice must never abort the launch it describes.
/// A local five-line helper rather than a widened `devflow-core` API — the core
/// monitor has its own private equivalent for its own writes, and exporting it
/// for one CLI caller would grow the crate's public surface for no other gain.
fn append_monitor_log(project_root: &Path, phase: PhaseId, entry: &str) {
    use std::io::Write;
    let path = agent_result::monitor_log_path(project_root, phase);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{entry}");
    }
}

/// Combine D-11's two authorization sources into `state.legacy_claude_launch`.
///
/// **OR-only: this never clears a persisted opt-out (W5).** `devflow resume` is
/// the recovery verb for a rate-limited or infra-paused phase, so an
/// unconditional `state.legacy_claude_launch = flag || env` would silently flip
/// a run that deliberately chose the legacy path back onto the stream transport
/// mid-flight — the same silent-drop class as `stop_until`'s old unconditional
/// clear (999.60), and invisible in exactly the same way. To turn the opt-out
/// back off, edit `.devflow/state-NN.json` or start a new run.
///
/// Returns whether the environment ALONE supplied the authorization, so the
/// caller can print the "a persisted default is never a silent one" notice —
/// `--yes-ship`'s shape, for the same reason.
pub(crate) fn apply_legacy_launch_opt_out(state: &mut State, flag: bool) -> bool {
    let env = devflow_core::config::claude_legacy_launch();
    state.legacy_claude_launch = state.legacy_claude_launch || flag || env;
    env && !flag
}

/// Entry-point name recorded in the `auto_chain_flag_repaired` payload by
/// [`commands::start`](crate::commands::start).
pub(crate) const AUTO_CHAIN_REPAIR_FROM_START: &str = "start";
/// Entry-point name recorded by [`resume`].
pub(crate) const AUTO_CHAIN_REPAIR_FROM_RESUME: &str = "resume";

/// Repair a leaked `workflow._auto_chain_active` before this launch spawns
/// anything, and say so on both channels when it found one (35.1 D-01/D-03).
///
/// **One helper, two call sites**, for the reason
/// [`apply_legacy_launch_opt_out`] gives about its own pair: `start` and
/// `resume` must not be able to drift into repairing on different terms, and a
/// second copy of the emit/print block is exactly how that drift happens.
///
/// `35.1 D-01` — why the repair exists at all: the in-process `AutoChainGuard`
/// covers a normal return, a `?` early-return and a panic-unwind, and
/// structurally cannot cover a `SIGKILL`, because `Drop` never runs. Repairing
/// forward at both launch entry points is the second, independent mechanism.
/// `start`'s call is the one that catches a leak that already reached
/// `develop`, since a freshly forked worktree inherits whatever the base branch
/// carries; `resume`'s is the one that catches a leak a killed run left behind
/// in its own worktree.
///
/// `35.1 D-03` — why it is loud: a stale flag is EVIDENCE that a previous run
/// for this phase was killed before it could clean up. Repairing it quietly
/// would discard that signal. The notice therefore says what the stale value
/// means, not merely that a value changed.
///
/// **These are 35.1's own `D-` numbers**, a different decision namespace from
/// phase 31's `D-11` cited a few lines above in `commands.rs` — the two
/// sequences must not be read as one.
///
/// A clean launch — nothing repaired and nothing refused — makes no notice, no
/// event and no write. Loudness is about repairs, not about launches.
///
/// Never fails the launch. A config DevFlow cannot parse warns and proceeds
/// here; `35.1-03`'s preflight is the layer where an unlaunchable shape becomes
/// a refusal.
pub(crate) fn repair_leaked_auto_chain_flag(
    project_root: &Path,
    launch_root: &Path,
    phase: PhaseId,
    entry_point: &'static str,
) {
    let outcome = match gsd_config::force_clear_auto_chain(launch_root) {
        Ok(outcome) => outcome,
        Err(err) => {
            println!(
                "warning: could not certify phase {phase}'s GSD chain flag clear at {} \
                 ({err}) — launching without the repair",
                gsd_config::config_path(launch_root).display()
            );
            return;
        }
    };

    if !outcome.repaired_anything() && outcome.commit_refused.is_none() {
        return;
    }

    events::emit(
        project_root,
        phase,
        "auto_chain_flag_repaired",
        serde_json::json!({
            "entry_point": entry_point,
            "working_tree_repaired": outcome.working_tree_repaired,
            "committed_tree_repaired": outcome.committed_tree_repaired,
            "commit_refused": outcome.commit_refused,
        }),
    );

    if outcome.repaired_anything() {
        println!(
            "note: devflow {entry_point} found phase {phase}'s GSD chain flag \
             (workflow._auto_chain_active) still set — a previous run for this phase \
             was killed before it could clear it. Cleared before launching \
             (working tree: {}, this branch's tip: {})",
            repaired_word(outcome.working_tree_repaired),
            repaired_word(outcome.committed_tree_repaired),
        );
    }
    if let Some(reason) = &outcome.commit_refused {
        println!("warning: {reason}");
    }
}

fn repaired_word(repaired: bool) -> &'static str {
    if repaired {
        "repaired"
    } else {
        "already clear"
    }
}

/// The D-15 gate: run the delivery canary at most once per run, record what it
/// found, and refuse to launch when it did not confirm.
///
/// `run_canary` is a parameter rather than a direct call so the gate's own
/// wiring — once-per-run, stream-path-only, refuse-on-failure, persist, emit —
/// is testable without spending a real agent invocation per case. The
/// production call site binds it to [`canary::ClaudeCanaryLauncher`].
///
/// **Both alternatives to refusing were considered and rejected by D-15.**
/// Warning and proceeding fails in unattended mode, which is DevFlow's normal
/// mode — the warning scrolls past and the run orphans its work anyway. Falling
/// back to the sequential single-document path is a silent capability
/// downgrade, the exact invisible-degradation class this phase exists to
/// eliminate.
///
/// A recorded `Absent`/`Unverified` refuses on EVERY later launch in the run,
/// not just the one that discovered it; only the canary RUN is once-per-run.
/// Select the delivery-canary launcher for a launch (round-3 D-07/B2).
///
/// Claude -> [`canary::ClaudeCanaryLauncher`]; Antigravity ->
/// [`canary::AntigravityCanaryLauncher`] (agy-based, event-key turn,
/// agent-aware close rule). Every other agent keeps Claude's launcher because
/// the canary gate only ever fires for stream launches, and Claude is the
/// only non-Antigravity stream agent today — the fallback is the pre-widening
/// behaviour, not a new decision.
fn canary_launcher_for(
    agent: AgentKind,
    workdir: std::path::PathBuf,
) -> Box<dyn canary::CanaryLauncher> {
    match agent {
        AgentKind::Antigravity => Box::new(canary::AntigravityCanaryLauncher { workdir }),
        _ => Box::new(canary::ClaudeCanaryLauncher { workdir }),
    }
}

fn canary_gate<F>(state: &mut State, stream_launch: bool, run_canary: F) -> Result<(), CliError>
where
    F: FnOnce() -> canary::CanaryOutcome,
{
    // The guard protects one specific premise: that a live `stream-json`
    // session is woken back up when a background task finishes. A launch that
    // resolved to the legacy single-document path does not rely on that
    // premise, so checking it there would spend an agent invocation to answer a
    // question the launch never asks.
    if !stream_launch {
        return Ok(());
    }

    let outcome = match &state.canary {
        Some(recorded) => recorded.clone(),
        None => {
            let outcome = run_canary();
            // Persisted IMMEDIATELY, before the refusal below can return early
            // — the `session_id_from_capture` idiom. A refusal that did not
            // record what it found would re-run the guard on the next launch
            // and lose the evidence of what was verified when.
            state.canary = Some(outcome.clone());
            workflow::save_state(state)?;
            emit_canary_outcome(state, &outcome);
            outcome
        }
    };

    match outcome {
        canary::CanaryOutcome::Confirmed => Ok(()),
        canary::CanaryOutcome::Absent => refuse_launch(
            state,
            "background-task notification delivery is ABSENT: a token DevFlow planted in a \
             throwaway startup task did not come back inside a top-level `result` event.\n\
             \n\
             DevFlow's multi-plan wave guarantee is NOT currently backed by observed \
             behaviour. With delivery gone, a wave that dispatches several plans \
             concurrently silently orphans their work — refusing to launch rather than \
             discovering that after the fact.\n\
             \n\
             This is undocumented CLI behaviour, last observed on claude_code_version \
             2.1.220; a CLI update can withdraw it. The capture the guard read is at \
             `.devflow/delivery-canary.jsonl`."
                .to_string(),
        ),
        canary::CanaryOutcome::Unverified(reason) => refuse_launch(
            state,
            format!(
                "the delivery canary COULD NOT RUN, so background-task notification \
                 delivery is unverified for this run. This is not a report that the \
                 behaviour is gone — the guard reached no conclusion either way.\n\
                 \n\
                 Reason: {reason}\n\
                 \n\
                 Refusing to launch: the multi-plan wave guarantee depends on that \
                 behaviour and this run has no evidence about it."
            ),
        ),
    }
}

/// Abort a launch the canary refused, leaving no half-launched stage behind.
///
/// Clearing `supervisor` applies WR-04's rationale (see
/// [`record_failed_launch`]) to an aborted launch: `transition()` has already
/// advanced `state.stage` and saved it by the time this runs, so a refusal that
/// left the PREVIOUS stage's supervisor record standing would read as a run
/// gone while active and send the operator to `devflow resume`, after the
/// wrong problem entirely. With no record, the run reads not launched, and the
/// right remedy is the message below.
fn refuse_launch(state: &mut State, message: String) -> Result<(), CliError> {
    state.supervisor = None;
    workflow::save_state(state)?;
    Err(CliError::Message(message))
}

/// Record a canary outcome in the run's provenance (D-15).
///
/// The payload carries the token's PREFIX and the CLI version, never the token
/// itself (T-31-13): the token is a per-run nonce with no value once the run is
/// over, and this repository already has a tracked evidence-leak class from
/// committed capture files (ROADMAP §999.69) that a guard should not add to.
/// The version is context for a later forensic read — which CLI the behaviour
/// was or was not witnessed on — and never the guard itself, which is the
/// distinction D-13 turns on.
fn emit_canary_outcome(state: &State, outcome: &canary::CanaryOutcome) {
    // Agent-aware (41-02 review finding F4). The canary LAUNCHER and trust
    // predicate were made agent-aware, but this emission path still hardcoded
    // the Claude event names and `claude --version` — so an Antigravity run
    // recorded Claude provenance and spent a `claude --version` spawn. Branch
    // on the agent: Antigravity emits `antigravity_delivery_canary_*` and
    // records `agy --version`; every other agent keeps the Claude names.
    let (event, version) = match state.agent {
        AgentKind::Antigravity => (
            match outcome {
                canary::CanaryOutcome::Confirmed => "antigravity_delivery_canary_confirmed",
                canary::CanaryOutcome::Absent => "antigravity_delivery_canary_absent",
                canary::CanaryOutcome::Unverified(_) => "antigravity_delivery_canary_unverified",
            },
            canary::antigravity_cli_version(),
        ),
        _ => (
            match outcome {
                canary::CanaryOutcome::Confirmed => "claude_delivery_canary_confirmed",
                canary::CanaryOutcome::Absent => "claude_delivery_canary_absent",
                canary::CanaryOutcome::Unverified(_) => "claude_delivery_canary_unverified",
            },
            canary::claude_cli_version(),
        ),
    };
    let reason = match outcome {
        canary::CanaryOutcome::Unverified(reason) => Some(truncate_reason(reason)),
        _ => None,
    };
    events::emit(
        &state.project_root,
        state.phase,
        event,
        serde_json::json!({
            "stage": state.stage.to_string(),
            "token_prefix": canary::TOKEN_PREFIX,
            "cli_version": version,
            "reason": reason,
        }),
    );
}

/// The stages the Claude `stream-json` launch has been widened to.
///
/// `Stage::Code` first (D-10): it is where 999.64 was observed — Phase 29 wave
/// 2 dispatched two executors from Code and orphaned both — and it is the
/// stage that actually backgrounds work, so it is the only one that exercises
/// task-notification delivery and the drain gate at all. Define would have
/// been a proxy measurement.
///
/// **The launch argv is stage-blind, so a per-stage capture is evidence about
/// the AGENT and never about the transport** (ROADMAP criterion 1; 34-REVIEW.md
/// R-02). [`devflow_core::agents::ClaudeDriver::build_command`]
/// (`crates/devflow-core/src/agents/claude.rs`) ignores all three of its
/// `_phase`, `_prompt` and `_extra_writable_roots` arguments — verified by
/// reading the body, which returns a fixed `vec![...]`, not by the underscore
/// prefixes — and so returns a **byte-identical** argv for every stage.
/// Membership in this constant therefore selects exactly one thing:
/// [`resolve_launch_shape`]'s pipe-owning branch. Nothing else about the
/// transport varies per stage.
///
/// The consequence is what makes the capture campaign worth running. A capture
/// taken at Define and a capture taken at Validate differ only in how the agent
/// behaved under that stage's prompt — whether it backgrounded work, whether a
/// `background_tasks_changed` event appears, whether the stream drains. Any
/// difference between two stages' captures is a fact about agent behaviour. It
/// is never a fact about the transport, because the transport was the same
/// bytes both times. Reading a per-stage difference as a transport difference
/// would be a proxy measurement of exactly the kind D-10 rejected.
///
/// Element ORDER here is semantically inert: the constant is consulted with
/// `slice::contains`, so no launch behaviour depends on it. The list is written
/// in `Stage`-enum declaration order for readability only.
///
/// # Per-stage evidence (ROADMAP criterion 1, phase 34)
///
/// Every one of the five `Stage` variants is accounted for by name below —
/// each entry names the capture that authorised it, or, for a stage left off
/// the list, what was attempted and what specifically prevented the evidence.
/// All five are currently ON the list, so no "recorded reason for staying
/// narrow" entry is live; the format is retained so that removing a stage
/// requires writing one rather than deleting a line.
///
/// All five captures come from a SINGLE run — `devflow start --phase 1
/// --no-worktree --agent claude --mode auto` against a throwaway repo
/// scaffolded by `scripts/scratch-dogfood-repo.sh`, on `claude` 2.1.222, driven
/// by a binary whose digest was verified byte-identical to the build made from
/// this tree after the widening. Evidence directories are under
/// `.planning/phases/34-…/34-evidence/{stage}/`.
///
/// - **`Stage::Define`** — WIDENED on `34-evidence/define/`. 8 top-level
///   NDJSON events; `BackgroundTaskState::NeverAnnounced`, which
///   [`monitor::CloseRule::should_close`] treats as vacuously drained by
///   design. **Thin by construction:** the stage ran 1 turn in 2.3 s with no
///   tool use, because the scratch scaffold pre-writes the plan and
///   `/gsd-discuss-phase` had nothing to gather. It is evidence that Define
///   takes the stream path and does not announce background tasks *on a
///   workload with no work in it* — not that Define never backgrounds work.
/// - **`Stage::Plan`** — WIDENED on `34-evidence/plan/`. 11 events, 2 turns,
///   11.8 s, `NeverAnnounced`. Same thinness and the same cause: the agent
///   reported "The deliverable already exists … No work performed".
/// - **`Stage::Code`** — WIDENED on `34-evidence/code/`. The substantive
///   capture of the run: 455 events, 49 turns, 695 s, 67 Bash / 22 Read /
///   5 Write / 3 Edit and **3 `Agent` sub-agent dispatches**. `NeverAnnounced`
///   throughout — see the refutation recorded in `34-evidence/DRAIN-ANALYSIS.md`.
///   **This capture is NEW and does not supersede Phase 31's transcription.**
///   Phase 31's raw capture was deleted during cleanup and never committed, so
///   that stage survives only as transcription; this capture is a *fresh
///   capture*, taken against a scaffolded single-file probe phase. The two
///   differ in workload shape, tool-use volume and backgrounding pressure —
///   exactly the variables the drain question turns on — so Phase 31's
///   transcription remains the only production-phase evidence for Code.
/// - **`Stage::Validate`** — WIDENED on `34-evidence/validate/`. 126 events,
///   28 turns, 199 s, `NeverAnnounced`. Recorded observation, not diagnosed
///   here: the agent self-reported `PHASE 1 IS NYQUIST-COMPLIANT` and DevFlow
///   still classified the stage as a `loop_back` to Code. That is the
///   validate trust boundary this phase exists to tighten, and the capture is
///   filed as an observation of it rather than as a defect.
/// - **`Stage::Ship`** — WIDENED on `34-evidence/ship/`. 463 events, 31 turns,
///   516 s, `NeverAnnounced`, with 5 further `Agent` dispatches. The stage
///   launched and ran to a top-level `result` marker; its *work* stopped at
///   preflight because the scratch repo has no git remote. The capture is
///   evidence about the launch path, which is what membership here selects —
///   it is NOT evidence that a real Ship completes.
///
/// **What none of these establish (D-10, n=1).** Each capture shows the shape
/// occurred ONCE. None of them shows it is the stage's steady behaviour across
/// prompts, phase shapes or CLI versions — Phase 30 needed n=2–3 trials before
/// its drain measurements meant anything. A `NeverAnnounced` reading from a
/// 2.3-second no-op is the weakest form this evidence takes, and Define and
/// Plan are both that form.
///
/// **Criterion 7 — the D-15 canary refusal has MOVED, deliberately.** With
/// `Stage::Define` on the stream path, [`canary_gate`] now runs at Define
/// instead of Code. A run whose canary returns `Absent`/`Unverified` therefore
/// refuses at the FIRST stage, instead of completing Define and Plan on the
/// legacy path and only then refusing. This is a real change to unattended
/// behaviour, accepted rather than mitigated: D-15 rejected both alternatives
/// (warn-and-proceed fails unattended; falling back to the legacy path is a
/// silent capability downgrade). On the capture run the canary returned
/// `Confirmed` at Define, so the relocated refusal did not fire — the
/// relocation is recorded here on the strength of the code path, not on the
/// strength of having watched it refuse.
const STREAM_JSON_STAGES: &[Stage] = &[
    Stage::Define,
    Stage::Plan,
    Stage::Code,
    Stage::Validate,
    Stage::Ship,
];

/// Whether this launch should use the `stream-json` transport and the
/// pipe-owning monitor.
///
/// **Agent coverage (round-3 D-10):** Claude and Antigravity. The
/// `legacy_opt_out` term applies ONLY to Claude — `DEVFLOW_CLAUDE_LEGACY_LAUNCH`
/// is an escape hatch for Claude's pre-31 single-document launch, and
/// Antigravity has no single-document format, so the variable must never route
/// it to `MonitorLaunch::Legacy` (stdin would be `/dev/null` and the child
/// would silently fail; antigravity reviewer notice (b)). Antigravity
/// evaluates purely on `STREAM_JSON_STAGES` membership.
///
/// **This is a SEQUENCING choice, not a behaviour prediction.** Constraint 1
/// forbids deciding at launch time which stages will background work; it
/// permits rolling a change out one stage at a time. The reason for
/// sequencing at all is evidentiary (D-09): every gate fixture today is
/// labelled SYNTHETIC in-source and no archived capture contains a prompt
/// echo, so the stream parser's production correctness is currently
/// *reasoned, not witnessed*.
///
/// **What widens [`STREAM_JSON_STAGES`]:** a passing acceptance run (D-16/D-18
/// — a two-plan wave where both plans produce a `SUMMARY.md` and merge)
/// producing the first real production `stream-json` capture to verify the
/// parser against. Not a green unit suite, and not "the stage reported
/// Success" — the completion oracle already scored the orphaned Phase 29 stage
/// as Success.
///
/// **`legacy_opt_out` is D-11's escape hatch (31-04)**, and it is folded in
/// HERE rather than checked separately at each use so that ONE predicate still
/// governs the launch shape, the D-15 canary gate that protects it, and the
/// loud notice. Two separate notions of "is this the stream path?" would be
/// free to drift, and the drift would show up as a guard firing on a launch it
/// does not protect — or, worse, not firing on one it does.
///
/// Note what the opt-out does NOT reach: `relaunch_checkpoint_session`
/// hardcodes `MonitorLaunch::Legacy` and calls `spawn_agent_and_record`
/// directly, so it never consults this predicate at all. That is a
/// pre-existing, deliberate legacy route (see `MonitorLaunch::Legacy`'s own
/// doc), recorded rather than silently covered.
pub(crate) fn stream_launch_enabled(agent: AgentKind, stage: Stage, legacy_opt_out: bool) -> bool {
    matches!(agent, AgentKind::Claude | AgentKind::Antigravity)
        && STREAM_JSON_STAGES.contains(&stage)
        && !(agent == AgentKind::Claude && legacy_opt_out)
}

/// The stages whose launch may set GSD's `workflow._auto_chain_active` flag
/// (D-05, `35.1-CONTEXT.md`). One element, deliberately.
///
/// **[`STREAM_JSON_STAGES`] above is the shape precedent and an IMPERFECT
/// analogy.** That constant lists all five stages because its effect is purely
/// a DevFlow-internal transport choice with no upstream consequence: whichever
/// stage it names, the only thing that changes is how DevFlow talks to the
/// child. This flag is not like that. It has *different upstream effects per
/// stage*:
///
/// - At `Stage::Code` it is harmless. `execute-phase.md` reads the flag only to
///   decide whether to auto-approve an ordinary `gate="blocking"` checkpoint;
///   it never chains from it.
/// - At `Stage::Plan` it CHAINS. `plan-phase.md:1564` launches
///   `gsd-execute-phase` when the flag is set, which double-executes the Code
///   stage and misattributes its commits.
///
/// **So "completing" this list toward `Stage::Plan` is a defect, not a
/// finished job.** D-04 spent an adversarial-review round ruling exactly that
/// out, and ROADMAP criterion 3 forbids it. The flag GSD uses for "approve this
/// checkpoint" and the flag it uses for "chain to the next workflow step" are
/// the same boolean upstream; the fix is a gsd-core change splitting them in
/// two — tracked as **G-01** — not a widening of this list. If you arrived here
/// intending to add a stage, that ticket is what you actually want.
///
/// **Why one element and not a `(Stage, FixType)` table (F-6):** the
/// Validate→Code loop-back changes only the prompt text, at the same
/// `state.stage == Stage::Code`. `select_loop_back_fix` returns a `FixType` and
/// never touches `Stage`, and `Stage` has no gaps-only variant — so gaps-only
/// and full-execute are already covered by this single entry, and a keyed table
/// would encode a distinction the source does not have.
const AUTO_CHAIN_ELIGIBLE_STAGES: &[Stage] = &[Stage::Code];

/// Whether this launch may set GSD's chain flag.
///
/// `Mode::Auto` is half the predicate, and it is not incidental (F-1): the
/// flag's whole effect is to let the agent approve checkpoints with no human
/// present. Doing that on a run the operator explicitly chose to SUPERVISE
/// would be a silent behaviour change to an already-shipped mode that nobody
/// asked for. `preflight_interactivity_check` is the file's existing precedent
/// for gating on `state.mode == Mode::Auto` for exactly this reason.
fn auto_chain_flag_eligible(stage: Stage, mode: Mode) -> bool {
    mode == Mode::Auto && AUTO_CHAIN_ELIGIBLE_STAGES.contains(&stage)
}

/// Holds GSD's `workflow._auto_chain_active` at a chosen value for the
/// lifetime of a supervised child, and returns it to `false` on the way out.
///
/// **Symmetric by design (F-3).** [`Self::engage`] takes the value the launch
/// requires — `true` when eligible, `false` when not — so an INELIGIBLE launch
/// does not merely leave whatever it finds: it actively asserts `false`. That
/// is what closes the hole opened by sending the flag-preserving token on every
/// Code prompt, which disables GSD's own sync-clear safety net: a stale `true`
/// leaked by a previously-interrupted run is overwritten by DevFlow before the
/// agent starts, rather than left for GSD to notice.
///
/// The write is conditional on the value differing
/// ([`gsd_config::set_auto_chain_active`] reads before it writes), so the
/// ineligible path is a genuine no-op on an already-`false` file and does not
/// dirty a tracked file on every stage launch.
///
/// **Diverges from `LegacyEnvOverride` deliberately.** That guard restores a
/// caller-supplied PRIOR value, because the env var it owns may have had a
/// meaningful pre-existing state. This one does not restore anything: `false`
/// is always the correct value on exit (D-06). A run that ends with the flag
/// still set is the failure mode this type exists to prevent, and "put back
/// whatever was there" would reproduce it exactly whenever what was there was a
/// leaked `true`.
///
/// Errors are logged, never propagated. A hand-edited or absent
/// `.planning/config.json` must not abort a long unattended run — the guard
/// declines to engage and the stage proceeds without checkpoint auto-approval,
/// which is the conservative direction (T-35.1-03).
struct AutoChainGuard {
    config_root: PathBuf,
}

impl AutoChainGuard {
    fn engage(config_root: &Path, active: bool) -> Self {
        match gsd_config::set_auto_chain_active(config_root, active) {
            Ok(changed) => {
                if changed {
                    info!(
                        "GSD chain flag set to {active} for this stage at {}",
                        gsd_config::config_path(config_root).display()
                    );
                }
            }
            Err(err) => warn!(
                "could not set the GSD chain flag at {}: {err} — proceeding without \
                 checkpoint auto-approval",
                gsd_config::config_path(config_root).display()
            ),
        }
        Self {
            config_root: config_root.to_path_buf(),
        }
    }
}

impl Drop for AutoChainGuard {
    fn drop(&mut self) {
        if let Err(err) = gsd_config::set_auto_chain_active(&self.config_root, false) {
            warn!(
                "could not clear the GSD chain flag at {}: {err}",
                gsd_config::config_path(&self.config_root).display()
            );
        }
    }
}

/// The `__monitor` subcommand's `--launch` value: which supervision shape the
/// supervisor wires the agent's stdio for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum LaunchShape {
    /// Stdin `/dev/null`, stdout to the phase capture file
    /// (`MonitorLaunch::Legacy`).
    Capture,
    /// The supervisor owns both pipes and delivers the prompt as the first
    /// turn (`MonitorLaunch::PipeOwning`).
    Stream,
}

/// One `__monitor` invocation, as `main` (or the test build's harness entry)
/// parsed it from the argv `monitor::spawn_monitor` built.
#[derive(Debug, Clone)]
pub(crate) struct MonitorInvocation {
    /// Project root whose `.devflow/` files the supervisor writes.
    pub(crate) project_root: PathBuf,
    /// Phase whose stage machine to advance once the agent completes.
    pub(crate) phase: PhaseId,
    /// Stage the launch ran; passed to advance.
    pub(crate) stage: Option<Stage>,
    /// The agent's working directory.
    pub(crate) workdir: PathBuf,
    /// Which coding agent this is.
    pub(crate) agent: AgentKind,
    /// The supervision shape.
    pub(crate) launch: LaunchShape,
    /// Where the supervisor publishes its socket.
    pub(crate) socket: PathBuf,
    /// The per-launch id the launcher waits to see in a ping reply.
    pub(crate) launch_id: String,
    /// Adapter environment variable NAMES to re-apply to the agent from this
    /// process's environment (values never travel on argv, T-50-41).
    pub(crate) keep_env: Vec<String>,
    /// The stream shape's prompt file.
    pub(crate) prompt_file: Option<PathBuf>,
    /// The stream shape's idle timeout in seconds.
    pub(crate) idle_timeout_secs: Option<u64>,
    /// The agent's program and argv.
    pub(crate) argv: Vec<String>,
}

/// The stop reason a run ended through its supervisor records.
pub(crate) const SUPERVISOR_STOP_REASON: &str =
    "stopped through the supervisor (socket shutdown or SIGTERM/SIGINT)";

/// The stop reason an advance records when it stops at a safe point
/// (50-13, R-2).
pub(crate) const ADVANCE_STOP_REASON: &str = "stopped through the supervisor during advance";

/// The bound on the two waits a stop can put between two supervisors
/// (50-13, T-50-54): a stopped supervisor's wait for the per-phase lock
/// before it records its stop, and safe point 3's wait for an unpublished
/// next supervisor's final `stopped` line. `2 × TERMINATE_VERIFY_WAIT + 5 s`:
/// the next supervisor's kill grace plus margin.
const SUPERVISOR_STOP_WAIT: Duration =
    Duration::from_secs(2 * devflow_core::agent::TERMINATE_VERIFY_WAIT.as_secs() + 5);

/// What a supervisor's advance reads on this thread (50-13): its stop token,
/// for the advance's safe points, and the successor the advance recorded.
struct EngagedAdvance {
    token: StopToken,
    successor: Option<SupervisorRecord>,
}

thread_local! {
    /// Set only while [`StopTokenGuard`] is alive. Thread-local, not a
    /// process-global static, so parallel in-crate tests never share it; an
    /// advance run by anything other than a supervisor (the hidden
    /// `devflow advance`, a test calling `advance_with` directly) sees `None`
    /// and every safe point proceeds.
    static ENGAGED_ADVANCE: std::cell::RefCell<Option<EngagedAdvance>> =
        const { std::cell::RefCell::new(None) };
}

/// Makes a supervisor's stop token visible to the safe points of the
/// advance running on this thread, for as long as the guard lives (RAII,
/// the [`AutoChainGuard`] shape). Dropping it clears the token and the
/// successor slot.
struct StopTokenGuard {
    /// Keeps the guard on the thread whose slot it set.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl StopTokenGuard {
    fn engage(token: StopToken) -> Self {
        ENGAGED_ADVANCE.with(|slot| {
            *slot.borrow_mut() = Some(EngagedAdvance {
                token,
                successor: None,
            });
        });
        Self {
            _not_send: std::marker::PhantomData,
        }
    }
}

impl Drop for StopTokenGuard {
    fn drop(&mut self) {
        ENGAGED_ADVANCE.with(|slot| slot.borrow_mut().take());
    }
}

/// Run `f` on the engaged stop token, or return `None` when no supervisor's
/// advance is running on this thread.
fn with_stop_token<R>(f: impl FnOnce(&StopToken) -> R) -> Option<R> {
    let token = ENGAGED_ADVANCE.with(|slot| slot.borrow().as_ref().map(|e| e.token.clone()));
    token.map(|token| f(&token))
}

/// Remember `record` as the successor this advance published, so
/// [`run_monitor`] marks this supervisor handed off once the advance has
/// returned. A no-op when no supervisor's advance is engaged.
fn record_successor(record: SupervisorRecord) {
    ENGAGED_ADVANCE.with(|slot| {
        if let Some(engaged) = slot.borrow_mut().as_mut() {
            engaged.successor = Some(record);
        }
    });
}

/// Take the successor [`record_successor`] stored, if any.
fn take_successor() -> Option<SupervisorRecord> {
    ENGAGED_ADVANCE.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .and_then(|engaged| engaged.successor.take())
    })
}

/// An advance safe point (50-13, R-2): when the engaged supervisor's stop
/// has been requested, record the stop in `state` and return `true`, so the
/// caller returns at once without launching anything (the per-phase lock
/// guard drops on the way out, and the supervisor's `finish` answers a
/// parked caller `stopped`). `false` with no stop requested, or outside a
/// supervisor's advance.
fn stopped_at_safe_point(state: &mut State, safe_point: u8) -> Result<bool, CliError> {
    if with_stop_token(StopToken::safe_point) != Some(SafePoint::StopHere) {
        return Ok(false);
    }
    record_advance_stop(state, safe_point)?;
    Ok(true)
}

/// The test half of an advance safe point, for a caller that must clear
/// something before it calls [`record_advance_stop`]: `true` when the
/// engaged supervisor's stop has been requested (the run is then marked
/// ended as a stop), `false` with none or outside a supervisor's advance.
pub(crate) fn stop_requested_at_safe_point() -> bool {
    with_stop_token(StopToken::safe_point) == Some(SafePoint::StopHere)
}

/// Persist a stop an advance honoured at `safe_point`: `stopped` with
/// [`ADVANCE_STOP_REASON`], and an `advance_stopped` event naming the point.
pub(crate) fn record_advance_stop(state: &mut State, safe_point: u8) -> Result<(), CliError> {
    state.stopped = true;
    state.stop_reason = Some(ADVANCE_STOP_REASON.to_string());
    workflow::save_state(state)?;
    events::emit(
        &state.project_root,
        state.phase,
        "advance_stopped",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "safe_point": safe_point,
        }),
    );
    info!(
        "phase {}: {ADVANCE_STOP_REASON} (safe point {safe_point})",
        state.phase
    );
    Ok(())
}

/// Safe point 3's shutdown of a next supervisor that was spawned and
/// answered its ping but is not published: ask it to stop through its own
/// socket, bounded by [`SUPERVISOR_STOP_WAIT`]. It records nothing itself
/// (the state does not name it), so this launcher's stop is the record.
fn shut_down_unpublished(record: &SupervisorRecord) {
    match devflow_core::supervisor::request_shutdown(&record.socket_path, SUPERVISOR_STOP_WAIT) {
        Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. }) => info!(
            "stopped the unpublished next supervisor at {}",
            record.socket_path.display()
        ),
        Ok(other) => warn!(
            "the unpublished next supervisor at {} answered {other:?} to shutdown",
            record.socket_path.display()
        ),
        Err(err) => warn!(
            "could not stop the unpublished next supervisor at {}: {err}",
            record.socket_path.display()
        ),
    }
}

/// How an advance's gate wait ended (safe point 4).
#[derive(Debug)]
pub(crate) enum GateWaitEnd {
    /// The gate was answered.
    Answered(devflow_core::gates::GateResponse),
    /// No answer arrived within the timeout.
    TimedOut,
    /// The supervisor running this advance was stopped during the wait.
    Stopped,
}

/// An advance's gate wait (50-13, R-2, safe point 4). Inside a supervisor's
/// advance the run is `GateWait` for the wait and `Advancing` again after
/// it, and the poll is cancelled by the supervisor's stop token, so a
/// SIGTERM/SIGINT or socket stop ends the wait within about a second as
/// [`GateWaitEnd::Stopped`]. Anywhere else (the hidden `devflow advance`, a
/// foreground gate) it is `Gates::poll_response`, unchanged.
pub(crate) fn wait_for_gate_response(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    timeout_secs: u64,
) -> GateWaitEnd {
    let Some(token) = with_stop_token(StopToken::clone) else {
        return Gates::poll_response(project_root, phase, stage, timeout_secs)
            .map_or(GateWaitEnd::TimedOut, GateWaitEnd::Answered);
    };
    token.enter(OwnerPhase::GateWait);
    let polled = Gates::poll_response_cancellable(project_root, phase, stage, timeout_secs, || {
        token.stop_requested()
    });
    token.enter(OwnerPhase::Advancing);
    match polled {
        Some(response) => GateWaitEnd::Answered(response),
        None if token.safe_point() == SafePoint::StopHere => GateWaitEnd::Stopped,
        None => GateWaitEnd::TimedOut,
    }
}

/// How long a [`test_hold`] waits for its release file.
#[cfg(test)]
const TEST_HOLD_WAIT: Duration = Duration::from_secs(30);

/// Test-only hold point (50-13): inside a supervisor's own advance (a stop
/// token engaged), when `DEVFLOW_TEST_HOLD` is `<point>:<path>` for this
/// `point`, write `<path>.held` and wait (bounded by [`TEST_HOLD_WAIT`]) until
/// `<path>` exists, so a test can land a stop, a signal or a kill in a chosen
/// window. A launch made outside an advance (the test's own first launch)
/// never holds. Compiled out of non-test builds, together with every call.
#[cfg(test)]
pub(crate) fn test_hold(point: &str) {
    if with_stop_token(|_| ()).is_none() {
        return;
    }
    let Ok(hold) = std::env::var("DEVFLOW_TEST_HOLD") else {
        return;
    };
    let Some((at, path)) = hold.split_once(':') else {
        return;
    };
    if at != point {
        return;
    }
    let _ = std::fs::write(format!("{path}.held"), "");
    let release = Path::new(path);
    let deadline = std::time::Instant::now() + TEST_HOLD_WAIT;
    while !release.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The `__monitor` process body (50-11): supervise the stage's agent under
/// an addressed supervisor (`monitor::run_supervised_stage`), then, only
/// when the agent completed on its own, advance the stage machine
/// in-process (D-02). A stopped run is recorded as stopped and never
/// advances.
///
/// The handoff (50-13, R-1): the advance runs with this supervisor's stop
/// token engaged for its safe points and, when it launches the next stage,
/// records the next supervisor under the per-phase lock before it returns.
/// Only after it has returned (the lock released) is this supervisor marked
/// handed off, so every stop that reaches it during the advance is a
/// flagged `Advancing` stop (RV-04), and only then does `finish` unlink its
/// socket: the state never names a socket that is gone.
///
/// `advance` is `true` on every production path; the test build's
/// supervise-only monitor harness entry (50-10) passes `false` so an
/// in-crate launch supervises its agent without moving the stage machine on.
///
/// Runs in the detached `__monitor` process, never in the operator's CLI.
pub(crate) fn run_monitor(invocation: &MonitorInvocation, advance: bool) -> Result<(), CliError> {
    let MonitorInvocation {
        project_root,
        phase,
        stage,
        workdir,
        launch,
        ..
    } = invocation;
    let (project_root, phase, stage) = (project_root.as_path(), *phase, *stage);
    let spec = stage_spec(invocation)?;

    // D-01/D-06: hold GSD's `workflow._auto_chain_active` at the value this
    // launch requires for as long as the agent runs, and return it to
    // `false` before anything else happens: on the `?` below when the
    // supervisor fails, and right after the agent's run otherwise.
    //
    // The target is `workdir`, not `project_root`: `.planning/config.json` is
    // a tracked file inside the worktree the agent's cwd is set to, and that
    // copy is the one GSD's `check auto-mode` reads.
    //
    // Conditioned on the launch shape explicitly: since 50-11 both shapes
    // reach this function, and only a stream launch can bound the flag's
    // lifetime to the agent's (35.1-03's preflight refuses the capture shape
    // in `Mode::Auto` for exactly that reason). A capture launch never gets
    // the flag.
    //
    // The guard is dropped BEFORE the advance, not after it: the advance
    // launches the next stage and waits for that stage's supervisor to
    // publish, and that supervisor has already set the flag for its own
    // agent. Clearing it after the advance would clear the next stage's
    // value.
    //
    // A state that will not load is NOT fatal here: warn, skip the guard, and
    // let `advance` surface the real state error afterwards with its own
    // context.
    let auto_chain_guard = match launch {
        LaunchShape::Capture => None,
        LaunchShape::Stream => match workflow::load_state(project_root, phase) {
            Ok(state) => Some(AutoChainGuard::engage(
                workdir,
                auto_chain_flag_eligible(state.stage, state.mode),
            )),
            Err(err) => {
                warn!(
                    "monitor could not load state for phase {phase} ({err}) — running \
                     without the GSD chain-flag guard"
                );
                None
            }
        },
    };

    let run = monitor::run_supervised_stage(spec)
        .map_err(|err| CliError::Message(format!("stage supervisor failed: {err}")))?;
    drop(auto_chain_guard);

    let result = match run.outcome {
        AgentOutcome::Completed(_) if advance => {
            let stop_guard = StopTokenGuard::engage(run.supervisor.token());
            let advanced = advance_for_stage(project_root, Some(phase), stage);
            // The advance has returned and released the per-phase lock. Only
            // a run whose advance published a successor is handed off.
            if let Some(successor) = take_successor() {
                let token = run.supervisor.token();
                #[cfg(test)]
                test_hold("before_handoff_mark");
                token.enter(OwnerPhase::HandedOff);
                // RV-04 / R5-I1 (50-23): a stop that reached this supervisor
                // after safe point 3 was flagged (it stayed `Advancing`) and
                // must not be dropped. The one owner mutex orders every stop
                // request before the marking above (flagged, so seen here) or
                // after it (answered `handed_off`, acting on nothing), so one
                // check right after the marking drops no flagged stop.
                if token.stop_requested() {
                    forward_stop_to_successor(&token, project_root, phase, &successor);
                }
            }
            drop(stop_guard);
            advanced
        }
        AgentOutcome::Completed(_) => {
            wait_for_launch_record(project_root, phase, &invocation.launch_id);
            Ok(())
        }
        _ => {
            record_supervisor_stop(project_root, phase, &invocation.launch_id);
            Ok(())
        }
    };
    // Never a `stopped` line for a completed run: `finish` decides the final
    // line from the recorded outcome, so a shutdown that arrived while
    // advancing is answered `handed_off`.
    run.supervisor.finish();
    result
}

/// Forward a stop that reached this supervisor after safe point 3 to the
/// successor its advance recorded (50-23, RV-04), through the successor's
/// socket, bounded by [`SUPERVISOR_STOP_WAIT`]. Writes no state: the
/// successor is the recorded supervisor and records its own stop.
///
/// A confirmed `Stopped` takes a safe point on `token`, which returns
/// `StopHere`, so `finish` answers a parked shutdown caller `stopped` (the
/// stop did happen, through the successor) and that caller does not also
/// retry against the successor. Anything else leaves `finish` answering
/// `handed_off`, so a socket caller retries against the state's record, and
/// names the `devflow stop --phase N --launch <id>` repair. Both outcomes
/// are recorded as a `stop_forwarded` event, because this process's stderr
/// is `/dev/null` in production. The stop's source is not consulted: the
/// token does not keep it (T-50-75).
fn forward_stop_to_successor(
    token: &StopToken,
    project_root: &Path,
    phase: PhaseId,
    successor: &SupervisorRecord,
) {
    let launch_id = &successor.launch_id;
    let outcome =
        devflow_core::supervisor::request_shutdown(&successor.socket_path, SUPERVISOR_STOP_WAIT);
    let confirmed = matches!(
        outcome,
        Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
    );
    let repair = format!("devflow stop --phase {phase} --launch {launch_id}");
    let detail = match &outcome {
        Ok(outcome) => format!("{outcome:?}"),
        Err(err) => err.to_string(),
    };
    if confirmed {
        let _ = token.safe_point();
        eprintln!("forwarded a stop received after safe point 3 to launch {launch_id}: stopped");
    } else {
        eprintln!(
            "a stop reached this supervisor after safe point 3 and forwarding it to launch \
             {launch_id} did not confirm a stop ({detail}); run `{repair}`"
        );
    }
    events::emit(
        project_root,
        phase,
        "stop_forwarded",
        serde_json::json!({
            "launch_id": launch_id,
            "confirmed": confirmed,
            "outcome": detail,
            "repair": (!confirmed).then_some(repair),
        }),
    );
}

/// The [`monitor::StageSpec`] for `invocation`: the stream shape reads its
/// prompt file, and each `--keep-env` name takes its value from this
/// process's environment.
fn stage_spec(invocation: &MonitorInvocation) -> Result<monitor::StageSpec, CliError> {
    let Some((program, args)) = invocation.argv.split_first() else {
        return Err(CliError::Message(
            "monitor was given no child program to supervise".to_string(),
        ));
    };
    let io = match invocation.launch {
        LaunchShape::Capture => monitor::StageIo::Capture,
        LaunchShape::Stream => {
            let (Some(prompt_file), Some(idle_secs)) =
                (&invocation.prompt_file, invocation.idle_timeout_secs)
            else {
                return Err(CliError::Message(
                    "a stream launch needs --prompt-file and --idle-timeout-secs".to_string(),
                ));
            };
            let prompt = std::fs::read_to_string(prompt_file).map_err(|err| {
                CliError::Message(format!(
                    "monitor could not read the prompt file {}: {err}",
                    prompt_file.display()
                ))
            })?;
            monitor::StageIo::Stream {
                prompt,
                idle_timeout: Duration::from_secs(idle_secs),
            }
        }
    };
    let socket = devflow_core::supervisor::SocketPath::new(invocation.socket.clone())
        .map_err(|err| CliError::Message(format!("monitor socket path refused: {err}")))?;
    let mut spec = monitor::StageSpec::new(
        invocation.project_root.clone(),
        invocation.phase,
        invocation.agent,
        program.clone(),
        io,
        socket,
        invocation.launch_id.clone(),
    );
    spec.stage = invocation.stage;
    spec.workdir = invocation.workdir.clone();
    spec.args = args.to_vec();
    spec.envs = kept_env(&invocation.keep_env);
    Ok(spec)
}

/// Each named variable with its value from this process's environment,
/// where `spawn_monitor` put it. A name that is unset or not UTF-8 is
/// skipped with a warning.
fn kept_env(names: &[String]) -> Vec<(String, String)> {
    names
        .iter()
        .filter_map(
            |name| match std::env::var_os(name).map(|value| value.into_string()) {
                Some(Ok(value)) => Some((name.clone(), value)),
                _ => {
                    warn!(
                        "--keep-env {name}: not set to a UTF-8 value in the monitor's environment"
                    );
                    None
                }
            },
        )
        .collect()
}

/// Record a run its supervisor stopped: `state.stopped = true` with
/// [`SUPERVISOR_STOP_REASON`], under the per-phase lock. Never advances.
///
/// Only when the state names THIS launch (50-13, T-50-54): checked without
/// the lock first, then again under it. A next supervisor that was never
/// published (safe point 3 stops it while its launcher holds the lock and
/// records the stop itself) and a superseded supervisor (the state names its
/// successor, whose run it must not mark stopped) both write nothing, so
/// neither waits on a lock its stopper holds.
///
/// Never fails (T-50-68): a state that will not load, a lock wait that
/// times out or a save that fails is logged, naming the state path and why
/// `stopped` was not recorded, and the caller still reaches `finish`, which
/// answers a parked stop caller `stopped`.
fn record_supervisor_stop(project_root: &Path, phase: PhaseId, launch_id: &str) {
    let path = workflow::state_path(project_root, phase);
    let names_this_launch = |state: &State| {
        state
            .supervisor
            .as_ref()
            .is_some_and(|record| record.launch_id == launch_id)
    };
    let not_recorded = |why: &str| {
        warn!(
            "phase {phase}: `stopped` was not recorded in {}: {why}",
            path.display()
        );
    };
    match workflow::load_state(project_root, phase) {
        Err(err) => return not_recorded(&format!("the state does not load ({err})")),
        Ok(state) if !names_this_launch(&state) => {
            info!(
                "phase {phase}: the state does not name launch {launch_id}; its stop is \
                 not this supervisor's to record"
            );
            return;
        }
        Ok(_) => {}
    }
    let _lock = match lock::acquire_blocking(project_root, phase, SUPERVISOR_STOP_WAIT) {
        Ok(guard) => guard,
        Err(err) => return not_recorded(&format!("the per-phase lock was not acquired ({err})")),
    };
    let mut state = match workflow::load_state(project_root, phase) {
        Ok(state) if names_this_launch(&state) => state,
        Ok(_) => {
            info!("phase {phase}: launch {launch_id} was superseded before its stop was recorded");
            return;
        }
        Err(err) => return not_recorded(&format!("the state does not load ({err})")),
    };
    state.stopped = true;
    state.stop_reason = Some(SUPERVISOR_STOP_REASON.to_string());
    if let Err(err) = workflow::save_state(&state) {
        return not_recorded(&format!("the save failed ({err})"));
    }
    info!("phase {phase}: {SUPERVISOR_STOP_REASON}");
}

/// How long the supervise-only entry waits for its launcher's record.
const LAUNCH_RECORD_WAIT: Duration = Duration::from_secs(15);

/// The supervise-only path (test build): keep serving until the launcher has
/// recorded THIS launch in the state file (bounded), so a launcher still
/// waiting for the publish cannot miss a socket that finished first. A
/// production supervisor gets the same ordering from `advance`, which takes
/// the per-phase lock its launcher holds until after the publish.
fn wait_for_launch_record(project_root: &Path, phase: PhaseId, launch_id: &str) {
    let deadline = std::time::Instant::now() + LAUNCH_RECORD_WAIT;
    while std::time::Instant::now() < deadline {
        let recorded = workflow::load_state(project_root, phase)
            .ok()
            .and_then(|state| state.supervisor)
            .is_some_and(|record| record.launch_id == launch_id);
        if recorded {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    warn!("launch {launch_id} was not recorded within {LAUNCH_RECORD_WAIT:?}");
}

/// Resolve, once per launch, the cache directory and the monitor executable
/// a stage launch uses (50-10).
///
/// The released binary always re-execs itself as `__monitor` and registers
/// under [`devflow_core::registry::cache_dir`]. There is no environment
/// override of either: the test-build branch below is selected at compile
/// time.
#[cfg(not(test))]
pub(crate) fn launch_context() -> Result<monitor::LaunchContext, CliError> {
    let cache_dir = devflow_core::registry::cache_dir().ok_or_else(|| {
        CliError::Message(
            "cannot resolve DevFlow's cache directory for this launch: set HOME, \
             XDG_CACHE_HOME or DEVFLOW_CACHE_DIR"
                .to_string(),
        )
    })?;
    let exe = std::env::current_exe().map_err(|err| {
        CliError::Message(format!(
            "could not resolve the devflow executable for the stage monitor: {err}"
        ))
    })?;
    Ok(monitor::LaunchContext::new(
        cache_dir,
        monitor::MonitorExec::Subcommand { exe },
    ))
}

/// Selects the advance-running monitor harness entry for a `cfg(test)` stage
/// launch when set to `1` (50-10). Read only by the test build of
/// [`launch_context`].
#[cfg(test)]
pub(crate) const TEST_MONITOR_FULL_ENV: &str = "DEVFLOW_TEST_MONITOR_FULL";

/// The test build of [`launch_context`] (50-10, D-07).
///
/// Inside this crate's unit-test binary, `current_exe()` is libtest, which
/// rejects `__monitor`'s arguments, so the monitor is re-entered at an
/// `#[ignore]`d harness entry in `test_support` that runs the same
/// `run_monitor` body. The cache dir must come from the test process's own
/// `DEVFLOW_CACHE_DIR`, which only a child test process given one through
/// `run_test_in_child`'s `extra_env` has: an in-crate test that launches a
/// stage any other way fails here, loudly, instead of registering under the
/// developer's real HOME.
#[cfg(test)]
pub(crate) fn launch_context() -> Result<monitor::LaunchContext, CliError> {
    let cache_dir = std::env::var_os("DEVFLOW_CACHE_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::Message(
                "in-crate test launched a stage without a hermetic DEVFLOW_CACHE_DIR; \
                 run it through run_test_in_child with extra_env"
                    .to_string(),
            )
        })?;
    let exe = std::env::current_exe()
        .map_err(|err| CliError::Message(format!("could not resolve the test binary: {err}")))?;
    let test_name = if std::env::var_os(TEST_MONITOR_FULL_ENV).is_some_and(|value| value == "1") {
        "test_support::monitor_entry_full"
    } else {
        "test_support::monitor_entry_supervise_only"
    };
    Ok(monitor::LaunchContext::new(
        cache_dir,
        monitor::MonitorExec::HarnessEntry {
            exe,
            test_name: test_name.to_string(),
        },
    ))
}

/// End a launch that did not happen (50-23, T-50-62): clear
/// `state.supervisor` and save.
///
/// WR-04's rationale, moved from the monitor pid to the socket record:
/// `transition()` has already advanced and saved `state.stage` by the time a
/// launch runs, so a record still naming the previous stage's supervisor,
/// which is ending or gone, reads GONE-while-active and sends the operator to
/// `devflow resume`, the wrong remedy. With no record the run reads not
/// launched, and the launch's own error message is the remedy.
///
/// Called only on a failure path, never before the save that records a new
/// record (50-13's rule).
fn record_failed_launch(state: &mut State) -> Result<(), CliError> {
    state.supervisor = None;
    workflow::save_state(state)?;
    Ok(())
}

/// The tail of [`launch_stage_inner`]: validate the agent binary, archive
/// the prior capture, spawn the monitor, and record the launch. Extracted (28-03, Task 2) so
/// [`relaunch_checkpoint_session`] can share this EXACT tail for a
/// checkpoint auto-decide resume — a resume is a continuation of the same
/// stage's agent run, not a fresh stage entry, so it must not duplicate this
/// bookkeeping nor drift from it over time.
///
/// Emits no events beyond `capture_archived` and `stage_launched` — a
/// checkpoint resume is distinguished from an ordinary launch by its own
/// separate `checkpoint_auto_decided` event, emitted by the caller BEFORE
/// this function ever runs, not by mutating either event's shape here.
fn spawn_agent_and_record(
    state: &mut State,
    program: &str,
    args: &[String],
    extra_env: &[(String, String)],
    archived_stage: Option<Stage>,
    launch: monitor::MonitorLaunch,
) -> Result<(), CliError> {
    // Persist the caller's launch bookkeeping (`launch_stage_inner`'s
    // `checkpoint_resumes` reset and Validate dispatch stamp) before
    // anything is spawned, as this save always has. It once also cleared
    // the previous stage's monitor pid (WR-04, 18-fix); that field is gone
    // from the CLI (50-22), and the record keeps its own rule below.
    workflow::save_state(state)?;

    // WR-04 (18-fix, 50-23 T-50-62): every error before the publish save
    // ends the launch with no supervisor record (see `record_failed_launch`),
    // so a failed relaunch never leaves `state.stage` advanced beside a
    // record naming the previous stage's supervisor; the success path below
    // never clears the record before the save that replaces it.
    let SpawnedLaunch {
        published,
        ctx,
        launch_label,
    } = match spawn_next_supervisor(state, program, args, extra_env, archived_stage, launch) {
        Ok(Some(spawned)) => spawned,
        // A stop honoured at safe point 2 or 3 (50-13), already recorded.
        Ok(None) => return Ok(()),
        Err(err) => {
            return Err(match record_failed_launch(state) {
                Ok(()) => err,
                Err(clear_err) => CliError::Message(format!(
                    "{err}; clearing the supervisor record after it also failed: {clear_err}"
                )),
            });
        }
    };
    let pid = published.supervisor_pid;
    let socket_path = published.record.socket_path.display().to_string();
    let launch_id = published.record.launch_id.clone();
    // `transition()` calls `workflow::save_state` BEFORE `launch_stage`, so a
    // record held only in memory here is lost unless it is written again
    // (18b). The ping-confirmed supervisor record replaces any previous one
    // in this same save; it is never cleared first.
    //
    // The handoff (50-13, R-1): this save is the publish. The previous
    // supervisor, when this launch is its advance, stays alive and
    // `Advancing` through it (RV-04: never handed off here) and only
    // unlinks its own socket after the advance has returned, so a reader
    // between the two always finds a live address.
    #[cfg(test)]
    test_hold("before_record_save");
    let record = published.record.clone();
    match publish_record(state, published, workflow::save_state)? {
        PublishOutcome::HandedOff => record_successor(record),
        PublishOutcome::SuccessorAlreadyEnded => {}
    }
    #[cfg(test)]
    test_hold("after_publish");
    match devflow_core::ship::consume_cron_instructions(&state.project_root, state.phase) {
        Ok(Some(path_kind)) => {
            let path_kind = match path_kind {
                devflow_core::ship::CronInstructionPathKind::PerPhase => "per-phase",
                devflow_core::ship::CronInstructionPathKind::Legacy => "legacy",
                devflow_core::ship::CronInstructionPathKind::Both => "both",
            };
            events::emit(
                &state.project_root,
                state.phase,
                "cron_instructions_consumed",
                serde_json::json!({
                    "trigger": "resume_consumed",
                    "path_kind": path_kind,
                }),
            );
        }
        Ok(None) => {}
        Err(err) => warn!(phase = %state.phase, "could not consume cron instructions: {err}"),
    }
    // 23b: register this (project_root, phase) in the machine-global
    // registry on the same code path that just recorded the supervisor, so a
    // running phase cannot be missing from `devflow gate list --all-roots`.
    // Best-effort observability — never a reason to fail a launch.
    let _ = devflow_core::registry::register_in(&ctx.cache_dir, &state.project_root, state.phase);
    events::emit(
        &state.project_root,
        state.phase,
        "stage_launched",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "agent": state.agent.to_string(),
            "supervisor_pid": pid,
            "socket_path": socket_path,
            "launch_id": launch_id,
            "launch": launch_label,
        }),
    );
    println!(
        "stage {} → launched {} (supervisor pid {pid}, {launch_label} supervisor at {socket_path})",
        state.stage,
        agents::driver_for(state.agent).name()
    );
    Ok(())
}

/// The stop reason a launcher records when the next stage's supervisor it
/// just published had already ended (50-23, T-50-64).
pub(crate) const NEXT_STOPPED_BEFORE_SAVE_REASON: &str =
    "the next stage's supervisor was stopped before its record was saved";

/// How long [`successor_runs_on`] keeps re-probing a STALE successor: a
/// finishing supervisor drops its listener just before it unlinks.
const STALE_REPROBE_WINDOW: Duration = Duration::from_millis(200);

/// The interval of [`successor_runs_on`]'s STALE re-probe.
const STALE_REPROBE_INTERVAL: Duration = Duration::from_millis(20);

/// How [`publish_record`] ended.
enum PublishOutcome {
    /// The record is saved and the next supervisor is running it.
    HandedOff,
    /// The record is saved, but the next supervisor had already ended, so
    /// the run is recorded `stopped`.
    SuccessorAlreadyEnded,
}

/// Record the next stage's supervisor (`published`) in `state` with one
/// `save`, the publish (50-13, R-1 step 4; 50-23).
///
/// - The save fails: shut the new supervisor down through its socket
///   (bounded by [`SUPERVISOR_STOP_WAIT`]), so no live supervisor is left
///   unrecorded, and return the error ([`shut_down_unrecorded`]).
/// - The save succeeds: probe the new socket once ([`successor_runs_on`]).
///   A successor still running is handed off. One that already ended (a
///   stop sent straight to it takes 50-13's no-write path while the state
///   still names this launcher's run) is recorded here: `stopped` with
///   [`NEXT_STOPPED_BEFORE_SAVE_REASON`], saved again under the per-phase
///   lock the caller holds.
///
/// A successor that has accepted a stop but is still in its kill grace
/// answers owner `agent_running` and is handed off: this save lands before
/// that probe, and it loads the state only after its outcome is decided, so
/// it finds its own launch id and records its own stop.
///
/// Signals nothing by pid: every step goes through `published`'s socket.
fn publish_record<E: Into<CliError>>(
    state: &mut State,
    published: monitor::Published,
    mut save: impl FnMut(&State) -> Result<(), E>,
) -> Result<PublishOutcome, CliError> {
    let record = published.record;
    state.supervisor = Some(record.clone());
    if let Err(err) = save(state) {
        return Err(shut_down_unrecorded(state, &record, err.into()));
    }
    if successor_runs_on(&record.socket_path) {
        return Ok(PublishOutcome::HandedOff);
    }
    state.stopped = true;
    state.stop_reason = Some(NEXT_STOPPED_BEFORE_SAVE_REASON.to_string());
    save(state).map_err(Into::into)?;
    info!(
        "phase {}: {NEXT_STOPPED_BEFORE_SAVE_REASON} (launch {})",
        state.phase, record.launch_id
    );
    Ok(PublishOutcome::SuccessorAlreadyEnded)
}

/// After a failed publish save (`save_err`): ask the unrecorded supervisor
/// at `record` to stop, and return the launch's error.
///
/// A confirmed `Stopped` clears the record (best effort) and the error says
/// the supervisor was shut down. Anything else (RV-08: `HandedOff`,
/// `Refused`, a probe error or a timeout) means it may still be running:
/// nothing is cleared or saved, so the in-memory record keeps naming it and
/// no save erases the handle the file holds, and the error names its socket,
/// its launch id and the `devflow stop --phase N --launch <id>` repair, which
/// finds it by its sidecar whatever the state says (50-14).
fn shut_down_unrecorded(
    state: &mut State,
    record: &SupervisorRecord,
    save_err: CliError,
) -> CliError {
    let failed = format!(
        "could not save the record of the next stage's supervisor (launch {}): {save_err}",
        record.launch_id
    );
    match devflow_core::supervisor::request_shutdown(&record.socket_path, SUPERVISOR_STOP_WAIT) {
        Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. }) => {
            let cleared = record_failed_launch(state)
                .err()
                .map(|err| format!("; clearing its record also failed: {err}"))
                .unwrap_or_default();
            CliError::Message(format!(
                "{failed}; that supervisor was shut down through its socket{cleared}"
            ))
        }
        other => {
            let outcome = match other {
                Ok(outcome) => format!("{outcome:?}"),
                Err(err) => err.to_string(),
            };
            CliError::Message(format!(
                "{failed}; shutting it down at {} did not confirm a stop ({outcome}), so it may \
                 still be running; run `devflow stop --phase {} --launch {}` to end it",
                record.socket_path.display(),
                state.phase,
                record.launch_id
            ))
        }
    }
}

/// Whether the successor whose record was just saved is still running and
/// can be handed off: Alive with owner `agent_running`, `advancing` or
/// `gate_wait`. Gone, or Alive with any other owner (it finished without
/// advancing), is `false`. STALE is re-probed for
/// [`STALE_REPROBE_WINDOW`]: Gone within it is `false`, and a socket still
/// STALE is `true` (a crashed successor stays recorded and reads STALE, a
/// real finding with its own repair). A probe error is `true`: the record
/// stays and `status` reports the error.
fn successor_runs_on(socket: &Path) -> bool {
    use devflow_core::supervisor::{Liveness, probe};
    let running = [
        OwnerPhase::AgentRunning,
        OwnerPhase::Advancing,
        OwnerPhase::GateWait,
    ];
    let deadline = std::time::Instant::now() + STALE_REPROBE_WINDOW;
    loop {
        match probe(socket) {
            Ok(Liveness::Alive(reply)) => {
                return running.iter().any(|phase| phase.as_str() == reply.owner);
            }
            Ok(Liveness::Gone) => return false,
            Ok(Liveness::Stale) if std::time::Instant::now() < deadline => {
                std::thread::sleep(STALE_REPROBE_INTERVAL);
            }
            Ok(Liveness::Stale) => return true,
            Err(err) => {
                warn!(
                    "could not probe the next supervisor at {} after its publish: {err}",
                    socket.display()
                );
                return true;
            }
        }
    }
}

/// What [`spawn_next_supervisor`] hands back to [`spawn_agent_and_record`]
/// for the publish: the ping-confirmed supervisor, the launch context it
/// was spawned under, and the launch shape's label.
struct SpawnedLaunch {
    published: monitor::Published,
    ctx: monitor::LaunchContext,
    launch_label: &'static str,
}

/// The fallible steps of [`spawn_agent_and_record`] before the publish save:
/// validate the agent binary, archive the prior capture, resolve the launch
/// context, and spawn the next supervisor, with 50-13's safe points 2 and 3
/// around the spawn. `Ok(None)` when a stop was honoured at one of them (the
/// stop is already recorded). Every `Err` is a launch that did not happen,
/// and its caller clears the supervisor record ([`record_failed_launch`]).
fn spawn_next_supervisor(
    state: &mut State,
    program: &str,
    args: &[String],
    extra_env: &[(String, String)],
    archived_stage: Option<Stage>,
    launch: monitor::MonitorLaunch,
) -> Result<Option<SpawnedLaunch>, CliError> {
    ensure_agent_binary(program)?;

    // 17d (Task 2, D-17-D-19) originally placed the self-dogfood
    // build-staleness gate here, before spawn_monitor. 25b (D-03) moved it
    // out: it is now adjudicated exactly once, in `commands::start`, after
    // `state.worktree_path` is set and before this whole launch path is ever
    // entered — so a phase that modifies DevFlow's own source can progress
    // past every stage boundary instead of being re-blocked at each one. See
    // `commands::start` for the new call site and its accompanying D-04/D-05
    // trade-off notes.
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
    let ctx = launch_context()?;
    let launch_label = match &launch {
        monitor::MonitorLaunch::PipeOwning { .. } => "stream",
        monitor::MonitorLaunch::Legacy => "capture",
    };
    #[cfg(test)]
    test_hold("before_spawn");
    // Safe point 2 (50-13): a stop taken during the advance ends it before
    // the next stage's supervisor is spawned.
    if stopped_at_safe_point(state, 2)? {
        return Ok(None);
    }
    let published = monitor::spawn_monitor(state, program, args, extra_env, launch, &ctx)
        .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    #[cfg(test)]
    test_hold("before_publish");
    // Safe point 3 (50-13): the next supervisor answered its ping but is not
    // published. On a stop, shut it down through its socket and never save
    // its record; the state keeps this run's record, marked stopped.
    if with_stop_token(StopToken::safe_point) == Some(SafePoint::StopHere) {
        shut_down_unpublished(&published.record);
        record_advance_stop(state, 3)?;
        return Ok(None);
    }
    Ok(Some(SpawnedLaunch {
        published,
        ctx,
        launch_label,
    }))
}

/// Render the re-scan gate's context: which plan files carry a human-only
/// checkpoint DevFlow did not record at this phase's Code preflight.
///
/// Names the plan FILES rather than quoting the task bodies. A task element
/// can be arbitrarily long and the gate context is truncated for the notify
/// payload, so quoting bodies would push the actionable part — which file to
/// look at — past the cut.
fn rescan_gate_context(unapproved: &[&CheckpointDeclaration], mode: Mode) -> String {
    let mut plan_files: Vec<&str> = unapproved
        .iter()
        .map(|declaration| declaration.plan_file.as_str())
        .collect();
    plan_files.sort_unstable();
    plan_files.dedup();
    let next_step = match mode {
        Mode::Auto => {
            "approve, or switch this phase to Supervise for its remaining stages to repair, or abort"
        }
        Mode::Supervise => {
            "approve then make a separate human Code decision, repair under Supervise, or abort"
        }
    };
    format!(
        "[checkpoint re-scan] {} human-only checkpoint(s) in {} were added or changed after this \
         phase's Code preflight — a human must review them before continuing ({next_step})",
        unapproved.len(),
        plan_files.join(", ")
    )
}

/// A re-scan rejection in Auto mode is not an ordinary retry. The declaration
/// remains unapproved, so an unattended Code launch correctly refuses it;
/// routing through `GateAction::LoopBack` would open another unattended gate
/// with no answer. Persist the human-requested handoff before cleanup, so a
/// subsequent `resume` is supervised and cannot race a stale response file.
fn park_auto_rescan_repair(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    unapproved_files: &[String],
) -> Result<(), CliError> {
    debug_assert_eq!(state.mode, Mode::Auto);
    state.mode = Mode::Supervise;
    state.stopped = true;
    state.gate_pending = false;
    let reason = format!(
        "checkpoint re-scan rejected in Auto mode; phase switched to Supervise for its remaining \
         stages and parked for repair of {}",
        unapproved_files.join(", ")
    );
    state.stop_reason = Some(match state.stop_reason.take() {
        Some(existing) if !existing.is_empty() => format!("{existing}; {reason}"),
        _ => reason,
    });
    workflow::save_state(state)?;
    Gates::cleanup(project_root, state.phase, stage)?;
    events::emit(
        project_root,
        state.phase,
        "checkpoint_repair_parked",
        serde_json::json!({
            "stage": stage.to_string(),
            "from_mode": "auto",
            "to_mode": "supervise",
            "plan_files": unapproved_files,
        }),
    );
    println!(
        "checkpoint re-scan rejected in Auto mode; phase {} is now Supervise for its remaining \
         stages and parked for repair. Review the changed plan, then run `devflow resume --phase {}`.",
        state.phase, state.phase
    );
    Ok(())
}

/// The distinct plan files among `unapproved`, sorted — the actionable part of
/// both the gate context and the fall-through reason.
fn unapproved_plan_files(unapproved: &[&CheckpointDeclaration]) -> Vec<String> {
    let mut files: Vec<String> = unapproved
        .iter()
        .map(|declaration| declaration.plan_file.clone())
        .collect();
    files.sort_unstable();
    files.dedup();
    files
}

/// The `CheckpointApproval` that records `current`'s human-only declarations.
///
/// Shared by the re-scan gate's approval arm and `preflight`'s recording sites
/// so an approval and a preflight can never disagree about what "the set"
/// means — a divergence there would gate forever or approve too much, and
/// neither is visible from a passing test of either site alone.
pub(crate) fn recorded_approval(current: &[CheckpointDeclaration]) -> CheckpointApproval {
    CheckpointApproval::Recorded(
        current
            .iter()
            .filter(|declaration| declaration.blocking_human || declaration.human_action)
            .map(|declaration| ApprovedCheckpoint {
                plan_file: declaration.plan_file.clone(),
                element: declaration.element.clone(),
            })
            .collect(),
    )
}

/// Resume the exited Claude session that raised a confirmed human-blocking
/// checkpoint (D-03/D-04), continuing the SAME stage rather than launching a
/// fresh one.
///
/// D-04: resuming preserves the original session's conversation context and
/// completed-task history — a fresh stage spawn would re-read CONTEXT.md/
/// RESEARCH.md and re-run already-completed tasks, which a resume must
/// avoid. [`monitor::spawn_monitor`] always launches from
/// `state.worktree_path` (unchanged across relaunches within one phase run),
/// so the directory scope `--resume` requires needs no new plumbing here.
/// The monitor's own `devflow advance --phase N` tail is what re-enters the
/// loop for this same stage once the resumed session exits — a checkpoint
/// the agent resolves simply continues from wherever `advance()` picks up
/// next.
///
/// Does NOT call [`run_preflight`] and does NOT change `state.stage` — this
/// is a continuation of the current stage's agent run, not a new stage
/// entry.
pub(crate) fn relaunch_checkpoint_session(
    state: &mut State,
    session_id: &str,
) -> Result<(), CliError> {
    state.checkpoint_resumes = state.checkpoint_resumes.saturating_add(1);
    let instruction = prompt::checkpoint_auto_decide_prompt(state.phase);

    // D-07: recorded BEFORE the relaunch spawns, so a spawn failure still
    // leaves the decision on record — with no flag and no human in the loop
    // beforehand, this event is the ONLY way anyone learns after the fact
    // what the agent decided on its own.
    events::emit(
        &state.project_root,
        state.phase,
        "checkpoint_auto_decided",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "session_id": session_id,
            "instruction": truncate_reason(&instruction),
            "attempt": state.checkpoint_resumes,
            "policy": "D-03b: Auto-only checkpoint auto-decide",
        }),
    );

    // 47-03: built by `resume_launch_shape` so the delivered instruction is
    // assertable without a spawn. The event above quotes this function's own
    // `instruction`; the debug assertion pins that quote to what is delivered.
    let (program, args) = resume_launch_shape(state.phase, session_id);
    debug_assert_eq!(
        args[1], instruction,
        "the checkpoint_auto_decided event must quote the instruction the resume delivers"
    );

    // `Legacy`, deliberately: `exec_resume_command` builds the pre-31
    // single-document shape (positional instruction, `--output-format json`),
    // so its capture is a `SingleDocEnvelope` and there is no stdin turn to
    // deliver. Routing it through the pipe-owning arm would hand that
    // single-document child a stdin document it never reads.
    spawn_agent_and_record(
        state,
        program,
        &args,
        &[],
        None,
        monitor::MonitorLaunch::Legacy,
    )
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
pub(crate) fn launch_stage(
    state: &mut State,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<(), CliError> {
    let driver = agents::driver_for(state.agent);
    let prompt = prompt_override.clone().unwrap_or_else(|| {
        driver.render_prompt(&prompt::StageIntent::for_stage_in_project(
            state.stage,
            state.phase,
            Some(&state.project_root),
        ))
    });
    let roots = state
        .worktree_path
        .as_deref()
        .map(|wt| worktree_writable_roots(&state.project_root, wt))
        .unwrap_or_default();
    let (program, _args) = driver.build_command(state.phase, &prompt, &roots);
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
    if !run_preflight(&project_root, state, driver.as_ref())? {
        return Ok(());
    }

    launch_stage_inner(state, prompt_override, archived_stage)
}

/// Route an `Ambiguous` outcome from the PRIMARY advance() monitor loop
/// (A2, 41-antigravity UAT): the agent's own final message self-reported
/// success, but the CLI's result envelope was torn down by a transport-level
/// cancellation (`context canceled` / `context deadline exceeded`). The stage
/// is RE-DRIVEN — the same stage is relaunched with the same prompt — rather
/// than gated (the agent already succeeded) or advanced (a torn envelope is
/// not proof of a clean finish).
///
/// Bounded by the same shared `infra_failures` ceiling as
/// [`handle_rate_limited_outcome`] (D-08's intentional shared infra counter):
/// once bumping would reach the ceiling, the re-drive stops and the outcome
/// routes through the infra gate/abort path. Never touches
/// `consecutive_failures`, and never advances.
pub(crate) fn handle_ambiguous_outcome(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let projected_infra_failures = state.infra_failures.saturating_add(1);
    if projected_infra_failures >= mode::MAX_INFRA_FAILURES {
        return handle_infra_outcome(project_root, state, stage, reason);
    }
    state.infra_failures = projected_infra_failures;
    workflow::save_state(state)?;

    events::emit(
        project_root,
        state.phase,
        "ambiguous_transport_retry",
        serde_json::json!({
            "stage": stage.to_string(),
            "infra_failures": state.infra_failures,
            "reason": reason,
        }),
    );

    // Re-drive the SAME stage (never advance): `launch_stage` re-renders the
    // stage prompt, re-archives the prior (torn) capture, and spawns a fresh
    // monitor for `state.stage` — unchanged, since this outcome did not
    // advance. `Some(stage)` names the archived capture correctly.
    launch_stage(state, None, Some(stage))
}

/// Resume a rate-limited or infra-paused phase from its saved stage (review
/// consensus #5). Loads the persisted `.devflow/state-{NN}.json` and
/// relaunches its saved stage via [`launch_stage`] — unlike `start`, this
/// does NOT call `State::new`, `feature_start`, or `ensure_phase_worktree`:
/// the branch/worktree already exist and agent/mode are read from the saved
/// state, so neither needs to be passed as a flag and the workflow is never
/// reset to Define.
///
/// 20c (review: Codex MEDIUM — resume semantics): a phase halted by
/// `devflow start --until <stage>` persists `stopped`/`stop_reason`/
/// `stop_until`. Without clearing them here, `state.stop_until ==
/// Some(from)` would immediately re-stop the phase the next time
/// `transition()` ran, and the phase would remain marked `stopped` forever
/// even though the operator explicitly asked to resume past it. Cleared and
/// persisted BEFORE `launch_stage`, so a reload mid-relaunch already sees
/// the phase as no longer stopped.
///
/// D-15 (999.60): that clear is now gated on `state.stopped`. `resume` is
/// also the recovery verb for a rate-limited or infra-paused phase, and in
/// that case `stop_until` is a cap the operator set that has NOT fired —
/// `stopped` is the exact discriminator between "the cap fired and the
/// operator is overriding it" (clear it, per the 20c paragraph above) and
/// "the cap is still pending" (leave it alone, or the run silently sails
/// past a boundary the operator named). The save/relaunch ordering is
/// unchanged either way.
/// D-11 (31-04): `--legacy-claude-launch` is accepted here too, so an operator
/// can force the pre-31 path onto a run already in flight without restarting
/// it. The combination is OR-only — see [`apply_legacy_launch_opt_out`] for why
/// a plain `devflow resume` must not clear an opt-out the operator already
/// chose.
///
/// Criterion 2's 48-20 guard refuses to resume while the recorded monitor or
/// agent remains live. It runs under the phase lock before state is loaded or
/// written; see [`crate::commands::refuse_resume_over_a_live_run`].
pub(crate) fn resume(
    project_root: &Path,
    phase: PhaseId,
    agent: Option<AgentKind>,
    legacy_claude_launch: bool,
) -> Result<(), CliError> {
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "another devflow process (pid {pid}) is already running"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    crate::commands::refuse_resume_over_a_live_run(project_root, phase)?;
    let mut state = workflow::load_state(project_root, phase)?;
    // CR-03 (49-REVIEW.md): an exported (or emptied) notify command reaches
    // the chain this resume launches through the state file.
    state.gate_notify_cmd = devflow_core::gates::apply_notify_env(state.gate_notify_cmd.take());
    if let Some(requested) = agent
        && requested != state.agent
    {
        let from_agent = state.agent;
        let mut candidate = state.clone();
        candidate.agent = requested;
        // Runs the FULL generic preflight bundle (major-bump, unattended-
        // launch-shape, interactivity, gh-auth) against the candidate, not
        // just interactivity. `launch_stage` runs this same bundle again a
        // few lines below via `run_preflight`, and it is stricter than plain
        // interactivity: e.g. a Claude/Antigravity-only chain-flag guard
        // means a same-stage handoff to any other agent in Auto mode fails
        // the unattended-launch-shape check even though the target driver
        // declares the stage headless-safe. Checking only interactivity here
        // let such a handoff commit (`state.agent` mutated, `agent_handoff`
        // emitted) and then immediately hit that later gate — leaving the
        // phase "handed off" to a driver whose launch never actually ran
        // (44-CORE-REVIEW-FINDINGS.md finding 2b). Checking the full bundle
        // up front refuses it before anything is mutated, same as the
        // existing byte-identical-on-refusal guarantee below.
        generic_preflight_checks(project_root, &candidate).map_err(|reason| {
            CliError::Message(format!(
                "handoff to {requested} refused at saved {} stage: {reason}",
                state.stage
            ))
        })?;
        ensure_agent_binary(agent_program(requested))
            .map_err(|err| CliError::Message(format!("handoff to {requested} refused: {err}")))?;
        agents::driver_for(requested)
            .health(&candidate)
            .map_err(|err| CliError::Message(format!("handoff to {requested} refused: {err}")))?;

        state.agent = requested;
        workflow::save_state(&state)?;
        events::emit(
            project_root,
            phase,
            "agent_handoff",
            serde_json::json!({
                "stage": state.stage.to_string(),
                "from_agent": from_agent.to_string(),
                "to_agent": requested.to_string(),
                "reason": "resume --agent",
            }),
        );
        println!("handoff: {from_agent} → {requested}");
    }
    if state.stopped {
        state.stopped = false;
        state.stop_reason = None;
        state.stop_until = None;
    }
    // Combined BEFORE the save below, so the persisted value exists before the
    // detached monitor this relaunch spawns ever consults it.
    if apply_legacy_launch_opt_out(&mut state, legacy_claude_launch) {
        println!(
            "note: legacy Claude launch forced by DEVFLOW_CLAUDE_LEGACY_LAUNCH \
             (D-11, 31-CONTEXT.md) — a persisted default is never a silent one"
        );
    }
    // 35.1 D-01, `resume`'s half: this is the entry point that catches a leak a
    // SIGKILLed run left behind in its own worktree — the case the in-process
    // guard structurally cannot cover, because `Drop` never runs on a kill.
    // Placed after `load_state` (the state is what names the launch root) and
    // before the `save_state` below, so the repair is complete before anything
    // is spawned. Root resolved with the same
    // `worktree_path.unwrap_or(project_root)` idiom `spawn_monitor` uses for
    // `--workdir` (F-11) — a second spelling here would repair a different copy
    // of the file from the one the agent reads.
    let launch_root = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf());
    repair_leaked_auto_chain_flag(
        project_root,
        &launch_root,
        phase,
        AUTO_CHAIN_REPAIR_FROM_RESUME,
    );
    workflow::save_state(&state)?;
    match launch_stage(&mut state, None, None) {
        Ok(()) => Ok(()),
        Err(err) => {
            // `state.stopped` was just persisted as `false` above (needed so
            // a reload mid-relaunch already sees the phase as active — D-15/
            // 999.60, see the doc comment above). If `launch_stage` then
            // fails outright (as opposed to routing through its own gate,
            // which returns `Ok(())`), that save is now a lie: the state
            // file claims an active, running phase, but nothing launched.
            // `check_dead_agent`/`check_dead_monitor` both skip a phase
            // marked `stopped` (commands.rs), so this "zombie" combination
            // is exactly the one case those checks cannot flag
            // (44-CORE-REVIEW-FINDINGS.md finding 2a). Re-mark the phase
            // stopped, with a reason naming what failed, so it is visible
            // and actionable again rather than silently stalled. Best-effort
            // save — the original launch error is what the caller needs to
            // see either way.
            state.stopped = true;
            state.stop_reason = Some(format!("resume launch failed: {err}"));
            let _ = workflow::save_state(&state);
            Err(err)
        }
    }
}

/// The single active phase: `Ok(Some)` when exactly one is active, `Ok(None)`
/// when none, and an error naming the candidates when several are — shared by
/// `advance`'s legacy fallback and `logs`'s default-phase resolution so the
/// ambiguity rule and message live in one place.
pub(crate) fn single_active_phase(project_root: &Path) -> Result<Option<PhaseId>, CliError> {
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
pub(crate) fn resolve_sole_active_phase(project_root: &Path) -> Result<PhaseId, CliError> {
    single_active_phase(project_root)?
        .ok_or_else(|| CliError::Message("no active DevFlow state — nothing to advance".into()))
}

/// T-28-16 (28-03, Task 3): a CONFIRMED human-blocking checkpoint that could
/// not be auto-resolved (no session id on record, or the resume ceiling
/// already exhausted) must not read as a generic stage failure once it
/// falls through to the never-silent gate below — `why` names the exact
/// precondition that failed, appended to whatever reason the agent itself
/// reported (or standing alone, if the agent reported none).
fn augment_unresolved_checkpoint_reason(reason: Option<String>, why: &str) -> String {
    match reason {
        Some(r) if !r.is_empty() => {
            format!("{r} — confirmed checkpoint could not auto-resolve: {why}")
        }
        _ => format!("confirmed checkpoint could not auto-resolve: {why}"),
    }
}

/// Advance the stage machine after a monitored agent for `state.stage` exits.
/// Invoked by the monitor process; not normally run by a human.
#[cfg(test)]
pub(crate) fn advance(project_root: &Path, phase: Option<PhaseId>) -> Result<(), CliError> {
    advance_for_stage(project_root, phase, None)
}

pub(crate) fn advance_for_stage(
    project_root: &Path,
    phase: Option<PhaseId>,
    stage: Option<Stage>,
) -> Result<(), CliError> {
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
                    PhaseId::new(0),
                    "advance_failed",
                    serde_json::json!({ "reason": err.to_string() }),
                );
                return Err(err);
            }
        },
    };
    advance_with(project_root, phase, stage, ADVANCE_LOCK_WAIT)
}

pub(crate) fn advance_with(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
    lock_wait: Duration,
) -> Result<(), CliError> {
    // CR-03 (13-REVIEW.md): the lock is scoped per-phase, not per-project.
    // advance() holds it across a gate's multi-day blocking wait, and every
    // successful run ends at a mandatory Ship gate — a project-wide lock
    // would starve `devflow parallel`'s sibling phases with no retry.
    let _lock = match lock::acquire_blocking(project_root, phase, lock_wait) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            events::emit(
                project_root,
                phase,
                "advance_failed",
                serde_json::json!({
                    "reason": "lock wait expired",
                    "holder_pid": pid,
                    "waited_secs": lock_wait.as_secs_f64(),
                }),
            );
            return Err(CliError::Message(format!(
                "advance lock wait expired after {} seconds; holder pid {pid}",
                lock_wait.as_secs_f64()
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    // Load under the lock: with per-phase state files keyed by the same
    // phase as the lock, there is no cross-phase TOCTOU left by
    // construction — a concurrent advance of another phase touches a
    // different file. A duplicate advance of THIS phase queues behind the
    // lock, then stage binding below handles a launch that has moved on.
    let mut state = workflow::load_state(project_root, phase)?;
    // Safe point 1 (50-13): a stop taken while this supervisor waited for
    // the lock ends the advance before it evaluates anything.
    if stopped_at_safe_point(&mut state, 1)? {
        return Ok(());
    }
    #[cfg(test)]
    test_hold("after_safe_point_1");
    // 999.138 residual: a stopped run never advances. `resume` clears
    // `stopped` before it launches, so no caller advances a stopped state
    // legitimately.
    if state.stopped {
        events::emit(
            project_root,
            phase,
            "advance_failed",
            serde_json::json!({
                "reason": "stopped",
                "stop_reason": state.stop_reason,
            }),
        );
        return Err(CliError::Message(format!(
            "phase {phase} is stopped ({}); run `devflow resume --phase {phase}` to continue it",
            state.stop_reason.as_deref().unwrap_or("no reason recorded")
        )));
    }
    // CR-03 (49-REVIEW.md): a hand-run `advance` updates the run's notify
    // command; the detached chain carries no variable, so its `advance`
    // keeps the persisted one.
    state.gate_notify_cmd = devflow_core::gates::apply_notify_env(state.gate_notify_cmd.take());

    match stage {
        Some(expected) if expected != state.stage => {
            events::emit(
                project_root,
                phase,
                "advance_failed",
                serde_json::json!({
                    "reason": "stage mismatch",
                    "expected": expected.to_string(),
                    "actual": state.stage.to_string(),
                }),
            );
            return Err(CliError::Message(format!(
                "advance stage mismatch: expected {expected}, state is {}",
                state.stage
            )));
        }
        None => events::emit(
            project_root,
            phase,
            "advance_stage_unbound",
            serde_json::json!({ "stage": state.stage.to_string() }),
        ),
        Some(_) => {}
    }

    // Project-resolved (45-01): `evaluate_agent_result`'s Layer 2 counts
    // commits in a `{trunk}..{feature}` range, so a defaulted trunk
    // over- or under-reports the agent's work.
    //
    // CR-02: the run's persisted base wins over ambient configuration —
    // `advance` is a separate process from the `start` that resolved it, so
    // re-resolving here counts commits against the wrong trunk whenever the
    // environment differs.
    let git_flow =
        devflow_core::config::git_flow_for_run(project_root, state.base_branch.as_deref());
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

    // D-04 (28-03): record the session id for EVERY evaluated stage, not
    // only ones that turn out to be checkpoints — by the time any later
    // process (a checkpoint auto-decide relaunch) needs it, the value is
    // already durable. `session_id_from_capture` returns `None` for a
    // non-Claude agent's stdout (no `session_id` key at the JSON envelope's
    // top level) or a missing capture, so this is a safe no-op for every
    // non-Claude run.
    if let Some(session_id) = agent_result::session_id_from_capture(project_root, phase) {
        state.session_id = Some(session_id);
        workflow::save_state(&state)?;
    }

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
        Action::GateReview => {
            // D-01/D-03/D-05 (28-03): before the ordinary per-stage failure
            // dispatch, check whether this failure is actually a confirmed
            // human-blocking checkpoint DevFlow can resolve unattended in
            // Auto mode by resuming the exact session that raised it. Evaluated IN THIS
            // ORDER — load-bearing — (1) agent is Claude (D-05) and the stage
            // is Code, the one stage that runs plans and so the only one that
            // can stop at a plan's checkpoint (second external review round of
            // the CR-01 fix, 2026-10-10); (2) the
            // phase's plans statically declare a blocking-human checkpoint —
            // the PRIMARY, agent-uncontrollable gate, checked BEFORE
            // anything agent-controlled (T-28-01); (3) the agent reported
            // one, either by declaring `"checkpoint": "blocking-human"` on the
            // stage's final DEVFLOW_RESULT marker or by a `Gate:` declaration
            // line with no stated outcome after it — the latest statement wins
            // (operator decision 2026-10-10) — and never by the wording of a
            // failure reason (49-REVIEW CR-01); (4) a session id is on record; and (5) the
            // resume ceiling has not been exhausted. A changed declaration
            // always opens the re-scan gate. Only an unchanged, recorded set
            // in Auto resumes without a human; Supervise falls through to a
            // human gate. Any other false -> fall through to the unchanged
            // per-stage dispatch below.
            //
            // Steps (2) and (3) deliberately read DIFFERENT roots (999.76,
            // ROADMAP criterion 6). Step (2) reads the EXECUTION root:
            // `.planning/` is tracked content, so an in-flight phase's
            // `{N}-PLAN.md` lives on `feature/phase-{N}` INSIDE the worktree
            // and is absent from the main checkout for the phase's whole
            // duration — passing `project_root` here made this entire arm
            // silently dead in worktree mode, DevFlow's default operating
            // shape. Step (3) still reads `project_root`, because the stdout
            // capture lives under `.devflow/` in the project root;
            // retargeting it would break checkpoint detection in exactly the
            // mode this change repairs.
            let mut reason = result.reason.clone();
            let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
            let checkpoint_confirmed = state.agent == AgentKind::Claude
                && stage == Stage::Code
                && verify::phase_has_blocking_human_checkpoint(execution_root, phase)
                && (result.declares_blocking_human_checkpoint()
                    || agent_result::checkpoint_reported_in_capture(project_root, phase));
            if checkpoint_confirmed {
                let ceiling_ok = state.checkpoint_resumes < mode::MAX_CHECKPOINT_RESUMES;
                match (&state.session_id, ceiling_ok) {
                    (Some(session_id), true) => {
                        let session_id = session_id.clone();
                        // D-07 (CHKPT-02, backlog 999.126): this is the only
                        // route that arms the agent's own auto-decide, and the
                        // agent may have WRITTEN a plan file during the Code
                        // stage that just exited. Re-scan the execution root
                        // now — a fresh snapshot, deliberately re-read at
                        // decision time rather than carried from the check
                        // above — and compare it against the set a Code
                        // preflight recorded. Anything new or rewritten parks
                        // at a human gate instead of being auto-decided.
                        //
                        // Placed in THIS arm only. `(Some(_), false)` and
                        // `(None, _)` never arm auto-decide; they already fall
                        // through to the never-silent per-stage gate below, so
                        // a compare there would gate nothing that is not
                        // already gated (C-8).
                        let current = verify::phase_checkpoint_declarations(execution_root, phase);
                        let unapproved = state.checkpoint_approval.unapproved(&current);
                        if !unapproved.is_empty() {
                            let context = rescan_gate_context(&unapproved, state.mode);
                            // The unapproved plan files, captured BEFORE the
                            // gate: `unapproved` borrows `current`, and the
                            // LoopBack arm needs these names after `&mut state`
                            // has been handed to the gate.
                            let unapproved_files = unapproved_plan_files(&unapproved);

                            // `None` for the auto-response is load-bearing and
                            // not a placeholder: this gate exists precisely
                            // because a human has not seen these declarations,
                            // so nothing may pre-authorize it.
                            let action = run_gate_with_timeout(
                                project_root,
                                &mut state,
                                stage,
                                &context,
                                gate_timeout_secs(),
                                None,
                            )?;
                            match action {
                                // R-8 / T-48-15-01: approval must become
                                // DURABLE, or the gate reopens on every resume
                                // and an operator learns to answer it without
                                // reading. Record the set that was just SCANNED
                                // — not the stale one the gate was opened about
                                // — and persist it before relaunching, because
                                // the next resume runs in a separate process.
                                GateAction::Advance => {
                                    // CR-01: drop the gate/response/ack now, so
                                    // a later Code gate in this run cannot
                                    // silently consume this approval as its own
                                    // answer.
                                    let _ = Gates::cleanup(project_root, phase, stage);
                                    state.gate_pending = false;
                                    state.checkpoint_approval = recorded_approval(&current);
                                    workflow::save_state(&state)?;
                                    if state.mode == Mode::Auto {
                                        return relaunch_checkpoint_session(
                                            &mut state,
                                            &session_id,
                                        );
                                    }
                                    reason = Some(augment_unresolved_checkpoint_reason(
                                        reason,
                                        "Supervise mode requires a human decision for the declared blocking-human checkpoint",
                                    ));
                                }
                                // T-48-15-03: a rejection records NOTHING.
                                // Widening the set here would mean the next
                                // resume proceeded as though the human had said
                                // yes. In Auto, it also cannot use the generic
                                // loop-back: a second unattended Code preflight
                                // correctly refuses the still-unapproved human
                                // checkpoint. Park it for the explicitly
                                // requested supervised repair instead.
                                GateAction::LoopBack(_) => {
                                    if state.mode == Mode::Auto {
                                        return park_auto_rescan_repair(
                                            project_root,
                                            &mut state,
                                            stage,
                                            &unapproved_files,
                                        );
                                    }
                                    // Supervise may use the ordinary repair
                                    // loop: its next Code run is not
                                    // unattended, and the unchanged response
                                    // is the human's request to repair.
                                    reason = Some(augment_unresolved_checkpoint_reason(
                                        reason,
                                        &format!(
                                            "checkpoint added or changed after Code preflight in {}",
                                            unapproved_files.join(", ")
                                        ),
                                    ));
                                }
                                // The same abort every other gate uses, so
                                // cleanup, state clearing and deregistration
                                // all happen exactly once and identically.
                                GateAction::Abort(note) => {
                                    return abort(project_root, &state, &note);
                                }
                            }
                        } else if state.mode == Mode::Auto {
                            return relaunch_checkpoint_session(&mut state, &session_id);
                        } else {
                            reason = Some(augment_unresolved_checkpoint_reason(
                                reason,
                                "Supervise mode requires a human decision for the declared blocking-human checkpoint",
                            ));
                        }
                    }
                    (Some(_), false) => {
                        // T-28-16: a confirmed checkpoint that could not be
                        // auto-resolved must not read as a generic stage
                        // failure — name the exhausted precondition in the
                        // reason the never-silent gate below renders.
                        reason = Some(augment_unresolved_checkpoint_reason(
                            reason,
                            &format!(
                                "resume ceiling ({}) exhausted",
                                mode::MAX_CHECKPOINT_RESUMES
                            ),
                        ));
                    }
                    (None, _) => {
                        reason = Some(augment_unresolved_checkpoint_reason(
                            reason,
                            "no session id on record",
                        ));
                    }
                }
            }
            match stage {
                // Validate failures drive the Code↔Validate loop (or a gate).
                Stage::Validate => {
                    handle_validate_outcome(project_root, &mut state, ValidateOutcome::Failed)
                }
                // Ship distinguishes an agent crash (AgentFailed) from a
                // review rejection (ReviewFailed, `review:`-prefixed reason).
                Stage::Ship => handle_ship_failure(project_root, &mut state, reason),
                // Every other non-Validate failure (incl. Unknown, D-06) is
                // never silent (WR-11): it always fires a gate + notify
                // instead of returning a bare error or silently advancing.
                _ => handle_stage_failure(project_root, &mut state, stage, reason),
            }
        }
        // ResourceKilled/AgentUnavailable: a dedicated infra path, identical
        // for every stage (including Validate/Ship) — MUST NOT route through
        // handle_validate_outcome/handle_ship_failure, which would bump
        // consecutive_failures (review consensus #4, D-08).
        Action::GateInfra => handle_infra_outcome(project_root, &mut state, stage, result.reason),
        // RateLimited / Ambiguous: auto-resume via the primary loop (D-09 /
        // A2). RateLimited schedules a cron resume; Ambiguous (a transport
        // cancel whose own envelope still carried a success marker) re-drives
        // the SAME stage immediately. Both bounded by the shared
        // infra-failure ceiling (D-08).
        Action::AutoResume => match result.status {
            agent_result::AgentStatus::Ambiguous => {
                handle_ambiguous_outcome(project_root, &mut state, stage, result.reason)
            }
            _ => handle_rate_limited_outcome(project_root, &mut state, phase, stage, result.reason),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use devflow_core::gates::Gates;
    use devflow_core::mode::Mode;
    use devflow_core::state::AgentKind;

    macro_rules! enter_agent_free_child {
        ($name:expr, $program:literal) => {
            if !devflow_core::test_support::in_child_test($name) {
                let path_dir = agent_free_dir_with_agent_stub($program);
                let output =
                    crate::test_support::run_hermetic_test_in_child($name, path_dir.path(), &[]);
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, $name,
                );
                return;
            }
        };
    }

    /// D-04/D-05/F-1: the chain flag is engaged for `Stage::Code` under
    /// `Mode::Auto` and for nothing else.
    ///
    /// Every `Stage` variant is named EXPLICITLY rather than iterated over a
    /// slice. That is the point: adding a variant to `Stage` must become a
    /// compile error here — forcing whoever adds it to decide, on the record,
    /// whether the new stage may auto-approve checkpoints — instead of a
    /// silently-unexercised case that the iteration would have swallowed.
    #[test]
    fn auto_chain_eligibility_is_code_and_auto_mode_only() {
        // The one eligible combination.
        assert!(auto_chain_flag_eligible(Stage::Code, Mode::Auto));

        // Right stage, wrong mode. A run the operator chose to SUPERVISE must
        // never have its checkpoints auto-approved (F-1).
        assert!(!auto_chain_flag_eligible(Stage::Code, Mode::Supervise));

        // Every other stage, under the mode that would otherwise qualify.
        // `Stage::Plan` is the load-bearing one: the same flag makes
        // `plan-phase.md` chain into `execute-phase.md` (ROADMAP criterion 3).
        assert!(!auto_chain_flag_eligible(Stage::Define, Mode::Auto));
        assert!(!auto_chain_flag_eligible(Stage::Plan, Mode::Auto));
        assert!(!auto_chain_flag_eligible(Stage::Validate, Mode::Auto));
        assert!(!auto_chain_flag_eligible(Stage::Ship, Mode::Auto));

        // Exhaustiveness tripwire: this match names all five variants with no
        // wildcard arm, so a new `Stage` fails to compile here rather than
        // slipping past the five assertions above.
        for stage in [
            Stage::Define,
            Stage::Plan,
            Stage::Code,
            Stage::Validate,
            Stage::Ship,
        ] {
            let expected = match stage {
                Stage::Code => true,
                Stage::Define | Stage::Plan | Stage::Validate | Stage::Ship => false,
            };
            assert_eq!(auto_chain_flag_eligible(stage, Mode::Auto), expected);
        }
    }

    /// A real-shape `.planning/config.json` under `root`, committed, with the
    /// chain flag seeded either way.
    fn seed_gsd_config(root: &Path, active: bool) {
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(
            root.join(".planning/config.json"),
            format!(
                "{{\n  \"commit_docs\": true,\n  \"workflow\": {{\n    \
                 \"granularity\": \"medium\",\n    \"auto_advance\": true,\n    \
                 \"_auto_chain_active\": {active}\n  }}\n}}\n"
            ),
        )
        .unwrap();
        let git = |args: &[&str]| {
            let ok = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?} failed");
        };
        git(&["add", ".planning/config.json"]);
        git(&["commit", "-q", "-m", "add gsd config"]);
    }

    /// Every `auto_chain_flag_repaired` line in a project's event log.
    fn repair_events(root: &Path) -> Vec<serde_json::Value> {
        let path = devflow_core::events::events_path(root);
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        raw.lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|line| line["event"] == "auto_chain_flag_repaired")
            .collect()
    }

    /// D-03: a genuine repair announces itself in `.devflow/events.jsonl`, and
    /// the payload names WHICH entry point found the leak — without that, a
    /// reader cannot tell a leak inherited from `develop` by a fresh worktree
    /// (`start`) from one a killed run left in its own worktree (`resume`).
    #[test]
    fn auto_chain_flag_repaired_event_names_the_entry_point_that_found_the_leak() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        seed_gsd_config(root, true);
        assert!(
            gsd_config::auto_chain_active(root).unwrap(),
            "the fixture must actually carry the leak, or this test is vacuous"
        );

        repair_leaked_auto_chain_flag(root, root, PhaseId::new(81), "resume");

        let events = repair_events(root);
        assert_eq!(events.len(), 1, "exactly one repair event: {events:?}");
        assert_eq!(events[0]["entry_point"], "resume");
        assert_eq!(events[0]["working_tree_repaired"], true);
        assert!(
            !gsd_config::auto_chain_active(root).unwrap(),
            "the repair must actually clear the flag, not merely report it"
        );
    }

    /// THE CONTROL. Without it, an implementation that emitted on every launch
    /// would satisfy the test above — the event would be there either way, and
    /// "emitted for the right reason" and "emits unconditionally" would be
    /// indistinguishable.
    ///
    /// D-03 is about repairs, not about launches: an ordinary clean launch must
    /// write no event at all.
    #[test]
    fn auto_chain_flag_repaired_event_is_absent_on_a_clean_launch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        seed_gsd_config(root, false);

        repair_leaked_auto_chain_flag(root, root, PhaseId::new(82), "start");

        assert!(
            repair_events(root).is_empty(),
            "a launch that found nothing to repair must write no event: {:?}",
            repair_events(root)
        );
    }

    /// 18b: after `launch_stage` spawns a monitor, the persisted state file
    /// for that phase carries the published supervisor record — `transition()`
    /// saves state BEFORE calling `launch_stage`, so the record must be saved
    /// again inside `launch_stage` or it is lost.
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** This test's subject is
    /// record persistence, not which launch path runs, so it previously relied on
    /// its stage being ABSENT from `STREAM_JSON_STAGES` — an incidental
    /// premise that 34-05's widening destroys, at which point `canary_gate`
    /// invokes the real `ClaudeCanaryLauncher` and the launch fails on a
    /// delivery refusal that has nothing to do with record persistence. Pinning
    /// the legacy path via the opt-out makes the premise explicit and stable
    /// under any contents of the constant, exactly as
    /// `canary_gate_only_applies_to_the_stream_launch_path` does.
    #[test]
    fn launch_stage_persists_the_supervisor_record_for_reload() {
        const NAME: &str =
            "pipeline_launch::tests::launch_stage_persists_the_supervisor_record_for_reload";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(65);
        // `Mode::Supervise`, and the mode is now load-bearing (35.1-03): the
        // legacy opt-out set below makes this a launch shape whose Code stage
        // cannot bound the chain flag's lifetime, and
        // `preflight_unattended_launch_check` refuses exactly that combination
        // in `Mode::Auto`. This test's subject — record persistence — is
        // mode-independent, so supervise keeps the premise the 34-06 note
        // below establishes without asking the preflight to permit a launch
        // D-07 exists to refuse.
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let result = launch_stage(&mut state, None, None);

        // WR-03 / 999.46: this launch_stage call spawns a real detached
        // monitor wrapper, same as the staleness test above; the guard must
        // reap it before `dir` drops below. Bound here — this test's LAST
        // `&mut state` use — the guard outranks every panicking checkpoint
        // that follows: `result.unwrap()`, the `assert!` on
        // `state.supervisor.is_some()`, `workflow::load_state(...).unwrap()`,
        // and the `assert_eq!` on the reloaded record. Binding here also covers
        // the narrower case of a launch that spawned the monitor and then
        // failed a later `?` inside `launch_stage_inner` — `result` would be
        // `Err` but the supervisor would nonetheless be live (G-25-2, 25-17).
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        result.unwrap();

        assert!(
            state.supervisor.is_some(),
            "launch_stage must record the published supervisor on the in-memory state"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.supervisor, state.supervisor,
            "the supervisor record published by launch_stage must be persisted to disk, \
             since transition() saves state before launch_stage runs"
        );
    }

    /// Poll `done` every 50 ms until it holds or `limit` passes; returns its
    /// last answer.
    fn poll_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + limit;
        loop {
            if done() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Names the file a V-07 child writes its recorded socket path to.
    const V07_OUT_ENV: &str = "DEVFLOW_TEST_V07_OUT";

    /// A `claude` stand-in that answers `--version`, then waits for `go` in
    /// its working directory (the project root) and exits 0, with `sleep`
    /// linked beside it so the wait works under the replaced PATH.
    fn go_waiting_claude_dir() -> tempfile::TempDir {
        let path_dir = agent_free_dir_with_agent_stub("claude");
        std::fs::write(
            path_dir.path().join("claude"),
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
             while [ ! -e go ]; do sleep 0.05; done\nexit 0\n",
        )
        .unwrap();
        let real_sleep = ["/usr/bin/sleep", "/bin/sleep"]
            .into_iter()
            .map(Path::new)
            .find(|path| path.is_file())
            .expect("sleep must exist to run this test");
        std::os::unix::fs::symlink(real_sleep, path_dir.path().join("sleep")).unwrap();
        path_dir
    }

    /// V-07 (criterion 3): a launch persists `state.supervisor.socket_path`
    /// under its context's cache dir (`DEVFLOW_CACHE_DIR/sock/`), never under
    /// the project root, and the reloaded state holds exactly the published
    /// record. Control: the same launch in a child with another cache dir
    /// records a path under THAT dir, so the path follows the context.
    ///
    /// The first child also runs the advance-running harness entry
    /// (`test_support::monitor_entry_full`, `DEVFLOW_TEST_MONITOR_FULL=1`) and
    /// proves it is reached: with the state moved on to Plan while the agent
    /// runs, its in-process advance records `advance_failed` naming the
    /// launched stage (`define`). The second child, on the supervise-only
    /// entry, must record no advance event at all.
    #[test]
    fn launch_publishes_the_socket_path_under_the_cache_dir_in_state() {
        const NAME: &str =
            "pipeline_launch::tests::launch_publishes_the_socket_path_under_the_cache_dir_in_state";
        if !devflow_core::test_support::in_child_test(NAME) {
            let mut recorded = Vec::new();
            for full in [true, false] {
                let path_dir = go_waiting_claude_dir();
                let cache = crate::test_support::hermetic_cache_dir();
                let out = path_dir.path().join("v07-socket-path");
                let mut env: Vec<(&str, &std::ffi::OsStr)> = vec![
                    ("DEVFLOW_CACHE_DIR", cache.path().as_os_str()),
                    (V07_OUT_ENV, out.as_os_str()),
                ];
                if full {
                    env.push((TEST_MONITOR_FULL_ENV, std::ffi::OsStr::new("1")));
                }
                let output =
                    devflow_core::test_support::run_test_in_child(NAME, path_dir.path(), &env);
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, NAME,
                );
                let socket = PathBuf::from(std::fs::read_to_string(&out).unwrap());
                assert!(
                    socket.starts_with(cache.path().join("sock")),
                    "{} must be under {}",
                    socket.display(),
                    cache.path().display()
                );
                recorded.push((cache, socket));
            }
            let (a, b) = (&recorded[0], &recorded[1]);
            assert_ne!(a.0.path(), b.0.path(), "two distinct cache dirs");
            assert!(
                !a.1.starts_with(b.0.path()) && !b.1.starts_with(a.0.path()),
                "control: each launch's socket follows its own cache dir: {} and {}",
                a.1.display(),
                b.1.display()
            );
            return;
        }
        let _guard = env_lock();
        let cache = PathBuf::from(
            std::env::var_os("DEVFLOW_CACHE_DIR").expect("the parent passes DEVFLOW_CACHE_DIR"),
        );
        let out =
            PathBuf::from(std::env::var_os(V07_OUT_ENV).expect("the parent passes the out file"));
        let full = std::env::var_os(TEST_MONITOR_FULL_ENV).is_some_and(|value| value == "1");

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(1107);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        // The capture shape, for the same reason as
        // `launch_stage_persists_the_supervisor_record_for_reload`: no delivery canary.
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let result = launch_stage(&mut state, None, None);
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);
        result.unwrap();

        let record = state
            .supervisor
            .clone()
            .expect("the launch records its supervisor");
        assert!(
            record.socket_path.starts_with(cache.join("sock")),
            "{} must be under {}/sock",
            record.socket_path.display(),
            cache.display()
        );
        assert!(
            !record.socket_path.starts_with(root),
            "the socket must not live in the project tree"
        );
        assert!(record.socket_path.as_os_str().len() <= 107);
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.supervisor.as_ref(),
            Some(&record),
            "the reloaded state holds exactly the published record"
        );
        match devflow_core::supervisor::probe(&record.socket_path).unwrap() {
            devflow_core::supervisor::Liveness::Alive(reply) => {
                assert_eq!(reply.launch_id, record.launch_id);
                assert_eq!(reply.launch, "capture");
            }
            other => panic!("the recorded socket must answer ALIVE, got {other:?}"),
        }
        std::fs::write(&out, record.socket_path.display().to_string()).unwrap();

        let events_path = devflow_core::events::events_path(root);
        let advance_events = || {
            std::fs::read_to_string(&events_path)
                .unwrap_or_default()
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|event| {
                    event["event"]
                        .as_str()
                        .is_some_and(|name| name.starts_with("advance_"))
                })
                .collect::<Vec<_>>()
        };
        if full {
            let mut moved = workflow::load_state(root, phase).unwrap();
            moved.stage = Stage::Plan;
            workflow::save_state(&moved).unwrap();
        }
        std::fs::write(root.join("go"), "").unwrap();
        let gone = poll_until(Duration::from_secs(30), || {
            devflow_core::supervisor::probe(&record.socket_path)
                .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
        });
        assert!(gone, "the supervisor finishes once its agent exits");
        let events = advance_events();
        if full {
            let failed = events
                .iter()
                .find(|event| event["event"] == "advance_failed")
                .unwrap_or_else(|| panic!("monitor_entry_full must run the advance: {events:?}"));
            assert_eq!(failed["reason"], "stage mismatch", "{failed}");
            assert_eq!(
                failed["expected"], "define",
                "the advance is passed the launched stage: {failed}"
            );
        } else {
            assert!(
                events.is_empty(),
                "control: the supervise-only entry never advances: {events:?}"
            );
        }
    }

    /// A `claude` stand-in that leaves `agent-ran` in its working directory
    /// (the project root) and exits 0 without reading its stdin.
    fn marker_writing_claude_dir() -> tempfile::TempDir {
        let path_dir = agent_free_dir_with_agent_stub("claude");
        std::fs::write(
            path_dir.path().join("claude"),
            "#!/bin/sh\necho ran > agent-ran\nexit 0\n",
        )
        .unwrap();
        path_dir
    }

    /// 50-10 tracer (D-07, T-50-27): an in-crate stream launch really runs its
    /// agent, because `launch_context()`'s test branch re-enters this test
    /// binary at the `test_support::monitor_entry_supervise_only` harness
    /// entry, which runs the real `run_monitor` body; and the launch registers
    /// only under the cache dir its parent handed it.
    ///
    /// The control launches the same shape through
    /// `MonitorExec::Subcommand` at this test binary, which is what every
    /// in-crate stream launch did before 50-10: libtest rejects `__monitor`'s
    /// arguments, so the agent never runs. The same "agent ran" check that
    /// passes above must fail there, or it proves nothing.
    #[test]
    fn an_in_crate_stream_launch_runs_its_monitor_through_the_harness_entry() {
        const NAME: &str = "pipeline_launch::tests::an_in_crate_stream_launch_runs_its_monitor_through_the_harness_entry";
        if !devflow_core::test_support::in_child_test(NAME) {
            // The child gets a parent-owned `/tmp/dfu…` DEVFLOW_CACHE_DIR.
            let path_dir = marker_writing_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let cache = PathBuf::from(
            std::env::var_os("DEVFLOW_CACHE_DIR").expect("the parent passes DEVFLOW_CACHE_DIR"),
        );
        let prompt = || monitor::MonitorLaunch::PipeOwning {
            prompt: "50-10 tracer".to_string(),
        };

        // The harness-entry launch.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(1010);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let launched = spawn_agent_and_record(&mut state, "claude", &[], &[], None, prompt());
        let _reap = ReapMonitorOnDrop::after_launch(&state);
        launched.unwrap();
        let exit_path = devflow_core::agent_result::exit_code_path(root, phase);
        let exited = poll_until(Duration::from_secs(30), || {
            std::fs::read_to_string(&exit_path).is_ok_and(|code| !code.trim().is_empty())
        });
        assert!(
            root.join("agent-ran").is_file(),
            "the harness entry must run the agent: no marker at {} (exit file written: {exited})",
            root.join("agent-ran").display()
        );
        assert_eq!(
            std::fs::read_to_string(&exit_path).unwrap().trim(),
            "0",
            "the supervised agent's exit code must be recorded"
        );
        let entry = devflow_core::registry::entry_path_in(&cache, &state.project_root, phase);
        assert!(
            entry.is_file(),
            "the launch must register under its own cache dir: no {}",
            entry.display()
        );

        // The control: the pre-50-10 re-exec of this test binary.
        let control_dir = tempfile::tempdir().unwrap();
        let control_root = control_dir.path();
        let control_phase = PhaseId::new(1011);
        let mut control = State::new(
            control_phase,
            AgentKind::Claude,
            Mode::Supervise,
            control_root.to_path_buf(),
        );
        control.stage = Stage::Code;
        let subcommand = monitor::LaunchContext::new(
            cache.clone(),
            monitor::MonitorExec::Subcommand {
                exe: std::env::current_exe().unwrap(),
            },
        );
        // Since 50-11 `spawn_monitor` waits for the publish, so the control
        // monitor (libtest given `__monitor`'s arguments) exiting by itself
        // comes back as `NotReady` rather than as a pid to poll.
        let control_result =
            monitor::spawn_monitor(&control, "claude", &[], &[], prompt(), &subcommand);
        assert!(
            matches!(control_result, Err(monitor::MonitorError::NotReady(_))),
            "the control monitor (libtest given __monitor's arguments) must exit by itself \
             without publishing: {control_result:?}"
        );
        assert!(
            !control_root.join("agent-ran").exists(),
            "control: re-execing the test binary as __monitor must NOT run the agent"
        );
        assert!(
            !devflow_core::agent_result::exit_code_path(control_root, control_phase).exists(),
            "control: no monitor body ran, so no exit file may exist"
        );
    }

    /// The CLI source files under `src/`, found from `CARGO_MANIFEST_DIR`.
    fn cli_source_files() -> Vec<PathBuf> {
        fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    found.push(path);
                }
            }
        }
        let mut found = Vec::new();
        walk(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        found.sort();
        assert!(
            found
                .iter()
                .any(|path| path.ends_with("src/pipeline_launch.rs")),
            "the scan must see this file, or it scanned the wrong directory: {found:?}"
        );
        found
    }

    /// `source` with every line whose first non-blank characters are `//`
    /// dropped.
    fn code_lines(source: &str) -> impl Iterator<Item = &str> {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
    }

    /// Lines of `source` (comments dropped) that register a root through the
    /// env-reading `registry::register(`. `register_in(` does not match: `_`
    /// follows `register` there.
    fn unscoped_register_calls(source: &str) -> Vec<String> {
        let needle = concat!("registry::", "register(");
        code_lines(source)
            .filter(|line| line.contains(needle))
            .map(str::to_string)
            .collect()
    }

    /// T-50-27: every root registration in the CLI goes through
    /// `register_in(ctx.cache_dir, …)`, so a test's cache dir is the only
    /// place an in-crate launch can register.
    #[test]
    fn cli_source_registers_roots_only_through_register_in() {
        let planted = concat!(
            "    let _ = devflow_core::registry::",
            "register(&root, phase);\n"
        );
        assert_eq!(
            unscoped_register_calls(planted).len(),
            1,
            "planted control: a live unscoped register call must be flagged"
        );
        let commented = concat!(
            "    // let _ = devflow_core::registry::",
            "register(&root, phase);\n"
        );
        assert!(
            unscoped_register_calls(commented).is_empty(),
            "planted control: the same call behind // must not be flagged"
        );
        let scoped = concat!(
            "    let _ = devflow_core::registry::",
            "register_in(&ctx.cache_dir, &root, phase);\n"
        );
        assert!(
            unscoped_register_calls(scoped).is_empty(),
            "register_in( must not be flagged"
        );

        let mut offenders = Vec::new();
        for path in cli_source_files() {
            let source = std::fs::read_to_string(&path).unwrap();
            for line in unscoped_register_calls(&source) {
                offenders.push(format!("{}: {}", path.display(), line.trim()));
            }
        }
        assert!(
            offenders.is_empty(),
            "CLI source must register roots only through registry::register_in: {offenders:#?}"
        );
    }

    /// The 48-09 D-09 ENV_MUTEX-guarded `set_var`/`remove_var` calls already
    /// in this crate's tests (preflight 19, pipeline_launch 7, pipeline_gate
    /// 6, pipeline_outcomes 4; counted 2026-10-14 and again 2026-10-16).
    const GUARDED_ENV_MUTATION_CEILING: usize = 36;

    /// The process-env mutators, spelled through `concat!` so this file adds
    /// nothing to its own scan.
    const ENV_MUTATORS: [&str; 2] = [concat!("env::", "set_var"), concat!("env::", "remove_var")];

    /// The variables 50-10's cache isolation and harness-entry selection must
    /// reach a child only through `run_test_in_child`'s `extra_env`.
    const HERMETIC_ENV_LITERALS: [&str; 6] = [
        concat!("\"DEVFLOW_", "CACHE_DIR\""),
        concat!("\"HO", "ME\""),
        concat!("\"XDG_", "CACHE_HOME\""),
        concat!("\"XDG_", "CONFIG_HOME\""),
        concat!("\"DEVFLOW_", "MONITOR_ARGV\""),
        concat!("\"DEVFLOW_", "TEST_MONITOR_FULL\""),
    ];

    /// `source` with `//` lines dropped and the rest joined, so a call split
    /// across lines still reads as one.
    fn joined_code(source: &str) -> String {
        code_lines(source).collect::<Vec<_>>().join("\n")
    }

    /// Calls in comment-stripped `source` that mutate one of
    /// [`HERMETIC_ENV_LITERALS`]: a mutator, `(`, optional whitespace, then
    /// the variable's string literal.
    fn hermetic_env_mutations(source: &str) -> Vec<String> {
        let code = joined_code(source);
        let mut found = Vec::new();
        for mutator in ENV_MUTATORS {
            let call = format!("{mutator}(");
            for (at, _) in code.match_indices(&call) {
                let argument = code[at + call.len()..].trim_start();
                if let Some(literal) = HERMETIC_ENV_LITERALS
                    .iter()
                    .find(|literal| argument.starts_with(**literal))
                {
                    found.push(format!("{mutator}({literal}"));
                }
            }
        }
        found
    }

    /// Every `set_var`/`remove_var` in comment-stripped `source`.
    fn env_mutation_count(source: &str) -> usize {
        let code = joined_code(source);
        ENV_MUTATORS
            .iter()
            .map(|mutator| code.matches(mutator).count())
            .sum()
    }

    /// 50-10 Task 2: the hermetic variables reach a child test process only
    /// through `extra_env`, never through a process-global mutation, and this
    /// plan adds no other process-env mutation to the crate (the 36 guarded
    /// 48-09 calls stay as they are).
    #[test]
    fn cli_source_sets_no_hermetic_env_var_and_adds_no_process_env_mutation() {
        let split_call = concat!(
            "        std::env::set",
            "_var(\n",
            "            \"DEVFLOW_",
            "CACHE_DIR\", dir);\n"
        );
        assert_eq!(
            hermetic_env_mutations(split_call).len(),
            1,
            "planted control: a two-line mutation of the cache variable must be flagged"
        );
        let other_var = concat!(
            "        std::env::set",
            "_var(\n",
            "            \"DEVFLOW_",
            "OTHER\", dir);\n"
        );
        assert!(
            hermetic_env_mutations(other_var).is_empty(),
            "planted control: a mutation of an unrelated variable must not be flagged"
        );
        let three_calls = concat!(
            "    std::env::set",
            "_var(\"A\", \"1\");\n",
            "    std::env::remove",
            "_var(\"A\");\n",
            "    // std::env::set",
            "_var(\"A\", \"2\");\n"
        );
        assert_eq!(
            env_mutation_count(three_calls),
            2,
            "planted control: one set, one remove and one commented-out set count 2"
        );

        let mut hermetic = Vec::new();
        let mut total = 0;
        for path in cli_source_files() {
            let source = std::fs::read_to_string(&path).unwrap();
            for call in hermetic_env_mutations(&source) {
                hermetic.push(format!("{}: {call}", path.display()));
            }
            total += env_mutation_count(&source);
        }
        assert!(
            hermetic.is_empty(),
            "the hermetic variables must reach a child only through extra_env: {hermetic:#?}"
        );
        assert!(
            total <= GUARDED_ENV_MUTATION_CEILING,
            "CLI source holds {total} set_var/remove_var calls, more than the \
             {GUARDED_ENV_MUTATION_CEILING} ENV_MUTEX-guarded 48-09 calls"
        );
    }
    /// 20c (review: Codex MEDIUM — resume semantics): a phase halted by
    /// `--until <stage>` has `stopped`/`stop_reason`/`stop_until` persisted.
    /// `resume` must clear all three BEFORE relaunching — otherwise
    /// `transition()`'s `stop_until == Some(from)` check would immediately
    /// re-stop the phase the next time it advances, and the phase would
    /// stay marked `stopped` forever despite the operator's explicit
    /// resume. Asserts on the persisted state (not just `resume`'s exit
    /// code), since `transition()` saves state before `launch_stage` runs.
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** The subject is resume's
    /// stop-marker semantics, not which launch path the relaunch takes. The
    /// opt-out is persisted here rather than set on a local binding because
    /// `resume()` loads its own `State` from disk; `apply_legacy_launch_opt_out`
    /// ORs the persisted value, so it survives the reload.
    #[test]
    fn resume_clears_stop_marker_and_advances_past_stop_point() {
        const NAME: &str =
            "pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(66);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None, false);

        // `resume()` loads its own `State` from the state file and never writes the
        // spawned pid back into this test's local `state`, so binding the guard from
        // that local binding would capture `None` and silently reap nothing. Read the
        // pid back from disk. `ReapMonitorOnDrop` captures `Option<u32>` by value, so
        // the guard does not borrow the temporary it is built from.
        //
        // Bound here, ahead of `result.unwrap()` below: a `resume()` that spawned the
        // monitor and then failed a later `?` leaves `result` as `Err` with the pid
        // nonetheless live.
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(
            !reloaded.stopped,
            "resume must clear stopped so the phase is no longer marked halted"
        );
        assert_eq!(
            reloaded.stop_reason, None,
            "resume must clear stop_reason alongside stopped"
        );
        assert_eq!(
            reloaded.stop_until, None,
            "resume must clear stop_until so the phase does not immediately re-stop \
             the next time it advances past Plan"
        );
        assert!(
            reloaded.supervisor.is_some(),
            "resume() must have spawned a monitor whose supervisor is recorded in state — if this \
             fails, the reap guard above is silently reaping nothing and this test has \
             stopped covering the launch path it was written to cover"
        );
    }

    /// A phase parked at an open, already-answered Code gate: the state says
    /// `gate_pending`, the request file is on disk, and an unconsumed response
    /// sits beside it. Returns the (request, response) bytes as planted.
    fn seed_code_gate_with_pending_answer(
        root: &Path,
        phase: PhaseId,
        mode: Mode,
    ) -> (Vec<u8>, Vec<u8>) {
        let mut state = State::new(phase, AgentKind::Claude, mode, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_pending = true;
        state.stopped = true;
        state.stop_reason = Some("stage failed; gate open".to_string());
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            "[never-silent] stage code failed — human review needed",
        )
        .unwrap();
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: stale answer nobody polled","responded_by":"test"}"#,
        )
        .unwrap();
        (
            std::fs::read(Gates::gate_path(root, phase, Stage::Code)).unwrap(),
            std::fs::read(&response_path).unwrap(),
        )
    }

    /// SURV-02 / Phase 48 criterion 3 (48-CONTEXT D-05 "verified correction"):
    /// `resume` relaunches the saved stage and never reads a pending answer.
    /// The gate here is a stage-failure gate, not a preflight refusal: the
    /// relaunch's preflight PASSES (Supervise mode), so nothing re-fires the
    /// gate, and the planted answer must survive byte-identical — not read,
    /// not acked, not deleted.
    ///
    /// Scope: asserts on the state immediately after `resume()` returns. The
    /// detached monitor it spawns is reaped at drop; what that monitor would
    /// do with the answer later is outside this test.
    #[test]
    fn resume_relaunches_without_consuming_a_pending_gate_answer() {
        const NAME: &str =
            "pipeline_launch::tests::resume_relaunches_without_consuming_a_pending_gate_answer";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(91);
        let (request_before, response_before) =
            seed_code_gate_with_pending_answer(root, phase, Mode::Supervise);

        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        // (a) the saved stage was relaunched.
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.stage,
            Stage::Code,
            "resume must relaunch the saved stage"
        );
        assert!(
            reloaded.supervisor.is_some(),
            "resume must have spawned a monitor for the saved stage"
        );
        assert_eq!(
            stage_launched_count(root, phase),
            1,
            "resume must launch the saved stage exactly once"
        );

        // (b) the pending answer was neither read-and-consumed nor deleted.
        let response_path = Gates::response_path(root, phase, Stage::Code);
        assert_eq!(
            std::fs::read(&response_path).ok(),
            Some(response_before),
            "resume must leave the pending gate answer on disk byte-identical"
        );
        assert_eq!(
            std::fs::read(Gates::gate_path(root, phase, Stage::Code)).ok(),
            Some(request_before),
            "resume must leave the gate request untouched"
        );
        assert!(
            !Gates::ack_path(root, phase, Stage::Code).exists(),
            "an ack file means a poller consumed the answer"
        );
        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            !events.contains("\"gate_resolved\""),
            "resume must not resolve the gate: {events}"
        );
    }

    /// THE CONTROL for the test above: the one path 48-CONTEXT says DOES
    /// consume a stale answer on resume — a preflight refusal. Mode::Auto at
    /// Code with the legacy launch forced is refused by
    /// `preflight_unattended_launch_check`, the refusal gate re-fires at the
    /// same phase+stage, reads the planted abort answer, and `abort()` cleans
    /// it up. Same fixture, opposite result: proves the assertions above can
    /// observe consumption when it happens.
    #[test]
    fn resume_preflight_refusal_consumes_a_pending_gate_answer() {
        const NAME: &str =
            "pipeline_launch::tests::resume_preflight_refusal_consumes_a_pending_gate_answer";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(92);
        seed_code_gate_with_pending_answer(root, phase, Mode::Auto);

        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        assert!(
            !Gates::response_path(root, phase, Stage::Code).exists(),
            "the preflight-refusal gate must have consumed the stale answer"
        );
        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            events.contains("\"gate_resolved\""),
            "the stale answer must have resolved the re-fired gate: {events}"
        );
        assert_eq!(
            stage_launched_count(root, phase),
            0,
            "an aborted preflight must not launch the stage"
        );
    }

    /// D-15 (999.60): `resume` is also the recovery verb for a rate-limited
    /// or infra-paused phase — a case where `stopped` is `false` and
    /// `stop_until` is a cap the operator set that has NOT yet fired. Before
    /// this fix, `resume()` unconditionally cleared `stop_until` alongside
    /// `stopped`/`stop_reason`, so an unfired `--until` cap was silently
    /// discarded and the run sailed past the stage the operator capped, with
    /// nothing in the record saying it was ever dropped. Asserts on the
    /// persisted (reloaded) state, mirroring
    /// `resume_clears_stop_marker_and_advances_past_stop_point` above, since
    /// `transition()`'s `stop_until == Some(from)` interception only sees
    /// what was actually written to disk.
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** The subject is the
    /// unfired `--until` cap's survival, not which launch path the relaunch
    /// takes. Persisted rather than set locally, for the same reason as the
    /// sibling test above: `resume()` reloads its own `State`.
    #[test]
    fn resume_preserves_unfired_until_cap() {
        const NAME: &str = "pipeline_launch::tests::resume_preserves_unfired_until_cap";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(67);
        // `Mode::Supervise` for the same 35.1-03 reason as
        // `launch_stage_persists_the_supervisor_record_for_reload` above: the legacy
        // opt-out set below is refused in `Mode::Auto` by
        // `preflight_unattended_launch_check`, and this test's subject — the
        // unfired `stop_until` cap surviving a resume — is mode-independent.
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        // Current stage is earlier than the cap (Define < Plan), and the
        // phase was NOT stopped by the cap — this is the rate-limit/infra
        // recovery shape, not the "cap already fired" shape.
        state.stop_until = Some(Stage::Plan);
        state.stopped = false;
        state.stop_reason = None;
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None, false);

        // Same reap-before-unwrap ordering as the sibling test above: read
        // the pid back from disk, since `resume()` loads its own `State`
        // internally and never writes the spawned pid into this test's
        // local `state` binding.
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.stop_until,
            Some(Stage::Plan),
            "resume must NOT discard an unfired --until cap: stopped was false, so the \
             cap has not yet done its job and the operator's boundary must survive"
        );
        assert!(
            !reloaded.stopped,
            "an unfired cap must not itself flip stopped to true — resume only relaunches"
        );
        assert!(
            reloaded.supervisor.is_some(),
            "resume() must still have spawned a monitor whose supervisor is recorded in state"
        );
    }

    /// D-15 (999.60) third case: an ordinary rate-limit/infra resume with no
    /// `--until` cap at all (`stop_until: None`) must remain unaffected by
    /// gating the clear on `state.stopped` — nothing to preserve, nothing to
    /// clear, and the relaunch must still happen.
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** The subject is that the
    /// no-cap resume path is undisturbed, not which launch path the relaunch
    /// takes. Persisted rather than set locally, as in the two siblings above.
    #[test]
    fn resume_without_a_cap_is_unchanged() {
        const NAME: &str = "pipeline_launch::tests::resume_without_a_cap_is_unchanged";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(68);
        // `Mode::Supervise` for the same 35.1-03 reason as the two tests
        // above — the legacy opt-out below is refused in `Mode::Auto`, and the
        // absence of a `stop_until` cap is mode-independent.
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stop_until = None;
        state.stopped = false;
        state.stop_reason = None;
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None, false);

        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.stop_until, None,
            "no cap was ever set, so none must appear after resume"
        );
        assert!(!reloaded.stopped);
        assert_eq!(reloaded.stop_reason, None);
        assert!(
            reloaded.supervisor.is_some(),
            "resume() must still relaunch and record a supervisor with no cap present"
        );
    }

    #[test]
    fn resume_with_agent_hands_off_and_relaunches_under_the_new_driver() {
        const NAME: &str = "pipeline_launch::tests::resume_with_agent_hands_off_and_relaunches_under_the_new_driver";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("codex");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(74);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let record = devflow_core::ship::build_single_agent_cron_instructions(
            root,
            phase,
            "2026-06-18T15:45:30Z",
        );
        devflow_core::ship::write_cron_instructions(root, &record).unwrap();

        let result = resume(root, phase, Some(AgentKind::Codex), false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Codex);
        assert!(reloaded.supervisor.is_some());
        assert!(!devflow_core::ship::cron_instructions_path(root, phase).exists());
        let events = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap();
        let handoff = events
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|event| event["event"] == "agent_handoff")
            .expect("handoff event must exist");
        assert_eq!(handoff["stage"], "code");
        assert_eq!(handoff["from_agent"], "claude");
        assert_eq!(handoff["to_agent"], "codex");
        assert_eq!(handoff["reason"], "resume --agent");
    }

    #[test]
    fn resume_without_agent_leaves_the_saved_agent_untouched() {
        const NAME: &str =
            "pipeline_launch::tests::resume_without_agent_leaves_the_saved_agent_untouched";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(75);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();
        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();
        assert_eq!(
            workflow::load_state(root, phase).unwrap().agent,
            AgentKind::Claude
        );
    }

    #[test]
    fn resume_with_agent_refuses_before_touching_state_when_target_cannot_run_the_stage() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(76);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Define;
        workflow::save_state(&state).unwrap();
        let state_path = workflow::state_path(root, phase);
        let before = std::fs::read(&state_path).unwrap();

        let err = resume(root, phase, Some(AgentKind::Codex), false).unwrap_err();

        assert!(err.to_string().contains("refused"));
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            events
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .all(|event| event["event"] != "agent_handoff")
        );
    }

    /// Regression for 44-CORE-REVIEW-FINDINGS.md finding 2b: Codex declares
    /// Code headless-safe (`codex.rs`), so the early handoff check used to
    /// approve this handoff on interactivity grounds alone — only for
    /// `launch_stage`'s later, stricter unattended-launch-shape check (the
    /// Claude/Antigravity-only chain-flag guard, Auto mode only) to refuse
    /// it a moment later, after `state.agent` and the `agent_handoff` event
    /// were already committed. The handoff must now be refused up front,
    /// before anything is mutated — same as any other refused handoff.
    #[test]
    fn resume_with_agent_refuses_auto_mode_handoff_that_would_fail_the_later_unattended_launch_check()
     {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(81);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let state_path = workflow::state_path(root, phase);
        let before = std::fs::read(&state_path).unwrap();

        let err = resume(root, phase, Some(AgentKind::Codex), false).unwrap_err();

        assert!(err.to_string().contains("refused"), "{err}");
        assert!(err.to_string().contains("codex"), "{err}");
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            events
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .all(|event| event["event"] != "agent_handoff")
        );
    }

    #[test]
    fn resume_with_same_agent_is_an_ordinary_idempotent_resume() {
        const NAME: &str =
            "pipeline_launch::tests::resume_with_same_agent_is_an_ordinary_idempotent_resume";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(77);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();
        let result = resume(root, phase, Some(AgentKind::Claude), false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();
        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            events
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .all(|event| event["event"] != "agent_handoff")
        );
    }

    /// Pitfall 1: Plan is deliberately not gated by the Define-only artifact check.
    #[test]
    fn resume_with_agent_allows_plan_stage() {
        const NAME: &str = "pipeline_launch::tests::resume_with_agent_allows_plan_stage";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("codex");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(78);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        workflow::save_state(&state).unwrap();
        let result = resume(root, phase, Some(AgentKind::Codex), false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();
        assert_eq!(
            workflow::load_state(root, phase).unwrap().agent,
            AgentKind::Codex
        );
    }

    /// Whole-state comparison keeps this handoff check current when State gains fields.
    #[test]
    fn resume_with_agent_preserves_every_state_field_except_agent() {
        const NAME: &str =
            "pipeline_launch::tests::resume_with_agent_preserves_every_state_field_except_agent";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("codex");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(79);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.consecutive_failures = 3;
        state.infra_failures = 2;
        workflow::save_state(&state).unwrap();
        let mut before = serde_json::to_value(&state).unwrap();
        let result = resume(root, phase, Some(AgentKind::Codex), false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();
        let mut after = serde_json::to_value(workflow::load_state(root, phase).unwrap()).unwrap();

        // 48-07 (D-07): `checkpoint_approval` is the third field a handoff
        // legitimately writes, and it is asserted BELOW rather than merely
        // excluded — dropping a field from a tripwire without replacing its
        // coverage is how a whole-state check quietly stops checking.
        //
        // The write is intended. A handoff re-enters `launch_stage` for the
        // Code stage, which is a Code evaluation: the moment DevFlow looks at
        // the plan files before handing them to an agent — here, a DIFFERENT
        // agent than the one that stopped. Recording what it saw is the whole
        // mechanism CHKPT-02 relies on.
        let approval_before = before
            .as_object_mut()
            .unwrap()
            .remove("checkpoint_approval")
            .unwrap();
        let approval_after = after
            .as_object_mut()
            .unwrap()
            .remove("checkpoint_approval")
            .unwrap();
        assert_eq!(
            approval_before,
            serde_json::json!("pending"),
            "the fixture must start from Pending"
        );
        assert_eq!(
            approval_after,
            serde_json::json!({ "recorded": [] }),
            "the handoff's Code preflight must record the set it saw — this fixture has no \
             plan files, so the set is legitimately empty"
        );

        // 50-11: the relaunch also records its ping-confirmed supervisor,
        // asserted rather than merely excluded for the same reason.
        let supervisor_before = before
            .as_object_mut()
            .unwrap()
            .remove("supervisor")
            .unwrap();
        let supervisor_after = after.as_object_mut().unwrap().remove("supervisor").unwrap();
        assert_eq!(supervisor_before, serde_json::Value::Null);
        assert!(
            supervisor_after["socket_path"].is_string()
                && supervisor_after["launch_id"].is_string(),
            "the relaunch must record its published supervisor: {supervisor_after}"
        );

        for value in [&mut before, &mut after] {
            value.as_object_mut().unwrap().remove("agent");
        }
        assert_eq!(before, after);
    }

    #[test]
    fn resume_with_agent_from_a_rate_limited_state_relaunches() {
        const NAME: &str =
            "pipeline_launch::tests::resume_with_agent_from_a_rate_limited_state_relaunches";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("codex");
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(80);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.stopped = true;
        state.stop_reason = Some("rate limited".to_string());
        workflow::save_state(&state).unwrap();
        let record = devflow_core::ship::build_single_agent_cron_instructions(
            root,
            phase,
            "2026-06-18T15:45:30Z",
        );
        devflow_core::ship::write_cron_instructions(root, &record).unwrap();
        let result = resume(root, phase, Some(AgentKind::Codex), false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Codex);
        assert!(!reloaded.stopped);
        assert!(reloaded.supervisor.is_some());
        assert!(!devflow_core::ship::cron_instructions_path(root, phase).exists());
    }

    /// Regression for 44-CORE-REVIEW-FINDINGS.md finding 2a: `resume` clears
    /// and persists `stopped`/`stop_reason`/`stop_until` BEFORE calling
    /// `launch_stage` (needed so a reload mid-relaunch already sees the
    /// phase as active — D-15/999.60). If `launch_stage` then fails outright
    /// — here, the agent binary is simply missing — that save must not be
    /// left standing: a state file claiming `stopped: false` with nothing
    /// actually running is a zombie invisible to `check_dead_agent`/
    /// `check_dead_monitor`, which both skip any phase already marked
    /// `stopped`. `resume` must re-mark the phase `stopped` (with a reason)
    /// on this failure path, not leave it looking falsely active.
    ///
    /// The unrunnable-agent window runs in a CHILD process
    /// ([`run_test_without_git`], 46-06 / review finding C-01) so the emptied
    /// `PATH` is invisible to every sibling test; the fixture stays in the
    /// parent because building it shells out to `git`.
    #[test]
    fn resume_re_marks_stopped_when_launch_stage_fails_outright() {
        const NAME: &str = "pipeline_launch::tests::\
                            resume_re_marks_stopped_when_launch_stage_fails_outright";
        let phase = PhaseId::new(82);

        if let Some(root) = child_no_git_root() {
            // CHILD half: `PATH` is an empty directory, so no `claude` binary
            // is resolvable anywhere on it and `ensure_agent_binary` inside
            // `launch_stage` fails deterministically regardless of what is
            // installed on the host running this test.
            let err = resume(&root, phase, None, false).unwrap_err();
            assert!(err.to_string().contains("not found"), "{err}");
            let reloaded = workflow::load_state(&root, phase).unwrap();
            assert!(
                reloaded.stopped,
                "a failed resume must leave the phase marked stopped, not a false-active zombie"
            );
            assert!(reloaded.stop_reason.is_some_and(|r| r.contains("claude")));
            assert!(reloaded.supervisor.is_none());
            return;
        }

        // PARENT half: `init_repo` and `save_state` both need a working
        // `git`, so the fixture cannot move across the process boundary.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.stopped = true;
        state.stop_reason = Some("rate limited".to_string());
        workflow::save_state(&state).unwrap();

        let out = run_test_without_git(NAME, root);
        assert_child_ran_exactly_one_passing_test(&out, NAME);
    }

    /// D-01/D-06 regression: a Code-stage `Unknown` outcome (Layer 3's
    /// "process gone but commits exist" case) must route through
    /// `handle_stage_failure`'s never-silent gate, never
    /// `transition(.., Stage::Validate)`. Drives a real `advance()` on a
    /// scoped thread, polling for the Code gate file (not a Validate one) to
    /// prove the dispatch never took the success/Advance arm.
    #[test]
    fn code_unknown_does_not_transition_to_validate() {
        const NAME: &str = "pipeline_launch::tests::code_unknown_does_not_transition_to_validate";
        enter_agent_free_child!(NAME, "claude");
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "abort-fixture test must run in a child with an agent-free PATH (999.80)"
        );
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(72);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let git = |args: &[&str]| {
            assert!(
                devflow_core::test_support::git_command(root)
                    .args(args)
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
                advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();
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

    /// WR-04 (18-fix, 50-23): an early failure in `launch_stage_inner` —
    /// before `monitor::spawn_monitor` ever runs — must not leave the
    /// PREVIOUS stage's supervisor record behind. Pre-fix, the state still
    /// named the previous stage's (now-dead) monitor after the launch
    /// returned early via `?`, and `status`/`doctor` then misreported it as
    /// gone while active, naming `devflow resume` — the wrong remedy for
    /// what is actually a launch failure.
    ///
    /// Which early failure this reaches (checked by hand mutants, 50-22): the
    /// child's PATH holds only `git` and an `exit 0` stub `claude`, so for an
    /// `Auto` Code launch the delivery canary reads `Absent` and the launch
    /// ends in [`refuse_launch`], before `ensure_agent_binary` runs. The
    /// other early-failure path, [`record_failed_launch`], is pinned by
    /// `a_failed_monitor_spawn_clears_the_supervisor_record`.
    #[test]
    fn launch_stage_inner_clears_the_supervisor_record_on_early_failure() {
        const NAME: &str = "pipeline_launch::tests::\
                            launch_stage_inner_clears_the_supervisor_record_on_early_failure";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(93);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        // 50-23: the prior stage's supervisor record, which must be cleared,
        // not carried forward into the new stage, or `status` reads
        // GONE-while-active and names `devflow resume`.
        state.supervisor = Some(fabricated_prior_record(root));
        workflow::save_state(&state).unwrap();

        let result = launch_stage_inner(&mut state, None, None);

        assert!(
            result.is_err(),
            "the launch must fail early: the stub `claude` cannot confirm the canary"
        );
        assert_eq!(
            state.supervisor, None,
            "an early launch failure must clear the prior supervisor record in memory"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.supervisor, None,
            "the supervisor record clear must be persisted to state.json"
        );
    }

    /// D-15 (44-02): a failed monitor relaunch must leave the phase cron
    /// instructions available for a later retry. The invalid worktree is
    /// deliberate: archive bookkeeping has no capture to process, while the
    /// monitor's child spawn fails at its working-directory validation.
    /// Consumption is after the successful spawn in `spawn_agent_and_record`,
    /// so this test is the negative control for the successful resume tests.
    #[test]
    fn failed_relaunch_preserves_the_phase_cron_instructions_record() {
        const NAME: &str =
            "pipeline_launch::tests::failed_relaunch_preserves_the_phase_cron_instructions_record";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(144);
        let missing_worktree = root.join("worktree-that-does-not-exist");
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.worktree_path = Some(missing_worktree.clone());
        workflow::save_state(&state).unwrap();

        let record = devflow_core::ship::build_single_agent_cron_instructions(
            root,
            phase,
            "2026-06-18T15:45:30Z",
        );
        devflow_core::ship::write_cron_instructions(root, &record).unwrap();
        assert!(
            devflow_core::ship::cron_instructions_path(root, phase).exists(),
            "the fixture must contain a phase cron record before relaunch"
        );
        assert!(
            !missing_worktree.exists(),
            "the failure fixture must be absent"
        );

        let result = spawn_agent_and_record(
            &mut state,
            "claude",
            &[],
            &[],
            None,
            monitor::MonitorLaunch::Legacy,
        );

        assert!(
            result.is_err(),
            "monitor spawn must fail for an absent worktree"
        );
        assert!(
            devflow_core::ship::cron_instructions_path(root, phase).exists(),
            "a failed monitor relaunch must preserve the phase cron record"
        );
        assert!(
            events_of_kind(root, "cron_instructions_consumed").is_empty(),
            "a failed monitor relaunch must not emit cron_instructions_consumed"
        );
    }
    /// D-10: `advance_evaluated` emits `status` via `AgentStatus::as_wire_str()`
    /// (never the Debug-lowercase formatter that collapses `ResourceKilled`
    /// into `resourcekilled`) and carries the `decided_by_layer` evidence
    /// field.
    #[test]
    fn advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed() {
        const NAME: &str = "pipeline_launch::tests::advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed";
        enter_agent_free_child!(NAME, "claude");
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "abort-fixture test must run in a child with an agent-free PATH (999.80)"
        );
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(78);
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

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

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

    /// Collect every event of `kind` for the phase recorded in `root`'s
    /// event log, oldest-first.
    fn events_of_kind(root: &Path, kind: &str) -> Vec<serde_json::Value> {
        let contents = std::fs::read_to_string(events::events_path(root)).unwrap_or_default();
        contents
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["event"] == kind)
            .collect()
    }

    #[test]
    fn advance_waits_for_a_held_lock_then_proceeds() {
        const NAME: &str = "pipeline_launch::tests::advance_waits_for_a_held_lock_then_proceeds";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(48);
        init_repo(root);
        write_unreported_failure_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let held = lock::acquire(root, phase).expect("test holder acquires phase lock");

        std::thread::scope(|scope| {
            scope.spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                drop(held);
            });
            let result = advance_with(root, phase, None, Duration::from_secs(10));
            assert!(result.is_ok(), "advance result: {result:?}");
        });
    }

    #[test]
    fn advance_lock_wait_expiry_emits_advance_failed() {
        const NAME: &str = "pipeline_launch::tests::advance_lock_wait_expiry_emits_advance_failed";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(49);
        let _held = lock::acquire(root, phase).expect("test holder acquires phase lock");

        let err = advance_with(root, phase, None, Duration::from_millis(300))
            .expect_err("advance must time out behind a live holder");
        assert!(err.to_string().contains("holder pid"));
        let failed = events_of_kind(root, "advance_failed");
        assert_eq!(failed.len(), 1, "one durable failure event: {failed:?}");
        assert_eq!(failed[0]["phase"], 49);
        assert_eq!(failed[0]["reason"], "lock wait expired");
        assert!(failed[0]["holder_pid"].as_str().is_some());
    }

    #[test]
    fn advance_refuses_when_the_saved_stage_moved_on() {
        const NAME: &str = "pipeline_launch::tests::advance_refuses_when_the_saved_stage_moved_on";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(50);
        init_repo(root);
        write_unreported_failure_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let state_path = workflow::state_path(root, phase);
        let before = std::fs::read(&state_path).unwrap();

        let err = advance_with(root, phase, Some(Stage::Define), Duration::from_secs(1))
            .expect_err("advance must reject a stage that has moved on");
        assert!(err.to_string().contains("stage mismatch"));
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        let failed = events_of_kind(root, "advance_failed");
        assert_eq!(failed.len(), 1, "one durable refusal event: {failed:?}");
        assert_eq!(failed[0]["reason"], "stage mismatch");
        assert_eq!(failed[0]["expected"], "define");
        assert_eq!(failed[0]["actual"], "code");
    }

    #[test]
    fn advance_without_stage_proceeds_and_records_the_legacy_invocation() {
        const NAME: &str = "pipeline_launch::tests::advance_without_stage_proceeds_and_records_the_legacy_invocation";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(51);
        init_repo(root);
        write_unreported_failure_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        advance_with(root, phase, None, Duration::from_secs(1)).unwrap();
        assert_eq!(events_of_kind(root, "advance_stage_unbound").len(), 1);
    }

    #[test]
    fn existing_advance_test_callers_pass_their_fixture_stage() {
        const NAME: &str =
            "pipeline_launch::tests::existing_advance_test_callers_pass_their_fixture_stage";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(52);
        init_repo(root);
        write_unreported_failure_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        advance_with(root, phase, Some(Stage::Code), Duration::from_secs(1)).unwrap();
        assert!(events_of_kind(root, "advance_failed").is_empty());
        assert!(events_of_kind(root, "advance_stage_unbound").is_empty());
    }

    /// D-04/D-07 (28-03, Task 2): a checkpoint resume records the
    /// `checkpoint_auto_decided` audit event exactly once, carrying the
    /// session id and stage — the only durable record of an unattended
    /// checkpoint decision.
    #[test]
    fn relaunch_checkpoint_session_emits_exactly_one_audit_event() {
        const NAME: &str =
            "pipeline_launch::tests::relaunch_checkpoint_session_emits_exactly_one_audit_event";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(84);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let result = relaunch_checkpoint_session(&mut state, "sess-abc-123");
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        result.unwrap();

        let matches = events_of_kind(root, "checkpoint_auto_decided");
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one checkpoint_auto_decided event: {matches:?}"
        );
        assert_eq!(matches[0]["session_id"], "sess-abc-123");
        assert_eq!(matches[0]["stage"], "code");
        assert_eq!(matches[0]["attempt"], 1);
        assert_eq!(
            matches[0]["policy"], "D-03b: Auto-only checkpoint auto-decide",
            "the audit event must name the policy the runtime actually applied"
        );
    }

    /// D-04 (28-03, Task 2): the resume ceiling increments with saturating
    /// arithmetic and the incremented value is persisted to disk, since a
    /// relaunch and its bookkeeping cross separate `devflow advance`
    /// invocations.
    #[test]
    fn relaunch_checkpoint_session_increments_and_persists_counter() {
        const NAME: &str =
            "pipeline_launch::tests::relaunch_checkpoint_session_increments_and_persists_counter";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(85);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.checkpoint_resumes = 1;
        workflow::save_state(&state).unwrap();

        let result = relaunch_checkpoint_session(&mut state, "sess-xyz");
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        result.unwrap();

        assert_eq!(state.checkpoint_resumes, 2);
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.checkpoint_resumes, 2,
            "the incremented counter must persist to disk"
        );
    }

    /// D-03/D-04 (28-03, Task 2): a checkpoint resume must not re-run
    /// preflight or move the phase to a new stage — it is a continuation of
    /// the current stage's agent run, not a new stage entry.
    #[test]
    fn relaunch_checkpoint_session_does_not_change_stage() {
        const NAME: &str =
            "pipeline_launch::tests::relaunch_checkpoint_session_does_not_change_stage";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(86);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let result = relaunch_checkpoint_session(&mut state, "sess-stage");
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        result.unwrap();

        assert_eq!(
            state.stage,
            Stage::Code,
            "a checkpoint resume must not advance the stage"
        );
    }

    /// 28-03 (Task 2): an ordinary stage launch resets the checkpoint-resume
    /// budget to zero, including in a state that carried a nonzero count
    /// from a prior stage's resume attempts — this is what makes the
    /// ceiling bound one stage's resume budget rather than a phase's entire
    /// lifetime.
    #[test]
    fn launch_stage_inner_resets_checkpoint_resumes_counter() {
        const NAME: &str =
            "pipeline_launch::tests::launch_stage_inner_resets_checkpoint_resumes_counter";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(87);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.checkpoint_resumes = 2;
        // 31-03: Code is the one stage widened to the `stream-json` transport,
        // so a launch here passes the D-15 delivery-canary gate. Recording an
        // already-`Confirmed` outcome is what a real run looks like by its
        // second Code launch, and it keeps this test about the resume counter
        // rather than about the guard — `launch_stage_inner_refuses_at_code_
        // when_the_canary_cannot_confirm` covers the guard's own wiring.
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        workflow::save_state(&state).unwrap();

        let result = launch_stage_inner(&mut state, None, None);
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        result.unwrap();

        assert_eq!(
            state.checkpoint_resumes, 0,
            "an ordinary stage launch must reset the checkpoint-resume budget"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.checkpoint_resumes, 0);
    }

    /// 35.2 P-02: the nonce stamp and baseline re-observation are co-located
    /// and gated on Validate. Both arms must disagree — a stamp on every
    /// stage re-widens the window at the Code stage.
    #[test]
    fn launch_stage_inner_stamps_the_validate_dispatch_nonce_with_its_baseline() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(92);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        std::fs::create_dir_all(&worktree).unwrap();
        state.worktree_path = Some(worktree.clone());

        // Write an artifact under the evidence root.
        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-stamp", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded())),
            "verdict: pass\n",
        )
        .unwrap();

        let fingerprint_before =
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase);

        // Validate stage: nonce advances, fingerprint refreshed.
        state.stage = Stage::Validate;
        stamp_validate_dispatch_window(&mut state);
        assert_eq!(state.verification_run_nonce, Some(1));
        assert_eq!(
            state.last_verification_fingerprint, fingerprint_before,
            "fingerprint must match the on-disk artifact after stamp"
        );
        assert!(state.verification_baseline_captured);

        // Non-Validate stage: nothing changes.
        let nonce_after_validate = state.verification_run_nonce;
        let fp_after_validate = state.last_verification_fingerprint;
        state.stage = Stage::Code;
        stamp_validate_dispatch_window(&mut state);
        assert_eq!(
            state.verification_run_nonce, nonce_after_validate,
            "nonce must not change on non-Validate stages"
        );
        assert_eq!(
            state.last_verification_fingerprint, fp_after_validate,
            "fingerprint must not change on non-Validate stages"
        );
    }

    // ---- D-15 delivery-canary gate (31-03) ------------------------------
    //
    // Every case below except the last drives the gate with an INJECTED
    // outcome rather than the real `ClaudeCanaryLauncher`, and that bounds
    // what they establish. They prove the GATE's wiring — once per run,
    // stream path only, refuse on both failure modes, persist, emit. They
    // prove nothing at all about whether the real `claude` CLI still
    // delivers background-task notifications; only plan 31-05's acceptance
    // run against the real CLI can establish that.

    /// A canary stand-in that records how many times it was asked to run.
    fn counting_canary(
        calls: &std::cell::Cell<usize>,
        outcome: canary::CanaryOutcome,
    ) -> impl FnOnce() -> canary::CanaryOutcome + '_ {
        move || {
            calls.set(calls.get() + 1);
            outcome
        }
    }

    fn canary_state(root: &Path, phase: PhaseId) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        state
    }

    /// D-15: the guard runs ONCE PER RUN. A canary that re-ran at every stage
    /// transition would re-spend a real throwaway agent invocation each time —
    /// the symptom 31-RESEARCH Pitfall 5 names for a guard that landed in the
    /// per-stage `preflight` hook instead of here.
    #[test]
    fn canary_runs_once_per_run() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(120);
        let mut state = canary_state(root, phase);
        let calls = std::cell::Cell::new(0usize);

        canary_gate(
            &mut state,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Confirmed),
        )
        .unwrap();
        assert_eq!(
            calls.get(),
            1,
            "the first launch of a run must run the guard"
        );
        assert_eq!(state.canary, Some(canary::CanaryOutcome::Confirmed));

        canary_gate(
            &mut state,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Confirmed),
        )
        .unwrap();
        assert_eq!(
            calls.get(),
            1,
            "a second stage launch in the same run must read the recorded outcome, \
             not re-spend an agent invocation"
        );

        // Negative control: the counter CAN reach 2. Without this, a
        // `counting_canary` that was never wired in at all would produce the
        // same reading as a correctly once-per-run guard.
        let mut fresh_run = canary_state(root, PhaseId::new(phase.major() + 1));
        canary_gate(
            &mut fresh_run,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Confirmed),
        )
        .unwrap();
        assert_eq!(
            calls.get(),
            2,
            "a run with no recorded outcome must run the guard — if this fails, the \
             assertion above is measuring a closure that is never invoked"
        );
    }

    /// The guard protects one premise: that a live `stream-json` session is
    /// woken back up when a background task finishes. A launch that resolved to
    /// the legacy single-document path does not rely on that premise.
    ///
    /// **The false-branch discriminator is D-11's legacy opt-out, deliberately —
    /// NOT stage membership** (ROADMAP criterion 7). This test previously took
    /// `Stage::Plan`'s absence from [`STREAM_JSON_STAGES`] as its premise. That
    /// premise is destroyed by the very rollout the constant exists to
    /// sequence: once all five stages are widened, no Claude stage yields
    /// `false` and the test becomes unconstructible as written — it would have
    /// had to be deleted or rewritten under the time pressure of the widening
    /// commit. `legacy_opt_out` is a separate `&&` term that
    /// `stream_launch_enabled` respects regardless of the constant's
    /// contents, so a premise built on it survives full widening.
    ///
    /// Verified, not assumed: this pair was run against a temporarily
    /// fully-widened `STREAM_JSON_STAGES` and stayed green, while the
    /// pre-rebuild version failed on its `Stage::Plan` premise.
    #[test]
    fn canary_gate_only_applies_to_the_stream_launch_path() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(121);
        let mut state = canary_state(root, phase);
        // `Stage::Code` is on the stream path today and stays on it. The
        // variable under test is the opt-out, not the stage.
        state.stage = Stage::Code;
        state.legacy_claude_launch = true;

        // Driven by the REAL predicate rather than a hardcoded `false`, so this
        // test tracks the rollout instead of a copy of it.
        let stream_launch =
            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(
            !stream_launch,
            "the legacy opt-out must force this launch off the stream path for this test \
             to mean anything"
        );
        // Negative control: the SAME agent and the SAME stage, with only the
        // opt-out cleared, DOES fire. Without this, the reading above would be
        // a constant rather than a discrimination — and unlike a stage-
        // membership control, this one cannot be invalidated by widening.
        assert!(
            stream_launch_enabled(AgentKind::Claude, state.stage, false),
            "clearing the opt-out must flip the predicate back to true, or the check above \
             is vacuous"
        );

        let calls = std::cell::Cell::new(0usize);
        // `Absent` deliberately: if the gate ran this canary at all, the call
        // below returns Err and the unwrap fails.
        canary_gate(
            &mut state,
            stream_launch,
            counting_canary(&calls, canary::CanaryOutcome::Absent),
        )
        .unwrap();

        assert_eq!(
            calls.get(),
            0,
            "a legacy launch must not spend an agent invocation on a premise it never relies on"
        );
        assert_eq!(
            state.canary, None,
            "a launch that never ran the guard must not record an outcome for it"
        );
    }

    /// The other direction of the pair above: with the opt-out cleared, the
    /// same fixture DOES run the guard and DOES record its outcome.
    ///
    /// The refusal half alone cannot distinguish "the gate correctly skipped a
    /// legacy launch" from "the gate is wired to nothing". This is the case
    /// that must produce the opposite result, at the level of the gate's own
    /// effects rather than the predicate's return value.
    ///
    /// `Confirmed`, not `Absent`: the sibling test can use `Absent` only
    /// because its gate never invokes the canary at all. Here it does, and an
    /// `Absent` outcome would make `canary_gate` return `Err` and the `unwrap`
    /// below fail for a reason that has nothing to do with what is under test.
    #[test]
    fn canary_gate_still_fires_for_a_widened_stage_without_the_opt_out() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(123);
        let mut state = canary_state(root, phase);
        state.stage = Stage::Code;
        state.legacy_claude_launch = false;

        let stream_launch =
            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(
            stream_launch,
            "without the opt-out this stage must be on the stream path, or this test is \
             asserting the sibling test's case a second time"
        );

        let calls = std::cell::Cell::new(0usize);
        canary_gate(
            &mut state,
            stream_launch,
            counting_canary(&calls, canary::CanaryOutcome::Confirmed),
        )
        .unwrap();

        assert_eq!(
            calls.get(),
            1,
            "a stream launch with no recorded outcome must spend the guard exactly once"
        );
        assert_eq!(
            state.canary,
            Some(canary::CanaryOutcome::Confirmed),
            "a launch that ran the guard must persist what it found"
        );
    }

    /// D-15's refusal. Warning-and-proceeding was rejected (the warning scrolls
    /// past in unattended mode) and so was falling back to sequential dispatch
    /// (a silent capability downgrade).
    #[test]
    fn absent_canary_refuses_to_launch() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(122);
        let mut state = canary_state(root, phase);
        // The previous stage's supervisor record, as `transition()` would
        // leave it (50-23).
        state.supervisor = Some(fabricated_prior_record(root));
        workflow::save_state(&state).unwrap();

        let calls = std::cell::Cell::new(0usize);
        let err = canary_gate(
            &mut state,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Absent),
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(
            message.contains("ABSENT"),
            "the refusal must name which of the two failure modes occurred, got: {message}"
        );
        assert!(
            message.contains("multi-plan wave"),
            "the refusal must say WHICH guarantee is no longer backed by observed behaviour, \
             got: {message}"
        );

        assert_eq!(
            state.supervisor, None,
            "a refused launch must not leave the previous stage's supervisor record standing: \
             it reads GONE-while-active and sends the operator to `devflow resume`"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.supervisor, None,
            "the cleared supervisor record must be persisted, not only cleared in memory"
        );
        assert_eq!(
            reloaded.canary,
            Some(canary::CanaryOutcome::Absent),
            "a refusal must still record what the guard found, or the next launch re-runs it"
        );
    }

    /// "The CLI could not be run" and "the CLI ran and the behaviour is gone"
    /// call for different operator action. A merged message would report a
    /// missing binary as a broken premise (T-31-12: the real risk this guard
    /// carries is a FALSE refusal).
    #[test]
    fn unverified_canary_refuses_to_launch_with_a_distinct_message() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let mut unverified_state = canary_state(root, PhaseId::new(123));
        let calls = std::cell::Cell::new(0usize);
        let unverified = canary_gate(
            &mut unverified_state,
            true,
            counting_canary(
                &calls,
                canary::CanaryOutcome::Unverified(
                    "could not run `claude`: No such file or directory (os error 2)".to_string(),
                ),
            ),
        )
        .unwrap_err()
        .to_string();

        assert!(
            unverified.contains("No such file or directory"),
            "the reason the guard could not run must reach the operator, got: {unverified}"
        );
        assert!(
            !unverified.contains("ABSENT"),
            "an unverified guard must NOT claim the behaviour is gone, got: {unverified}"
        );

        // The comparison that makes "distinct" a measurement rather than a
        // claim: the same gate, the other failure mode, a different message.
        let mut absent_state = canary_state(root, PhaseId::new(124));
        let absent = canary_gate(&mut absent_state, true, || canary::CanaryOutcome::Absent)
            .unwrap_err()
            .to_string();
        assert_ne!(
            unverified, absent,
            "the two failure modes must not render the same diagnosis"
        );
    }

    /// D-15: every run carries evidence of what was verified when.
    #[test]
    fn canary_outcome_is_persisted_and_emitted() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(125);
        let mut state = canary_state(root, phase);
        let calls = std::cell::Cell::new(0usize);

        canary_gate(
            &mut state,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Confirmed),
        )
        .unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.canary,
            Some(canary::CanaryOutcome::Confirmed),
            "the outcome must survive to the next `devflow` process — each stage launch is one"
        );

        let log = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap();
        let line = log
            .lines()
            .find(|line| line.contains("claude_delivery_canary_confirmed"))
            .expect("the run's provenance must carry the canary outcome");
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(event["event"], "claude_delivery_canary_confirmed");
        assert!(phase.matches_json(event.get("phase")));

        // T-31-13: the PREFIX, exactly — never a whole token. An exact
        // comparison against the constant is what catches a later change that
        // swaps the field's value for the token itself.
        assert_eq!(
            event["token_prefix"],
            canary::TOKEN_PREFIX,
            "the payload must carry the token's prefix and nothing more"
        );
        assert_eq!(
            line.matches(canary::TOKEN_PREFIX).count(),
            1,
            "the prefix must appear exactly once — a second occurrence means a token leaked in"
        );
    }

    /// F4 (41-02 review): the canary outcome emission is agent-aware — an
    /// Antigravity run records `antigravity_delivery_canary_*`, never a
    /// Claude event, and reads `agy --version` rather than `claude --version`.
    #[test]
    fn antigravity_canary_outcome_emits_antigravity_provenance() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(126);
        let mut state = State::new(
            phase,
            AgentKind::Antigravity,
            Mode::Auto,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let calls = std::cell::Cell::new(0usize);
        canary_gate(
            &mut state,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Confirmed),
        )
        .unwrap();

        let log = std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap();
        let line = log
            .lines()
            .find(|line| line.contains("antigravity_delivery_canary_confirmed"))
            .expect("an Antigravity run must emit the antigravity canary event");
        let event: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(event["event"], "antigravity_delivery_canary_confirmed");
        // Never a Claude event on an Antigravity run.
        assert!(
            !log.contains("claude_delivery_canary_"),
            "an Antigravity run must not record Claude canary provenance"
        );
    }

    /// The linkage the five gate tests above cannot show: that
    /// `launch_stage_inner` actually calls the gate, at the widened stage, with
    /// the REAL launcher bound. Runs against the `exit 0` stub `claude`, which
    /// produces an empty capture and therefore an honest `Absent` — no real
    /// agent invocation, and no claim about the real CLI's behaviour.
    #[test]
    fn launch_stage_inner_refuses_at_code_when_the_canary_cannot_confirm() {
        const NAME: &str = "pipeline_launch::tests::launch_stage_inner_refuses_at_code_when_the_canary_cannot_confirm";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(126);
        let mut state = canary_state(root, phase);

        let result = launch_stage_inner(&mut state, None, None);
        // The refusal is meant to spawn nothing, but bind the guard anyway —
        // this test is worthless if it silently starts leaking monitors.
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        let message = result
            .expect_err("a launch whose canary cannot confirm must not proceed")
            .to_string();
        assert!(
            message.contains("ABSENT"),
            "the launch path must surface the guard's own diagnosis, got: {message}"
        );
        assert_eq!(state.canary, Some(canary::CanaryOutcome::Absent));
        assert!(
            state.supervisor.is_none(),
            "a refused launch must record no supervisor"
        );
        assert_eq!(
            stage_launched_count(root, phase),
            0,
            "a refused launch must emit no stage_launched event — nothing was launched"
        );
    }

    // D-01/D-03/D-05 (28-03, Task 3): the dispatch guard's six named
    // scenarios. The gate value is assembled from a const/format! rather
    // than a bare source literal (28-01's precedent) so this file itself
    // never contains the literal `gate="blocking-human"`.
    const HUMAN_GATE_VALUE_FOR_TEST: &str = "blocking-human";

    /// The PLAIN `blocking` gate — deliberately NOT the human-blocking one.
    /// Used only by [`write_plan_without_checkpoint`]'s decoy body, so the
    /// literal `verify::phase_has_blocking_human_checkpoint` searches for is
    /// absent from it (the Phase 26 near-miss distinction that
    /// `verify.rs`'s `..._false_for_plain_blocking_gate` already pins).
    const PLAIN_GATE_VALUE_FOR_TEST: &str = "blocking";

    /// Write a synthetic phase's plan declaring a `blocking-human`
    /// checkpoint task, matching `verify::phase_plan_files`'s discovery
    /// pattern (`.planning/phases/{NN}-*/{NN}-*-PLAN.md`).
    fn write_declared_checkpoint_plan(root: &Path, phase: PhaseId) {
        let dir = root.join(".planning/phases").join(format!(
            "{padded}-checkpoint-fixture",
            padded = phase.padded()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let body = format!(
            "---\nphase: {phase}\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE_FOR_TEST}\">\n</task>\n"
        );
        std::fs::write(
            dir.join(format!("{padded}-01-PLAN.md", padded = phase.padded())),
            body,
        )
        .unwrap();
    }

    /// D-05 (35-02): the decoy PLAN.
    ///
    /// Identical to [`write_declared_checkpoint_plan`] in discovery shape —
    /// same `.planning/phases/{NN}-checkpoint-fixture/{NN}-01-PLAN.md`
    /// location under whatever root it is handed, same front-matter-plus-task
    /// body — but declaring the plain `blocking` gate, so
    /// `phase_has_blocking_human_checkpoint` does not match it.
    ///
    /// Written under `project_root` by the worktree-mode test so that
    /// reverting the call site's root argument fails because the WRONG ROOT
    /// was read, not because the main checkout happened to be empty. The
    /// bare "leave `project_root` empty" alternative also discriminates, but
    /// partly by a condition production never satisfies: a real main checkout
    /// always carries `.planning/phases/`, often including a previous run's
    /// copy of the very phase under test.
    fn write_plan_without_checkpoint(root: &Path, phase: PhaseId) {
        let dir = root.join(".planning/phases").join(format!(
            "{padded}-checkpoint-fixture",
            padded = phase.padded()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let body = format!(
            "---\nphase: {phase}\n---\n\n<task type=\"checkpoint:decision\" gate=\"{PLAIN_GATE_VALUE_FOR_TEST}\">\n</task>\n"
        );
        std::fs::write(
            dir.join(format!("{padded}-01-PLAN.md", padded = phase.padded())),
            body,
        )
        .unwrap();
    }

    /// Write a captured stdout containing BOTH the `**Gate:**
    /// blocking-human` confirmation literal and a `DEVFLOW_RESULT` failed
    /// marker, so `advance()`'s Layer 1 deterministically classifies the
    /// outcome as `Failed` (-> `Action::GateReview`) with no exit-code/pid
    /// file or background thread needed.
    /// A confirmed checkpoint in the current convention: the relayed `Gate:`
    /// block, then a final marker that declares the checkpoint. A plain
    /// failure marker after the gate would be the latest stated outcome and
    /// would not confirm it (operator decision 2026-10-10).
    fn write_confirmed_checkpoint_capture(root: &Path, phase: PhaseId) {
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let stdout = format!(
            "## CHECKPOINT REACHED\n\n**Type:** human-verify\n**Gate:** {HUMAN_GATE_VALUE_FOR_TEST}\n\nDEVFLOW_RESULT: {{\"status\": \"failed\", \"checkpoint\": \"{HUMAN_GATE_VALUE_FOR_TEST}\", \"reason\": \"checkpoint pending\"}}\n"
        );
        std::fs::write(agent_result::stdout_path(root, phase), stdout).unwrap();
    }

    /// A capture whose `DEVFLOW_RESULT` marker fails but never reports the
    /// human-blocking `Gate:` literal — an ordinary failure, not a
    /// checkpoint.
    fn write_unreported_failure_capture(root: &Path, phase: PhaseId) {
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\": \"failed\", \"reason\": \"ordinary failure\"}\n",
        )
        .unwrap();
    }

    /// A capture whose only checkpoint signal is the declared marker field
    /// (49-REVIEW CR-01): no `Gate:` line anywhere, just
    /// `"checkpoint": "blocking-human"` on the final `DEVFLOW_RESULT`.
    fn write_marker_declared_checkpoint_capture(root: &Path, phase: PhaseId) {
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let stdout = format!(
            "Plan 01 stopped at Task 1.\n\nDEVFLOW_RESULT: {{\"status\": \"failed\", \"checkpoint\": \"{HUMAN_GATE_VALUE_FOR_TEST}\", \"reason\": \"Task 1 needs a decision\"}}\n"
        );
        std::fs::write(agent_result::stdout_path(root, phase), stdout).unwrap();
    }

    /// Live Attempt 1's real failure (2026-10-07): the reason describes a
    /// blocking-human halt in prose but declares nothing. The reverted prose
    /// heuristic auto-resumed this; it must now take the ordinary gate.
    fn write_prose_only_checkpoint_capture(root: &Path, phase: PhaseId) {
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\": \"failed\", \"reason\": \"Phase 01 plan 01-01 halted at Task 1, a blocking-human decision checkpoint\"}\n",
        )
        .unwrap();
    }

    /// Pre-write a rejected gate response so `run_gate`'s poll (invoked by
    /// the fall-through path's `handle_stage_failure`) returns immediately
    /// instead of blocking — mirrors
    /// `advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed`'s
    /// fixture pattern.
    fn write_abort_gate_response(root: &Path, phase: PhaseId, stage: Stage) {
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "write_abort_gate_response must run inside a child test with an agent-free PATH (999.80)"
        );
        let response_path = Gates::response_path(root, phase, stage);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();
    }

    /// The `CheckpointApproval` a passing Code preflight would have left for
    /// `execution_root` (48-07, C-10).
    ///
    /// `State::new` now yields `Pending`, which approves nothing, so a
    /// fixture built straight from it parks at the re-scan gate instead of
    /// reaching the auto-decide route. These fixtures are asserting the
    /// auto-decide route, so they must start from the state a Code preflight
    /// actually leaves behind. Derived from the fixture's own plan files
    /// rather than hand-written, so a fixture whose plans change cannot drift
    /// into approving a set it never declared.
    fn recorded_approval_for(
        execution_root: &Path,
        phase: PhaseId,
    ) -> devflow_core::state::CheckpointApproval {
        devflow_core::state::CheckpointApproval::Recorded(
            verify::phase_checkpoint_declarations(execution_root, phase)
                .into_iter()
                .filter(|declaration| declaration.blocking_human || declaration.human_action)
                .map(|declaration| devflow_core::state::ApprovedCheckpoint {
                    plan_file: declaration.plan_file,
                    element: declaration.element,
                })
                .collect(),
        )
    }

    /// The positive case: declared + reported + Claude + session id + under
    /// the ceiling -> resumes and records exactly one audit event, with no
    /// `gate_fired` for this stage.
    #[test]
    fn advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records() {
        const NAME: &str = "pipeline_launch::tests::advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_TEST_ROOT";

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);

            let phase = PhaseId::new(88);
            write_declared_checkpoint_plan(root, phase);
            write_confirmed_checkpoint_capture(root, phase);

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Code;
            state.session_id = Some("sess-checkpoint-1".to_string());
            // C-10 (48-07): the state a passing Code preflight leaves. No
            // assertion below changed.
            state.checkpoint_approval = recorded_approval_for(root, phase);
            workflow::save_state(&state).unwrap();

            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();

        let phase = PhaseId::new(88);
        // Unused on the relaunch route; if detection regresses, the
        // fall-through gate reads it and returns instead of blocking forever.
        write_abort_gate_response(root, phase, Stage::Code);

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        result.unwrap();

        let auto_decided = events_of_kind(root, "checkpoint_auto_decided");
        assert_eq!(
            auto_decided.len(),
            1,
            "expected exactly one checkpoint_auto_decided event: {auto_decided:?}"
        );
        assert_eq!(auto_decided[0]["session_id"], "sess-checkpoint-1");
        assert_eq!(auto_decided[0]["stage"], "code");

        let gate_fired = events_of_kind(root, "gate_fired");
        assert!(
            gate_fired.iter().all(|e| e["stage"] != "code"),
            "a confirmed, auto-resolved checkpoint must never also fire the \
             generic gate for the same stage: {gate_fired:?}"
        );
    }

    #[test]
    fn checkpoint_added_after_code_preflight_parks_at_the_rescan_gate() {
        const NAME: &str = "pipeline_launch::tests::checkpoint_added_after_code_preflight_parks_at_the_rescan_gate";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_ROOT";

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);

            let phase = PhaseId::new(96);
            write_declared_checkpoint_plan(root, phase);
            write_confirmed_checkpoint_capture(root, phase);

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Code;
            state.session_id = Some("sess-checkpoint-rescan".to_string());
            workflow::save_state(&state).unwrap();

            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();
        let phase = PhaseId::new(96);
        write_abort_gate_response(root, phase, Stage::Code);

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let auto_decided = events_of_kind(root, "checkpoint_auto_decided");
        assert!(
            auto_decided.is_empty(),
            "a checkpoint added after Code preflight must not auto-decide: {auto_decided:?}"
        );
        let gate_fired = events_of_kind(root, "gate_fired");
        assert!(
            gate_fired.iter().any(|event| event["stage"] == "code"),
            "the added checkpoint must park at a Code gate: {gate_fired:?}"
        );
    }

    // -----------------------------------------------------------------
    // 48-07 Task 2 (CHKPT-02, D-07): new, changed and unrecorded sets gate;
    // an unchanged approved set still auto-decides.
    //
    // All four tests below build through ONE fixture builder
    // (`build_rescan_fixture`). Exactly two things vary: the plan text on
    // disk and the `CheckpointApproval` the state starts from. That is what
    // makes the control meaningful — the control passing while a gating test
    // fails cannot be a fixture difference, because there is only one
    // fixture.
    // -----------------------------------------------------------------

    /// A REAL plan task element, copied from
    /// `19-release-integrity-main-rs-decomposition/19-05-PLAN.md` (its
    /// `<tasks>` section). Real text rather than a minimal stub because the
    /// comparison rule is about normalized element BODIES: a synthetic
    /// one-line task would pass a byte-compare and a whitespace-normalizing
    /// compare identically, and so could not tell them apart.
    fn rescan_plan_body() -> String {
        format!(
            "---\nphase: 19\n---\n\n\
             <task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE_FOR_TEST}\">\n  \
             <name>Task 1: Dogfood the acceptance contract against a non-compliant diff</name>\n\n  \
             <action>\n    \
             Prepare the scratch diffs described in `<what-built>` in the working tree —\n    \
             Diff A (non-compliant: assert-constant-against-itself), Diff C\n    \
             (non-compliant: reproduces the production algorithm inside the test), and\n    \
             Diff B (a compliant control) — then run the review against each in turn.\n  \
             </action>\n\
             </task>\n"
        )
    }

    /// The same element with trailing whitespace added to two lines and CRs
    /// on another — the one edit 48-02's normalization is specified to
    /// ignore. Used by the CONTROL, so the control asserts normalization and
    /// the auto-decide path together.
    fn rescan_plan_body_whitespace_only_edit() -> String {
        rescan_plan_body()
            .replace("</name>\n", "</name>   \n")
            .replace("<action>\n", "<action>\t\n")
            .replace("</action>\n", "</action>  \r\n")
    }

    /// A REAL body edit: one word of prose changed inside the element.
    fn rescan_plan_body_rewritten() -> String {
        rescan_plan_body().replace(
            "Diff B (a compliant control)",
            "Diff B (a compliant control, now rewritten by the agent)",
        )
    }

    fn rescan_plan_file_name(phase: PhaseId) -> String {
        format!("{padded}-01-PLAN.md", padded = phase.padded())
    }

    /// The `CheckpointApproval` a Code preflight would have left after seeing
    /// `body` — derived by running 48-02's own parser over it, never
    /// hand-written, so a recorded set cannot drift from what the scanner
    /// actually produces.
    fn recorded_from_body(phase: PhaseId, body: &str) -> devflow_core::state::CheckpointApproval {
        devflow_core::state::CheckpointApproval::Recorded(
            verify::parse_checkpoint_declarations(&rescan_plan_file_name(phase), body)
                .into_iter()
                .filter(|declaration| declaration.blocking_human || declaration.human_action)
                .map(|declaration| devflow_core::state::ApprovedCheckpoint {
                    plan_file: declaration.plan_file,
                    element: declaration.element,
                })
                .collect(),
        )
    }

    fn write_rescan_plan(root: &Path, phase: PhaseId, body: &str) {
        let dir = root.join(".planning/phases").join(format!(
            "{padded}-checkpoint-fixture",
            padded = phase.padded()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(rescan_plan_file_name(phase)), body).unwrap();
    }

    const RESCAN_SESSION_ID: &str = "sess-checkpoint-rescan-shared";

    /// THE shared fixture builder. Everything that reaches the resume
    /// decision is identical across the four tests except `plan_body` and
    /// `approval`.
    fn build_rescan_fixture(
        root: &Path,
        phase: PhaseId,
        plan_body: &str,
        approval: devflow_core::state::CheckpointApproval,
        mode: Mode,
    ) {
        init_repo(root);
        write_rescan_plan(root, phase, plan_body);
        write_confirmed_checkpoint_capture(root, phase);

        let mut state = State::new(phase, AgentKind::Claude, mode, root.to_path_buf());
        state.stage = Stage::Code;
        // A session id AND an unexhausted ceiling, so every one of these
        // tests lands in the `(Some(session_id), true)` arm where the
        // compare lives. Without both, they would exercise a fall-through
        // that gates for an entirely different reason.
        state.session_id = Some(RESCAN_SESSION_ID.to_string());
        state.checkpoint_approval = approval;
        // A run that has already launched Code once has recorded its delivery
        // canary. Without this, any path that reaches `launch_stage` — the
        // rejection path does, via `handle_stage_failure`'s LoopBack arm —
        // refuses because the stub agent cannot return a notification token,
        // and the test fails on the canary instead of on the behaviour under
        // test. Same fixture shape other launch tests in this module use.
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        workflow::save_state(&state).unwrap();
    }

    /// The child half of all four tests: run `advance` and return the two
    /// event streams the assertions read.
    ///
    /// Pre-writes an abort response for EVERY case, including the control.
    /// For the control it is inert (that path never opens a gate), and it is
    /// there so a control that REGRESSES into gating fails on an assertion
    /// instead of blocking on a three-day gate poll — an unbounded hang
    /// cannot be told apart from a wedged harness.
    fn run_rescan_advance(
        root: &Path,
        phase: PhaseId,
    ) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
        write_abort_gate_response(root, phase, Stage::Code);

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        (
            events_of_kind(root, "checkpoint_auto_decided"),
            events_of_kind(root, "gate_fired"),
        )
    }

    fn assert_parked_at_rescan_gate(
        auto_decided: &[serde_json::Value],
        gate_fired: &[serde_json::Value],
    ) {
        assert!(
            auto_decided.is_empty(),
            "an unapproved checkpoint must not auto-decide: {auto_decided:?}"
        );
        assert!(
            gate_fired.iter().any(|event| event["stage"] == "code"),
            "an unapproved checkpoint must park at a Code gate: {gate_fired:?}"
        );
    }

    /// Write an arbitrary gate response so a gate's poll returns immediately.
    ///
    /// `GateAction::from_response` maps these three shapes: approved ->
    /// `Advance`; not approved with a note containing "abort" -> `Abort`;
    /// anything else not approved -> `LoopBack`.
    fn write_gate_response(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        approved: bool,
        note: Option<&str>,
    ) {
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "write_gate_response must run inside a child test with an agent-free PATH (999.80)"
        );
        let response_path = Gates::response_path(root, phase, stage);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        let body = match note {
            Some(note) => serde_json::json!({
                "approved": approved, "note": note, "responded_by": "test"
            }),
            None => serde_json::json!({ "approved": approved, "responded_by": "test" }),
        };
        std::fs::write(&response_path, serde_json::to_vec(&body).unwrap()).unwrap();
    }

    /// 48-15 Task 1 / R-8: approving the re-scan gate must make the approval
    /// DURABLE — record the set the human just saw, then relaunch — so the
    /// very next resume of an unchanged plan auto-decides without asking
    /// again. A gate that reopens on every resume is one an operator learns
    /// to answer without reading.
    #[test]
    fn approving_the_rescan_gate_records_the_set_and_relaunches() {
        const NAME: &str =
            "pipeline_launch::tests::approving_the_rescan_gate_records_the_set_and_relaunches";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_APPROVE_ROOT";
        let phase = PhaseId::new(101);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            // Approved: the ORIGINAL body. On disk: the REWRITTEN one, so the
            // compare finds one unapproved declaration and opens the gate.
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_rewritten(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Auto,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();

        write_gate_response(root, phase, Stage::Code, true, None);

        let first = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded_first = workflow::load_state(root, phase).ok();
        let _reap_first = reloaded_first.as_ref().map(ReapMonitorOnDrop::after_launch);
        first.unwrap();

        // The approval is recorded as the set now ON DISK, not as the stale
        // recorded set the gate was opened about.
        let after_approval = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            after_approval.checkpoint_approval,
            recorded_from_body(phase, &rescan_plan_body_rewritten()),
            "approving must record the freshly scanned set, persisted for the next process"
        );
        assert_eq!(
            events_of_kind(root, "checkpoint_auto_decided").len(),
            1,
            "approving must reach the relaunch path"
        );
        let gates_after_first = events_of_kind(root, "gate_fired")
            .into_iter()
            .filter(|event| event["stage"] == "code")
            .count();
        assert_eq!(
            gates_after_first, 1,
            "exactly one re-scan gate should have fired"
        );

        // DURABILITY, asserted as the predicate the next resume evaluates
        // rather than by driving a second `advance()`.
        //
        // A second real advance cannot measure this in this fixture: the
        // relaunch above spawns the stub agent, whose own stdout REPLACES the
        // confirmed-checkpoint capture, so the next advance classifies a
        // different result entirely and never reaches the re-scan route. That
        // is a fixture artifact, not behaviour — measured, not assumed (it
        // produced a never-silent gate on an unrelated path).
        //
        // The composition is still covered end to end:
        // `unchanged_recorded_checkpoint_still_auto_decides` (48-07) drives a
        // REAL advance with a matching recorded set and asserts auto-decide
        // with no gate. This test supplies the other half — that an approval
        // produces exactly such a matching set, on disk, for the separate
        // process that reads it next.
        let current_on_disk = verify::phase_checkpoint_declarations(root, phase);
        assert!(
            after_approval
                .checkpoint_approval
                .unapproved(&current_on_disk)
                .is_empty(),
            "after approval the next resume's compare must find nothing to gate"
        );
        // Negative control: the same predicate against the pre-approval set
        // MUST still find the declaration, or the assertion above would pass
        // for a set that approves everything.
        assert_eq!(
            recorded_from_body(phase, &rescan_plan_body())
                .unapproved(&current_on_disk)
                .len(),
            1,
            "the pre-approval set must still leave one declaration unapproved"
        );
    }

    /// D-03b: a Supervise operator may approve the re-scan's new declaration
    /// set, but that approval is not permission for Claude to decide the
    /// checkpoint. It records the set, then opens the ordinary Code gate.
    #[test]
    fn supervise_rescan_approval_records_but_requires_a_human_gate() {
        const NAME: &str =
            "pipeline_launch::tests::supervise_rescan_approval_records_but_requires_a_human_gate";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_SUPERVISE_RESCAN_ROOT";
        let phase = PhaseId::new(111);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_rewritten(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Supervise,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();
        let expected_approval = recorded_from_body(phase, &rescan_plan_body_rewritten());
        write_gate_response(root, phase, Stage::Code, true, None);

        let writer_root = root.to_path_buf();
        let writer = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                if workflow::load_state(&writer_root, phase)
                    .is_ok_and(|state| state.checkpoint_approval == expected_approval)
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "approval was not persisted before the human Code gate"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            write_gate_response(&writer_root, phase, Stage::Code, false, Some("abort"));
        });

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        writer.join().expect("second-gate response writer");
        result.unwrap();

        workflow::load_state(root, phase)
            .expect_err("the abort response must end the supervised run");
        assert!(
            events_of_kind(root, "checkpoint_auto_decided").is_empty(),
            "Supervise re-scan approval must never auto-decide"
        );
        let code_gates = events_of_kind(root, "gate_fired")
            .into_iter()
            .filter(|event| event["stage"] == "code")
            .count();
        assert_eq!(code_gates, 2, "re-scan plus human-decision gates must fire");
    }

    /// An Auto-mode rejection cannot reuse the ordinary failure loop-back: the
    /// next unattended Code preflight correctly refuses the still-unapproved
    /// human-only checkpoint. Park the phase in Supervise before cleaning the
    /// response. The bounded writer feeds an abort only if the pre-fix third
    /// gate appears; its marker and the exact one-gate assertion below make
    /// both directions observable rather than treating a short run as proof.
    #[test]
    fn auto_rescan_rejection_parks_for_supervised_repair() {
        const NAME: &str =
            "pipeline_launch::tests::auto_rescan_rejection_parks_for_supervised_repair";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_AUTO_RESCAN_REJECT_ROOT";
        let phase = PhaseId::new(113);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_rewritten(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Auto,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();
        let before = workflow::load_state(root, phase)
            .unwrap()
            .checkpoint_approval;
        let response_path = Gates::response_path(root, phase, Stage::Code);
        write_gate_response(
            root,
            phase,
            Stage::Code,
            false,
            Some("the rewritten task needs supervised repair"),
        );

        let writer_root = root.to_path_buf();
        let writer_response_path = response_path.clone();
        let third_gate_response_written =
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer_third_gate_response_written =
            std::sync::Arc::clone(&third_gate_response_written);
        let writer = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                // Check the response before the state. The park saves
                // `stopped` before it removes the response, so a response the
                // park removed is always seen with `stopped` set. Reading the
                // state first let a whole park land between the two reads.
                let response_gone = !writer_response_path.exists();
                let state = workflow::load_state(&writer_root, phase);
                if state.as_ref().is_ok_and(|state| state.stopped) {
                    return;
                }
                if response_gone {
                    write_gate_response(&writer_root, phase, Stage::Code, false, Some("abort"));
                    writer_third_gate_response_written
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "the re-scan response was not consumed"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();
        writer.join().expect("third-gate negative-control writer");

        let after = workflow::load_state(root, phase)
            .expect("Auto rejection must park the phase instead of aborting it");
        assert_eq!(after.mode, Mode::Supervise);
        assert!(after.stopped, "the automatic run must be parked");
        assert!(
            after
                .stop_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("switched to Supervise")),
            "the parked reason must explain why Auto did not retry: {:?}",
            after.stop_reason
        );
        assert_eq!(
            after.checkpoint_approval, before,
            "a rejection must not bless the changed declaration"
        );
        let parked_code_gates = events_of_kind(root, "gate_fired")
            .into_iter()
            .filter(|event| event["stage"] == "code")
            .count();
        assert_eq!(
            parked_code_gates, 1,
            "only the re-scan gate may fire before supervised resume; a second gate is the \
             pre-fix unattended loopback, not a successful park"
        );
        assert!(
            !third_gate_response_written.load(std::sync::atomic::Ordering::SeqCst),
            "the negative-control writer must stay inert unless the old third gate appears"
        );
        assert!(
            events_of_kind(root, "checkpoint_auto_decided").is_empty(),
            "a rejection must never relaunch Claude's checkpoint decision"
        );
        assert_eq!(events_of_kind(root, "checkpoint_repair_parked").len(), 1);
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists()
                && !response_path.exists()
                && !Gates::ack_path(root, phase, Stage::Code).exists(),
            "parking must leave no stale gate protocol files"
        );

        resume(root, phase, None, false)
            .expect("the parked phase must resume through the supervised path");
        let resumed = workflow::load_state(root, phase)
            .expect("Supervise resume must leave a live state for its monitor");
        let _reap_guard = ReapMonitorOnDrop::after_launch(&resumed);
        assert_eq!(resumed.mode, Mode::Supervise);
        assert_eq!(
            resumed.checkpoint_approval, before,
            "a supervised repair launch must not reinterpret the rejected declaration as approved"
        );
        assert!(
            !resumed.stopped,
            "resume clears the park only after launching the supervised continuation"
        );
        assert!(
            events_of_kind(root, "stage_launched")
                .iter()
                .any(|event| event["stage"] == "code"),
            "resume must actually launch the parked Code stage"
        );
    }

    /// 48-15 Task 1 / T-48-15-03: a non-abort rejection must record NOTHING.
    /// Widening the approved set on a rejection is the repudiation path — the
    /// human said no and the next resume would proceed as though they had
    /// said yes. It also must not relaunch, and the plan file it objected to
    /// has to appear in what the fall-through gate shows the operator.
    #[test]
    fn rejecting_the_rescan_gate_records_nothing_and_falls_through() {
        const NAME: &str =
            "pipeline_launch::tests::rejecting_the_rescan_gate_records_nothing_and_falls_through";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_REJECT_ROOT";
        let phase = PhaseId::new(102);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            // SUPERVISE, and the reason is load-bearing. The rejection path
            // ends in `handle_stage_failure`'s LoopBack arm, which calls
            // `launch_stage` -> `run_preflight`. In AUTO,
            // `preflight_unattended_launch_check` refuses that launch
            // precisely BECAUSE the plan declares a human-only checkpoint, so
            // a third gate opens with no response left and the test blocks on
            // the three-day poll instead of failing. Measured, not assumed:
            // the Auto run printed `[DOES NOT HOLD] no plan declares a
            // human-only checkpoint` and spawned zero monitors.
            //
            // The route under test has no mode check (D-07), and nothing in
            // the rejection path reads `mode`, so Supervise exercises the
            // same code with the downstream refusal out of the way.
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_rewritten(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Supervise,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();

        let before = workflow::load_state(root, phase)
            .unwrap()
            .checkpoint_approval;

        // Not approved, and the note deliberately does NOT say "abort" —
        // `GateAction::from_response` maps that to LoopBack, the "I will fix
        // the plan" answer. The fall-through gate below consumes this same
        // response, which is correct: it is the same question.
        write_gate_response(
            root,
            phase,
            Stage::Code,
            false,
            Some("the rewritten task needs review first"),
        );

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded.as_ref().map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let after = workflow::load_state(root, phase)
            .unwrap()
            .checkpoint_approval;
        assert_eq!(
            after, before,
            "a rejection must leave the approved set exactly as it was"
        );
        assert!(
            events_of_kind(root, "checkpoint_auto_decided").is_empty(),
            "a rejection must never reach the relaunch path"
        );

        // A rejection must REACH the existing non-auto-decide dispatch, not
        // stop at the re-scan gate. `handle_stage_failure` is the only thing
        // that emits a `[never-silent]` context, and the augmented reason it
        // renders must name the plan file the human objected to — that gate
        // is where they will actually read it.
        //
        // Asserting on the never-silent context specifically, NOT merely on
        // "some gate mentions the plan file": the re-scan gate's own context
        // already names it, so the weaker assertion passes without any of
        // this plan's changes and would measure nothing.
        let plan_file = rescan_plan_file_name(phase);
        let gate_contexts: Vec<String> = events_of_kind(root, "gate_fired")
            .iter()
            .filter_map(|event| event["context"].as_str().map(str::to_string))
            .collect();
        assert!(
            gate_contexts.iter().any(|context| {
                context.contains("[never-silent]")
                    && context.contains("could not auto-resolve")
                    && context.contains(&plan_file)
            }),
            "the fall-through gate must name `{plan_file}` in an augmented \
             unresolved-checkpoint reason; contexts were {gate_contexts:?}"
        );
        // At least two: the re-scan gate AND the never-silent fall-through.
        // Deliberately not an EXACT count — the LoopBack arm relaunches the
        // stage, and that monitor's own background `devflow advance` can fire
        // further gates on its own schedule. Pinning the count would make this
        // test fail on a race rather than on the behaviour it asserts; the
        // never-silent context check above is what discriminates.
        let code_gates = events_of_kind(root, "gate_fired")
            .into_iter()
            .filter(|event| event["stage"] == "code")
            .count();
        assert!(
            code_gates >= 2,
            "a rejection fires the re-scan gate AND then the never-silent \
             fall-through gate; {code_gates} gate(s) means the dispatch was \
             not reached"
        );
    }

    /// 48-15 Task 1: an abort note must terminate through the SAME
    /// `pipeline_gate::abort` every other gate uses — not a bespoke exit
    /// path that skips gate cleanup, state clearing or deregistration.
    #[test]
    fn rejecting_the_rescan_gate_with_abort_aborts() {
        const NAME: &str = "pipeline_launch::tests::rejecting_the_rescan_gate_with_abort_aborts";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_ABORT_ROOT";
        let phase = PhaseId::new(103);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_rewritten(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Auto,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();

        write_gate_response(
            root,
            phase,
            Stage::Code,
            false,
            Some("abort: the agent rewrote its own gate"),
        );

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded.as_ref().map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        assert!(
            events_of_kind(root, "checkpoint_auto_decided").is_empty(),
            "an abort must never relaunch the session"
        );
        assert_eq!(
            events_of_kind(root, "workflow_aborted").len(),
            1,
            "an abort note must terminate through pipeline_gate::abort"
        );
        assert!(
            workflow::load_state(root, phase).is_err(),
            "abort clears state; a surviving state file means a bespoke exit path was taken"
        );
    }

    /// A rewritten body of an ALREADY-APPROVED checkpoint is new relative to
    /// the recorded set. The recorded set holds the original element; the
    /// plan on disk holds an edited one.
    #[test]
    fn rewritten_body_of_an_approved_checkpoint_parks_at_the_rescan_gate() {
        const NAME: &str = "pipeline_launch::tests::rewritten_body_of_an_approved_checkpoint_parks_at_the_rescan_gate";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_REWRITTEN_ROOT";
        let phase = PhaseId::new(97);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            // Approved: the ORIGINAL body. On disk: the REWRITTEN one.
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_rewritten(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Auto,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let (auto_decided, gate_fired) = run_rescan_advance(root.as_path(), phase);
        assert_parked_at_rescan_gate(&auto_decided, &gate_fired);
    }

    /// `Unrecorded` — a state file from a binary predating the field —
    /// approves nothing, so a declared checkpoint parks.
    #[test]
    fn unrecorded_checkpoint_set_parks_at_the_rescan_gate() {
        const NAME: &str =
            "pipeline_launch::tests::unrecorded_checkpoint_set_parks_at_the_rescan_gate";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_UNRECORDED_ROOT";
        let phase = PhaseId::new(98);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body(),
                devflow_core::state::CheckpointApproval::Unrecorded,
                Mode::Auto,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let (auto_decided, gate_fired) = run_rescan_advance(root.as_path(), phase);
        assert_parked_at_rescan_gate(&auto_decided, &gate_fired);
    }

    /// `Pending` — this binary created the state but no Code evaluation ever
    /// recorded a set. Fails if `Pending` is read as approval.
    #[test]
    fn pending_checkpoint_set_parks_at_the_rescan_gate() {
        const NAME: &str =
            "pipeline_launch::tests::pending_checkpoint_set_parks_at_the_rescan_gate";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_PENDING_ROOT";
        let phase = PhaseId::new(99);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body(),
                // Left exactly where `State::new` puts it.
                devflow_core::state::CheckpointApproval::Pending,
                Mode::Auto,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let (auto_decided, gate_fired) = run_rescan_advance(root.as_path(), phase);
        assert_parked_at_rescan_gate(&auto_decided, &gate_fired);
    }

    /// THE CONTROL, and the opposite-result case for all three above: a
    /// recorded set that still matches must auto-decide exactly as before
    /// this plan existed — no gate, one `checkpoint_auto_decided`, relaunch
    /// reached.
    ///
    /// The plan on disk differs from the recorded body by TRAILING
    /// WHITESPACE and a CR only. That is deliberate: it makes the control
    /// also the normalization test. A byte-exact compare would gate here,
    /// and gating on a whitespace-only edit is how this feature becomes
    /// noise an operator learns to click through.
    #[test]
    fn unchanged_recorded_checkpoint_still_auto_decides() {
        const NAME: &str =
            "pipeline_launch::tests::unchanged_recorded_checkpoint_still_auto_decides";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_CONTROL_ROOT";
        let phase = PhaseId::new(100);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_whitespace_only_edit(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Auto,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let (auto_decided, gate_fired) = run_rescan_advance(root.as_path(), phase);
        assert_eq!(
            auto_decided.len(),
            1,
            "an unchanged recorded set must still auto-decide exactly once: {auto_decided:?}"
        );
        assert_eq!(auto_decided[0]["session_id"], RESCAN_SESSION_ID);
        assert!(
            !gate_fired.iter().any(|event| event["stage"] == "code"),
            "an unchanged recorded set must not open a Code gate: {gate_fired:?}"
        );
    }

    /// D-03b: the unattended exception belongs to Auto only. A user who
    /// selected Supervise must see a planner-declared human checkpoint as a
    /// Code gate; recording it does not authorize DevFlow to resume Claude.
    ///
    /// `unchanged_recorded_checkpoint_still_auto_decides` is the opposite
    /// control: it keeps D-03's original liveness guarantee for Auto.
    #[test]
    fn supervise_unchanged_recorded_checkpoint_requires_human_gate() {
        const NAME: &str =
            "pipeline_launch::tests::supervise_unchanged_recorded_checkpoint_requires_human_gate";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_SUPERVISE_CHECKPOINT_ROOT";
        let phase = PhaseId::new(110);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            build_rescan_fixture(
                root,
                phase,
                &rescan_plan_body_whitespace_only_edit(),
                recorded_from_body(phase, &rescan_plan_body()),
                Mode::Supervise,
            );
            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let (auto_decided, gate_fired) = run_rescan_advance(root.as_path(), phase);
        assert!(
            auto_decided.is_empty(),
            "Supervise must not auto-decide a human checkpoint: {auto_decided:?}"
        );
        assert!(
            gate_fired.iter().any(|event| event["stage"] == "code"),
            "Supervise must open a Code gate for a human checkpoint: {gate_fired:?}"
        );
    }

    /// 999.84 / HARDEN-04 (35-02): the WORKTREE-MODE sibling of the test
    /// above, which is deliberately left byte-unchanged rather than moved
    /// under a worktree — extending it in place would have deleted the only
    /// call-site-level coverage of the no-worktree path, trading one gap for
    /// another.
    ///
    /// Same five preconditions, three differences: `state.worktree_path` is
    /// set, the `blocking-human` PLAN exists ONLY inside that worktree, and
    /// `project_root` carries a DECOY PLAN for the same phase declaring no
    /// blocking-human gate (D-05).
    ///
    /// Reverting `Action::GateReview`'s
    /// `verify::phase_has_blocking_human_checkpoint` argument from
    /// `execution_root` back to `project_root` makes this test fail: the arm
    /// finds only the decoy, never confirms the checkpoint, and falls through
    /// to the generic gate instead of auto-deciding. The sibling above keeps
    /// PASSING under that same revert (no worktree, PLAN at the root), which
    /// localises the failure to root selection rather than to checkpoint
    /// machinery in general. **That revert was performed and the failure
    /// observed** (35-02 SUMMARY records both outputs verbatim).
    ///
    /// D-06: the performed revert is a one-time act nothing re-runs, so the
    /// mechanical opposite-result control below ships inside this test and
    /// re-runs on every `cargo test`. The two establish different things and
    /// neither replaces the other — the control proves the two roots
    /// DISAGREE for this fixture, which is what makes the revert meaningful;
    /// only the performed revert establishes that the call site passes the
    /// execution root.
    #[test]
    fn advance_with_worktree_declared_checkpoint_reads_the_execution_root() {
        const NAME: &str = "pipeline_launch::tests::advance_with_worktree_declared_checkpoint_reads_the_execution_root";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(94);
        // D-05: a plain directory, not a real `git worktree add`. The
        // argument under test resolves a path, and a linked worktree's files
        // are ordinary files — `monitor::spawn_monitor`'s own worktree test
        // uses the same plain-directory fixture.
        let worktree = root.join("phase-worktree");
        std::fs::create_dir_all(&worktree).unwrap();

        write_declared_checkpoint_plan(&worktree, phase);
        write_plan_without_checkpoint(root, phase);
        write_confirmed_checkpoint_capture(root, phase);
        // Inert on the path this test asserts — the auto-decide arm returns
        // before `run_gate` is ever reached, so nothing reads this response.
        // It exists so that the REVERTED form of the call site (the D-06
        // demonstration) produces a real assertion failure instead of
        // hanging: falling through to the never-silent gate makes `run_gate`
        // poll for an operator response that no test will ever write, and an
        // unbounded hang cannot distinguish a failed assertion from a wedged
        // harness. The three negative siblings below pre-write it for the
        // same reason.
        write_abort_gate_response(root, phase, Stage::Code);

        // D-06's mechanical control, asserted BEFORE `advance()` so a fixture
        // that has stopped discriminating reports as a fixture failure rather
        // than as a checkpoint-machinery failure.
        assert!(
            verify::phase_has_blocking_human_checkpoint(&worktree, phase),
            "the execution root holds the declaring PLAN, so the declaration must be found"
        );
        assert!(
            !verify::phase_has_blocking_human_checkpoint(root, phase),
            "opposite-result case: the project root holds ONLY the decoy, which declares \
             no blocking-human gate, so it must return false — if both roots answered the \
             same, this fixture would be measuring the presence of a PLAN somewhere rather \
             than which root the call site reads"
        );

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-checkpoint-worktree".to_string());
        state.worktree_path = Some(worktree.clone());
        // C-10 (48-07): recorded from the WORKTREE, the execution root this
        // test exists to pin. Recording from `root` instead would approve the
        // decoy's empty set and this test would park. No assertion changed.
        state.checkpoint_approval = recorded_approval_for(&worktree, phase);
        workflow::save_state(&state).unwrap();

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        result.unwrap();

        let auto_decided = events_of_kind(root, "checkpoint_auto_decided");
        assert_eq!(
            auto_decided.len(),
            1,
            "expected exactly one checkpoint_auto_decided event — the declaration lives \
             in the worktree, so the arm must read the EXECUTION root: {auto_decided:?}"
        );
        assert_eq!(auto_decided[0]["session_id"], "sess-checkpoint-worktree");
        assert_eq!(auto_decided[0]["stage"], "code");

        let gate_fired = events_of_kind(root, "gate_fired");
        assert!(
            gate_fired.iter().all(|e| e["stage"] != "code"),
            "a confirmed, auto-resolved checkpoint must never also fire the \
             generic gate for the same stage: {gate_fired:?}"
        );
    }

    /// D-01 primary-gate proof (T-28-01): the SAME reported capture, but no
    /// plan for this phase declares a checkpoint at all -> must fall through
    /// to the ordinary never-silent gate. Zero checkpoint_auto_decided, at
    /// least one gate_fired.
    #[test]
    fn advance_without_declared_checkpoint_falls_through_to_generic_gate() {
        const NAME: &str = "pipeline_launch::tests::advance_without_declared_checkpoint_falls_through_to_generic_gate";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(89);
        // Deliberately no write_declared_checkpoint_plan call.
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-should-not-resume".to_string());
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

        assert!(
            events_of_kind(root, "checkpoint_auto_decided").is_empty(),
            "a phase whose plans never declared a checkpoint must never auto-resume, \
             even if its capture LOOKS like it reported one"
        );
        assert!(
            !events_of_kind(root, "gate_fired").is_empty(),
            "the ordinary never-silent gate must still fire"
        );
    }

    /// Declared, but the capture does not confirm it — an ordinary failure
    /// in a phase that happens to declare a checkpoint elsewhere.
    /// The declared marker field alone confirms the checkpoint: same route and
    /// audit trail as a reported `Gate:` line (49-REVIEW CR-01).
    #[test]
    fn advance_with_declared_checkpoint_and_marker_field_relaunches_and_records() {
        const NAME: &str = "pipeline_launch::tests::advance_with_declared_checkpoint_and_marker_field_relaunches_and_records";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_TEST_ROOT";

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);

            let phase = PhaseId::new(186);
            write_declared_checkpoint_plan(root, phase);
            write_marker_declared_checkpoint_capture(root, phase);

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Code;
            state.session_id = Some("sess-marker-field".to_string());
            state.checkpoint_approval = recorded_approval_for(root, phase);
            workflow::save_state(&state).unwrap();

            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();
        let phase = PhaseId::new(186);
        // Unused on the relaunch route; if detection regresses, the
        // fall-through gate reads it and returns instead of blocking forever,
        // so the assertions below fail rather than hang.
        write_abort_gate_response(root, phase, Stage::Code);

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        result.unwrap();

        let auto_decided = events_of_kind(root, "checkpoint_auto_decided");
        assert_eq!(
            auto_decided.len(),
            1,
            "a declared marker checkpoint must auto-decide exactly once: {auto_decided:?}"
        );
        assert_eq!(auto_decided[0]["session_id"], "sess-marker-field");
        let gate_fired = events_of_kind(root, "gate_fired");
        assert!(
            gate_fired.iter().all(|e| e["stage"] != "code"),
            "an auto-resolved checkpoint must not also fire the generic gate: {gate_fired:?}"
        );
    }

    /// The capture route alone confirms a Code checkpoint: a plain failure
    /// marker, then a `Gate:` line as the latest stated outcome. The evaluated
    /// result declares nothing, so only the capture scan can relaunch here
    /// (round-3 external review: every positive test also declared the field,
    /// so dropping the capture route left the suite green).
    #[test]
    fn advance_with_declared_checkpoint_and_gate_line_only_relaunches_and_records() {
        const NAME: &str = "pipeline_launch::tests::advance_with_declared_checkpoint_and_gate_line_only_relaunches_and_records";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_TEST_ROOT";

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);

            let phase = PhaseId::new(191);
            write_declared_checkpoint_plan(root, phase);
            std::fs::create_dir_all(root.join(".devflow")).unwrap();
            std::fs::write(
                agent_result::stdout_path(root, phase),
                format!(
                    "DEVFLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"checkpoint pending\"}}\n## CHECKPOINT REACHED\n\n**Gate:** {HUMAN_GATE_VALUE_FOR_TEST}\n"
                ),
            )
            .unwrap();

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Code;
            state.session_id = Some("sess-gate-line-only".to_string());
            state.checkpoint_approval = recorded_approval_for(root, phase);
            workflow::save_state(&state).unwrap();

            let path_dir = agent_free_dir_with_agent_stub("claude");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(ROOT_ENV, root.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let root = PathBuf::from(
            std::env::var_os(ROOT_ENV)
                .expect("child test must receive its parent-built fixture root"),
        );
        let root = root.as_path();
        let phase = PhaseId::new(191);
        // Unused on the relaunch route; if detection regresses, the
        // fall-through gate reads it and returns instead of blocking forever.
        write_abort_gate_response(root, phase, Stage::Code);

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        result.unwrap();

        let auto_decided = events_of_kind(root, "checkpoint_auto_decided");
        assert_eq!(
            auto_decided.len(),
            1,
            "a gate line as the latest stated outcome must auto-decide exactly once: {auto_decided:?}"
        );
        assert_eq!(auto_decided[0]["session_id"], "sess-gate-line-only");
        assert!(
            reloaded_for_reap
                .as_ref()
                .is_some_and(|reloaded| reloaded.supervisor.is_some()),
            "the checkpoint must relaunch the session, not only record the event"
        );
        let gate_fired = events_of_kind(root, "gate_fired");
        assert!(
            gate_fired.iter().all(|e| e["stage"] != "code"),
            "an auto-resolved checkpoint must not also fire the generic gate: {gate_fired:?}"
        );
    }

    /// 49-REVIEW CR-01 at the pipeline: a failure whose reason only describes
    /// a blocking-human halt takes the ordinary gate, even in a phase that
    /// declares such a checkpoint.
    #[test]
    fn advance_with_declared_checkpoint_and_prose_only_reason_falls_through() {
        const NAME: &str = "pipeline_launch::tests::advance_with_declared_checkpoint_and_prose_only_reason_falls_through";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(187);
        write_declared_checkpoint_plan(root, phase);
        write_prose_only_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-prose-only".to_string());
        state.checkpoint_approval = recorded_approval_for(root, phase);
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }

    /// Only the Code stage runs plans, so only Code may declare a checkpoint
    /// through the marker field. A Plan failure carrying the same marker takes
    /// the ordinary gate (external review 2026-10-09: the arm has no stage
    /// guard, and Ship or Plan would otherwise be resumed to "resolve" a gate).
    #[test]
    fn advance_with_marker_declared_checkpoint_outside_code_falls_through() {
        const NAME: &str = "pipeline_launch::tests::advance_with_marker_declared_checkpoint_outside_code_falls_through";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(188);
        write_declared_checkpoint_plan(root, phase);
        write_marker_declared_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Plan);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.session_id = Some("sess-plan-declared".to_string());
        state.checkpoint_approval = recorded_approval_for(root, phase);
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Plan)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }

    /// The whole checkpoint route is Code-only, the `Gate:` line included: a
    /// Plan failure whose capture declares the gate takes the ordinary gate
    /// (second external review round, 2026-10-10).
    #[test]
    fn advance_with_gate_line_checkpoint_outside_code_falls_through() {
        const NAME: &str =
            "pipeline_launch::tests::advance_with_gate_line_checkpoint_outside_code_falls_through";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(190);
        write_declared_checkpoint_plan(root, phase);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            format!("## CHECKPOINT REACHED\n\n**Gate:** {HUMAN_GATE_VALUE_FOR_TEST}\n"),
        )
        .unwrap();
        write_abort_gate_response(root, phase, Stage::Plan);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.session_id = Some("sess-plan-gate-line".to_string());
        state.checkpoint_approval = recorded_approval_for(root, phase);
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Plan)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }

    /// The field must name `blocking-human` exactly: a Code failure that
    /// declares some other checkpoint takes the ordinary gate. Catches a
    /// pipeline that tests only for the field's presence.
    #[test]
    fn advance_with_other_marker_checkpoint_value_falls_through() {
        const NAME: &str =
            "pipeline_launch::tests::advance_with_other_marker_checkpoint_value_falls_through";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(189);
        write_declared_checkpoint_plan(root, phase);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\": \"failed\", \"checkpoint\": \"human-verify\", \"reason\": \"Task 2 needs a look\"}\n",
        )
        .unwrap();
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-other-value".to_string());
        state.checkpoint_approval = recorded_approval_for(root, phase);
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }

    #[test]
    fn advance_with_declared_checkpoint_but_unreported_gate_falls_through() {
        const NAME: &str = "pipeline_launch::tests::advance_with_declared_checkpoint_but_unreported_gate_falls_through";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(90);
        write_declared_checkpoint_plan(root, phase);
        write_unreported_failure_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-unreported".to_string());
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }

    /// Declared + reported, but no session id on record -> falls through,
    /// and the never-silent gate's context names the missing precondition.
    #[test]
    fn advance_with_confirmed_checkpoint_and_no_session_id_falls_through() {
        const NAME: &str = "pipeline_launch::tests::advance_with_confirmed_checkpoint_and_no_session_id_falls_through";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(91);
        write_declared_checkpoint_plan(root, phase);
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = None;
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        let gate_fired = events_of_kind(root, "gate_fired");
        assert!(!gate_fired.is_empty());
        assert!(
            gate_fired.iter().any(|e| e["context"]
                .as_str()
                .unwrap_or_default()
                .contains("session id")),
            "the never-silent gate's context must name the missing session id: {gate_fired:?}"
        );
    }

    /// Declared + reported + session id present, but the resume ceiling is
    /// already exhausted -> falls through, and the gate context names the
    /// exhaustion.
    #[test]
    fn advance_at_checkpoint_resume_ceiling_falls_through_to_generic_gate() {
        const NAME: &str = "pipeline_launch::tests::advance_at_checkpoint_resume_ceiling_falls_through_to_generic_gate";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(92);
        write_declared_checkpoint_plan(root, phase);
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-at-ceiling".to_string());
        state.checkpoint_resumes = mode::MAX_CHECKPOINT_RESUMES;
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        let gate_fired = events_of_kind(root, "gate_fired");
        assert!(!gate_fired.is_empty());
        assert!(
            gate_fired.iter().any(|e| e["context"]
                .as_str()
                .unwrap_or_default()
                .contains("ceiling")),
            "the never-silent gate's context must name the exhausted ceiling: {gate_fired:?}"
        );
    }

    /// D-05: declared + reported + session id present, but the agent is NOT
    /// Claude -> must never resume, regardless of the other four
    /// preconditions.
    #[test]
    fn advance_with_non_claude_agent_never_resumes() {
        const NAME: &str = "pipeline_launch::tests::advance_with_non_claude_agent_never_resumes";
        enter_agent_free_child!(NAME, "codex");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(93);
        write_declared_checkpoint_plan(root, phase);
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-non-claude".to_string());
        workflow::save_state(&state).unwrap();

        advance_for_stage(root, Some(phase), Some(Stage::Code)).unwrap();

        assert!(
            events_of_kind(root, "checkpoint_auto_decided").is_empty(),
            "a non-Claude agent must never take the resume path (D-05)"
        );
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }

    // ---- D-11: the legacy-launch opt-out (31-04) -------------------------

    /// Set `DEVFLOW_CLAUDE_LEGACY_LAUNCH` for the duration of a test, restoring
    /// the prior value on drop. Every user holds `ENV_MUTEX`.
    struct LegacyEnvOverride(Option<std::ffi::OsString>);

    impl LegacyEnvOverride {
        // D-09 (48-09): mutates `DEVFLOW_CLAUDE_LEGACY_LAUNCH` process-wide.
        // Deferred deliberately, not overlooked: THIS process reads the value, so
        // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
        // race and the value is restored on every exit path, unwinding included.
        #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
        fn set(value: &str) -> Self {
            let prior = std::env::var_os("DEVFLOW_CLAUDE_LEGACY_LAUNCH");
            // SAFETY: serialized by ENV_MUTEX; restored on drop.
            unsafe { std::env::set_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH", value) };
            Self(prior)
        }
    }

    impl Drop for LegacyEnvOverride {
        // D-09 (48-09): mutates `DEVFLOW_CLAUDE_LEGACY_LAUNCH` process-wide.
        // Deferred deliberately, not overlooked: THIS process reads the value, so
        // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
        // race and the value is restored on every exit path, unwinding included.
        #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
        fn drop(&mut self) {
            // SAFETY: same serialization as `set`.
            unsafe {
                match self.0.take() {
                    Some(prior) => std::env::set_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH", prior),
                    None => std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH"),
                }
            }
        }
    }

    fn legacy_state(root: &Path, phase: PhaseId, opt_out: bool) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.legacy_claude_launch = opt_out;
        state
    }

    /// D-11: one flag forces the pre-31 shape back on, even for a stage the
    /// rollout HAS reached — otherwise the escape hatch is unreachable exactly
    /// where it is needed.
    #[test]
    fn legacy_launch_flag_forces_the_single_document_path() {
        let dir = tempfile::tempdir().unwrap();
        let state = legacy_state(dir.path(), PhaseId::new(130), true);

        // Precondition: without the opt-out this stage IS in the rollout, so
        // the assertion below is a real discrimination and not a stage that
        // was going to be legacy anyway.
        assert!(
            stream_launch_enabled(state.agent, state.stage, false),
            "Stage::Code must be in STREAM_JSON_STAGES for this test to mean anything"
        );

        assert!(!stream_launch_enabled(
            state.agent,
            state.stage,
            state.legacy_claude_launch
        ));

        let driver = agents::driver_for(state.agent);
        let (program, args, launch) = resolve_launch_shape(
            state.agent,
            driver.as_ref(),
            state.phase,
            "the stage prompt".to_string(),
            &[],
            false,
        );

        assert!(matches!(launch, monitor::MonitorLaunch::Legacy));
        assert_eq!(program, "claude");
        assert_eq!(
            (program, args),
            agents::ClaudeDriver::exec_command_single_document("the stage prompt"),
            "the forced path must be exec_command_single_document byte-for-byte, \
             not an approximation of it"
        );
    }

    /// D-03 (47-03): a resumed Claude session receives the gate rule in two
    /// separately delivered turns — the Code loop-back prompt as the stdin user
    /// turn (turn 1) and the checkpoint resume instruction positionally in argv
    /// (turn 2). Co-residence is a property of the session transcript, not of
    /// any string DevFlow builds, so this test NEVER joins the two: each is
    /// captured from the production constructor that delivers it and asserted on
    /// its own.
    ///
    /// Controls: the stream-launch precondition (without it turn 1 resolves to
    /// `Legacy`, there is no stdin turn, and the rule checks would be vacuous
    /// rather than false); `assert_ne!` plus mutual non-containment (otherwise
    /// two captures collapsed onto one source would still pass); and turn 1
    /// still naming `package-verification` (D-02 keeps that prohibition, so a
    /// fix that deletes the sentence instead of splitting it fails here).
    #[test]
    fn the_gate_rule_holds_in_both_delivered_turns() {
        let phase = PhaseId::new(47);
        let agent = AgentKind::Claude;
        let driver = agents::driver_for(agent);
        let stream_launch = stream_launch_enabled(agent, Stage::Code, false);
        assert!(
            stream_launch,
            "Stage::Code must be stream-launch-enabled for Claude for this test to mean anything"
        );

        // Turn 1: `loop_back_to_code`'s own render (pipeline_gate.rs), then the
        // launch shape `launch_stage_inner` resolves it to.
        let loop_back_prompt = driver.render_prompt(&prompt::StageIntent::Code {
            phase,
            fix: Some(prompt::FixType::FullExecute),
        });
        let (_program, _args, launch) = resolve_launch_shape(
            agent,
            driver.as_ref(),
            phase,
            loop_back_prompt,
            &[],
            stream_launch,
        );
        let turn1 = match launch {
            monitor::MonitorLaunch::PipeOwning { prompt: stdin_turn } => stdin_turn,
            monitor::MonitorLaunch::Legacy => {
                panic!("the Code loop-back must deliver a stdin user turn, not a Legacy launch")
            }
        };

        // Turn 2: the real resume builder. Index rather than `.get()`, so an
        // absent element panics instead of reading quietly as `None`.
        let (_program, argv) = resume_launch_shape(phase, "session-under-test");
        assert_eq!(
            argv[0], "-p",
            "resume argv shape changed; the instruction is not argv[1]"
        );
        let turn2 = argv[1].clone();

        assert_ne!(
            turn1, turn2,
            "the two turns must be distinct delivered artifacts"
        );
        assert!(
            !turn1.contains(&turn2) && !turn2.contains(&turn1),
            "neither turn may contain the other, or the pair is one source twice"
        );
        assert!(
            turn1.contains("package-verification"),
            "D-02: turn 1 must keep the unconditional package-verification prohibition"
        );

        let mut contradictions = Vec::new();
        if turn1.contains("gate or a package-verification checkpoint") {
            contradictions.push("turn 1 forbids a blocking-human gate unconditionally");
        }
        // Strengthened once the shared constant existed (47-03 Task 3): turn 1
        // must carry the one definition, not merely lack the old sentence.
        if !turn1.contains(prompt::GATE_RESOLUTION_RULE) {
            contradictions.push("turn 1 does not carry the shared GATE_RESOLUTION_RULE");
        }
        if !turn2.contains("blocking-human") {
            contradictions.push(
                "turn 2 does not name the blocking-human gate it resumes the agent to resolve",
            );
        }
        assert!(
            contradictions.is_empty(),
            "the delivered turns disagree on the gate rule: {contradictions:#?}"
        );

        // D-15 (47-04): each delivered turn is also a reviewed `insta` baseline,
        // taken from the same captures asserted above. Two separately named
        // snapshots, NEVER one joined string: a joined snapshot would
        // reintroduce the composition D-03 rejects, which production never
        // performs (47-RESEARCH.md B-3).
        insta::assert_snapshot!("turn_one_code_prompt", turn1);
        insta::assert_snapshot!("turn_two_resume_prompt", turn2);
    }

    /// Phase 39 Stage 1 regression: Pi always resolves to `MonitorLaunch::Legacy`.
    /// Pi is never Claude, so `stream_launch` is false and the `else` branch
    /// applies; `PipeOwning` deadlocks Pi (Pi consumes stdin until EOF while
    /// `PipeOwning` holds it open — phase-38 review).
    #[test]
    fn pi_resolves_to_legacy_launch() {
        let mut state = State::new(
            PhaseId::new(39),
            AgentKind::Pi,
            Mode::Auto,
            std::path::PathBuf::from("/tmp"),
        );
        state.stage = Stage::Code;

        // Precondition: Pi must NOT be in the stream-json rollout, so the
        // assertion below discriminates a broken `stream_launch_enabled`
        // predicate instead of a stage that was going to be Legacy anyway
        // (phase-39 code review, finding 3).
        assert!(
            !stream_launch_enabled(state.agent, state.stage, false),
            "Pi must never be stream-launch-enabled for this test to mean anything"
        );

        let driver = agents::driver_for(state.agent);
        let (program, args, launch) = resolve_launch_shape(
            state.agent,
            driver.as_ref(),
            state.phase,
            "the stage prompt".to_string(),
            &[],
            false,
        );
        assert!(matches!(launch, monitor::MonitorLaunch::Legacy));
        assert_eq!(program, "pi");
        assert_eq!(args, vec!["-p", "--no-approve", "the stage prompt"]);
    }

    /// Off by default: nothing set anywhere leaves the run on the Phase 31
    /// transport. An escape hatch that engaged on its own would be the silent
    /// downgrade D-11 rejects.
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_CLAUDE_LEGACY_LAUNCH` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn legacy_launch_is_off_by_default() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let state = legacy_state(dir.path(), PhaseId::new(131), false);

        assert!(
            !state.legacy_claude_launch,
            "State::new must default the opt-out to off"
        );
        assert!(!devflow_core::config::claude_legacy_launch());

        let stream_launch =
            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(stream_launch);

        let driver = agents::driver_for(state.agent);
        let (_program, _args, launch) = resolve_launch_shape(
            state.agent,
            driver.as_ref(),
            state.phase,
            "the stage prompt".to_string(),
            &[],
            stream_launch,
        );
        assert!(matches!(launch, monitor::MonitorLaunch::PipeOwning { .. }));
    }

    /// D-11's "logged loudly" has to survive an unattended run where nobody is
    /// watching stdout, so the notice lands on THREE channels. This pins the
    /// two durable ones — the monitor log (the detached monitor's stdio is
    /// null) and the run's event ledger. Stdout is not asserted here; see the
    /// summary's "what this does not establish".
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_CLAUDE_LEGACY_LAUNCH` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn legacy_launch_use_is_recorded_in_provenance() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let state = legacy_state(root, PhaseId::new(132), true);

        announce_forced_legacy_launch(&state);

        let events = events_of_kind(root, "claude_legacy_launch_forced");
        assert_eq!(events.len(), 1, "exactly one provenance record: {events:?}");
        // Lowercase: `Stage`'s Display is the wire form, and every other event
        // in this ledger records it the same way.
        assert_eq!(events[0]["stage"].as_str(), Some("code"));
        assert_eq!(
            events[0]["source"].as_str(),
            Some("state:legacy_claude_launch"),
            "with no env var set in this process, the source is the persisted flag"
        );

        let log = std::fs::read_to_string(agent_result::monitor_log_path(root, PhaseId::new(132)))
            .expect("the monitor log must exist — it is the only channel a detached run has");
        assert!(log.contains("legacy launch"), "monitor log: {log}");

        // The required message (B3): the opt-out is an explicit acceptance of
        // 999.64, not an escape from a guard. Asserted on the pure notice so a
        // reworded log line cannot quietly drop it.
        let notice = forced_legacy_launch_notice(state.stage, LegacyLaunchSource::PersistedState);
        assert!(notice.contains("999.64"), "notice: {notice}");
        // Case-insensitive: the requirement is the WORD, not its emphasis.
        assert!(
            notice.to_ascii_lowercase().contains("orphan"),
            "the notice must say delegated work may be orphaned, in plain words: {notice}"
        );
        assert!(
            log.contains("999.64"),
            "the durable channel must carry it too, not just stdout: {log}"
        );
    }

    /// Tests SCOPING, not a permitted bypass (adversarial review B3,
    /// downgraded on evidence).
    ///
    /// The D-15 canary asks whether a live `stream-json` session is woken back
    /// up when a background task finishes. `MonitorLaunch::Legacy` runs the
    /// child with stdin at `/dev/null`, so a task-notification turn has no
    /// channel to arrive on — the mechanism the canary tests structurally does
    /// not exist on that path. Running it there would spend a real agent
    /// invocation on a 300s deadline answering a question the launch never
    /// asks. What the operator loses instead is named in the loud notice: the
    /// legacy path is where a multi-plan wave orphans delegated work (999.64).
    #[test]
    fn legacy_launch_skips_the_delivery_canary() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(133);
        let mut state = canary_state(root, phase);
        state.legacy_claude_launch = true;

        // Driven by the REAL predicate, so this tracks the wiring rather than
        // a copy of it.
        let stream_launch =
            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(!stream_launch);
        // Negative control: the SAME stage and agent, opt-out off, does run the
        // stream path — so the reading above is the opt-out's doing.
        assert!(stream_launch_enabled(state.agent, state.stage, false));

        let calls = std::cell::Cell::new(0usize);
        // `Absent` deliberately: if the gate ran this canary at all, the call
        // below returns Err and the unwrap fails.
        canary_gate(
            &mut state,
            stream_launch,
            counting_canary(&calls, canary::CanaryOutcome::Absent),
        )
        .unwrap();

        assert_eq!(calls.get(), 0);
        assert_eq!(state.canary, None);
    }

    /// D-11 rejects automatic fallback: a silent downgrade is the same
    /// invisible-degradation class as the bug this phase fixes.
    ///
    /// **Written as a guard against a future well-meaning addition.** The
    /// obvious "helpful" change — noticing an unparseable stream capture and
    /// relaunching on the legacy path — is exactly what must not appear. This
    /// test fails first if it does.
    ///
    /// **Scope, stated so this does not read as broader coverage than it is:**
    /// it covers *parse-failure-driven* fallback only. It does NOT prove that
    /// nothing anywhere selects legacy automatically — that claim is already
    /// false on `develop`. `relaunch_checkpoint_session` hardcodes
    /// `MonitorLaunch::Legacy`, is reached by unconditional checkpoint
    /// auto-decide, and bypasses both `canary_gate` and
    /// `stream_launch_enabled` by calling `spawn_agent_and_record`
    /// directly. That is a pre-existing, deliberate exception recorded in
    /// 31-04-SUMMARY.md as a known un-migrated route, not something this test
    /// covers.
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_CLAUDE_LEGACY_LAUNCH` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn parse_failure_does_not_trigger_a_fallback() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(134);
        let state = legacy_state(root, phase, false);
        workflow::save_state(&state).unwrap();

        // A capture the stream parser cannot make sense of: JSONL-shaped enough
        // to be a stream, with nothing any parser can turn into a verdict.
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "{\"type\":\"system\",\"subtype\":\"init\"}\n{\"type\":\"assistant\"\n",
        )
        .unwrap();

        // The ORDINARY verdict for a capture the stream parser cannot make
        // sense of — whatever the cascade already does with it. What matters
        // here is only that it is not a success and not a relaunch: constraint
        // 9 item 1 makes a torn line fail CLOSED rather than letting an earlier
        // turn stand in for the lost one.
        let verdict = agent_result::evaluate_layer1(root, phase);
        assert!(
            verdict
                .as_ref()
                .is_none_or(|r| r.status != agent_result::AgentStatus::Success),
            "fixture precondition: an unparseable capture must never read as success: {verdict:?}"
        );

        // ...and the launch shape for the very same state is UNCHANGED by that
        // failure. Nothing consulted the capture to pick a transport.
        let stream_launch =
            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(
            stream_launch,
            "a parse failure must not select the legacy path — D-11 rejects automatic fallback"
        );
        let driver = agents::driver_for(state.agent);
        let (_program, _args, launch) = resolve_launch_shape(
            state.agent,
            driver.as_ref(),
            state.phase,
            "the stage prompt".to_string(),
            &[],
            stream_launch,
        );
        assert!(matches!(launch, monitor::MonitorLaunch::PipeOwning { .. }));

        // And no provenance event claiming a forced legacy launch was written,
        // which is what a silent fallback would have looked like from outside.
        assert!(events_of_kind(root, "claude_legacy_launch_forced").is_empty());
    }

    /// W4: a naive `env::var(..).is_ok()` would make
    /// `DEVFLOW_CLAUDE_LEGACY_LAUNCH=false` *enable* the legacy path — an
    /// accidental-reach path D-11 forbids. The value is parsed as a bool.
    #[test]
    fn legacy_launch_env_var_is_parsed_as_a_bool() {
        let _guard = env_lock();

        {
            let _env = LegacyEnvOverride::set("true");
            assert!(devflow_core::config::claude_legacy_launch());
        }
        {
            // The case that motivates the test: PRESENT but false.
            let _env = LegacyEnvOverride::set("false");
            assert!(
                !devflow_core::config::claude_legacy_launch(),
                "`=false` must not enable the legacy path"
            );
        }
        {
            // Garbage warns and is ignored — it does not silently enable.
            let _env = LegacyEnvOverride::set("yes-please");
            assert!(!devflow_core::config::claude_legacy_launch());
        }
        {
            // Empty is treated as unset, matching `env_value`'s filter.
            let _env = LegacyEnvOverride::set("");
            assert!(!devflow_core::config::claude_legacy_launch());
        }
    }

    /// W5: `devflow resume` must not silently flip a run back to the stream
    /// path mid-flight. The persisted opt-out is OR-ed, never cleared — the
    /// same silent-drop class as `stop_until`'s unconditional clear (999.60),
    /// which was fixed by gating it.
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_CLAUDE_LEGACY_LAUNCH` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn resume_does_not_clear_a_persisted_legacy_launch() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(135);
        let mut state = legacy_state(root, phase, true);
        workflow::save_state(&state).unwrap();

        // The exact combination `resume` applies, with no flag and no env var.
        apply_legacy_launch_opt_out(&mut state, false);

        assert!(
            state.legacy_claude_launch,
            "a plain `devflow resume` must not drop the operator's opt-out"
        );

        // Negative control: the combination is not a constant `true` — a state
        // that never had the opt-out stays off.
        let mut never_opted_out = legacy_state(root, PhaseId::new(phase.major() + 1), false);
        apply_legacy_launch_opt_out(&mut never_opted_out, false);
        assert!(!never_opted_out.legacy_claude_launch);
    }

    // ------------------------------------------------------------------
    // Phase 41 Task 3: the widened stream predicate (Antigravity joins
    // Claude; the legacy opt-out stays Claude-only, D-10), the PipeOwning
    // launch shape, the by-agent canary dispatch (B2/D-07), and the
    // AutoChainGuard comment correction.
    // ------------------------------------------------------------------

    #[test]
    fn stream_launch_includes_antigravity_on_stream_stages() {
        assert!(
            stream_launch_enabled(AgentKind::Antigravity, Stage::Code, false),
            "Antigravity is a stream agent on a stream stage (round-3 D-10)"
        );
        // Claude unchanged.
        assert!(stream_launch_enabled(AgentKind::Claude, Stage::Code, false));
        // The other adapters never stream.
        for agent in [AgentKind::Codex, AgentKind::OpenCode, AgentKind::Pi] {
            assert!(
                !stream_launch_enabled(agent, Stage::Code, false),
                "{agent:?} must stay off the stream path"
            );
        }
    }

    /// D-10 (antigravity notice (b)): `DEVFLOW_CLAUDE_LEGACY_LAUNCH` is
    /// Claude's escape hatch. It must force CLAUDE off the stream path but
    /// never route ANTIGRAVITY to Legacy — Antigravity has no single-document
    /// format, and Legacy stdin is `/dev/null`.
    #[test]
    fn stream_launch_includes_antigravity_ignores_claude_legacy_opt_out() {
        assert!(
            !stream_launch_enabled(AgentKind::Claude, Stage::Code, true),
            "the legacy opt-out keeps its hold on Claude"
        );
        assert!(
            stream_launch_enabled(AgentKind::Antigravity, Stage::Code, true),
            "the legacy opt-out must NOT move Antigravity (D-10)"
        );
        // Same with the opt-out cleared, for the record.
        assert!(stream_launch_enabled(
            AgentKind::Antigravity,
            Stage::Code,
            false
        ));
    }

    #[test]
    fn stream_launch_includes_antigravity_resolves_to_pipe_owning() {
        let driver = devflow_core::agents::driver_for(AgentKind::Antigravity);
        let (_program, _args, launch) = resolve_launch_shape(
            AgentKind::Antigravity,
            driver.as_ref(),
            PhaseId::new(7),
            "prompt".to_string(),
            &[],
            true,
        );
        assert!(
            matches!(launch, monitor::MonitorLaunch::PipeOwning { .. }),
            "Antigravity on a stream stage must reach the pipe-owning arm"
        );
    }

    /// B2/D-07: the canary launcher is selected BY AGENT — Antigravity gets
    /// the agy-based launcher, never a Claude invocation.
    #[test]
    fn canary_launcher_for_selects_antigravity_canary() {
        let antg = canary_launcher_for(AgentKind::Antigravity, std::path::PathBuf::from("/tmp"));
        assert_eq!(
            antg.agent(),
            AgentKind::Antigravity,
            "Antigravity must drive the agy-based canary (B2)"
        );
        let claude = canary_launcher_for(AgentKind::Claude, std::path::PathBuf::from("/tmp"));
        assert_eq!(
            claude.agent(),
            AgentKind::Claude,
            "Claude keeps the Claude canary"
        );
    }

    /// The AutoChainGuard comment claimed "implies a Claude + stream launch";
    /// the widened predicate made that false. The guard's engagement for an
    /// Antigravity auto-mode Code launch is the conjunction run_monitor
    /// actually applies: stream predicate true -> PipeOwning shape -> the
    /// chain-flag eligibility predicate holds.
    #[test]
    fn auto_chain_guard_antigravity_engages_on_auto_code() {
        let driver = devflow_core::agents::driver_for(AgentKind::Antigravity);
        assert!(
            stream_launch_enabled(AgentKind::Antigravity, Stage::Code, false),
            "premise: Antigravity is on the stream path at Code"
        );
        let (_program, _args, launch) = resolve_launch_shape(
            AgentKind::Antigravity,
            driver.as_ref(),
            PhaseId::new(7),
            "prompt".to_string(),
            &[],
            true,
        );
        assert!(
            matches!(launch, monitor::MonitorLaunch::PipeOwning { .. }),
            "premise: the launch shape is PipeOwning — the only arm run_monitor guards"
        );
        assert!(
            auto_chain_flag_eligible(Stage::Code, Mode::Auto),
            "premise: the chain flag is eligible for auto Code — run_monitor engages \
             the guard for ANY pipe-owning launch (Claude + Antigravity today)"
        );
    }

    // ---- 50-13: the stage handoff, advance safe points and the gate wait ----

    /// The `<point>:<path>` hold variable `test_hold` reads (50-13).
    const HOLD_ENV: &str = "DEVFLOW_TEST_HOLD";

    /// The marker a handoff fixture's first agent prints when it completes.
    const SUCCESS_MARKER: &str = r#"DEVFLOW_RESULT: {"status":"success"}"#;

    /// How long a handoff child waits for any one step of the chain.
    const HANDOFF_STEP_WAIT: Duration = Duration::from_secs(45);

    /// A PATH dir whose fake `claude` answers `--version`. Its first run
    /// (stage N) waits for `go` in its working directory (the project root)
    /// and then prints `first_marker` and exits 0. Every later run (stage
    /// N+1) waits for `go2` and then prints a success marker. Both waits are
    /// bounded (about 60 s), so no fake agent outlives a failed test by long,
    /// and the fixture's `stop_until` ends the chain after N+1.
    fn handoff_claude_dir(first_marker: &str) -> tempfile::TempDir {
        let path_dir = agent_free_dir_with_agent_stub("claude");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
             if [ ! -e first-done ]; then\n  : > first-done\n  gate=go\n  marker='{first_marker}'\n\
             else\n  gate=go2\n  marker='{SUCCESS_MARKER}'\nfi\n\
             i=0\nwhile [ ! -e \"$gate\" ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
             printf '%s\\n' \"$marker\"\nexit 0\n"
        );
        std::fs::write(path_dir.path().join("claude"), script).unwrap();
        let real_sleep = ["/usr/bin/sleep", "/bin/sleep"]
            .into_iter()
            .map(Path::new)
            .find(|path| path.is_file())
            .expect("sleep must exist to run this test");
        std::os::unix::fs::symlink(real_sleep, path_dir.path().join("sleep")).unwrap();
        path_dir
    }

    /// The parent half of a handoff test: run `name` in a hermetic child on
    /// the advance-running monitor entry, with `DEVFLOW_TEST_HOLD` naming
    /// `hold` (when given) and a release path in a parent-owned dir.
    fn run_handoff_child(name: &str, hold: Option<&str>, first_marker: &str) {
        let path_dir = handoff_claude_dir(first_marker);
        let hold_dir = tempfile::Builder::new()
            .prefix("dfh")
            .tempdir_in("/tmp")
            .unwrap();
        let hold_value = hold.map(|point| {
            std::ffi::OsString::from(format!(
                "{point}:{}",
                hold_dir.path().join("release").display()
            ))
        });
        let mut env: Vec<(&str, &std::ffi::OsStr)> = vec![
            (TEST_MONITOR_FULL_ENV, std::ffi::OsStr::new("1")),
            ("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("120")),
        ];
        if let Some(value) = &hold_value {
            env.push((HOLD_ENV, value.as_os_str()));
        }
        let output = run_hermetic_test_in_child(name, path_dir.path(), &env);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
    }

    /// One launched stage N in a handoff child: the repo, its phase, N's
    /// record and the hermetic cache dir. Dropping it releases any hold and
    /// reaps every supervisor whose socket is still in the cache dir, so a
    /// failing assertion leaves no `__monitor` or fake agent behind.
    struct HandoffRun {
        _dir: tempfile::TempDir,
        root: PathBuf,
        phase: PhaseId,
        n: devflow_core::state::SupervisorRecord,
        cache: PathBuf,
    }

    impl HandoffRun {
        /// Launch stage Define for `phase` (capture shape, Supervise mode,
        /// `--until plan`) and return once N is recorded.
        fn launch(phase: PhaseId) -> Self {
            Self::launch_with(phase, |_, _| {})
        }

        /// [`launch`](Self::launch), with `configure` applied to the repo
        /// and the state before it is saved and stage N launched.
        fn launch_with(phase: PhaseId, configure: impl FnOnce(&Path, &mut State)) -> Self {
            let cache = PathBuf::from(
                std::env::var_os("DEVFLOW_CACHE_DIR").expect("the parent passes DEVFLOW_CACHE_DIR"),
            );
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            init_repo(&root);
            std::fs::create_dir_all(root.join(".planning")).unwrap();
            std::fs::write(
                root.join(".planning/ROADMAP.md"),
                format!("# Roadmap\n\n### Phase {phase}: handoff fixture\n"),
            )
            .unwrap();
            let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.clone());
            state.legacy_claude_launch = true;
            state.stop_until = Some(Stage::Plan);
            configure(&root, &mut state);
            workflow::save_state(&state).unwrap();
            let launched = launch_stage(&mut state, None, None);
            let n = state.supervisor.clone();
            let run = Self {
                _dir: dir,
                root,
                phase,
                n: n.clone().unwrap_or_else(|| {
                    panic!("the launch must record stage N's supervisor: {launched:?}")
                }),
                cache,
            };
            launched.unwrap();
            run
        }

        /// The release path `test_hold` waits on, from `DEVFLOW_TEST_HOLD`.
        fn release_path() -> Option<PathBuf> {
            let hold = std::env::var(HOLD_ENV).ok()?;
            let (_, path) = hold.split_once(':')?;
            Some(PathBuf::from(path))
        }

        /// Let stage N's agent finish.
        fn go(&self) {
            std::fs::write(self.root.join("go"), "").unwrap();
        }

        /// Block until a supervisor reports it reached the configured hold.
        fn wait_held(&self) {
            let release = Self::release_path().expect("this child runs with a hold");
            let held = PathBuf::from(format!("{}.held", release.display()));
            assert!(
                poll_until(HANDOFF_STEP_WAIT, || held.exists()),
                "the {} hold was never reached; events:\n{}",
                std::env::var(HOLD_ENV).unwrap_or_default(),
                self.events()
            );
        }

        /// The run's whole event log, for failure messages.
        fn events(&self) -> String {
            std::fs::read_to_string(devflow_core::events::events_path(&self.root))
                .unwrap_or_default()
        }

        /// Release the configured hold.
        fn release(&self) {
            if let Some(release) = Self::release_path() {
                std::fs::write(release, "").unwrap();
            }
        }

        fn state(&self) -> State {
            workflow::load_state(&self.root, self.phase).expect("the run's state loads")
        }

        /// Every `.sock` in the cache dir's `sock/` other than N's.
        fn other_sockets(&self) -> Vec<PathBuf> {
            std::fs::read_dir(self.cache.join("sock"))
                .map(|entries| {
                    entries
                        .filter_map(|entry| Some(entry.ok()?.path()))
                        .filter(|path| path.extension().is_some_and(|ext| ext == "sock"))
                        .filter(|path| *path != self.n.socket_path)
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    impl Drop for HandoffRun {
        fn drop(&mut self) {
            self.release();
            // Two passes: a stage N still advancing when the first pass
            // stops it may have launched N+1 by the second.
            for _ in 0..2 {
                let sockets: Vec<PathBuf> = std::fs::read_dir(self.cache.join("sock"))
                    .map(|entries| {
                        entries
                            .filter_map(|entry| Some(entry.ok()?.path()))
                            .filter(|path| path.extension().is_some_and(|ext| ext == "sock"))
                            .collect()
                    })
                    .unwrap_or_default();
                for socket in sockets {
                    let Ok(sidecar) =
                        devflow_core::supervisor::read_sidecar(&socket.with_extension("json"))
                    else {
                        continue;
                    };
                    let mut holder = self.state_holder();
                    holder.supervisor = Some(devflow_core::state::SupervisorRecord::new(
                        socket,
                        sidecar.launch_id,
                        sidecar.agent_pgid,
                        sidecar.agent_start_time,
                        sidecar.boot_id,
                    ));
                    drop(ReapMonitorOnDrop::after_launch(&holder));
                }
            }
        }
    }

    impl HandoffRun {
        /// A throwaway `State` to carry one record into `ReapMonitorOnDrop`.
        fn state_holder(&self) -> State {
            State::new(
                self.phase,
                AgentKind::Claude,
                Mode::Supervise,
                self.root.clone(),
            )
        }
    }

    /// What a handoff observer saw in one poll: the state's recorded
    /// supervisor and how its socket probed.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Seen {
        Alive(String),
        Stale,
        Gone,
        NoRecord,
        Unloadable,
        ProbeFailed,
    }

    fn observe(root: &Path, phase: PhaseId) -> Seen {
        let Ok(state) = workflow::load_state(root, phase) else {
            return Seen::Unloadable;
        };
        let Some(record) = state.supervisor else {
            return Seen::NoRecord;
        };
        match devflow_core::supervisor::probe(&record.socket_path) {
            Ok(devflow_core::supervisor::Liveness::Alive(reply)) => Seen::Alive(reply.launch_id),
            Ok(devflow_core::supervisor::Liveness::Stale) => Seen::Stale,
            Ok(devflow_core::supervisor::Liveness::Gone) => Seen::Gone,
            Err(_) => Seen::ProbeFailed,
        }
    }

    /// A thread that polls [`observe`] every 5 ms (and whether `watched`
    /// exists) until stopped.
    struct Observer {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        handle: std::thread::JoinHandle<Vec<(Seen, bool)>>,
    }

    impl Observer {
        fn start(root: &Path, phase: PhaseId, watched: &Path) -> Self {
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (root, watched, flag) = (
                root.to_path_buf(),
                watched.to_path_buf(),
                std::sync::Arc::clone(&stop),
            );
            let handle = std::thread::spawn(move || {
                let mut seen = Vec::new();
                while !flag.load(std::sync::atomic::Ordering::Acquire) {
                    seen.push((observe(&root, phase), watched.exists()));
                    std::thread::sleep(Duration::from_millis(5));
                }
                seen
            });
            Self { stop, handle }
        }

        fn finish(self) -> Vec<(Seen, bool)> {
            self.stop.store(true, std::sync::atomic::Ordering::Release);
            self.handle.join().expect("the observer thread")
        }
    }

    /// Every observation between the first `Alive(from)` and the first
    /// `Alive(to)` that reads `Gone`, `Stale` or no record: the handoff's
    /// gaps. An error when either endpoint was never observed.
    fn handoff_gaps(seen: &[(Seen, bool)], from: &str, to: &str) -> Result<Vec<Seen>, String> {
        let first = |id: &str| {
            seen.iter()
                .position(|(observed, _)| *observed == Seen::Alive(id.to_string()))
        };
        let start = first(from).ok_or_else(|| format!("never saw {from} alive"))?;
        let end = first(to).ok_or_else(|| format!("never saw {to} alive"))?;
        if end < start {
            return Err(format!("saw {to} alive before {from}"));
        }
        Ok(seen[start..end]
            .iter()
            .map(|(observed, _)| observed.clone())
            .filter(|observed| matches!(observed, Seen::Gone | Seen::Stale | Seen::NoRecord))
            .collect())
    }

    /// V-13 (R-1, T-50-34): a stage handoff publishes N+1 before N unlinks.
    /// Stage N's agent finishes, N's in-process advance launches N+1 and
    /// records it, and N is parked at the `after_publish` hold (lock still
    /// held). At the hold the state names N+1, N+1 probes Alive, and N's
    /// socket still exists and answers with owner `advancing` (RV-04: N is
    /// not marked handed off inside the advance). Released, N unlinks and
    /// N+1 stays Alive. A 5 ms observer must see no Gone, Stale or missing
    /// record between N's first Alive and N+1's first Alive; it can miss a
    /// gap shorter than 5 ms, so that check is supplementary and
    /// `the_handoff_observer_detects_a_pre_clear_gap` shows it sees one.
    #[test]
    fn stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked() {
        const NAME: &str = "pipeline_launch::tests::stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_publish"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1301));
        let observer = Observer::start(&run.root, run.phase, &run.n.socket_path);
        run.go();
        run.wait_held();

        let next = run
            .state()
            .supervisor
            .expect("at the after_publish hold the state records a supervisor");
        assert_ne!(
            next.launch_id, run.n.launch_id,
            "at the after_publish hold the state names N+1, not N"
        );
        match devflow_core::supervisor::probe(&next.socket_path) {
            Ok(devflow_core::supervisor::Liveness::Alive(reply)) => {
                assert_eq!(reply.launch_id, next.launch_id);
            }
            other => panic!("N+1 must probe Alive at the hold, got {other:?}"),
        }
        assert!(
            run.n.socket_path.exists(),
            "N's socket file must still exist at the after_publish hold"
        );
        match devflow_core::supervisor::probe(&run.n.socket_path) {
            Ok(devflow_core::supervisor::Liveness::Alive(reply)) => assert_eq!(
                reply.owner, "advancing",
                "RV-04: N stays Advancing until its advance returns"
            ),
            other => panic!("N must still answer at the hold, got {other:?}"),
        }

        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                devflow_core::supervisor::probe(&run.n.socket_path)
                    .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
            }),
            "N unlinks its socket once its advance has returned"
        );
        match devflow_core::supervisor::probe(&next.socket_path) {
            Ok(devflow_core::supervisor::Liveness::Alive(reply)) => {
                assert_eq!(reply.launch_id, next.launch_id);
            }
            other => panic!("N+1 must stay Alive after N exits, got {other:?}"),
        }
        let seen = observer.finish();
        let gaps = handoff_gaps(&seen, &run.n.launch_id, &next.launch_id)
            .unwrap_or_else(|why| panic!("observer: {why}: {seen:?}"));
        assert!(
            gaps.is_empty(),
            "state read GONE or no record across the handoff: {gaps:?}"
        );
    }

    /// A live in-process supervisor serving `launch_id` at a short socket
    /// path in `dir`, for the observer control.
    fn fake_supervisor(
        dir: &Path,
        root: &Path,
        phase: PhaseId,
        launch_id: &str,
    ) -> (
        devflow_core::supervisor::runtime::Supervisor,
        devflow_core::state::SupervisorRecord,
    ) {
        let path = dir.join(format!("{launch_id}.sock"));
        let socket = devflow_core::supervisor::SocketPath::new(path.clone()).unwrap();
        let sidecar: devflow_core::supervisor::Sidecar =
            serde_json::from_value(serde_json::json!({
                "launch_id": launch_id,
                "project_root": root,
                "phase": phase,
                "stage": null,
                "agent": "claude",
                "launch": "capture",
                "agent_pid": 0,
                "agent_pgid": 0,
                "agent_start_time": 0,
                "boot_id": "",
                "supervisor_pid": std::process::id(),
                "supervisor_start_time": 0,
                "created_at": "2026-10-17T00:00:00Z",
            }))
            .unwrap();
        let supervisor = devflow_core::supervisor::runtime::Supervisor::serve(socket, sidecar)
            .expect("serve the fake supervisor");
        let record = devflow_core::state::SupervisorRecord::new(
            path,
            launch_id.to_string(),
            0,
            0,
            String::new(),
        );
        (supervisor, record)
    }

    /// The observer control for V-13: the same observer and gap rule applied
    /// to a simulated pre-clear handoff (a save with no supervisor between
    /// N's record and N+1's, the WR-04 pitfall) reports the gap; the same
    /// sequence without the pre-clear reports none. Both endpoints are real
    /// in-process supervisors answering pings.
    #[test]
    fn the_handoff_observer_detects_a_pre_clear_gap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let sock_dir = tempfile::Builder::new()
            .prefix("dfs")
            .tempdir_in("/tmp")
            .unwrap();
        let phase = PhaseId::new(1302);
        let (sup_n, rec_n) = fake_supervisor(sock_dir.path(), root, phase, "obsn");
        let (sup_next, rec_next) = fake_supervisor(sock_dir.path(), root, phase, "obsnext");
        let save = |record: Option<&devflow_core::state::SupervisorRecord>| {
            let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
            state.supervisor = record.cloned();
            workflow::save_state(&state).unwrap();
        };
        let hold = Duration::from_millis(80);

        for pre_clear in [true, false] {
            save(Some(&rec_n));
            let observer = Observer::start(root, phase, &rec_n.socket_path);
            std::thread::sleep(hold);
            if pre_clear {
                save(None);
                std::thread::sleep(hold);
            }
            save(Some(&rec_next));
            std::thread::sleep(hold);
            let seen = observer.finish();
            let gaps = handoff_gaps(&seen, "obsn", "obsnext")
                .unwrap_or_else(|why| panic!("observer: {why}: {seen:?}"));
            if pre_clear {
                assert!(
                    gaps.contains(&Seen::NoRecord),
                    "the observer must see the pre-clear gap: {seen:?}"
                );
            } else {
                assert!(gaps.is_empty(), "control: no pre-clear, no gap: {gaps:?}");
            }
        }
        sup_n.finish();
        sup_next.finish();
    }

    /// The RV-04 order rule, as a pure scan over `src`: `spawn_agent_and_record`
    /// records N+1 as the successor and never marks N handed off, and
    /// `run_monitor` takes the successor and marks the hand-off. Each function
    /// is its comment-stripped lines from the column-0 line starting with its
    /// signature through the next line that is exactly `}`.
    fn handoff_order_violations(src: &str) -> Vec<String> {
        fn body<'a>(src: &'a str, signature: &str, floor: usize) -> Option<Vec<&'a str>> {
            let mut lines = src.lines();
            let first = lines.by_ref().find(|line| line.starts_with(signature))?;
            let mut body = vec![first];
            for line in lines {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                body.push(line);
                if line == "}" {
                    break;
                }
            }
            (body.len() >= floor).then_some(body)
        }
        let has = |lines: &[&str], needle: &str| lines.iter().any(|line| line.contains(needle));
        let mut violations = Vec::new();
        match body(src, "fn spawn_agent_and_record(", 40) {
            None => violations.push(SCAN_SPAWN_NOT_FOUND.to_string()),
            Some(lines) => {
                if has(&lines, "OwnerPhase::HandedOff") {
                    violations.push(SCAN_SPAWN_HANDS_OFF.to_string());
                }
                if !has(&lines, "record_successor(") {
                    violations.push(SCAN_SPAWN_NO_SUCCESSOR.to_string());
                }
            }
        }
        match body(src, "pub(crate) fn run_monitor(", 10) {
            None => violations.push(SCAN_RUN_NOT_FOUND.to_string()),
            Some(lines) => {
                if !has(&lines, "OwnerPhase::HandedOff") {
                    violations.push(SCAN_RUN_NO_HAND_OFF.to_string());
                }
                if !has(&lines, "take_successor(") {
                    violations.push(SCAN_RUN_NO_TAKE.to_string());
                }
            }
        }
        violations
    }

    const SCAN_SPAWN_NOT_FOUND: &str =
        "spawn_agent_and_record not found (or under 40 comment-stripped lines)";
    const SCAN_SPAWN_HANDS_OFF: &str = "spawn_agent_and_record enters the handed-off phase";
    const SCAN_SPAWN_NO_SUCCESSOR: &str = "spawn_agent_and_record never calls record_successor";
    const SCAN_RUN_NOT_FOUND: &str = "run_monitor not found (or under 10 comment-stripped lines)";
    const SCAN_RUN_NO_HAND_OFF: &str = "run_monitor never enters the handed-off phase";
    const SCAN_RUN_NO_TAKE: &str = "run_monitor never calls take_successor";

    /// RV-04 (permanent source scan): N is marked handed off only in
    /// `run_monitor`, after its advance returned and only when the advance
    /// recorded a successor; `spawn_agent_and_record` records the successor
    /// and leaves N `Advancing`. Planted controls prove the scan can flag
    /// each violation and passes a correct layout.
    #[test]
    fn the_handed_off_marking_stays_in_run_monitor_after_the_advance() {
        let real = handoff_order_violations(include_str!("pipeline_launch.rs"));
        assert!(real.is_empty(), "pipeline_launch.rs: {real:?}");

        let pad = |n: usize| "    let _filler = 0;\n".repeat(n);
        let spawn = |extra: &str| format!("fn spawn_agent_and_record(\n{}{extra}}}\n", pad(40));
        let run = |extra: &str| format!("pub(crate) fn run_monitor(\n{}{extra}}}\n", pad(10));
        let good_spawn = spawn("    record_successor(record);\n");
        let good_run = run(
            "    if let Some(next) = take_successor() {\n        token.enter(OwnerPhase::HandedOff);\n    }\n",
        );
        let only = |violation: &str| vec![violation.to_string()];

        assert_eq!(
            handoff_order_violations(&format!("{good_spawn}{good_run}")),
            Vec::<String>::new(),
            "a correct pair"
        );
        assert_eq!(
            handoff_order_violations(&format!(
                "{}{good_run}",
                spawn("    record_successor(record);\n    token.enter(OwnerPhase::HandedOff);\n")
            )),
            only(SCAN_SPAWN_HANDS_OFF)
        );
        assert_eq!(
            handoff_order_violations(&format!(
                "{}{good_run}",
                spawn(
                    "    record_successor(record);\n    // token.enter(OwnerPhase::HandedOff);\n"
                )
            )),
            Vec::<String>::new(),
            "a comment line is not code"
        );
        assert_eq!(
            handoff_order_violations(&format!("{}{good_run}", spawn(""))),
            only(SCAN_SPAWN_NO_SUCCESSOR)
        );
        assert_eq!(
            handoff_order_violations(&format!(
                "{good_spawn}{}",
                run("    let _next = take_successor();\n")
            )),
            only(SCAN_RUN_NO_HAND_OFF)
        );
        assert_eq!(
            handoff_order_violations(&format!(
                "{good_spawn}{}",
                run("    token.enter(OwnerPhase::HandedOff);\n")
            )),
            only(SCAN_RUN_NO_TAKE)
        );
        assert_eq!(
            handoff_order_violations(&good_run),
            only(SCAN_SPAWN_NOT_FOUND)
        );
        assert_eq!(
            handoff_order_violations(&good_spawn),
            only(SCAN_RUN_NOT_FOUND)
        );
        assert_eq!(
            handoff_order_violations(&format!(
                "fn spawn_agent_and_record(\n{}    record_successor(record);\n}}\n{good_run}",
                pad(5)
            )),
            only(SCAN_SPAWN_NOT_FOUND),
            "a body under its floor"
        );
        assert_eq!(
            handoff_order_violations(&format!(
                "{good_spawn}pub(crate) fn run_monitor(\n    take_successor();\n    token.enter(OwnerPhase::HandedOff);\n}}\n"
            )),
            only(SCAN_RUN_NOT_FOUND),
            "a run_monitor under its floor"
        );
    }

    /// The run's events named `name`.
    fn events_named(root: &Path, name: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(devflow_core::events::events_path(root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["event"].as_str() == Some(name))
            .collect()
    }

    /// Send `shutdown` to the supervisor at `socket` on the wire, wait for
    /// its `accepted` line (returned first), run `then`, and return the final
    /// line read within `final_wait`. Unlike `request_shutdown`, the caller
    /// acts at a known point: after the supervisor has taken the stop.
    fn shutdown_accepted_then(
        socket: &Path,
        then: impl FnOnce(),
        final_wait: Duration,
    ) -> (serde_json::Value, serde_json::Value) {
        use std::io::{BufRead as _, Write as _};
        let stream = std::os::unix::net::UnixStream::connect(socket).expect("connect to N");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        (&stream)
            .write_all(b"{\"op\":\"shutdown\"}\n")
            .expect("send shutdown");
        let mut reader = std::io::BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("the accepted line");
        let accepted: serde_json::Value = serde_json::from_str(&line)
            .unwrap_or_else(|err| panic!("accepted line {line:?}: {err}"));
        then();
        stream.set_read_timeout(Some(final_wait)).unwrap();
        line.clear();
        reader.read_line(&mut line).expect("the final line");
        let last: serde_json::Value =
            serde_json::from_str(&line).unwrap_or_else(|err| panic!("final line {line:?}: {err}"));
        (accepted, last)
    }

    /// Whether any process is left in group `pgid` (`kill(-pgid, 0)`).
    fn group_alive(pgid: u32) -> bool {
        let Ok(signed) = libc::pid_t::try_from(pgid) else {
            return false;
        };
        assert!(signed > 1, "refusing to probe group {signed}");
        // SAFETY: signal 0 only checks for existence; `-signed < -1` names
        // one process group, never our own (0) or every process (-1).
        unsafe { libc::kill(-signed, 0) == 0 }
    }

    /// The body of the held-advance pair: N parks at `before_spawn` (safe
    /// point 2 follows the hold), then a stop is (or is not) taken before the
    /// release.
    fn held_advance_before_spawn(phase: PhaseId, stop: bool) {
        let _guard = env_lock();
        let run = HandoffRun::launch(phase);
        run.go();
        run.wait_held();
        let release = || run.release();
        if stop {
            let (accepted, last) =
                shutdown_accepted_then(&run.n.socket_path, release, Duration::from_secs(30));
            assert_eq!(accepted["owner"], "advancing", "{accepted}");
            assert_eq!(last["stopped"], true, "N answers stopped: {last}");
            assert!(
                poll_until(HANDOFF_STEP_WAIT, || {
                    devflow_core::supervisor::probe(&run.n.socket_path)
                        .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
                }),
                "N's socket is gone after the stop"
            );
            assert!(
                poll_until(Duration::from_secs(5), || matches!(
                    lock::holder_status(&run.root, run.phase),
                    lock::HolderStatus::NoHolder
                )),
                "the per-phase lock is released"
            );
            let state = run.state();
            assert!(state.stopped, "state.stopped after a stop at safe point 2");
            assert_eq!(state.stop_reason.as_deref(), Some(ADVANCE_STOP_REASON));
            assert_eq!(
                stage_launched_count(&run.root, run.phase),
                1,
                "no next stage is launched: {}",
                run.events()
            );
            let stopped = events_named(&run.root, "advance_stopped");
            assert_eq!(stopped.len(), 1, "{}", run.events());
            assert_eq!(stopped[0]["safe_point"], 2, "{}", stopped[0]);
            assert!(
                run.other_sockets().is_empty(),
                "no next supervisor was spawned"
            );
        } else {
            release();
            assert!(
                poll_until(HANDOFF_STEP_WAIT, || stage_launched_count(
                    &run.root, run.phase
                ) == 2),
                "control: without a stop the held advance launches the next stage: {}",
                run.events()
            );
            let next = run
                .state()
                .supervisor
                .expect("control: the next stage is recorded");
            assert_ne!(next.launch_id, run.n.launch_id);
            assert!(
                matches!(
                    devflow_core::supervisor::probe(&next.socket_path),
                    Ok(devflow_core::supervisor::Liveness::Alive(_))
                ),
                "control: the next supervisor is Alive"
            );
            assert!(!run.state().stopped, "control: not stopped");
        }
    }

    /// R-2, safe point 2: a stop taken while N is advancing is honoured right
    /// before the next stage's spawn. Nothing is launched, the state is
    /// stopped with the safe-point reason, the lock is released and N's
    /// socket is gone.
    #[test]
    fn a_stop_during_advance_stops_at_the_next_safe_point_and_launches_nothing() {
        const NAME: &str = "pipeline_launch::tests::a_stop_during_advance_stops_at_the_next_safe_point_and_launches_nothing";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_spawn"), SUCCESS_MARKER);
            return;
        }
        held_advance_before_spawn(PhaseId::new(1303), true);
    }

    /// The control for the held-advance pair: the same hold and no stop, so
    /// the next stage is launched and Alive.
    #[test]
    fn without_a_stop_the_held_advance_launches_the_next_stage() {
        const NAME: &str =
            "pipeline_launch::tests::without_a_stop_the_held_advance_launches_the_next_stage";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_spawn"), SUCCESS_MARKER);
            return;
        }
        held_advance_before_spawn(PhaseId::new(1304), false);
    }

    /// R-2, safe point 3 (T-50-54): a stop taken while N holds the lock
    /// between N+1's ping and its publish shuts N+1 down through its socket
    /// and does not publish it. The state keeps N's record and is stopped with
    /// N's safe-point reason; no agent of N+1 survives; and N answers
    /// `stopped` in under `TERMINATE_VERIFY_WAIT + 3 s`, below both bounded
    /// waits a lock deadlock would sit out (N's final timeout and N+1's lock
    /// wait), so a stopped N+1 that waited on N's lock fails the timing.
    #[test]
    fn a_stop_between_the_next_spawn_and_its_publish_shuts_the_next_supervisor_down() {
        const NAME: &str = "pipeline_launch::tests::a_stop_between_the_next_spawn_and_its_publish_shuts_the_next_supervisor_down";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_publish"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1305));
        run.go();
        run.wait_held();
        let next_socket = match run.other_sockets().as_slice() {
            [one] => one.clone(),
            other => panic!("exactly one next supervisor is spawned at the hold: {other:?}"),
        };
        let next = devflow_core::supervisor::read_sidecar(&next_socket.with_extension("json"))
            .expect("N+1's sidecar");
        assert!(group_alive(next.agent_pgid), "N+1's agent runs at the hold");

        let mut released_at = None;
        let (accepted, last) = shutdown_accepted_then(
            &run.n.socket_path,
            || {
                released_at = Some(std::time::Instant::now());
                run.release();
            },
            Duration::from_secs(30),
        );
        let took = released_at.expect("released").elapsed();
        assert_eq!(accepted["owner"], "advancing", "{accepted}");
        assert_eq!(last["stopped"], true, "N answers stopped: {last}");
        let bound = devflow_core::agent::TERMINATE_VERIFY_WAIT + Duration::from_secs(3);
        assert!(
            took < bound,
            "N's stop took {took:?}, not under {bound:?}: a lock deadlock between N and N+1"
        );
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                devflow_core::supervisor::probe(&next_socket)
                    .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
            }),
            "N+1 was shut down through its socket"
        );
        assert!(
            poll_until(Duration::from_secs(5), || !group_alive(next.agent_pgid)),
            "no agent of N+1 survives"
        );
        let state = run.state();
        assert_eq!(
            state.supervisor.as_ref().map(|record| &record.launch_id),
            Some(&run.n.launch_id),
            "N+1 was never published: the state keeps N's record"
        );
        assert!(state.stopped, "state.stopped after a stop at safe point 3");
        assert_eq!(state.stop_reason.as_deref(), Some(ADVANCE_STOP_REASON));
        assert_eq!(
            stage_launched_count(&run.root, run.phase),
            1,
            "{}",
            run.events()
        );
        let stopped = events_named(&run.root, "advance_stopped");
        assert_eq!(stopped.len(), 1, "{}", run.events());
        assert_eq!(stopped[0]["safe_point"], 3, "{}", stopped[0]);
    }

    /// Write a capture whose Layer 1 marker reads success, with exit code 0.
    fn write_success_capture(root: &Path, phase: PhaseId) {
        let stdout = devflow_core::agent_result::stdout_path(root, phase);
        std::fs::create_dir_all(stdout.parent().unwrap()).unwrap();
        std::fs::write(&stdout, format!("work done\n{SUCCESS_MARKER}\n")).unwrap();
        std::fs::write(
            devflow_core::agent_result::exit_code_path(root, phase),
            "0\n",
        )
        .unwrap();
    }

    /// 999.138 residual (A6): `advance_with` refuses a stopped state with an
    /// `advance_failed {reason: "stopped"}` event and an error naming
    /// `devflow resume`. Control: the same state with `stopped: false`
    /// advances (here to its `--until define` stop, so nothing launches).
    #[test]
    fn advance_refuses_a_stopped_state() {
        let _guard = env_lock();
        for stopped in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(1307);
            let mut state = State::new(
                phase,
                AgentKind::Claude,
                Mode::Supervise,
                root.to_path_buf(),
            );
            state.stop_until = Some(Stage::Define);
            state.stopped = stopped;
            workflow::save_state(&state).unwrap();
            write_success_capture(root, phase);

            let result = advance_with(root, phase, Some(Stage::Define), Duration::from_secs(5));
            let failed = events_named(root, "advance_failed");
            if stopped {
                let err = result.expect_err("a stopped state must not advance");
                assert!(
                    err.to_string().contains("devflow resume --phase 1307"),
                    "the error names the repair: {err}"
                );
                assert_eq!(failed.len(), 1, "{failed:?}");
                assert_eq!(failed[0]["reason"], "stopped", "{}", failed[0]);
                assert!(
                    events_named(root, "advance_evaluated").is_empty(),
                    "a refused advance evaluates nothing"
                );
            } else {
                result.expect("control: an unstopped state advances");
                assert!(failed.is_empty(), "control: {failed:?}");
                assert_eq!(events_named(root, "advance_evaluated").len(), 1);
                let finished = events_named(root, "workflow_finished");
                assert_eq!(finished.len(), 1, "control reaches its --until stop");
            }
        }
    }

    /// T-50-68: a supervisor stopped while its state file will not load still
    /// answers `stopped` (not an error, a closed connection or the final
    /// timeout), its socket goes, its agent group goes, and the state bytes
    /// are unchanged: the no-write path. Control: the same stop on a run
    /// whose state loads records `stopped`, so the unchanged bytes are the
    /// no-write path and not a harness that cannot see a write.
    #[test]
    fn a_stopped_supervisor_whose_state_will_not_load_still_answers_stopped() {
        const NAME: &str = "pipeline_launch::tests::a_stopped_supervisor_whose_state_will_not_load_still_answers_stopped";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, None, SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let wait = 2 * devflow_core::agent::TERMINATE_VERIFY_WAIT + Duration::from_secs(5);
        for corrupt in [true, false] {
            let run = HandoffRun::launch(PhaseId::new(if corrupt { 1308 } else { 1309 }));
            let state_file = workflow::state_path(&run.root, run.phase);
            if corrupt {
                std::fs::write(&state_file, b"{ this is not json").unwrap();
            }
            let before = std::fs::read(&state_file).unwrap();
            let outcome = devflow_core::supervisor::request_shutdown(&run.n.socket_path, wait);
            assert!(
                matches!(
                    outcome,
                    Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
                ),
                "corrupt={corrupt}: the stop reads Stopped, got {outcome:?}"
            );
            assert!(
                poll_until(HANDOFF_STEP_WAIT, || {
                    devflow_core::supervisor::probe(&run.n.socket_path)
                        .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
                }),
                "corrupt={corrupt}: the socket is gone"
            );
            assert!(
                poll_until(Duration::from_secs(5), || !group_alive(run.n.agent_pgid)),
                "corrupt={corrupt}: the agent group is gone"
            );
            if corrupt {
                assert_eq!(
                    std::fs::read(&state_file).unwrap(),
                    before,
                    "the state that will not load is left byte for byte"
                );
            } else {
                let state = run.state();
                assert!(state.stopped, "control: a loadable state records stopped");
                assert_eq!(state.stop_reason.as_deref(), Some(SUPERVISOR_STOP_REASON));
            }
        }
    }

    /// The marker a gate fixture's first agent prints: a failed Define,
    /// which the never-silent path parks at a human gate.
    const FAILED_MARKER: &str = r#"DEVFLOW_RESULT: {"status":"failed","reason":"gate fixture"}"#;

    /// Signal stage N's supervisor by the pid in its sidecar, after
    /// confirming that pid is still the recorded process.
    fn signal_supervisor(run: &HandoffRun, signal: libc::c_int) {
        let sidecar =
            devflow_core::supervisor::read_sidecar(&run.n.socket_path.with_extension("json"))
                .expect("N's sidecar");
        let pid = sidecar.supervisor_pid;
        assert!(
            devflow_core::agent::is_same_process(pid, sidecar.supervisor_start_time),
            "the sidecar's supervisor {pid} is the recorded process"
        );
        let signed = libc::pid_t::try_from(pid).expect("pid fits pid_t");
        assert!(signed > 1, "refusing to signal pid {signed}");
        // SAFETY: `signed > 1` names the supervisor just identity-checked.
        assert_eq!(unsafe { libc::kill(signed, signal) }, 0, "signal {pid}");
    }

    /// Launch a gate fixture and return once N is parked at its Define gate:
    /// the gate file exists and N holds the lock (its owner phase is the
    /// control's assertion, not a precondition here).
    fn launch_parked_at_gate(phase: PhaseId) -> HandoffRun {
        let run = HandoffRun::launch(phase);
        run.go();
        let gate = Gates::gate_path(&run.root, phase, Stage::Define);
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || gate.exists()
                && events_named(&run.root, "gate_fired").len() == 1),
            "N parks at the Define gate: {}",
            run.events()
        );
        assert!(
            matches!(
                lock::holder_status(&run.root, phase),
                lock::HolderStatus::Live { .. }
            ),
            "N holds the per-phase lock across the gate wait"
        );
        run
    }

    /// V-15 (R-2, T-50-33): SIGTERM to a supervisor parked at a gate aborts
    /// the wait (safe point 4): within 5 s the lock has no holder, the state
    /// is stopped with the safe-point reason, the socket is gone and no next
    /// stage was launched.
    #[test]
    fn a_signal_during_a_gate_wait_aborts_and_releases_the_lock() {
        const NAME: &str =
            "pipeline_launch::tests::a_signal_during_a_gate_wait_aborts_and_releases_the_lock";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, None, FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = launch_parked_at_gate(PhaseId::new(1310));
        signal_supervisor(&run, libc::SIGTERM);
        let ended = poll_until(Duration::from_secs(5), || {
            matches!(
                lock::holder_status(&run.root, run.phase),
                lock::HolderStatus::NoHolder
            ) && devflow_core::supervisor::probe(&run.n.socket_path)
                .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
        });
        assert!(
            ended,
            "within 5 s of SIGTERM the lock is released and the socket gone: {}",
            run.events()
        );
        let state = run.state();
        assert!(state.stopped, "state.stopped after a signal at the gate");
        assert_eq!(state.stop_reason.as_deref(), Some(ADVANCE_STOP_REASON));
        assert_eq!(
            stage_launched_count(&run.root, run.phase),
            1,
            "{}",
            run.events()
        );
        let stopped = events_named(&run.root, "advance_stopped");
        assert_eq!(stopped.len(), 1, "{}", run.events());
        assert_eq!(stopped[0]["safe_point"], 4, "{}", stopped[0]);
    }

    /// The V-15 control: with no signal the gate wait holds (lock held, N in
    /// `gate_wait`) after 3 s, and answering the gate lets it proceed: an
    /// abort answer releases the lock and ends the supervisor.
    #[test]
    fn without_a_signal_the_gate_wait_holds_until_answered() {
        const NAME: &str =
            "pipeline_launch::tests::without_a_signal_the_gate_wait_holds_until_answered";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, None, FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = launch_parked_at_gate(PhaseId::new(1311));
        std::thread::sleep(Duration::from_secs(3));
        assert!(
            matches!(
                lock::holder_status(&run.root, run.phase),
                lock::HolderStatus::Live { .. }
            ),
            "control: still parked with the lock held after 3 s"
        );
        assert!(
            matches!(
                devflow_core::supervisor::probe(&run.n.socket_path),
                Ok(devflow_core::supervisor::Liveness::Alive(ref reply)) if reply.owner == "gate_wait"
            ),
            "control: N still answers gate_wait"
        );
        assert!(!run.state().stopped, "control: not stopped while waiting");
        Gates::respond(
            &run.root,
            run.phase,
            Stage::Define,
            &devflow_core::gates::GateResponse {
                approved: false,
                note: Some("abort: control answer".to_string()),
                responded_by: Some("test".to_string()),
            },
        )
        .unwrap();
        let ended = poll_until(HANDOFF_STEP_WAIT, || {
            matches!(
                lock::holder_status(&run.root, run.phase),
                lock::HolderStatus::NoHolder
            ) && devflow_core::supervisor::probe(&run.n.socket_path)
                .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
        });
        assert!(
            ended,
            "control: the answered gate proceeds: {}",
            run.events()
        );
        assert_eq!(events_named(&run.root, "workflow_aborted").len(), 1);
        assert!(events_named(&run.root, "advance_stopped").is_empty());
    }

    // ---- CR-01 (50-REVIEW.md): a stop before a `--yes-ship` merge ----

    /// Whether `branch` exists in the repo at `root`.
    fn branch_exists(root: &Path, branch: &str) -> bool {
        devflow_core::test_support::git_command(root)
            .args(["rev-parse", "--verify", "--quiet", branch])
            .status()
            .unwrap()
            .success()
    }

    /// The body of the `--yes-ship` pair: stage N is Ship with `--yes-ship`
    /// and a feature branch finalization can merge, and its advance is held
    /// at the configured point. With `stop`, a socket stop is taken there
    /// (owner `advancing`, so it is only flagged) before the release: the
    /// advance must ship nothing, record the stop at `safe_point` and answer
    /// the caller `stopped`. Without it (the control) the run ships.
    fn held_yes_ship_advance(phase: PhaseId, stop: bool, safe_point: u8) {
        let _guard = env_lock();
        let branch = format!("feature/phase-{}", phase.padded());
        let run = HandoffRun::launch_with(phase, |root, state| {
            assert!(
                devflow_core::test_support::git_command(root)
                    .args(["branch", &branch, "develop"])
                    .status()
                    .unwrap()
                    .success()
            );
            state.stage = Stage::Ship;
            state.yes_ship = true;
            state.stop_until = None;
        });
        run.go();
        run.wait_held();
        if stop {
            let (accepted, last) = shutdown_accepted_then(
                &run.n.socket_path,
                || run.release(),
                Duration::from_secs(30),
            );
            assert_eq!(accepted["owner"], "advancing", "{accepted}");
            assert!(
                events_named(&run.root, "workflow_shipped").is_empty(),
                "a stop accepted before the --yes-ship merge must ship nothing: {}",
                run.events()
            );
            assert!(
                branch_exists(&run.root, &branch),
                "no after-ship hook may run: the feature branch is kept"
            );
            let state = run.state();
            assert!(
                state.stopped,
                "state.stopped after a stop at safe point {safe_point}"
            );
            assert_eq!(state.stop_reason.as_deref(), Some(ADVANCE_STOP_REASON));
            assert!(!state.gate_pending, "no gate is left pending");
            assert!(!Gates::response_path(&run.root, run.phase, Stage::Ship).exists());
            let stopped = events_named(&run.root, "advance_stopped");
            assert_eq!(stopped.len(), 1, "{}", run.events());
            assert_eq!(stopped[0]["safe_point"], safe_point, "{}", stopped[0]);
            assert_eq!(
                last["stopped"], true,
                "the stop's caller reads stopped: {last}"
            );
        } else {
            run.release();
            assert!(
                poll_until(HANDOFF_STEP_WAIT, || events_named(
                    &run.root,
                    "workflow_shipped"
                )
                .len()
                    == 1),
                "control: without a stop the --yes-ship run ships: {}",
                run.events()
            );
            assert!(
                !branch_exists(&run.root, &branch),
                "control: the after-ship hooks ran (the feature branch is cleaned up)"
            );
            assert!(events_named(&run.root, "advance_stopped").is_empty());
        }
    }

    /// CR-01, safe point 5: a stop accepted after safe point 1 and before
    /// `--yes-ship` writes its approval stops the run there. The pre-written
    /// approval would otherwise beat the stop in the gate wait and let
    /// finalization merge, bump, tag and push.
    #[test]
    fn a_stop_after_safe_point_1_is_honoured_before_the_yes_ship_approval() {
        const NAME: &str = "pipeline_launch::tests::a_stop_after_safe_point_1_is_honoured_before_the_yes_ship_approval";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), SUCCESS_MARKER);
            return;
        }
        held_yes_ship_advance(PhaseId::new(1314), true, 5);
    }

    /// The control for safe point 5: the same hold and no stop, and the
    /// `--yes-ship` run ships.
    #[test]
    fn without_a_stop_after_safe_point_1_the_yes_ship_run_ships() {
        const NAME: &str =
            "pipeline_launch::tests::without_a_stop_after_safe_point_1_the_yes_ship_run_ships";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), SUCCESS_MARKER);
            return;
        }
        held_yes_ship_advance(PhaseId::new(1315), false, 5);
    }

    /// CR-01, safe point 6: a stop accepted after the Ship gate was answered
    /// and before finalization stops the run before the merge.
    #[test]
    fn a_stop_before_finalization_is_honoured_before_the_merge() {
        const NAME: &str =
            "pipeline_launch::tests::a_stop_before_finalization_is_honoured_before_the_merge";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_finalization"), SUCCESS_MARKER);
            return;
        }
        held_yes_ship_advance(PhaseId::new(1316), true, 6);
    }

    /// The control for safe point 6: the same hold and no stop, and
    /// finalization ships.
    #[test]
    fn without_a_stop_before_finalization_the_run_ships() {
        const NAME: &str =
            "pipeline_launch::tests::without_a_stop_before_finalization_the_run_ships";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_finalization"), SUCCESS_MARKER);
            return;
        }
        held_yes_ship_advance(PhaseId::new(1317), false, 6);
    }

    /// R-10: a SIGKILL of N between N+1's publish and N's unlink (N parked
    /// at `after_publish`) leaves the state naming the live N+1, and N's
    /// leftover socket probes Stale with a sidecar whose launch id differs
    /// from the state's.
    #[test]
    fn a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor()
    {
        const NAME: &str = "pipeline_launch::tests::a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_publish"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1312));
        run.go();
        run.wait_held();
        signal_supervisor(&run, libc::SIGKILL);
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || matches!(
                devflow_core::supervisor::probe(&run.n.socket_path),
                Ok(devflow_core::supervisor::Liveness::Stale)
            )),
            "N's leftover socket probes Stale after the kill"
        );
        let state = run.state();
        let next = state.supervisor.expect("the state names a supervisor");
        assert_ne!(next.launch_id, run.n.launch_id, "the state names N+1");
        match devflow_core::supervisor::probe(&next.socket_path) {
            Ok(devflow_core::supervisor::Liveness::Alive(reply)) => {
                assert_eq!(reply.launch_id, next.launch_id);
            }
            other => panic!("N+1 must be Alive after N's kill, got {other:?}"),
        }
        let leftover =
            devflow_core::supervisor::read_sidecar(&run.n.socket_path.with_extension("json"))
                .expect("N's sidecar is left behind");
        assert_ne!(
            leftover.launch_id, next.launch_id,
            "N's leftover sidecar names a launch other than the state's"
        );
        assert_eq!(leftover.launch_id, run.n.launch_id);
    }

    // ---- 50-23: every launch failure path has a defined end state ----

    /// A previous stage's supervisor record whose socket does not exist and
    /// whose agent pgid does not fit `pid_t`, so no reaper can signal it.
    fn fabricated_prior_record(dir: &Path) -> SupervisorRecord {
        SupervisorRecord::new(
            dir.join("prior-stage-never-bound.sock"),
            "priorstage0000000000".to_string(),
            4_294_967_000,
            1,
            String::new(),
        )
    }

    /// Names which run of `a_failed_monitor_spawn_clears_the_supervisor_record`
    /// a child is (`fail` or `ok`). Not a DevFlow setting, so it carries no
    /// `DEVFLOW_` prefix the doc check would require documented.
    const SPAWN_CASE_ENV: &str = "P50_23_SPAWN_CASE";

    /// WR-04's supervisor equivalent (T-50-62): a launch whose
    /// `spawn_monitor` fails before spawning anything (its socket path is
    /// over `sun_path`, so `SocketPath::for_launch` refuses it) clears the
    /// previous stage's record in memory and on disk, and leaves no socket or
    /// sidecar under the cache dir. Control: the same launch under a short
    /// cache dir records a new record with a new launch id.
    #[test]
    fn a_failed_monitor_spawn_clears_the_supervisor_record() {
        const NAME: &str =
            "pipeline_launch::tests::a_failed_monitor_spawn_clears_the_supervisor_record";
        if !devflow_core::test_support::in_child_test(NAME) {
            for case in ["fail", "ok"] {
                let path_dir = go_waiting_claude_dir();
                let short = crate::test_support::hermetic_cache_dir();
                let cache = if case == "fail" {
                    let long = short.path().join("c".repeat(80));
                    std::fs::create_dir_all(&long).unwrap();
                    long
                } else {
                    short.path().to_path_buf()
                };
                let env: Vec<(&str, &std::ffi::OsStr)> = vec![
                    ("DEVFLOW_CACHE_DIR", cache.as_os_str()),
                    (SPAWN_CASE_ENV, std::ffi::OsStr::new(case)),
                ];
                let output =
                    devflow_core::test_support::run_test_in_child(NAME, path_dir.path(), &env);
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, NAME,
                );
            }
            return;
        }
        let _guard = env_lock();
        let cache = PathBuf::from(
            std::env::var_os("DEVFLOW_CACHE_DIR").expect("the parent passes DEVFLOW_CACHE_DIR"),
        );
        let fail = std::env::var(SPAWN_CASE_ENV).expect("the parent names the case") == "fail";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(2301);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.legacy_claude_launch = true;
        let prior = fabricated_prior_record(root);
        state.supervisor = Some(prior.clone());
        workflow::save_state(&state).unwrap();

        let result = launch_stage(&mut state, None, None);
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);
        let reloaded = workflow::load_state(root, phase).unwrap();
        if fail {
            let err = result.expect_err("a socket path over sun_path cannot be bound");
            assert!(
                err.to_string().contains("sun_path"),
                "the error names the socket path bound: {err}"
            );
            assert_eq!(
                state.supervisor, None,
                "the failed spawn clears the prior record in memory"
            );
            assert_eq!(
                reloaded.supervisor, None,
                "the failed spawn clears the prior record on disk"
            );
            let left: Vec<PathBuf> =
                std::fs::read_dir(devflow_core::registry::socket_dir_in(&cache))
                    .map(|entries| entries.filter_map(|e| Some(e.ok()?.path())).collect())
                    .unwrap_or_default();
            assert!(left.is_empty(), "no socket or sidecar is left: {left:?}");
        } else {
            result.expect("control: the launch succeeds under a short cache dir");
            let record = state
                .supervisor
                .clone()
                .expect("control: the launch records its supervisor");
            assert_ne!(
                record.launch_id, prior.launch_id,
                "control: a new launch id replaces the prior one"
            );
            assert_eq!(reloaded.supervisor.as_ref(), Some(&record));
            let outcome = devflow_core::supervisor::request_shutdown(
                &record.socket_path,
                SUPERVISOR_STOP_WAIT,
            );
            assert!(
                matches!(
                    outcome,
                    Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
                ),
                "control: the launched supervisor stops through its socket: {outcome:?}"
            );
        }
    }

    /// `source`'s non-test lines: the part before the first line that is
    /// exactly `mod tests {`, without its whole-line `//` comments. A
    /// trailing comment stays, as part of its code line.
    fn non_test_code_lines(source: &str) -> Vec<&str> {
        source
            .lines()
            .take_while(|line| *line != "mod tests {")
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect()
    }

    /// The lines of the item whose header line starts with `header`, through
    /// the next line starting with `closing`; `None` when no line starts with
    /// `header`.
    fn item_lines<'a>(lines: &[&'a str], header: &str, closing: &str) -> Option<Vec<&'a str>> {
        let start = lines.iter().position(|line| line.starts_with(header))?;
        let mut item = vec![lines[start]];
        for line in &lines[start + 1..] {
            item.push(*line);
            if line.starts_with(closing) {
                break;
            }
        }
        Some(item)
    }

    const CLEAR_SPAWN_NOT_FOUND: &str =
        "spawn_agent_and_record not found in the non-test code (or under 40 lines)";
    const CLEAR_SPAWN_CLEARS: &str =
        "spawn_agent_and_record clears the supervisor record in its own body";
    const CLEAR_SPAWN_NO_HELPER: &str = "spawn_agent_and_record never calls record_failed_launch";
    const CLEAR_REFUSE_NOT_FOUND: &str =
        "refuse_launch not found in the non-test code (or under 3 lines)";
    const CLEAR_REFUSE_NO_CLEAR: &str = "refuse_launch does not clear the supervisor record";

    /// 50-13's rule as a pure scan over `source` (R6-G4): in the non-test,
    /// comment-stripped code, `spawn_agent_and_record` never clears the
    /// supervisor record itself (only its failure helper does) and calls
    /// `record_failed_launch`, and `refuse_launch` clears the record.
    fn launch_record_clear_violations(source: &str) -> Vec<String> {
        let lines = non_test_code_lines(source);
        let has = |item: &[&str], needle: &str| item.iter().any(|line| line.contains(needle));
        let mut violations = Vec::new();
        match item_lines(&lines, "fn spawn_agent_and_record(", "}").filter(|item| item.len() >= 40)
        {
            None => violations.push(CLEAR_SPAWN_NOT_FOUND.to_string()),
            Some(item) => {
                if has(&item, "supervisor = None") {
                    violations.push(CLEAR_SPAWN_CLEARS.to_string());
                }
                if !has(&item, "record_failed_launch(") {
                    violations.push(CLEAR_SPAWN_NO_HELPER.to_string());
                }
            }
        }
        match item_lines(&lines, "fn refuse_launch(", "}").filter(|item| item.len() >= 3) {
            None => violations.push(CLEAR_REFUSE_NOT_FOUND.to_string()),
            Some(item) if !has(&item, "supervisor = None") => {
                violations.push(CLEAR_REFUSE_NO_CLEAR.to_string());
            }
            Some(_) => {}
        }
        violations
    }

    /// T-50-62 (permanent source scan, R6-G4): the supervisor record is
    /// cleared only on a launch failure path. The real file yields no
    /// violation; planted sources, each padded to the floors so it carries
    /// one defect, yield exactly that defect's violation.
    #[test]
    fn the_supervisor_record_is_cleared_only_on_launch_failure_paths() {
        let real = launch_record_clear_violations(include_str!("pipeline_launch.rs"));
        assert!(real.is_empty(), "pipeline_launch.rs: {real:?}");

        let pad = |n: usize| "    let _filler = 0;\n".repeat(n);
        let spawn = |body: &str| format!("fn spawn_agent_and_record(\n{}{body}}}\n", pad(40));
        let refuse = |body: &str| {
            format!(
                "fn refuse_launch(state: &mut State) {{\n{}{body}}}\n",
                pad(2)
            )
        };
        let good_spawn = spawn("    return Err(failed(record_failed_launch(state), err));\n");
        let good_refuse = refuse("    state.supervisor = None;\n");
        let only = |violation: &str| vec![violation.to_string()];

        assert_eq!(
            launch_record_clear_violations(&format!("{good_spawn}{good_refuse}")),
            Vec::<String>::new(),
            "a correct layout"
        );
        assert_eq!(
            launch_record_clear_violations(&format!(
                "{}{good_refuse}",
                spawn("    record_failed_launch(state)?;\n    state.supervisor = None;\n")
            )),
            only(CLEAR_SPAWN_CLEARS)
        );
        assert_eq!(
            launch_record_clear_violations(&format!(
                "{}{good_refuse}",
                spawn("    record_failed_launch(state)?; // never: state.supervisor = None\n")
            )),
            only(CLEAR_SPAWN_CLEARS),
            "a trailing comment is part of its code line"
        );
        assert_eq!(
            launch_record_clear_violations(&format!(
                "{}{good_refuse}",
                spawn("    // record_failed_launch(state)?;\n")
            )),
            only(CLEAR_SPAWN_NO_HELPER),
            "a commented-out call is not a call"
        );
        assert_eq!(
            launch_record_clear_violations(&format!("{good_spawn}{}", refuse(""))),
            only(CLEAR_REFUSE_NO_CLEAR)
        );
        assert_eq!(
            launch_record_clear_violations(&format!("mod tests {{\n{good_spawn}{good_refuse}")),
            vec![
                CLEAR_SPAWN_NOT_FOUND.to_string(),
                CLEAR_REFUSE_NOT_FOUND.to_string()
            ],
            "functions only after the test-module cut are not found"
        );
        assert_eq!(
            launch_record_clear_violations(&format!(
                "fn spawn_agent_and_record(\n{}    record_failed_launch(state)?;\n}}\n{good_refuse}",
                pad(5)
            )),
            only(CLEAR_SPAWN_NOT_FOUND),
            "a body under its floor"
        );
    }

    /// A launch context's supervisor, spawned straight through
    /// `spawn_monitor` on the supervise-only harness entry for a fake agent
    /// that waits for `go` (never created), with a reaper for it. The state
    /// on disk holds `prior` and is left untouched.
    struct PublishFixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        phase: PhaseId,
        state: State,
        prior: SupervisorRecord,
        published: Option<monitor::Published>,
        reaper: Option<ReapMonitorOnDrop>,
    }

    impl PublishFixture {
        fn spawn(phase: PhaseId) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            init_repo(&root);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.clone());
            state.legacy_claude_launch = true;
            let prior = fabricated_prior_record(&root);
            state.supervisor = Some(prior.clone());
            workflow::save_state(&state).unwrap();
            let ctx = launch_context().expect("the parent passes DEVFLOW_CACHE_DIR");
            let published = monitor::spawn_monitor(
                &state,
                "claude",
                &[],
                &[],
                monitor::MonitorLaunch::Legacy,
                &ctx,
            )
            .expect("the next supervisor publishes");
            let mut holder = state.clone();
            holder.supervisor = Some(published.record.clone());
            let reaper = Some(ReapMonitorOnDrop::after_launch(&holder));
            Self {
                _dir: dir,
                root,
                phase,
                state,
                prior,
                published: Some(published),
                reaper,
            }
        }

        fn take(&mut self) -> monitor::Published {
            self.published.take().expect("published once")
        }

        fn reloaded(&self) -> State {
            workflow::load_state(&self.root, self.phase).expect("the state loads")
        }
    }

    impl Drop for PublishFixture {
        fn drop(&mut self) {
            drop(self.reaper.take());
        }
    }

    /// A save that always fails, as a full disk would.
    fn failing_save(_: &State) -> Result<(), CliError> {
        Err(CliError::Workflow(
            devflow_core::workflow::WorkflowError::Io(std::io::Error::other(
                "injected save failure: no space left on device",
            )),
        ))
    }

    fn probes_alive(socket: &Path) -> bool {
        matches!(
            devflow_core::supervisor::probe(socket),
            Ok(devflow_core::supervisor::Liveness::Alive(_))
        )
    }

    fn probes_gone(socket: &Path) -> bool {
        devflow_core::supervisor::probe(socket)
            .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
    }

    /// The parent half of a publish-save test: a child on the supervise-only
    /// entry whose fake agent waits for `go` in its working directory.
    fn run_publish_child(name: &str) {
        let path_dir = go_waiting_claude_dir();
        let output = run_hermetic_test_in_child(name, path_dir.path(), &[]);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
    }

    /// RV-08 (V-38, T-50-63): a failed publish save whose shutdown of the new
    /// supervisor is not confirmed (its recorded socket refuses the connect)
    /// keeps the new record in memory, saves nothing over the file's record,
    /// and names `devflow stop --phase N --launch <id>`. The new supervisor
    /// is still running. Its opposite is the next test, whose confirmed
    /// shutdown clears the record and names no repair.
    #[test]
    fn a_failed_publish_save_whose_shutdown_is_not_confirmed_names_the_stop_launch_repair() {
        const NAME: &str = "pipeline_launch::tests::a_failed_publish_save_whose_shutdown_is_not_confirmed_names_the_stop_launch_repair";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_publish_child(NAME);
            return;
        }
        let _guard = env_lock();
        let mut fixture = PublishFixture::spawn(PhaseId::new(2302));
        let mut published = fixture.take();
        let real = published.record.socket_path.clone();
        let new_id = published.record.launch_id.clone();
        let refused = fixture.root.join("refused.sock");
        drop(std::os::unix::net::UnixListener::bind(&refused).expect("bind then drop"));
        published.record.socket_path = refused.clone();

        let err = publish_record(&mut fixture.state, published, failing_save)
            .err()
            .expect("a failed save is an error");
        let text = err.to_string();
        assert!(
            text.contains("injected save failure"),
            "names the save error: {text}"
        );
        let repair = format!("devflow stop --phase {} --launch {new_id}", fixture.phase);
        assert!(text.contains(&repair), "names `{repair}`: {text}");
        assert!(
            text.contains(&refused.display().to_string()),
            "names the socket: {text}"
        );
        assert_eq!(
            fixture.state.supervisor.as_ref().map(|r| &r.launch_id),
            Some(&new_id),
            "the in-memory record still names the new launch"
        );
        assert_eq!(
            fixture.reloaded().supervisor.as_ref(),
            Some(&fixture.prior),
            "nothing was saved over the file's record"
        );
        assert!(probes_alive(&real), "the new supervisor is still running");
        let outcome = devflow_core::supervisor::request_shutdown(&real, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                outcome,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "the operator's handle still ends it: {outcome:?}"
        );
    }

    /// R-1 step 4 (T-50-63): a failed publish save shuts the new supervisor
    /// down through its socket before returning the error: its socket reads
    /// Gone, its agent group is gone, and the record does not name it.
    #[test]
    fn a_failed_publish_save_shuts_the_new_supervisor_down() {
        const NAME: &str =
            "pipeline_launch::tests::a_failed_publish_save_shuts_the_new_supervisor_down";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_publish_child(NAME);
            return;
        }
        let _guard = env_lock();
        let mut fixture = PublishFixture::spawn(PhaseId::new(2303));
        let published = fixture.take();
        let record = published.record.clone();

        let err = publish_record(&mut fixture.state, published, failing_save)
            .err()
            .expect("a failed save is an error");
        let text = err.to_string();
        assert!(
            text.contains("injected save failure"),
            "names the save error: {text}"
        );
        assert!(
            text.contains("shut down"),
            "names the shutdown of the new supervisor: {text}"
        );
        assert!(
            !text.contains("--launch"),
            "a confirmed shutdown needs no repair line: {text}"
        );
        assert!(
            poll_until(Duration::from_secs(5), || probes_gone(&record.socket_path)),
            "the new supervisor's socket reads Gone"
        );
        assert!(
            poll_until(Duration::from_secs(5), || !group_alive(record.agent_pgid)),
            "the new supervisor's agent group is gone"
        );
        assert_ne!(
            fixture.state.supervisor.as_ref().map(|r| &r.launch_id),
            Some(&record.launch_id),
            "the record does not name the shut-down supervisor"
        );
    }

    /// The control for the failed-save pair: with `workflow::save_state` the
    /// publish hands off, the reloaded state names the new launch and the
    /// new supervisor probes Alive.
    #[test]
    fn a_successful_publish_save_records_and_keeps_the_new_supervisor() {
        const NAME: &str = "pipeline_launch::tests::a_successful_publish_save_records_and_keeps_the_new_supervisor";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_publish_child(NAME);
            return;
        }
        let _guard = env_lock();
        let mut fixture = PublishFixture::spawn(PhaseId::new(2304));
        let published = fixture.take();
        let record = published.record.clone();

        let outcome = publish_record(&mut fixture.state, published, workflow::save_state)
            .expect("control: the save succeeds");
        assert!(
            matches!(outcome, PublishOutcome::HandedOff),
            "control: a running successor is handed off"
        );
        assert_eq!(
            fixture.reloaded().supervisor.as_ref(),
            Some(&record),
            "control: the reloaded state names the new launch"
        );
        assert!(!fixture.reloaded().stopped, "control: not stopped");
        assert!(
            probes_alive(&record.socket_path),
            "control: the new supervisor is running"
        );
        let shutdown =
            devflow_core::supervisor::request_shutdown(&record.socket_path, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                shutdown,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "{shutdown:?}"
        );
    }

    /// The next supervisor a held stage N spawned: the one sweep entry whose
    /// launch id is not N's, with its sidecar.
    fn next_supervisor(run: &HandoffRun) -> (PathBuf, devflow_core::supervisor::Sidecar) {
        let found: Vec<_> = devflow_core::supervisor::sweep_in(&run.cache)
            .into_iter()
            .filter_map(|entry| {
                let sidecar = entry.sidecar?;
                (sidecar.launch_id != run.n.launch_id).then_some((entry.socket_path, sidecar))
            })
            .collect();
        match <[_; 1]>::try_from(found) {
            Ok([one]) => one,
            Err(other) => panic!("exactly one next supervisor at the hold: {other:?}"),
        }
    }

    /// Release N, then wait for its socket to read Gone (its save, probe and
    /// `finish` are done).
    fn release_and_wait_for_n(run: &HandoffRun) {
        run.release();
        assert!(
            poll_until(SUPERVISOR_STOP_WAIT, || probes_gone(&run.n.socket_path)),
            "N finishes once released: {}",
            run.events()
        );
    }

    /// T-50-64: a stop sent straight to N+1 while N is held before its
    /// publish save (so N+1 ends on 50-13's no-write path, the state still
    /// naming N) is recorded by N's post-save re-probe: `stopped` with this
    /// plan's reason, the state naming N+1, which reads Gone with no agent
    /// left.
    #[test]
    fn a_stop_sent_straight_to_the_next_supervisor_before_its_record_is_saved_is_recorded() {
        const NAME: &str = "pipeline_launch::tests::a_stop_sent_straight_to_the_next_supervisor_before_its_record_is_saved_is_recorded";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_publish"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(2305));
        run.go();
        run.wait_held();
        let (next_socket, next) = next_supervisor(&run);
        let outcome =
            devflow_core::supervisor::request_shutdown(&next_socket, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                outcome,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "N+1 stops: {outcome:?}"
        );
        release_and_wait_for_n(&run);

        let state = run.state();
        assert_eq!(
            state.supervisor.as_ref().map(|r| &r.launch_id),
            Some(&next.launch_id),
            "the state names N+1"
        );
        assert!(
            state.stopped,
            "the stop is recorded (stop_reason {:?})",
            state.stop_reason
        );
        assert_eq!(
            state.stop_reason.as_deref(),
            Some(NEXT_STOPPED_BEFORE_SAVE_REASON)
        );
        assert!(probes_gone(&next_socket), "N+1 reads Gone");
        assert!(
            poll_until(Duration::from_secs(5), || !group_alive(next.agent_pgid)),
            "no agent of N+1 survives"
        );
    }

    /// The control: the same held publish with no stop hands off, so the
    /// state names a running N+1 and is not stopped.
    #[test]
    fn without_a_stop_to_the_next_supervisor_the_held_publish_hands_off() {
        const NAME: &str = "pipeline_launch::tests::without_a_stop_to_the_next_supervisor_the_held_publish_hands_off";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_publish"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(2306));
        run.go();
        run.wait_held();
        let (next_socket, next) = next_supervisor(&run);
        release_and_wait_for_n(&run);

        let state = run.state();
        assert_eq!(
            state.supervisor.as_ref().map(|r| &r.launch_id),
            Some(&next.launch_id),
            "control: the state names N+1"
        );
        assert!(
            !state.stopped,
            "control: not stopped ({:?})",
            state.stop_reason
        );
        assert!(probes_alive(&next_socket), "control: N+1 runs on");
    }

    /// The stopped-outcome hold variable (R-6E): the path a stopped
    /// supervisor waits on between its agent's exit and its outcome decision.
    const STOPPED_OUTCOME_HOLD_ENV: &str = "DEVFLOW_TEST_STOPPED_OUTCOME_HOLD";

    /// Names which run of the kill-grace test a child is (`a` or `b`). Not a
    /// DevFlow setting, so no `DEVFLOW_` prefix.
    const KILL_GRACE_RUN_ENV: &str = "P50_23_KILL_GRACE_RUN";

    /// The marker N+1's fake agent leaves when SIGTERM reaches it.
    const TERM_SEEN: &str = "term-seen";

    /// [`handoff_claude_dir`] with a stage N+1 agent that traps SIGTERM by
    /// leaving [`TERM_SEEN`] in its working directory and running on, so its
    /// supervisor's kill grace ends in SIGKILL.
    fn term_trapping_handoff_claude_dir() -> tempfile::TempDir {
        let path_dir = handoff_claude_dir(SUCCESS_MARKER);
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
             if [ ! -e first-done ]; then\n  : > first-done\n  gate=go\n\
             else\n  trap ': > {TERM_SEEN}' TERM\n  gate=go2\nfi\n\
             i=0\nwhile [ ! -e \"$gate\" ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
             printf '%s\\n' '{SUCCESS_MARKER}'\nexit 0\n"
        );
        std::fs::write(path_dir.path().join("claude"), script).unwrap();
        path_dir
    }

    /// Creates its path when dropped, so a failing kill-grace run never
    /// leaves N+1 parked at the stopped-outcome hold.
    struct ReleaseOnDrop(PathBuf);

    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, "");
        }
    }

    /// Wait (bounded) for `path` to exist.
    fn wait_for_file(path: &Path, within: Duration) -> bool {
        poll_until(within, || path.exists())
    }

    /// R6-E (V-50, T-50-64): a stop N+1 has accepted but not carried out
    /// when N saves its record is recorded by N+1 itself. Ordered only by
    /// events: N held at `before_publish`, the stop sent to N+1, N+1's
    /// agent marker, N+1 parked at the stopped-outcome hold (owner still
    /// `agent_running`), then the two releases. Run (a) releases N first:
    /// N saves, probes N+1 (running) and hands off with no stop written,
    /// then N+1 records its own stop. Run (b), the in-test control, releases
    /// N+1 first: it ends on the no-write path and N's re-probe records this
    /// plan's reason. The release order alone selects the end state.
    #[test]
    fn a_next_supervisor_still_in_its_kill_grace_at_publish_records_its_own_stop() {
        const NAME: &str = "pipeline_launch::tests::a_next_supervisor_still_in_its_kill_grace_at_publish_records_its_own_stop";
        if !devflow_core::test_support::in_child_test(NAME) {
            for run in ["a", "b"] {
                let path_dir = term_trapping_handoff_claude_dir();
                let hold_dir = tempfile::Builder::new()
                    .prefix("dfh")
                    .tempdir_in("/tmp")
                    .unwrap();
                let n_hold = std::ffi::OsString::from(format!(
                    "before_publish:{}",
                    hold_dir.path().join("release").display()
                ));
                let k_file = hold_dir.path().join("k-release");
                let env: Vec<(&str, &std::ffi::OsStr)> = vec![
                    (TEST_MONITOR_FULL_ENV, std::ffi::OsStr::new("1")),
                    ("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("120")),
                    (HOLD_ENV, n_hold.as_os_str()),
                    (STOPPED_OUTCOME_HOLD_ENV, k_file.as_os_str()),
                    (KILL_GRACE_RUN_ENV, std::ffi::OsStr::new(run)),
                ];
                let output = run_hermetic_test_in_child(NAME, path_dir.path(), &env);
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, NAME,
                );
            }
            return;
        }
        let _guard = env_lock();
        let release_first_n = std::env::var(KILL_GRACE_RUN_ENV).expect("the run") == "a";
        let k_file = PathBuf::from(
            std::env::var_os(STOPPED_OUTCOME_HOLD_ENV).expect("the parent passes the hold"),
        );
        let k_held = PathBuf::from(format!("{}.held", k_file.display()));
        let step = SUPERVISOR_STOP_WAIT;
        let run = HandoffRun::launch(PhaseId::new(if release_first_n { 2307 } else { 2308 }));
        let _release_k = ReleaseOnDrop(k_file.clone());
        run.go();
        run.wait_held();
        let (next_socket, next) = next_supervisor(&run);
        let stopper = {
            let socket = next_socket.clone();
            std::thread::spawn(move || {
                devflow_core::supervisor::request_shutdown(&socket, Duration::from_secs(60))
            })
        };
        assert!(
            wait_for_file(&run.root.join(TERM_SEEN), Duration::from_secs(5)),
            "N+1 accepted the stop and signalled its agent group"
        );
        assert!(
            wait_for_file(&k_held, step),
            "stopped-outcome hold never reached"
        );
        let mut owner = None;
        let answered = poll_until(step, || {
            match devflow_core::supervisor::probe(&next_socket) {
                Ok(devflow_core::supervisor::Liveness::Alive(reply)) => {
                    owner = Some(reply.owner);
                    true
                }
                Ok(other) => panic!("N+1 must be Alive at the hold, found {other:?}"),
                Err(_) => false,
            }
        });
        assert!(answered, "N+1 answers a ping while parked at the hold");
        assert_eq!(
            owner.as_deref(),
            Some("agent_running"),
            "expected owner agent_running, found {owner:?}"
        );

        let ended = |socket: &Path| poll_until(step, || probes_gone(socket));
        if release_first_n {
            release_and_wait_for_n(&run);
            let state = run.state();
            assert_eq!(
                state.supervisor.as_ref().map(|r| &r.launch_id),
                Some(&next.launch_id),
                "N saved N+1's record"
            );
            assert!(
                !state.stopped,
                "N handed off and wrote no stop: found stopped {} with stop_reason {:?}",
                state.stopped, state.stop_reason
            );
            std::fs::write(&k_file, "").unwrap();
            assert!(ended(&next_socket), "N+1 finishes once released");
        } else {
            std::fs::write(&k_file, "").unwrap();
            assert!(ended(&next_socket), "N+1 finishes once released");
            release_and_wait_for_n(&run);
        }
        let outcome = stopper.join().expect("the stopper thread");
        assert!(
            matches!(
                outcome,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "the stop sent to N+1 reads Stopped: {outcome:?}"
        );
        let state = run.state();
        assert_eq!(
            state.supervisor.as_ref().map(|r| &r.launch_id),
            Some(&next.launch_id),
            "the state names N+1"
        );
        assert!(state.stopped, "the stop is recorded");
        let expected = if release_first_n {
            SUPERVISOR_STOP_REASON
        } else {
            NEXT_STOPPED_BEFORE_SAVE_REASON
        };
        assert_eq!(state.stop_reason.as_deref(), Some(expected));
        assert!(
            poll_until(Duration::from_secs(5), || !group_alive(next.agent_pgid)),
            "no agent of N+1 survives"
        );
    }

    // ---- 50-23 Task 3: a stop after safe point 3 is forwarded (RV-04) ----

    /// Names which run of the forward test a child is (`a` to `d`). Not a
    /// DevFlow setting, so no `DEVFLOW_` prefix.
    const FORWARD_RUN_ENV: &str = "P50_23_FORWARD_RUN";

    /// The parent half of a forward test: [`run_handoff_child`] with the run
    /// label passed to the child.
    fn run_forward_child(name: &str, hold: &str, label: &str) {
        let path_dir = handoff_claude_dir(SUCCESS_MARKER);
        let hold_dir = tempfile::Builder::new()
            .prefix("dfh")
            .tempdir_in("/tmp")
            .unwrap();
        let hold_value = std::ffi::OsString::from(format!(
            "{hold}:{}",
            hold_dir.path().join("release").display()
        ));
        let env: Vec<(&str, &std::ffi::OsStr)> = vec![
            (TEST_MONITOR_FULL_ENV, std::ffi::OsStr::new("1")),
            ("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("120")),
            (HOLD_ENV, hold_value.as_os_str()),
            (FORWARD_RUN_ENV, std::ffi::OsStr::new(label)),
        ];
        let output = run_hermetic_test_in_child(name, path_dir.path(), &env);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
    }

    /// RV-04 (V-35, T-50-73) and R5-I1 (V-48, T-50-83): a stop that reaches
    /// stage N after safe point 3 (N is still `Advancing`, so it is flagged)
    /// is forwarded by N to the successor it recorded, and N+1 stops and
    /// records its own stop. Four runs, each a fresh child: (a) SIGTERM while
    /// N is held at `before_record_save`; (b) at `after_publish`; (c) a
    /// socket stop at `before_record_save`, whose caller is answered
    /// `stopped`, not `handed_off`; (d) SIGTERM while N is held at
    /// `before_handoff_mark`, after its advance returned and before it marks
    /// itself handed off.
    #[test]
    fn a_stop_reaching_the_supervisor_after_safe_point_3_is_forwarded_to_the_next_supervisor() {
        const NAME: &str = "pipeline_launch::tests::a_stop_reaching_the_supervisor_after_safe_point_3_is_forwarded_to_the_next_supervisor";
        if !devflow_core::test_support::in_child_test(NAME) {
            for (label, hold) in [
                ("a", "before_record_save"),
                ("b", "after_publish"),
                ("c", "before_record_save"),
                ("d", "before_handoff_mark"),
            ] {
                run_forward_child(NAME, hold, label);
            }
            return;
        }
        let _guard = env_lock();
        let label = std::env::var(FORWARD_RUN_ENV).expect("the parent names the run");
        let phase = match label.as_str() {
            "a" => 2309,
            "b" => 2310,
            "c" => 2311,
            _ => 2312,
        };
        let run = HandoffRun::launch(PhaseId::new(phase));
        run.go();
        run.wait_held();
        if label == "c" {
            let (accepted, last) =
                shutdown_accepted_then(&run.n.socket_path, || run.release(), HANDOFF_STEP_WAIT);
            assert_eq!(accepted["owner"], "advancing", "run {label}: {accepted}");
            assert_eq!(
                last["stopped"], true,
                "run {label}: N answers the forwarded stop `stopped`, so its caller does not \
                 retry: {last}"
            );
        } else {
            signal_supervisor(&run, libc::SIGTERM);
            // The control thread polls the signal pipe, so the stop is
            // flagged well within this.
            std::thread::sleep(Duration::from_millis(500));
            run.release();
        }
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)),
            "run {label}: N finishes: {}",
            run.events()
        );
        let next = run
            .state()
            .supervisor
            .expect("the state names a supervisor");
        assert_ne!(
            next.launch_id, run.n.launch_id,
            "run {label}: the state names N+1"
        );
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&next.socket_path)),
            "run {label}: N+1 was stopped through the forward: {}",
            run.events()
        );
        let state = run.state();
        assert!(
            state.stopped,
            "run {label}: N+1 recorded its stop (stop_reason {:?}): {}",
            state.stop_reason,
            run.events()
        );
        assert_eq!(
            state.stop_reason.as_deref(),
            Some(SUPERVISOR_STOP_REASON),
            "run {label}: N+1's own stop reason"
        );
        assert!(
            poll_until(Duration::from_secs(5), || !group_alive(next.agent_pgid)),
            "run {label}: no agent of N+1 survives"
        );
        let forwarded = events_named(&run.root, "stop_forwarded");
        assert_eq!(forwarded.len(), 1, "run {label}: {}", run.events());
        assert_eq!(forwarded[0]["confirmed"], true, "{}", forwarded[0]);
        assert_eq!(forwarded[0]["launch_id"], next.launch_id.as_str());
    }

    /// The control for the forward test: the same `before_record_save` hold
    /// and no stop leaves N+1 running and the run not stopped.
    #[test]
    fn without_a_stop_after_safe_point_3_the_next_supervisor_runs_on() {
        const NAME: &str =
            "pipeline_launch::tests::without_a_stop_after_safe_point_3_the_next_supervisor_runs_on";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_forward_child(NAME, "before_record_save", "control");
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(2313));
        run.go();
        run.wait_held();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)),
            "control: N finishes: {}",
            run.events()
        );
        let state = run.state();
        let next = state
            .supervisor
            .clone()
            .expect("control: the state names N+1");
        assert_ne!(next.launch_id, run.n.launch_id);
        assert!(
            !state.stopped,
            "control: not stopped ({:?})",
            state.stop_reason
        );
        assert!(probes_alive(&next.socket_path), "control: N+1 runs on");
        assert!(
            events_named(&run.root, "stop_forwarded").is_empty(),
            "control: nothing was forwarded"
        );
        let outcome =
            devflow_core::supervisor::request_shutdown(&next.socket_path, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                outcome,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "control: N+1 stops through its socket: {outcome:?}"
        );
    }

    const ORDER_RUN_NOT_FOUND: &str = "run_monitor not found in the non-test code";
    const ORDER_NO_TAKE: &str = "run_monitor never calls take_successor";
    const ORDER_NO_HOLD: &str = "no before_handoff_mark hold after take_successor";
    const ORDER_NO_MARK: &str = "no handed-off marking after the before_handoff_mark hold";
    const ORDER_NO_CHECK: &str = "no stop_requested() check after the handed-off marking";

    /// R5-I1 as a pure scan over `source`'s non-test, comment-stripped code:
    /// in `run_monitor`, in this order, the first `take_successor(` line, a
    /// later `test_hold("before_handoff_mark")`, a later
    /// `OwnerPhase::HandedOff` and a later `stop_requested()`. Names the
    /// missing function or the first step it cannot find.
    fn handoff_order_violation(source: &str) -> Option<String> {
        let lines = non_test_code_lines(source);
        let Some(body) = item_lines(&lines, "pub(crate) fn run_monitor(", "}") else {
            return Some(ORDER_RUN_NOT_FOUND.to_string());
        };
        let steps = [
            ("take_successor(", ORDER_NO_TAKE),
            ("test_hold(\"before_handoff_mark\")", ORDER_NO_HOLD),
            ("OwnerPhase::HandedOff", ORDER_NO_MARK),
            ("stop_requested()", ORDER_NO_CHECK),
        ];
        let mut from = 0;
        for (needle, missing) in steps {
            match body[from..].iter().position(|line| line.contains(needle)) {
                Some(at) => from += at + 1,
                None => return Some(missing.to_string()),
            }
        }
        None
    }

    /// R5-I1 (permanent source scan, R6-G4): `run_monitor` checks for a stop
    /// right after its handed-off marking, with the `before_handoff_mark`
    /// hold just before that marking. Every planted source carries a decoy
    /// marking before `take_successor(` and a decoy `stop_requested()` check
    /// in another function.
    #[test]
    fn run_monitor_checks_for_a_stop_right_after_the_handed_off_marking() {
        assert_eq!(
            handoff_order_violation(include_str!("pipeline_launch.rs")),
            None,
            "pipeline_launch.rs"
        );
        let take = "    let next = take_successor();\n";
        let hold = "    test_hold(\"before_handoff_mark\");\n";
        let mark = "    token.enter(OwnerPhase::HandedOff);\n";
        let check = "    if token.stop_requested() {}\n";
        let planted = |steps: &[&str]| {
            format!(
                "pub(crate) fn run_monitor(\n    decoy.enter(OwnerPhase::HandedOff);\n{}}}\nfn other() {{\n    if token.stop_requested() {{}}\n}}\n",
                steps.concat()
            )
        };
        let violation = |steps: &[&str]| handoff_order_violation(&planted(steps));
        let missing = |text: &str| Some(text.to_string());

        assert_eq!(
            violation(&[take, hold, mark, check]),
            None,
            "the planned layout"
        );
        assert_eq!(
            violation(&[take, check, hold, mark, check]),
            None,
            "the hint's check-plus-re-check layout"
        );
        assert_eq!(
            violation(&[take, check, mark]),
            missing(ORDER_NO_HOLD),
            "the pre-revision layout"
        );
        assert_eq!(
            violation(&[take, hold, check, mark]),
            missing(ORDER_NO_CHECK),
            "a hold whose only check precedes the marking (run (d)'s RED state)"
        );
        assert_eq!(
            violation(&[take, mark, hold, check]),
            missing(ORDER_NO_MARK),
            "a hold after the marking"
        );
        assert_eq!(
            violation(&[take, hold, mark, "    // if token.stop_requested() {}\n"]),
            missing(ORDER_NO_CHECK),
            "a commented-out post-marking check"
        );
        assert_eq!(
            violation(&[take, hold, check]),
            missing(ORDER_NO_MARK),
            "no marking"
        );
        assert_eq!(
            handoff_order_violation(&format!("fn other() {{\n{take}{hold}{mark}{check}}}\n")),
            missing(ORDER_RUN_NOT_FOUND),
            "no run_monitor"
        );
    }
}
