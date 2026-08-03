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
use crate::pipeline_gate::transition;
use crate::pipeline_outcomes::{
    ValidateOutcome, classify_validate_outcome, handle_infra_outcome, handle_rate_limited_outcome,
    handle_ship_failure, handle_ship_outcome, handle_stage_failure, handle_validate_outcome,
    truncate_reason,
};
use crate::preflight::{ensure_agent_binary, run_preflight, worktree_writable_roots};
use devflow_core::config::{GitFlowConfig, capture_retention};
use devflow_core::outcome_policy::{self, Action};
use devflow_core::prompt;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{agent_result, agents, canary, events, lock, mode, monitor, verify, workflow};
use std::path::Path;

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
) -> Result<(), CliError> {
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
    // D-09/D-10 sequencing gate. `ClaudeAgent::exec_command` is itself
    // unconditional and stage-blind (constraint 1 forbids predicting at launch
    // time which stages background work); the choice of which stages have been
    // widened to it *yet* is a rollout-order choice, made here at the call
    // site, which constraint 1 permits.
    //
    // Evaluated ONCE and reused by the canary gate below, so a single predicate
    // governs both the launch shape and the guard that protects it — the guard
    // must fire on exactly the launches whose premise it checks, and two
    // separate evaluations of "is this the stream path?" would be free to drift.
    let stream_launch = claude_stream_launch_enabled(state.agent, state.stage);

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
    canary_gate(state, stream_launch, move || {
        canary::run_delivery_canary(
            &canary::ClaudeCanaryLauncher {
                workdir: canary_workdir,
            },
            &canary_capture_dir,
        )
    })?;

    let (program, args, launch) = if stream_launch {
        let (program, args) = adapter.exec_command(state.phase, &prompt, &roots);
        (program, args, monitor::MonitorLaunch::PipeOwning { prompt })
    } else if state.agent == AgentKind::Claude {
        // Claude on a stage the rollout has not reached: the explicitly named
        // pre-31 builder, NOT `exec_command` — which now returns the
        // stream-json shape for every stage.
        let (program, args) = agents::ClaudeAgent::exec_command_single_document(&prompt);
        (program, args, monitor::MonitorLaunch::Legacy)
    } else {
        let (program, args) = adapter.exec_command(state.phase, &prompt, &roots);
        (program, args, monitor::MonitorLaunch::Legacy)
    };

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

    spawn_agent_and_record(
        state,
        program,
        &args,
        &adapter.extra_env(),
        archived_stage,
        launch,
    )
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
/// Clearing `monitor_pid` applies WR-04's rationale (see
/// [`spawn_agent_and_record`]) to an aborted launch: `transition()` has already
/// advanced `state.stage` and saved it by the time this runs, so a refusal that
/// left the PREVIOUS stage's monitor pid standing would make `liveness()`
/// report `Stuck → devflow resume` and send the operator after the wrong
/// problem entirely. The right remedy is the message below.
fn refuse_launch(state: &mut State, message: String) -> Result<(), CliError> {
    state.monitor_pid = None;
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
    let (event, reason) = match outcome {
        canary::CanaryOutcome::Confirmed => ("claude_delivery_canary_confirmed", None),
        canary::CanaryOutcome::Absent => ("claude_delivery_canary_absent", None),
        canary::CanaryOutcome::Unverified(reason) => (
            "claude_delivery_canary_unverified",
            Some(truncate_reason(reason)),
        ),
    };
    events::emit(
        &state.project_root,
        state.phase,
        event,
        serde_json::json!({
            "stage": state.stage.to_string(),
            "token_prefix": canary::TOKEN_PREFIX,
            "cli_version": canary::claude_cli_version(),
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
const STREAM_JSON_STAGES: &[Stage] = &[Stage::Code];

/// Whether this launch should use the `stream-json` transport and the
/// pipe-owning monitor.
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
fn claude_stream_launch_enabled(agent: AgentKind, stage: Stage) -> bool {
    agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)
}

/// The detached pipe-owning monitor's own process body (Phase 31): supervise
/// the child, then advance the stage machine exactly as the shell monitor's
/// `devflow advance` tail did.
///
/// Runs in the `__monitor` process, never in the operator's CLI.
///
/// `envs` is deliberately empty here — the adapter's extra env was applied to
/// THIS process by `spawn_monitor` and rides down by inheritance. That is
/// sufficient only because the sole adapter routed through the pipe-owning arm
/// (Claude) declares no extra env; see the note at `spawn_monitor`'s
/// `PipeOwning` arm before widening it.
pub(crate) fn run_monitor(
    project_root: &Path,
    phase: u32,
    workdir: &Path,
    prompt_file: &Path,
    idle_timeout_secs: u64,
    argv: &[String],
) -> Result<(), CliError> {
    let prompt = std::fs::read_to_string(prompt_file).map_err(|err| {
        CliError::Message(format!(
            "monitor could not read the prompt file {}: {err}",
            prompt_file.display()
        ))
    })?;
    let Some((program, args)) = argv.split_first() else {
        return Err(CliError::Message(
            "monitor was given no child program to supervise".to_string(),
        ));
    };

    monitor::run_pipe_owning_monitor(
        project_root,
        phase,
        workdir,
        &prompt,
        std::time::Duration::from_secs(idle_timeout_secs),
        program,
        args,
        &[],
    )
    .map_err(|err| CliError::Message(format!("pipe-owning monitor failed: {err}")))?;

    advance(project_root, Some(phase))
}

/// The tail of [`launch_stage_inner`]: clear the stale monitor pid, validate
/// the agent binary, archive the prior capture, spawn the monitor, and
/// record the launch. Extracted (28-03, Task 2) so
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
    // WR-04 (18-fix): clear the prior stage's monitor pid up front, before
    // any fallible step below (`ensure_agent_binary`) can return early via
    // `?`. Without this, a failed relaunch left `state.stage` already
    // advanced (by `transition()`, before this function was ever called)
    // alongside a stale `monitor_pid` still naming the PREVIOUS stage's
    // (now-dead) monitor — `liveness()` then misreports `Stuck → devflow
    // resume`, even when the real remedy is unrelated. The real pid is set
    // again below once `monitor::spawn_monitor` actually succeeds.
    state.monitor_pid = None;
    workflow::save_state(state)?;

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
    let pid = monitor::spawn_monitor(state, program, args, extra_env, launch)
        .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    // `transition()` calls `workflow::save_state` BEFORE `launch_stage`, so a
    // pid recorded only in memory here is lost unless it is written again
    // (18b).
    state.monitor_pid = Some(pid);
    workflow::save_state(state)?;
    // 23b: register this (project_root, phase) in the machine-global
    // registry on the same code path that just recorded monitor_pid, so a
    // running phase cannot be missing from `devflow gate list --all-roots`.
    // Best-effort observability — never a reason to fail a launch.
    let _ = devflow_core::registry::register(&state.project_root, state.phase);
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
        agents::adapter_for(state.agent).name()
    );
    Ok(())
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
            "policy": "D-03: unconditional agent auto-decide, no flag/config toggle",
        }),
    );

    let (program, args) = agents::ClaudeAgent::exec_resume_command(session_id, &instruction);

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
pub(crate) fn resume(project_root: &Path, phase: u32) -> Result<(), CliError> {
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
    if state.stopped {
        state.stopped = false;
        state.stop_reason = None;
        state.stop_until = None;
    }
    workflow::save_state(&state)?;
    launch_stage(&mut state, None, None)
}

