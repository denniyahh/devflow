//! Pipeline seam A (D-06): resolving a stage's launch and driving the advance
//! decision after its agent exits, inside the foreground runner (D-02).
//! Extracted mechanically (19-08, D-09 pure move) out of `main.rs`.
//!
//! There is one stage chain: [`run_runner`]'s loop runs each [`StageLaunch`] the
//! chain hands back, in its own process, and nothing here spawns a stage any other
//! way. [`launch_stage`] calls [`crate::preflight::run_preflight`] on the way in and
//! turns its [`crate::preflight::PreflightOutcome`] into the launch: an approved
//! preflight gate resolves through [`launch_stage_inner`] directly, skipping the
//! just-adjudicated check for that launch only (18-07, D-18f).
//!
//! **This module participates in the pipeline's three-way module cycle
//! (19-RESEARCH.md Pattern 1), and that is intentional:**
//! `pipeline_gate::transition` (and `loop_back_to_code`) call [`launch_stage`]
//! at their last line and return its launch, which is the edge that closes
//! `launch → outcomes → gate → launch` back to this module. Rust permits cyclic
//! module references (only the crate dependency graph must be acyclic), so this
//! compiles cleanly. All three pipeline modules import from each other directly —
//! see `pipeline_gate`'s module doc comment for the explicit caveat that this
//! cycle is NOT a wave-parallelism promise for future pipeline work.

use crate::CliError;
use crate::pipeline_gate::transition;
use crate::pipeline_outcomes::{
    ValidateOutcome, classify_validate_outcome, gate_or_abort_infra, handle_infra_outcome,
    handle_rate_limited_outcome, handle_stage_failure, handle_validate_outcome, truncate_reason,
};
use crate::preflight::{
    PreflightOutcome, agent_program, ensure_agent_binary, generic_preflight_checks,
    print_fallback_notice, run_preflight, validate_fallback_agent, worktree_writable_roots,
};
use devflow_core::config::capture_retention;
use devflow_core::gates::{AwaitOutcome, ClaimedAnswer, CloseReport, GateAnswer, GateId, Gates};
use devflow_core::mode::Mode;
use devflow_core::outcome_policy::{self, Action};
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::stop::{
    AgentOutcome, OwnerPhase, STOPPED_EXIT_CODE, SafePoint, StopSource, StopToken,
};
use devflow_core::{agent_result, agents, canary, events, gsd_config, mode, monitor, workflow};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// The prompt a launch with no override gives the agent, rendered by `driver`
/// (Phase 51 D-09). At Code it replays the persisted fix pass
/// ([`State::code_fix`]), so a same-stage relaunch inside a gaps-only or
/// full-execute pass continues that pass; every other stage gets
/// its default intent. The fix TYPE is persisted and rendered here, at launch,
/// so after a handoff the fallback's own renderer is used. The operator's notes
/// ride the same way (GATE-01): the fix pass's [`State::code_fix_note`], only
/// while a fix pass is in flight, and the stage's [`State::relaunch_note`],
/// attached to the intent the default builder returns. In a gap-closure pass
/// ([`State::gap_closure_pass`], GATE-03) Plan renders the gaps Plan intent
/// and Code the gaps-only pass, so a stop and resume keeps the pass's commands.
/// In a reviews pass ([`State::reviews_pass`], D-04) Plan renders the reviews
/// Plan intent, ahead of the gaps arm; the two never hold together, because the
/// reviews flag is spent when the run leaves Plan and only Validate sets the
/// gaps marker. `resume --reviews` sets the flag only at an `--until plan` halt,
/// which always comes before the run's first Validate: only `start` sets the cap,
/// and `transition` halts the first Plan exit under it.
///
/// The only place a default stage prompt is rendered:
/// [`launch_stage_inner`] (whose prompt the agent receives) and
/// [`launch_stage`] (which resolves the program from it) both call this, which
/// `every_no_override_launch_renders_through_default_stage_prompt` pins.
pub(crate) fn default_stage_prompt(state: &State, driver: &dyn agents::AgentDriver) -> String {
    let intent = match state.stage {
        // D-04 (#250): a reviews pass's Plan runs `--reviews`, at its first launch and
        // at every relaunch, a resume included. Checked first, though it cannot hold
        // with the gaps marker: it is spent on leaving Plan, and only Validate sets that.
        Stage::Plan if state.reviews_pass => prompt::StageIntent::PlanReviews {
            phase: state.phase,
            relaunch_note: state.relaunch_note.clone(),
        },
        // GATE-03 (review C6): a gap-closure pass's Plan runs `--gaps`, at its
        // first launch and at every relaunch, a resume included.
        Stage::Plan if state.gap_closure_pass => prompt::StageIntent::PlanGaps {
            phase: state.phase,
            relaunch_note: state.relaunch_note.clone(),
        },
        // The gaps-only pass is derived from the gap-closure marker when no
        // fix pass is recorded, so an `--until code` halt, which clears
        // `code_fix`, cannot turn a gaps Code pass into a full execute.
        Stage::Code => prompt::StageIntent::Code {
            phase: state.phase,
            fix: state
                .code_fix
                .or(state.gap_closure_pass.then_some(prompt::FixType::GapsOnly)),
            fix_note: state.code_fix.and(state.code_fix_note.clone()),
            relaunch_note: state.relaunch_note.clone(),
            // D-32 part 1: a resumed re-verify pass quotes GSD's recorded words.
            reverify_report: state
                .code_fix
                .filter(|fix| *fix == prompt::FixType::StaleReverify)
                .and(state.code_fix_report.clone()),
        },
        stage => prompt::StageIntent::for_stage(stage, state.phase)
            .with_relaunch_note(state.relaunch_note.clone()),
    };
    driver.render_prompt(&intent)
}

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
) -> Result<StageLaunch, CliError> {
    let driver = agents::driver_for(state.agent);
    let prompt = prompt_override.unwrap_or_else(|| default_stage_prompt(state, driver.as_ref()));
    prepare_stage_launch(state, prompt, archived_stage)
}

/// The part of [`launch_stage_inner`] after the prompt is rendered, up to the spawn: the
/// canary gate, the launch shape, the checkpoint-resume reset, and the [`StageLaunch`]
/// that either spawn path runs.
fn prepare_stage_launch(
    state: &mut State,
    prompt: String,
    archived_stage: Option<Stage>,
) -> Result<StageLaunch, CliError> {
    let driver = agents::driver_for(state.agent);
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
    let stream_launch = stream_launch_enabled(state.agent, state.stage);

    // D-15: refuse before any launch work if the undocumented CLI behaviour
    // this transport depends on is not backed by observed behaviour. Placed
    // ahead of the spawn so a refusal costs no archival rollover and spawns no
    // monitor.
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
    let args = wire_ask_registration(state, driver.as_ref(), args)?;

    stage_launch(
        state,
        program,
        &args,
        &driver.environment(),
        archived_stage,
        launch,
    )
}

/// The phase's `devflow_ask` registration file, `.devflow/ask-NN.mcp.json`: written
/// by each stage launch whose driver registers through a file (claude), removed by
/// `discard`.
pub(crate) fn ask_registration_path(project_root: &Path, phase: PhaseId) -> PathBuf {
    workflow::devflow_dir(project_root).join(format!("ask-{}.mcp.json", phase.padded()))
}

/// Put a driver's ask arguments into its launch argv where the driver says.
fn splice_ask_args(mut args: Vec<String>, wiring: &agents::AskWiring) -> Vec<String> {
    let at = match wiring.placement {
        agents::ArgPlacement::Append => args.len(),
        agents::ArgPlacement::BeforeLast => args.len().saturating_sub(1),
    };
    args.splice(at..at, wiring.args.iter().cloned());
    args
}

/// D-03: register the `devflow` MCP server for this stage launch: build the
/// registration, ask the driver for its wiring, write its file atomically when it
/// has one, and splice its arguments into `args`. A driver that cannot register is
/// refused here, before the launch. Nothing outside `.devflow/` is written.
fn wire_ask_registration(
    state: &State,
    driver: &dyn agents::AgentDriver,
    args: Vec<String>,
) -> Result<Vec<String>, CliError> {
    let exe = std::env::current_exe().map_err(|err| {
        CliError::Message(format!(
            "the devflow_ask server cannot be registered: this runner's own executable \
             cannot be resolved ({err})"
        ))
    })?;
    let reg = agents::AskRegistration {
        exe,
        project_root: state.project_root.clone(),
        phase: state.phase,
        stage: state.stage,
        timeout_secs: crate::config_parse::gate_timeout_secs(),
        run_started_at: state.started_at.clone(),
        config_path: ask_registration_path(&state.project_root, state.phase),
    };
    let wiring = driver
        .ask_wiring(&reg)
        .map_err(|why| CliError::Message(format!("refusing to launch `{}`: {why}", state.agent)))?;
    if let Some((path, text)) = &wiring.file {
        write_ask_registration(path, text)?;
    }
    Ok(splice_ask_args(args, &wiring))
}

/// Write the registration file through a temp beside it and a rename, so the agent
/// never reads a half-written file. The temp `.ask-NN.mcp.json.tmp` is covered by
/// the ask lock's `.gitignore` pattern.
fn write_ask_registration(path: &Path, text: &str) -> Result<(), CliError> {
    let failed = |err: std::io::Error| {
        CliError::Message(format!(
            "could not write the devflow_ask registration {}: {err}",
            path.display()
        ))
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.tmp"));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(failed)?;
    }
    std::fs::write(&temp, format!("{text}\n")).map_err(failed)?;
    std::fs::rename(&temp, path).map_err(failed)
}

/// How a stage's agent is launched, before the runner builds its
/// [`monitor::StageIo`]: the stream shape delivers `prompt` as the first turn
/// on the agent's stdin and keeps the pipes; the capture shape gives the agent
/// `/dev/null` for stdin and the capture file for stdout.
#[derive(Debug)]
pub(crate) enum StageShape {
    /// [`monitor::StageIo::Stream`], with this prompt as the first turn.
    Stream {
        /// The stage prompt, delivered on the agent's stdin rather than argv.
        prompt: String,
    },
    /// [`monitor::StageIo::Capture`].
    Capture,
}

/// Resolve a stage launch into `(program, argv, launch shape)`.
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
) -> (&'static str, Vec<String>, StageShape) {
    if stream_launch {
        let (program, args) = driver.build_command(phase, &prompt, roots);
        (program, args, StageShape::Stream { prompt })
    } else if agent == AgentKind::Claude {
        // Claude on a stage the rollout has not reached (none today; Phase 52
        // removed D-11's opt-out): the explicitly named pre-31 builder, NOT
        // `exec_command` — which now returns the stream-json shape for every
        // stage.
        let (program, args) = agents::ClaudeDriver::exec_command_single_document(&prompt);
        (program, args, StageShape::Capture)
    } else {
        let (program, args) = driver.build_command(phase, &prompt, roots);
        (program, args, StageShape::Capture)
    }
}

/// Entry-point name recorded in the `auto_chain_flag_repaired` payload by
/// [`commands::start`](crate::commands::start).
pub(crate) const AUTO_CHAIN_REPAIR_FROM_START: &str = "start";
/// Entry-point name recorded by [`resume_with_fallback`].
pub(crate) const AUTO_CHAIN_REPAIR_FROM_RESUME: &str = "resume";

/// Repair a leaked `workflow._auto_chain_active` before this launch spawns
/// anything, and say so on both channels when it found one (35.1 D-01/D-03).
///
/// **One helper, two call sites**: `start` and `resume` must not be able to
/// drift into repairing on different terms, and a second copy of the
/// emit/print block is exactly how that drift happens.
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

    let note = auto_chain_repair_note(entry_point, phase, &outcome);
    if !outcome.repaired_anything() && outcome.commit_refused.is_none() {
        // 52.1 D-22: recording GSD's own `false` is not a repair, and neither
        // is leaving a difference DevFlow cannot attribute uncommitted. Each is
        // said in the foreground and writes no `auto_chain_flag_repaired`
        // event. A clean launch has no note at all.
        if let Some(note) = note {
            println!("{note}");
        }
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

    if let Some(note) = note {
        println!("{note}");
    }
    if let Some(reason) = &outcome.commit_refused {
        println!("warning: {reason}");
    }
}

/// The foreground `note:` line for what [`repair_leaked_auto_chain_flag`] found,
/// or `None` when there is nothing to say. Names a killed run only when a set
/// flag was actually cleared; a recorded `false` (52.1 D-22) says nothing leaked;
/// a difference left uncommitted says so, and what that costs at Validate.
fn auto_chain_repair_note(
    entry_point: &str,
    phase: PhaseId,
    outcome: &gsd_config::ClearOutcome,
) -> Option<String> {
    if outcome.repaired_anything() {
        return Some(format!(
            "note: devflow {entry_point} found phase {phase}'s GSD chain flag \
             (workflow._auto_chain_active) still set — a previous run for this phase \
             was killed before it could clear it. Cleared before launching \
             (working tree: {}, this branch's tip: {})",
            repaired_word(outcome.working_tree_repaired),
            repaired_word(outcome.committed_tree_repaired),
        ));
    }
    if outcome.recorded_gsd_false {
        return Some(format!(
            "note: devflow {entry_point} recorded GSD's workflow._auto_chain_active: false \
             for phase {phase} in a commit (the committed config held no true flag; \
             nothing was leaked)"
        ));
    }
    if outcome.left_uncommitted {
        return Some(format!(
            "note: devflow {entry_point} left .planning/config.json uncommitted for phase \
             {phase}: it differs from this branch's tip and DevFlow cannot attribute the \
             change (no copy holds a true chain flag, and the working copy holds no false \
             to record). Validate will not record a pass until it is committed"
        ));
    }
    None
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
    // resolved to the capture shape does not rely on that premise, so checking
    // it there would spend an agent invocation to answer a
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
            let recorded = outcome.clone();
            commit(state, |s| s.canary = Some(recorded))?;
            emit_canary_outcome(state, &outcome);
            outcome
        }
    };

    match outcome {
        canary::CanaryOutcome::Confirmed => Ok(()),
        canary::CanaryOutcome::Absent => refuse_launch(
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
        canary::CanaryOutcome::Unverified(reason) => refuse_launch(format!(
            "the delivery canary COULD NOT RUN, so background-task notification \
                 delivery is unverified for this run. This is not a report that the \
                 behaviour is gone — the guard reached no conclusion either way.\n\
                 \n\
                 Reason: {reason}\n\
                 \n\
                 Refusing to launch: the multi-plan wave guarantee depends on that \
                 behaviour and this run has no evidence about it."
        )),
    }
}

/// The refusal [`canary_gate`] would give a launch of `state`'s stage under its agent
/// from the canary outcome the run already recorded, described for an operator; `None`
/// when the launch would not be refused for that reason. The same predicate as
/// [`launch_stage_inner`]'s (the stream path, a recorded `Absent` or `Unverified`), read
/// without running a canary or writing anything. It is a second copy of `canary_gate`'s
/// mapping, so `recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome`
/// pins the two together: change one and that test fails until the other matches.
pub(crate) fn recorded_canary_refusal(state: &State) -> Option<String> {
    if !stream_launch_enabled(state.agent, state.stage) {
        return None;
    }
    match state.canary.as_ref()? {
        canary::CanaryOutcome::Confirmed => None,
        canary::CanaryOutcome::Absent => {
            Some("background-task notification delivery is ABSENT".to_string())
        }
        canary::CanaryOutcome::Unverified(reason) => Some(format!(
            "the delivery canary could not run: {}",
            truncate_reason(reason)
        )),
    }
}

/// Abort a launch the canary refused, leaving no half-launched stage behind: the
/// stage is not launched, and the refusal names why. The state records no launch, so
/// there is nothing to clear.
fn refuse_launch(message: String) -> Result<(), CliError> {
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
/// taken at Plan and a capture taken at Validate differ only in how the agent
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
/// Every one of the three `Stage` variants is accounted for by name below —
/// each entry names the capture that authorised it, or, for a stage left off
/// the list, what was attempted and what specifically prevented the evidence.
/// All three are currently ON the list, so no "recorded reason for staying
/// narrow" entry is live; the format is retained so that removing a stage
/// requires writing one rather than deleting a line. The same run also
/// captured the Define and Ship stages (`34-evidence/define/`,
/// `34-evidence/ship/`); v4.0 removed both stages from [`Stage`] (D-01).
///
/// All five of that run's captures come from a SINGLE run — `devflow start --phase 1
/// --no-worktree --agent claude --mode auto` against a throwaway repo
/// scaffolded by `scripts/scratch-dogfood-repo.sh`, on `claude` 2.1.222, driven
/// by a binary whose digest was verified byte-identical to the build made from
/// this tree after the widening. Evidence directories are under
/// `.planning/phases/34-…/34-evidence/{stage}/`.
///
/// - **`Stage::Plan`** — WIDENED on `34-evidence/plan/`. 11 events, 2 turns,
///   11.8 s, `NeverAnnounced`, which [`monitor::CloseRule::should_close`]
///   treats as vacuously drained by design. **Thin by construction:** the
///   scratch scaffold pre-writes the plan, and the agent reported "The
///   deliverable already exists … No work performed". It is evidence that Plan
///   takes the stream path and does not announce background tasks *on a
///   workload with no work in it* — not that Plan never backgrounds work.
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
///
/// **What none of these establish (D-10, n=1).** Each capture shows the shape
/// occurred ONCE. None of them shows it is the stage's steady behaviour across
/// prompts, phase shapes or CLI versions — Phase 30 needed n=2–3 trials before
/// its drain measurements meant anything. A `NeverAnnounced` reading from a
/// near no-op is the weakest form this evidence takes, and Plan's is that
/// form.
///
/// **Criterion 7 — the D-15 canary refusal runs at the FIRST stage, Plan.**
/// With every stage on the stream path, [`canary_gate`] runs at the run's
/// first launch instead of at Code. A run whose canary returns
/// `Absent`/`Unverified` therefore refuses before any stage completes on the
/// legacy path. This is a real change to unattended behaviour, accepted rather
/// than mitigated: D-15 rejected both alternatives (warn-and-proceed fails
/// unattended; falling back to the legacy path is a silent capability
/// downgrade). On the phase-34 capture run the canary returned `Confirmed` at
/// its first stage (then Define), so the refusal did not fire — it is recorded
/// here on the strength of the code path, not on the strength of having
/// watched it refuse.
const STREAM_JSON_STAGES: &[Stage] = &[Stage::Plan, Stage::Code, Stage::Validate];

/// Whether this launch should use the `stream-json` transport and the
/// pipe-owning monitor.
///
/// **Agent coverage (round-3 D-10):** Claude and Antigravity, purely on
/// `STREAM_JSON_STAGES` membership. Every other agent (codex, opencode, pi,
/// hermes) takes the capture shape, `StageShape::Capture`. There is no
/// opt-out: Phase 52 removed Claude's pre-31 single-document launch and its
/// flag and environment override, so one predicate governs the launch shape
/// and the D-15 canary gate that protects it.
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
pub(crate) fn stream_launch_enabled(agent: AgentKind, stage: Stage) -> bool {
    matches!(agent, AgentKind::Claude | AgentKind::Antigravity)
        && STREAM_JSON_STAGES.contains(&stage)
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
/// **Restores only its own write (D-46, review 5 V3).** When [`Self::engage`]
/// changed the file, the guard keeps the bytes it read before and the bytes it
/// wrote. On drop, if the file still holds exactly its own write, the earlier
/// bytes go back (temp write, then rename in the same directory), so a file it
/// found without the key, or with the key `false` in another layout, is
/// byte-identical after the stage and never reads as a dirty tree. Those
/// earlier bytes never hold `true`: bytes whose flag reads `true` are not kept
/// (a leaked `true` an ineligible launch clears stays cleared), so 35.1 D-06's
/// `false` on exit stands. Otherwise (the guard wrote nothing,
/// or the file changed during the stage) the drop clears the flag as before, so
/// a leaked `true` is still cleared and an edit made during the stage is kept.
///
/// Errors are logged, never propagated. A hand-edited or absent
/// `.planning/config.json` must not abort a long unattended run — the guard
/// declines to engage and the stage proceeds without checkpoint auto-approval,
/// which is the conservative direction (T-35.1-03).
struct AutoChainGuard {
    config_root: PathBuf,
    /// The file's bytes before `engage` and the bytes `engage` wrote, when
    /// `engage` changed it.
    restore: Option<(Vec<u8>, Vec<u8>)>,
}

impl AutoChainGuard {
    fn engage(config_root: &Path, active: bool) -> Self {
        let path = gsd_config::config_path(config_root);
        // Bytes that hold a `true` flag are never kept for a restore: a leaked
        // `true` that an ineligible launch clears stays cleared (F-3).
        let before = std::fs::read(&path)
            .ok()
            .filter(|_| matches!(gsd_config::auto_chain_active(config_root), Ok(false)));
        let mut restore = None;
        match gsd_config::set_auto_chain_active(config_root, active) {
            Ok(changed) => {
                if changed {
                    info!(
                        "GSD chain flag set to {active} for this stage at {}",
                        path.display()
                    );
                    if let (Some(before), Ok(written)) = (before, std::fs::read(&path)) {
                        restore = Some((before, written));
                    }
                }
            }
            Err(err) => warn!(
                "could not set the GSD chain flag at {}: {err} — proceeding without \
                 checkpoint auto-approval",
                path.display()
            ),
        }
        Self {
            config_root: config_root.to_path_buf(),
            restore,
        }
    }

    /// Put `before` back when the file still holds exactly `written`. `true`
    /// when it did.
    fn restore_own_write(&self, before: &[u8], written: &[u8]) -> bool {
        let path = gsd_config::config_path(&self.config_root);
        if std::fs::read(&path).ok().as_deref() != Some(written) {
            return false;
        }
        let temp = path.with_extension("json.devflow-restore");
        let restored = std::fs::write(&temp, before).and_then(|()| std::fs::rename(&temp, &path));
        match restored {
            Ok(()) => true,
            Err(err) => {
                let _ = std::fs::remove_file(&temp);
                warn!(
                    "could not restore the GSD config at {}: {err}",
                    path.display()
                );
                false
            }
        }
    }
}

impl Drop for AutoChainGuard {
    fn drop(&mut self) {
        if let Some((before, written)) = self.restore.take()
            && self.restore_own_write(&before, &written)
        {
            return;
        }
        if let Err(err) = gsd_config::set_auto_chain_active(&self.config_root, false) {
            warn!(
                "could not clear the GSD chain flag at {}: {err}",
                gsd_config::config_path(&self.config_root).display()
            );
        }
    }
}

/// The stop reason an advance records when it stops at a safe point
/// (50-13, R-2).
pub(crate) const ADVANCE_STOP_REASON: &str =
    "stopped during advance: the runner was ended by its unit stop";

/// What an advance reads on this thread (50-13): its stop token, for the advance's
/// safe points. One runner per phase holds the runner lock for the whole run (D-07),
/// so its state writes need no fence.
struct EngagedAdvance {
    token: StopToken,
}

thread_local! {
    /// Set only while [`StopTokenGuard`] is alive. Thread-local, not a
    /// process-global static, so parallel in-crate tests never share it; an
    /// advance run outside the runner (a test calling `advance_with` directly) sees
    /// `None`, and its safe points never stop.
    static ENGAGED_ADVANCE: std::cell::RefCell<Option<EngagedAdvance>> =
        const { std::cell::RefCell::new(None) };
}

/// Makes the runner's stop token visible to the safe points of the advance running
/// on this thread, for as long as the guard lives (RAII, the [`AutoChainGuard`]
/// shape). Dropping it clears the slot.
pub(crate) struct StopTokenGuard {
    /// Keeps the guard on the thread whose slot it set.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl StopTokenGuard {
    /// Engage the foreground runner's run-long token (D-05): the safe points, the gate
    /// wait and the test holds read `token`.
    pub(crate) fn engage_token(token: StopToken) -> Self {
        ENGAGED_ADVANCE.with(|slot| {
            *slot.borrow_mut() = Some(EngagedAdvance { token });
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

/// `Gates::close` for a stage of the run. It takes the gate lock, so it must not be
/// called inside a state-write closure.
pub(crate) fn close_gate(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
) -> Result<CloseReport, CliError> {
    Ok(Gates::close(project_root, phase, stage)?)
}

/// Commit one write site's change to the phase's state (D-08), and replace the
/// caller's copy with the state as committed.
///
/// `apply` sets only the fields this site changes, on the state as it is on disk
/// now, so what another writer changed since the caller loaded its copy is kept.
/// Assigning a whole `State` copy in `apply` would re-create the lost update this
/// exists to close.
///
/// A field changed in the caller's copy but not set in `apply` is reverted by the
/// replace. That is on purpose: a field a site forgot to commit shows up in its
/// tests rather than reaching disk by luck through a later write.
///
/// `apply` runs under the state write lock, so it never spawns a process, runs
/// hooks, waits on a gate or takes another lock.
pub(crate) fn commit(state: &mut State, apply: impl FnOnce(&mut State)) -> Result<(), CliError> {
    let (committed, ()) = workflow::update_state(&state.project_root, state.phase, |s| {
        apply(s);
        Ok::<_, CliError>(())
    })?;
    *state = committed;
    Ok(())
}

/// Commit a rejected gate's `note` as the run's relaunch note
/// ([`State::relaunch_note`], GATE-01 / D-39), for the relaunch of the stage the
/// gate stopped at. A rejection with a note replaces an earlier one; a rejection
/// with none keeps it, so a second failure answered without a note does not
/// drop what the operator already asked for at this stage. Writes nothing when
/// there is no note. The note leaves with the stage (`transition`, the Validate
/// loop-back to Code, `finish_run`).
pub(crate) fn record_relaunch_note(
    state: &mut State,
    note: Option<String>,
) -> Result<(), CliError> {
    let Some(note) = note else {
        return Ok(());
    };
    commit(state, |s| s.relaunch_note = Some(note))
}

/// CR-03 (49-REVIEW.md): commit the notify command this process's environment
/// resolves to, where `resume` and the runner's advance read it, so it reaches the
/// chain they launch whatever they do next. With no change, nothing is written: a
/// runner whose unit carries no variable keeps the persisted command.
fn commit_notify_env(state: &mut State) -> Result<(), CliError> {
    let resolved = devflow_core::gates::apply_notify_env(state.gate_notify_cmd.clone());
    if resolved == state.gate_notify_cmd {
        return Ok(());
    }
    commit(state, |s| s.gate_notify_cmd = resolved)
}

/// An advance safe point (50-13, R-2): when a stop has been requested
/// ([`stop_requested_at_safe_point`]), record the stop in `state` and return `true`, so
/// the caller returns at once without launching anything (a supervisor's `finish` answers
/// a parked caller `stopped`). `false` with no stop requested.
fn stopped_at_safe_point(state: &mut State, safe_point: u8) -> Result<bool, CliError> {
    if !stop_requested_at_safe_point() {
        return Ok(false);
    }
    record_advance_stop(state, safe_point)?;
    Ok(true)
}

/// The test half of an advance safe point, for a caller that must clear
/// something before it calls [`record_advance_stop`]: `true` when the
/// engaged token's stop has been requested (the run is then marked ended as a stop).
/// A thread with no engaged token never stops here: the runner, the one process that
/// catches stop signals, engages its token before its first advance.
pub(crate) fn stop_requested_at_safe_point() -> bool {
    with_stop_token(StopToken::safe_point) == Some(SafePoint::StopHere)
}

/// Persist a stop an advance honoured at `safe_point`: `stopped` with
/// [`ADVANCE_STOP_REASON`] and `gate_pending` cleared in one write, and an
/// `advance_stopped` event naming the point.
pub(crate) fn record_advance_stop(state: &mut State, safe_point: u8) -> Result<(), CliError> {
    let reason = ADVANCE_STOP_REASON;
    let source = "runner";
    commit(state, |s| {
        s.stopped = true;
        s.stop_reason = Some(reason.to_string());
        s.gate_pending = false;
    })?;
    events::emit(
        &state.project_root,
        state.phase,
        "advance_stopped",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "safe_point": safe_point,
            "source": source,
        }),
    );
    info!("phase {}: {reason} (safe point {safe_point})", state.phase);
    Ok(())
}

/// How a gate wait ended (safe point 4).
#[derive(Debug)]
pub(crate) enum GateWaitEnd {
    /// The gate was answered.
    Answered(devflow_core::gates::GateResponse),
    /// No answer arrived within the timeout.
    TimedOut,
    /// The supervisor running this advance was stopped during the wait.
    Stopped,
}

/// A gate wait (50-13, R-2, safe point 4), for one gate id's answer and no other
/// (D-04, D-05). It holds no write lock. Inside a supervisor's advance the run is
/// `GateWait` for the wait and `Advancing` again after it, and the wait is cancelled
/// by the supervisor's stop token, so a SIGTERM/SIGINT or socket stop ends it within
/// about a second as [`GateWaitEnd::Stopped`]. With no engaged token no stop cancels
/// it: only the runner catches stop signals, and it engages its token first.
///
/// # Errors
/// A file in this gate's slot that does not bind to it could not be set aside, because
/// the gate closed or was replaced, or its lock or the file could not be taken. The
/// wait ends and the file stays where it is (R-09).
pub(crate) fn wait_for_gate_response(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    gate_id: &GateId,
    timeout_secs: u64,
) -> Result<GateWaitEnd, CliError> {
    let token = with_stop_token(StopToken::clone);
    if let Some(token) = &token {
        token.enter(OwnerPhase::GateWait);
    }
    let outcome = await_bound_answer(project_root, phase, stage, gate_id, timeout_secs, || {
        token.as_ref().is_some_and(StopToken::stop_requested)
    });
    if let Some(token) = &token {
        token.enter(OwnerPhase::Advancing);
    }
    Ok(match outcome? {
        Some(answer) => GateWaitEnd::Answered(answer.response),
        None if stop_requested_at_safe_point() => GateWaitEnd::Stopped,
        None => GateWaitEnd::TimedOut,
    })
}

/// `Gates::await_answer`, setting aside each file in the slot that does not bind to
/// `gate_id` and waiting again with the time left (R-09: the read itself moves nothing).
/// Returns the gate's answer, or `None` on a timeout or a cancel.
fn await_bound_answer(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    gate_id: &GateId,
    timeout_secs: u64,
    cancel: impl Fn() -> bool,
) -> Result<Option<GateAnswer>, CliError> {
    let started = std::time::Instant::now();
    loop {
        let left = timeout_secs.saturating_sub(started.elapsed().as_secs());
        match Gates::await_answer(project_root, phase, stage, gate_id, left, &cancel) {
            AwaitOutcome::Unbound { reason } => {
                Gates::set_aside_unbound(project_root, phase, stage, gate_id).map_err(|err| {
                    CliError::Message(format!(
                        "phase {phase}: the wait for {stage} gate {gate_id} ended: the file in \
                         its answer slot does not bind to it ({reason}) and was not set aside: \
                         {err}"
                    ))
                })?;
            }
            AwaitOutcome::Answered(answer) => return Ok(Some(answer)),
            AwaitOutcome::TimedOut | AwaitOutcome::Cancelled => return Ok(None),
        }
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

/// Test-only hold point (50.1-16) for a launcher, which [`test_hold`] never holds
/// outside an engaged advance: when `DEVFLOW_TEST_LAUNCH_HOLD` is `<point>:<path>`
/// for this `point`, append a line to `<path>.held` and wait (bounded by
/// [`TEST_HOLD_WAIT`]) until `<path>` exists. One line per arrival, so a test can
/// count the launchers that reached it. Points: `guarded`, after the live-run guard
/// inside the reservation's write-lock closure; `reserved`, after that write;
/// `holder_claimed`, once a foreground process holds the phase (for `start` and `resume`,
/// with its reservation pending and no supervisor yet); and `before_publish_save`, once
/// the supervisor has answered its ping.
/// Compiled out of non-test builds, together with every call.
#[cfg(test)]
pub(crate) fn launch_test_hold(point: &str) {
    use std::io::Write as _;
    let Ok(hold) = std::env::var("DEVFLOW_TEST_LAUNCH_HOLD") else {
        return;
    };
    let Some((at, path)) = hold.split_once(':') else {
        return;
    };
    if at != point {
        return;
    }
    if let Ok(mut held) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{path}.held"))
    {
        let _ = writeln!(held, "{point}");
    }
    let release = Path::new(path);
    let deadline = std::time::Instant::now() + TEST_HOLD_WAIT;
    while !release.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The label a launch shape carries in `stage_launched` and on stdout.
fn launch_label(launch: &StageShape) -> &'static str {
    match launch {
        StageShape::Stream { .. } => "stream",
        StageShape::Capture => "capture",
    }
}

/// The foreground runner (D-02): `devflow __runner`, what a phase's user unit runs.
///
/// It takes the phase's runner lock before anything else and holds it until it
/// returns (D-07), and only then reports ready to systemd (D-04a), so a runner that
/// cannot take the lock fails the unit's start. It never takes the phase holder. It then
/// runs the saved stage in this process ([`monitor::run_stage`], under one
/// [`StopToken`] for the whole run), advances, and runs each stage the advance hands
/// back, until there is none. Any error ends the runner non-zero with the error on
/// stderr. No agent is spawned and no gate is waited on inside a state-write closure.
///
/// A SIGTERM or SIGINT (D-05) reaches the run's token through one signal dispatcher,
/// started before READY and kept until this returns: it ends a running agent's group,
/// and flags a stop the advance's safe points and gate wait read. The run then records
/// `stopped` and returns [`STOPPED_EXIT_CODE`] ([`settle_advance`]).
pub(crate) fn run_runner(project_root: &Path, phase: PhaseId) -> Result<i32, CliError> {
    devflow_core::stop::install_signal_pipe().map_err(|err| {
        CliError::Message(format!(
            "the runner could not install its stop handling: {err}"
        ))
    })?;
    let _runner_lock = devflow_core::liveness::claim_runner_lock(project_root, phase)
        .map_err(|err| CliError::Message(format!("phase {phase}: {err}")))?;
    let token = StopToken::for_run();
    // The signal pipe's only reader, from before READY until this returns.
    let _signals = devflow_core::stop::spawn_signal_dispatcher(token.clone()).map_err(|err| {
        CliError::Message(format!(
            "the runner could not start its signal dispatcher: {err}"
        ))
    })?;
    crate::service::notify_ready().map_err(|err| {
        CliError::Message(format!(
            "the runner could not report ready to systemd: {err}"
        ))
    })?;
    let _token_slot = StopTokenGuard::engage_token(token.clone());
    let mut state = workflow::load_state(project_root, phase)?;
    let first = runner_first_step(&mut state);
    let mut settled = settle_advance(project_root, phase, &token, first)?;
    loop {
        let launch = match settled {
            Settled::Next(launch) => *launch,
            Settled::Exit(code) => return Ok(code),
        };
        let result = run_one_stage(project_root, phase, launch, &token)?;
        settled = settle_advance(project_root, phase, &token, result)?;
        // After the hand-back, before the next stage begins.
        #[cfg(test)]
        if matches!(settled, Settled::Next(_)) {
            test_hold("before_runner_stage");
        }
    }
}

/// The runner's first step, from the saved state (D-02): the launch of `state.stage`,
/// handed back for [`run_runner`]'s loop to run (a launch a preflight gate's approval
/// starts comes back the same way). `None` when the launch resolved without one.
///
/// A state that still reads `gate_pending` refuses: `start` writes a fresh state and
/// `resume` clears the flag and closes the phase's gates under its holder before it
/// launches, so a pending gate here was left by no launcher, and the runner, which
/// cannot rebuild the dead waiter's question, names `devflow resume`.
fn runner_first_step(state: &mut State) -> Result<Option<StageLaunch>, CliError> {
    if state.gate_pending {
        return Err(CliError::Message(format!(
            "phase {phase}: the saved state still waits on a {stage} gate no runner holds; \
             run `devflow resume --phase {phase}` to rerun the stage",
            phase = state.phase,
            stage = state.stage
        )));
    }
    launch_stage(state, None, None)
}

/// What the runner does after an advance result: run the next stage, or exit with a code.
enum Settled {
    Next(Box<StageLaunch>),
    Exit(i32),
}

/// Read one advance result in the runner's fixed order (review r3 codex-2 + agy-2): a
/// state on disk that reads finished ends the run with 0 and writes nothing, since a
/// permitted finish wins over a late stop; then a stop requested on the run's token is
/// recorded ([`record_runner_stop`]) and ends it with [`STOPPED_EXIT_CODE`], after an
/// `Ok` or an `Err` alike, so a gate wait a stop ended never reaches the failed-advance
/// route; then an `Err` is a failed advance ([`recover_failed_advance`]), whose result is
/// read in the same order; otherwise `Some` runs the next stage and `None` ends the run
/// with [`runner_exit_code`].
fn settle_advance(
    project_root: &Path,
    phase: PhaseId,
    token: &StopToken,
    result: Result<Option<StageLaunch>, CliError>,
) -> Result<Settled, CliError> {
    if let Some(settled) = settled_by_finish_or_stop(project_root, phase, token) {
        return Ok(settled);
    }
    let next = match result {
        Ok(next) => next,
        Err(err) => {
            let recovered = recover_failed_advance(project_root, phase, &err);
            if let Some(settled) = settled_by_finish_or_stop(project_root, phase, token) {
                return Ok(settled);
            }
            recovered?
        }
    };
    Ok(match next {
        Some(launch) => Settled::Next(Box::new(launch)),
        None => Settled::Exit(runner_exit_code(
            workflow::load_state(project_root, phase).ok().as_ref(),
        )),
    })
}

/// The first two steps of [`settle_advance`]'s order: a finished state, then a stop.
fn settled_by_finish_or_stop(
    project_root: &Path,
    phase: PhaseId,
    token: &StopToken,
) -> Option<Settled> {
    if workflow::load_state(project_root, phase).is_ok_and(|state| state.finished) {
        return Some(Settled::Exit(0));
    }
    if token.stop_requested() {
        record_runner_stop(project_root, phase, token.stop_source());
        return Some(Settled::Exit(STOPPED_EXIT_CODE));
    }
    None
}

/// The stop reason the runner records for a stop from `source` (D-05).
fn runner_stop_reason(source: Option<StopSource>) -> String {
    match source {
        Some(StopSource::Signal(signal)) if signal == libc::SIGTERM => {
            "stopped by SIGTERM to the runner".to_string()
        }
        Some(StopSource::Signal(signal)) if signal == libc::SIGINT => {
            "stopped by SIGINT to the runner".to_string()
        }
        Some(source) => format!("stopped by the runner ({source:?})"),
        None => "stopped by the runner".to_string(),
    }
}

/// Record a stop the runner's token took (D-05): `stopped` with
/// [`runner_stop_reason`], and a `runner_stopped` event. A state that already reads
/// stopped (a safe point's [`record_advance_stop`] wrote it) keeps its reason, and an
/// absent state is not written. Logged, never an error: the runner exits stopped either
/// way.
fn record_runner_stop(project_root: &Path, phase: PhaseId, source: Option<StopSource>) {
    let reason = runner_stop_reason(source);
    let stage = match workflow::load_state(project_root, phase) {
        Ok(state) if state.stopped => {
            info!(
                "phase {phase}: the stop is already recorded ({:?})",
                state.stop_reason
            );
            return;
        }
        Ok(state) => state.stage,
        Err(err) => {
            info!("phase {phase}: no state to record the stop in ({err})");
            return;
        }
    };
    let written = workflow::update_state(project_root, phase, |state| {
        state.stopped = true;
        state.stop_reason = Some(reason.clone());
        Ok::<(), CliError>(())
    });
    if let Err(err) = written {
        warn!("phase {phase}: `stopped` was not recorded: {err}");
        return;
    }
    events::emit(
        project_root,
        phase,
        "runner_stopped",
        serde_json::json!({ "stage": stage.to_string(), "reason": reason }),
    );
    eprintln!("phase {phase}: {reason}");
}

/// Run one handed-back stage in this process and advance past it: the advance's result,
/// or, when the stage could not be started, the result of routing that failure through
/// [`handle_infra_outcome`] after [`failed_start`]'s event (never silent, W-2).
fn run_one_stage(
    project_root: &Path,
    phase: PhaseId,
    launch: StageLaunch,
    token: &StopToken,
) -> Result<Result<Option<StageLaunch>, CliError>, CliError> {
    // A stop that arrived during the hand-back starts no stage.
    if token.stop_requested() {
        return Ok(Ok(None));
    }
    if let Err(err) = begin_runner_stage(&launch) {
        return Ok(Err(err));
    }
    let stage = launch.state.stage;
    let agent = launch.state.agent;
    let relaunch = launch.relaunch.clone();
    let auto_chain_guard = match launch.launch {
        StageShape::Stream { .. } => Some(AutoChainGuard::engage(
            launch
                .state
                .worktree_path
                .as_deref()
                .unwrap_or(&launch.state.project_root),
            auto_chain_flag_eligible(stage, launch.state.mode),
        )),
        StageShape::Capture => None,
    };
    let ran = monitor::run_stage(runner_stage_spec(launch), token).map_err(|err| match err {
        // `MonitorError::Io`'s text names a monitor spawn; here the agent is spawned.
        monitor::MonitorError::Io(io) => {
            CliError::Message(format!("could not spawn the agent: {io}"))
        }
        other => CliError::Message(other.to_string()),
    });
    // Before the advance: the flag lives as long as the agent does.
    drop(auto_chain_guard);
    let outcome = match ran {
        Ok(outcome) => outcome,
        Err(err) => {
            let (event, payload, reason) =
                failed_start(stage, agent, relaunch.as_ref(), &err.to_string());
            events::emit(project_root, phase, event, payload);
            let mut state = workflow::load_state(project_root, phase)?;
            return Ok(handle_infra_outcome(
                project_root,
                &mut state,
                stage,
                Some(reason),
            ));
        }
    };
    if outcome == AgentOutcome::Stopped {
        eprintln!("phase {phase}: stage {stage} was stopped; nothing advances");
        return Ok(Ok(None));
    }
    Ok(advance_for_stage(project_root, phase, Some(stage)))
}

/// Route an `Err` out of a launch preparation or an advance (a missing agent binary at
/// the next stage, say) through [`failed_advance`]'s event and [`handle_infra_outcome`]
/// for the stage the state on disk now records (review r2 codex-6). Its result feeds the
/// loop; an `Err` from it, or a state that no longer reads, ends the runner non-zero.
fn recover_failed_advance(
    project_root: &Path,
    phase: PhaseId,
    err: &CliError,
) -> Result<Option<StageLaunch>, CliError> {
    let mut state = workflow::load_state(project_root, phase).map_err(|load_err| {
        CliError::Message(format!(
            "phase {phase}: {err}; the runner could not read the state to gate it: {load_err}"
        ))
    })?;
    let stage = state.stage;
    let (event, payload, reason) = failed_advance(phase, stage, state.agent, &err.to_string());
    events::emit(project_root, phase, event, payload);
    handle_infra_outcome(project_root, &mut state, stage, Some(reason))
}

/// The event, its payload and the infra reason for a stage the runner could not start
/// (W-2). With `relaunch` it is the handoff's `agent_handoff_relaunch_failed`, with the
/// keys and reason text [`fail_over_on_rate_limit`] has always written, so the two paths
/// cannot drift; without one it is `stage_launch_failed`.
fn failed_start(
    stage: Stage,
    agent: AgentKind,
    relaunch: Option<&HandoffRelaunch>,
    error: &str,
) -> (&'static str, serde_json::Value, String) {
    match relaunch {
        Some(HandoffRelaunch {
            from,
            to,
            rate_limit,
        }) => {
            let rate_limit = rate_limit.as_deref().unwrap_or("no reason given");
            (
                "agent_handoff_relaunch_failed",
                serde_json::json!({
                    "stage": stage.to_string(),
                    "from_agent": from.to_string(),
                    "to_agent": to.to_string(),
                    "reason": error,
                }),
                format!(
                    "the handoff to {to} could not relaunch the {stage} stage: {error} \
                     (after the rate limit: {rate_limit})"
                ),
            )
        }
        None => (
            "stage_launch_failed",
            serde_json::json!({
                "stage": stage.to_string(),
                "agent": agent.to_string(),
                "error": error,
            }),
            format!("the runner could not start the {stage} agent: {error}"),
        ),
    }
}

/// The event, its payload and the infra reason for a launch preparation or advance the
/// runner could not complete (review r2 codex-6): `runner_advance_failed`.
fn failed_advance(
    phase: PhaseId,
    stage: Stage,
    agent: AgentKind,
    error: &str,
) -> (&'static str, serde_json::Value, String) {
    (
        "runner_advance_failed",
        serde_json::json!({
            "stage": stage.to_string(),
            "agent": agent.to_string(),
            "error": error,
        }),
        format!("the runner could not advance phase {phase} at the {stage} stage: {error}"),
    )
}

/// The runner's exit code for the state on disk after its loop (review r2 agy-3): 0 for
/// a run that reads finished, or stopped at its `--until` stage (transition's clean stop
/// leaves `stage` at the stage that completed, with `stop_until == Some(stage)`); 1 when
/// the state is absent (an abort cleared it), stopped any other way, or anything else.
fn runner_exit_code(state: Option<&State>) -> i32 {
    match state {
        Some(state) if state.finished => 0,
        Some(state) if state.stopped && state.stop_until == Some(state.stage) => 0,
        _ => 1,
    }
}

/// What the runner does before each [`monitor::run_stage`], in place of what the
/// supervised path's launch does around its spawn: archive the previous capture
/// ([`archive_prior_capture`]), register the phase (best effort, as a supervised
/// launch does), and emit `stage_launched` with this runner's pid.
fn begin_runner_stage(launch: &StageLaunch) -> Result<(), CliError> {
    let state = &launch.state;
    archive_prior_capture(state, launch.archived_stage)?;
    let cache_dir = launch_cache_dir()?;
    let _ = devflow_core::registry::register_in(&cache_dir, &state.project_root, state.phase);
    let label = launch_label(&launch.launch);
    let runner_pid = std::process::id();
    events::emit(
        &state.project_root,
        state.phase,
        "stage_launched",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "agent": state.agent.to_string(),
            "launch": label,
            "runner_pid": runner_pid,
        }),
    );
    println!(
        "stage {} → launched {} (runner pid {runner_pid}, {label})",
        state.stage,
        agents::driver_for(state.agent).name()
    );
    Ok(())
}

/// The [`monitor::StageSpec`] the runner runs `launch` with.
fn runner_stage_spec(launch: StageLaunch) -> monitor::StageSpec {
    let StageLaunch {
        state,
        program,
        args,
        extra_env,
        launch,
        ..
    } = launch;
    let io = match launch {
        StageShape::Stream { prompt } => monitor::StageIo::Stream {
            prompt,
            idle_timeout: monitor::idle_timeout_setting_for(state.agent).timeout,
        },
        StageShape::Capture => monitor::StageIo::Capture,
    };
    let mut spec = monitor::StageSpec::new(
        state.project_root.clone(),
        state.phase,
        state.agent,
        program,
        io,
    );
    spec.stage = Some(state.stage);
    spec.workdir = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| state.project_root.clone());
    spec.args = args;
    spec.envs = extra_env;
    // An open question holds the idle window for at most one capped call plus
    // its client margin of silence (OD-1 (b), review concern 13).
    spec.question_wait = Some(Duration::from_secs(
        agents::ASK_HOLD_CAP_SECS + agents::ASK_CLIENT_TIMEOUT_MARGIN_SECS,
    ));
    spec
}

/// Resolve, once per launch, the cache directory a launch registers and reserves
/// under (50-10): [`devflow_core::registry::cache_dir`]. There is no environment
/// override: the test-build branch below is selected at compile time.
#[cfg(not(test))]
pub(crate) fn launch_cache_dir() -> Result<PathBuf, CliError> {
    devflow_core::registry::cache_dir().ok_or_else(|| {
        CliError::Message(
            "cannot resolve DevFlow's cache directory for this launch: set HOME, \
             XDG_CACHE_HOME or DEVFLOW_CACHE_DIR"
                .to_string(),
        )
    })
}

/// The test build of [`launch_cache_dir`] (50-10, D-07): the cache dir comes from the
/// test process's own `DEVFLOW_CACHE_DIR`, which only a child test process given one
/// through `run_hermetic_test_in_child`'s `extra_env` has, so an in-crate test that
/// resolves a cache dir any other way fails here, loudly, instead of registering under
/// the developer's real HOME.
#[cfg(test)]
pub(crate) fn launch_cache_dir() -> Result<PathBuf, CliError> {
    std::env::var_os("DEVFLOW_CACHE_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::Message(
                "in-crate test launched a stage without a hermetic DEVFLOW_CACHE_DIR; \
                 run it through run_hermetic_test_in_child with extra_env"
                    .to_string(),
            )
        })
}

/// The rate-limit handoff a [`StageLaunch`] relaunches the stage after (SUPV-02), so a
/// runner that cannot start it names the handoff ([`failed_start`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandoffRelaunch {
    /// The agent that hit the rate limit.
    pub(crate) from: AgentKind,
    /// The fallback the stage was handed to.
    pub(crate) to: AgentKind,
    /// The rate-limit reason the handoff followed, when the outcome gave one.
    pub(crate) rate_limit: Option<String>,
}

