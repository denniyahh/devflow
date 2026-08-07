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
    let stream_launch =
        claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);

    // D-11 (31-04): the opt-out is loud, on three channels. Fires only when the
    // opt-out is what made the difference — forcing legacy on a stage the
    // rollout has not reached changes nothing, and a notice there would imply
    // it did. Placed ahead of the canary gate so the operator learns what the
    // legacy path costs before anything else happens.
    if !stream_launch && claude_stream_launch_enabled(state.agent, state.stage, false) {
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
    canary_gate(state, stream_launch, move || {
        canary::run_delivery_canary(
            &canary::ClaudeCanaryLauncher {
                workdir: canary_workdir,
            },
            &canary_capture_dir,
        )
    })?;

    let (program, args, launch) = resolve_launch_shape(
        state.agent,
        adapter.as_ref(),
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

    spawn_agent_and_record(
        state,
        program,
        &args,
        &adapter.extra_env(),
        archived_stage,
        launch,
    )
}

/// Resolve a stage launch into `(program, argv, monitor arm)`.
///
/// Extracted from [`launch_stage_inner`] unchanged (31-04) so the shape a
/// launch resolves to is assertable without spawning a process. The body is the
/// pre-extraction `if/else if/else` verbatim; `stream_launch` is the caller's
/// already-computed [`claude_stream_launch_enabled`] reading, threaded in
/// rather than recomputed so one predicate still governs the launch shape, the
/// canary gate, and the D-11 notice.
fn resolve_launch_shape(
    agent: AgentKind,
    adapter: &dyn agents::AgentAdapter,
    phase: u32,
    prompt: String,
    roots: &[std::path::PathBuf],
    stream_launch: bool,
) -> (&'static str, Vec<String>, monitor::MonitorLaunch) {
    if stream_launch {
        let (program, args) = adapter.exec_command(phase, &prompt, roots);
        (program, args, monitor::MonitorLaunch::PipeOwning { prompt })
    } else if agent == AgentKind::Claude {
        // Claude on a stage the rollout has not reached, or a run that took
        // D-11's opt-out: the explicitly named pre-31 builder, NOT
        // `exec_command` — which now returns the stream-json shape for every
        // stage.
        let (program, args) = agents::ClaudeAgent::exec_command_single_document(&prompt);
        (program, args, monitor::MonitorLaunch::Legacy)
    } else {
        let (program, args) = adapter.exec_command(phase, &prompt, roots);
        (program, args, monitor::MonitorLaunch::Legacy)
    }
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
fn append_monitor_log(project_root: &Path, phase: u32, entry: &str) {
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
///
/// **The launch argv is stage-blind, so a per-stage capture is evidence about
/// the AGENT and never about the transport** (ROADMAP criterion 1; 34-REVIEW.md
/// R-02). [`agents::ClaudeAgent::exec_command`]
/// (`crates/devflow-core/src/agents/claude.rs:46`) ignores all three of its
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
fn claude_stream_launch_enabled(agent: AgentKind, stage: Stage, legacy_opt_out: bool) -> bool {
    !legacy_opt_out && agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)
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
/// D-11 (31-04): `--legacy-claude-launch` is accepted here too, so an operator
/// can force the pre-31 path onto a run already in flight without restarting
/// it. The combination is OR-only — see [`apply_legacy_launch_opt_out`] for why
/// a plain `devflow resume` must not clear an opt-out the operator already
/// chose.
pub(crate) fn resume(
    project_root: &Path,
    phase: u32,
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
    let mut state = workflow::load_state(project_root, phase)?;
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
                && verify::phase_has_blocking_human_checkpoint(execution_root, phase)
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
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** This test's subject is
    /// pid persistence, not which launch path runs, so it previously relied on
    /// its stage being ABSENT from `STREAM_JSON_STAGES` — an incidental
    /// premise that 34-05's widening destroys, at which point `canary_gate`
    /// invokes the real `ClaudeCanaryLauncher` and the launch fails on a
    /// delivery refusal that has nothing to do with pid persistence. Pinning
    /// the legacy path via the opt-out makes the premise explicit and stable
    /// under any contents of the constant, exactly as
    /// `canary_gate_only_applies_to_the_stream_launch_path` does.
    #[test]
    fn launch_stage_persists_monitor_pid_for_reload() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 65;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
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
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** The subject is resume's
    /// stop-marker semantics, not which launch path the relaunch takes. The
    /// opt-out is persisted here rather than set on a local binding because
    /// `resume()` loads its own `State` from disk; `apply_legacy_launch_opt_out`
    /// ORs the persisted value, so it survives the reload.
    #[test]
    fn resume_clears_stop_marker_and_advances_past_stop_point() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 66;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = resume(root, phase, false);

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
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** The subject is the
    /// unfired `--until` cap's survival, not which launch path the relaunch
    /// takes. Persisted rather than set locally, for the same reason as the
    /// sibling test above: `resume()` reloads its own `State`.
    #[test]
    fn resume_preserves_unfired_until_cap() {
        let _guard = env_lock();

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
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = resume(root, phase, false);

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
    ///
    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
    /// (34-06); do not "simplify" the opt-out away.** The subject is that the
    /// no-cap resume path is undisturbed, not which launch path the relaunch
    /// takes. Persisted rather than set locally, as in the two siblings above.
    #[test]
    fn resume_without_a_cap_is_unchanged() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 68;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stop_until = None;
        state.stopped = false;
        state.stop_reason = None;
        // Not because this test wants legacy behaviour, but because its
        // subject is orthogonal to the launch path (34-06).
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let stub_dir = stub_agent_binary("claude");
        let original_path = std::env::var_os("PATH");
        let stubbed_path = prepend_path(&stub_dir, &original_path);
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", &stubbed_path);
        }

        let result = resume(root, phase, false);

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
        let _guard = env_lock();

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
        let _guard = env_lock();

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
        let _guard = env_lock();

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
        let _guard = env_lock();

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
        let _guard = env_lock();

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
        let _guard = env_lock();

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
    ///
    /// **The false-branch discriminator is D-11's legacy opt-out, deliberately —
    /// NOT stage membership** (ROADMAP criterion 7). This test previously took
    /// `Stage::Plan`'s absence from [`STREAM_JSON_STAGES`] as its premise. That
    /// premise is destroyed by the very rollout the constant exists to
    /// sequence: once all five stages are widened, no Claude stage yields
    /// `false` and the test becomes unconstructible as written — it would have
    /// had to be deleted or rewritten under the time pressure of the widening
    /// commit. `legacy_opt_out` is a separate `&&` term that
    /// `claude_stream_launch_enabled` respects regardless of the constant's
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

        let phase = 121;
        let mut state = canary_state(root, phase);
        // `Stage::Code` is on the stream path today and stays on it. The
        // variable under test is the opt-out, not the stage.
        state.stage = Stage::Code;
        state.legacy_claude_launch = true;

        // Driven by the REAL predicate rather than a hardcoded `false`, so this
        // test tracks the rollout instead of a copy of it.
        let stream_launch =
            claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
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
            claude_stream_launch_enabled(AgentKind::Claude, state.stage, false),
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

        let phase = 123;
        let mut state = canary_state(root, phase);
        state.stage = Stage::Code;
        state.legacy_claude_launch = false;

        let stream_launch =
            claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
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
        let _guard = env_lock();

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
        let _guard = env_lock();

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
        let _guard = env_lock();

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

    /// The PLAIN `blocking` gate — deliberately NOT the human-blocking one.
    /// Used only by [`write_plan_without_checkpoint`]'s decoy body, so the
    /// literal `verify::phase_has_blocking_human_checkpoint` searches for is
    /// absent from it (the Phase 26 near-miss distinction that
    /// `verify.rs`'s `..._false_for_plain_blocking_gate` already pins).
    const PLAIN_GATE_VALUE_FOR_TEST: &str = "blocking";

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
    fn write_plan_without_checkpoint(root: &Path, phase: u32) {
        let dir = root
            .join(".planning/phases")
            .join(format!("{phase:02}-checkpoint-fixture"));
        std::fs::create_dir_all(&dir).unwrap();
        let body = format!(
            "---\nphase: {phase}\n---\n\n<task type=\"checkpoint:decision\" gate=\"{PLAIN_GATE_VALUE_FOR_TEST}\">\n</task>\n"
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
        let _guard = env_lock();

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
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 94;
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

    // ---- D-11: the legacy-launch opt-out (31-04) -------------------------

    /// Set `DEVFLOW_CLAUDE_LEGACY_LAUNCH` for the duration of a test, restoring
    /// the prior value on drop. Every user holds `ENV_MUTEX`.
    struct LegacyEnvOverride(Option<std::ffi::OsString>);

    impl LegacyEnvOverride {
        fn set(value: &str) -> Self {
            let prior = std::env::var_os("DEVFLOW_CLAUDE_LEGACY_LAUNCH");
            // SAFETY: serialized by ENV_MUTEX; restored on drop.
            unsafe { std::env::set_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH", value) };
            Self(prior)
        }
    }

    impl Drop for LegacyEnvOverride {
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

    fn legacy_state(root: &Path, phase: u32, opt_out: bool) -> State {
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
        let state = legacy_state(dir.path(), 130, true);

        // Precondition: without the opt-out this stage IS in the rollout, so
        // the assertion below is a real discrimination and not a stage that
        // was going to be legacy anyway.
        assert!(
            claude_stream_launch_enabled(state.agent, state.stage, false),
            "Stage::Code must be in STREAM_JSON_STAGES for this test to mean anything"
        );

        assert!(!claude_stream_launch_enabled(
            state.agent,
            state.stage,
            state.legacy_claude_launch
        ));

        let adapter = agents::adapter_for(state.agent);
        let (program, args, launch) = resolve_launch_shape(
            state.agent,
            adapter.as_ref(),
            state.phase,
            "the stage prompt".to_string(),
            &[],
            false,
        );

        assert!(matches!(launch, monitor::MonitorLaunch::Legacy));
        assert_eq!(program, "claude");
        assert_eq!(
            (program, args),
            agents::ClaudeAgent::exec_command_single_document("the stage prompt"),
            "the forced path must be exec_command_single_document byte-for-byte, \
             not an approximation of it"
        );
    }

    /// Off by default: nothing set anywhere leaves the run on the Phase 31
    /// transport. An escape hatch that engaged on its own would be the silent
    /// downgrade D-11 rejects.
    #[test]
    fn legacy_launch_is_off_by_default() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let state = legacy_state(dir.path(), 131, false);

        assert!(
            !state.legacy_claude_launch,
            "State::new must default the opt-out to off"
        );
        assert!(!devflow_core::config::claude_legacy_launch());

        let stream_launch =
            claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(stream_launch);

        let adapter = agents::adapter_for(state.agent);
        let (_program, _args, launch) = resolve_launch_shape(
            state.agent,
            adapter.as_ref(),
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
    fn legacy_launch_use_is_recorded_in_provenance() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let state = legacy_state(root, 132, true);

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

        let log = std::fs::read_to_string(agent_result::monitor_log_path(root, 132))
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

        let phase = 133;
        let mut state = canary_state(root, phase);
        state.legacy_claude_launch = true;

        // Driven by the REAL predicate, so this tracks the wiring rather than
        // a copy of it.
        let stream_launch =
            claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(!stream_launch);
        // Negative control: the SAME stage and agent, opt-out off, does run the
        // stream path — so the reading above is the opt-out's doing.
        assert!(claude_stream_launch_enabled(
            state.agent,
            state.stage,
            false
        ));

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
    /// `claude_stream_launch_enabled` by calling `spawn_agent_and_record`
    /// directly. That is a pre-existing, deliberate exception recorded in
    /// 31-04-SUMMARY.md as a known un-migrated route, not something this test
    /// covers.
    #[test]
    fn parse_failure_does_not_trigger_a_fallback() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 134;
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
            claude_stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
        assert!(
            stream_launch,
            "a parse failure must not select the legacy path — D-11 rejects automatic fallback"
        );
        let adapter = agents::adapter_for(state.agent);
        let (_program, _args, launch) = resolve_launch_shape(
            state.agent,
            adapter.as_ref(),
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
    fn resume_does_not_clear_a_persisted_legacy_launch() {
        let _guard = env_lock();
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::remove_var("DEVFLOW_CLAUDE_LEGACY_LAUNCH") };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = 135;
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
        let mut never_opted_out = legacy_state(root, phase + 1, false);
        apply_legacy_launch_opt_out(&mut never_opted_out, false);
        assert!(!never_opted_out.legacy_claude_launch);
    }
}