/// The single active phase: `Ok(Some)` when exactly one is active, `Ok(None)`
/// when none, and an error naming the candidates when several are — shared by
/// `advance`'s legacy fallback and `logs`'s default-phase resolution so the
/// ambiguity rule and message live in one place.
pub(crate) fn single_active_phase(project_root: &Path) -> Result<Option<u32>, CliError> {
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
pub(crate) fn resolve_sole_active_phase(project_root: &Path) -> Result<u32, CliError> {
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
pub(crate) fn advance(project_root: &Path, phase: Option<u32>) -> Result<(), CliError> {
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
            // human-blocking checkpoint DevFlow can resolve unattended by
            // resuming the exact session that raised it. Evaluated IN THIS
            // ORDER — load-bearing — (1) agent is Claude (D-05); (2) the
            // phase's plans statically declare a blocking-human checkpoint —
            // the PRIMARY, agent-uncontrollable gate, checked BEFORE
            // anything agent-controlled (T-28-01); (3) the capture confirms
            // one was reported; (4) a session id is on record; (5) the
            // resume ceiling has not been exhausted. All five true -> resume
            // and return. Any false -> fall through to the unchanged
            // per-stage dispatch below.
            let mut reason = result.reason.clone();
            let checkpoint_confirmed = state.agent == AgentKind::Claude
                && verify::phase_has_blocking_human_checkpoint(project_root, phase)
                && agent_result::checkpoint_reported_in_capture(project_root, phase);
            if checkpoint_confirmed {
                let ceiling_ok = state.checkpoint_resumes < mode::MAX_CHECKPOINT_RESUMES;
                match (&state.session_id, ceiling_ok) {
                    (Some(session_id), true) => {
                        let session_id = session_id.clone();
                        return relaunch_checkpoint_session(&mut state, &session_id);
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
        // RateLimited: auto-resume via the primary loop's single-agent cron
        // path (D-09), bounded by the shared infra-failure ceiling (D-08).
        Action::AutoResume => {
            handle_rate_limited_outcome(project_root, &mut state, phase, stage, result.reason)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use devflow_core::gates::Gates;
    use devflow_core::mode::Mode;
    use devflow_core::state::AgentKind;

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

        // WR-03 / 999.46: this launch_stage call spawns a real detached
        // monitor wrapper, same as the staleness test above; the guard must
        // reap it before `dir` drops below. Bound here — this test's LAST
        // `&mut state` use — the guard outranks every panicking checkpoint
        // that follows: `result.unwrap()`, the `assert!` on
        // `state.monitor_pid.is_some()`, `workflow::load_state(...).unwrap()`,
        // and the `assert_eq!` on the reloaded pid. Binding here also covers
        // the narrower case of a launch that spawned the monitor and then
        // failed a later `?` inside `launch_stage_inner` — `result` would be
        // `Err` but the pid would nonetheless be live (G-25-2, 25-17).
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

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
    /// 20c (review: Codex MEDIUM — resume semantics): a phase halted by
    /// `--until <stage>` has `stopped`/`stop_reason`/`stop_until` persisted.
    /// `resume` must clear all three BEFORE relaunching — otherwise
    /// `transition()`'s `stop_until == Some(from)` check would immediately
    /// re-stop the phase the next time it advances, and the phase would
    /// stay marked `stopped` forever despite the operator's explicit
    /// resume. Asserts on the persisted state (not just `resume`'s exit
    /// code), since `transition()` saves state before `launch_stage` runs.
    #[test]
    fn resume_clears_stop_marker_and_advances_past_stop_point() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 66;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = resume(root, phase);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

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
            reloaded.monitor_pid.is_some(),
            "resume() must have spawned a monitor whose pid is recorded in state — if this \
             fails, the reap guard above is silently reaping nothing and this test has \
             stopped covering the launch path it was written to cover"
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
    #[test]
    fn resume_preserves_unfired_until_cap() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 67;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        // Current stage is earlier than the cap (Define < Plan), and the
        // phase was NOT stopped by the cap — this is the rate-limit/infra
        // recovery shape, not the "cap already fired" shape.
        state.stop_until = Some(Stage::Plan);
        state.stopped = false;
        state.stop_reason = None;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = resume(root, phase);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

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
            reloaded.monitor_pid.is_some(),
            "resume() must still have spawned a monitor whose pid is recorded in state"
        );
    }

    /// D-15 (999.60) third case: an ordinary rate-limit/infra resume with no
    /// `--until` cap at all (`stop_until: None`) must remain unaffected by
    /// gating the clear on `state.stopped` — nothing to preserve, nothing to
    /// clear, and the relaunch must still happen.
    #[test]
    fn resume_without_a_cap_is_unchanged() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 68;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stop_until = None;
        state.stopped = false;
        state.stop_reason = None;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = resume(root, phase);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

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
            reloaded.monitor_pid.is_some(),
            "resume() must still relaunch and record a monitor pid with no cap present"
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

    /// D-04/D-07 (28-03, Task 2): a checkpoint resume records the
    /// `checkpoint_auto_decided` audit event exactly once, carrying the
    /// session id and stage — the only durable record of an unattended
    /// checkpoint decision.
    #[test]
    fn relaunch_checkpoint_session_emits_exactly_one_audit_event() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 84;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = relaunch_checkpoint_session(&mut state, "sess-abc-123");

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
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
    }

    /// D-04 (28-03, Task 2): the resume ceiling increments with saturating
    /// arithmetic and the incremented value is persisted to disk, since a
    /// relaunch and its bookkeeping cross separate `devflow advance`
    /// invocations.
    #[test]
    fn relaunch_checkpoint_session_increments_and_persists_counter() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 85;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.checkpoint_resumes = 1;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = relaunch_checkpoint_session(&mut state, "sess-xyz");

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
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
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 86;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = relaunch_checkpoint_session(&mut state, "sess-stage");

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
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
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 87;
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

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = launch_stage_inner(&mut state, None, None);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);

        result.unwrap();

        assert_eq!(
            state.checkpoint_resumes, 0,
            "an ordinary stage launch must reset the checkpoint-resume budget"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.checkpoint_resumes, 0);
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

    fn canary_state(root: &Path, phase: u32) -> State {
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
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 120;
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
        let mut fresh_run = canary_state(root, phase + 1);
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
    #[test]
    fn canary_gate_only_applies_to_the_stream_launch_path() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 121;
        let mut state = canary_state(root, phase);
        // Plan is not in `STREAM_JSON_STAGES`, so it resolves to
        // `exec_command_single_document` + `MonitorLaunch::Legacy`.
        state.stage = Stage::Plan;

        // Driven by the REAL predicate rather than a hardcoded `false`, so this
        // test tracks the rollout instead of a copy of it.
        let stream_launch = claude_stream_launch_enabled(state.agent, state.stage);
        assert!(
            !stream_launch,
            "Stage::Plan must still resolve to the legacy path for this test to mean anything"
        );
        // Negative control: the same predicate DOES fire for the widened stage,
        // so the reading above is a real discrimination and not a constant.
        assert!(
            claude_stream_launch_enabled(AgentKind::Claude, Stage::Code),
            "the predicate must still say yes somewhere, or the check above is vacuous"
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

    /// D-15's refusal. Warning-and-proceeding was rejected (the warning scrolls
    /// past in unattended mode) and so was falling back to sequential dispatch
    /// (a silent capability downgrade).
    #[test]
    fn absent_canary_refuses_to_launch() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 122;
        let mut state = canary_state(root, phase);
        // The previous stage's monitor pid, as `transition()` would leave it.
        state.monitor_pid = Some(4_294_967_000);
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

        assert!(
            state.monitor_pid.is_none(),
            "a refused launch must not leave the previous stage's monitor pid standing — \
             liveness() would report Stuck and point at `devflow resume`, which cannot help"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(
            reloaded.monitor_pid.is_none(),
            "the cleared pid must be persisted, not only cleared in memory"
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
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let mut unverified_state = canary_state(root, 123);
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
        let mut absent_state = canary_state(root, 124);
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
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 125;
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
        assert_eq!(event["phase"], phase);

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

    /// The linkage the five gate tests above cannot show: that
    /// `launch_stage_inner` actually calls the gate, at the widened stage, with
    /// the REAL launcher bound. Runs against the `exit 0` stub `claude`, which
    /// produces an empty capture and therefore an honest `Absent` — no real
    /// agent invocation, and no claim about the real CLI's behaviour.
    #[test]
    fn launch_stage_inner_refuses_at_code_when_the_canary_cannot_confirm() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 126;
        let mut state = canary_state(root, phase);

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = launch_stage_inner(&mut state, None, None);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
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
            state.monitor_pid.is_none(),
            "a refused launch must record no monitor pid"
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

    /// Write a synthetic phase's plan declaring a `blocking-human`
    /// checkpoint task, matching `verify::phase_plan_files`'s discovery
    /// pattern (`.planning/phases/{NN}-*/{NN}-*-PLAN.md`).
    fn write_declared_checkpoint_plan(root: &Path, phase: u32) {
        let dir = root
            .join(".planning/phases")
            .join(format!("{phase:02}-checkpoint-fixture"));
        std::fs::create_dir_all(&dir).unwrap();
        let body = format!(
            "---\nphase: {phase}\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE_FOR_TEST}\">\n</task>\n"
        );
        std::fs::write(dir.join(format!("{phase:02}-01-PLAN.md")), body).unwrap();
    }

    /// Write a captured stdout containing BOTH the `**Gate:**
    /// blocking-human` confirmation literal and a `DEVFLOW_RESULT` failed
    /// marker, so `advance()`'s Layer 1 deterministically classifies the
    /// outcome as `Failed` (-> `Action::GateReview`) with no exit-code/pid
    /// file or background thread needed.
    fn write_confirmed_checkpoint_capture(root: &Path, phase: u32) {
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let stdout = format!(
            "## CHECKPOINT REACHED\n\n**Type:** human-verify\n**Gate:** {HUMAN_GATE_VALUE_FOR_TEST}\n\nDEVFLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"checkpoint pending\"}}\n"
        );
        std::fs::write(agent_result::stdout_path(root, phase), stdout).unwrap();
    }

    /// A capture whose `DEVFLOW_RESULT` marker fails but never reports the
    /// human-blocking `Gate:` literal — an ordinary failure, not a
    /// checkpoint.
    fn write_unreported_failure_capture(root: &Path, phase: u32) {
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\": \"failed\", \"reason\": \"ordinary failure\"}\n",
        )
        .unwrap();
    }

    /// Pre-write a rejected gate response so `run_gate`'s poll (invoked by
    /// the fall-through path's `handle_stage_failure`) returns immediately
    /// instead of blocking — mirrors
    /// `advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed`'s
    /// fixture pattern.
    fn write_abort_gate_response(root: &Path, phase: u32, stage: Stage) {
        let response_path = Gates::response_path(root, phase, stage);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();
    }

    /// The positive case: declared + reported + Claude + session id + under
    /// the ceiling -> resumes and records exactly one audit event, with no
    /// `gate_fired` for this stage.
    #[test]
    fn advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 88;
        write_declared_checkpoint_plan(root, phase);
        write_confirmed_checkpoint_capture(root, phase);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-checkpoint-1".to_string());
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = advance(root, Some(phase));

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
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

    /// D-01 primary-gate proof (T-28-01): the SAME reported capture, but no
    /// plan for this phase declares a checkpoint at all -> must fall through
    /// to the ordinary never-silent gate. Zero checkpoint_auto_decided, at
    /// least one gate_fired.
    #[test]
    fn advance_without_declared_checkpoint_falls_through_to_generic_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 89;
        // Deliberately no write_declared_checkpoint_plan call.
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-should-not-resume".to_string());
        workflow::save_state(&state).unwrap();

        advance(root, Some(phase)).unwrap();

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
    #[test]
    fn advance_with_declared_checkpoint_but_unreported_gate_falls_through() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 90;
        write_declared_checkpoint_plan(root, phase);
        write_unreported_failure_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-unreported".to_string());
        workflow::save_state(&state).unwrap();

        advance(root, Some(phase)).unwrap();

        assert!(events_of_kind(root, "checkpoint_auto_decided").is_empty());
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }

    /// Declared + reported, but no session id on record -> falls through,
    /// and the never-silent gate's context names the missing precondition.
    #[test]
    fn advance_with_confirmed_checkpoint_and_no_session_id_falls_through() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 91;
        write_declared_checkpoint_plan(root, phase);
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = None;
        workflow::save_state(&state).unwrap();

        advance(root, Some(phase)).unwrap();

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
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 92;
        write_declared_checkpoint_plan(root, phase);
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-at-ceiling".to_string());
        state.checkpoint_resumes = mode::MAX_CHECKPOINT_RESUMES;
        workflow::save_state(&state).unwrap();

        advance(root, Some(phase)).unwrap();

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
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 93;
        write_declared_checkpoint_plan(root, phase);
        write_confirmed_checkpoint_capture(root, phase);
        write_abort_gate_response(root, phase, Stage::Code);

        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.session_id = Some("sess-non-claude".to_string());
        workflow::save_state(&state).unwrap();

        advance(root, Some(phase)).unwrap();

        assert!(
            events_of_kind(root, "checkpoint_auto_decided").is_empty(),
            "a non-Claude agent must never take the resume path (D-05)"
        );
        assert!(!events_of_kind(root, "gate_fired").is_empty());
    }
}