/// Give the phase back after a `start` that failed, or returned without launching: a
/// state its own guard write `created` (no run was ever written into it) is removed,
/// and otherwise nothing changes. The phase holder the caller still holds keeps every
/// other launcher out until then (D-07). A failure here is only warned about.
pub(crate) fn abandon_reservation(project_root: &Path, phase: PhaseId, created: bool) {
    if !created {
        return;
    }
    if let Err(err) = workflow::clear_state(project_root, phase) {
        eprintln!("warning: could not remove phase {phase}'s unlaunched state: {err}");
    }
}

/// One stage launch, resolved and not yet spawned, with the state as committed. The foreground
/// runner runs it in its own process ([`run_runner`]).
#[derive(Debug)]
pub(crate) struct StageLaunch {
    /// The state the launch was resolved from, as committed.
    pub(crate) state: State,
    /// The agent program.
    pub(crate) program: String,
    /// The agent's argv after the program.
    pub(crate) args: Vec<String>,
    /// The adapter environment.
    pub(crate) extra_env: Vec<(String, String)>,
    /// The stage whose capture this launch archives, when it is not `state.stage`.
    pub(crate) archived_stage: Option<Stage>,
    /// The supervision shape.
    pub(crate) launch: StageShape,
    /// The rate-limit handoff this launch relaunches the stage after, set only by
    /// [`fail_over_on_rate_limit`].
    pub(crate) relaunch: Option<HandoffRelaunch>,
}

/// Commit the state before anything is spawned, as this save always has, so the launch
/// carries the state on disk, and return the launch.
/// It once also cleared the previous stage's monitor pid (WR-04, 18-fix); that field is
/// gone from the CLI (50-22).
fn stage_launch(
    state: &mut State,
    program: &str,
    args: &[String],
    extra_env: &[(String, String)],
    archived_stage: Option<Stage>,
    launch: StageShape,
) -> Result<StageLaunch, CliError> {
    commit(state, |_| {})?;
    Ok(StageLaunch {
        state: state.clone(),
        program: program.to_string(),
        args: args.to_vec(),
        extra_env: extra_env.to_vec(),
        archived_stage,
        launch,
        relaunch: None,
    })
}

/// Archive the previous launch's capture before this launch overwrites it: `-stdout`
/// and `-exit` move into history and `-idle-timeout` and `-agent-pid` are removed
/// ([`agent_result::archive_phase_files`]), with a `capture_archived` event when
/// anything was archived.
fn archive_prior_capture(state: &State, archived_stage: Option<Stage>) -> Result<(), CliError> {
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
    Ok(())
}

#[cfg(test)]
thread_local! {
    /// Set on a thread to record each [`launch_stage`] call made on it, as the
    /// stage and agent it launches, before anything can refuse the launch: a
    /// unit test's launch seam, where no agent binary is on `PATH`.
    pub(crate) static LAUNCH_PROBE: std::cell::RefCell<Option<Vec<(Stage, AgentKind)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Resolve the launch of `state.stage` and hand it back for the runner's loop to run
/// ([`run_runner`]): `None` when the preflight stopped the launch. An optional
/// `prompt_override` is used for Code loop-backs (fix prompts).
///
/// Resolves the prompt/adapter/roots/program, validates the agent binary, then runs
/// the readiness gate ([`run_preflight`]) and turns its outcome into the launch:
/// checks that pass, and a preflight gate a human approved, both resolve through
/// [`launch_stage_inner`]. An approval resolves with no prompt override and no
/// archived stage and skips the re-check for this one launch only (18f, D-18f,
/// T-18-28: the skip must never leak beyond the single stage a human approved).
pub(crate) fn launch_stage(
    state: &mut State,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<Option<StageLaunch>, CliError> {
    #[cfg(test)]
    LAUNCH_PROBE.with_borrow_mut(|probe| {
        if let Some(launches) = probe {
            launches.push((state.stage, state.agent));
        }
    });
    let driver = agents::driver_for(state.agent);
    let prompt = prompt_override
        .clone()
        .unwrap_or_else(|| default_stage_prompt(state, driver.as_ref()));
    let roots = state
        .worktree_path
        .as_deref()
        .map(|wt| worktree_writable_roots(&state.project_root, wt))
        .unwrap_or_default();
    let (program, _args) = driver.build_command(state.phase, &prompt, &roots);
    ensure_agent_binary(program)?;
    crate::preflight::ensure_ask_registration(state.agent)?;

    // 17c (Task 1, D-13-D-16): a scoped readiness gate runs before any agent
    // time is spent — a failing check surfaces as a named preflight gate +
    // notify (never a hard exit, D-15), not here.
    //
    // CR-01 (17-08 gap closure): the preflight launches nothing itself, so the
    // agent is resolved exactly once, here, whichever way the preflight ended.
    let project_root = state.project_root.clone();
    let outcome = run_preflight(&project_root, state, driver.as_ref())?;
    launch_after_preflight(state, outcome, prompt_override, archived_stage)
}

/// The launch [`launch_stage`] resolves from `outcome`: checks that passed launch with
/// the caller's prompt and archived stage, an approved preflight gate launches once with
/// neither (D-18f, T-18-28), and a stopped preflight launches nothing.
pub(crate) fn launch_after_preflight(
    state: &mut State,
    outcome: PreflightOutcome,
    prompt_override: Option<String>,
    archived_stage: Option<Stage>,
) -> Result<Option<StageLaunch>, CliError> {
    match outcome {
        // `launch_stage_inner` renders the prompt again after the preflight, as it
        // always has: the gate can change what the default prompt says.
        PreflightOutcome::Proceed => {
            launch_stage_inner(state, prompt_override, archived_stage).map(Some)
        }
        PreflightOutcome::Approved => launch_stage_inner(state, None, None).map(Some),
        PreflightOutcome::Stop => Ok(None),
    }
}

/// Route an `Ambiguous` outcome from the PRIMARY advance() monitor loop
/// (A2, 41-antigravity UAT): the agent's own final message self-reported
/// success, but the CLI's result envelope was torn down by a transport-level
/// cancellation (`context canceled` / `context deadline exceeded`). The stage
/// is RE-DRIVEN — the same stage is relaunched with the same prompt — rather
/// than gated (the agent already succeeded) or advanced (a torn envelope is
/// not proof of a clean finish).
///
/// Bounded by the shared `infra_failures` ceiling (D-08): once bumping would
/// reach the ceiling, the re-drive stops and the outcome
/// routes through the infra gate/abort path. Never touches
/// `consecutive_failures`, and never advances.
pub(crate) fn handle_ambiguous_outcome(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<Option<StageLaunch>, CliError> {
    commit(state, |s| {
        s.infra_failures = s.infra_failures.saturating_add(1)
    })?;
    if state.infra_failures >= mode::MAX_INFRA_FAILURES {
        return gate_or_abort_infra(project_root, state, stage, reason);
    }

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

/// The agent a rate limit at `stage` hands off to (SUPV-02), or `None` when the
/// run should wait at the rate-limit gate instead
/// ([`handle_rate_limited_outcome`]).
///
/// `stage` is the stage the supervisor launched. Only [`Stage::Code`] is
/// eligible (operator ruling R-2): a Plan relaunch would meet the PLAN.md
/// short-circuit in the idempotent stage prompt and could accept a partial plan
/// (T-51-33), and no other stage was designed for a mid-stage driver change. The
/// fallback must also be unused (D-05) and differ from the agent driving the run,
/// since handing a stage to the agent that just hit the limit is no handoff.
fn failover_target(state: &State, stage: Stage) -> Option<AgentKind> {
    let fallback = state.fallback_agent?;
    (stage == Stage::Code && !state.fallback_consumed && fallback != state.agent)
        .then_some(fallback)
}

/// Why [`hand_off_agent`] substitutes the agent. Its wire text is the
/// `agent_handoff` event's `reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandoffReason {
    /// The operator's `devflow resume --agent`.
    ResumeAgent,
    /// A rate limit at Code with an unused fallback (SUPV-02).
    RateLimited,
}

impl HandoffReason {
    fn as_wire_str(self) -> &'static str {
        match self {
            HandoffReason::ResumeAgent => "resume --agent",
            HandoffReason::RateLimited => "rate_limited",
        }
    }
}

/// A handoff [`hand_off_agent`] refused before writing anything. Distinct from a
/// state-write error, which it returns as a `CliError`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HandoffRefusal {
    /// The full generic preflight bundle refused the candidate state.
    Preflight(String),
    /// The target's binary is missing or its driver's health check failed.
    Unavailable(String),
}

impl HandoffRefusal {
    /// The refusal's text, without any prefix.
    fn reason(&self) -> &str {
        match self {
            HandoffRefusal::Preflight(reason) | HandoffRefusal::Unavailable(reason) => reason,
        }
    }
}

/// What [`hand_off_agent`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HandoffOutcome {
    /// The state now names `to`; `from` is the agent it replaced.
    HandedOff { from: AgentKind },
    /// Nothing was written.
    Refused(HandoffRefusal),
}

/// Substitute `to` for the run's agent at its current stage (D-04): the one
/// function `devflow resume --agent` and rate-limit failover share.
///
/// Every check runs against a candidate copy before ANY mutation, so a refusal
/// leaves the state byte-identical. It runs the FULL generic preflight bundle
/// (unattended-launch-shape, interactivity), not just
/// interactivity: `launch_stage` runs this same bundle again via
/// `run_preflight`, and it is stricter than plain interactivity — e.g. a
/// Claude/Antigravity-only chain-flag guard means a same-stage handoff to any
/// other agent in Auto mode fails the unattended-launch-shape check even though
/// the target driver declares the stage headless-safe. Checking only
/// interactivity here let such a handoff commit (`state.agent` mutated,
/// `agent_handoff` emitted) and then immediately hit that later gate — leaving
/// the phase "handed off" to a driver whose launch never actually ran
/// (44-CORE-REVIEW-FINDINGS.md finding 2b).
///
/// Then ONE commit sets the agent and, for a rate-limit hop, marks the fallback
/// consumed in the same write (D-05). The same write clears the recorded delivery
/// canary and session id (D-04): both are evidence about the previous agent, so
/// the new one proves its own delivery path. Then `agent_handoff` is emitted with
/// the tree it hands over: `tree_dirty` and the phase branch's `commit_count`,
/// each `null` when it could not be measured. The tree is handed over as it is
/// (D-10): nothing here resets, stashes, cleans or checks out the worktree, and no
/// commit count is required (D-08).
fn hand_off_agent(
    project_root: &Path,
    state: &mut State,
    to: AgentKind,
    reason: HandoffReason,
) -> Result<HandoffOutcome, CliError> {
    let from = state.agent;
    if let Some(refusal) = handoff_refusal(project_root, state, to) {
        return Ok(HandoffOutcome::Refused(refusal));
    }

    // D-10: what is handed over, measured before the commit and never touched.
    // `None` (git could not answer) stays `null` in the event, never a made-up value.
    let launch_root = state.worktree_path.as_deref().unwrap_or(project_root);
    let tree_dirty = crate::staleness::run_git_stdout(launch_root, &["status", "--porcelain"])
        .map(|porcelain| !porcelain.trim().is_empty());
    let commit_count = agent_result::phase_commit_count(
        project_root,
        &devflow_core::config::git_flow_for_run(project_root, state.base_branch.as_deref()),
        state.phase,
    );

    commit(state, |s| {
        s.agent = to;
        if reason == HandoffReason::RateLimited {
            s.fallback_consumed = true;
        }
        // D-04: the canary outcome and the session id are evidence about the
        // agent that produced them. Cleared here, `canary_gate` runs the new
        // agent's own canary on its first stream launch instead of admitting it
        // on the previous agent's `Confirmed`.
        s.canary = None;
        s.session_id = None;
    })?;
    events::emit(
        project_root,
        state.phase,
        "agent_handoff",
        serde_json::json!({
            "stage": state.stage.to_string(),
            "from_agent": from.to_string(),
            "to_agent": to.to_string(),
            "reason": reason.as_wire_str(),
            "tree_dirty": tree_dirty,
            "commit_count": commit_count,
        }),
    );
    Ok(HandoffOutcome::HandedOff { from })
}

/// The checks [`hand_off_agent`] makes before it writes anything, on a copy of `state`
/// naming `to`: the full generic preflight bundle, the target's binary and ask
/// registration, then its driver's health. `None` when all pass. Reads `state` only, so
/// a resume can judge a handoff at the stage it is about to launch before any write.
fn handoff_refusal(project_root: &Path, state: &State, to: AgentKind) -> Option<HandoffRefusal> {
    let mut candidate = state.clone();
    candidate.agent = to;
    if let Err(why) = generic_preflight_checks(project_root, &candidate) {
        return Some(HandoffRefusal::Preflight(why));
    }
    if let Err(err) = ensure_agent_binary(agent_program(to))
        .and_then(|()| crate::preflight::ensure_ask_registration(to))
    {
        return Some(HandoffRefusal::Unavailable(err.to_string()));
    }
    agents::driver_for(to)
        .health(&candidate)
        .err()
        .map(HandoffRefusal::Unavailable)
}

/// The error a resume reports for a handoff to `requested` refused at `stage`, the
/// stage the resume launches.
fn resume_handoff_refused(requested: AgentKind, stage: Stage, refusal: HandoffRefusal) -> CliError {
    CliError::Message(match refusal {
        HandoffRefusal::Preflight(reason) => format!(
            "handoff to {requested} refused at the {stage} stage this resume launches: {reason}"
        ),
        HandoffRefusal::Unavailable(err) => format!("handoff to {requested} refused: {err}"),
    })
}

/// Continue a rate-limited Code stage under the unused fallback `to` (SUPV-02).
///
/// Reached only from the explicit `RateLimited` arm of `advance_with`, after
/// [`failover_target`] selected `to`, instead of [`handle_rate_limited_outcome`]'s
/// gate. A hop is not an infra fault, so a successful one leaves
/// `infra_failures` alone (D-06). Once handed off, the same stage is relaunched
/// under the new driver — the tail of [`handle_ambiguous_outcome`] without its
/// bump and ceiling check. No commit-count precondition applies (D-08) and the
/// worktree is handed over untouched (D-10).
///
/// A refused handoff is never a silent skip (D-07): `agent_handoff_refused`
/// names the fallback and the refusal, then the run waits at the rate-limit
/// gate exactly as it would have with no fallback, given the ORIGINAL
/// rate-limit reason so the gate still names its reset time. Only a committed hop consumes the fallback (D-05),
/// so a refusal leaves it unused.
///
/// A relaunch that fails AFTER the handoff commit is never silent either (D-04,
/// R2-D1): `agent_handoff_relaunch_failed` names the stage, both agents and the
/// error, then [`handle_infra_outcome`] bumps `infra_failures` once and opens the
/// never-silent gate, or aborts at the ceiling. Not the rate-limit gate: a refused
/// canary stays recorded and refuses every later launch, so a resume after the
/// reset could not succeed, and a successor whose shutdown was not confirmed may still run.
fn fail_over_on_rate_limit(
    project_root: &Path,
    state: &mut State,
    phase: PhaseId,
    stage: Stage,
    to: AgentKind,
    reason: Option<String>,
) -> Result<Option<StageLaunch>, CliError> {
    match hand_off_agent(project_root, state, to, HandoffReason::RateLimited)? {
        HandoffOutcome::HandedOff { from } => {
            #[cfg(test)]
            test_hold("after_failover_commit");
            let relaunch = HandoffRelaunch {
                from,
                to,
                rate_limit: reason,
            };
            let relaunched = launch_stage(state, None, Some(stage));
            match relaunched {
                // Inside the runner the launch comes back here: it carries the handoff,
                // so a start the runner cannot make names it (`failed_start`).
                Ok(Some(mut launch)) => {
                    launch.relaunch = Some(relaunch);
                    Ok(Some(launch))
                }
                Ok(None) => Ok(None),
                Err(err) => {
                    let (event, payload, infra_reason) =
                        failed_start(stage, state.agent, Some(&relaunch), &err.to_string());
                    events::emit(project_root, phase, event, payload);
                    handle_infra_outcome(project_root, state, stage, Some(infra_reason))
                }
            }
        }
        HandoffOutcome::Refused(refusal) => {
            events::emit(
                project_root,
                phase,
                "agent_handoff_refused",
                serde_json::json!({
                    "stage": stage.to_string(),
                    "from_agent": state.agent.to_string(),
                    "to_agent": to.to_string(),
                    "reason": refusal.reason(),
                }),
            );
            handle_rate_limited_outcome(project_root, state, phase, stage, reason)
        }
    }
}

/// [`resume_with_fallback`] with no `--fallback-agent`: the shorthand the tests use.
#[cfg(test)]
pub(crate) fn resume(
    project_root: &Path,
    phase: PhaseId,
    agent: Option<AgentKind>,
) -> Result<(), CliError> {
    resume_with_fallback(project_root, phase, agent, None, false)
}

/// Resume a rate-limited or infra-paused phase from its saved stage (review
/// consensus #5). Loads the persisted `.devflow/state-{NN}.json` and
/// relaunches its saved stage via [`launch_stage`] — unlike `start`, this
/// does NOT call `State::new`, `feature_start`, or `ensure_phase_worktree`:
/// the branch/worktree already exist and agent/mode are read from the saved
/// state, so neither needs to be passed as a flag and the workflow is never
/// reset to Plan.
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
/// unchanged either way. A stopped run clears the cap only when the cap itself
/// stopped it ([`crate::pipeline_gate::until_cap_fired`]): an operator `devflow stop`
/// during the capped stage leaves `stopped` too, and keeps its cap through the resume.
///
/// The reservation at the top (D-01, D-08) runs [`crate::commands::resume_guards`] under
/// the write lock: it refuses a live run, a finished run, no state and a state that
/// does not load, each naming the one verb that applies (52.1 D-09.1); see
/// [`crate::commands::reserve_launch`]. A resume that fails, or
/// returns without spawning from it, gives the reservation back. The phase's holder is
/// claimed first (D-17), so a resume another foreground process refuses reserves nothing.
///
/// D-06: a rerun asks again. Before it relaunches, `resume` closes the phase's gates and
/// reports each answer it set aside and each claimed answer it found unapplied
/// ([`close_gates_for_resume`]). It never applies a pending answer to the rerun's gate.
///
/// D-04 (#250): `reviews` (`resume --reviews`) reruns a Plan its `--until plan` cap
/// halted as a reviews pass, `/gsd-plan-phase N --reviews`, with the cap kept, instead
/// of advancing; see [`resume_loaded`]. It combines with `--agent` and
/// `--fallback-agent` as a plain resume does.
pub(crate) fn resume_with_fallback(
    project_root: &Path,
    phase: PhaseId,
    agent: Option<AgentKind>,
    fallback_agent: Option<AgentKind>,
    reviews: bool,
) -> Result<(), CliError> {
    // 999.229: said first, so it prints even when the resume then refuses.
    if let Some(warning) = devflow_core::config::external_verify_env_warning_now() {
        eprintln!("{warning}");
    }

    // D-03: no launch without an answering user manager, and no fallback launch.
    crate::commands::refuse_without_a_user_manager(phase, crate::commands::LaunchVerb::Resume)?;
    // D-07: the holder covers the guards, every state write and the launch, until
    // systemd-run returns, so it keeps a second launcher out.
    let phase_holder = crate::commands::claim_foreground_holder(project_root, phase)?;
    // D-07a, review agy-1: a unit still stopping after a stop is waited out, not refused,
    // and the liveness read after it is the live row's fact.
    let liveness = crate::commands::wait_out_a_stopping_unit(
        project_root,
        phase,
        crate::commands::LaunchVerb::Resume,
    )?;
    let reserved = crate::commands::reserve_launch(
        project_root,
        phase,
        crate::commands::LaunchVerb::Resume,
        crate::commands::GuardFacts::from_liveness(&liveness),
        None,
    )?;
    #[cfg(test)]
    launch_test_hold("holder_claimed");
    // 999.251: a resume past a fired `--until` cap archives the halted stage's capture
    // under that stage, not the one it advances to.
    let archived_stage = resume_advance(&reserved.state, reviews).map(|(from, _)| from);
    let mut state = resume_loaded(
        project_root,
        phase,
        reserved.state,
        agent,
        fallback_agent,
        reviews,
    )?;
    // The runner checks the agent binary again at each launch; checked here too, so a
    // resume that cannot run its agent fails now, re-marked stopped below, rather than
    // in a unit nobody watches.
    // The stopped stage's capture is archived here, under the holder, as the runner's
    // stage start would: until then `logs -f` would read its exit file as the resumed
    // stage's.
    let launched =
        crate::preflight::ensure_agent_binary(crate::preflight::agent_program(state.agent))
            .and_then(|()| crate::preflight::ensure_ask_registration(state.agent))
            .and_then(|()| archive_prior_capture(&state, archived_stage))
            .map_err(|err| crate::service::LaunchError::Failed(err.to_string()))
            .and_then(|()| crate::service::launch_runner_unit(project_root, phase));
    let headline = format!("resumed phase {phase} at its {} stage", state.stage);
    match launched {
        Ok(unit) => {
            drop(phase_holder);
            crate::commands::print_runner_launched(phase, &headline, &unit);
            Ok(())
        }
        Err(crate::service::LaunchError::Failed(why)) => Err(re_mark_stopped_after_failed_launch(
            &mut state, phase, &why, reviews,
        )),
        // 999.244: systemd-run was killed at its bound, so the manager may have accepted
        // the unit; the runner is read once under the holder, which it never takes.
        Err(crate::service::LaunchError::Unknown { unit, why }) => {
            match crate::commands::read_timed_out_launch(project_root, phase) {
                crate::commands::TimedOutLaunch::Live => {
                    drop(phase_holder);
                    crate::commands::print_runner_launched(phase, &headline, &unit);
                    crate::commands::print_timed_out_live_note(&why);
                    Ok(())
                }
                crate::commands::TimedOutLaunch::NotLive => Err(
                    re_mark_stopped_after_failed_launch(&mut state, phase, &why, reviews),
                ),
                crate::commands::TimedOutLaunch::Unknown(reading) => Err(CliError::Message(
                    crate::commands::unconfirmed_launch_message("resume", phase, &why, &reading),
                )),
            }
        }
    }
}

/// The commit in [`resume`] cleared `stopped` so a reload mid-launch reads the run
/// active; a launch that failed outright makes that a lie (44-CORE finding 2a), so the run
/// is marked stopped again, naming what failed. A state on disk that already reads
/// `stopped` was stopped by its runner after that commit (a timed-out launch whose runner
/// ran, recorded its own stop and exited, #258), so its stop reason is the runner's and is
/// kept. Best effort: the launch error is what the caller needs either way.
///
/// A failed `resume --reviews` launch (`reviews`, D-04, review item 8) puts the
/// `--until plan` halt back instead: the halt's stop reason and no reviews pass, so
/// [`crate::pipeline_gate::until_cap_fired`] holds again and both a retried
/// `resume --reviews` and a plain `resume` are accepted. The runner's own recorded stop
/// is kept first, as above.
fn re_mark_stopped_after_failed_launch(
    state: &mut State,
    phase: PhaseId,
    why: &str,
    reviews: bool,
) -> CliError {
    let err = CliError::Message(format!("resume: phase {phase} was not launched: {why}"));
    let _ = commit(state, |s| {
        if !s.stopped {
            s.stopped = true;
            if reviews {
                s.stop_reason = Some(crate::pipeline_gate::until_halt_reason(Stage::Plan));
                s.reviews_pass = false;
            } else {
                s.stop_reason = Some(format!("resume launch failed: {err}"));
            }
        }
    });
    err
}

/// The label field of a quarantine file's name, `NN-{stage}.{label}.{secs}.{pid}.{seq}.quarantined.json`.
fn quarantine_file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('-'))
        .and_then(|(_, rest)| rest.split('.').nth(1))
        .unwrap_or("unknown")
        .to_string()
}

/// What `resume` says of a claimed answer it removed at the run's stage (R-02). The note
/// is printed escaped, because an operator typed it and this goes to a terminal.
fn unapplied_answer_line(stage: Stage, claimed: &ClaimedAnswer) -> String {
    let gate_id = &claimed.gate_id;
    let subject = match &claimed.answer {
        None => format!("an answer of unknown content for the {stage} gate (gate {gate_id})"),
        Some(answer) => {
            let decision = if answer.response.approved {
                "approved"
            } else {
                "rejected"
            };
            let note = answer
                .response
                .note
                .as_ref()
                .map_or_else(String::new, |note| format!(", note {note:?}"));
            format!("an answer for the {stage} gate (gate {gate_id}: {decision}{note})")
        }
    };
    format!(
        "resume: {subject} was claimed, with no record that it was applied; the rerun asks again"
    )
}

/// Every line `resume` prints about one stage it closed. `gate_id` is what the stage's gate
/// file named before the close; a stage with no gate file names an answer by the
/// quarantined file's own label.
///
/// `at_current_stage` is whether the run's state names this stage. A claimed pair there is
/// usually an answer whose consumer ended before acting on it, whatever `gate_pending` reads:
/// the consumer writes `gate_pending = false` before its caller acts (R2-05). It can also
/// have been acted on with the gate left open (an `--until` stop, a close that failed),
/// so the line says only what is known. A claimed pair at
/// any other stage is the leftover of an answer the run acted on and moved past, so it gets
/// no line here; `Gates::close` has logged its removal either way.
fn resume_gate_lines(
    stage: Stage,
    gate_id: Option<&str>,
    report: &CloseReport,
    at_current_stage: bool,
) -> Vec<String> {
    let mut lines: Vec<String> = report
        .quarantined
        .iter()
        .map(|path| {
            let label = gate_id.map_or_else(|| quarantine_file_label(path), str::to_string);
            format!(
                "resume: set aside an unused answer for the {stage} gate (gate {label}): {} — the \
                 rerun asks again",
                path.display()
            )
        })
        .collect();
    if at_current_stage {
        lines.extend(
            report
                .claimed
                .iter()
                .map(|claimed| unapplied_answer_line(stage, claimed)),
        );
    }
    lines
}

/// Close every stage's gate of the phase `resume` is about to rerun, under one gate-lock
/// acquisition, and return the lines that report it (D-06, R2-05). Every stage, not only
/// those with a gate file: a pair can outlive its gate file, and it is still closed and
/// reported. Each claimed answer at `current_stage` also leaves a
/// `resume_found_unapplied_answer` event.
///
/// A close error is a warning, not a refusal: the rerun's own gate sets the answer aside
/// when it opens, and what was closed before the error is still reported.
fn close_gates_for_resume(
    project_root: &Path,
    phase: PhaseId,
    current_stage: Stage,
) -> Vec<String> {
    for stage in [Stage::Plan, Stage::Code, Stage::Validate] {
        record_leftover_question(project_root, phase, stage);
    }
    // Read before the close, which removes the files that name them.
    let gate_ids = [Stage::Plan, Stage::Code, Stage::Validate].map(|stage| {
        let gate_id = crate::commands::gate_file_id(project_root, phase, stage)
            .ok()
            .flatten();
        (stage, gate_id)
    });
    let (reports, result) = Gates::close_phase(project_root, phase);
    if let Err(err) = result {
        eprintln!(
            "warning: resume could not close every gate of phase {phase}: {err}; the rerun's \
             own gate sets aside any answer left behind"
        );
    }
    let mut lines = Vec::new();
    for (stage, report) in &reports {
        let gate_id = gate_ids
            .iter()
            .find(|(candidate, _)| candidate == stage)
            .and_then(|(_, gate_id)| gate_id.as_deref());
        let at_current_stage = *stage == current_stage;
        lines.extend(resume_gate_lines(*stage, gate_id, report, at_current_stage));
        if at_current_stage {
            for claimed in &report.claimed {
                events::emit(
                    project_root,
                    phase,
                    "resume_found_unapplied_answer",
                    serde_json::json!({
                        "stage": stage.to_string(),
                        "gate_id": claimed.gate_id,
                        "approved": claimed.answer.as_ref().map(|answer| answer.response.approved),
                    }),
                );
            }
        }
    }
    lines
}

/// The question id and gate id of a `question` gate open at `stage`, or `None` for
/// no gate, any other kind of gate, or one that cannot be read (logged).
fn open_question_gate(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
) -> Option<(String, GateId)> {
    let gate = crate::pipeline_gate::read_gate_file(project_root, phase, stage)?;
    if gate.gate_kind != Some(devflow_core::gates::GateKind::Question) {
        return None;
    }
    let read = GateId::parse(&gate.gate_id)
        .map_err(|err| err.to_string())
        .and_then(
            |gate_id| match Gates::read_question(project_root, phase, stage) {
                Ok(Some(spec)) => Ok((spec.question_id, gate_id)),
                Ok(None) => Err("the question gate carries no question".to_string()),
                Err(err) => Err(err.to_string()),
            },
        );
    read.map_err(|err| {
        warn!("phase {phase}: could not read the {stage} question gate: {err}");
        eprintln!("phase {phase}: could not read the {stage} question gate: {err}");
    })
    .ok()
}

/// Record `question_closed` for a question the runner closes.
fn emit_runner_question_closed(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    (question_id, gate_id): (&str, &GateId),
    reason: &str,
    closed_by: Option<&str>,
) {
    events::emit(
        project_root,
        phase,
        events::QUESTION_CLOSED,
        serde_json::json!({
            "stage": stage.to_string(),
            "question_id": question_id,
            "gate_id": gate_id.as_str(),
            "reason": reason,
            "closed_by": closed_by,
        }),
    );
}

/// Close a question gate its agent left open at `stage` (C1 backstop, after any agent
/// exit). An abort bound to it, which the operator published before the server died,
/// is applied first: it commits [`State::abort_requested`] and records
/// `question_closed{aborted_by_operator}`, since the close would set the answer aside.
/// Otherwise the close records `question_closed{agent_exited}`. Any other gate is left
/// alone. Errors are logged, never returned.
fn close_question_gate(project_root: &Path, phase: PhaseId, stage: Stage) {
    let Some((question_id, gate_id)) = open_question_gate(project_root, phase, stage) else {
        return;
    };
    let question = (question_id.as_str(), &gate_id);
    match Gates::read_answer(project_root, phase, stage, &gate_id) {
        devflow_core::gates::AnswerRead::Bound(answer) if answer.response.abort => {
            let note = crate::ask_server::abort_note(answer.response.note.as_deref(), &question_id);
            let committed = workflow::update_state(project_root, phase, |state| {
                state.abort_requested = Some(note);
                Ok::<(), CliError>(())
            });
            if let Err(err) = committed {
                warn!("phase {phase}: the question's abort was not recorded: {err}");
                eprintln!(
                    "phase {phase}: the abort bound to question {question_id} was not recorded: {err}"
                );
            }
            let closed_by = answer.response.responded_by.as_deref();
            emit_runner_question_closed(
                project_root,
                phase,
                stage,
                question,
                "aborted_by_operator",
                closed_by,
            );
        }
        _ => {
            emit_runner_question_closed(project_root, phase, stage, question, "agent_exited", None)
        }
    }
    if let Err(err) = Gates::close_if_current(project_root, phase, stage, &gate_id) {
        warn!("phase {phase}: could not close the {stage} question gate {gate_id}: {err}");
        eprintln!("phase {phase}: could not close the {stage} question gate {gate_id}: {err}");
    }
}

/// Record `question_closed{closed_on_resume}` for a question gate `resume` is about to
/// close. The close itself is `resume`'s, which reports any answer it sets aside; a
/// bound abort is not applied, since the operator's `resume` is the later word.
fn record_leftover_question(project_root: &Path, phase: PhaseId, stage: Stage) {
    if let Some((question_id, gate_id)) = open_question_gate(project_root, phase, stage) {
        let question = (question_id.as_str(), &gate_id);
        emit_runner_question_closed(
            project_root,
            phase,
            stage,
            question,
            "closed_on_resume",
            None,
        );
    }
}

/// [`resume_with_fallback`] after its reservation, up to the launch: every refusal and
/// every state write (gate closing, notes, the handoff, the fallback re-arm) under the
/// caller's phase holder. `state` is the copy that wrote, which may already be behind the
/// file; the committed copy is returned for the caller to launch.
///
/// 999.251 (D-02): a run its own `--until` cap halted resumes at the stage
/// [`resume_advance`] names, with the bookkeeping a normal advance makes
/// ([`crate::pipeline_gate::enter_stage`]) and a `transition` event, recorded before any
/// handoff so `agent_handoff` names the stage launched. Every check before the first
/// write judges that stage. Any other stop reruns its stage.
///
/// `reviews` (`resume --reviews`, D-04) reruns the halted Plan instead of advancing:
/// the final commit sets [`State::reviews_pass`] and keeps the `--until plan` cap, so
/// the reviews Plan halts again and a plain `resume` then advances.
fn resume_loaded(
    project_root: &Path,
    phase: PhaseId,
    mut state: State,
    agent: Option<AgentKind>,
    fallback_agent: Option<AgentKind>,
    reviews: bool,
) -> Result<State, CliError> {
    crate::commands::refuse_unprepared_run(&state)?;
    // D-04, restrictive verbs: `--reviews` is judged before any write, the state first,
    // so a wrong-state refusal never depends on the review file.
    if reviews {
        if let Some(refusal) = reviews_resume_refusal(&state) {
            return Err(CliError::Message(refusal));
        }
        let launch_root = state
            .worktree_path
            .clone()
            .unwrap_or_else(|| project_root.to_path_buf());
        require_reviews_file_in_checkout(&launch_root, phase)?;
    }
    // 999.251 (D-02): a run its own `--until` cap halted resumes at the next stage;
    // any other stop reruns the stage it stopped in.
    let saved_stage = state.stage;
    let advance = resume_advance(&state, reviews);
    // Every check before the first write judges the stage this resume launches, on a
    // copy that is never written (`commit` applies its closure to the file's copy).
    let mut judged = state.clone();
    if let Some((from, to)) = advance {
        crate::pipeline_gate::enter_stage(&mut judged, from, to);
    }
    // D-01, D-02, D-03: every fallback check runs here, BEFORE `commit_notify_env`
    // below, so a refused resume commits nothing at all (review B-3).
    refuse_fallback_for_resume(&judged, agent, fallback_agent)?;
    refuse_recorded_canary_for_resume(&judged, agent)?;
    let handoff_to = agent.filter(|requested| *requested != state.agent);
    if let Some(requested) = handoff_to
        && let Some(refusal) = handoff_refusal(project_root, &judged, requested)
    {
        return Err(resume_handoff_refused(requested, judged.stage, refusal));
    }
    // CR-03 (49-REVIEW.md): an exported (or emptied) notify command reaches
    // the chain this resume launches through the state file.
    commit_notify_env(&mut state)?;
    if let Some((from, to)) = advance {
        // The cap is spent by the advance, so a later plain resume after a raced
        // handoff refusal below cannot re-halt on it.
        commit(&mut state, |s| {
            crate::pipeline_gate::enter_stage(s, from, to);
            s.stop_until = None;
        })?;
        events::emit(
            project_root,
            phase,
            "transition",
            serde_json::json!({
                "from": from.to_string(),
                "to": to.to_string(),
            }),
        );
    }
    if let Some(requested) = handoff_to {
        // The shared substitution (D-04): every check runs before anything is
        // mutated, so a refusal leaves the state byte-identical.
        match hand_off_agent(
            project_root,
            &mut state,
            requested,
            HandoffReason::ResumeAgent,
        )? {
            HandoffOutcome::HandedOff { from } => println!("handoff: {from} → {requested}"),
            HandoffOutcome::Refused(refusal) => {
                return Err(resume_handoff_refused(requested, state.stage, refusal));
            }
        }
    }
    // 35.1 D-01, `resume`'s half: this is the entry point that catches a leak a
    // SIGKILLed run left behind in its own worktree — the case the in-process
    // guard structurally cannot cover, because `Drop` never runs on a kill.
    // Placed after `load_state` (the state is what names the launch root) and
    // before the commit below, so the repair is complete before anything
    // is spawned. Root resolved with the same
    // `worktree_path.unwrap_or(project_root)` idiom `runner_stage_spec` uses for
    // the agent's workdir (F-11) — a second spelling here would repair a different copy
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
    // The stopped test reads the file as it is now: the operator's `resume` is the
    // later word on a stop that landed after this copy loaded, and a run launched
    // while still marked stopped could never advance.
    // D-06: the rerun asks again, so the gate this run abandons is closed before the
    // relaunch and its unused answer set aside. After every refusal above, so a refused
    // resume leaves the gate as it found it.
    // The gate it closes is the halted stage's, so it reads the saved stage.
    for line in close_gates_for_resume(project_root, phase, saved_stage) {
        println!("{line}");
    }
    commit(&mut state, |s| {
        if s.stopped {
            // D-04: a reviews resume reruns the halted Plan, so it does not pass the
            // `--until plan` boundary and keeps the cap; `until_halt_target` stays as the
            // halt wrote it, read only after a fired cap, whose halt rewrites it.
            if crate::pipeline_gate::until_cap_fired(s) && !reviews {
                s.stop_until = None;
            }
            s.stopped = false;
            s.stop_reason = None;
        }
        if reviews {
            s.reviews_pass = true;
        }
        s.gate_pending = false;
        // D-01's re-arm, in the commit every resume makes, so it applies to a stopped
        // run and one whose supervisor exited alike. A plain resume leaves a spent fallback
        // spent (D-05).
        if let Some(fallback) = fallback_agent {
            s.fallback_agent = Some(fallback);
            s.fallback_consumed = false;
        }
    })?;
    if reviews {
        println!(
            "resume: Plan reruns as a reviews pass (/gsd-plan-phase {phase} --reviews); the \
             --until plan cap still holds, so the run halts again after Plan"
        );
    }
    if let Some(fallback) = fallback_agent {
        print_fallback_notice(fallback, state.agent);
    }
    Ok(state)
}

/// The advance a resume makes when the run's own `--until` cap stopped it
/// ([`crate::pipeline_gate::until_cap_fired`]), else `None`: the halted stage and the
/// stage the halted transition was moving to ([`State::until_halt_target`], #281), so a
/// halt reached through Plan->Validate resumes at Validate. A state without a recorded
/// target advances to [`Stage::next`]; an `--until` halt never happens after Validate,
/// so a fired cap always has a next.
///
/// A reviews resume (`resume --reviews`, D-04) never advances: it reruns the halted
/// Plan as a reviews pass, so it is `None` too.
fn resume_advance(state: &State, reviews: bool) -> Option<(Stage, Stage)> {
    if reviews || !crate::pipeline_gate::until_cap_fired(state) {
        return None;
    }
    state
        .until_halt_target
        .or_else(|| state.stage.next())
        .map(|to| (state.stage, to))
}

/// The refusal of a `resume --reviews` (D-04) on a run that is not halted after Plan by
/// its own `--until plan` cap, else `None`. Restrictive verbs: the flag does one job,
/// rerunning that halted Plan as a reviews pass, and names the plain resume otherwise.
fn reviews_resume_refusal(state: &State) -> Option<String> {
    if crate::pipeline_gate::until_cap_fired(state) && state.stage == Stage::Plan {
        return None;
    }
    let phase = state.phase;
    let stage = state.stage;
    let fact = if state.stopped {
        let reason = state.stop_reason.as_deref().unwrap_or("no reason recorded");
        format!("stopped in {stage}: {reason}")
    } else {
        format!("is in {stage} and not stopped")
    };
    Some(format!(
        "resume: --reviews reruns Plan as a reviews pass only on a run its --until plan cap \
         halted after Plan; phase {phase} {fact}; run devflow resume --phase {phase} without \
         --reviews"
    ))
}

/// D-04: refuse a `resume --reviews` whose run checkout, `launch_root` (the tree the
/// reviews Plan reads), has no `{padded}-REVIEWS.md` in the phase's directory on disk.
/// Unlike `start`, which reads the base it forks from, the run's tree already exists.
fn require_reviews_file_in_checkout(launch_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    let padded = phase.padded();
    let present = devflow_core::gsd_reader::phase_dir(launch_root, phase)
        .is_ok_and(|dir| dir.join(format!("{padded}-REVIEWS.md")).is_file());
    if present {
        return Ok(());
    }
    Err(crate::commands::reviews_file_refusal(
        crate::commands::LaunchVerb::Resume,
        phase,
        &format!("in the run's checkout {}", launch_root.display()),
    ))
}

/// Refuse a `resume` the run's recorded delivery canary would refuse, before it writes
/// anything.
///
/// A `--agent` naming another agent hands the run off, and the handoff clears the
/// recorded canary, so it is never refused here.
/// Otherwise a recorded refusal ([`recorded_canary_refusal`]) refuses every launch under
/// the run's agent. Letting the resume go on would clear `stopped` and `stop_reason`,
/// fail at [`canary_gate`], and re-mark the run stopped with a reason that no longer names
/// the way on. Refused here, the stop reason that names it survives.
///
/// Reads no file and writes nothing; `state` is the copy `resume` loaded.
fn refuse_recorded_canary_for_resume(
    state: &State,
    agent: Option<AgentKind>,
) -> Result<(), CliError> {
    if agent.is_some_and(|requested| requested != state.agent) {
        return Ok(());
    }
    match recorded_canary_refusal(state) {
        None => Ok(()),
        Some(refusal) => Err(CliError::Message(format!(
            "resume refused: the delivery canary recorded for this run refuses {agent} at \
             the {stage} stage ({refusal}), so the relaunch would be refused too; run \
             `devflow resume --phase {phase} --agent <other>` to continue on another agent \
             (the handoff clears the recorded canary). Nothing was changed.",
            agent = state.agent,
            stage = state.stage,
            phase = state.phase,
        ))),
    }
}

/// The fallback checks `resume` makes before it writes anything (D-01, D-02, D-03).
///
/// The run is judged as it would launch: its agent after `--agent`. Then:
/// - a `--fallback-agent` is held to [`validate_fallback_agent`], as on `start`;
/// - with none, a saved fallback that is still unused and equals that agent is refused
///   (D-02): `--agent` onto the run's own fallback would leave the two coinciding.
///
/// Reads no file and writes nothing; `state` is the copy `resume` loaded.
fn refuse_fallback_for_resume(
    state: &State,
    agent: Option<AgentKind>,
    fallback_agent: Option<AgentKind>,
) -> Result<(), CliError> {
    let mut candidate = state.clone();
    candidate.agent = agent.unwrap_or(state.agent);
    match fallback_agent {
        Some(fallback) => validate_fallback_agent(&candidate, fallback),
        None => match state.fallback_agent {
            Some(saved) if !state.fallback_consumed && saved == candidate.agent => {
                Err(CliError::Message(format!(
                    "--agent {saved} is this run's saved unused fallback ({saved}); a run's agent \
                     and its fallback must differ — pass --fallback-agent with another agent to \
                     re-arm a different one"
                )))
            }
            _ => Ok(()),
        },
    }
}

/// The single active phase: `Ok(Some)` when exactly one is active, `Ok(None)`
/// when none, and an error naming the candidates when several are — `logs`'s
/// default-phase resolution.
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

/// The reason a failed stage that declared a blocking-human checkpoint carries
/// to the failure gate (D-13).
const UNASKED_CHECKPOINT_REASON: &str =
    "the agent reported a blocking-human checkpoint it could not ask through devflow_ask";

/// Append `why` to the reason the agent itself reported, or let it stand alone
/// when the agent reported none, so a declared checkpoint never reads as a
/// generic stage failure at the gate.
fn augment_unresolved_checkpoint_reason(reason: Option<String>, why: &str) -> String {
    match reason {
        Some(r) if !r.is_empty() => format!("{r} — {why}"),
        _ => why.to_string(),
    }
}

/// A test's advance of `phase` with no stage binding: the launch it hands back, which
/// no test runs (the runner's loop is the only caller that runs one).
#[cfg(test)]
pub(crate) fn advance(
    project_root: &Path,
    phase: Option<PhaseId>,
) -> Result<Option<StageLaunch>, CliError> {
    advance_for_stage(
        project_root,
        phase.expect("a test advance names its phase"),
        None,
    )
}

/// Advance the stage machine after the agent for `stage` exited: the next
/// [`StageLaunch`] for the runner to run ([`run_runner`]), or `None` when the run has
/// nothing more to launch (it finished, halted, aborted, or waits on nothing).
pub(crate) fn advance_for_stage(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
) -> Result<Option<StageLaunch>, CliError> {
    advance_with(project_root, phase, stage)
}

pub(crate) fn advance_with(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
) -> Result<Option<StageLaunch>, CliError> {
    // No long lock (D-08): a fresh supervisor's advance is ordered behind its launcher's
    // publish by the promotion wait, and each write takes the state write lock for
    // itself. A duplicate advance of this phase is refused by stage binding below once
    // the first has moved the stage on.
    let mut state = workflow::load_state(project_root, phase)?;
    // Safe point 1 (50-13): a stop taken while this supervisor waited for
    // its promotion ends the advance before it evaluates anything.
    if stopped_at_safe_point(&mut state, 1)? {
        return Ok(None);
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
    // CR-03 (49-REVIEW.md): an advance whose environment sets the notify variable
    // updates the run's notify command; with none set it keeps the persisted one.
    commit_notify_env(&mut state)?;

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

    // C1 backstop: a question gate the stage's agent left behind (its server was
    // killed) is closed now, applying an abort bound to it, so the check below sees
    // the abort and no question gate outlives its agent.
    close_question_gate(project_root, phase, state.stage);
    state = workflow::load_state(project_root, phase)?;

    // D-10: an operator's abort at a question ends the run once its agent has
    // exited. The question server recorded it; the abort removes the state file,
    // which consumes it. The stage outcome is never routed in that case.
    if let Some(note) = state.abort_requested.clone() {
        crate::pipeline_gate::abort(project_root, &state, &note)?;
        return Ok(None);
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

    // D-04 (28-03): record the session id for EVERY evaluated stage, so any
    // later process that needs it finds the value already durable. `session_id_from_capture` returns `None` for a
    // non-Claude agent's stdout (no `session_id` key at the JSON envelope's
    // top level) or a missing capture, so this is a safe no-op for every
    // non-Claude run.
    if let Some(session_id) = agent_result::session_id_from_capture(project_root, phase) {
        commit(&mut state, |s| s.session_id = Some(session_id))?;
    }

    // D-01/D-06: dispatch on the exhaustive outcome_policy::decide_action
    // table (no wildcard arm upstream) so a new/unhandled AgentStatus variant
    // is a compile error here rather than a silent advance. Replaces the old
    // `matches!(Failed | RateLimited)` boolean, which let Unknown fall
    // through into the success arm below.
    match outcome_policy::decide_action(stage, result.status) {
        Action::Advance => match stage {
            // GATE-03 (Pitfall 5): a gaps Plan pass must have written a new
            // gap-closure plan before the gaps-only Code pass runs.
            Stage::Plan if state.gap_closure_pass => {
                crate::pipeline_outcomes::handle_gaps_plan_outcome(project_root, &mut state)
            }
            Stage::Plan => transition(project_root, &mut state, Stage::Code),
            Stage::Code => transition(project_root, &mut state, Stage::Validate),
            Stage::Validate => {
                // 13b verdict-vs-ran + 18e: the Validate prompt now REQUIRES
                // a verdict, so ONLY an explicit `verdict: pass` finishes
                // the run. A missing verdict is a fail-safe (gate/loop), NOT a
                // silent pass — closes the composition bug where a
                // marker-less/verdict-less Validate could otherwise finish
                // it. `classify_validate_outcome` additionally resolves
                // the `external_verify` three-way matrix (D-18e): agreement
                // advances, disagreement/no-verdict gates immediately.
                handle_validate_outcome(
                    project_root,
                    &mut state,
                    classify_validate_outcome(&result),
                )
            }
        },
        Action::GateReview => {
            // D-13 (53-03): a stage that stopped at a blocking-human checkpoint
            // it could not ask through `devflow_ask` reaches the operator at the
            // never-silent failure gate, for every agent and stage, with a
            // reason naming it. Nothing relaunches the session (D-12).
            let declared_checkpoint = result.declares_blocking_human_checkpoint();
            let mut reason = result.reason.clone();
            if declared_checkpoint {
                reason = Some(augment_unresolved_checkpoint_reason(
                    reason,
                    UNASKED_CHECKPOINT_REASON,
                ));
            }
            match stage {
                // Validate failures drive the Code↔Validate loop (or a gate),
                // except a declared checkpoint: the loop would relaunch Code
                // on a question the operator never saw and count it as a
                // validation failure (review concern 12).
                Stage::Validate if !declared_checkpoint => {
                    handle_validate_outcome(project_root, &mut state, ValidateOutcome::Failed)
                }
                // Every other failure (incl. Unknown, D-06) is never silent
                // (WR-11): it always fires a gate + notify instead of
                // returning a bare error or silently advancing.
                _ => handle_stage_failure(project_root, &mut state, stage, reason),
            }
        }
        // ResourceKilled/AgentUnavailable: a dedicated infra path, identical
        // for every stage (including Validate/Ship) — MUST NOT route through
        // handle_validate_outcome/handle_ship_failure, which would bump
        // consecutive_failures (review consensus #4, D-08).
        Action::GateInfra => handle_infra_outcome(project_root, &mut state, stage, result.reason),
        // RateLimited / Ambiguous (D-09 / A2). RateLimited hands off to an
        // unused fallback, or waits at the rate-limit gate without touching
        // the infra counter (D-20); Ambiguous (a transport cancel whose own
        // envelope still carried a success marker) re-drives the SAME stage
        // immediately, bounded by the shared infra-failure ceiling (D-08).
        Action::AutoResume => match result.status {
            agent_result::AgentStatus::Ambiguous => {
                handle_ambiguous_outcome(project_root, &mut state, stage, result.reason)
            }
            // SUPV-02: the failover branch sits ahead of the rate-limit gate,
            // and only `RateLimited` reaches it; the wildcard below never fails
            // over.
            agent_result::AgentStatus::RateLimited => match failover_target(&state, stage) {
                Some(to) => fail_over_on_rate_limit(
                    project_root,
                    &mut state,
                    phase,
                    stage,
                    to,
                    result.reason,
                ),
                None => handle_rate_limited_outcome(
                    project_root,
                    &mut state,
                    phase,
                    stage,
                    result.reason,
                ),
            },
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
    use devflow_core::test_support::{answer_open_gate, gate_answer_files, preanswer_gate};

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

    /// The infra ceiling is read from the committed counter: a stale copy below it
    /// does not re-drive the stage once the counter on disk has reached it. The agent-free
    /// `PATH` makes a re-drive fail at its binary check rather than start an agent.
    #[test]
    fn an_ambiguous_outcome_reads_the_infra_ceiling_from_the_committed_counter() {
        const NAME: &str = "pipeline_launch::tests::an_ambiguous_outcome_reads_the_infra_ceiling_from_the_committed_counter";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(91);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = 1;
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.infra_failures = mode::MAX_INFRA_FAILURES - 1;
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        let outcome = handle_ambiguous_outcome(
            root,
            &mut state,
            Stage::Code,
            Some("context canceled".into()),
        );

        assert!(
            matches!(
                workflow::load_state(root, phase),
                Err(workflow::WorkflowError::MissingState(_))
            ),
            "the ceiling on disk must abort, not re-drive the stage (handler returned {outcome:?})"
        );
        outcome.unwrap();
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
        assert!(!auto_chain_flag_eligible(Stage::Plan, Mode::Auto));
        assert!(!auto_chain_flag_eligible(Stage::Validate, Mode::Auto));

        // Exhaustiveness tripwire: this match names every variant with no
        // wildcard arm, so a new `Stage` fails to compile here rather than
        // slipping past the assertions above.
        for stage in [Stage::Plan, Stage::Code, Stage::Validate] {
            let expected = match stage {
                Stage::Code => true,
                Stage::Plan | Stage::Validate => false,
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

    /// D-46 (review 5, V3): the chain-flag guard an auto Code stage on the
    /// stream shape engages leaves `.planning/config.json` as it found it. Two
    /// committed shapes: a file with no `workflow._auto_chain_active` key, and
    /// one holding it `false` in 4-space indent (which `to_string_pretty` would
    /// rewrite). After the drop the D-46 dirty-tree read lists nothing and the
    /// bytes are the committed ones. Third row: an edit to another key made
    /// while the guard is engaged is kept, and the flag reads `false` (the
    /// guard restores only its own write). Control in the same test: while the
    /// guard is engaged, the same read lists the config, so it sees the write.
    #[test]
    fn an_auto_code_stage_leaves_the_gsd_config_as_it_found_it() {
        let active = auto_chain_flag_eligible(Stage::Code, Mode::Auto);
        assert!(
            active,
            "premise: an auto Code stage engages the flag `true`"
        );
        let no_key = "{\n  \"workflow\": {\n    \"auto_advance\": true\n  }\n}\n";
        let four_space_false =
            "{\n    \"workflow\": {\n        \"_auto_chain_active\": false\n    }\n}\n";
        let config_path = ".planning/config.json".to_string();
        for (row, committed, edit_while_engaged) in [
            ("no key", no_key, false),
            ("false, 4-space", four_space_false, false),
            ("no key, edited while engaged", no_key, true),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);
            std::fs::create_dir_all(root.join(".planning")).unwrap();
            std::fs::write(root.join(&config_path), committed).unwrap();
            let git = |args: &[&str]| {
                let output = devflow_core::test_support::git_command(root)
                    .args(args)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "git {args:?}: {output:?}");
            };
            git(&["add", &config_path]);
            git(&["commit", "-q", "-m", "gsd config"]);
            assert_eq!(
                crate::pipeline_outcomes::read_tree(root),
                crate::pipeline_outcomes::TreeRead::Clean,
                "{row}: premise: the committed tree is clean"
            );

            let guard = AutoChainGuard::engage(root, active);
            assert_eq!(
                crate::pipeline_outcomes::read_tree(root),
                crate::pipeline_outcomes::TreeRead::Dirty(vec![config_path.clone()]),
                "{row}: control: the read sees the engaged guard's write"
            );
            if edit_while_engaged {
                let mut value: serde_json::Value = serde_json::from_str(
                    &std::fs::read_to_string(root.join(&config_path)).unwrap(),
                )
                .unwrap();
                value["workflow"]["auto_advance"] = serde_json::Value::Bool(false);
                std::fs::write(
                    root.join(&config_path),
                    serde_json::to_string_pretty(&value).unwrap(),
                )
                .unwrap();
            }
            drop(guard);

            let after = std::fs::read_to_string(root.join(&config_path)).unwrap();
            if edit_while_engaged {
                let value: serde_json::Value = serde_json::from_str(&after).unwrap();
                assert_eq!(
                    value["workflow"]["auto_advance"],
                    serde_json::Value::Bool(false),
                    "{row}: the edit made while engaged is kept: {after}"
                );
                assert!(
                    !gsd_config::auto_chain_active(root).unwrap(),
                    "{row}: the flag reads false after the drop: {after}"
                );
            } else {
                assert_eq!(after, committed, "{row}: the bytes are the committed ones");
                assert_eq!(
                    crate::pipeline_outcomes::read_tree(root),
                    crate::pipeline_outcomes::TreeRead::Clean,
                    "{row}: the drop leaves no dirty path"
                );
            }
        }
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

    /// GSD's first `false` over a committed config that lacks the key is
    /// recorded in a commit, with no repair event and no killed-run wording:
    /// nothing leaked, so nothing may read as a killed run.
    #[test]
    fn recording_gsds_false_emits_no_repair_event_and_no_killed_run_note() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let config = root.join(".planning/config.json");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            "{\n  \"workflow\": {\n    \"auto_advance\": true\n  }\n}\n",
        )
        .unwrap();
        let git = |args: &[&str]| {
            let output = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
            String::from_utf8_lossy(&output.stdout).into_owned()
        };
        git(&["add", ".planning/config.json"]);
        git(&[
            "commit",
            "-q",
            "-m",
            "add gsd config without the chain flag",
        ]);
        std::fs::write(
            &config,
            "{\n  \"workflow\": {\n    \"auto_advance\": true,\n    \
             \"_auto_chain_active\": false\n  }\n}\n",
        )
        .unwrap();

        repair_leaked_auto_chain_flag(root, root, PhaseId::new(83), "start");

        assert!(
            repair_events(root).is_empty(),
            "recording GSD's false is not a repair and must write no repair event: {:?}",
            repair_events(root)
        );
        assert_eq!(
            git(&["log", "-1", "--format=%s"]).trim_end(),
            "chore(gsd): record GSD's workflow._auto_chain_active: false",
            "the launch repair must have recorded the working copy, or the absent event is vacuous"
        );
        let outcome = gsd_config::ClearOutcome {
            recorded_gsd_false: true,
            ..Default::default()
        };
        let note = auto_chain_repair_note("start", PhaseId::new(83), &outcome)
            .expect("a recorded false must be announced");
        assert!(
            note.contains("recorded GSD's workflow._auto_chain_active: false"),
            "the note must say what was recorded: {note}"
        );
        assert!(
            !note.contains("was killed before it could clear it"),
            "the note must not name a killed run: {note}"
        );
    }

    /// The words only the left-uncommitted note uses.
    const LEFT_UNCOMMITTED_NOTE: &str = "left .planning/config.json uncommitted";

    /// A small GSD config in GSD's own serialization, with the chain flag
    /// holding `flag`, or absent for `None`.
    fn gsd_config_text(flag: Option<bool>) -> String {
        let key = flag
            .map(|value| format!(",\n    \"_auto_chain_active\": {value}"))
            .unwrap_or_default();
        format!("{{\n  \"workflow\": {{\n    \"auto_advance\": true{key}\n  }}\n}}\n")
    }

    fn git_stdout(root: &Path, args: &[&str]) -> String {
        let output = devflow_core::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// A repository whose branch tip holds `committed` as
    /// `.planning/config.json` and whose working copy holds `working`.
    fn repo_with_gsd_config(committed: &str, working: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let config = root.join(".planning/config.json");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, committed).unwrap();
        git_stdout(root, &["add", ".planning/config.json"]);
        git_stdout(root, &["commit", "-q", "-m", "add gsd config"]);
        std::fs::write(&config, working).unwrap();
        dir
    }

    /// The foreground note `start` prints for the row a repository is in, read
    /// from the real outcome of the repair.
    fn start_note_for(committed: Option<bool>, working: Option<bool>) -> Option<String> {
        let dir = repo_with_gsd_config(&gsd_config_text(committed), &gsd_config_text(working));
        let outcome = gsd_config::force_clear_auto_chain(dir.path()).unwrap();
        auto_chain_repair_note("start", PhaseId::new(85), &outcome)
    }

    /// The branch tip holds GSD's `false` and the working copy lost the key.
    /// DevFlow commits nothing (operator ruling, 2026-11-15) and writes no
    /// repair event, and both entry points say in the foreground that the file
    /// is left uncommitted and what that costs.
    #[test]
    fn a_config_change_devflow_cannot_attribute_is_left_uncommitted_and_said() {
        let dir = repo_with_gsd_config(&gsd_config_text(Some(false)), &gsd_config_text(None));
        let root = dir.path();
        let head_before = git_stdout(root, &["rev-parse", "HEAD"]);

        repair_leaked_auto_chain_flag(root, root, PhaseId::new(84), "start");

        assert_eq!(
            git_stdout(root, &["rev-parse", "HEAD"]),
            head_before,
            "the launch repair must commit nothing in this row"
        );
        assert_eq!(
            git_stdout(root, &["status", "--porcelain"]),
            " M .planning/config.json\n",
            "the file stays modified and uncommitted"
        );
        assert!(
            repair_events(root).is_empty(),
            "nothing was repaired, so no repair event may be written: {:?}",
            repair_events(root)
        );
        // The launch repair changed nothing, so asking again reads the same row.
        let outcome = gsd_config::force_clear_auto_chain(root).unwrap();
        for entry_point in ["start", "resume"] {
            let note = auto_chain_repair_note(entry_point, PhaseId::new(84), &outcome)
                .unwrap_or_else(|| {
                    panic!("{entry_point}: a file left uncommitted must be said in the foreground")
                });
            assert!(
                note.starts_with(&format!(
                    "note: devflow {entry_point} {LEFT_UNCOMMITTED_NOTE} for phase 84"
                )),
                "{entry_point}: the note must name the entry point, the file and the phase: {note}"
            );
            assert!(
                note.contains("DevFlow cannot attribute the change"),
                "{entry_point}: the note must say why nothing was committed: {note}"
            );
            assert!(
                note.contains("Validate will not record a pass until it is committed"),
                "{entry_point}: the note must say what the uncommitted file costs: {note}"
            );
            assert!(
                !note.contains("was killed before it could clear it"),
                "{entry_point}: the note must not name a killed run: {note}"
            );
        }
    }

    /// A clean launch has no config difference, so nothing is said at all.
    #[test]
    fn a_clean_launch_prints_no_left_uncommitted_note() {
        assert_eq!(start_note_for(Some(false), Some(false)), None);
    }

    /// GSD's first `false` is committed, so the note is the recording one.
    #[test]
    fn recording_gsds_false_prints_no_left_uncommitted_note() {
        let note = start_note_for(None, Some(false)).expect("premise: a recorded false is said");

        assert!(
            note.contains("recorded GSD's workflow._auto_chain_active: false"),
            "premise: this is the recording note: {note}"
        );
        assert!(
            !note.contains(LEFT_UNCOMMITTED_NOTE),
            "a recorded false leaves nothing uncommitted: {note}"
        );
    }

    /// A leak in the working copy is repaired there, so the note is the
    /// killed-run one.
    #[test]
    fn a_repaired_leak_prints_no_left_uncommitted_note() {
        let note = start_note_for(Some(false), Some(true)).expect("premise: a repair is said");

        assert!(
            note.contains("was killed before it could clear it"),
            "premise: this is the killed-run note: {note}"
        );
        assert!(
            !note.contains(LEFT_UNCOMMITTED_NOTE),
            "a repaired leak leaves nothing uncommitted: {note}"
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
    /// `register_in(cache_dir, …)`, so a test's cache dir is the only
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
            "register_in(&cache_dir, &root, phase);\n"
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
    /// The run's agent is Codex, a capture-shape agent, persisted because
    /// `resume()` loads its own `State` from disk: the subject is resume's
    /// stop-marker semantics, not which launch path the relaunch takes. The
    /// mode is supervise because auto mode refuses Codex at Plan, the first
    /// stage (D-07's unattended check and the CONTEXT.md check).
    #[test]
    fn resume_clears_stop_marker_and_advances_past_stop_point() {
        const NAME: &str =
            "pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point";
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

        let phase = PhaseId::new(66);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None);

        let _runners = crate::service::double::LaunchedRunners;

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
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
    }

    /// A run at Plan, stopped with `stop_until = Some(Plan)` and `stop_reason`, saved
    /// for `resume` to load, with a Plan capture beside it for the resume to archive.
    fn save_stopped_in_plan(root: &Path, phase: PhaseId, agent: AgentKind, stop_reason: &str) {
        let mut state = State::new(phase, agent, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some(stop_reason.to_string());
        workflow::save_state(&state).unwrap();
        std::fs::write(agent_result::stdout_path(root, phase), "plan output\n").unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "0\n").unwrap();
    }

    /// The runner's first step on the reloaded state, with the launches it made.
    fn first_step_after_resume(root: &Path, phase: PhaseId) -> (State, Vec<(Stage, AgentKind)>) {
        let mut reloaded = workflow::load_state(root, phase).unwrap();
        LAUNCH_PROBE.set(Some(Vec::new()));
        let first = runner_first_step(&mut reloaded);
        let probe = LAUNCH_PROBE.take().expect("the probe was set");
        let launch = match first {
            Ok(Some(launch)) => launch,
            Ok(None) => panic!("the runner's first step resolved no launch: {probe:?}"),
            Err(err) => panic!("the runner's first step refused the launch: {err}"),
        };
        (launch.state, probe)
    }

    /// 999.251 (D-02): the dogfood procedure runs a `--reviews` replan between an
    /// `--until plan` halt and `resume`, so the resume launches Code, the stage after
    /// the one the cap halted, with the Plan capture archived as Plan's.
    #[test]
    fn resume_after_an_until_plan_halt_launches_code() {
        const NAME: &str = "pipeline_launch::tests::resume_after_an_until_plan_halt_launches_code";
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
        let phase = PhaseId::new(97);
        save_stopped_in_plan(
            root,
            phase,
            AgentKind::Codex,
            &crate::pipeline_gate::until_halt_reason(Stage::Plan),
        );

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        let mut probed = reloaded.clone();
        LAUNCH_PROBE.set(Some(Vec::new()));
        let first = runner_first_step(&mut probed);
        let probe = LAUNCH_PROBE.take().expect("the probe was set");
        assert_eq!(
            probe,
            vec![(Stage::Code, AgentKind::Codex)],
            "resume after an `--until plan` halt launches Code, not Plan: {probe:?}"
        );
        let launch = match first {
            Ok(Some(launch)) => launch,
            Ok(None) => panic!("the runner's first step resolved no Code launch"),
            Err(err) => panic!("the runner's first step refused the Code launch: {err}"),
        };
        assert_eq!(launch.state.stage, Stage::Code);
        assert_eq!(launch.state.agent, AgentKind::Codex);
        assert_eq!(reloaded.stage, Stage::Code, "the saved state reads Code");
        let transition = events_named(root, "transition")
            .pop()
            .expect("resume recorded a transition");
        assert_eq!(transition["from"], "plan");
        assert_eq!(transition["to"], "code");
        let archived = events_named(root, "capture_archived");
        let [archived] = archived.as_slice() else {
            panic!("resume archived one capture: {archived:?}");
        };
        let stage = &archived["stage"];
        assert_eq!(stage, "plan", "the Plan capture was archived as {stage}");
        assert_eq!(archived["to_stage"], "code");
    }

    /// Control for 999.251: an operator's `devflow stop` during the capped Plan leaves
    /// `stop_until = Some(Plan)` too, and its resume reruns Plan, which never finished.
    #[test]
    fn resume_after_an_operator_stop_in_plan_reruns_plan() {
        const NAME: &str =
            "pipeline_launch::tests::resume_after_an_operator_stop_in_plan_reruns_plan";
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
        let phase = PhaseId::new(98);
        save_stopped_in_plan(root, phase, AgentKind::Codex, "stopped via `devflow stop`");

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let (launched, probe) = first_step_after_resume(root, phase);
        assert_eq!(
            probe,
            vec![(Stage::Plan, AgentKind::Codex)],
            "an operator stop in Plan relaunches Plan: {probe:?}"
        );
        assert_eq!(launched.stage, Stage::Plan);
        assert_eq!(
            workflow::load_state(root, phase).unwrap().stage,
            Stage::Plan
        );
        assert!(
            events_named(root, "transition").is_empty(),
            "an operator stop records no transition"
        );
    }

    /// Write `{padded}-REVIEWS.md` into a phase directory of `root`'s checkout.
    fn write_reviews_file(root: &Path, phase: PhaseId) {
        let padded = phase.padded();
        let dir = root.join(format!(".planning/phases/{padded}-x"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{padded}-REVIEWS.md")), "# reviews\n").unwrap();
    }

    /// A Plan or Code state of `phase` stopped with `stop_until = Some(stage)` and `reason`.
    fn stopped_state(phase: PhaseId, stage: Stage, reason: &str) -> State {
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, PathBuf::new());
        state.stage = stage;
        state.stop_until = Some(stage);
        state.stopped = true;
        state.stop_reason = Some(reason.to_string());
        state
    }

    /// Restrictive verbs: an operator stop during the capped Plan is not a halt after
    /// Plan, so `resume --reviews` refuses it, naming why and the plain resume.
    #[test]
    fn reviews_resume_refuses_an_operator_stop_in_plan() {
        let phase = PhaseId::new(5276);
        let state = stopped_state(phase, Stage::Plan, "stopped via `devflow stop`");
        let refusal = reviews_resume_refusal(&state);
        assert!(
            refusal
                .as_deref()
                .is_some_and(|text| text.contains("halted after Plan")
                    && text.contains(&format!("devflow resume --phase {phase}"))),
            "resume --reviews accepted a Plan the operator stopped: {refusal:?}"
        );
    }

    /// Restrictive verbs: an `--until code` halt is a fired cap, but after Code, so
    /// `resume --reviews` refuses it.
    #[test]
    fn reviews_resume_refuses_an_until_code_halt() {
        let state = stopped_state(
            PhaseId::new(5277),
            Stage::Code,
            &crate::pipeline_gate::until_halt_reason(Stage::Code),
        );
        let refusal = reviews_resume_refusal(&state);
        assert!(
            refusal.is_some(),
            "resume --reviews accepted a halt after Code: {refusal:?}"
        );
    }

    /// D-04: `resume --reviews` on an `--until plan` halt whose checkout has no
    /// REVIEWS.md is refused, naming the expected path, and writes no state.
    #[test]
    fn resume_with_reviews_without_a_reviews_file_is_refused() {
        const NAME: &str =
            "pipeline_launch::tests::resume_with_reviews_without_a_reviews_file_is_refused";
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
        let phase = PhaseId::new(5275);
        save_stopped_in_plan(
            root,
            phase,
            AgentKind::Codex,
            &crate::pipeline_gate::until_halt_reason(Stage::Plan),
        );
        let padded = phase.padded();
        std::fs::create_dir_all(root.join(format!(".planning/phases/{padded}-x"))).unwrap();
        let state_file = workflow::state_path(root, phase);
        let before = std::fs::read(&state_file).unwrap();

        let result = resume_with_fallback(root, phase, None, None, true);
        let _runners = crate::service::double::LaunchedRunners;
        let err = result.err().map(|err| err.to_string()).unwrap_or_default();
        assert!(
            err.contains(&format!(".planning/phases/{padded}-*/{padded}-REVIEWS.md"))
                && err.contains(&root.display().to_string()),
            "resume --reviews without a REVIEWS.md was not refused naming the path: {err}"
        );
        assert_eq!(
            std::fs::read(&state_file).unwrap(),
            before,
            "a refused resume --reviews changed the state: {err}"
        );
    }

    /// Review item 8: a `resume --reviews` whose launch fails puts the `--until plan`
    /// halt back, so the run stays retryable by `resume --reviews` and a plain `resume`.
    #[test]
    fn a_failed_resume_with_reviews_launch_stays_retryable() {
        const NAME: &str =
            "pipeline_launch::tests::a_failed_resume_with_reviews_launch_stays_retryable";
        if run_resume_timeout_child(NAME).is_some() {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5278);
        save_stopped_in_plan(
            root,
            phase,
            AgentKind::Codex,
            &crate::pipeline_gate::until_halt_reason(Stage::Plan),
        );
        write_reviews_file(root, phase);
        crate::service::double::set_launch_end(crate::service::double::LaunchEnd::Failed);
        let error = resume_with_fallback(root, phase, None, None, true)
            .expect_err("a failed launch fails")
            .to_string();
        assert!(
            error.contains("systemd-run could not start unit"),
            "{error}"
        );
        let state = workflow::load_state(root, phase).unwrap();
        assert!(
            crate::pipeline_gate::until_cap_fired(&state)
                && !state.reviews_pass
                && reviews_resume_refusal(&state).is_none(),
            "a failed resume --reviews launch left the run unretryable: {state:?}"
        );

        crate::service::double::set_launch_end(crate::service::double::LaunchEnd::Started);
        let retry = resume_with_fallback(root, phase, None, None, true);
        let _runners = crate::service::double::LaunchedRunners;
        retry.expect("the retried resume --reviews launches");
    }

    /// #338 WR-01: a timed-out `resume --reviews` whose runner ran, wrote the state and
    /// died keeps that state and its reviews pass, so a plain `resume` reruns the
    /// reviews Plan instead of reading a finished `--until plan` halt and advancing.
    #[test]
    fn a_timed_out_resume_with_reviews_whose_runner_changed_the_state_keeps_the_reviews_pass() {
        const NAME: &str = "pipeline_launch::tests::a_timed_out_resume_with_reviews_whose_runner_changed_the_state_keeps_the_reviews_pass";
        const RUNNER_SESSION: &str = "runner-session-5394";
        if run_resume_timeout_child(NAME).is_some() {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5394);
        save_stopped_in_plan(
            root,
            phase,
            AgentKind::Codex,
            &crate::pipeline_gate::until_halt_reason(Stage::Plan),
        );
        write_reviews_file(root, phase);
        crate::service::double::set_launch_end(crate::service::double::LaunchEnd::TimedOutLeaving(
            Ok("inactive"),
        ));
        let runner_root = root.to_path_buf();
        crate::service::double::run_during_launch(move || {
            workflow::update_state(&runner_root, phase, |state| {
                state.session_id = Some(RUNNER_SESSION.to_string());
                Ok::<_, workflow::WorkflowError>(())
            })
            .expect("the stand-in runner writes the state");
        });

        let result = resume_with_fallback(root, phase, None, None, true);
        let _runners = crate::service::double::LaunchedRunners;
        let error = result
            .expect_err("a timed-out resume with no live runner fails")
            .to_string();

        let state = workflow::load_state(root, phase).unwrap();
        assert!(
            !crate::pipeline_gate::until_cap_fired(&state)
                && state.reviews_pass
                && state.stopped
                && state
                    .stop_reason
                    .as_deref()
                    .is_some_and(|reason| reason.starts_with("resume launch failed")),
            "a timed-out resume --reviews whose runner changed the state faked the --until plan halt: {state:?}"
        );
        assert!(
            state.session_id.as_deref() == Some(RUNNER_SESSION),
            "the failed resume overwrote what its runner wrote: {state:?}"
        );
        let unsaid = "the failed resume did not say it kept the changed state";
        assert!(error.contains("so it was kept"), "{unsaid}: {error}");
        assert!(error.contains("devflow status"), "{unsaid}: {error}");
        assert!(
            error.contains("devflow discard --phase 5394"),
            "{unsaid}: {error}"
        );
    }

    /// D-04 (#250), the resume clause: `resume --reviews` on a run its `--until plan`
    /// cap halted reruns Plan as a reviews pass instead of advancing, and keeps the cap,
    /// so the reviews Plan halts again.
    #[test]
    fn resume_with_reviews_after_an_until_plan_halt_reruns_plan_as_a_reviews_pass() {
        const NAME: &str = "pipeline_launch::tests::resume_with_reviews_after_an_until_plan_halt_reruns_plan_as_a_reviews_pass";
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
        let phase = PhaseId::new(5274);
        save_stopped_in_plan(
            root,
            phase,
            AgentKind::Codex,
            &crate::pipeline_gate::until_halt_reason(Stage::Plan),
        );
        write_reviews_file(root, phase);
        let saved = workflow::load_state(root, phase).unwrap();

        let result = resume_with_fallback(root, phase, None, None, true);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        let mut probed = reloaded.clone();
        LAUNCH_PROBE.set(Some(Vec::new()));
        let first = runner_first_step(&mut probed);
        let probe = LAUNCH_PROBE.take().expect("the probe was set");
        assert_eq!(
            probe,
            vec![(Stage::Plan, AgentKind::Codex)],
            "resume --reviews after an --until plan halt launched {probe:?}, not a reviews Plan"
        );
        match first {
            Ok(Some(launch)) => assert_eq!(launch.state.stage, Stage::Plan),
            Ok(None) => panic!("the runner's first step resolved no Plan launch"),
            Err(err) => panic!("the runner's first step refused the Plan launch: {err}"),
        }
        assert!(
            reloaded.reviews_pass,
            "resume --reviews set no reviews pass"
        );
        assert_eq!(
            reloaded.stop_until,
            Some(Stage::Plan),
            "the cap was dropped"
        );
        assert_eq!(reloaded.until_halt_target, saved.until_halt_target);
        assert!(!reloaded.stopped, "the resumed run still reads stopped");
        let prompt =
            default_stage_prompt(&reloaded, agents::driver_for(AgentKind::Claude).as_ref());
        assert!(
            prompt.contains(&format!("/gsd-plan-phase {phase} --reviews")),
            "the resumed Plan does not render the reviews pass: {prompt}"
        );
    }

    /// #281 (D-09): a Plan gate answered "validate again" under `--until plan` moves
    /// Plan->Validate, which the cap halts; the resume launches the Validate that move
    /// asked for, not Code, and spends the recorded target.
    #[test]
    fn resume_after_an_until_plan_halt_reached_through_validate_again_launches_validate() {
        const NAME: &str = "pipeline_launch::tests::resume_after_an_until_plan_halt_reached_through_validate_again_launches_validate";
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
        let phase = PhaseId::new(5380);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        workflow::save_state(&state).unwrap();
        std::fs::write(agent_result::stdout_path(root, phase), "plan output\n").unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "0\n").unwrap();
        // The move `Step::ValidateAgain` makes at a Plan gate.
        let halted = crate::pipeline_gate::transition(root, &mut state, Stage::Validate);
        assert!(
            matches!(halted, Ok(None)),
            "the cap halted the move to Validate"
        );

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let (launched, probe) = first_step_after_resume(root, phase);
        assert_eq!(
            probe,
            vec![(Stage::Validate, AgentKind::Codex)],
            "resume after a validate-again --until halt launched {probe:?}, not Validate"
        );
        assert_eq!(launched.stage, Stage::Validate);
        assert_eq!(
            workflow::load_state(root, phase).unwrap().until_halt_target,
            None,
            "the resume spent the halt target"
        );
    }

    /// #258 (operator decision 3): a resume whose launch timed out with its runner not
    /// live keeps the `--until` halt the runner recorded during the launch, so the run
    /// still reads as halted by its cap rather than as a failed resume.
    #[test]
    fn a_timed_out_resume_keeps_the_until_halt_its_runner_recorded() {
        const NAME: &str =
            "pipeline_launch::tests::a_timed_out_resume_keeps_the_until_halt_its_runner_recorded";
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
        let phase = PhaseId::new(5373);
        save_stopped_in_plan(root, phase, AgentKind::Codex, "stopped via `devflow stop`");
        crate::service::double::set_launch_end(crate::service::double::LaunchEnd::TimedOutLeaving(
            Ok("inactive"),
        ));
        let runner_root = root.to_path_buf();
        crate::service::double::run_during_launch(move || {
            workflow::update_state(&runner_root, phase, |state| {
                state.stopped = true;
                state.stop_reason = Some(crate::pipeline_gate::until_halt_reason(Stage::Plan));
                Ok::<_, workflow::WorkflowError>(())
            })
            .expect("the stand-in runner records its --until halt");
        });

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        assert!(
            result.is_err(),
            "a timed-out resume with no live runner fails"
        );

        let state = workflow::load_state(root, phase).unwrap();
        let reason = state.stop_reason.clone();
        assert!(
            crate::pipeline_gate::until_cap_fired(&state),
            "a resume whose launch timed out overwrote the --until halt its runner recorded: \
             stop_reason {reason:?}"
        );
    }

    /// 999.251 with `--agent`: the handoff is judged and recorded at Code, the stage the
    /// resume launches, after the `transition` that moved the run there.
    #[test]
    fn resume_with_agent_after_an_until_plan_halt_hands_off_at_code() {
        const NAME: &str =
            "pipeline_launch::tests::resume_with_agent_after_an_until_plan_halt_hands_off_at_code";
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
        let phase = PhaseId::new(99);
        save_stopped_in_plan(
            root,
            phase,
            AgentKind::Claude,
            &crate::pipeline_gate::until_halt_reason(Stage::Plan),
        );

        let result = resume(root, phase, Some(AgentKind::Codex));
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Codex);
        let events: Vec<serde_json::Value> =
            std::fs::read_to_string(devflow_core::events::events_path(root))
                .unwrap()
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();
        let position = |name: &str| events.iter().position(|event| event["event"] == name);
        let handoff_at = position("agent_handoff").expect("handoff event must exist");
        let stage = &events[handoff_at]["stage"];
        assert_eq!(
            stage, "code",
            "a resume --agent after an --until plan halt handed off at {stage}"
        );
        let transition_at = position("transition").expect("resume recorded a transition");
        assert_eq!(events[transition_at]["from"], "plan");
        assert_eq!(events[transition_at]["to"], "code");
        assert!(
            transition_at < handoff_at,
            "the transition is recorded before the handoff"
        );
        assert_eq!(reloaded.stage, Stage::Code, "the saved state reads Code");
    }

    /// A phase parked at an open, answered `stage` gate whose waiter was killed before it
    /// consumed the answer: the state says `gate_pending`, the gate file is on disk,
    /// and an unconsumed answer for that gate sits beside it. Returns the gate's id
    /// and the answer's bytes as planted.
    fn seed_gate_with_pending_answer(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        mode: Mode,
        response: &devflow_core::gates::GateResponse,
    ) -> (devflow_core::gates::GateId, Vec<u8>) {
        let mut state = State::new(phase, AgentKind::Codex, mode, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = stage;
        state.gate_pending = true;
        state.stopped = true;
        state.stop_reason = Some("stage failed; gate open".to_string());
        workflow::save_state(&state).unwrap();

        let opened = Gates::open_gate(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "[never-silent] stage failed — human review needed",
        )
        .unwrap();
        let answer_path = Gates::answer(root, phase, stage, &opened.gate_id, response).unwrap();
        let bytes = std::fs::read(&answer_path).unwrap();
        (opened.gate_id, bytes)
    }

    /// [`seed_gate_with_pending_answer`] at Code, with an aborting answer.
    fn seed_code_gate_with_pending_answer(
        root: &Path,
        phase: PhaseId,
        mode: Mode,
    ) -> (devflow_core::gates::GateId, Vec<u8>) {
        seed_gate_with_pending_answer(
            root,
            phase,
            Stage::Code,
            mode,
            &abort_response("abort: stale answer nobody polled"),
        )
    }

    /// A gate at `stage`, answered with an approval and claimed, as a consumer that then
    /// died leaves it. Returns the gate's id.
    fn plant_claimed_answer(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> devflow_core::gates::GateId {
        let opened = Gates::open_gate(
            root,
            phase,
            stage,
            devflow_core::gates::GateKind::StageFailure,
            "planted claimed answer",
        )
        .unwrap();
        Gates::answer(
            root,
            phase,
            stage,
            &opened.gate_id,
            &gate_response(true, None),
        )
        .unwrap();
        assert_eq!(
            Gates::claim(root, phase, stage, &opened.gate_id).unwrap(),
            devflow_core::gates::Claim::Claimed
        );
        opened.gate_id
    }

    /// The names of every event in the log, in order.
    fn event_order(root: &Path) -> Vec<String> {
        std::fs::read_to_string(events::events_path(root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|event| event["event"].as_str().map(str::to_string))
            .collect()
    }

    /// SURV-02 / Phase 48 criterion 3, as D-06 revised it: a `resume` whose relaunch opens
    /// no gate relaunches the saved stage, and neither applies nor deletes a pending
    /// answer. The gate here is a stage-failure gate, not a preflight refusal: the
    /// relaunch's preflight PASSES (Supervise mode), so nothing opens a new gate, and the
    /// close at the top of `resume` is the only thing that moved the answer.
    ///
    /// What happens when the relaunch does open a gate is
    /// `resume_quarantines_an_unused_gate_answer_and_opens_a_new_gate_id`.
    #[test]
    fn a_resume_that_opens_no_gate_sets_the_pending_answer_aside_and_relaunches() {
        const NAME: &str = "pipeline_launch::tests::a_resume_that_opens_no_gate_sets_the_pending_answer_aside_and_relaunches";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(91);
        let (gate, answer_before) =
            seed_code_gate_with_pending_answer(root, phase, Mode::Supervise);

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        // (a) the saved stage was relaunched.
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.stage,
            Stage::Code,
            "resume must relaunch the saved stage"
        );
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume must launch the saved stage exactly once, as the runner's unit"
        );

        // (b) the pending answer was set aside whole, and nothing consumed it.
        let quarantined = Gates::quarantined(root);
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(quarantined[0].label, gate.as_str());
        assert_eq!(
            std::fs::read(&quarantined[0].path).unwrap(),
            answer_before,
            "the set-aside file holds the answer's bytes"
        );
        assert_eq!(
            gate_answer_files(root, phase, Stage::Code),
            Vec::<PathBuf>::new(),
            "the answer is out of the answer slots"
        );
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "the abandoned gate is closed"
        );
        assert!(
            gate_claim_files(root, phase, Stage::Code).is_empty(),
            "a claim file means a consumer took the answer"
        );
        assert!(
            events_of_kind(root, "gate_resolved").is_empty(),
            "resume must not resolve the gate"
        );
    }

    /// D-06 and R-02: `resume` closes the gate before it relaunches, keeps the answer's
    /// bytes, and says so. libtest captures a passing test's `println!`, so the wording is
    /// pinned here on `resume_gate_lines` and the print itself by the e2e
    /// `resume_prints_the_answer_it_set_aside`.
    #[test]
    fn resume_reports_the_answers_it_set_aside_for_the_closed_gate() {
        const NAME: &str =
            "pipeline_launch::tests::resume_reports_the_answers_it_set_aside_for_the_closed_gate";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(93);
        let (gate, answer) = seed_code_gate_with_pending_answer(root, phase, Mode::Supervise);

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let quarantined = Gates::quarantined(root);
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(std::fs::read(&quarantined[0].path).unwrap(), answer);
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
        assert!(!workflow::load_state(root, phase).unwrap().gate_pending);
        // The relaunch is the runner's unit (52.2-06), launched after `resume_loaded`
        // returned: the idle test runner has run no stage, so the set-aside is in the log
        // and no stage launch is.
        let order = event_order(root);
        assert!(
            order.iter().any(|e| e == "gate_answer_quarantined")
                && !order.iter().any(|e| e == "stage_launched")
                && crate::service::double::launched_units().len() == 1,
            "the gate closes before the relaunch: {order:?}"
        );
        assert!(
            events_of_kind(root, "resume_found_unapplied_answer").is_empty(),
            "an answer nobody claimed is reported as unused, not as unapplied"
        );

        let path = &quarantined[0].path;
        let unused = CloseReport {
            quarantined: vec![path.clone()],
            ..CloseReport::default()
        };
        assert_eq!(
            resume_gate_lines(Stage::Code, Some(gate.as_str()), &unused, true),
            vec![format!(
                "resume: set aside an unused answer for the code gate (gate {gate}): {} — the \
                 rerun asks again",
                path.display()
            )]
        );
        assert_eq!(
            resume_gate_lines(Stage::Code, None, &unused, true),
            vec![format!(
                "resume: set aside an unused answer for the code gate (gate {gate}): {} — the \
                 rerun asks again",
                path.display()
            )],
            "with no gate file the quarantined file's own label names the gate"
        );

        let claimed = |answer| CloseReport {
            claimed: vec![ClaimedAnswer {
                gate_id: "G1".to_string(),
                answer,
            }],
            ..CloseReport::default()
        };
        let approved = claimed(Some(GateAnswer {
            gate_id: "G1".to_string(),
            response: gate_response(true, None),
        }));
        assert_eq!(
            resume_gate_lines(Stage::Code, Some("G1"), &approved, true),
            vec![
                "resume: an answer for the code gate (gate G1: approved) was claimed, with no \
                 record that it was applied; the rerun asks again"
                    .to_string()
            ]
        );
        assert_eq!(
            resume_gate_lines(Stage::Code, Some("G1"), &approved, false),
            Vec::<String>::new(),
            "a claimed pair at a stage the run left is an applied answer's leftover"
        );
        let rejected = claimed(Some(GateAnswer {
            gate_id: "G1".to_string(),
            response: gate_response(false, Some("fix it")),
        }));
        assert_eq!(
            resume_gate_lines(Stage::Code, None, &rejected, true),
            vec![
                "resume: an answer for the code gate (gate G1: rejected, note \"fix it\") was claimed, with no record that it was applied; the rerun asks again"
                    .to_string()
            ],
            "the claimed entry names itself, whatever the gate file said"
        );
        assert_eq!(
            resume_gate_lines(Stage::Code, None, &claimed(None), true),
            vec![
                "resume: an answer of unknown content for the code gate (gate G1) was claimed, with no record that it was applied; the rerun asks again"
                    .to_string()
            ]
        );
        assert_eq!(
            resume_gate_lines(Stage::Code, None, &CloseReport::default(), true),
            Vec::<String>::new(),
            "a stage with nothing to report prints nothing"
        );
    }

    /// R-02's condition, end to end: a claimed pair at a stage the run left is an applied
    /// answer's leftover. `resume` closes it, and core's event is all that is said of it.
    #[test]
    fn resume_says_nothing_of_a_claimed_pair_at_another_stage() {
        const NAME: &str =
            "pipeline_launch::tests::resume_says_nothing_of_a_claimed_pair_at_another_stage";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(95);
        seed_code_gate_with_pending_answer(root, phase, Mode::Supervise);
        let left_behind = plant_claimed_answer(root, phase, Stage::Plan);

        let result = resume(root, phase, None);
        result.unwrap();

        assert!(!Gates::claim_path(root, phase, Stage::Plan, &left_behind).exists());
        assert!(!Gates::answer_path(root, phase, Stage::Plan, &left_behind).exists());
        let removed = events_of_kind(root, "gate_claimed_answer_removed");
        assert_eq!(removed.len(), 1, "{removed:?}");
        assert_eq!(removed[0]["gate_id"], left_behind.as_str());
        assert_eq!(
            events_of_kind(root, "resume_found_unapplied_answer"),
            Vec::<serde_json::Value>::new()
        );
    }

    /// R2-05: a claimed pair at the run's stage is reported though `gate_pending` reads
    /// false and its gate file is already gone. A filter on either would skip it.
    #[test]
    fn resume_reports_a_claimed_pair_whose_gate_file_is_gone() {
        const NAME: &str =
            "pipeline_launch::tests::resume_reports_a_claimed_pair_whose_gate_file_is_gone";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(96);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.stopped = true;
        workflow::save_state(&state).unwrap();
        let gate = plant_claimed_answer(root, phase, Stage::Code);
        std::fs::remove_file(Gates::gate_path(root, phase, Stage::Code)).unwrap();

        let result = resume(root, phase, None);
        result.unwrap();

        assert!(!Gates::claim_path(root, phase, Stage::Code, &gate).exists());
        let found = events_of_kind(root, "resume_found_unapplied_answer");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0]["stage"], "code");
        assert_eq!(found[0]["gate_id"], gate.as_str());
        assert_eq!(found[0]["approved"], true);
    }

    /// D-06 (V-17): an answer whose waiter was killed before it consumed the answer
    /// never decides the gate a `resume` reaches next. `resume` closes the old gate and
    /// sets its answer aside before it launches the runner's unit (52.2-06), so the
    /// gate the rerun opens there, under its own id, has nothing in its answer slots.
    #[test]
    fn resume_quarantines_an_unused_gate_answer_and_opens_a_new_gate_id() {
        const NAME: &str = "pipeline_launch::tests::resume_quarantines_an_unused_gate_answer_and_opens_a_new_gate_id";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("codex");
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("1"))],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(92);
        let (old_gate, old_answer) = seed_code_gate_with_pending_answer(root, phase, Mode::Auto);

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        result.expect("the resume launches the runner unit");

        assert!(
            events_of_kind(root, "gate_resolved").is_empty(),
            "the killed waiter's answer must not decide the rerun's gate: {:?}",
            events_of_kind(root, "gate_resolved")
        );
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "the old gate is closed, so the rerun's gate opens under its own id"
        );
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "the rerun is the runner unit"
        );
        assert_eq!(
            gate_answer_files(root, phase, Stage::Code),
            Vec::<PathBuf>::new(),
            "the old answer is out of the answer slots"
        );

        let quarantined = Gates::quarantined(root);
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(quarantined[0].label, old_gate.as_str());
        assert_eq!(
            std::fs::read(&quarantined[0].path).unwrap(),
            old_answer,
            "the set-aside file holds the answer's bytes"
        );
        let events = events_of_kind(root, "gate_answer_quarantined");
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0]["reason"], "gate_closed");
        assert_eq!(events[0]["gate_id"], old_gate.as_str());
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
    /// The run's agent is Codex, a capture-shape agent, persisted because
    /// `resume()` reloads its own `State`: the subject is not which launch path
    /// the relaunch takes.
    #[test]
    fn resume_preserves_unfired_until_cap() {
        const NAME: &str = "pipeline_launch::tests::resume_preserves_unfired_until_cap";
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

        let phase = PhaseId::new(67);
        // `Mode::Supervise` for the same 35.1-03 reason as
        // `launch_stage_persists_the_supervisor_record_for_reload` above: a Codex
        // Code stage is refused in `Mode::Auto` by
        // `preflight_unattended_launch_check`, and this test's subject — the
        // unfired `stop_until` cap surviving a resume — is mode-independent.
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        // Current stage is the cap's own stage (Plan), which has not yet
        // completed, and the phase was NOT stopped by the cap — this is the rate-limit/infra
        // recovery shape, not the "cap already fired" shape.
        state.stop_until = Some(Stage::Plan);
        state.verification_baseline_captured = true;
        state.stopped = false;
        state.stop_reason = None;
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None);

        let _runners = crate::service::double::LaunchedRunners;

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
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
    }

    /// A run capped with `--until plan` that the operator stops in the middle of Plan
    /// reads `stopped` with `stage == Plan` and `stop_until == Some(Plan)`, the same
    /// shape the cap's own halt writes; only the stop reason differs. The cap has not
    /// fired, so `resume` keeps it and the resumed run still halts after Plan.
    /// `resume_clears_stop_marker_and_advances_past_stop_point` is the control: a run
    /// the cap halted has it cleared.
    #[test]
    fn resume_keeps_until_cap_after_an_operator_stop() {
        const NAME: &str = "pipeline_launch::tests::resume_keeps_until_cap_after_an_operator_stop";
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

        let phase = PhaseId::new(69);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some(runner_stop_reason(Some(StopSource::Signal(libc::SIGTERM))));
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None);

        let _runners = crate::service::double::LaunchedRunners;

        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.stop_until,
            Some(Stage::Plan),
            "an operator stop during the capped stage is not the cap firing: resume \
             must keep --until plan, or the resumed run goes on into Code"
        );
        assert!(!reloaded.stopped, "resume clears the operator's stop");
        assert_eq!(
            reloaded.stop_reason, None,
            "resume clears the stop's reason"
        );
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
    }

    /// `resume` archives the stopped stage's capture before it launches, so a
    /// `devflow logs -f` started right after it never reads the old stage's exit code
    /// as the resumed stage's. The archive is the one stage rollovers make: it is
    /// recorded as a `capture_archived` event.
    #[test]
    fn resume_archives_the_stopped_stages_capture_before_launch() {
        const NAME: &str =
            "pipeline_launch::tests::resume_archives_the_stopped_stages_capture_before_launch";
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

        let phase = PhaseId::new(70);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Plan;
        state.stopped = true;
        state.stop_reason = Some(runner_stop_reason(Some(StopSource::Signal(libc::SIGTERM))));
        workflow::save_state(&state).unwrap();
        let exit_file = agent_result::exit_code_path(root, phase);
        std::fs::write(agent_result::stdout_path(root, phase), "old plan output\n").unwrap();
        std::fs::write(&exit_file, "143\n").unwrap();

        let result = resume(root, phase, None);

        let _runners = crate::service::double::LaunchedRunners;

        result.unwrap();

        assert!(
            !exit_file.exists(),
            "resume left the stopped stage's exit file, which logs -f reads as the resumed \
             stage's exit"
        );
        assert_eq!(
            events_named(root, "capture_archived").len(),
            1,
            "the old capture was archived, not deleted"
        );
    }

    /// D-15 (999.60) third case: an ordinary rate-limit/infra resume with no
    /// `--until` cap at all (`stop_until: None`) must remain unaffected by
    /// gating the clear on `state.stopped` — nothing to preserve, nothing to
    /// clear, and the relaunch must still happen.
    ///
    /// The run's agent is Codex, a capture-shape agent, persisted because
    /// `resume()` reloads its own `State`: the subject is not which launch path
    /// the relaunch takes.
    #[test]
    fn resume_without_a_cap_is_unchanged() {
        const NAME: &str = "pipeline_launch::tests::resume_without_a_cap_is_unchanged";
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

        let phase = PhaseId::new(68);
        // `Mode::Supervise` for the same 35.1-03 reason as the two tests
        // above — a Codex Code stage is refused in `Mode::Auto`, and the
        // absence of a `stop_until` cap is mode-independent.
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.stop_until = None;
        state.verification_baseline_captured = true;
        state.stopped = false;
        state.stop_reason = None;
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None);

        let _runners = crate::service::double::LaunchedRunners;

        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.stop_until, None,
            "no cap was ever set, so none must appear after resume"
        );
        assert!(!reloaded.stopped);
        assert_eq!(reloaded.stop_reason, None);
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
    }

    /// Run `name` in a hermetic child with a Codex stub on `PATH`, as the launch-timeout
    /// tests of `resume` run: in the parent, the child's stdout then stderr once it
    /// passed; in the child, `None`.
    fn run_resume_timeout_child(name: &str) -> Option<String> {
        if devflow_core::test_support::in_child_test(name) {
            return None;
        }
        let path_dir = agent_free_dir_with_agent_stub("codex");
        let output = crate::test_support::run_hermetic_test_in_child(name, path_dir.path(), &[]);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
        Some(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }

    /// A stopped-at-Code Codex run of `phase` saved in `root`, resumed with its launch
    /// ending as `end`.
    fn resume_with_launch_end(
        root: &Path,
        phase: PhaseId,
        end: crate::service::double::LaunchEnd,
    ) -> Result<(), CliError> {
        init_repo(root);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        crate::service::double::set_launch_end(end);
        resume(root, phase, None)
    }

    /// 999.244: a resume whose systemd-run timed out after the runner took its lock
    /// reports the run resumed, says systemd-run timed out, and leaves it active.
    #[test]
    fn resume_reports_a_timed_out_launch_with_a_live_runner_as_resumed() {
        const NAME: &str = "pipeline_launch::tests::resume_reports_a_timed_out_launch_with_a_live_runner_as_resumed";
        if let Some(printed) = run_resume_timeout_child(NAME) {
            assert!(
                printed.contains("systemd-run did not return in time"),
                "{printed}"
            );
            assert!(printed.contains("so the run is live"), "{printed}");
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(85);
        let result = resume_with_launch_end(
            dir.path(),
            phase,
            crate::service::double::LaunchEnd::TimedOutAfterStart,
        );
        let _runners = crate::service::double::LaunchedRunners;
        result.expect("a timed-out launch whose runner is live is a resumed run");
        assert!(
            !workflow::load_state(dir.path(), phase).unwrap().stopped,
            "the live run is not re-marked stopped"
        );
    }

    /// 999.244: a resume whose systemd-run timed out with no runner and an inactive unit
    /// re-marks the run stopped, as a failed launch does.
    #[test]
    fn resume_re_marks_stopped_a_timed_out_launch_whose_runner_is_not_live() {
        const NAME: &str = "pipeline_launch::tests::\
             resume_re_marks_stopped_a_timed_out_launch_whose_runner_is_not_live";
        if run_resume_timeout_child(NAME).is_some() {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(86);
        let error = resume_with_launch_end(
            dir.path(),
            phase,
            crate::service::double::LaunchEnd::TimedOutLeaving(Ok("inactive")),
        )
        .expect_err("a launch with no live runner fails")
        .to_string();
        assert!(
            error.contains("resume: phase 86 was not launched"),
            "{error}"
        );
        let state = workflow::load_state(dir.path(), phase).unwrap();
        assert!(state.stopped, "the run is re-marked stopped");
        assert!(
            state
                .stop_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("resume launch failed")),
            "{:?}",
            state.stop_reason
        );
    }

    /// 999.244: a resume whose systemd-run timed out while the unit is still activating
    /// leaves the run active and names `status` and `discard`: the runner may yet start.
    #[test]
    fn resume_keeps_the_state_of_a_timed_out_launch_whose_unit_is_activating() {
        const NAME: &str = "pipeline_launch::tests::\
             resume_keeps_the_state_of_a_timed_out_launch_whose_unit_is_activating";
        if run_resume_timeout_child(NAME).is_some() {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(87);
        let error = resume_with_launch_end(
            dir.path(),
            phase,
            crate::service::double::LaunchEnd::TimedOutLeaving(Ok("activating")),
        )
        .expect_err("an unconfirmed launch is not reported as resumed")
        .to_string();
        assert!(error.contains("is activating"), "{error}");
        assert!(error.contains("its state is kept"), "{error}");
        assert!(error.contains("`devflow status`"), "{error}");
        assert!(error.contains("`devflow discard --phase 87`"), "{error}");
        assert!(!error.contains("was not launched"), "{error}");
        assert!(
            !workflow::load_state(dir.path(), phase).unwrap().stopped,
            "a run that may be live is not re-marked stopped"
        );
    }

    /// 999.244 control: a resume whose systemd-run failed outright re-marks the run
    /// stopped, without reading the runner.
    #[test]
    fn resume_re_marks_stopped_a_launch_that_failed_outright() {
        const NAME: &str =
            "pipeline_launch::tests::resume_re_marks_stopped_a_launch_that_failed_outright";
        if run_resume_timeout_child(NAME).is_some() {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(88);
        let error =
            resume_with_launch_end(dir.path(), phase, crate::service::double::LaunchEnd::Failed)
                .expect_err("a failed launch fails")
                .to_string();
        assert!(
            error.contains("resume: phase 88 was not launched"),
            "{error}"
        );
        assert!(
            error.contains("systemd-run could not start unit"),
            "{error}"
        );
        assert!(
            workflow::load_state(dir.path(), phase).unwrap().stopped,
            "the run is re-marked stopped"
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
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, Some(AgentKind::Codex));
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Codex);
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
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
        let phase = PhaseId::new(75);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let result = resume(root, phase, None);
        result.unwrap();
        assert_eq!(
            workflow::load_state(root, phase).unwrap().agent,
            AgentKind::Codex
        );
    }

    #[test]
    fn resume_with_agent_refuses_before_touching_state_when_target_cannot_run_the_stage() {
        const NAME: &str = "pipeline_launch::tests::\
             resume_with_agent_refuses_before_touching_state_when_target_cannot_run_the_stage";
        enter_agent_free_child!(NAME, "codex");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(76);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.verification_baseline_captured = true;
        workflow::save_state(&state).unwrap();
        let state_path = workflow::state_path(root, phase);
        let before = std::fs::read(&state_path).unwrap();

        let err = resume(root, phase, Some(AgentKind::Codex)).unwrap_err();

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
        const NAME: &str = "pipeline_launch::tests::\
             resume_with_agent_refuses_auto_mode_handoff_that_would_fail_the_later_unattended_launch_check";
        enter_agent_free_child!(NAME, "codex");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(81);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let state_path = workflow::state_path(root, phase);
        let before = std::fs::read(&state_path).unwrap();

        let err = resume(root, phase, Some(AgentKind::Codex)).unwrap_err();

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
        let phase = PhaseId::new(77);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let result = resume(root, phase, Some(AgentKind::Codex));
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

    /// A handoff at Plan is allowed in supervise mode: Plan is the first stage, and its
    /// artifact and unattended checks apply in auto mode only. The auto-mode refusal at Plan
    /// is `resume_with_agent_refuses_before_touching_state_when_target_cannot_run_the_stage`.
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
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.stage = Stage::Plan;
        workflow::save_state(&state).unwrap();
        let result = resume(root, phase, Some(AgentKind::Codex));
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
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.consecutive_failures = 3;
        state.infra_failures = 2;
        workflow::save_state(&state).unwrap();
        let mut before = serde_json::to_value(&state).unwrap();
        let result = resume(root, phase, Some(AgentKind::Codex));
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();
        let mut after = serde_json::to_value(workflow::load_state(root, phase).unwrap()).unwrap();

        // D-01: the relaunch is the runner's unit; the state records no launch.
        assert!(
            after.get("supervisor").is_none(),
            "resume recorded a supervisor: {after}"
        );
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
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
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.stopped = true;
        state.stop_reason = Some("rate limited".to_string());
        workflow::save_state(&state).unwrap();
        let result = resume(root, phase, Some(AgentKind::Codex));
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Codex);
        assert!(!reloaded.stopped);
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
    }

    /// Run the test `name` in a hermetic child whose `PATH` holds `claude` and `codex`
    /// stubs, with `extra_env` set on the child only, returning `true` in the parent,
    /// whose work is done, and `false` in the child.
    fn parent_of_a_claude_and_codex_child(
        name: &str,
        extra_env: &[(&str, &std::ffi::OsStr)],
    ) -> bool {
        if devflow_core::test_support::in_child_test(name) {
            return false;
        }
        let path_dir = agent_free_dir_with_agent_stub("claude");
        add_agent_stub(path_dir.path(), "codex");
        let output =
            crate::test_support::run_hermetic_test_in_child(name, path_dir.path(), extra_env);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
        true
    }

    /// A stopped Code-stage run of `phase` under Claude, in a fresh repo at `root`, with
    /// `fallback` saved as its fallback and `consumed` as its spent mark. Its delivery
    /// canary is recorded `Confirmed`, so a stub `claude` relaunches on the stream shape
    /// without running one.
    fn save_stopped_code_run(
        root: &Path,
        phase: PhaseId,
        fallback: Option<AgentKind>,
        consumed: bool,
    ) -> State {
        init_repo(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.stopped = true;
        state.stop_reason = Some("rate limited".to_string());
        state.fallback_agent = fallback;
        state.fallback_consumed = consumed;
        // A stub `claude` cannot answer the stream launch's canary; a recorded outcome
        // is reused, never re-run.
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        workflow::save_state(&state).unwrap();
        state
    }

    /// D-01 (re-arm, stopped): `resume --fallback-agent` on a stopped run sets the
    /// fallback and clears a spent mark.
    #[test]
    fn resume_with_a_fallback_agent_rearms_a_consumed_fallback() {
        const NAME: &str =
            "pipeline_launch::tests::resume_with_a_fallback_agent_rearms_a_consumed_fallback";
        if parent_of_a_claude_and_codex_child(NAME, &[]) {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5144);
        save_stopped_code_run(root, phase, Some(AgentKind::Codex), true);

        let result = resume_with_fallback(root, phase, None, Some(AgentKind::Codex), false);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(!reloaded.fallback_consumed, "the spent mark is cleared");
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert!(!reloaded.stopped);
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
    }

    /// D-01 (re-arm, supervisor gone): a run whose supervisor exited without
    /// stopping it is not stopped. `resume --fallback-agent` re-arms it all the same.
    #[test]
    fn resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_run_whose_supervisor_exited() {
        const NAME: &str = "pipeline_launch::tests::\
                            resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_run_whose_supervisor_exited";
        if parent_of_a_claude_and_codex_child(NAME, &[]) {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5148);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.fallback_agent = Some(AgentKind::Codex);
        state.fallback_consumed = true;
        // A stub `claude` cannot answer the stream launch's canary; a recorded outcome
        // is reused, never re-run.
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        workflow::save_state(&state).unwrap();

        let paused = workflow::load_state(root, phase).unwrap();
        assert!(!paused.stopped, "the run is not stopped");

        let result = resume_with_fallback(root, phase, None, Some(AgentKind::Codex), false);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(!reloaded.fallback_consumed, "the spent mark is cleared");
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "the run relaunched as the runner unit"
        );
    }

    /// D-05: a plain `resume` leaves a spent fallback spent.
    #[test]
    fn resume_without_a_fallback_flag_keeps_a_consumed_fallback_spent() {
        const NAME: &str = "pipeline_launch::tests::resume_without_a_fallback_flag_keeps_a_consumed_fallback_spent";
        if parent_of_a_claude_and_codex_child(NAME, &[]) {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5145);
        save_stopped_code_run(root, phase, Some(AgentKind::Codex), true);

        let result = resume(root, phase, None);
        let _runners = crate::service::double::LaunchedRunners;
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(reloaded.fallback_consumed, "a plain resume does not re-arm");
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert_eq!(
            crate::service::double::launched_units().len(),
            1,
            "resume launched the runner unit"
        );
    }

    /// The notify command a refusal test saves, and the different one its child's
    /// environment carries: a refused resume must leave the saved one in place.
    const SAVED_NOTIFY: &str = "old";
    const ENV_NOTIFY: &str = "new";

    /// D-02 through `resume --agent`: handing the run to its own unused fallback is
    /// refused before any write, the notify command included (review B-3).
    #[test]
    fn resume_refuses_an_agent_equal_to_the_saved_unused_fallback() {
        const NAME: &str =
            "pipeline_launch::tests::resume_refuses_an_agent_equal_to_the_saved_unused_fallback";
        if parent_of_a_claude_and_codex_child(
            NAME,
            &[(
                devflow_core::gates::GATE_NOTIFY_CMD_ENV,
                std::ffi::OsStr::new(ENV_NOTIFY),
            )],
        ) {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5146);
        let mut state = save_stopped_code_run(root, phase, Some(AgentKind::Codex), false);
        state.gate_notify_cmd = Some(SAVED_NOTIFY.to_string());
        workflow::save_state(&state).unwrap();

        let result = resume_with_fallback(root, phase, Some(AgentKind::Codex), None, false);
        let reloaded = workflow::load_state(root, phase).unwrap();
        let err = result
            .expect_err("handing the run to its own unused fallback is refused")
            .to_string();

        assert!(
            err.contains("--agent codex"),
            "names the requested agent: {err}"
        );
        assert!(
            err.contains("saved unused fallback (codex)"),
            "names the saved fallback: {err}"
        );
        assert_eq!(reloaded.gate_notify_cmd.as_deref(), Some(SAVED_NOTIFY));
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert!(!reloaded.fallback_consumed);
        assert!(reloaded.stopped);
    }

    /// D-02 through `resume --fallback-agent`: a fallback equal to the run's agent is
    /// refused before any write, the notify command included (review B-3).
    #[test]
    fn resume_refuses_a_fallback_equal_to_the_run_agent_before_any_write() {
        const NAME: &str = "pipeline_launch::tests::\
                            resume_refuses_a_fallback_equal_to_the_run_agent_before_any_write";
        if parent_of_a_claude_and_codex_child(
            NAME,
            &[(
                devflow_core::gates::GATE_NOTIFY_CMD_ENV,
                std::ffi::OsStr::new(ENV_NOTIFY),
            )],
        ) {
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5147);
        let mut state = save_stopped_code_run(root, phase, None, false);
        state.gate_notify_cmd = Some(SAVED_NOTIFY.to_string());
        workflow::save_state(&state).unwrap();

        let result = resume_with_fallback(root, phase, None, Some(AgentKind::Claude), false);
        let reloaded = workflow::load_state(root, phase).unwrap();
        let err = result
            .expect_err("a fallback equal to the run's agent is refused")
            .to_string();

        assert!(
            err.contains("--fallback-agent claude"),
            "names the fallback: {err}"
        );
        assert!(
            err.contains("run's own agent (claude)"),
            "names the agent: {err}"
        );
        assert_eq!(reloaded.gate_notify_cmd.as_deref(), Some(SAVED_NOTIFY));
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert_eq!(reloaded.fallback_agent, None);
        assert!(!reloaded.fallback_consumed);
        assert!(reloaded.stopped);
    }

    /// Regression for 44-CORE-REVIEW-FINDINGS.md finding 2a: `resume` clears
    /// and persists `stopped`/`stop_reason`/`stop_until` BEFORE calling
    /// `launch_stage` (needed so a reload mid-relaunch already sees the
    /// phase as active — D-15/999.60). If `launch_stage` then fails outright
    /// — here, the agent binary is simply missing — that save must not be
    /// left standing: a state file claiming `stopped: false` with nothing
    /// actually running reads as a run that never stopped. `resume` must
    /// re-mark the phase `stopped` (with a reason) on this failure path, not
    /// leave it looking falsely active.
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
            let err = resume(&root, phase, None).unwrap_err();
            assert!(err.to_string().contains("not found"), "{err}");
            let reloaded = workflow::load_state(&root, phase).unwrap();
            assert!(
                reloaded.stopped,
                "a failed resume must leave the phase marked stopped, not a false-active zombie"
            );
            assert!(reloaded.stop_reason.is_some_and(|r| r.contains("claude")));
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
        state.verification_baseline_captured = true;
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

        std::thread::scope(|scope| {
            scope.spawn(|| {
                advance_for_stage(root, phase, Some(Stage::Code)).unwrap();
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

            answer_open_gate(
                root,
                phase,
                Stage::Code,
                &serde_json::from_str(
                    r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
            )
            .unwrap();
        });
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

        preanswer_gate(
            root,
            phase,
            Stage::Code,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

        advance_for_stage(root, phase, Some(Stage::Code)).unwrap();

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

        let err = advance_with(root, phase, Some(Stage::Plan))
            .expect_err("advance must reject a stage that has moved on");
        assert!(err.to_string().contains("stage mismatch"));
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        let failed = events_of_kind(root, "advance_failed");
        assert_eq!(failed.len(), 1, "one durable refusal event: {failed:?}");
        assert_eq!(failed[0]["reason"], "stage mismatch");
        assert_eq!(failed[0]["expected"], "plan");
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

        advance_with(root, phase, None).unwrap();
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

        advance_with(root, phase, Some(Stage::Code)).unwrap();
        assert!(events_of_kind(root, "advance_failed").is_empty());
        assert!(events_of_kind(root, "advance_stage_unbound").is_empty());
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
    /// **The false-branch discriminator is the agent, NOT stage membership**
    /// (ROADMAP criterion 7): a capture-shape agent (Codex) at a stage that is on
    /// the stream path for Claude. A stage-membership premise would be destroyed by
    /// the rollout [`STREAM_JSON_STAGES`] sequences; the agent term survives full
    /// widening. Phase 52 removed the Claude legacy opt-out this test used before.
    #[test]
    fn canary_gate_only_applies_to_the_stream_launch_path() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(121);
        let mut state = canary_state(root, phase);
        // `Stage::Code` is on the stream path today and stays on it. The
        // variable under test is the agent, not the stage.
        state.stage = Stage::Code;
        state.agent = AgentKind::Codex;

        // Driven by the REAL predicate rather than a hardcoded `false`, so this
        // test tracks the rollout instead of a copy of it.
        let stream_launch = stream_launch_enabled(state.agent, state.stage);
        assert!(
            !stream_launch,
            "a capture-shape agent must be off the stream path for this test to mean anything"
        );
        // Negative control: the SAME stage under Claude DOES fire. Without this,
        // the reading above would be a constant rather than a discrimination — and
        // unlike a stage-membership control, this one cannot be invalidated by
        // widening.
        assert!(
            stream_launch_enabled(AgentKind::Claude, state.stage),
            "the same stage under Claude must be on the stream path, or the check above \
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
            "a capture launch must not spend an agent invocation on a premise it never relies on"
        );
        assert_eq!(
            state.canary, None,
            "a launch that never ran the guard must not record an outcome for it"
        );
    }

    /// The other direction of the pair above: under Claude, the same fixture
    /// DOES run the guard and DOES record its outcome.
    ///
    /// The refusal half alone cannot distinguish "the gate correctly skipped a
    /// capture launch" from "the gate is wired to nothing". This is the case
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

        let stream_launch = stream_launch_enabled(state.agent, state.stage);
        assert!(
            stream_launch,
            "under Claude this stage must be on the stream path, or this test is \
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

        let reloaded = workflow::load_state(root, phase).unwrap();
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

    /// `recorded_canary_refusal` re-reads `canary_gate`'s decision without running a
    /// canary, so the two must agree on every recorded outcome: it is `Some` exactly when
    /// `canary_gate`, given the stream predicate `launch_stage_inner` computes for the
    /// same state, refuses. Enumerates every agent, stage and recorded outcome.
    /// With nothing recorded `canary_gate` runs a fresh canary, so the copy
    /// reports no recorded refusal; that case is asserted on its own. Control: the
    /// enumeration contains both an admitted and a refused launch, so agreement is not
    /// "both always say the same thing".
    #[test]
    fn recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let base = canary_state(root, PhaseId::new(126));

        let agents = [
            AgentKind::Claude,
            AgentKind::Codex,
            AgentKind::OpenCode,
            AgentKind::Pi,
            AgentKind::Antigravity,
        ];
        let stages = [Stage::Plan, Stage::Code, Stage::Validate];
        let outcomes = [
            canary::CanaryOutcome::Confirmed,
            canary::CanaryOutcome::Absent,
            canary::CanaryOutcome::Unverified("no binary".to_string()),
        ];
        let (mut admitted, mut refused) = (0usize, 0usize);
        for agent in agents {
            for stage in stages {
                let mut state = base.clone();
                state.agent = agent;
                state.stage = stage;
                state.canary = None;
                assert_eq!(
                    recorded_canary_refusal(&state),
                    None,
                    "nothing recorded: {agent} {stage}"
                );
                for outcome in &outcomes {
                    state.canary = Some(outcome.clone());
                    let stream = stream_launch_enabled(agent, stage);
                    let gate_refuses = canary_gate(&mut state.clone(), stream, || {
                        unreachable!("a recorded outcome is never re-run")
                    })
                    .is_err();
                    assert_eq!(
                        recorded_canary_refusal(&state).is_some(),
                        gate_refuses,
                        "{agent} {stage} {outcome:?}"
                    );
                    if gate_refuses {
                        refused += 1;
                    } else {
                        admitted += 1;
                    }
                }
            }
        }
        assert!(
            admitted > 0 && refused > 0,
            "control: admitted {admitted}, refused {refused}"
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

        let message = result
            .expect_err("a launch whose canary cannot confirm must not proceed")
            .to_string();
        assert!(
            message.contains("ABSENT"),
            "the launch path must surface the guard's own diagnosis, got: {message}"
        );
        assert_eq!(state.canary, Some(canary::CanaryOutcome::Absent));
        assert_eq!(
            stage_launched_count(root, phase),
            0,
            "a refused launch must emit no stage_launched event — nothing was launched"
        );
    }

    // The gate value is assembled from a const/format! rather than a bare
    // source literal (28-01's precedent) so this file itself never contains
    // the literal `gate="blocking-human"`.
    const HUMAN_GATE_VALUE_FOR_TEST: &str = "blocking-human";

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
        preanswer_gate(
            root,
            phase,
            stage,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );
    }

    /// A response in one of the three shapes `write_gate_response` documents.
    fn gate_response(approved: bool, note: Option<&str>) -> devflow_core::gates::GateResponse {
        devflow_core::gates::GateResponse {
            approved,
            abort: false,
            note: note.map(str::to_string),
            responded_by: Some("test".to_string()),
        }
    }

    /// An abort answer, the shape `devflow gate abort` writes.
    fn abort_response(note: &str) -> devflow_core::gates::GateResponse {
        devflow_core::gates::GateResponse {
            abort: true,
            ..gate_response(false, Some(note))
        }
    }

    /// Pre-answer the next gate at `stage` so its wait returns immediately.
    ///
    /// `GateAction::from_response` maps the three shapes by their fields:
    /// `abort` -> `Abort`; approved -> `Advance`; anything else -> `LoopBack`.
    fn preanswer_gate_response(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        response: &devflow_core::gates::GateResponse,
    ) {
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "preanswer_gate_response must run inside a child test with an agent-free PATH (999.80)"
        );
        preanswer_gate(root, phase, stage, response);
    }

    /// Run `handler` on another thread until it opens `stage`'s gate, read the
    /// gate file's `gate_kind`, answer the gate with an abort and wait for
    /// `handler`.
    fn gate_kind_at_gate(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        handler: impl FnOnce() + Send,
    ) -> serde_json::Value {
        const GATE_OPENS_WITHIN: Duration = Duration::from_secs(30);
        let gate_path = Gates::gate_path(root, phase, stage);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(handler);
            let started = std::time::Instant::now();
            while !gate_path.exists() {
                assert!(
                    !waiter.is_finished(),
                    "the handler returned without opening the {stage} gate"
                );
                assert!(
                    started.elapsed() < GATE_OPENS_WITHIN,
                    "the {stage} gate did not open within {GATE_OPENS_WITHIN:?}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            let gate: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&gate_path).unwrap()).unwrap();
            answer_open_gate(root, phase, stage, &abort_response("test cleanup")).unwrap();
            waiter.join().unwrap();
            gate
        })
    }

    /// D-39: no gate is kindless. A stage failure at Plan and at Code and an
    /// infra failure each record `stage_failure`, and a failed preflight check
    /// `preflight`. Control in the same test: a stage-failure gate at Validate
    /// and a Validate disagreement gate both have stage Validate and record
    /// different kinds.
    #[test]
    fn each_status_less_gate_records_its_kind() {
        const NAME: &str = "pipeline_launch::tests::each_status_less_gate_records_its_kind";
        enter_agent_free_child!(NAME, "claude");
        let at = |phase: u32, stage: Stage| {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(phase);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, dir.path().into());
            state.stage = stage;
            workflow::save_state(&state).unwrap();
            (dir, phase, state)
        };

        let mut stage_failures = Vec::new();
        for (number, stage) in [
            (5261, Stage::Plan),
            (5262, Stage::Code),
            (5263, Stage::Validate),
        ] {
            let (dir, phase, mut state) = at(number, stage);
            let gate = gate_kind_at_gate(dir.path(), phase, stage, || {
                let _ = crate::pipeline_outcomes::handle_stage_failure(
                    dir.path(),
                    &mut state,
                    stage,
                    Some("the launch failed".into()),
                );
            });
            assert_eq!(gate["gate_kind"], "stage_failure", "{stage}: {gate}");
            stage_failures.push(gate);
        }

        let (dir, phase, mut state) = at(5264, Stage::Code);
        let gate = gate_kind_at_gate(dir.path(), phase, Stage::Code, || {
            let _ = crate::pipeline_outcomes::handle_infra_outcome(
                dir.path(),
                &mut state,
                Stage::Code,
                Some("the agent was killed".into()),
            );
        });
        assert_eq!(gate["gate_kind"], "stage_failure", "infra: {gate}");

        let (dir, phase, mut state) = at(5265, Stage::Code);
        let gate = gate_kind_at_gate(dir.path(), phase, Stage::Code, || {
            let _ = crate::preflight::run_preflight(dir.path(), &mut state, &AlwaysFailAdapter);
        });
        assert_eq!(gate["gate_kind"], "preflight", "preflight: {gate}");

        // Control: two gates at Validate, one a stage failure, one a disagreement.
        let (dir, phase, mut state) = at(5266, Stage::Validate);
        let _gsd = crate::test_support::validate_evidence(
            dir.path(),
            phase,
            &[crate::test_support::gsd_says(
                devflow_core::gsd_reader::GsdStatus::Passed,
            )],
        );
        let disagreement = gate_kind_at_gate(dir.path(), phase, Stage::Validate, || {
            let _ = crate::pipeline_outcomes::handle_validate_outcome(
                dir.path(),
                &mut state,
                crate::pipeline_outcomes::ValidateOutcome::Failed,
            );
        });
        let stage_failure = &stage_failures[2];
        assert_eq!(stage_failure["stage"], "validate");
        assert_eq!(disagreement["stage"], "validate");
        assert_eq!(
            disagreement["gate_kind"], "disagreement_over_passed",
            "{disagreement}"
        );
        assert_ne!(stage_failure["gate_kind"], disagreement["gate_kind"]);
    }

    // ---- D-13 fallback: a declared checkpoint opens the failure gate (53-03) ----

    /// What one failed stage left after [`advance_failed_stage`] ran its advance.
    struct FailedStageAdvance {
        dir: tempfile::TempDir,
        launch: Option<StageLaunch>,
        launches: Vec<(Stage, AgentKind)>,
        /// The run's state while its gate was open, before the abort answer
        /// cleared it.
        state_at_gate: State,
    }

    impl FailedStageAdvance {
        /// The `gate_fired` events this advance recorded for `stage`.
        fn gates_at(&self, stage: Stage) -> Vec<serde_json::Value> {
            events_of_kind(self.dir.path(), "gate_fired")
                .into_iter()
                .filter(|event| event["stage"] == stage.to_string())
                .collect()
        }

        /// The `stage_launched` events on record for `stage`.
        fn launches_recorded_at(&self, stage: Stage) -> usize {
            events_of_kind(self.dir.path(), "stage_launched")
                .iter()
                .filter(|event| event["stage"] == stage.to_string())
                .count()
        }
    }

    /// Advance a fresh `agent` run whose `stage` just exited failed, with or
    /// without the final marker's `checkpoint: blocking-human` declaration.
    /// The phase's plans declare that checkpoint and a session id is on record,
    /// so the removed relaunch had every input it ever read. The runner's
    /// `stage_launched` for the stage is on record. The gate the advance opens is
    /// read with the run's state, then answered with an abort. Validate starts one
    /// failure below the ceiling, so a failure the fix loop counts gates instead
    /// of relaunching Code.
    fn advance_failed_stage(
        agent: AgentKind,
        stage: Stage,
        declared: bool,
        phase: PhaseId,
    ) -> FailedStageAdvance {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_declared_checkpoint_plan(root, phase);
        if declared {
            write_marker_declared_checkpoint_capture(root, phase);
        } else {
            write_unreported_failure_capture(root, phase);
        }
        let mut state = State::new(phase, agent, Mode::Auto, root.to_path_buf());
        state.stage = stage;
        state.session_id = Some("sess-declared-checkpoint".to_string());
        if stage == Stage::Validate {
            state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
            state.last_validate_failure_commit_count = Some(0);
        }
        workflow::save_state(&state).unwrap();
        events::emit(
            root,
            phase,
            "stage_launched",
            serde_json::json!({"stage": stage.to_string(), "agent": agent.to_string()}),
        );

        const GATE_OPENS_WITHIN: Duration = Duration::from_secs(30);
        let gate_path = Gates::gate_path(root, phase, stage);
        let (launch, launches, state_at_gate) = std::thread::scope(|scope| {
            let waiter = scope.spawn(|| {
                LAUNCH_PROBE.set(Some(Vec::new()));
                let launch = advance_for_stage(root, phase, Some(stage)).unwrap();
                (launch, LAUNCH_PROBE.take().expect("the probe was set"))
            });
            let started = std::time::Instant::now();
            while !gate_path.exists() {
                assert!(
                    !waiter.is_finished(),
                    "the advance returned without opening the {stage} gate"
                );
                assert!(
                    started.elapsed() < GATE_OPENS_WITHIN,
                    "the {stage} gate did not open within {GATE_OPENS_WITHIN:?}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            let state_at_gate = workflow::load_state(root, phase).unwrap();
            answer_open_gate(root, phase, stage, &abort_response("test cleanup")).unwrap();
            let (launch, launches) = waiter.join().unwrap();
            (launch, launches, state_at_gate)
        });
        FailedStageAdvance {
            dir,
            launch,
            launches,
            state_at_gate,
        }
    }

    const UNASKED_CHECKPOINT_TEXT: &str = "could not ask through devflow_ask";

    /// D-13: a Code stage that declares a blocking-human checkpoint on its final
    /// result opens the never-silent stage failure gate naming it, for Claude
    /// and Codex alike, and nothing relaunches the session.
    #[test]
    fn a_declared_checkpoint_opens_the_failure_gate_without_a_relaunch() {
        const NAME: &str = "pipeline_launch::tests::a_declared_checkpoint_opens_the_failure_gate_without_a_relaunch";
        enter_agent_free_child!(NAME, "claude");
        for (number, agent) in [(5301, AgentKind::Claude), (5302, AgentKind::Codex)] {
            let run = advance_failed_stage(agent, Stage::Code, true, PhaseId::new(number));
            let gates = run.gates_at(Stage::Code);
            assert_eq!(gates.len(), 1, "{agent}: one gate at Code: {gates:?}");
            assert_eq!(gates[0]["gate_kind"], "stage_failure", "{agent}: {gates:?}");
            let context = gates[0]["context"].as_str().expect("the gate's context");
            assert!(
                context.contains(UNASKED_CHECKPOINT_TEXT),
                "{agent}: the failure gate must name the unasked checkpoint: {context}"
            );
            assert!(
                run.launch.is_none(),
                "{agent}: the advance handed back a launch"
            );
            assert!(
                run.launches.is_empty(),
                "{agent}: relaunched {:?}",
                run.launches
            );
            assert_eq!(
                run.launches_recorded_at(Stage::Code),
                1,
                "{agent}: exactly the one stage_launched the runner recorded"
            );
        }
    }

    /// Control for the D-13 sentence: the same Code failure with no declaration
    /// opens the same gate kind, and its context does not name a checkpoint.
    #[test]
    fn a_failure_without_a_declaration_has_no_checkpoint_sentence() {
        const NAME: &str =
            "pipeline_launch::tests::a_failure_without_a_declaration_has_no_checkpoint_sentence";
        enter_agent_free_child!(NAME, "claude");
        let run = advance_failed_stage(AgentKind::Claude, Stage::Code, false, PhaseId::new(5303));
        let gates = run.gates_at(Stage::Code);
        assert_eq!(gates.len(), 1, "one gate at Code: {gates:?}");
        assert_eq!(gates[0]["gate_kind"], "stage_failure", "{gates:?}");
        let context = gates[0]["context"].as_str().expect("the gate's context");
        assert!(
            !context.contains(UNASKED_CHECKPOINT_TEXT),
            "an undeclared failure must not name a checkpoint: {context}"
        );
        assert!(run.launch.is_none() && run.launches.is_empty());
    }

    /// Review concern 12: a Validate failure that declares the checkpoint opens
    /// the stage failure gate, not the Code↔Validate fix loop, which would
    /// relaunch Code on a question the operator never saw and count it as a
    /// validation failure. Control in the same test: the same Validate failure
    /// without the declaration still takes the fix loop, which reads GSD's
    /// verification, counts the failure and gates at the ceiling.
    #[test]
    fn a_declared_checkpoint_at_validate_opens_the_failure_gate_not_the_fix_loop() {
        const NAME: &str = "pipeline_launch::tests::a_declared_checkpoint_at_validate_opens_the_failure_gate_not_the_fix_loop";
        enter_agent_free_child!(NAME, "claude");
        let declared =
            advance_failed_stage(AgentKind::Claude, Stage::Validate, true, PhaseId::new(5304));
        let gates = declared.gates_at(Stage::Validate);
        assert_eq!(gates.len(), 1, "one gate at Validate: {gates:?}");
        assert_eq!(gates[0]["gate_kind"], "stage_failure", "{gates:?}");
        let context = gates[0]["context"].as_str().expect("the gate's context");
        assert!(context.contains(UNASKED_CHECKPOINT_TEXT), "{context}");
        let state = &declared.state_at_gate;
        assert_eq!(state.stage, Stage::Validate);
        assert_eq!(
            state.consecutive_failures,
            mode::MAX_CONSECUTIVE_FAILURES - 1,
            "a declared checkpoint is not a validation failure"
        );
        assert!(
            events_of_kind(declared.dir.path(), "gsd_verification").is_empty(),
            "the fix loop's verification read must not run"
        );
        assert!(declared.launch.is_none() && declared.launches.is_empty());
        assert_eq!(declared.launches_recorded_at(Stage::Validate), 1);
        assert_eq!(declared.launches_recorded_at(Stage::Code), 0);

        let undeclared = advance_failed_stage(
            AgentKind::Claude,
            Stage::Validate,
            false,
            PhaseId::new(5305),
        );
        let gates = undeclared.gates_at(Stage::Validate);
        assert_eq!(gates.len(), 1, "control: one gate at Validate: {gates:?}");
        assert_ne!(gates[0]["gate_kind"], "stage_failure", "control: {gates:?}");
        let state = &undeclared.state_at_gate;
        assert_eq!(
            state.consecutive_failures,
            mode::MAX_CONSECUTIVE_FAILURES,
            "control: the fix loop counts the failure"
        );
        assert_eq!(
            events_of_kind(undeclared.dir.path(), "gsd_verification").len(),
            1,
            "control: the fix loop reads GSD's verification"
        );
    }

    // ---- launch-shape selection (D-11's opt-out was removed in Phase 52) ----

    /// Phase 39 Stage 1 regression: Pi always resolves to `StageShape::Capture`.
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
            !stream_launch_enabled(state.agent, state.stage),
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
        assert!(matches!(launch, StageShape::Capture));
        assert_eq!(program, "pi");
        assert_eq!(args, vec!["-p", "--no-approve", "the stage prompt"]);
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
    /// it covers *parse-failure-driven* fallback only, not every route that
    /// could select legacy.
    #[test]
    fn parse_failure_does_not_trigger_a_fallback() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(134);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
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
        let stream_launch = stream_launch_enabled(state.agent, state.stage);
        assert!(
            stream_launch,
            "a parse failure must not select the capture shape — D-11 rejects automatic fallback"
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
        assert!(matches!(launch, StageShape::Stream { .. }));
    }

    // ------------------------------------------------------------------
    // Phase 41 Task 3: the widened stream predicate (Antigravity joins
    // Claude, D-10), the PipeOwning
    // launch shape, the by-agent canary dispatch (B2/D-07), and the
    // AutoChainGuard comment correction.
    // ------------------------------------------------------------------

    /// Claude and Antigravity take the stream shape at every stream stage, with no
    /// opt-out; every other agent takes the capture shape (`StageShape::Capture`).
    #[test]
    fn stream_launch_includes_antigravity_on_stream_stages() {
        for stage in STREAM_JSON_STAGES.iter().copied() {
            for agent in [AgentKind::Claude, AgentKind::Antigravity] {
                assert!(
                    stream_launch_enabled(agent, stage),
                    "{agent:?} is a stream agent at {stage} (round-3 D-10)"
                );
            }
            for agent in [
                AgentKind::Codex,
                AgentKind::OpenCode,
                AgentKind::Pi,
                AgentKind::Hermes,
            ] {
                assert!(
                    !stream_launch_enabled(agent, stage),
                    "{agent:?} must stay off the stream path at {stage}"
                );
            }
        }
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
            matches!(launch, StageShape::Stream { .. }),
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
            stream_launch_enabled(AgentKind::Antigravity, Stage::Code),
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
            matches!(launch, StageShape::Stream { .. }),
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

    /// The refusal a process meets when another foreground process holds the phase
    /// (D-12): it names the holder's pid and that another verb is in progress, and never
    /// `resume` or `stop`, which refuses while the holder is held.
    fn assert_refused_by_the_holder(error: &str, phase: PhaseId) {
        assert!(
            error.contains(&format!("(pid {})", std::process::id())),
            "the refusal names the holder's pid: {error}"
        );
        assert!(
            error.contains(&format!("another DevFlow verb holds phase {phase}"))
                && error.contains("in progress"),
            "{error}"
        );
        assert!(!error.contains("devflow resume"), "{error}");
        assert!(!error.contains("devflow stop"), "{error}");
    }

    /// D-17 (V-31): `resume` refuses while another process holds the phase, before it
    /// reserves, so the state is untouched.
    #[test]
    fn resume_refuses_while_another_process_holds_the_phase() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2905);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
        state.stage = Stage::Plan;
        state.stopped = true;
        state.stop_reason = Some("stopped via `devflow stop`".to_string());
        workflow::save_state(&state).unwrap();
        let _hold = devflow_core::liveness::claim_phase_holder(root, phase).unwrap();
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();

        let error = resume(root, phase, None)
            .expect_err("a held phase refuses")
            .to_string();

        assert_refused_by_the_holder(&error, phase);
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "nothing is reserved"
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

    /// D-10 (#324): an advance stop's reason and its `advance_stopped` event name the
    /// runner, not the supervisor Phase 52.2 removed.
    #[test]
    fn an_advance_stop_names_the_runner_not_the_supervisor() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5324);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.into());
        workflow::save_state(&state).unwrap();
        record_advance_stop(&mut state, 2).unwrap();
        let reason = workflow::load_state(root, phase).unwrap().stop_reason;
        let source = events_named(root, "advance_stopped")
            .last()
            .map(|event| event["source"].clone());
        assert!(
            reason.as_deref() == Some(ADVANCE_STOP_REASON)
                && ADVANCE_STOP_REASON.contains("runner")
                && !ADVANCE_STOP_REASON.contains("supervisor")
                && source.as_ref().and_then(serde_json::Value::as_str) == Some("runner"),
            "the advance stop still names the supervisor: {reason:?} / {source:?}"
        );
    }

    // ---- 51-01: rate-limit failover at Code (SUPV-02) ----

    /// The capture-shape rate-limit envelope a rate-limited stub prints. Its
    /// retry time parses, so the no-fallback gate names it as the reset time
    /// (`pipeline_outcomes::tests::primary_loop_rate_limited_waits_at_the_code_gate`).
    const RATE_LIMIT_ENVELOPE: &str =
        r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2099-06-18T15:45:30Z"}"#;

    /// `git <args>` in `root`, trimmed stdout; panics on a failed git.
    fn git_out(root: &Path, args: &[&str]) -> String {
        let output = devflow_core::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed: {output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// R-2: the fallback is selected only at Code, and only while it is set,
    /// unused and different from the agent driving the run. Every `Stage` is
    /// named in the match, so a new stage is a compile error here rather than
    /// a case nobody decided.
    #[test]
    fn failover_target_selects_the_fallback_only_at_code() {
        let mut state = State::new(
            PhaseId::new(5100),
            AgentKind::Claude,
            Mode::Supervise,
            PathBuf::from("/repo"),
        );
        state.fallback_agent = Some(AgentKind::Codex);
        for stage in [Stage::Plan, Stage::Code, Stage::Validate] {
            let expected = match stage {
                Stage::Code => Some(AgentKind::Codex),
                Stage::Plan | Stage::Validate => None,
            };
            assert_eq!(
                failover_target(&state, stage),
                expected,
                "an unused fallback at {stage}"
            );
        }

        let mut consumed = state.clone();
        consumed.fallback_consumed = true;
        assert_eq!(
            failover_target(&consumed, Stage::Code),
            None,
            "a consumed fallback is never selected again"
        );
        let mut same = state.clone();
        same.fallback_agent = Some(AgentKind::Claude);
        assert_eq!(
            failover_target(&same, Stage::Code),
            None,
            "a fallback equal to the run's agent is no handoff"
        );
        let mut absent = state;
        absent.fallback_agent = None;
        assert_eq!(failover_target(&absent, Stage::Code), None, "no fallback");
    }

    /// Write the rate-limit envelope as the phase's capture, for an advance
    /// run with no agent.
    fn write_rate_limit_capture(root: &Path, phase: PhaseId) {
        let stdout = devflow_core::agent_result::stdout_path(root, phase);
        std::fs::create_dir_all(stdout.parent().unwrap()).unwrap();
        std::fs::write(&stdout, format!("{RATE_LIMIT_ENVELOPE}\n")).unwrap();
    }

    /// Assert a rate limit with no usable fallback waited at `phase`'s `stage`
    /// gate: one `gate_fired` there naming `devflow resume --phase N`, and no
    /// `rate_limit_resume_scheduled` event.
    fn assert_waited_at_the_gate(root: &Path, phase: PhaseId, stage: Stage) {
        let fired = events_named(root, "gate_fired");
        assert_eq!(fired.len(), 1, "one gate: {fired:?}");
        assert_eq!(fired[0]["stage"], stage.to_string(), "{}", fired[0]);
        assert!(
            fired[0]["context"]
                .as_str()
                .is_some_and(|context| context.contains(&format!("devflow resume --phase {phase}"))),
            "the gate names the resume command: {}",
            fired[0]
        );
        assert!(
            events_named(root, "rate_limit_resume_scheduled").is_empty(),
            "no resume is scheduled"
        );
    }

    /// D-07: a handoff refused at runtime — here the fallback's binary is
    /// missing — says so in `agent_handoff_refused`, naming the fallback and
    /// the refusal, then waits at the Code gate as a run with no fallback
    /// does. The fallback stays unconsumed, the agent unchanged and
    /// `infra_failures` untouched (D-20).
    ///
    /// Supervise mode: under Auto the candidate preflight refuses Codex at
    /// Code on the unattended launch shape before the binary check runs, and
    /// the reason would not name the missing binary.
    #[test]
    fn a_refused_handoff_waits_at_the_gate_and_names_the_refusal() {
        const NAME: &str =
            "pipeline_launch::tests::a_refused_handoff_waits_at_the_gate_and_names_the_refusal";
        enter_agent_free_child!(NAME, "claude");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5103);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.fallback_agent = Some(AgentKind::Codex);
        workflow::save_state(&state).unwrap();
        write_rate_limit_capture(root, phase);

        let reloaded = state_on_disk_at_gate(root, phase, Stage::Code, || {
            advance(root, Some(phase)).unwrap();
        });

        let refused = events_named(root, "agent_handoff_refused");
        assert_eq!(refused.len(), 1, "one refusal event: {refused:?}");
        let refusal = &refused[0];
        assert_eq!(refusal["stage"], "code", "{refusal}");
        assert_eq!(refusal["from_agent"], "claude", "{refusal}");
        assert_eq!(refusal["to_agent"], "codex", "{refusal}");
        assert!(
            refusal["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("not found")),
            "the refusal names the missing binary: {refusal}"
        );
        assert_waited_at_the_gate(root, phase, Stage::Code);
        assert!(
            events_named(root, "agent_handoff").is_empty(),
            "a refused handoff is not a handoff"
        );
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert!(
            !reloaded.fallback_consumed,
            "only a committed hop consumes it"
        );
        assert_eq!(
            reloaded.infra_failures, 0,
            "a rate limit is not an infra fault"
        );
    }

    /// R-2: a rate limit at Plan with an unused fallback configured waits at
    /// the Plan gate and leaves the fallback alone. A `codex` stub is on PATH, so
    /// the absence of a hop is not explained by a missing binary.
    ///
    /// Supervise mode, so that with the stage term removed from
    /// [`failover_target`] this run reaches a committed hop rather than a
    /// preflight refusal (the RED-equivalent this control was checked with).
    #[test]
    fn a_rate_limit_at_plan_with_a_fallback_waits_at_the_plan_gate() {
        const NAME: &str =
            "pipeline_launch::tests::a_rate_limit_at_plan_with_a_fallback_waits_at_the_plan_gate";
        enter_agent_free_child!(NAME, "codex");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5104);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Plan;
        state.fallback_agent = Some(AgentKind::Codex);
        workflow::save_state(&state).unwrap();
        write_rate_limit_capture(root, phase);

        let reloaded = state_on_disk_at_gate(root, phase, Stage::Plan, || {
            advance(root, Some(phase)).unwrap();
        });

        // The hop checks come first so that a hop at Plan is the failure
        // reported, not its knock-on missing gate.
        assert!(
            events_named(root, "agent_handoff").is_empty(),
            "no handoff at Plan: {:?}",
            events_named(root, "agent_handoff")
        );
        assert!(
            events_named(root, "agent_handoff_refused").is_empty(),
            "Plan never attempts a handoff"
        );
        assert_waited_at_the_gate(root, phase, Stage::Plan);
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert!(!reloaded.fallback_consumed);
    }

    // ---- 51-06: what crosses the hop, and its boundaries at Code ----

    /// Ignore `.devflow/` in the repo at `root` and commit that on the current
    /// branch, so the run's own state and capture files do not make the tree read
    /// dirty: what `git status --porcelain` then reports is the agent's work alone.
    fn ignore_devflow_dir(root: &Path) {
        std::fs::write(root.join(".gitignore"), ".devflow/\n").unwrap();
        git_out(root, &["add", ".gitignore"]);
        git_out(root, &["commit", "-q", "-m", "ignore .devflow"]);
    }

    /// A PATH dir holding a `claude` and a `codex` that each wait (bounded, about
    /// 60 s) for a file that never appears, and `sleep` for that wait. A stage
    /// relaunched onto either stays running, so its supervisor makes no further
    /// state write while the test reads the state.
    fn waiting_agents_dir() -> tempfile::TempDir {
        let path_dir = agent_free_dir_with_agent_stub("claude");
        for program in ["claude", "codex"] {
            let path = path_dir.path().join(program);
            devflow_core::test_support::write_executable(
                &path,
                "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
                 i=0\nwhile [ ! -e never-released ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
                 exit 0\n",
            );
        }
        let real_sleep = ["/usr/bin/sleep", "/bin/sleep"]
            .into_iter()
            .map(Path::new)
            .find(|path| path.is_file())
            .expect("sleep must exist to run this test");
        std::os::unix::fs::symlink(real_sleep, path_dir.path().join("sleep")).unwrap();
        path_dir
    }

    /// D-04: delivery-canary evidence is per agent. The handoff commit clears the
    /// recorded canary and session id, so a Claude fallback whose own canary reads
    /// `Absent` is refused, after exactly one canary run, instead of launching on
    /// the primary's `Confirmed`. Control first: with no handoff, the same recorded
    /// `Confirmed` admits a Claude launch without running a canary, so the refusal
    /// below is the handoff's doing. (A codex run handed to claude since operator
    /// answer (2) refuses antigravity, which has no `devflow_ask` registration.)
    #[test]
    fn a_handoff_reruns_the_delivery_canary_for_the_new_agent() {
        const NAME: &str =
            "pipeline_launch::tests::a_handoff_reruns_the_delivery_canary_for_the_new_agent";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5161);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.stage = Stage::Code;
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        state.session_id = Some("s1".to_string());
        workflow::save_state(&state).unwrap();

        let calls = std::cell::Cell::new(0usize);
        let mut without_handoff = state.clone();
        without_handoff.agent = AgentKind::Claude;
        canary_gate(
            &mut without_handoff,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Absent),
        )
        .expect("control: a recorded Confirmed admits the launch");
        assert_eq!(
            calls.get(),
            0,
            "control: a recorded outcome is reused, never re-run"
        );

        let outcome = hand_off_agent(
            root,
            &mut state,
            AgentKind::Claude,
            HandoffReason::RateLimited,
        )
        .unwrap();
        assert_eq!(
            outcome,
            HandoffOutcome::HandedOff {
                from: AgentKind::Codex
            }
        );
        let mut reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert_eq!(
            reloaded.canary, None,
            "the primary's canary evidence does not cross the hop"
        );
        assert_eq!(
            reloaded.session_id, None,
            "the primary's session id does not cross the hop"
        );

        let refused = canary_gate(
            &mut reloaded,
            true,
            counting_canary(&calls, canary::CanaryOutcome::Absent),
        );
        assert_eq!(calls.get(), 1, "the fallback's own canary ran once");
        let message = refused
            .expect_err("an Absent canary for the fallback refuses its launch")
            .to_string();
        assert!(message.contains("ABSENT"), "{message}");
    }

    /// D-10: the `agent_handoff` event records the tree it hands over, measured
    /// before the commit: `tree_dirty` from `git status --porcelain` at the launch
    /// root and `commit_count` from the phase branch. `.devflow/` is ignored, so
    /// the dirt is the untracked file alone, and the handoff leaves that file in
    /// place (nothing is reset, stashed or cleaned).
    #[test]
    fn the_handoff_event_records_the_tree_it_hands_over() {
        const NAME: &str =
            "pipeline_launch::tests::the_handoff_event_records_the_tree_it_hands_over";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        ignore_devflow_dir(root);
        let phase = PhaseId::new(5162);
        let git_flow = devflow_core::config::git_flow_for_run(root, None);
        let branch = format!("{}phase-{}", git_flow.feature_prefix, phase.padded());
        git_out(root, &["checkout", "-q", "-b", &branch]);
        git_out(root, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git_out(root, &["commit", "-q", "--allow-empty", "-m", "second"]);
        let untracked = root.join("untracked-work.txt");
        std::fs::write(&untracked, "half-done\n").unwrap();
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let outcome = hand_off_agent(
            root,
            &mut state,
            AgentKind::Codex,
            HandoffReason::RateLimited,
        )
        .unwrap();
        assert_eq!(
            outcome,
            HandoffOutcome::HandedOff {
                from: AgentKind::Claude
            }
        );

        let handoffs = events_named(root, "agent_handoff");
        assert_eq!(handoffs.len(), 1, "{handoffs:?}");
        assert_eq!(handoffs[0]["tree_dirty"], true, "{}", handoffs[0]);
        assert_eq!(handoffs[0]["commit_count"], 2, "{}", handoffs[0]);
        assert_eq!(
            std::fs::read_to_string(&untracked).unwrap(),
            "half-done\n",
            "the untracked file is handed over as it was"
        );
    }

    /// D-08: a Code rate limit with nothing committed on the phase branch still
    /// hands off, through a real `advance`, and the event says so: `commit_count:
    /// 0` on a clean tree. The fallback's `codex` waits, so its supervisor writes
    /// nothing more before the state is read.
    #[test]
    fn a_rate_limit_before_any_commit_still_hands_off() {
        const NAME: &str = "pipeline_launch::tests::a_rate_limit_before_any_commit_still_hands_off";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = waiting_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        ignore_devflow_dir(root);
        let phase = PhaseId::new(5163);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.fallback_agent = Some(AgentKind::Codex);
        workflow::save_state(&state).unwrap();
        write_rate_limit_capture(root, phase);

        let launch = advance(root, Some(phase)).unwrap();
        let reloaded = workflow::load_state(root, phase).unwrap();

        let handoffs = events_named(root, "agent_handoff");
        assert_eq!(handoffs.len(), 1, "one handoff: {handoffs:?}");
        let handoff = &handoffs[0];
        assert_eq!(handoff["stage"], "code", "{handoff}");
        assert_eq!(handoff["to_agent"], "codex", "{handoff}");
        assert_eq!(handoff["commit_count"], 0, "{handoff}");
        assert_eq!(handoff["tree_dirty"], false, "{handoff}");
        assert_eq!(reloaded.agent, AgentKind::Codex);
        assert!(reloaded.fallback_consumed);
        assert_eq!(
            launch.map(|launch| (launch.state.stage, launch.state.agent)),
            Some((Stage::Code, AgentKind::Codex)),
            "the fallback's relaunch is handed back"
        );
        assert!(
            events_named(root, "rate_limit_resume_scheduled").is_empty(),
            "a handed-off rate limit schedules no resume"
        );
    }

    /// A Supervise-mode Code run of `phase` under Claude in a fresh repo at `root`,
    /// with `fallback` and `consumed` saved, its `infra_failures` at `infra`, and a
    /// rate-limit capture for `advance` to read. Its delivery canary is recorded
    /// `Confirmed`, so a Claude relaunch runs none.
    fn save_rate_limited_code_run(
        root: &Path,
        phase: PhaseId,
        fallback: Option<AgentKind>,
        consumed: bool,
        infra: u32,
    ) {
        init_repo(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.fallback_agent = fallback;
        state.fallback_consumed = consumed;
        state.infra_failures = infra;
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        workflow::save_state(&state).unwrap();
        write_rate_limit_capture(root, phase);
    }

    /// D-06 at its boundary, at Code (R-2): a hop is not an infra fault. With
    /// `infra_failures` one below the ceiling, a rate limit with an unused
    /// fallback hands off and leaves the counter where it was. Control in the same
    /// test: the same state with no fallback does not hand off; it waits at the
    /// Code gate, with the counter also where it was (D-20).
    ///
    /// Supervise mode with a Codex fallback, a pair the Code preflight admits;
    /// `gate_or_abort_infra` aborts at the ceiling in every mode.
    #[test]
    fn an_unused_fallback_hands_off_one_below_the_infra_ceiling() {
        const NAME: &str =
            "pipeline_launch::tests::an_unused_fallback_hands_off_one_below_the_infra_ceiling";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = waiting_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let below_ceiling = mode::MAX_INFRA_FAILURES - 1;

        let with_fallback = tempfile::tempdir().unwrap();
        let root = with_fallback.path();
        let phase = PhaseId::new(5164);
        save_rate_limited_code_run(root, phase, Some(AgentKind::Codex), false, below_ceiling);
        let result = advance(root, Some(phase));
        let reloaded = workflow::load_state(root, phase);
        result.unwrap();
        let reloaded = reloaded.expect("a handed-off run keeps its state");
        assert_eq!(
            events_named(root, "agent_handoff").len(),
            1,
            "one handoff one below the ceiling"
        );
        assert_eq!(
            reloaded.infra_failures, below_ceiling,
            "the hop charged nothing to the infra counter (D-06)"
        );
        assert_eq!(reloaded.agent, AgentKind::Codex);

        let without_fallback = tempfile::tempdir().unwrap();
        let control_root = without_fallback.path();
        let control_phase = PhaseId::new(5165);
        save_rate_limited_code_run(control_root, control_phase, None, false, below_ceiling);
        let at_gate = state_on_disk_at_gate(control_root, control_phase, Stage::Code, || {
            advance(control_root, Some(control_phase)).unwrap();
        });
        assert!(
            events_named(control_root, "agent_handoff").is_empty(),
            "control: no fallback, no handoff"
        );
        assert_eq!(
            at_gate.agent,
            AgentKind::Claude,
            "control: the agent is unchanged"
        );
        assert_eq!(
            at_gate.infra_failures, below_ceiling,
            "control: the rate limit charged nothing either (D-20)"
        );
    }

    /// D-05: one hop per run, no automatic switch back. Once the fallback is
    /// consumed, a later Code rate limit waits at the Code gate on the current agent.
    /// Second fixture, the adjacency edge at runtime: an unconsumed fallback equal
    /// to the agent is no handoff either. A `codex` stub is on PATH, so a missing
    /// binary does not explain the absent hop.
    #[test]
    fn a_rate_limit_after_the_one_hop_waits_at_the_gate() {
        const NAME: &str =
            "pipeline_launch::tests::a_rate_limit_after_the_one_hop_waits_at_the_gate";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();
        for (phase, fallback, consumed) in [
            (PhaseId::new(5166), AgentKind::Codex, true),
            (PhaseId::new(5167), AgentKind::Claude, false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            save_rate_limited_code_run(root, phase, Some(fallback), consumed, 0);

            let reloaded = state_on_disk_at_gate(root, phase, Stage::Code, || {
                advance(root, Some(phase)).unwrap();
            });

            assert!(
                events_named(root, "agent_handoff").is_empty(),
                "phase {phase}: no hop; events: {:?}",
                events_named(root, "agent_handoff")
            );
            assert_waited_at_the_gate(root, phase, Stage::Code);
            assert_eq!(reloaded.agent, AgentKind::Claude, "phase {phase}");
            assert_eq!(reloaded.fallback_consumed, consumed, "phase {phase}");
        }
    }

    /// A transport cancel whose envelope still carries a success marker: the
    /// Antigravity shape `agent_result` reads as [`agent_result::AgentStatus::Ambiguous`]
    /// at Layer 1, whatever the run's agent.
    const AMBIGUOUS_CANCEL_CAPTURE: &str = concat!(
        r#"{"event":"init","model":"gemini-3.7-flash-high","inputFormat":"stream-json","outputFormat":"stream-json","printTimeout":"60m"}"#,
        "\n",
        r#"{"event":"result","result":{"status":"ERROR","response":"DEVFLOW_RESULT: {\"status\":\"success\"}\n","error":"context canceled"}}"#,
        "\n"
    );

    /// The ordering edge: failover sits only inside the `RateLimited` arm. An
    /// `Ambiguous` outcome at Code with an unused fallback re-drives the same
    /// agent, emits no `agent_handoff`, and leaves the fallback unused. The
    /// re-driven `claude` waits, so its supervisor writes nothing more before the
    /// state is read.
    #[test]
    fn an_ambiguous_outcome_with_an_unused_fallback_re_drives_without_handing_off() {
        const NAME: &str = "pipeline_launch::tests::an_ambiguous_outcome_with_an_unused_fallback_re_drives_without_handing_off";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = waiting_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5168);
        save_rate_limited_code_run(root, phase, Some(AgentKind::Codex), false, 0);
        std::fs::write(
            devflow_core::agent_result::stdout_path(root, phase),
            AMBIGUOUS_CANCEL_CAPTURE,
        )
        .unwrap();

        let launch = advance(root, Some(phase)).unwrap();
        let reloaded = workflow::load_state(root, phase).unwrap();

        assert_eq!(
            events_named(root, "ambiguous_transport_retry").len(),
            1,
            "the Ambiguous route ran"
        );
        assert!(
            events_named(root, "agent_handoff").is_empty(),
            "an Ambiguous outcome never hands off: {:?}",
            events_named(root, "agent_handoff")
        );
        assert!(!reloaded.fallback_consumed, "the fallback stays unused");
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert_eq!(
            launch.map(|launch| (launch.state.stage, launch.state.agent)),
            Some((Stage::Code, AgentKind::Claude)),
            "the same agent was re-driven"
        );
    }

    // ---- 51-07: a same-stage relaunch replays the interrupted fix pass (D-09) ----

    /// [`waiting_agents_dir`] whose `claude` and `codex` first write their argv,
    /// NUL-separated, to `<program>-argv.bin` in their working directory, then
    /// create `<program>-argv.done`, then wait (bounded, about 60 s) like the
    /// stubs they replace. The marker is created after the write completes, so a
    /// reader that sees it reads the whole argv.
    fn argv_recording_agents_dir() -> tempfile::TempDir {
        let path_dir = waiting_agents_dir();
        for program in ["claude", "codex"] {
            let path = path_dir.path().join(program);
            devflow_core::test_support::write_executable(
                &path,
                &format!(
                    "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
                     for arg in \"$@\"; do printf '%s\\0' \"$arg\"; done > {program}-argv.bin\n\
                     : > {program}-argv.done\n\
                     i=0\nwhile [ ! -e never-released ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
                     exit 0\n"
                ),
            );
        }
        path_dir
    }

    /// The argv the `program` stub from [`argv_recording_agents_dir`] recorded in
    /// `workdir`, waiting (bounded) for its marker; panics naming the program if
    /// the stub never ran.
    fn recorded_argv(workdir: &Path, program: &str) -> Vec<String> {
        const RECORD_WAIT: Duration = Duration::from_secs(30);
        let done = workdir.join(format!("{program}-argv.done"));
        let deadline = std::time::Instant::now() + RECORD_WAIT;
        while !done.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the {program} stub recorded no argv within {RECORD_WAIT:?} in {}",
                workdir.display()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let bytes = std::fs::read(workdir.join(format!("{program}-argv.bin"))).unwrap();
        bytes
            .split(|byte| *byte == 0)
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect()
    }

    /// Run a Code rate limit through a real `advance` for a Supervise-mode Claude
    /// run of `phase` at `root` with an unused Codex fallback, from a Code state
    /// entered by a loop-back with `fix` (`None`: a plain Code state). Returns the
    /// argv the handed-back `codex` launch carries.
    fn fail_over_from_code(
        root: &Path,
        phase: PhaseId,
        fix: Option<prompt::FixType>,
    ) -> Vec<String> {
        fail_over_from_code_with_note(root, phase, fix, None)
    }

    /// [`fail_over_from_code`] whose loop-back into Code carried the rejection
    /// note `note` (GATE-01): the fix pass records it as `code_fix_note`.
    fn fail_over_from_code_with_note(
        root: &Path,
        phase: PhaseId,
        fix: Option<prompt::FixType>,
        note: Option<&str>,
    ) -> Vec<String> {
        init_repo(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = if fix.is_some() {
            Stage::Validate
        } else {
            Stage::Code
        };
        state.fallback_agent = Some(AgentKind::Codex);
        workflow::save_state(&state).unwrap();
        if let Some(fix) = fix {
            crate::pipeline_gate::prepare_loop_back_to_code(
                root,
                &mut state,
                fix,
                note.map(str::to_string),
                None,
                crate::pipeline_gate::LoopBackReason::GateResponse,
            )
            .unwrap();
        }
        let entered = workflow::load_state(root, phase).unwrap();
        assert_eq!(entered.stage, Stage::Code);
        assert_eq!(
            entered.code_fix, fix,
            "the Code pass records the fix type it was entered with"
        );
        write_rate_limit_capture(root, phase);

        let launch = advance(root, Some(phase))
            .unwrap()
            .expect("the handoff relaunches the stage");
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Codex, "the run handed off");
        assert_eq!(
            launch.state.agent,
            AgentKind::Codex,
            "the fallback is launched"
        );
        launch.args
    }

    /// D-09 (Code, review B-1): a Code failover in the middle of a gaps-only pass
    /// hands the fallback the gaps-only workflow, rendered by the FALLBACK's
    /// driver, and the assertion is on the argv the handed-back `codex` launch carries.
    /// Control in the same test: the same failover from a plain Code pass
    /// delivers the plain execute-phase rendering.
    #[test]
    fn a_code_failover_during_a_gaps_only_pass_replays_gaps_only() {
        const NAME: &str =
            "pipeline_launch::tests::a_code_failover_during_a_gaps_only_pass_replays_gaps_only";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = argv_recording_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let codex = agents::driver_for(AgentKind::Codex);
        let code_prompt = |phase: PhaseId, fix: Option<prompt::FixType>| {
            codex.render_prompt(&prompt::StageIntent::Code {
                phase,
                fix,
                fix_note: None,
                relaunch_note: None,
                reverify_report: None,
            })
        };

        let fix_dir = tempfile::tempdir().unwrap();
        let fix_phase = PhaseId::new(5171);
        let argv = fail_over_from_code(fix_dir.path(), fix_phase, Some(prompt::FixType::GapsOnly));
        let gaps_only = code_prompt(fix_phase, Some(prompt::FixType::GapsOnly));
        assert_ne!(gaps_only, code_prompt(fix_phase, None));
        assert!(
            argv.contains(&gaps_only),
            "the fallback receives the gaps-only pass in Codex's own rendering: {argv:?}"
        );
        assert!(
            !argv.contains(&code_prompt(fix_phase, None)),
            "the fallback does not receive the plain execute-phase prompt: {argv:?}"
        );

        let plain_dir = tempfile::tempdir().unwrap();
        let plain_phase = PhaseId::new(5175);
        let argv = fail_over_from_code(plain_dir.path(), plain_phase, None);
        assert!(
            argv.contains(&code_prompt(plain_phase, None)),
            "control: a plain Code failover delivers execute-phase: {argv:?}"
        );
        assert!(
            !argv.contains(&code_prompt(plain_phase, Some(prompt::FixType::GapsOnly))),
            "control: a plain Code failover delivers no fix prompt: {argv:?}"
        );
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
    /// advances (here to its `--until plan` stop, so nothing launches).
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
            state.stop_until = Some(Stage::Plan);
            state.stopped = stopped;
            workflow::save_state(&state).unwrap();
            write_success_capture(root, phase);

            let result = advance_with(root, phase, Some(Stage::Plan));
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

    /// D-10, review concern 5: an advance over a state carrying a question's
    /// abort request aborts the run before it routes the stage outcome, and the
    /// abort consumes the request: the state file is gone, so no later `start`
    /// or `resume` reads it, and exactly one `workflow_aborted` is recorded.
    /// Control: the same state with no request routes its outcome (here to its
    /// `--until plan` stop) and records no abort.
    #[test]
    fn the_abort_request_is_consumed_by_the_abort() {
        let _guard = env_lock();
        for requested in [Some("stop"), None] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(1308);
            let mut state = State::new(
                phase,
                AgentKind::Claude,
                Mode::Supervise,
                root.to_path_buf(),
            );
            state.stop_until = Some(Stage::Plan);
            state.abort_requested = requested.map(str::to_string);
            workflow::save_state(&state).unwrap();
            assert_eq!(
                workflow::load_state(root, phase).unwrap().abort_requested,
                state.abort_requested,
                "precondition: the saved state carries the request"
            );
            write_success_capture(root, phase);

            let launch = advance_with(root, phase, Some(Stage::Plan)).expect("the advance");
            assert!(launch.is_none(), "no further stage is launched");
            let aborted = events_named(root, "workflow_aborted");
            if requested.is_some() {
                assert_eq!(aborted.len(), 1, "exactly one abort event: {aborted:?}");
                assert_eq!(aborted[0]["reason"], "stop", "{}", aborted[0]);
                assert!(
                    events_named(root, "advance_evaluated").is_empty(),
                    "an aborted advance routes no stage outcome"
                );
                assert!(
                    matches!(
                        workflow::load_state(root, phase),
                        Err(workflow::WorkflowError::MissingState(_))
                    ),
                    "the abort consumes the request with the state"
                );
            } else {
                assert!(aborted.is_empty(), "control: no abort: {aborted:?}");
                assert_eq!(events_named(root, "advance_evaluated").len(), 1);
                assert_eq!(
                    events_named(root, "workflow_finished").len(),
                    1,
                    "control reaches its --until stop"
                );
            }
        }
    }

    /// A question a test opens at `stage`, with its registration dropped unfinished
    /// as a SIGKILLed server leaves it: the spec and the gate id.
    fn open_abandoned_question(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> (devflow_core::question::QuestionSpec, GateId) {
        let (spec, gate_id, registration) = open_held_question(root, phase, stage);
        drop(registration);
        (spec, gate_id)
    }

    /// A question a test opens at `stage` with its waiter registration held.
    fn open_held_question(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> (
        devflow_core::question::QuestionSpec,
        GateId,
        devflow_core::gates::WaiterRegistration,
    ) {
        let spec = devflow_core::question::QuestionSpec::from_arguments(&serde_json::json!({
            "question": "Pick a budget",
            "options": [{ "id": "a", "label": "A" }, { "id": "b", "label": "B" }],
            "kind": "decision",
        }))
        .unwrap();
        let (opened, registration) =
            Gates::open_question_registered(root, phase, stage, &spec).unwrap();
        (spec, opened.gate_id, registration)
    }

    /// C1 backstop: after its agent exits, a question gate left at the stage (its
    /// waiter gone) is removed and records `question_closed{agent_exited}` with
    /// the spec's question id. Control: a stage-failure gate at the stage is left
    /// in place and records no question event.
    #[test]
    fn an_agent_exit_closes_its_stage_question_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1309);
        let (spec, gate_id) = open_abandoned_question(root, phase, Stage::Code);

        close_question_gate(root, phase, Stage::Code);

        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "the question gate is still present"
        );
        let closed = events_named(root, "question_closed");
        assert_eq!(closed.len(), 1, "one question_closed: {closed:?}");
        assert_eq!(closed[0]["reason"], "agent_exited", "{}", closed[0]);
        assert_eq!(closed[0]["question_id"], spec.question_id, "{}", closed[0]);
        assert_eq!(closed[0]["gate_id"], gate_id.as_str(), "{}", closed[0]);
        assert_eq!(closed[0]["stage"], "code", "{}", closed[0]);
        assert!(closed[0]["closed_by"].is_null(), "{}", closed[0]);

        let control = tempfile::tempdir().unwrap();
        let control_root = control.path();
        Gates::open_gate(
            control_root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "a failure",
        )
        .unwrap();
        close_question_gate(control_root, phase, Stage::Code);
        assert!(
            Gates::gate_path(control_root, phase, Stage::Code).exists(),
            "control: the stage-failure gate was removed"
        );
        assert!(
            events_named(control_root, "question_closed").is_empty(),
            "control: a stage-failure gate was given a question event"
        );
    }

    /// Review concern 4: an abort the operator published to a question just before
    /// its server died still ends the run. The advance records
    /// `question_closed{aborted_by_operator}` with the answer's responder, aborts with
    /// the abort's note (one `workflow_aborted`, no state left), removes the gate and
    /// launches nothing. Control: the same shape with no answer records
    /// `agent_exited` and no abort, and routes its outcome to its `--until` stop.
    #[test]
    fn an_abort_published_before_the_server_died_still_ends_the_run() {
        let _guard = env_lock();
        for answered in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(1310);
            let mut state = State::new(
                phase,
                AgentKind::Claude,
                Mode::Supervise,
                root.to_path_buf(),
            );
            state.stop_until = Some(Stage::Plan);
            workflow::save_state(&state).unwrap();
            write_success_capture(root, phase);
            let (_spec, gate_id, registration) = open_held_question(root, phase, Stage::Plan);
            if answered {
                Gates::answer(
                    root,
                    phase,
                    Stage::Plan,
                    &gate_id,
                    &devflow_core::gates::GateResponse {
                        approved: false,
                        abort: true,
                        note: Some("stop".into()),
                        responded_by: Some("operator".into()),
                    },
                )
                .unwrap();
            }
            drop(registration);

            let launch = advance_with(root, phase, Some(Stage::Plan)).expect("the advance");

            assert!(launch.is_none(), "no further stage is launched");
            assert!(
                !Gates::gate_path(root, phase, Stage::Plan).exists(),
                "the question gate is still present"
            );
            let closed = events_named(root, "question_closed");
            assert_eq!(closed.len(), 1, "one question_closed: {closed:?}");
            assert_eq!(closed[0]["gate_id"], gate_id.as_str(), "{}", closed[0]);
            let aborted = events_named(root, "workflow_aborted");
            if answered {
                assert_eq!(closed[0]["reason"], "aborted_by_operator", "{}", closed[0]);
                assert_eq!(closed[0]["closed_by"], "operator", "{}", closed[0]);
                assert_eq!(
                    aborted.len(),
                    1,
                    "the published abort did not end the run: {aborted:?}"
                );
                assert_eq!(aborted[0]["reason"], "stop", "{}", aborted[0]);
                assert!(
                    matches!(
                        workflow::load_state(root, phase),
                        Err(workflow::WorkflowError::MissingState(_))
                    ),
                    "the abort consumes the request with the state"
                );
            } else {
                assert_eq!(closed[0]["reason"], "agent_exited", "{}", closed[0]);
                assert!(aborted.is_empty(), "control: no abort: {aborted:?}");
                assert_eq!(
                    events_named(root, "workflow_finished").len(),
                    1,
                    "control reaches its --until stop"
                );
            }
        }
    }

    /// `resume` closing a leftover question gate records
    /// `question_closed{closed_on_resume}` for it; a non-question gate it closes as
    /// before, with no question event.
    #[test]
    fn resume_records_question_closed_for_a_leftover_question_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1311);
        let (spec, gate_id) = open_abandoned_question(root, phase, Stage::Code);
        Gates::open_gate(
            root,
            phase,
            Stage::Plan,
            devflow_core::gates::GateKind::StageFailure,
            "a failure",
        )
        .unwrap();

        close_gates_for_resume(root, phase, Stage::Code);

        for stage in [Stage::Plan, Stage::Code] {
            assert!(
                !Gates::gate_path(root, phase, stage).exists(),
                "resume left the {stage} gate"
            );
        }
        let closed = events_named(root, "question_closed");
        assert_eq!(closed.len(), 1, "one question_closed: {closed:?}");
        assert_eq!(closed[0]["reason"], "closed_on_resume", "{}", closed[0]);
        assert_eq!(closed[0]["question_id"], spec.question_id, "{}", closed[0]);
        assert_eq!(closed[0]["gate_id"], gate_id.as_str(), "{}", closed[0]);
        assert_eq!(closed[0]["stage"], "code", "{}", closed[0]);
    }

    // ---- D-16 (999.166 item 6): the exit file records a safe-point stop ----

    // ---- 50.1-17: the stage handoff records its next supervisor first (D-01) ----

    // ---- 50-23: every launch failure path has a defined end state ----

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

    /// `line`'s code part: the text before its first `//` that lies outside a
    /// `"…"` string literal, found by a quote scan honouring `\` escapes. A trailing
    /// comment is not code, and a `//` inside a literal (`"http://x"`) is not one.
    fn code_part(line: &str) -> &str {
        let bytes = line.as_bytes();
        let mut in_string = false;
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' if in_string => index += 1,
                b'"' => in_string = !in_string,
                b'/' if !in_string && bytes.get(index + 1) == Some(&b'/') => {
                    return &line[..index];
                }
                _ => {}
            }
            index += 1;
        }
        line
    }

    /// Resume a stopped Code run of `phase` at `root` under Codex whose Code pass
    /// was entered with `fix`, and return the argv the spawned `codex` received
    /// and the guard that ends its runner. Codex takes the prompt in argv.
    fn resume_code_run(
        root: &Path,
        phase: PhaseId,
        fix: Option<prompt::FixType>,
    ) -> (Vec<String>, crate::service::double::LaunchedRunners) {
        resume_code_run_with_note(root, phase, fix, None)
    }

    /// [`resume_code_run`] whose fix pass holds the rejection note `note` as
    /// `code_fix_note` (GATE-01).
    fn resume_code_run_with_note(
        root: &Path,
        phase: PhaseId,
        fix: Option<prompt::FixType>,
        note: Option<&str>,
    ) -> (Vec<String>, crate::service::double::LaunchedRunners) {
        init_repo(root);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.code_fix = fix;
        state.code_fix_note = note.map(str::to_string);
        state.stopped = true;
        state.stop_reason = Some("rate limited".to_string());
        workflow::save_state(&state).unwrap();

        crate::service::double::set_full_runner(true);
        let result = resume(root, phase, None);
        let runners = crate::service::double::LaunchedRunners;
        result.unwrap();
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.code_fix, fix, "a resume keeps the fix pass");
        (recorded_argv(root, "codex"), runners)
    }

    /// D-09: a `devflow resume` in the middle of a gaps-only pass relaunches that pass, asserted on the argv the spawned
    /// `codex` received. Control: the same resume with no fix pass recorded
    /// delivers the plain execute-phase prompt.
    #[test]
    fn a_resume_mid_gaps_only_pass_replays_gaps_only() {
        const NAME: &str = "pipeline_launch::tests::a_resume_mid_gaps_only_pass_replays_gaps_only";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = argv_recording_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let codex = agents::driver_for(AgentKind::Codex);
        let code_prompt = |phase: PhaseId, fix: Option<prompt::FixType>| {
            codex.render_prompt(&prompt::StageIntent::Code {
                phase,
                fix,
                fix_note: None,
                relaunch_note: None,
                reverify_report: None,
            })
        };

        let fix_dir = tempfile::tempdir().unwrap();
        let fix_phase = PhaseId::new(5173);
        let (argv, _reap_fix) =
            resume_code_run(fix_dir.path(), fix_phase, Some(prompt::FixType::GapsOnly));
        assert!(
            argv.contains(&code_prompt(fix_phase, Some(prompt::FixType::GapsOnly))),
            "the resumed agent receives the gaps-only pass: {argv:?}"
        );
        assert!(
            !argv.contains(&code_prompt(fix_phase, None)),
            "the resumed agent does not receive the plain execute-phase prompt: {argv:?}"
        );

        let plain_dir = tempfile::tempdir().unwrap();
        let plain_phase = PhaseId::new(5176);
        let (argv, _reap_plain) = resume_code_run(plain_dir.path(), plain_phase, None);
        assert!(
            argv.contains(&code_prompt(plain_phase, None)),
            "control: a plain resume delivers execute-phase: {argv:?}"
        );
        assert!(
            !argv.contains(&code_prompt(plain_phase, Some(prompt::FixType::GapsOnly))),
            "control: a plain resume delivers no fix prompt: {argv:?}"
        );
    }

    /// Resume a stopped Codex run of `phase` at `root` at `stage`, with the
    /// gap-closure marker `gap_pass` and the fix pass `code_fix`, stopped as
    /// `devflow stop` leaves it. Returns the argv the spawned `codex` received,
    /// the state on disk after the resume and the guard that ends its runner.
    fn resume_gap_pass_run(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        gap_pass: bool,
        code_fix: Option<prompt::FixType>,
    ) -> (Vec<String>, State, crate::service::double::LaunchedRunners) {
        init_repo(root);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = stage;
        state.gap_closure_pass = gap_pass;
        state.code_fix = code_fix;
        state.stopped = true;
        state.stop_reason = Some("stopped by `devflow stop`".to_string());
        workflow::save_state(&state).unwrap();

        crate::service::double::set_full_runner(true);
        let result = resume(root, phase, None);
        let runners = crate::service::double::LaunchedRunners;
        result.unwrap();
        let reloaded = workflow::load_state(root, phase).unwrap();
        (recorded_argv(root, "codex"), reloaded, runners)
    }

    /// Review C6 (GATE-03): a gaps Plan pass that was stopped and is resumed
    /// relaunches as the gaps Plan pass (`plan-phase --gaps`), and the resume
    /// keeps the gap-closure marker. Asserted on the argv the spawned `codex`
    /// received. Control in the same test: the same stopped Plan without the
    /// marker resumes an ordinary Plan prompt without `--gaps`.
    #[test]
    fn a_stopped_gaps_plan_pass_resumes_with_gaps() {
        const NAME: &str = "pipeline_launch::tests::a_stopped_gaps_plan_pass_resumes_with_gaps";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = argv_recording_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let codex = agents::driver_for(AgentKind::Codex);

        let gaps_dir = tempfile::tempdir().unwrap();
        let gaps_phase = PhaseId::new(5271);
        let (argv, reloaded, _reap_gaps) =
            resume_gap_pass_run(gaps_dir.path(), gaps_phase, Stage::Plan, true, None);
        let gaps_prompt = codex.render_prompt(&prompt::StageIntent::PlanGaps {
            phase: gaps_phase,
            relaunch_note: None,
        });
        assert!(
            gaps_prompt.contains(&format!("plan-phase.md for phase {gaps_phase} --gaps")),
            "the fixture's gaps Plan prompt runs plan-phase --gaps: {gaps_prompt}"
        );
        assert!(
            argv.contains(&gaps_prompt),
            "the resumed agent receives the gaps Plan pass: {argv:?}"
        );
        assert!(
            reloaded.gap_closure_pass,
            "the resume keeps the gap-closure marker"
        );

        // Control: no marker, an ordinary Plan without --gaps.
        let plain_dir = tempfile::tempdir().unwrap();
        let plain_phase = PhaseId::new(5272);
        let (argv, reloaded, _reap_plain) =
            resume_gap_pass_run(plain_dir.path(), plain_phase, Stage::Plan, false, None);
        assert!(
            argv.contains(
                &codex.render_prompt(&prompt::StageIntent::for_stage(Stage::Plan, plain_phase))
            ),
            "control: a plain resume delivers the ordinary Plan prompt: {argv:?}"
        );
        assert!(
            !argv.iter().any(|arg| arg.contains("--gaps")),
            "control: a plain resume does not run --gaps: {argv:?}"
        );
        assert!(!reloaded.gap_closure_pass, "control: no marker appears");
    }

    /// D-04 (#250): a reviews pass's Plan state, saved and reloaded as a resume reloads
    /// it, renders the reviews pass with no resume flag: by codex through plan-phase.md's
    /// reviews mode, by claude as `/gsd-plan-phase N --reviews`, neither with the plain
    /// Plan prompt's PLAN.md short-circuit.
    #[test]
    fn a_reviews_pass_plan_renders_reviews_after_resume() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5273);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.stage = Stage::Plan;
        state.reviews_pass = true;
        state.stopped = true;
        workflow::save_state(&state).unwrap();
        let reloaded = workflow::load_state(root, phase).unwrap();

        let codex = default_stage_prompt(&reloaded, agents::driver_for(AgentKind::Codex).as_ref());
        let claude =
            default_stage_prompt(&reloaded, agents::driver_for(AgentKind::Claude).as_ref());
        for (prompt, needle) in [
            (&codex, format!("plan-phase.md for phase {phase} --reviews")),
            (&claude, format!("/gsd-plan-phase {phase} --reviews")),
        ] {
            assert!(
                prompt.contains(&needle) && !prompt.contains("already exists"),
                "a resumed reviews pass did not render --reviews: {prompt}"
            );
        }
    }

    /// Review C6 (GATE-03): a gaps Code pass whose `code_fix` was cleared (as
    /// an `--until code` halt leaves it; a stop keeps `code_fix`), resumed with
    /// the gap-closure marker still set, relaunches the gaps-only execute and
    /// not the full one: the gaps-only intent is derived from the marker.
    /// Control in the same test: the same state without the marker resumes a
    /// full execute.
    #[test]
    fn a_stopped_gaps_code_pass_resumes_with_gaps_only() {
        const NAME: &str =
            "pipeline_launch::tests::a_stopped_gaps_code_pass_resumes_with_gaps_only";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = argv_recording_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let codex = agents::driver_for(AgentKind::Codex);
        let code_prompt = |phase: PhaseId, fix: Option<prompt::FixType>| {
            codex.render_prompt(&prompt::StageIntent::Code {
                phase,
                fix,
                fix_note: None,
                relaunch_note: None,
                reverify_report: None,
            })
        };

        let gaps_dir = tempfile::tempdir().unwrap();
        let gaps_phase = PhaseId::new(5273);
        let (argv, reloaded, _reap_gaps) =
            resume_gap_pass_run(gaps_dir.path(), gaps_phase, Stage::Code, true, None);
        assert!(
            argv.contains(&code_prompt(gaps_phase, Some(prompt::FixType::GapsOnly))),
            "the resumed agent receives the gaps-only execute: {argv:?}"
        );
        assert!(
            !argv.contains(&code_prompt(gaps_phase, None)),
            "the resumed agent does not receive the full execute: {argv:?}"
        );
        assert!(reloaded.gap_closure_pass, "the resume keeps the marker");

        // Control: no marker and no fix pass, a full execute.
        let plain_dir = tempfile::tempdir().unwrap();
        let plain_phase = PhaseId::new(5274);
        let (argv, _reloaded, _reap_plain) =
            resume_gap_pass_run(plain_dir.path(), plain_phase, Stage::Code, false, None);
        assert!(
            argv.contains(&code_prompt(plain_phase, None)),
            "control: a plain resume delivers the full execute: {argv:?}"
        );
        assert!(
            !argv.iter().any(|arg| arg.contains("--gaps-only")),
            "control: a plain resume runs no gaps-only pass: {argv:?}"
        );
    }

    // ---- GATE-01: a replayed or relaunched pass keeps the operator's note ----

    /// A fix-pass note with a newline, a tab, quotes and a backtick.
    const FIX_NOTE_Q: &str =
        "Q: fix the gaps in\tparse.rs,\nthen run \"cargo test\" and `cargo fmt`";

    /// A relaunch note with a newline and a tab.
    const RELAUNCH_NOTE_R: &str = "R: the stage failed on a flaky step;\n\tretry it once";

    /// Whether some argument in `argv` holds `note` byte for byte.
    fn argv_holds(argv: &[String], note: &str) -> bool {
        argv.iter().any(|arg| arg.contains(note))
    }

    /// GATE-01 / D-14: a fix pass entered with a rejection note keeps that note
    /// when it is replayed, through `devflow resume` and through the rate-limit
    /// failover handoff; the assertion is on the argv the spawned `codex`
    /// received. Control in the same test: the same replays of a fix pass with
    /// no note carry no note block.
    #[test]
    fn a_replayed_fix_pass_keeps_the_note() {
        const NAME: &str = "pipeline_launch::tests::a_replayed_fix_pass_keeps_the_note";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = argv_recording_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let gaps = Some(prompt::FixType::GapsOnly);

        let resume_dir = tempfile::tempdir().unwrap();
        let (argv, _reap_resume) = resume_code_run_with_note(
            resume_dir.path(),
            PhaseId::new(5290),
            gaps,
            Some(FIX_NOTE_Q),
        );
        assert!(
            argv_holds(&argv, FIX_NOTE_Q),
            "a resumed fix pass hands the agent its note: {argv:?}"
        );
        let reloaded = workflow::load_state(resume_dir.path(), PhaseId::new(5290)).unwrap();
        assert_eq!(reloaded.code_fix_note.as_deref(), Some(FIX_NOTE_Q));

        let failover_dir = tempfile::tempdir().unwrap();
        let argv = fail_over_from_code_with_note(
            failover_dir.path(),
            PhaseId::new(5291),
            gaps,
            Some(FIX_NOTE_Q),
        );
        assert!(
            argv_holds(&argv, FIX_NOTE_Q),
            "the fallback's replay of the fix pass carries the note: {argv:?}"
        );

        // Control: the same two replays of a fix pass with no note.
        let plain_dir = tempfile::tempdir().unwrap();
        let (argv, _reap_plain) =
            resume_code_run_with_note(plain_dir.path(), PhaseId::new(5292), gaps, None);
        assert!(
            !argv.iter().any(|arg| arg.contains("OPERATOR NOTE")),
            "control: no note, no block: {argv:?}"
        );
        let plain_failover_dir = tempfile::tempdir().unwrap();
        let argv = fail_over_from_code_with_note(
            plain_failover_dir.path(),
            PhaseId::new(5293),
            gaps,
            None,
        );
        assert!(
            !argv.iter().any(|arg| arg.contains("OPERATOR NOTE")),
            "control: no note, no block: {argv:?}"
        );
    }

    /// A supervise-mode Codex run of `phase` at `root` at `stage`, saved.
    fn failing_stage_state(root: &Path, phase: PhaseId, stage: Stage) -> State {
        init_repo(root);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = stage;
        workflow::save_state(&state).unwrap();
        state
    }

    /// Fail `state`'s stage through `fail` (which opens its stage-failure gate),
    /// with that gate pre-answered as a rejection carrying `note`. Returns the
    /// state as it is on disk afterwards and the relaunch the answer resolved, if any.
    fn reject_stage_failure(
        root: &Path,
        state: &mut State,
        note: &str,
        fail: impl FnOnce(&Path, &mut State) -> Result<Option<StageLaunch>, CliError>,
    ) -> (State, Option<StageLaunch>) {
        preanswer_gate_response(
            root,
            state.phase,
            state.stage,
            &gate_response(false, Some(note)),
        );
        let launch = fail(root, state).unwrap();
        let reloaded = workflow::load_state(root, state.phase).unwrap();
        (reloaded, launch)
    }

    /// GATE-01 / D-39 (U1): a rejected stage-failure gate relaunches its stage
    /// with the reject's note in the prompt, through `relaunch_note`, at Plan,
    /// at Code (an ordinary Code pass) and at Validate (opened by the infra
    /// path). Rows: a Code fix pass with fix note Q fails and its gate is
    /// rejected with R, and the relaunch holds Q under the fix request's
    /// introduction, then R under the relaunch's, with `code_fix_note` still Q;
    /// and a run whose recorded delivery canary refuses its agent, where the
    /// reject launches nothing, stops the run naming `devflow resume`, and keeps
    /// R on disk for that resume. Control: the stage's first launch prompt does
    /// not hold the reject's note.
    #[test]
    fn a_rejected_stage_failure_relaunches_the_stage_with_the_note() {
        const NAME: &str =
            "pipeline_launch::tests::a_rejected_stage_failure_relaunches_the_stage_with_the_note";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = argv_recording_agents_dir();
            let output = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("20"))],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let codex = agents::driver_for(AgentKind::Codex);

        for (number, stage, infra) in [
            (5294, Stage::Plan, false),
            (5295, Stage::Code, false),
            (5296, Stage::Validate, true),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(number);
            let mut state = failing_stage_state(dir.path(), phase, stage);
            let first_launch = default_stage_prompt(&state, codex.as_ref());
            assert!(
                !first_launch.contains(RELAUNCH_NOTE_R),
                "control: {stage}'s first launch prompt holds no reject note"
            );
            let (reloaded, launch) =
                reject_stage_failure(dir.path(), &mut state, RELAUNCH_NOTE_R, |root, state| {
                    if infra {
                        crate::pipeline_outcomes::handle_infra_outcome(
                            root,
                            state,
                            stage,
                            Some("the agent was killed".into()),
                        )
                    } else {
                        crate::pipeline_outcomes::handle_stage_failure(
                            root,
                            state,
                            stage,
                            Some("the stage failed".into()),
                        )
                    }
                });
            assert_eq!(reloaded.stage, stage, "{stage}: the same stage relaunches");
            assert_eq!(
                reloaded.relaunch_note.as_deref(),
                Some(RELAUNCH_NOTE_R),
                "{stage}: the reject's note is the relaunch note"
            );
            assert_eq!(
                reloaded.code_fix_note, None,
                "{stage}: a stage-failure note is never the fix request"
            );
            let argv = launch.expect("the reject relaunches the stage").args;
            assert!(
                argv_holds(&argv, RELAUNCH_NOTE_R),
                "{stage}: the relaunched agent's prompt holds the note: {argv:?}"
            );
        }

        // Q -> failure -> R: a Code fix pass fails and its gate is rejected.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5297);
        let mut state = failing_stage_state(dir.path(), phase, Stage::Code);
        commit(&mut state, |s| {
            s.code_fix = Some(prompt::FixType::GapsOnly);
            s.code_fix_note = Some(FIX_NOTE_Q.to_string());
        })
        .unwrap();
        let (reloaded, launch) =
            reject_stage_failure(dir.path(), &mut state, RELAUNCH_NOTE_R, |root, state| {
                crate::pipeline_outcomes::handle_stage_failure(
                    root,
                    state,
                    Stage::Code,
                    Some("the fix pass failed".into()),
                )
            });
        assert_eq!(
            reloaded.code_fix_note.as_deref(),
            Some(FIX_NOTE_Q),
            "the fix request is kept"
        );
        assert_eq!(reloaded.relaunch_note.as_deref(), Some(RELAUNCH_NOTE_R));
        let argv = launch.expect("the reject relaunches the fix pass").args;
        let relaunched = argv
            .iter()
            .find(|arg| arg.contains(RELAUNCH_NOTE_R))
            .unwrap_or_else(|| panic!("the relaunch holds R: {argv:?}"));
        let at = |needle: &str| {
            relaunched
                .find(needle)
                .unwrap_or_else(|| panic!("the relaunch lacks {needle:?}:\n{relaunched}"))
        };
        assert!(
            at(prompt::FIX_NOTE_INTRODUCTION) < at(FIX_NOTE_Q)
                && at(FIX_NOTE_Q) < at(prompt::RELAUNCH_NOTE_INTRODUCTION)
                && at(prompt::RELAUNCH_NOTE_INTRODUCTION) < at(RELAUNCH_NOTE_R),
            "Q under the fix request's introduction, then R under the relaunch's:\n{relaunched}"
        );

        // 52-29 (U1), the `Quick` row: a quick pass carrying Q fails and its
        // gate is rejected with R through `reject_route`. The relaunch runs
        // `/gsd-quick` with Q under the fix request's introduction and R under
        // the relaunch's, and Q is still the fix note on disk.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5299);
        let mut state = failing_stage_state(dir.path(), phase, Stage::Code);
        commit(&mut state, |s| {
            s.code_fix = Some(prompt::FixType::Quick);
            s.code_fix_note = Some(FIX_NOTE_Q.to_string());
        })
        .unwrap();
        let (reloaded, launch) =
            reject_stage_failure(dir.path(), &mut state, RELAUNCH_NOTE_R, |root, state| {
                crate::pipeline_outcomes::handle_stage_failure(
                    root,
                    state,
                    Stage::Code,
                    Some("the quick pass failed".into()),
                )
            });
        assert_eq!(
            reloaded.code_fix,
            Some(prompt::FixType::Quick),
            "quick: kept"
        );
        assert_eq!(
            reloaded.code_fix_note.as_deref(),
            Some(FIX_NOTE_Q),
            "quick: the pending quick-fix note is kept byte for byte"
        );
        let argv = launch.expect("quick: the reject relaunches the pass").args;
        let relaunched = argv
            .iter()
            .find(|arg| arg.contains(RELAUNCH_NOTE_R))
            .unwrap_or_else(|| panic!("quick: the relaunch holds R: {argv:?}"));
        let at = |needle: &str| {
            relaunched
                .find(needle)
                .unwrap_or_else(|| panic!("quick: the relaunch lacks {needle:?}:\n{relaunched}"))
        };
        assert!(
            at("/gsd-quick") < at(prompt::FIX_NOTE_INTRODUCTION)
                && at(prompt::FIX_NOTE_INTRODUCTION) < at(FIX_NOTE_Q)
                && at(FIX_NOTE_Q) < at(prompt::RELAUNCH_NOTE_INTRODUCTION)
                && at(prompt::RELAUNCH_NOTE_INTRODUCTION) < at(RELAUNCH_NOTE_R),
            "quick: `/gsd-quick`, Q under the fix request, then R:\n{relaunched}"
        );

        // A run whose recorded delivery canary refuses its agent.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5298);
        init_repo(dir.path());
        let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, dir.path().into());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.canary = Some(canary::CanaryOutcome::Absent);
        workflow::save_state(&state).unwrap();
        assert!(
            recorded_canary_refusal(&state).is_some(),
            "the canary refuses Claude"
        );
        let (reloaded, launch) =
            reject_stage_failure(dir.path(), &mut state, RELAUNCH_NOTE_R, |root, state| {
                crate::pipeline_outcomes::handle_stage_failure(
                    root,
                    state,
                    Stage::Code,
                    Some("the stage failed".into()),
                )
            });
        assert!(launch.is_none(), "a refused relaunch launches nothing");
        assert!(reloaded.stopped, "the refused relaunch stops the run");
        let reason = reloaded.stop_reason.clone().unwrap_or_default();
        assert!(
            reason.contains(&format!("devflow resume --phase {phase} --agent <other>")),
            "the stop names the resume: {reason}"
        );
        assert_eq!(
            reloaded.relaunch_note.as_deref(),
            Some(RELAUNCH_NOTE_R),
            "the note is kept for that resume"
        );
    }

    // ---- 51-07: one place renders a no-override stage prompt (D-09, review B-1) ----

    const DEFAULT_RENDER_NEEDLE: &str = concat!("StageIntent::for_", "stage(");
    const DEFAULT_RENDER_NOT_ONCE: &str =
        "a default intent is built more than once (or never) in the non-test code";
    const DEFAULT_RENDER_OUTSIDE: &str = "a default intent is built outside default_stage_prompt";
    const DEFAULT_RENDER_INNER_BYPASS: &str =
        "launch_stage_inner does not call default_stage_prompt";
    const DEFAULT_RENDER_OUTER_BYPASS: &str = "launch_stage does not call default_stage_prompt";

    /// The fn named on a top-level line holding `fn <name>(` or `fn <name><`.
    fn top_level_fn_name(line: &str) -> Option<&str> {
        let after = code_part(line).split_once("fn ")?.1;
        let end = after.find(['(', '<'])?;
        Some(&after[..end])
    }

    /// For each of `lines`, the top-level fn whose item it lies in: from a
    /// non-indented line naming a fn through the next line starting with `}`.
    /// Lines in no top-level fn (an `impl` block's methods included) map to `None`.
    fn enclosing_top_level_fns<'a>(lines: &[&'a str]) -> Vec<Option<&'a str>> {
        let mut current = None;
        lines
            .iter()
            .map(|line| {
                if !line.starts_with(char::is_whitespace)
                    && let Some(name) = top_level_fn_name(line)
                {
                    current = Some(name);
                }
                let here = current;
                if line.starts_with('}') {
                    current = None;
                }
                here
            })
            .collect()
    }

    /// D-09's single-renderer rule as a pure scan over `source`, cut at its
    /// `mod tests {` line with comment lines and trailing comments stripped: the
    /// default stage intent is built exactly once, inside `default_stage_prompt`,
    /// and both `launch_stage_inner` (whose prompt the agent receives) and
    /// `launch_stage` (which resolves the program from it) call that fn.
    fn default_render_violations(source: &str) -> Vec<String> {
        let lines = non_test_code_lines(source);
        let owners = enclosing_top_level_fns(&lines);
        let mut violations = Vec::new();
        let renders: Vec<Option<&str>> = lines
            .iter()
            .zip(&owners)
            .filter(|(line, _)| code_part(line).contains(DEFAULT_RENDER_NEEDLE))
            .map(|(_, owner)| *owner)
            .collect();
        if renders.len() != 1 {
            violations.push(DEFAULT_RENDER_NOT_ONCE.to_string());
        }
        if renders
            .iter()
            .any(|owner| *owner != Some("default_stage_prompt"))
        {
            violations.push(DEFAULT_RENDER_OUTSIDE.to_string());
        }
        for (caller, violation) in [
            ("launch_stage_inner", DEFAULT_RENDER_INNER_BYPASS),
            ("launch_stage", DEFAULT_RENDER_OUTER_BYPASS),
        ] {
            let calls = lines.iter().zip(&owners).any(|(line, owner)| {
                *owner == Some(caller) && code_part(line).contains("default_stage_prompt(")
            });
            if !calls {
                violations.push(violation.to_string());
            }
        }
        violations
    }

    /// D-09 (review B-1): every no-override launch renders its prompt through
    /// `default_stage_prompt`, so a persisted fix type reaches the agent on every
    /// path. Planted sources first, each carrying one defect, then the real file.
    #[test]
    fn every_no_override_launch_renders_through_default_stage_prompt() {
        let render = format!("    driver.render_prompt(&{DEFAULT_RENDER_NEEDLE}..));\n");
        let default_fn =
            format!("fn default_stage_prompt(state: &State) -> String {{\n{render}}}\n");
        let caller = |header: &str, body: &str| format!("{header}\n{body}}}\n");
        let calls = "    let prompt = default_stage_prompt(state, driver.as_ref());\n";
        let inner = caller("pub(crate) fn launch_stage_inner(", calls);
        let outer = caller("pub(crate) fn launch_stage(", calls);
        let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        assert_eq!(
            default_render_violations(&format!("{default_fn}{inner}{outer}")),
            Vec::<String>::new(),
            "a correct layout"
        );
        assert_eq!(
            default_render_violations(&format!(
                "{default_fn}{inner}{outer}fn other() {{\n{render}}}\n"
            )),
            strings(&[DEFAULT_RENDER_NOT_ONCE, DEFAULT_RENDER_OUTSIDE]),
            "a second render in another fn is reported"
        );
        assert_eq!(
            default_render_violations(&format!(
                "{default_fn}{}{outer}",
                caller("pub(crate) fn launch_stage_inner(", &render)
            )),
            strings(&[
                DEFAULT_RENDER_NOT_ONCE,
                DEFAULT_RENDER_OUTSIDE,
                DEFAULT_RENDER_INNER_BYPASS
            ]),
            "the delivered prompt rendered in launch_stage_inner itself (review B-1)"
        );
        assert_eq!(
            default_render_violations(&format!(
                "{default_fn}{inner}{}",
                caller(
                    "pub(crate) fn launch_stage(",
                    "    let x = 1; // default_stage_prompt(\n"
                )
            )),
            strings(&[DEFAULT_RENDER_OUTER_BYPASS]),
            "a call in a trailing comment is not a call"
        );
        assert_eq!(
            default_render_violations(&format!(
                "{default_fn}{inner}{outer}mod tests {{\nfn other() {{\n{render}}}\n"
            )),
            Vec::<String>::new(),
            "a render after the test-module cut does not count"
        );

        let real = default_render_violations(include_str!("pipeline_launch.rs"));
        assert!(real.is_empty(), "pipeline_launch.rs: {real:?}");
    }

    // ---- 50-23 Task 3: a stop after safe point 3 is forwarded (RV-04) ----

    /// CR-03 and WR-10's successor: a hand-run `advance` whose environment names a
    /// notify command persists it where it reads it, so it survives an advance that
    /// fails before any later write. With the variable unset nothing is written at
    /// all: the file is the same inode with the same bytes, since every write
    /// replaces it by rename.
    #[test]
    fn advance_commits_a_changed_notify_command_where_it_sets_it() {
        use std::os::unix::fs::MetadataExt;
        const NAME: &str =
            "pipeline_launch::tests::advance_commits_a_changed_notify_command_where_it_sets_it";
        const PERSISTED: &str = ": persisted";
        const EXPORTED: &str = ": exported";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
            for env in [
                vec![(
                    devflow_core::gates::GATE_NOTIFY_CMD_ENV,
                    std::ffi::OsStr::new(EXPORTED),
                )],
                vec![],
            ] {
                let output =
                    crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &env);
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, NAME,
                );
            }
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2404);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some(PERSISTED.to_string());
        workflow::save_state(&state).unwrap();
        let path = workflow::state_path(root, phase);
        let bytes_before = std::fs::read(&path).unwrap();
        let inode_before = std::fs::metadata(&path).unwrap().ino();

        let err = advance_with(root, phase, Some(Stage::Plan))
            .expect_err("the stage mismatch fails the advance before any later write");
        assert!(err.to_string().contains("stage mismatch"), "{err}");

        if std::env::var_os(devflow_core::gates::GATE_NOTIFY_CMD_ENV).is_some() {
            assert_eq!(
                workflow::load_state(root, phase)
                    .unwrap()
                    .gate_notify_cmd
                    .as_deref(),
                Some(EXPORTED),
                "the exported command is on disk despite the failed advance"
            );
        } else {
            assert_eq!(std::fs::read(&path).unwrap(), bytes_before);
            assert_eq!(
                std::fs::metadata(&path).unwrap().ino(),
                inode_before,
                "no write happens when the command did not change"
            );
        }
    }

    /// `resume` clears the stop and keeps a field another writer changed after it
    /// loaded its copy.
    #[test]
    fn resume_commits_its_unstop_and_keeps_a_concurrent_field() {
        const NAME: &str =
            "pipeline_launch::tests::resume_commits_its_unstop_and_keeps_a_concurrent_field";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(2405);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.stopped = true;
        state.stop_reason = Some("stopped after code completed (--until code)".to_string());
        state.stop_until = Some(Stage::Code);
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.base_branch = Some("workspace/concurrent".to_string());
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        let result = resume_loaded(root, phase, state, None, None, false);
        result.unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            after.base_branch.as_deref(),
            Some("workspace/concurrent"),
            "the other writer's change survives the resume"
        );
        assert!(!after.stopped);
        assert_eq!(after.stop_reason, None);
        assert_eq!(after.stop_until, None);
        assert!(
            crate::service::double::launched_units().is_empty(),
            "resume_loaded launches nothing: its caller launches the runner unit"
        );
    }

    /// Whether `line` names `save_state` as a whole identifier: an occurrence with no
    /// ASCII letter, digit or `_` on either side, which also catches the function
    /// passed as a value.
    fn names_save_state(line: &str) -> bool {
        const NAME: &str = "save_state";
        let continues_identifier = |c: char| c.is_ascii_alphanumeric() || c == '_';
        line.match_indices(NAME).any(|(at, _)| {
            !line[..at]
                .chars()
                .next_back()
                .is_some_and(continues_identifier)
                && !line[at + NAME.len()..]
                    .chars()
                    .next()
                    .is_some_and(continues_identifier)
        })
    }

    /// Every line of `source` that names `save_state`, before the first line that is
    /// exactly `mod tests {`, leaving out whole-line `//` comments. Lines keep their
    /// numbers in `source`, and a trailing comment stays as part of its code line.
    fn save_state_violations(path: &str, source: &str) -> Vec<String> {
        source
            .lines()
            .enumerate()
            .take_while(|(_, line)| *line != "mod tests {")
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .filter(|(_, line)| names_save_state(line))
            .map(|(index, _)| format!("{path}: line {}: save_state", index + 1))
            .collect()
    }

    /// Every `.rs` file under `dir`, recursively, in sorted order.
    fn rust_files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.extend(rust_files_under(&path));
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
        files.sort();
        files
    }

    /// Every line of `source` that opens a gate without registering its waiter, before
    /// the first line that is exactly `mod tests {`, leaving out whole-line `//`
    /// comments. Lines keep their numbers in `source`.
    fn unregistered_gate_open_violations(label: &str, source: &str) -> Vec<String> {
        source
            .lines()
            .enumerate()
            .take_while(|(_, line)| *line != "mod tests {")
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .filter(|(_, line)| {
                [
                    "Gates::open_gate(",
                    "Gates::write_gate(",
                    "Gates::register_waiter(",
                ]
                .iter()
                .any(|opener| line.contains(opener))
            })
            .map(|(index, line)| format!("{label}: line {}: {}", index + 1, line.trim()))
            .collect()
    }

    /// D-04 order, every caller (V-37): a gate a process waits on opens through
    /// `Gates::open_registered`, so a writer that sees the gate file always finds its
    /// waiter registered. No production code in either crate calls the separate steps.
    /// There is no allowlist: a production opener that is not a waiter has no case
    /// today, and adding one is a reviewed change to this scan.
    ///
    /// Limit: integration tests under `crates/*/tests/` are test code and not scanned,
    /// and a call that does not spell the `Gates::` path is not seen.
    #[test]
    fn no_production_code_opens_a_gate_except_through_open_registered() {
        assert!(
            include_str!("main.rs").contains("#[cfg(test)]\nmod test_support;"),
            "the CLI's test_support.rs is skipped only while it is declared test-only"
        );
        assert!(
            include_str!("../../devflow-core/src/lib.rs")
                .contains("#[cfg(any(test, feature = \"test-support\"))]\npub mod test_support;"),
            "core's test_support.rs is skipped only while it is declared test-only"
        );
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let files: Vec<std::path::PathBuf> = ["devflow-cli/src", "devflow-core/src"]
            .iter()
            .flat_map(|source_root| rust_files_under(&crates.join(source_root)))
            .collect();
        let labelled: Vec<(String, &std::path::PathBuf)> = files
            .iter()
            .map(|file| {
                let label = file.strip_prefix(crates).unwrap().display().to_string();
                (label, file)
            })
            .collect();
        for expected in [
            "devflow-cli/src/pipeline_gate.rs",
            "devflow-core/src/workflow.rs",
        ] {
            assert!(
                labelled.iter().any(|(label, _)| label == expected),
                "the walk must reach {expected}"
            );
        }
        let skipped = [
            "devflow-core/src/gates.rs",
            "devflow-cli/src/test_support.rs",
            "devflow-core/src/test_support.rs",
        ];
        let real: Vec<String> = labelled
            .iter()
            .filter(|(label, _)| !skipped.contains(&label.as_str()))
            .flat_map(|(label, file)| {
                unregistered_gate_open_violations(label, &std::fs::read_to_string(file).unwrap())
            })
            .collect();
        assert_eq!(
            real,
            Vec::<String>::new(),
            "production gate openers outside open_registered"
        );

        assert_eq!(
            unregistered_gate_open_violations(
                "planted",
                "fn wait() {\n    Gates::write_gate(root, phase, stage, ctx)?;\n}\n"
            ),
            vec!["planted: line 2: Gates::write_gate(root, phase, stage, ctx)?;"],
            "a planted write_gate"
        );
        assert_eq!(
            unregistered_gate_open_violations(
                "planted",
                "    let opened = Gates::open_gate(root, phase, stage, ctx)?;\n"
            ),
            vec!["planted: line 1: let opened = Gates::open_gate(root, phase, stage, ctx)?;"],
            "a planted open_gate"
        );
        assert_eq!(
            unregistered_gate_open_violations(
                "planted",
                "    let registration = Gates::register_waiter(root, phase, stage, &id)?;\n"
            ),
            vec![
                "planted: line 1: let registration = Gates::register_waiter(root, phase, stage, &id)?;"
            ],
            "a planted register_waiter"
        );
        for (source, shape) in [
            (
                "    let (opened, registration) = Gates::open_registered(root, phase, stage, ctx)?;\n",
                "the one permitted opener",
            ),
            (
                "// Gates::write_gate(root, phase, stage, ctx)?;\n",
                "a commented-out call",
            ),
            (
                "mod tests {\n    Gates::open_gate(root, phase, stage, ctx);\n",
                "a call after `mod tests {`",
            ),
        ] {
            assert_eq!(
                unregistered_gate_open_violations("planted", source),
                Vec::<String>::new(),
                "{shape}"
            );
        }
    }

    /// V-11 (criterion 3): no production code in this crate names `save_state`, whose
    /// whole-snapshot write would bring back the lost update `update_state` removes.
    /// Only test code is excluded: `mod tests` and the `test_support` module, whose
    /// test-only declaration is asserted here, so no production file is allowlisted.
    #[test]
    fn no_cli_production_code_saves_state_outside_update_state() {
        const SOURCE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        assert!(
            include_str!("main.rs").contains("#[cfg(test)]\nmod test_support;"),
            "test_support.rs is skipped only while it is declared test-only"
        );
        let root = std::path::Path::new(SOURCE_ROOT);
        let files = rust_files_under(root);
        let relative: Vec<String> = files
            .iter()
            .map(|file| file.strip_prefix(root).unwrap().display().to_string())
            .collect();
        for expected in ["main.rs", "pipeline_outcomes.rs"] {
            assert!(
                relative.iter().any(|file| file == expected),
                "the walk must reach {expected}: {relative:?}"
            );
        }
        let real: Vec<String> = files
            .iter()
            .zip(&relative)
            .filter(|(_, relative)| relative.as_str() != "test_support.rs")
            .flat_map(|(file, relative)| {
                save_state_violations(relative, &std::fs::read_to_string(file).unwrap())
            })
            .collect();
        assert_eq!(
            real,
            Vec::<String>::new(),
            "production save_state references"
        );

        let call = "fn f() {\n    workflow::save_state(&state)?;\n}\n";
        assert_eq!(
            save_state_violations("planted.rs", call),
            vec!["planted.rs: line 2: save_state"],
            "a planted call"
        );
        let value = "    match publish_record(state, published, workflow::save_state)? {\n";
        assert_eq!(
            save_state_violations("planted.rs", value),
            vec!["planted.rs: line 1: save_state"],
            "the function passed as a value"
        );
        for (source, shape) in [
            (
                "let n = save_state_violations(p, s);",
                "a longer identifier",
            ),
            (
                "let n = my_save_state(p, s);",
                "a longer identifier, on the left",
            ),
            ("// workflow::save_state(&state)?;", "a commented-out call"),
            (
                "fn f() {}\nmod tests {\n    workflow::save_state(&state)?;\n}\n",
                "a call after `mod tests {`",
            ),
        ] {
            assert_eq!(
                save_state_violations("planted.rs", source),
                Vec::<String>::new(),
                "{shape}"
            );
        }
    }

    // ---- 51-10: the SUPV-01 evidence file is checked row by row (R2-10) ----

    /// The reads `51-SUPV01-EVIDENCE.md`'s `## Trace` table must hold a row for,
    /// keyed by the backticked name in the row's `Read` cell.
    const SUPV01_REQUIRED_READS: [&str; 7] = [
        "wait_for_publish",
        "successor_runs_on",
        "commit_pending_record",
        "remove_abandoned_liveness_file",
        "recorded_live_socket",
        "held_liveness_holder",
        "pending_live_socket",
    ];

    /// The `## Trace` table's columns, in order.
    const SUPV01_COLUMNS: [&str; 7] = [
        "Read",
        "Location",
        "Probes",
        "Unanswerable probe",
        "Ordinary launch",
        "Covered by",
        "Outcome",
    ];

    /// The `## Conclusion` lines complete mode requires, each naming a test path.
    const SUPV01_CONCLUSION_LINES: [&str; 2] = ["Failover arm: ", "Cron arm: "];

    /// The further `##` headings complete mode requires (51-13), besides `## Conclusion`.
    const SUPV01_COMPLETE_HEADINGS: [&str; 5] = [
        "## Experiments: post-save probe and pending-record lock probe",
        "## Coverage check",
        "## What this does not establish",
        "## D-13: natural rate limit during the Phase 51 dogfood run",
        "## Backlog findings",
    ];

    /// Every violation of the SUPV-01 evidence file's row contract in `text`,
    /// with cited files and tests resolved against `root` (R2-10). Trace mode
    /// (`complete == false`) accepts the pending marker `51-13`; complete mode
    /// (51-13) rejects it and requires the two conclusion lines and every
    /// heading in [`SUPV01_COMPLETE_HEADINGS`]. Never panics on
    /// an unreadable file: that is a violation naming its path.
    fn supv01_evidence_violations(text: &str, complete: bool, root: &Path) -> Vec<String> {
        let mut violations = Vec::new();
        let rows = supv01_trace_rows(text, &mut violations);
        for read in SUPV01_REQUIRED_READS {
            if !rows.iter().any(|row| row[0] == read) {
                violations.push(format!("`{read}`: no row in the `## Trace` table"));
            }
        }
        for row in &rows {
            supv01_row_violations(row, complete, root, &mut violations);
        }
        if complete {
            for heading in SUPV01_COMPLETE_HEADINGS {
                if markdown_section(text, heading).is_none() {
                    violations.push(format!("no `{heading}` heading"));
                }
            }
            let conclusion = markdown_section(text, "## Conclusion").unwrap_or_default();
            for label in SUPV01_CONCLUSION_LINES {
                let Some(line) = conclusion.lines().find(|line| line.starts_with(label)) else {
                    violations.push(format!("conclusion: no line starting `{}`", label.trim()));
                    continue;
                };
                let tests = test_paths_in(line);
                if tests.is_empty() {
                    violations.push(format!("conclusion `{}`: names no test path", label.trim()));
                }
                for test in tests {
                    if let Some(why) = unresolved_test(root, &test) {
                        violations.push(format!("conclusion `{}`: {why}", label.trim()));
                    }
                }
            }
        }
        violations
    }

    /// The lines under `heading` up to the next `## ` heading, or `None` when
    /// no line is exactly `heading`.
    fn markdown_section<'a>(text: &'a str, heading: &str) -> Option<String> {
        let mut lines = text.lines().skip_while(|line| line.trim_end() != heading);
        lines.next()?;
        let body: Vec<&'a str> = lines.take_while(|line| !line.starts_with("## ")).collect();
        Some(body.join("\n"))
    }

    /// The `## Trace` table's data rows as `[read, cells...]`: the row's
    /// backticked read name, then its seven cells. A missing section, a wrong
    /// header or a row with another cell count is a violation.
    fn supv01_trace_rows(text: &str, violations: &mut Vec<String>) -> Vec<[String; 8]> {
        let Some(section) = markdown_section(text, "## Trace") else {
            violations.push("no `## Trace` heading".to_string());
            return Vec::new();
        };
        let cells = |line: &str| -> Vec<String> {
            let inner = line.trim().trim_start_matches('|').trim_end_matches('|');
            inner
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect()
        };
        let mut table = section
            .lines()
            .filter(|line| line.trim_start().starts_with('|'));
        let Some(header) = table.next() else {
            violations.push(format!(
                "`## Trace` holds no table with the columns {}",
                SUPV01_COLUMNS.join(" | ")
            ));
            return Vec::new();
        };
        if cells(header) != SUPV01_COLUMNS {
            violations.push(format!(
                "`## Trace` table header is `{}`, not `{}`",
                header.trim(),
                SUPV01_COLUMNS.join(" | ")
            ));
            return Vec::new();
        }
        let mut rows = Vec::new();
        for line in table {
            let row = cells(line);
            if row
                .iter()
                .all(|cell| !cell.is_empty() && cell.chars().all(|c| matches!(c, '-' | ':')))
            {
                continue;
            }
            let Ok(row) = <[String; 7]>::try_from(row.clone()) else {
                violations.push(format!(
                    "`## Trace` row `{}` has {} cells, not 7",
                    line.trim(),
                    row.len()
                ));
                continue;
            };
            let read = row[0].split('`').nth(1).unwrap_or_default().to_string();
            let [a, b, c, d, e, f, g] = row;
            rows.push([read, a, b, c, d, e, f, g]);
        }
        rows
    }

    /// The tokens of `text` made of identifier, path and `::` characters.
    fn evidence_tokens(text: &str) -> impl Iterator<Item = &str> {
        text.split(|c: char| {
            !(c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '/' | '-'))
        })
        .map(|token| token.trim_end_matches(['.', ':', ',', ';']))
        .filter(|token| !token.is_empty())
    }

    /// Whether `token` is a test path: `<module>(::<module>)*::tests::<name>`,
    /// every segment a lowercase identifier.
    fn is_test_path(token: &str) -> bool {
        let Some((module, name)) = token.split_once("::tests::") else {
            return false;
        };
        let ident = |segment: &str| {
            segment
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
                && segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        };
        !module.is_empty() && module.split("::").all(ident) && ident(name)
    }

    /// Every test path in `text`.
    fn test_paths_in(text: &str) -> Vec<String> {
        evidence_tokens(text)
            .filter(|token| is_test_path(token))
            .map(str::to_string)
            .collect()
    }

    /// Every `crates/<path>.rs:<line>` in `text`, as `(path, line)`.
    fn locations_in(text: &str) -> Vec<(String, usize)> {
        evidence_tokens(text)
            .filter(|token| token.starts_with("crates/"))
            .filter_map(|token| {
                let (path, line) = token.split_once(".rs:")?;
                let line = line.parse().ok()?;
                Some((format!("{path}.rs"), line))
            })
            .collect()
    }

    /// The lines of `source` before its first line that trims to `mod tests {`.
    fn non_test_part(source: &str) -> String {
        source
            .lines()
            .take_while(|line| line.trim() != "mod tests {")
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Why `test` does not resolve under `root`, or `None` when one of its
    /// module's files declares `fn <name>(` after its first `mod tests {` line.
    fn unresolved_test(root: &Path, test: &str) -> Option<String> {
        let (module, name) = test.split_once("::tests::")?;
        let relative = module.replace("::", "/");
        let mut seen = Vec::new();
        for krate in ["devflow-cli", "devflow-core"] {
            let path = format!("crates/{krate}/src/{relative}.rs");
            let file = root.join(&path);
            if !file.exists() {
                continue;
            }
            let source = match std::fs::read_to_string(&file) {
                Ok(source) => source,
                Err(err) => return Some(format!("test {test}: cannot read {path}: {err}")),
            };
            let declared = source
                .lines()
                .skip_while(|line| line.trim() != "mod tests {")
                .any(|line| line.contains(&format!("fn {name}(")));
            if declared {
                return None;
            }
            seen.push(path);
        }
        Some(if seen.is_empty() {
            format!("test {test}: no module file crates/devflow-{{cli,core}}/src/{relative}.rs")
        } else {
            format!(
                "test {test} is not declared as `fn {name}(` after `mod tests {{` in {}",
                seen.join(" or ")
            )
        })
    }

    /// One trace row's violations: its location, coverage and outcome cells,
    /// each citation resolved against `root`.
    fn supv01_row_violations(
        row: &[String; 8],
        complete: bool,
        root: &Path,
        violations: &mut Vec<String>,
    ) {
        let [read, _, location, _, _, _, covered, outcome] = row;
        let read = if read.is_empty() {
            "<unnamed>"
        } else {
            read.as_str()
        };
        let locations = locations_in(location);
        if locations.is_empty() {
            violations.push(format!(
                "`{read}`: Location cites no `crates/…/*.rs:<line>`"
            ));
        }
        let mut sources = Vec::new();
        let mut all_present = true;
        for (path, line) in &locations {
            let file = root.join(path);
            if !file.exists() {
                violations.push(format!("`{read}`: cited file {path} does not exist"));
                all_present = false;
                continue;
            }
            match std::fs::read_to_string(&file) {
                Ok(source) => {
                    let count = source.lines().count();
                    if *line == 0 || *line > count {
                        violations.push(format!(
                            "`{read}`: {path}:{line} is past the file's end ({count} lines)"
                        ));
                    }
                    sources.push(source);
                }
                Err(err) => {
                    violations.push(format!("`{read}`: cannot read {path}: {err}"));
                    all_present = false;
                }
            }
        }
        if all_present
            && !sources.is_empty()
            && !sources
                .iter()
                .any(|source| non_test_part(source).contains(read))
        {
            violations.push(format!(
                "`{read}`: no cited file names `{read}` in its non-test code"
            ));
        }

        let covered = covered.trim().trim_matches('`').trim();
        let pending = covered == "51-13";
        let gap = covered
            .strip_prefix("gap:")
            .is_some_and(|reason| !reason.trim().is_empty());
        if !pending && !gap {
            let tests: Vec<&str> = covered
                .split(',')
                .map(|test| test.trim().trim_matches('`'))
                .collect();
            if tests.iter().all(|test| is_test_path(test)) {
                for test in tests {
                    if let Some(why) = unresolved_test(root, test) {
                        violations.push(format!("`{read}`: {why}"));
                    }
                }
            } else {
                violations.push(format!(
                    "`{read}`: Covered by `{covered}` is neither a test path, `gap: <reason>` \
                     nor `51-13`"
                ));
            }
        }
        if outcome.trim().is_empty() {
            violations.push(format!("`{read}`: empty Outcome"));
        }
        if complete && (pending || outcome.trim().starts_with("pending")) {
            violations.push(format!("`{read}`: still pending (51-13) in complete mode"));
        }
    }

    /// The `[read, location, covered by, outcome]` rows a planted trace holds,
    /// rendered into the three 51-10 headings with the table's other cells
    /// filled.
    fn planted_trace(rows: &[[&str; 4]], conclusion: &str) -> String {
        let mut text = String::from("# SUPV-01 evidence\n\n## Trace\n\n");
        text.push_str(&format!("| {} |\n", SUPV01_COLUMNS.join(" | ")));
        text.push_str(&format!("|{}\n", "---|".repeat(SUPV01_COLUMNS.len())));
        for [read, location, covered, outcome] in rows {
            text.push_str(&format!(
                "| `{read}` | {location} | successor | not published | yes | {covered} | {outcome} |\n"
            ));
        }
        text.push_str(
            "\n## Experiments: publish wait and cron-arm read\n\nPlanted.\n\n\
             ## Never-silent findings from 51-10\n\nnone\n",
        );
        text.push_str(conclusion);
        text
    }

    /// A planted source tree: the three files the trace cites, each naming the
    /// reads that file really holds above its `mod tests {` line, with test
    /// fns after it; `above_tests_only` is declared only above it, and the
    /// `pipeline_launch.rs` stand-in names `held_liveness_holder` only inside
    /// its test module.
    fn planted_source_tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for (path, body) in [
            (
                "crates/devflow-cli/src/pipeline_launch.rs",
                "fn successor_runs_on() {}\nfn commit_pending_record() {}\nfn above_tests_only() {}\n\
                 mod tests {\n    fn planted_test() {}\n    const READS: &str = \"held_liveness_holder\";\n}\n",
            ),
            (
                "crates/devflow-cli/src/commands.rs",
                "fn remove_abandoned_liveness_file() {}\nfn recorded_live_socket() {}\n\
                 fn held_liveness_holder() {}\nfn pending_live_socket() {}\n\
                 mod tests {\n    fn commands_test() {}\n}\n",
            ),
            (
                "crates/devflow-core/src/monitor.rs",
                "fn wait_for_publish() {}\nmod tests {\n    fn monitor_test() {}\n}\n",
            ),
        ] {
            let file = root.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, body).unwrap();
        }
        root
    }

    const PL: &str = "crates/devflow-cli/src/pipeline_launch.rs";
    const CMDS: &str = "crates/devflow-cli/src/commands.rs";
    const MON: &str = "crates/devflow-core/src/monitor.rs";

    /// The well-formed trace-mode rows (b): every citation resolves in
    /// [`planted_source_tree`], and five rows are left to 51-13.
    fn planted_trace_rows() -> Vec<[String; 4]> {
        let pending = |read: &str, location: String| {
            [
                read.to_string(),
                location,
                "51-13".to_string(),
                "pending (51-13)".to_string(),
            ]
        };
        vec![
            [
                "wait_for_publish".to_string(),
                format!("{MON}:1"),
                "monitor::tests::monitor_test".to_string(),
                "refused".to_string(),
            ],
            pending("successor_runs_on", format!("{PL}:1")),
            pending("commit_pending_record", format!("{PL}:2")),
            pending("remove_abandoned_liveness_file", format!("{CMDS}:1")),
            [
                "recorded_live_socket".to_string(),
                format!("{CMDS}:2"),
                "commands::tests::commands_test, pipeline_launch::tests::planted_test".to_string(),
                "refused alike".to_string(),
            ],
            pending("held_liveness_holder", format!("{CMDS}:3")),
            pending("pending_live_socket", format!("{CMDS}:4")),
        ]
    }

    /// The complete-mode rows (c): every row a test path or a gap.
    fn planted_complete_rows() -> Vec<[String; 4]> {
        planted_trace_rows()
            .into_iter()
            .map(|[read, location, covered, outcome]| {
                if covered == "51-13" {
                    let covered = if read == "remove_abandoned_liveness_file" {
                        "gap: decides only a lock file's deletion".to_string()
                    } else {
                        "pipeline_launch::tests::planted_test".to_string()
                    };
                    [read, location, covered, "measured".to_string()]
                } else {
                    [read, location, covered, outcome]
                }
            })
            .collect()
    }

    fn render(rows: &[[String; 4]], conclusion: &str) -> String {
        let rows: Vec<[&str; 4]> = rows
            .iter()
            .map(|[a, b, c, d]| [a.as_str(), b.as_str(), c.as_str(), d.as_str()])
            .collect();
        planted_trace(&rows, conclusion)
    }

    const PLANTED_CONCLUSION: &str = "\n## Conclusion\n\nFailover arm: pipeline_launch::tests::planted_test.\n\
                                      Cron arm: commands::tests::commands_test.\n";

    /// The five further sections complete mode requires (51-13), planted with bodies.
    const PLANTED_COMPLETE_SECTIONS: &str = "\n## Experiments: post-save probe and pending-record lock probe\n\nPlanted.\n\
         \n## Coverage check\n\nPlanted.\n\n## What this does not establish\n\nPlanted.\n\
         \n## D-13: natural rate limit during the Phase 51 dogfood run\n\nPlanted.\n\
         \n## Backlog findings\n\nnone\n";

    /// `rows` rendered as a complete-mode text: the five further sections, then `conclusion`.
    fn render_complete(rows: &[[String; 4]], conclusion: &str) -> String {
        render(rows, &format!("{PLANTED_COMPLETE_SECTIONS}{conclusion}"))
    }

    /// `rows` with the row for `read` replaced by `edit` applied to it.
    fn with_row(
        rows: Vec<[String; 4]>,
        read: &str,
        edit: impl Fn(&mut [String; 4]),
    ) -> Vec<[String; 4]> {
        rows.into_iter()
            .map(|mut row| {
                if row[0] == read {
                    edit(&mut row);
                }
                row
            })
            .collect()
    }

    /// Assert `violations` is exactly one, containing every fragment.
    fn exactly_one(violations: &[String], fragments: &[&str], case: &str) {
        assert_eq!(violations.len(), 1, "{case}: {violations:#?}");
        for fragment in fragments {
            assert!(
                violations[0].contains(fragment),
                "{case}: `{fragment}` missing from {violations:#?}"
            );
        }
    }

    /// R2-10: the evidence checker reads rows, not headings, and resolves every
    /// citation. Planted texts against a planted source tree, never the real
    /// one; runs in every build.
    #[test]
    fn the_supv01_evidence_check_rejects_a_stub() {
        let tree = planted_source_tree();
        let root = tree.path();
        let check = |text: &str, complete: bool| supv01_evidence_violations(text, complete, root);

        // (a) The shape the former `rg -q` gate accepted: headings and bare identifiers.
        let stub = "## Trace\n\nsuccessor_runs_on remove_abandoned_liveness_file possibly_live_sockets\n\n\
                    ## Experiments: publish wait and cron-arm read\n\n## Never-silent findings from 51-10\n";
        let violations = check(stub, false);
        for read in SUPV01_REQUIRED_READS {
            assert!(
                violations
                    .iter()
                    .any(|v| v.contains(&format!("`{read}`")) && v.contains("no row")),
                "(a) `{read}` is reported as missing its row: {violations:#?}"
            );
        }

        // (b) Well formed, five rows left to 51-13, no conclusion.
        let trace = render(&planted_trace_rows(), "");
        assert_eq!(check(&trace, false), Vec::<String>::new(), "(b) trace mode");
        let violations = check(&trace, true);
        assert_eq!(violations.len(), 12, "(b) complete mode: {violations:#?}");
        for heading in SUPV01_COMPLETE_HEADINGS {
            assert!(
                violations
                    .iter()
                    .any(|v| v.contains(heading) && v.contains("heading")),
                "(b) the missing `{heading}` heading is reported: {violations:#?}"
            );
        }
        for read in [
            "successor_runs_on",
            "commit_pending_record",
            "remove_abandoned_liveness_file",
            "held_liveness_holder",
            "pending_live_socket",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|v| v.contains(&format!("`{read}`")) && v.contains("pending")),
                "(b) `{read}` is reported pending: {violations:#?}"
            );
        }
        for label in SUPV01_CONCLUSION_LINES {
            assert!(
                violations
                    .iter()
                    .any(|v| v.contains(label.trim()) && v.contains("no line")),
                "(b) the missing `{label}` line is reported: {violations:#?}"
            );
        }

        // (c) Complete, every citation resolving.
        let complete_text = render_complete(&planted_complete_rows(), PLANTED_CONCLUSION);
        assert_eq!(
            check(&complete_text, false),
            Vec::<String>::new(),
            "(c) trace"
        );
        assert_eq!(
            check(&complete_text, true),
            Vec::<String>::new(),
            "(c) complete"
        );

        // (d) A Location without `:<line>`.
        let rows = with_row(planted_trace_rows(), "wait_for_publish", |row| {
            row[1] = MON.to_string();
        });
        exactly_one(
            &check(&render(&rows, ""), false),
            &["`wait_for_publish`", "Location"],
            "(d)",
        );

        // (e) A Covered by cell of free words.
        let rows = with_row(planted_trace_rows(), "wait_for_publish", |row| {
            row[2] = "the publish wait test".to_string();
        });
        exactly_one(
            &check(&render(&rows, ""), false),
            &["`wait_for_publish`", "Covered by"],
            "(e)",
        );

        // (f) A conclusion whose `Failover arm: ` line names no test path.
        let no_path = "\n## Conclusion\n\nFailover arm: it works.\n\
                       Cron arm: commands::tests::commands_test.\n";
        exactly_one(
            &check(&render_complete(&planted_complete_rows(), no_path), true),
            &["Failover arm", "no test path"],
            "(f)",
        );

        // (g) Fabricated: well formed in every cell, nothing resolving.
        let fabricated: Vec<[String; 4]> = SUPV01_REQUIRED_READS
            .iter()
            .map(|read| {
                [
                    (*read).to_string(),
                    "crates/x.rs:1".to_string(),
                    "pipeline_launch::tests::no_such_test".to_string(),
                    "measured".to_string(),
                ]
            })
            .collect();
        let fabricated_conclusion = "\n## Conclusion\n\nFailover arm: pipeline_launch::tests::no_such_test\n\
                                     Cron arm: pipeline_launch::tests::no_such_test\n";
        // Given the five further headings as well, so they cannot make it pass.
        let fabricated = render_complete(&fabricated, fabricated_conclusion);
        let trace_mode = check(&fabricated, false);
        assert_eq!(trace_mode.len(), 14, "(g) trace mode: {trace_mode:#?}");
        let complete_mode = check(&fabricated, true);
        assert_eq!(
            complete_mode.len(),
            16,
            "(g) complete mode: {complete_mode:#?}"
        );
        for read in SUPV01_REQUIRED_READS {
            for kind in ["does not exist", "no_such_test"] {
                assert_eq!(
                    trace_mode
                        .iter()
                        .filter(|v| v.contains(&format!("`{read}`")) && v.contains(kind))
                        .count(),
                    1,
                    "(g) `{read}` has one `{kind}` violation: {trace_mode:#?}"
                );
            }
        }
        for label in SUPV01_CONCLUSION_LINES {
            assert!(
                complete_mode
                    .iter()
                    .any(|v| v.contains(label.trim()) && v.contains("no_such_test")),
                "(g) the `{label}` line's test is unresolved: {complete_mode:#?}"
            );
        }

        // (h) A cited line past the file's last line.
        let rows = with_row(planted_trace_rows(), "wait_for_publish", |row| {
            row[1] = format!("{MON}:99");
        });
        exactly_one(
            &check(&render(&rows, ""), false),
            &["`wait_for_publish`", ":99", "past"],
            "(h)",
        );

        // (i) A test declared only above `mod tests {`.
        let rows = with_row(planted_trace_rows(), "wait_for_publish", |row| {
            row[2] = "pipeline_launch::tests::above_tests_only".to_string();
        });
        exactly_one(
            &check(&render(&rows, ""), false),
            &["`wait_for_publish`", "above_tests_only"],
            "(i)",
        );

        // (j) The only cited file exists but never names the read.
        let rows = with_row(planted_trace_rows(), "wait_for_publish", |row| {
            row[1] = format!("{CMDS}:1");
        });
        exactly_one(
            &check(&render(&rows, ""), false),
            &["`wait_for_publish`", "names"],
            "(j)",
        );

        // (k) The only cited file names the read only inside its test module.
        let rows = with_row(planted_trace_rows(), "held_liveness_holder", |row| {
            row[1] = format!("{PL}:1");
        });
        exactly_one(
            &check(&render(&rows, ""), false),
            &["`held_liveness_holder`", "names"],
            "(k)",
        );

        // (k) in complete mode: the same row in an otherwise complete text.
        let rows = with_row(planted_complete_rows(), "held_liveness_holder", |row| {
            row[1] = format!("{PL}:1");
        });
        exactly_one(
            &check(&render_complete(&rows, PLANTED_CONCLUSION), true),
            &["`held_liveness_holder`", "names"],
            "(k) complete",
        );

        // (l) A complete text missing `## Coverage check`.
        let without_coverage = complete_text.replace("\n## Coverage check\n\nPlanted.\n", "");
        assert_ne!(
            without_coverage, complete_text,
            "(l) the heading was removed"
        );
        exactly_one(
            &check(&without_coverage, true),
            &["## Coverage check", "heading"],
            "(l)",
        );

        // (m) A complete text whose conclusion has no `Cron arm: ` line.
        let no_cron = "\n## Conclusion\n\nFailover arm: pipeline_launch::tests::planted_test.\n";
        exactly_one(
            &check(&render_complete(&planted_complete_rows(), no_cron), true),
            &["Cron arm", "no line"],
            "(m)",
        );
    }

    /// R2-10 on the real file: `51-SUPV01-EVIDENCE.md`'s trace rows are
    /// complete and every citation resolves against the workspace root.
    #[test]
    #[ignore = "reads Phase 51's .planning evidence file, which develop and main do not carry; the phase gate runs it with --run-ignored only"]
    fn the_supv01_evidence_trace_rows_are_complete() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.join("../..");
        let path = root.join(".planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let violations = supv01_evidence_violations(&text, false, &root);
        assert!(violations.is_empty(), "{}: {violations:#?}", path.display());
    }

    /// R2-10 (51-13) on the real file, complete mode: no row is pending, every outcome
    /// is filled, every heading 51-13 adds is there, both conclusion lines name a test
    /// path, and every citation resolves against the workspace root.
    #[test]
    #[ignore = "reads Phase 51's .planning evidence file, which develop and main do not carry; the phase gate runs it with --run-ignored only"]
    fn the_supv01_evidence_is_complete() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.join("../..");
        let path = root.join(".planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let violations = supv01_evidence_violations(&text, true, &root);
        assert!(violations.is_empty(), "{}: {violations:#?}", path.display());
    }

    // ---- 51-13: the post-save successor probe answered Gone (D-12, B-2, R2-8) ----

    // ---- 52-32: resume and takeover decide arming the same way ----

    /// A `claude` stub that prints a rate-limit envelope whose retry time is long
    /// past, so an armed timer would answer the gate at once.
    fn past_reset_claude_script() -> String {
        "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
         printf '%s\\n' '{\"type\":\"result\",\"subtype\":\"error_rate_limit\",\"retry_after\":\"1700000000\"}'\n\
         exit 0\n"
            .to_string()
    }

    /// Run `name` in a hermetic child whose `PATH` holds [`past_reset_claude_script`]
    /// as `claude`, with a 20 s gate timeout so no waiting runner outlives the test by
    /// long. `true` in the parent,
    /// whose work is done.
    fn parent_of_a_past_reset_claude_child(name: &str) -> bool {
        if devflow_core::test_support::in_child_test(name) {
            return false;
        }
        let path_dir = agent_free_dir_with_agent_stub("claude");
        let stub = path_dir.path().join("claude");
        devflow_core::test_support::write_executable(&stub, &past_reset_claude_script());
        let output = crate::test_support::run_hermetic_test_in_child(
            name,
            path_dir.path(),
            &[("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("20"))],
        );
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
        true
    }

    /// A Code-stage Claude run of `phase` at `root`, Supervise, with a recorded
    /// `Confirmed` canary and `used` as its automatic-relaunch flag, saved.
    fn save_rate_limit_run(root: &Path, phase: PhaseId, used: bool) -> State {
        init_repo(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        state.rate_limit_auto_relaunch_used = used;
        workflow::save_state(&state).unwrap();
        state
    }

    /// The phase's open Code gate file, once it exists (bounded).
    fn the_open_code_gate(root: &Path, phase: PhaseId) -> devflow_core::gates::GateFile {
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                crate::pipeline_gate::read_gate_file(root, phase, Stage::Code).is_some()
            }),
            "no Code gate opened; events:\n{}",
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default()
        );
        crate::pipeline_gate::read_gate_file(root, phase, Stage::Code).unwrap()
    }

    /// How long a row watches for a timer that must not fire.
    const NO_TIMER_WITHIN: Duration = Duration::from_secs(3);

    /// Assert the run at `root` waits at a rate-limit gate with its not-before
    /// time recorded and no timer: the arming function says so over the state
    /// and gate on disk, and over a bounded wait the timer never fires and
    /// nothing launches beyond `launches`.
    fn assert_waits_with_no_timer(root: &Path, phase: PhaseId, launches: usize, row: &str) {
        let gate = the_open_code_gate(root, phase);
        assert_eq!(
            gate.gate_kind,
            Some(devflow_core::gates::GateKind::RateLimit),
            "{row}: {gate:?}"
        );
        assert!(gate.not_before.is_some(), "{row}: not_before is recorded");
        std::thread::sleep(NO_TIMER_WITHIN);
        let state = workflow::load_state(root, phase).unwrap();
        assert!(
            state.rate_limit_auto_relaunch_used,
            "{row}: the flag stays spent"
        );
        assert_eq!(
            crate::pipeline_gate::rate_limit_timer_deadline(&state, &gate),
            None,
            "{row}: no timer is armed"
        );
        assert!(
            events_named(root, "rate_limit_timer_fired").is_empty(),
            "{row}: the timer never fired"
        );
        assert!(
            events_named(root, "gate_resolved").is_empty(),
            "{row}: nothing answered the gate"
        );
        assert_eq!(stage_launched_count(root, phase), launches, "{row}");
        assert!(
            crate::pipeline_gate::read_gate_file(root, phase, Stage::Code).is_some(),
            "{row}: the gate is still open"
        );
    }

    /// Wait, bounded, until the timer has answered `root`'s gate.
    fn assert_the_timer_answers(root: &Path, row: &str) {
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                events_named(root, "rate_limit_timer_fired")
                    .iter()
                    .any(|event| event["outcome"] == "answered")
            }),
            "{row}: the armed timer never answered; events:\n{}",
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default()
        );
    }

    /// D-27.3 / D-38: a resume after the automatic relaunch was used does not
    /// re-arm it: the denied relaunch opens the gate with its not-before time
    /// and no timer. The same after a stop (`stopped` with a reason, as `devflow
    /// stop` leaves it) and then a resume. Control: the same resume over a run
    /// whose flag is still false arms the timer, which answers the gate.
    #[test]
    fn resume_does_not_rearm_the_auto_relaunch() {
        const NAME: &str = "pipeline_launch::tests::resume_does_not_rearm_the_auto_relaunch";
        if parent_of_a_past_reset_claude_child(NAME) {
            return;
        }
        let _guard = env_lock();
        for (offset, (stopped, used)) in [(false, true), (true, true), (false, false)]
            .into_iter()
            .enumerate()
        {
            let row = format!("stopped {stopped}, flag used {used}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(5160 + u32::try_from(offset).unwrap());
            let mut state = save_rate_limit_run(root, phase, used);
            if stopped {
                state.stopped = true;
                state.stop_reason = Some("stopped by devflow stop".to_string());
                workflow::save_state(&state).unwrap();
            }
            // The resumed stage's denial and gate happen in the runner the resume launches.
            crate::service::double::set_full_runner(true);
            let result = resume(root, phase, None);
            let _runners = crate::service::double::LaunchedRunners;
            result.unwrap();
            if used {
                assert_waits_with_no_timer(root, phase, 1, &row);
            } else {
                assert_the_timer_answers(root, &row);
            }
        }
    }

    // ---- 52.2-05: the carried stop orderings, on the runner ----

    /// The release path `test_hold` waits on, from `DEVFLOW_TEST_HOLD`.
    fn hold_release_path() -> Option<PathBuf> {
        let hold = std::env::var(HOLD_ENV).ok()?;
        let (_, path) = hold.split_once(':')?;
        Some(PathBuf::from(path))
    }

    /// The `claude` stub a runner stop-ordering child runs: it answers `--version` and
    /// otherwise prints `marker` and exits 0.
    fn runner_claude_dir(marker: &str) -> tempfile::TempDir {
        let path_dir = agent_free_dir_with_agent_stub("claude");
        std::fs::write(
            path_dir.path().join("claude"),
            format!(
                "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\nprintf '%s\\n' '{marker}'\n"
            ),
        )
        .unwrap();
        path_dir
    }

    /// The repo a runner stop-ordering child runs in: `repo` beside its hold's release
    /// file, which the parent owns.
    fn runner_child_root() -> PathBuf {
        let release = hold_release_path().expect("the parent names the hold");
        release.parent().expect("a hold dir").join("repo")
    }

    /// The child half of a runner stop-ordering test: a Claude/Supervise state at `stage`
    /// in [`runner_child_root`], run by [`run_runner`], which must return `expected`.
    fn run_runner_in_stop_child(phase: PhaseId, stage: Stage, expected: i32, what: &str) {
        let _guard = env_lock();
        let root = runner_child_root();
        std::fs::create_dir_all(&root).unwrap();
        init_repo(&root);
        std::fs::write(root.join(".gitignore"), ".devflow/\n").unwrap();
        let git = |args: &[&str]| {
            let output = devflow_core::test_support::git_command(&root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?}: {output:?}");
        };
        git(&["add", ".gitignore"]);
        git(&["commit", "-q", "-m", "runner stop fixture"]);
        crate::test_support::phase_dir_fixture(&root, phase);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.clone());
        state.stage = stage;
        state.verification_baseline_captured = true;
        // A stub `claude` cannot answer the stream launch's canary.
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        state.stop_until = None;
        workflow::save_state(&state).unwrap();
        let code = run_runner(&root, phase).expect("the runner returns an exit code");
        let events =
            std::fs::read_to_string(devflow_core::events::events_path(&root)).unwrap_or_default();
        assert_eq!(
            code, expected,
            "{what}: the runner's exit code; events:\n{events}"
        );
    }

    /// The parent half of a runner stop-ordering test: run `name` in a hermetic child
    /// whose `claude` prints `marker`, with GSD answering `gsd` and `DEVFLOW_TEST_HOLD` at
    /// `hold`. With `approve`, approve the Validate gate once it opens. Once the hold is
    /// reached, SIGTERM the child (the runner) and release it; return the hold dir and
    /// the child's repo root once the child passed.
    fn run_runner_stop_child(
        name: &str,
        hold: &str,
        marker: &str,
        gsd: &[devflow_core::gsd_reader::GsdStatus],
        approve: Option<PhaseId>,
    ) -> (tempfile::TempDir, PathBuf) {
        let path_dir = runner_claude_dir(marker);
        let hold_dir = tempfile::Builder::new()
            .prefix("dfh")
            .tempdir_in("/tmp")
            .unwrap();
        let release = hold_dir.path().join("release");
        let root = hold_dir.path().join("repo");
        let gsd_dir = tempfile::tempdir().unwrap();
        let home = gsd_dir.path().join("home");
        let replies: Vec<&str> = gsd.iter().map(|status| gsd_says(*status)).collect();
        devflow_core::test_support::plant_gsd_run_stub(
            &home,
            &replies,
            &gsd_dir.path().join("logs"),
        );
        let cache = hermetic_cache_dir();
        let hold_value = std::ffi::OsString::from(format!("{hold}:{}", release.display()));
        let env: [(&str, &std::ffi::OsStr); 4] = [
            ("DEVFLOW_CACHE_DIR", cache.path().as_os_str()),
            (HOLD_ENV, hold_value.as_os_str()),
            ("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("120")),
            (crate::test_support::TEST_GSD_HOME_ENV, home.as_os_str()),
        ];
        let mut child =
            devflow_core::test_support::spawn_test_in_child(name, path_dir.path(), &env);
        let events = || {
            std::fs::read_to_string(devflow_core::events::events_path(&root)).unwrap_or_default()
        };
        let fail = |child: &mut std::process::Child, why: String| -> ! {
            let _ = child.kill();
            let output = killed_child_output(child);
            panic!("{why}; events:\n{}\n{output}", events());
        };
        if let Some(phase) = approve {
            let gate = devflow_core::gates::Gates::gate_path(&root, phase, Stage::Validate);
            if !poll_until(HANDOFF_STEP_WAIT, || gate.exists()) {
                fail(&mut child, "the Validate gate never opened".to_string());
            }
            answer_open_gate(&root, phase, Stage::Validate, &gate_response(true, None)).unwrap();
        }
        let held = hold_dir.path().join("release.held");
        if !poll_until(TEST_HOLD_WAIT, || held.exists()) {
            fail(&mut child, format!("the hold was never reached: {hold}"));
        }
        let pid = libc::pid_t::try_from(child.id()).expect("a pid_t");
        // SAFETY: a plain kill(2) on the child this test spawned and has not reaped.
        assert_eq!(
            unsafe { libc::kill(pid, libc::SIGTERM) },
            0,
            "SIGTERM the runner"
        );
        std::fs::write(&release, "").unwrap();
        if !poll_until(HANDOFF_STEP_WAIT, || {
            child.try_wait().is_ok_and(|status| status.is_some())
        }) {
            fail(
                &mut child,
                "the runner did not exit after the stop".to_string(),
            );
        }
        let output = child
            .wait_with_output()
            .expect("collect the child's output");
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
        (hold_dir, root)
    }

    /// A killed child's output, for a failure message.
    fn killed_child_output(child: &mut std::process::Child) -> String {
        use std::io::Read as _;
        let _ = child.wait();
        let mut text = String::new();
        if let Some(stdout) = child.stdout.as_mut() {
            let _ = stdout.read_to_string(&mut text);
        }
        if let Some(stderr) = child.stderr.as_mut() {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    }

    /// The marker a Validate stub prints: a passing turn with a PASS verdict.
    const RUNNER_PASS_MARKER: &str = r#"DEVFLOW_RESULT: {"status":"success","verdict":"pass"}"#;

    /// D-05, carried from `a_stop_after_a_gate_answer_loses_to_a_permitted_finish`'s
    /// control: a stop that lands at `after_commit`, the Validate approve committed and
    /// not applied, when the approve's fresh read permits no finish (`gaps_found`),
    /// stops the run unfinished: one stop event, no further stage launched, and the
    /// runner exits with STOPPED_EXIT_CODE.
    #[test]
    fn a_stop_after_gate_approval_is_recorded_as_stopped() {
        const NAME: &str =
            "pipeline_launch::tests::a_stop_after_gate_approval_is_recorded_as_stopped";
        let phase = PhaseId::new(5051);
        if devflow_core::test_support::in_child_test(NAME) {
            run_runner_in_stop_child(
                phase,
                Stage::Validate,
                STOPPED_EXIT_CODE,
                "stop not recorded",
            );
            return;
        }
        use devflow_core::gsd_reader::GsdStatus;
        let (_hold, root) = run_runner_stop_child(
            NAME,
            "after_commit",
            RUNNER_PASS_MARKER,
            &[GsdStatus::Passed, GsdStatus::GapsFound],
            Some(phase),
        );
        let state = workflow::load_state(&root, phase).expect("a stopped run keeps its state");
        assert!(
            state.stopped && !state.finished,
            "stop not recorded: {state:?}"
        );
        assert!(
            events_named(&root, "workflow_finished").is_empty(),
            "stop not recorded: the run finished"
        );
        let stops = events_named(&root, "advance_stopped").len()
            + events_named(&root, "runner_stopped").len();
        assert_eq!(
            (stops, stage_launched_count(&root, phase)),
            (1, 1),
            "stop not recorded once, or a stage launched after it; events:\n{}",
            std::fs::read_to_string(devflow_core::events::events_path(&root)).unwrap_or_default()
        );
    }

    /// D-05, carried from `a_stop_after_a_gate_answer_loses_to_a_permitted_finish`: the
    /// same late stop at `after_commit` on a Validate approve whose finish is permitted
    /// (a fresh `passed`) loses to the finish. The run reads finished with the finish's
    /// reason, one `workflow_finished` `{"reason":"finished","shipped":false}`, no stop is
    /// written, and the runner exits 0.
    #[test]
    fn a_permitted_finish_wins_over_a_late_stop() {
        const NAME: &str = "pipeline_launch::tests::a_permitted_finish_wins_over_a_late_stop";
        let phase = PhaseId::new(5052);
        if devflow_core::test_support::in_child_test(NAME) {
            run_runner_in_stop_child(phase, Stage::Validate, 0, "finished run read stopped");
            return;
        }
        use devflow_core::gsd_reader::GsdStatus;
        let (_hold, root) = run_runner_stop_child(
            NAME,
            "after_commit",
            RUNNER_PASS_MARKER,
            &[GsdStatus::Passed, GsdStatus::Passed],
            Some(phase),
        );
        let state = workflow::load_state(&root, phase).expect("a finished run keeps its state");
        assert!(
            state.stopped && state.finished,
            "finished run read stopped: {state:?}"
        );
        assert_eq!(
            state.stop_reason.as_deref(),
            Some(crate::pipeline_gate::FINISHED_STOP_REASON),
            "finished run read stopped"
        );
        let finished = events_named(&root, "workflow_finished");
        assert_eq!(finished.len(), 1, "{finished:?}");
        assert_eq!(finished[0]["reason"], "finished", "{}", finished[0]);
        assert_eq!(finished[0]["shipped"], false, "{}", finished[0]);
        assert!(
            events_named(&root, "advance_stopped").is_empty()
                && events_named(&root, "runner_stopped").is_empty(),
            "finished run read stopped: a stop was written"
        );
    }

    /// D-05, carried from `a_stop_during_advance_stops_at_the_next_safe_point_and_launches_nothing`:
    /// a stop that lands at `before_runner_stage`, after Plan's advance handed Code back and
    /// before the runner begins it, starts no Code agent (no second `stage_launched`), and
    /// the run reads stopped.
    #[test]
    fn a_stop_during_the_next_stage_launch_is_recorded_as_stopped() {
        const NAME: &str =
            "pipeline_launch::tests::a_stop_during_the_next_stage_launch_is_recorded_as_stopped";
        let phase = PhaseId::new(5053);
        if devflow_core::test_support::in_child_test(NAME) {
            run_runner_in_stop_child(phase, Stage::Plan, STOPPED_EXIT_CODE, "stop not recorded");
            return;
        }
        let (_hold, root) = run_runner_stop_child(
            NAME,
            "before_runner_stage",
            SUCCESS_MARKER,
            &[devflow_core::gsd_reader::GsdStatus::Passed; 4],
            None,
        );
        assert_eq!(
            stage_launched_count(&root, phase),
            1,
            "the next stage launched after a stop"
        );
        let state = workflow::load_state(&root, phase).expect("a stopped run keeps its state");
        assert!(
            state.stopped && !state.finished,
            "stop not recorded: {state:?}"
        );
        assert_eq!(
            events_named(&root, "runner_stopped").len(),
            1,
            "stop not recorded: no runner_stopped event"
        );
    }

    /// Review r2 agy-3: the runner exits 0 only for a run that reads finished or stopped
    /// at its `--until` stage. Every other row, including an absent state (an abort
    /// cleared it), exits 1.
    #[test]
    fn the_runner_exit_code_is_zero_only_for_a_finished_or_until_halted_run() {
        let root = std::path::PathBuf::from("/nonexistent/runner-exit-code");
        let at = |stage: Stage, edit: &dyn Fn(&mut State)| {
            let mut state = State::new(
                PhaseId::new(1),
                AgentKind::Codex,
                Mode::Supervise,
                root.clone(),
            );
            state.stage = stage;
            edit(&mut state);
            state
        };
        let finished = at(Stage::Validate, &|s| {
            s.stopped = true;
            s.finished = true;
        });
        let until_halt = at(Stage::Code, &|s| {
            s.stopped = true;
            s.stop_until = Some(Stage::Code);
        });
        let stopped_no_until = at(Stage::Code, &|s| s.stopped = true);
        let stopped_other_until = at(Stage::Code, &|s| {
            s.stopped = true;
            s.stop_until = Some(Stage::Plan);
        });
        let gate_pending = at(Stage::Validate, &|s| s.gate_pending = true);
        let running = at(Stage::Code, &|_| {});
        let rows: [(&str, Option<&State>, i32); 7] = [
            ("no state (an abort cleared it)", None, 1),
            ("finished", Some(&finished), 0),
            ("stopped at its --until stage", Some(&until_halt), 0),
            ("stopped with no --until", Some(&stopped_no_until), 1),
            (
                "stopped with --until another stage",
                Some(&stopped_other_until),
                1,
            ),
            ("gate pending", Some(&gate_pending), 1),
            ("running", Some(&running), 1),
        ];
        for (row, state, expected) in rows {
            assert_eq!(
                runner_exit_code(state),
                expected,
                "runner_exit_code row `{row}`"
            );
        }
    }

    /// W-2: a failed start names the handoff (`agent_handoff_relaunch_failed`, with the
    /// keys and reason fail_over_on_rate_limit writes) only when the launch is a failover
    /// relaunch; a plain launch's failed start is `stage_launch_failed`.
    #[test]
    fn a_failed_start_names_the_handoff_only_for_a_failover_relaunch() {
        let relaunch = HandoffRelaunch {
            from: AgentKind::Claude,
            to: AgentKind::Codex,
            rate_limit: Some("rate limited until 17:00".to_string()),
        };
        let (event, payload, reason) = failed_start(
            Stage::Code,
            AgentKind::Codex,
            Some(&relaunch),
            "spawn refused",
        );
        assert!(
            event == "agent_handoff_relaunch_failed"
                && payload["stage"] == "code"
                && payload["from_agent"] == "claude"
                && payload["to_agent"] == "codex"
                && payload["reason"] == "spawn refused"
                && reason
                    == "the handoff to codex could not relaunch the code stage: spawn refused \
                        (after the rate limit: rate limited until 17:00)",
            "failover relaunch not named: {event} {payload} {reason}"
        );

        let (event, payload, reason) =
            failed_start(Stage::Code, AgentKind::Codex, None, "spawn refused");
        assert!(
            event == "stage_launch_failed"
                && payload["stage"] == "code"
                && payload["agent"] == "codex"
                && payload["error"] == "spawn refused"
                && payload.get("from_agent").is_none()
                && reason == "the runner could not start the code agent: spawn refused",
            "plain launch named a handoff: {event} {payload} {reason}"
        );
    }

    /// The name of the `fn` `line` declares, if it declares one.
    fn declared_fn(line: &str) -> Option<&str> {
        let at = line.find("fn ")?;
        let before = &line[..at];
        if !(before.trim().is_empty()
            || before.ends_with(' ') && !before.trim_end().ends_with(['.', ':', '=']))
        {
            return None;
        }
        let rest = &line[at + 3..];
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
        (end > 0 && rest[end..].starts_with(['(', '<'])).then(|| &rest[..end])
    }

    /// Each production call of `needle` (a path ending in `(`, matched at a word
    /// boundary) in `source`, as the name of the `fn` it is in. Production is the source
    /// up to its first `mod tests {` line, with `//` lines dropped; a line declaring the
    /// called `fn` itself is not a call.
    fn call_sites(source: &str, needle: &str) -> Vec<String> {
        let called = needle
            .trim_end_matches('(')
            .rsplit("::")
            .next()
            .unwrap_or(needle);
        let mut enclosing = String::new();
        let mut sites = Vec::new();
        for line in source
            .lines()
            .take_while(|line| !line.starts_with("mod tests {"))
            .filter(|line| !line.trim_start().starts_with("//"))
        {
            if let Some(name) = declared_fn(line) {
                enclosing = name.to_string();
                if name == called {
                    continue;
                }
            }
            for (at, _) in line.match_indices(needle) {
                let boundary = line[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                if boundary {
                    sites.push(enclosing.clone());
                }
            }
        }
        sites
    }

    /// [`call_sites`] of `needle` across every CLI source file, as `file: fn`.
    fn cli_call_sites(needle: &str) -> Vec<String> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|err| panic!("read {dir}: {err}"))
            .map(|entry| entry.expect("a src entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
            .collect();
        files.sort();
        assert!(
            files.len() > 10,
            "the scan read too few CLI sources: {files:?}"
        );
        files
            .iter()
            .flat_map(|path| {
                let source = std::fs::read_to_string(path)
                    .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
                let file = path.file_name().unwrap().to_string_lossy().to_string();
                call_sites(&source, needle)
                    .into_iter()
                    .map(move |name| format!("{file}: {name}"))
            })
            .collect()
    }

    /// D-03: codex's overrides go before the prompt, claude's flag after
    /// everything; an argv of the prompt alone keeps the prompt last.
    #[test]
    fn splice_ask_args_places_codex_wiring_before_the_prompt() {
        let wiring = |placement| agents::AskWiring {
            args: vec!["-c".into(), "x".into()],
            placement,
            file: None,
        };
        let argv = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            splice_ask_args(
                argv(&["exec", "--json", "PROMPT"]),
                &wiring(agents::ArgPlacement::BeforeLast)
            ),
            argv(&["exec", "--json", "-c", "x", "PROMPT"])
        );
        assert_eq!(
            splice_ask_args(
                argv(&["-p", "--verbose"]),
                &wiring(agents::ArgPlacement::Append)
            ),
            argv(&["-p", "--verbose", "-c", "x"])
        );
        assert_eq!(
            splice_ask_args(argv(&["PROMPT"]), &wiring(agents::ArgPlacement::BeforeLast)),
            argv(&["-c", "x", "PROMPT"])
        );
    }

    /// [`call_sites`] of `needle` in every `.rs` file under `dir`, recursively, as
    /// `<path under crates/>: fn`.
    fn crate_call_sites(dir: &Path, needle: &str, sites: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
            .map(|entry| entry.expect("a src entry").path())
            .collect();
        entries.sort();
        let crates = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        for path in entries {
            if path.is_dir() {
                crate_call_sites(&path, needle, sites);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
                let file = path
                    .strip_prefix(crates)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                sites.extend(
                    call_sites(&source, needle)
                        .into_iter()
                        .map(|name| format!("{file}: {name}")),
                );
            }
        }
    }

    /// D-02, T-53-21: the registration reaches only a real stage launch. Across
    /// both crates' non-test source, `ask_wiring(` is called in
    /// `wire_ask_registration`, which only `prepare_stage_launch` calls, and nowhere
    /// else that launches; the one other call is
    /// the driver conformance case, which launches nothing. The canary and
    /// preflight launches carry none, and the start/resume/handoff refusal asks
    /// `ask_support`. Control: a planted extra call site is reported.
    #[test]
    fn only_a_stage_launch_carries_the_ask_registration() {
        let planted = "fn prepare_stage_launch() {\n    let w = driver.ask_wiring(&reg);\n}\n\
                       fn run_delivery_canary() {\n    let w = driver.ask_wiring(&reg);\n}\n\
                       mod tests {\n    fn t() { driver.ask_wiring(&reg); }\n}\n";
        assert_eq!(
            call_sites(planted, "ask_wiring("),
            ["prepare_stage_launch", "run_delivery_canary"],
            "planted control: a second call site is reported"
        );
        let mut sites = Vec::new();
        for krate in ["devflow-cli", "devflow-core"] {
            let src = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
                .join(krate)
                .join("src");
            crate_call_sites(&src, "ask_wiring(", &mut sites);
        }
        assert_eq!(
            sites,
            [
                "devflow-cli/src/pipeline_launch.rs: wire_ask_registration",
                "devflow-core/src/agents/mod.rs: ask_declarations_agree",
            ],
            "production ask_wiring( call sites"
        );
        assert_eq!(
            cli_call_sites("wire_ask_registration("),
            ["pipeline_launch.rs: prepare_stage_launch"],
            "production wire_ask_registration( call sites"
        );
    }

    /// D-01, D-02: the runner has one production entry point, the `__runner` verb's arm
    /// in main.rs. `exactly_one_call_site_runs_a_stage` stays at one even if `start` ran
    /// the runner in-process, which this scan rules out. Control: a planted second call
    /// is listed.
    #[test]
    fn run_runner_has_one_caller_the_runner_verb() {
        let planted = "fn run() {\n    match cli.command {\n        Command::Runner { project, phase } \
                       => run_runner(&root, phase),\n    }\n}\n\
                       pub(crate) fn start(\n    root: &Path,\n) {\n    let _ = \
                       crate::pipeline_launch::run_runner(root, phase);\n}\n\
                       // run_runner(commented)\nmod tests {\n    run_runner(x);\n}\n";
        assert_eq!(
            call_sites(planted, "run_runner("),
            ["run", "start"],
            "planted control: a second run_runner( call is listed"
        );
        assert_eq!(
            cli_call_sites("run_runner("),
            ["main.rs: run"],
            "production run_runner( sites"
        );
        let main = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
            .expect("read main.rs");
        let arm = main
            .find("Command::Runner {")
            .and_then(|at| main[at..].find("run_runner(").map(|call| at + call))
            .expect("main.rs's Command::Runner arm calls run_runner(");
        let next_arm = main[main.find("Command::Runner {").unwrap()..]
            .find("Command::Resume {")
            .map(|at| at + main.find("Command::Runner {").unwrap())
            .expect("an arm after Command::Runner");
        assert!(
            arm < next_arm,
            "the one run_runner( call is not inside main.rs's Command::Runner arm"
        );
    }

    /// T-52.2-05, T-52.2-67 (MemPalace T-50.1-97), T-52.2-24: exactly one production call
    /// site runs a stage, the runner loop's `monitor::run_stage(` (in `run_one_stage`,
    /// which only `run_runner` calls), and no production code in the CLI calls
    /// `spawn_monitor(` (D-02: the runner loop is the only stage chain). Planted controls
    /// run through the same helper.
    #[test]
    fn exactly_one_call_site_runs_a_stage() {
        let planted_twice = "fn run_one_stage() {\n    monitor::run_stage(spec, token);\n}\n\
                             fn transition() {\n    let _ = monitor::run_stage(spec, token);\n}\n\
                             // monitor::run_stage(commented)\nmod tests {\n    monitor::run_stage(x);\n}\n";
        assert_eq!(
            call_sites(planted_twice, "run_stage("),
            ["run_one_stage", "transition"],
            "planted control: a second run_stage( call is listed"
        );
        let planted_spawn = "pub(crate) fn transition(\n    s: &mut State,\n) {\n    \
                             monitor::spawn_monitor(b);\n}\n\
                             // monitor::spawn_monitor(commented)\nmod tests {\n    \
                             monitor::spawn_monitor(x);\n}\n";
        assert_eq!(
            call_sites(planted_spawn, "spawn_monitor("),
            ["transition"],
            "planted control: a spawn_monitor( in transition is reported"
        );

        assert_eq!(
            cli_call_sites("run_stage("),
            ["pipeline_launch.rs: run_one_stage"],
            "production run_stage( sites"
        );
        assert_eq!(
            cli_call_sites("run_one_stage("),
            ["pipeline_launch.rs: run_runner"],
            "production run_one_stage( sites"
        );
        assert_eq!(
            cli_call_sites("spawn_monitor("),
            Vec::<String>::new(),
            "production spawn_monitor( sites"
        );
    }

    /// OD-1 (b), production value: every runner stage lets an open question
    /// hold the idle window for 960 s of silence, the codex cap plus its margin.
    #[test]
    fn the_runner_bounds_a_question_hold_by_the_hold_cap() {
        let root = Path::new("/work/project");
        for stage in [Stage::Plan, Stage::Code, Stage::Validate] {
            let mut state = State::new(
                PhaseId::new(7),
                AgentKind::Claude,
                Mode::Auto,
                root.to_path_buf(),
            );
            state.stage = stage;
            let spec = runner_stage_spec(StageLaunch {
                state,
                program: "claude".to_string(),
                args: Vec::new(),
                extra_env: Vec::new(),
                archived_stage: None,
                launch: StageShape::Capture,
                relaunch: None,
            });
            assert_eq!(spec.stage, Some(stage), "{stage}");
            assert_eq!(
                spec.question_wait,
                Some(Duration::from_secs(960)),
                "{stage}: the question hold's bound"
            );
        }
    }
}
