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
    ValidateOutcome, classify_validate_outcome, gate_or_abort_infra, handle_infra_outcome,
    handle_rate_limited_outcome, handle_ship_failure, handle_ship_outcome, handle_stage_failure,
    handle_validate_outcome, truncate_reason,
};
use crate::preflight::{
    agent_program, ensure_agent_binary, generic_preflight_checks, print_fallback_notice,
    run_preflight, validate_fallback_agent, worktree_writable_roots,
};
use devflow_core::config::capture_retention;
use devflow_core::gates::{
    AwaitOutcome, ClaimedAnswer, CloseReport, GateAction, GateAnswer, GateId, Gates,
};
use devflow_core::mode::Mode;
use devflow_core::outcome_policy::{self, Action};
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt;
use devflow_core::stage::Stage;
use devflow_core::state::SupervisorRecord;
use devflow_core::state::{AgentKind, ApprovedCheckpoint, CheckpointApproval, State};
use devflow_core::supervisor::runtime::{
    AgentGroupEnd, AgentOutcome, OwnerPhase, STOPPED_EXIT_CODE, SafePoint, StopToken,
};
use devflow_core::verify::CheckpointDeclaration;
use devflow_core::{
    agent_result, agents, canary, events, gsd_config, mode, monitor, verify, workflow,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// How long a supervisor may stay only pending, from the moment it serves, in a
/// release build (D-15). Past it the launcher is taken to have died.
///
/// The margin: the launcher's `wait_for_publish` polls every `PUBLISH_POLL` (20 ms) and
/// sees the serve within one probe, whose reply read is bounded by
/// `supervisor::protocol::IO_TIMEOUT` (2 s). `PUBLISH_WAIT` (10 s) bounds spawn-to-serve,
/// before this wait starts, so it is not on this path. The promotion write then waits
/// for at most one write-lock holder that honours the closure contract, shorter than
/// `workflow::WRITE_LOCK_REPAIR_WAIT` (10 s), then does its own load, write and fsyncs.
/// 2 s + 10 s leaves at least 3 s of 15 s for the poll, the launcher's own closure and
/// its fsyncs, and `promotion_wait_covers_a_live_launchers_serve_to_promotion_latency`
/// pins `IO_TIMEOUT + WRITE_LOCK_REPAIR_WAIT + 2 s <= PROMOTION_WAIT_RELEASE`.
///
/// What it does not cover, and why that fails closed: it assumes one holder ahead of the
/// launcher, and the promotion is a blocking workflow write. A holder that breaks the
/// closure contract (SIGSTOPped or in D-state), or several queued ahead, delay it past
/// the wait. Then the supervisor ends its agent group and exits writing nothing. When
/// the promotion lands, `publish_record` probes the successor and records `stopped` with
/// [`NEXT_STOPPED_BEFORE_SAVE_REASON`], or, if the probe still caught it running, the
/// state names a supervisor that then reads GONE and `status` suggests `resume`. No
/// second writer and no lost update either way. Bounding the promotion write was weighed
/// and not chosen: its failure path is itself a blocking workflow write behind the same
/// holder.
const PROMOTION_WAIT_RELEASE: Duration = Duration::from_secs(15);

/// How long a supervisor may stay only pending: [`PROMOTION_WAIT_RELEASE`], or 5 s in a
/// test build so a test can reach the expiry.
///
/// The margin in [`PROMOTION_WAIT_RELEASE`]'s doc holds for the release value only:
/// `IO_TIMEOUT` and `WRITE_LOCK_REPAIR_WAIT` keep their release values under test, and
/// `WRITE_LOCK_REPAIR_WAIT` is not shrunk there because the bounded-wait tests need the
/// real 10 s. So a test whose supervisor must be promoted never holds the state write
/// lock, or delays its launcher's promotion write any other way, for more than about 3 s.
#[cfg(not(test))]
const PROMOTION_WAIT: Duration = PROMOTION_WAIT_RELEASE;
#[cfg(test)]
const PROMOTION_WAIT: Duration = Duration::from_secs(5);

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

/// The prompt a launch with no override gives the agent, rendered by `driver`
/// (Phase 51 D-09). At Code it replays the persisted fix pass
/// ([`State::code_fix`]), so a same-stage relaunch inside an audit-fix,
/// gaps-only or full-execute pass continues that pass; every other stage gets
/// its default intent. The fix TYPE is persisted and rendered here, at launch,
/// so after a handoff the fallback's own renderer is used.
///
/// The only place a default stage prompt is rendered:
/// [`launch_stage_inner`] (whose prompt the agent receives) and
/// [`launch_stage`] (which resolves the program from it) both call this, which
/// `every_no_override_launch_renders_through_default_stage_prompt` pins.
fn default_stage_prompt(state: &State, driver: &dyn agents::AgentDriver) -> String {
    let intent = match state.stage {
        Stage::Code => prompt::StageIntent::Code {
            phase: state.phase,
            fix: state.code_fix,
        },
        stage => {
            prompt::StageIntent::for_stage_in_project(stage, state.phase, Some(&state.project_root))
        }
    };
    driver.render_prompt(&intent)
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
    let prompt = prompt_override.unwrap_or_else(|| default_stage_prompt(state, driver.as_ref()));
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
    // `infra_failures`). Persisted below by `spawn_agent_and_record`'s first
    // commit — no extra save needed here.
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
/// Entry-point name recorded by [`resume_with_fallback`].
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
            let recorded = outcome.clone();
            commit(state, |s| s.canary = Some(recorded))?;
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
///
/// One exception: a failover relaunch ([`FailoverRelaunchGuard`] engaged) keeps the
/// record. WR-04's premise does not hold there, since that relaunch runs the same
/// stage again after a handoff. Keeping the record lets the failover's never-silent
/// end (the infra gate, or the abort at the infra ceiling) be written by the
/// supervisor the state records, which then waits at the gate as the run's owner. The
/// exemption lasts for the failover relaunch alone: a launch the gate's answer starts
/// later runs unguarded and clears the record on failure as WR-04 says.
fn refuse_launch(state: &mut State, message: String) -> Result<(), CliError> {
    if !in_failover_relaunch() {
        commit(state, |s| s.supervisor = None)?;
    }
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
    /// The inherited descriptor holding this launch's liveness lock.
    pub(crate) liveness_fd: Option<i32>,
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

/// The stop reason a foreground holder records when it stops at a safe point because
/// `devflow stop` signalled it (D-18).
pub(crate) const HOLDER_STOP_REASON: &str = "stopped by devflow stop at a foreground safe point";

/// The bound on the two waits a stop can put between two supervisors
/// (50-13, T-50-54): a stopped supervisor's wait for the state write lock
/// before it records its stop, and safe point 3's wait for an unpublished
/// next supervisor's final `stopped` line. `2 × TERMINATE_VERIFY_WAIT + 5 s`:
/// the next supervisor's kill grace plus margin. `stop`'s own wait for a
/// final line covers it (`commands::shutdown_final_timeout`).
pub(crate) const SUPERVISOR_STOP_WAIT: Duration =
    Duration::from_secs(2 * devflow_core::agent::TERMINATE_VERIFY_WAIT.as_secs() + 5);

/// What a supervisor's advance reads on this thread (50-13): its stop token,
/// for the advance's safe points, its launch id, for the ownership fence (D-03),
/// and the successor the advance recorded.
struct EngagedAdvance {
    token: StopToken,
    launch_id: String,
    successor: Option<SupervisorRecord>,
}

thread_local! {
    /// Set only while [`StopTokenGuard`] is alive. Thread-local, not a
    /// process-global static, so parallel in-crate tests never share it; an
    /// advance run by anything other than a supervisor (the hidden
    /// `devflow advance`, a test calling `advance_with` directly) sees `None`,
    /// and its safe points read the holder's stop flag instead (D-18).
    static ENGAGED_ADVANCE: std::cell::RefCell<Option<EngagedAdvance>> =
        const { std::cell::RefCell::new(None) };
}

/// Makes a supervisor's stop token visible to the safe points of the
/// advance running on this thread, for as long as the guard lives (RAII,
/// the [`AutoChainGuard`] shape). Dropping it clears the token and the
/// successor slot.
pub(crate) struct StopTokenGuard {
    /// Keeps the guard on the thread whose slot it set.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl StopTokenGuard {
    pub(crate) fn engage(token: StopToken, launch_id: String) -> Self {
        ENGAGED_ADVANCE.with(|slot| {
            *slot.borrow_mut() = Some(EngagedAdvance {
                token,
                launch_id,
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

/// The launch id of the supervisor whose advance runs on this thread, or `None` for
/// any other advance (a hand-run `devflow advance`, which stays unfenced).
fn engaged_launch_id() -> Option<String> {
    ENGAGED_ADVANCE.with(|slot| slot.borrow().as_ref().map(|e| e.launch_id.clone()))
}

/// [`workflow::update_state`], fenced to the engaged supervisor's launch id inside a
/// supervisor's advance (D-03): a supervisor the state no longer records gets
/// [`workflow::WorkflowError::NotOwner`] and writes nothing.
fn update_engaged_state<T>(
    project_root: &Path,
    phase: PhaseId,
    apply: impl FnOnce(&mut State) -> Result<T, CliError>,
) -> Result<(State, T), CliError> {
    match engaged_launch_id() {
        Some(launch_id) => workflow::update_owned_state(project_root, phase, &launch_id, apply),
        None => workflow::update_state(project_root, phase, apply),
    }
}

/// Whether `err` is the ownership fence's refusal (D-03).
pub(crate) fn is_not_owner(err: &CliError) -> bool {
    matches!(
        err,
        CliError::Workflow(workflow::WorkflowError::NotOwner { .. })
    )
}

/// Run a gate side effect under the ownership fence (D-03, R-09). Inside a supervisor's
/// advance this is [`workflow::while_owner`], so the check and `op` share one hold of the
/// state write lock: a supersession is itself a state write under that lock, and lands
/// wholly before the check (`op` is refused with `NotOwner`) or wholly after `op`.
/// Outside one (a hand-run `devflow advance`, the foreground `devflow ship`) it is `op`.
///
/// `op` is a bounded file operation that calls `Gates::*` directly. The gate lock is the
/// only lock it may take, as the D-08 ruling of 2026-10-26 allows (see
/// [`workflow::while_owner`]).
///
/// Not reentrant: engaged, this takes the state write lock, so it must not be called
/// inside an `update_state`, `update_state_with`, `update_owned_state`,
/// `clear_state_if_owned` or `while_owner` closure, and `op` must not call `fenced`,
/// [`close_gate`] or [`commit`]. A second `flock` on the write-lock file from one process
/// blocks forever. A site that needs a gate side effect and a state write does them one
/// after the other, each under its own hold.
pub(crate) fn fenced<T>(
    project_root: &Path,
    phase: PhaseId,
    op: impl FnOnce() -> Result<T, CliError>,
) -> Result<T, CliError> {
    match engaged_launch_id() {
        Some(launch_id) => workflow::while_owner(project_root, phase, &launch_id, op),
        None => op(),
    }
}

/// Refuse with `NotOwner` a supervisor the state no longer records, before a side effect
/// the fence cannot hold its lock across (D-08), such as a checkout hook that merges, tags
/// and pushes. A check, not a fence: a supersession landing after it is not seen until the
/// next fenced step. It writes nothing, and passes outside a supervisor's advance.
pub(crate) fn ensure_still_recorded(project_root: &Path, phase: PhaseId) -> Result<(), CliError> {
    fenced(project_root, phase, || Ok(()))
}

/// `Gates::close` under the ownership fence ([`fenced`], and its non-reentrancy rule: this
/// must not be called inside a state-write or `while_owner` closure). `commands::start`
/// keeps its direct `Gates::close`: it runs before any supervisor exists.
pub(crate) fn close_gate(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
) -> Result<CloseReport, CliError> {
    fenced(project_root, phase, || {
        Ok(Gates::close(project_root, phase, stage)?)
    })
}

/// Remove the run's state: as the engaged supervisor when there is one
/// ([`workflow::clear_state_if_owned`]), unfenced otherwise.
pub(crate) fn clear_run_state(project_root: &Path, phase: PhaseId) -> Result<bool, CliError> {
    Ok(match engaged_launch_id() {
        Some(launch_id) => workflow::clear_state_if_owned(project_root, phase, &launch_id)?,
        None => workflow::clear_state(project_root, phase)?,
    })
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
/// Inside a supervisor's advance this is an owned write (D-03): a supervisor the state
/// no longer records gets [`workflow::WorkflowError::NotOwner`] and writes nothing. A
/// hand-run `devflow advance` stays unfenced.
///
/// `apply` runs under the state write lock, so it never spawns a process, runs
/// hooks, waits on a gate or takes another lock.
pub(crate) fn commit(state: &mut State, apply: impl FnOnce(&mut State)) -> Result<(), CliError> {
    let (committed, ()) = update_engaged_state(&state.project_root, state.phase, |s| {
        apply(s);
        Ok(())
    })?;
    *state = committed;
    Ok(())
}

/// CR-03 (49-REVIEW.md): commit the notify command this process's environment
/// resolves to, where `resume` and a hand-run `advance` read it, so it reaches the
/// chain they launch whatever they do next. With no change, nothing is written: the
/// detached chain carries no variable and keeps the persisted command.
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
/// engaged supervisor's stop has been requested (the run is then marked
/// ended as a stop), or, outside a supervisor's advance, when this foreground holder
/// caught a stop signal (D-18). `false` with neither.
pub(crate) fn stop_requested_at_safe_point() -> bool {
    with_stop_token(StopToken::safe_point) == Some(SafePoint::StopHere) || holder_stop_requested()
}

/// Whether this thread runs outside a supervisor's advance in a process whose D-18
/// handler caught a stop signal. A supervisor's own token always decides inside its
/// advance.
fn holder_stop_requested() -> bool {
    with_stop_token(|_| ()).is_none() && devflow_core::liveness::holder_stop_signal().is_some()
}

/// The reason a stop at a safe point on this thread records: [`ADVANCE_STOP_REASON`]
/// inside a supervisor's advance, [`HOLDER_STOP_REASON`] anywhere else (D-18).
pub(crate) fn safe_point_stop_reason() -> &'static str {
    if with_stop_token(|_| ()).is_some() {
        ADVANCE_STOP_REASON
    } else {
        HOLDER_STOP_REASON
    }
}

/// Persist a stop an advance honoured at `safe_point`: `stopped` with
/// [`safe_point_stop_reason`] and `gate_pending` cleared in one write, and an
/// `advance_stopped` event naming the point and whose stop it was.
pub(crate) fn record_advance_stop(state: &mut State, safe_point: u8) -> Result<(), CliError> {
    let reason = safe_point_stop_reason();
    let source = if with_stop_token(|_| ()).is_some() {
        "supervisor"
    } else {
        "foreground_holder"
    };
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

/// Safe point 3's shutdown of a next supervisor that was spawned and
/// answered its ping but is not published: ask it to stop through its own
/// socket, bounded by [`SUPERVISOR_STOP_WAIT`]. It records nothing itself
/// (the state does not name it as supervisor), so this launcher's stop is the
/// record. Whether the stop was confirmed.
fn shut_down_unpublished(record: &SupervisorRecord) -> bool {
    match devflow_core::supervisor::request_shutdown(&record.socket_path, SUPERVISOR_STOP_WAIT) {
        Ok(devflow_core::supervisor::ShutdownOutcome::Stopped {
            group_signalled, ..
        }) => {
            info!(
                "stopped the unpublished next supervisor at {}",
                record.socket_path.display()
            );
            if group_signalled {
                with_stop_token(StopToken::note_group_signalled);
            }
            true
        }
        Ok(other) => {
            warn!(
                "the unpublished next supervisor at {} answered {other:?} to shutdown",
                record.socket_path.display()
            );
            false
        }
        Err(err) => {
            warn!(
                "could not stop the unpublished next supervisor at {}: {err}",
                record.socket_path.display()
            );
            false
        }
    }
}

/// Safe point 3's stop: end the unpublished next supervisor through its socket, and
/// record the stop. A confirmed shutdown clears its pending record and gives its
/// reservation back. An unconfirmed one keeps the record, because that next supervisor
/// may still run and the record is the only thing naming it: the lock it holds reads
/// live until it exits, and abandoned after, when the next launch clears it.
fn end_unpublished(
    state: &mut State,
    record: &SupervisorRecord,
    reservation: monitor::Reservation,
) -> Result<(), CliError> {
    if shut_down_unpublished(record) {
        commit(state, |s| {
            if reservation_is_pending(s, &reservation) {
                s.pending_supervisor = None;
            }
        })?;
        reservation.discard();
    }
    record_advance_stop(state, 3)
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
/// about a second as [`GateWaitEnd::Stopped`]. Anywhere else (the hidden `devflow
/// advance`, a foreground gate) the holder's stop flag cancels it the same way (D-18).
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
    let outcome =
        await_bound_answer(
            project_root,
            phase,
            stage,
            gate_id,
            timeout_secs,
            || match &token {
                Some(token) => token.stop_requested(),
                None => devflow_core::liveness::holder_stop_signal().is_some(),
            },
        );
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
                fenced(project_root, phase, || {
                    Gates::set_aside_unbound(project_root, phase, stage, gate_id).map_err(|err| {
                        CliError::Message(format!(
                            "phase {phase}: the wait for {stage} gate {gate_id} ended: the file \
                             in its answer slot does not bind to it ({reason}) and was not set \
                             aside: {err}"
                        ))
                    })
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

/// The `__monitor` process body (50-11): supervise the stage's agent under
/// an addressed supervisor (`monitor::run_supervised_stage`), then, only
/// when the agent completed on its own, advance the stage machine
/// in-process (D-02). A stopped run is recorded as stopped and never
/// advances.
///
/// The promotion (D-15, Pitfall 5): a supervisor advances only once the state records
/// it. The promotion watch in `run_supervised_stage` reads "only pending" as wait, so a
/// fast agent's advance cannot run before its launcher's publish save; with no long
/// lock this is what orders the two.
///
/// The handoff (50-13, R-1): the advance runs with this supervisor's stop
/// token engaged for its safe points and its launch id engaged for the ownership
/// fence (D-03) and, when it launches the next stage, records the next supervisor
/// before it returns.
/// Only after it has returned is this supervisor marked
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

    let run = monitor::run_supervised_stage(spec).map_err(|err| {
        // Its stderr is `/dev/null`, so the event is the only record of this failure.
        events::emit(
            project_root,
            phase,
            "supervisor_failed",
            serde_json::json!({ "launch_id": invocation.launch_id, "error": err.to_string() }),
        );
        CliError::Message(format!("stage supervisor failed: {err}"))
    })?;
    // D-03: another launch owns the run, and so the flag: its supervisor set it for its own
    // agent. With no other owner (no record, or a promotion that never came) nothing else
    // would clear it before the next launch's repair, so this supervisor still does.
    if matches!(
        run.promotion,
        Some(monitor::Promotion::Superseded { recorded: Some(_) })
    ) {
        std::mem::forget(auto_chain_guard);
    } else {
        drop(auto_chain_guard);
    }

    // D-03, D-15: a supervisor the state does not record never advances, writes no state
    // and records no stop, however its agent ended. An orphan never promotes itself.
    let not_owner = match &run.promotion {
        Some(monitor::Promotion::Superseded { recorded }) => {
            Some((recorded.clone(), SUPERSEDED_REASON))
        }
        Some(monitor::Promotion::OrphanedPending) => Some((
            workflow::load_state(project_root, phase)
                .ok()
                .and_then(|state| state.supervisor)
                .map(|record| record.launch_id),
            ORPHANED_PENDING_REASON,
        )),
        _ => None,
    };
    let result = if let Some((recorded, reason)) = not_owner {
        end_superseded(invocation, &run, recorded, reason);
        Ok(())
    } else {
        match run.outcome {
            AgentOutcome::Completed(_) => match &run.promotion {
                Some(monitor::Promotion::Recorded) if advance => {
                    let stop_guard = StopTokenGuard::engage(
                        run.supervisor.token(),
                        invocation.launch_id.clone(),
                    );
                    let advanced = advance_for_stage(project_root, Some(phase), stage);
                    // The advance has returned. Only a run whose advance published a
                    // successor is handed off.
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
                    match advanced {
                        // D-03: the fence refused a write or a gate side effect.
                        Err(CliError::Workflow(workflow::WorkflowError::NotOwner {
                            recorded,
                            ..
                        })) => {
                            end_superseded(invocation, &run, recorded, SUPERSEDED_REASON);
                            Ok(())
                        }
                        other => {
                            record_safe_point_stop_exit(&run.supervisor.token(), invocation);
                            other
                        }
                    }
                }
                Some(monitor::Promotion::Recorded) => Ok(()),
                // A stop reached this supervisor while it waited for its promotion, with
                // its agent already gone: the run ends as a stop, and `finish` says so.
                None if run.supervisor.token().stop_requested() => {
                    let _ = run.supervisor.token().safe_point();
                    record_supervisor_stop(project_root, phase, &invocation.launch_id);
                    record_safe_point_stop_exit(&run.supervisor.token(), invocation);
                    Ok(())
                }
                other => Err(CliError::Message(format!(
                    "phase {phase}: launch {} did not advance: its promotion verdict is {other:?}",
                    invocation.launch_id
                ))),
            },
            _ => {
                record_supervisor_stop(project_root, phase, &invocation.launch_id);
                Ok(())
            }
        }
    };
    // Never a `stopped` line for a completed run: `finish` decides the final
    // line from the recorded outcome, so a shutdown that arrived while
    // advancing is answered `handed_off`.
    run.supervisor.finish();
    if let Some(hold) = run.liveness {
        remove_liveness_file(&hold);
    }
    result
}

/// D-16: a stop honoured after the agent completed leaves the exit file with the agent's own
/// code, or none once the next stage's launch archived it, beside a reply saying 143. Record
/// the stop outcome before `finish` sends that reply. A run handed off wrote nothing here,
/// since its successor owns the file, and a superseded supervisor never gets here. Its stderr
/// is `/dev/null`, so a failed write is an event, and it does not change the outcome.
fn record_safe_point_stop_exit(token: &StopToken, invocation: &MonitorInvocation) {
    if !token.ended_stopped() || token.phase() == OwnerPhase::HandedOff {
        return;
    }
    let path = agent_result::exit_code_path(&invocation.project_root, invocation.phase);
    if let Err(err) = std::fs::write(&path, format!("{STOPPED_EXIT_CODE}\n")) {
        warn!("could not record the stop in {}: {err}", path.display());
        events::emit(
            &invocation.project_root,
            invocation.phase,
            "stop_exit_file_failed",
            serde_json::json!({ "path": path.display().to_string(), "error": err.to_string() }),
        );
    }
}

/// The `supervisor_superseded` reason for a supervisor the state does not record.
const SUPERSEDED_REASON: &str = "superseded";

/// The `supervisor_superseded` reason for a supervisor whose promotion never came
/// (D-15): its promoter died.
const ORPHANED_PENDING_REASON: &str = "orphaned_pending";

/// What a supervisor does on finding it does not own the run (D-03): end its agent's
/// group if the agent still runs, and record that as a `supervisor_superseded` event,
/// because a production supervisor's stderr is `/dev/null`. It writes no state and
/// records no stop: a superseded but live supervisor cannot record its own stop
/// ([`record_supervisor_stop`] writes nothing for a launch the state does not record).
fn end_superseded(
    invocation: &MonitorInvocation,
    run: &monitor::StageRun,
    recorded: Option<String>,
    reason: &str,
) {
    let group_end = run
        .agent_group_end
        .unwrap_or_else(|| run.supervisor.end_agent_group());
    let agent_group = match group_end {
        AgentGroupEnd::Ended => "ended",
        _ => "already_exited",
    };
    warn!(
        "phase {}: launch {} ends without writing ({reason}): the state records {recorded:?}; \
         its agent group: {agent_group}",
        invocation.phase, invocation.launch_id
    );
    events::emit(
        &invocation.project_root,
        invocation.phase,
        "supervisor_superseded",
        serde_json::json!({
            "ours": invocation.launch_id,
            "recorded": recorded,
            "stage": invocation.stage.map(|stage| stage.to_string()),
            "reason": reason,
            "agent_group": agent_group,
        }),
    );
}

/// A supervisor's last act (D-01): remove its liveness file while it still holds
/// the lock. A file left behind reads abandoned once this process exits, and
/// `recover --clean` sweeps it, so a failure is only warned about.
fn remove_liveness_file(hold: &devflow_core::liveness::Hold) {
    if let Err(err) = std::fs::remove_file(hold.path()) {
        warn!(
            "could not remove the liveness file {}: {err}",
            hold.path().display()
        );
    }
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
        if let Ok(devflow_core::supervisor::ShutdownOutcome::Stopped {
            group_signalled: true,
            ..
        }) = outcome
        {
            token.note_group_signalled();
        }
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
    spec.liveness_fd = invocation.liveness_fd;
    spec.promotion_wait = Some(PROMOTION_WAIT);
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
/// [`SUPERVISOR_STOP_REASON`], under the state write lock. Never advances.
///
/// Only when the state names THIS launch (50-13, T-50-54): checked without
/// the lock first, then again under it. A next supervisor that was never
/// published (safe point 3 stops it and its launcher records the stop itself)
/// and a superseded supervisor (the state names its successor, whose run it
/// must not mark stopped) both write nothing.
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
    // Bounded (T-50-68): a wedged write-lock holder must not keep this supervisor from
    // reaching `finish`.
    let written =
        workflow::update_state_with_within(project_root, phase, SUPERVISOR_STOP_WAIT, |existing| {
            match existing {
                workflow::Existing::Loaded(mut state) if names_this_launch(&state) => {
                    state.stopped = true;
                    state.stop_reason = Some(SUPERVISOR_STOP_REASON.to_string());
                    Ok((Some(state), true))
                }
                workflow::Existing::Loaded(_) => Ok((None, false)),
                workflow::Existing::Missing => {
                    Err(workflow::WorkflowError::MissingState(path.clone()))
                }
                workflow::Existing::Unloadable(err) => Err(err),
            }
        });
    match written {
        Ok(true) => info!("phase {phase}: {SUPERVISOR_STOP_REASON}"),
        Ok(false) => {
            info!("phase {phase}: launch {launch_id} was superseded before its stop was recorded");
        }
        Err(err) => not_recorded(&format!("the state was not updated ({err})")),
    }
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
/// A reservation the failed launch made ends with it: a `pending_supervisor` naming
/// this launch is cleared too (D-01), always.
///
/// One exception to the record clear: a failover relaunch ([`FailoverRelaunchGuard`]
/// engaged) keeps `supervisor`. WR-04's premise does not hold there, since that
/// relaunch runs the same stage again after a handoff. Keeping the record lets the
/// failover's never-silent end (the infra gate, or the abort at the infra ceiling) be
/// written by the supervisor the state records, which then waits at the gate as the
/// run's owner. The exemption lasts for the failover relaunch alone: a launch the
/// gate's answer starts later runs unguarded and clears the record on failure as
/// WR-04 says.
///
/// Called only on a failure path, never before the save that records a new
/// record (50-13's rule).
fn record_failed_launch(state: &mut State) -> Result<(), CliError> {
    let failed = state.pending_supervisor.clone();
    let keep_record = in_failover_relaunch();
    commit(state, |s| {
        if !keep_record {
            s.supervisor = None;
        }
        if failed.is_some() && s.pending_supervisor == failed {
            s.pending_supervisor = None;
        }
    })
}

thread_local! {
    /// Set only while a [`FailoverRelaunchGuard`] is alive. Per thread, like
    /// [`ENGAGED_ADVANCE`]: the relaunch it covers runs on the advance's thread, and
    /// parallel in-crate tests never share it.
    static FAILOVER_RELAUNCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marks the launch running on this thread as a failover relaunch (SUPV-02, D-04),
/// for as long as the guard lives (RAII: an early return or a panic cannot leave it
/// set). While it is engaged, [`refuse_launch`] and [`record_failed_launch`] keep
/// `state.supervisor`, so the never-silent end that follows a failed relaunch is a
/// set of owned writes by the supervisor the state records (Phase 50 D-03 unchanged).
/// Engaged only around the `launch_stage` call in [`fail_over_on_rate_limit`].
struct FailoverRelaunchGuard {
    /// Keeps the guard on the thread whose flag it set.
    _not_send: std::marker::PhantomData<*const ()>,
}

impl FailoverRelaunchGuard {
    fn engage() -> Self {
        FAILOVER_RELAUNCH.with(|engaged| engaged.set(true));
        Self {
            _not_send: std::marker::PhantomData,
        }
    }
}

impl Drop for FailoverRelaunchGuard {
    fn drop(&mut self) {
        FAILOVER_RELAUNCH.with(|engaged| engaged.set(false));
    }
}

/// Whether a [`FailoverRelaunchGuard`] is engaged on this thread.
fn in_failover_relaunch() -> bool {
    FAILOVER_RELAUNCH.with(std::cell::Cell::get)
}

thread_local! {
    /// The reservation `start` or `resume` made at its top, until the launch that
    /// spawns from it takes it ([`take_held_reservation`]). Per thread because a
    /// launcher runs on one thread and a test may run two launchers in one process.
    static HELD_RESERVATION: std::cell::RefCell<Option<monitor::Reservation>> =
        const { std::cell::RefCell::new(None) };
}

/// Hand `reservation` to the launch this thread runs next.
pub(crate) fn hold_reservation(reservation: monitor::Reservation) {
    HELD_RESERVATION.with_borrow_mut(|held| *held = Some(reservation));
}

/// The reservation [`hold_reservation`] was given, if nothing has taken it yet.
fn take_held_reservation() -> Option<monitor::Reservation> {
    HELD_RESERVATION.with_borrow_mut(Option::take)
}

/// Give back the reservation this thread still holds: a launch that failed, or that
/// returned without spawning from it. A no-op once a launch has taken it.
///
/// The state stops naming it, and a state the reservation itself `created` (no run
/// was ever written into it) is removed. The liveness lock is released last, so no
/// other launcher can reserve this phase in between. A failure here is only warned
/// about: what is left reads abandoned, and the next launch clears it.
pub(crate) fn abandon_reservation(project_root: &Path, phase: PhaseId, created: bool) {
    let Some(reservation) = take_held_reservation() else {
        return;
    };
    if let Err(err) = clear_reservation(project_root, phase, reservation.launch_id(), created) {
        eprintln!(
            "warning: could not clear launch reservation {} from phase {phase}'s state: {err}",
            reservation.launch_id()
        );
    }
    reservation.discard();
}

/// The state half of [`abandon_reservation`].
fn clear_reservation(
    project_root: &Path,
    phase: PhaseId,
    launch_id: &str,
    created: bool,
) -> Result<(), CliError> {
    let remove_state = workflow::update_state_with(project_root, phase, |existing| {
        let workflow::Existing::Loaded(mut state) = existing else {
            return Ok::<_, CliError>((None, false));
        };
        let names_it = state
            .pending_supervisor
            .as_ref()
            .is_some_and(|pending| pending.launch_id == launch_id);
        if !names_it {
            return Ok((None, false));
        }
        if created {
            return Ok((None, true));
        }
        state.pending_supervisor = None;
        Ok((Some(state), false))
    })?;
    if remove_state {
        workflow::clear_state(project_root, phase)?;
    }
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
    let checkpoint_resumes = state.checkpoint_resumes;
    let verification_run_nonce = state.verification_run_nonce;
    let last_verification_fingerprint = state.last_verification_fingerprint;
    let last_verification_mtime_nanos = state.last_verification_mtime_nanos;
    let verification_baseline_captured = state.verification_baseline_captured;
    commit(state, |s| {
        s.checkpoint_resumes = checkpoint_resumes;
        s.verification_run_nonce = verification_run_nonce;
        s.last_verification_fingerprint = last_verification_fingerprint;
        s.last_verification_mtime_nanos = last_verification_mtime_nanos;
        s.verification_baseline_captured = verification_baseline_captured;
    })?;

    // WR-04 (18-fix, 50-23 T-50-62): every error before the publish save
    // ends the launch with no supervisor record (see `record_failed_launch`),
    // so a failed relaunch never leaves `state.stage` advanced beside a
    // record naming the previous stage's supervisor; the success path below
    // never clears the record before the save that replaces it. One exception:
    // a failover relaunch (`FailoverRelaunchGuard` engaged) keeps the record,
    // because it runs the same stage again after a handoff, so the failover's
    // never-silent end (the infra gate, or the abort at the infra ceiling) is
    // written by the supervisor the state records, which then waits at the gate.
    let SpawnedLaunch {
        published,
        reservation,
        ctx,
        launch_label,
    } = match spawn_next_supervisor(state, program, args, extra_env, archived_stage, launch) {
        Ok(Some(spawned)) => spawned,
        // A stop honoured at safe point 2 or 3 (50-13), already recorded.
        Ok(None) => return Ok(()),
        // A superseded supervisor's launch ends here: the run is not its to mark.
        Err(err) if is_not_owner(&err) => return Err(err),
        Err(err) => {
            return Err(match record_failed_launch(state) {
                Ok(()) => err,
                Err(clear_err) if is_not_owner(&clear_err) => clear_err,
                Err(clear_err) => CliError::Message(format!(
                    "{err}; clearing the supervisor record after it also failed: {clear_err}"
                )),
            });
        }
    };
    let pid = published.supervisor_pid;
    let socket_path = published.record.socket_path.display().to_string();
    let launch_id = published.record.launch_id.clone();
    // `transition()` commits BEFORE `launch_stage`, so a record held only in
    // memory here is lost unless it is written again (18b). The ping-confirmed
    // supervisor record replaces any previous one in this same save; it is never
    // cleared first.
    //
    // The handoff (50-13, R-1): this save is the publish. The previous
    // supervisor, when this launch is its advance, stays alive and
    // `Advancing` through it (RV-04: never handed off here) and only
    // unlinks its own socket after the advance has returned, so a reader
    // between the two always finds a live address.
    #[cfg(test)]
    test_hold("before_record_save");
    #[cfg(test)]
    launch_test_hold("before_publish_save");
    let record = published.record.clone();
    match publish_record(state, published, save_publish_fields)? {
        PublishOutcome::HandedOff => record_successor(record),
        PublishOutcome::SuccessorAlreadyEnded => {}
    }
    // The publish is done, so the launcher's copy of the liveness lock goes.
    // Only dropping releases it: `unlock` would also release the supervisor's.
    drop(reservation);
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
///   [`NEXT_STOPPED_BEFORE_SAVE_REASON`]. That second write is made as the
///   successor ([`save_successor_stop`]), because the state records it by then.
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
        return Err(shut_down_unrecorded(
            state,
            &record,
            published.supervisor_pid,
            err.into(),
        ));
    }
    promote_pending(state);
    #[cfg(not(test))]
    let probe = devflow_core::supervisor::probe;
    #[cfg(test)]
    let probe = successor_probe_for(state);
    if successor_runs_on(&record.socket_path, probe) {
        return Ok(PublishOutcome::HandedOff);
    }
    state.stopped = true;
    state.stop_reason = Some(NEXT_STOPPED_BEFORE_SAVE_REASON.to_string());
    save_successor_stop(state)?;
    info!(
        "phase {}: {NEXT_STOPPED_BEFORE_SAVE_REASON} (launch {})",
        state.phase, record.launch_id
    );
    Ok(PublishOutcome::SuccessorAlreadyEnded)
}

/// The publish's promotion (D-01): the reservation ends once the state records the
/// launch it reserved. A reservation naming another launch stays.
fn promote_pending(state: &mut State) {
    let recorded = state.supervisor.as_ref().map(|r| r.launch_id.as_str());
    let promoted = state
        .pending_supervisor
        .as_ref()
        .is_some_and(|pending| Some(pending.launch_id.as_str()) == recorded);
    if promoted {
        state.pending_supervisor = None;
    }
}

/// [`publish_record`]'s production save: the record and its promotion, the last write
/// a supervisor makes as the recorded one. A `stop` another writer landed while this
/// launch ran is on disk and is not this launcher's to clear.
///
/// The promotion happens only while the state still names this launch (D-01): a
/// reservation another writer cleared or replaced refuses the save, and
/// [`publish_record`] then shuts this supervisor down through its socket. Inside a
/// supervisor's advance it is also an owned write (D-03), so a superseded supervisor
/// promotes nothing.
fn save_publish_fields(published: &State) -> Result<(), CliError> {
    let launch_id = published.supervisor.as_ref().map(|r| r.launch_id.as_str());
    update_engaged_state(&published.project_root, published.phase, |s| {
        let pending_id = s.pending_supervisor.as_ref().map(|p| p.launch_id.as_str());
        let recorded_id = s.supervisor.as_ref().map(|r| r.launch_id.as_str());
        if launch_id.is_none() || (pending_id != launch_id && recorded_id != launch_id) {
            return Err(CliError::Message(format!(
                "phase {}'s state no longer names launch {} as pending",
                s.phase,
                launch_id.unwrap_or("<none>")
            )));
        }
        s.supervisor = published.supervisor.clone();
        promote_pending(s);
        Ok(())
    })
    .map(|_| ())
}

/// [`publish_record`]'s second write: `stopped` for a successor that had already ended.
/// The state records that successor by now, so it is made as the successor (D-03), not
/// as the engaged supervisor, which the fence would refuse. It only ever sets `stopped`.
fn save_successor_stop(published: &State) -> Result<(), CliError> {
    let launch_id = published
        .supervisor
        .as_ref()
        .map_or("", |record| record.launch_id.as_str());
    workflow::update_owned_state(&published.project_root, published.phase, launch_id, |s| {
        s.stopped = true;
        s.stop_reason = published.stop_reason.clone();
        Ok::<(), CliError>(())
    })
    .map(|_| ())
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
/// finds it by its sidecar whatever the state says (50-14). That repair refuses a
/// suspended supervisor, which cannot answer, so the error also names `kill -CONT`.
fn shut_down_unrecorded(
    state: &mut State,
    record: &SupervisorRecord,
    supervisor_pid: u32,
    save_err: CliError,
) -> CliError {
    let failed = format!(
        "could not save the record of the next stage's supervisor (launch {}): {save_err}",
        record.launch_id
    );
    // A superseded launcher promotes nothing and clears nothing. Its next supervisor is
    // not needed; it ends through its own socket, and the refusal goes up as itself.
    if is_not_owner(&save_err) {
        let ended =
            devflow_core::supervisor::request_shutdown(&record.socket_path, SUPERVISOR_STOP_WAIT);
        info!(
            "a superseded launcher asked launch {} to stop: {ended:?}",
            record.launch_id
        );
        return save_err;
    }
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
                 still be running; run `devflow stop --phase {} --launch {}` to end it. If `stop` \
                 says it could not probe the socket or the supervisor did not answer, the \
                 supervisor may be suspended: `kill -CONT {supervisor_pid}` lets the stop proceed",
                record.socket_path.display(),
                state.phase,
                record.launch_id
            ))
        }
    }
}

/// What [`successor_runs_on`]'s probe answers: [`devflow_core::supervisor::probe`]'s result.
type SuccessorProbeAnswer =
    Result<devflow_core::supervisor::Liveness, devflow_core::supervisor::ProbeError>;

/// Whether the successor whose record was just saved is still running and
/// can be handed off: Alive with owner `agent_running`, `advancing` or
/// `gate_wait`. Gone, or Alive with any other owner (it finished without
/// advancing), is `false`. STALE is re-probed for
/// [`STALE_REPROBE_WINDOW`]: Gone within it is `false`, and a socket still
/// STALE is `true` (a crashed successor stays recorded and reads STALE, a
/// real finding with its own repair). A probe error is `true`: the record
/// stays and `status` reports the error.
///
/// `probe` is [`devflow_core::supervisor::probe`] in every release build; only the
/// test build's [`successor_probe_for`] can substitute an answer (51-13, D-12), and
/// these arms decide in both.
fn successor_runs_on(socket: &Path, probe: impl Fn(&Path) -> SuccessorProbeAnswer) -> bool {
    use devflow_core::supervisor::Liveness;
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

/// Test-only seam (51-13, D-12, review B-2, R2-8): the probe [`publish_record`] hands
/// [`successor_runs_on`] in a test build. When `DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE`
/// is `<agent>:<stage>:<marker-path>` (lowercase, as `AgentKind` and `Stage` display)
/// for `state`'s agent and stage, it writes the marker file and answers `Gone` without
/// probing, so `successor_runs_on`'s own `Gone` arm records the run stopped while the
/// successor still runs. Otherwise it is the real probe. Keyed by the launch being
/// published, so a failover relaunch (`codex:code`) can be answered while the primary's
/// launch is not. Compiled only under `cfg(test)`, together with its one call (pinned
/// by `pipeline_launch::tests::the_successor_probe_seam_is_compiled_out_of_non_test_builds`).
#[cfg(test)]
fn successor_probe_for(state: &State) -> impl Fn(&Path) -> SuccessorProbeAnswer {
    let marker = std::env::var("DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE")
        .ok()
        .and_then(|value| {
            let mut parts = value.splitn(3, ':');
            let (agent, stage, marker) = (parts.next()?, parts.next()?, parts.next()?);
            (agent == state.agent.to_string() && stage == state.stage.to_string())
                .then(|| PathBuf::from(marker))
        });
    move |socket| match &marker {
        Some(marker) => {
            let _ = std::fs::write(marker, "");
            Ok(devflow_core::supervisor::Liveness::Gone)
        }
        None => devflow_core::supervisor::probe(socket),
    }
}

/// What [`spawn_next_supervisor`] hands back to [`spawn_agent_and_record`]
/// for the publish: the ping-confirmed supervisor, the reservation it was
/// spawned from, the launch context it was spawned under, and the launch
/// shape's label.
struct SpawnedLaunch {
    published: monitor::Published,
    reservation: monitor::Reservation,
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
    let spawn_failed = |err| CliError::Message(format!("could not spawn monitor: {err}"));
    let reservation = match take_held_reservation() {
        Some(held) if reservation_is_pending(state, &held) => held,
        held => {
            if let Some(held) = held {
                held.discard();
            }
            let reservation = monitor::Reservation::new(&ctx, state).map_err(spawn_failed)?;
            if let Err(err) = commit_pending_record(state, &reservation) {
                reservation.discard();
                return Err(err);
            }
            reservation
        }
    };
    #[cfg(test)]
    test_hold("after_pending_save");
    let published =
        match monitor::spawn_monitor(state, program, args, extra_env, launch, &ctx, &reservation) {
            Ok(published) => published,
            Err(err) => {
                reservation.discard();
                return Err(spawn_failed(err));
            }
        };
    #[cfg(test)]
    test_hold("before_publish");
    // Safe point 3 (50-13): the next supervisor answered its ping but is not
    // published. On a stop, shut it down through its socket and never save
    // its record; the state keeps this run's record, marked stopped.
    if stop_requested_at_safe_point() {
        end_unpublished(state, &published.record, reservation)?;
        return Ok(None);
    }
    Ok(Some(SpawnedLaunch {
        published,
        reservation,
        ctx,
        launch_label,
    }))
}

/// Record a launch that no `start` or `resume` reserved, a stage handoff above all, as
/// pending before it is spawned (D-01), so the state names every supervisor that might
/// be live for the phase, whatever happens to this launcher after the spawn (crash-table
/// row C4, T-50-91).
///
/// A record already there is replaced when its lock is not held, and its file removed
/// when it reads abandoned. A held one is a live launch this launcher knows nothing of,
/// so this launch stops, naming both ids: `start` and `resume` are kept off a live
/// supervisor by their guard, and a hand-run `advance` and `devflow ship`'s loop-back by
/// theirs (50.1-29). A supervisor whose own promotion never came never reaches this: it
/// does not advance (D-15).
///
/// Inside a supervisor's advance this is an owned write (D-03): a superseded supervisor
/// gets `NotOwner` and spawns nothing.
///
/// On `Err` this launch wrote nothing, so the caller's copy stops naming any pending
/// record: [`record_failed_launch`] clears the record its copy names, and a record on
/// disk here is another launch's.
fn commit_pending_record(
    state: &mut State,
    reservation: &monitor::Reservation,
) -> Result<(), CliError> {
    use devflow_core::liveness::{self, Held};
    let pending = reservation.pending_record();
    let (mut refused_over, mut cleared) = (None, None);
    let written = update_engaged_state(&state.project_root, state.phase, |s| {
        if let Some(existing) = &s.pending_supervisor
            && !matches!(
                liveness::probe(&existing.liveness_path),
                Ok(Held::Abandoned | Held::Missing)
            )
        {
            refused_over = Some(existing.launch_id.clone());
            return Err(CliError::Message(format!(
                "phase {}: launch {} was not spawned: the state names launch {} as pending and \
                 its liveness lock is held; run `devflow stop --phase {}` to end it",
                s.phase,
                reservation.launch_id(),
                existing.launch_id,
                s.phase
            )));
        }
        if let Some(replaced) = s.pending_supervisor.replace(pending) {
            crate::commands::remove_abandoned_liveness_file(&s.project_root, &replaced);
            cleared = Some(replaced.launch_id);
        }
        Ok(())
    });
    match written {
        Ok((committed, ())) => {
            *state = committed;
            if let Some(launch_id) = cleared {
                println!("cleared an abandoned launch reservation {launch_id}");
            }
            Ok(())
        }
        Err(err) => {
            state.pending_supervisor = None;
            if let Some(pending_id) = refused_over {
                events::emit(
                    &state.project_root,
                    state.phase,
                    "launch_refused_over_live_pending",
                    serde_json::json!({
                        "stage": state.stage.to_string(),
                        "launch_id": reservation.launch_id(),
                        "pending_launch_id": pending_id,
                    }),
                );
            }
            Err(err)
        }
    }
}

/// Whether `state` still names `reservation` as its pending supervisor.
fn reservation_is_pending(state: &State, reservation: &monitor::Reservation) -> bool {
    state
        .pending_supervisor
        .as_ref()
        .is_some_and(|pending| pending.launch_id == reservation.launch_id())
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
    let reason = format!(
        "checkpoint re-scan rejected in Auto mode; phase switched to Supervise for its remaining \
         stages and parked for repair of {}",
        unapproved_files.join(", ")
    );
    commit(state, |s| {
        s.mode = Mode::Supervise;
        s.stopped = true;
        s.gate_pending = false;
        s.stop_reason = Some(match s.stop_reason.take() {
            Some(existing) if !existing.is_empty() => format!("{existing}; {reason}"),
            _ => reason,
        });
    })?;
    close_gate(project_root, state.phase, stage)?;
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
/// cron-resume arm should run instead.
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
/// (major-bump, unattended-launch-shape, interactivity, gh-auth), not just
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
    let mut candidate = state.clone();
    candidate.agent = to;
    if let Err(why) = generic_preflight_checks(project_root, &candidate) {
        return Ok(HandoffOutcome::Refused(HandoffRefusal::Preflight(why)));
    }
    if let Err(err) = ensure_agent_binary(agent_program(to)) {
        return Ok(HandoffOutcome::Refused(HandoffRefusal::Unavailable(
            err.to_string(),
        )));
    }
    if let Err(err) = agents::driver_for(to).health(&candidate) {
        return Ok(HandoffOutcome::Refused(HandoffRefusal::Unavailable(err)));
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

/// Continue a rate-limited Code stage under the unused fallback `to` (SUPV-02).
///
/// Reached only from the explicit `RateLimited` arm of `advance_with`, after
/// [`failover_target`] selected `to`, and BEFORE [`handle_rate_limited_outcome`]'s
/// infra bump: a hop is not an infra fault, so a successful one leaves
/// `infra_failures` alone (D-06). Once handed off, the same stage is relaunched
/// under the new driver — the tail of [`handle_ambiguous_outcome`] without its
/// bump and ceiling check. No commit-count precondition applies (D-08) and the
/// worktree is handed over untouched (D-10).
///
/// A refused handoff is never a silent skip (D-07): `agent_handoff_refused`
/// names the fallback and the refusal, then the cron arm runs exactly as it
/// would have with no fallback, given the ORIGINAL rate-limit reason so its
/// retry time still parses. Only a committed hop consumes the fallback (D-05),
/// so a refusal leaves it unused.
///
/// A relaunch that fails AFTER the handoff commit is never silent either (D-04,
/// R2-D1): `agent_handoff_relaunch_failed` names the stage, both agents and the
/// error, then [`handle_infra_outcome`] bumps `infra_failures` once and opens the
/// never-silent gate, or aborts at the ceiling. Not the cron arm: a refused canary
/// stays recorded and refuses every later launch, so a scheduled resume could not
/// succeed, and a successor whose shutdown was not confirmed may still run. The
/// relaunch runs under [`FailoverRelaunchGuard`], so its failure keeps the
/// supervisor record and those writes stay owned. A `NotOwner` from the relaunch
/// means another launch owns the run: it is returned unchanged and nothing more
/// is written.
fn fail_over_on_rate_limit(
    project_root: &Path,
    state: &mut State,
    phase: PhaseId,
    stage: Stage,
    to: AgentKind,
    reason: Option<String>,
) -> Result<(), CliError> {
    match hand_off_agent(project_root, state, to, HandoffReason::RateLimited)? {
        HandoffOutcome::HandedOff { from } => {
            #[cfg(test)]
            test_hold("after_failover_commit");
            let relaunched = {
                let _keep_record = FailoverRelaunchGuard::engage();
                launch_stage(state, None, Some(stage))
            };
            match relaunched {
                Ok(()) => Ok(()),
                Err(err) if is_not_owner(&err) => Err(err),
                Err(err) => {
                    let error = err.to_string();
                    events::emit(
                        project_root,
                        phase,
                        "agent_handoff_relaunch_failed",
                        serde_json::json!({
                            "stage": stage.to_string(),
                            "from_agent": from.to_string(),
                            "to_agent": to.to_string(),
                            "reason": error,
                        }),
                    );
                    let rate_limit = reason.as_deref().unwrap_or("no reason given");
                    handle_infra_outcome(
                        project_root,
                        state,
                        stage,
                        Some(format!(
                            "the handoff to {to} could not relaunch the {stage} stage: {error} \
                             (after the rate limit: {rate_limit})"
                        )),
                    )
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
    legacy_claude_launch: bool,
) -> Result<(), CliError> {
    resume_with_fallback(project_root, phase, agent, legacy_claude_launch, None)
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
/// The reservation at the top (D-01, D-08) refuses to resume while the phase's
/// supervisor or agent remains live, and writes the pending record in one step with
/// that check; see [`crate::commands::reserve_launch`]. A resume that fails, or
/// returns without spawning from it, gives the reservation back. The phase's holder is
/// claimed first (D-17), so a resume another foreground process refuses reserves nothing.
///
/// D-06: a rerun asks again. Before it relaunches, `resume` closes the phase's gates and
/// reports each answer it set aside and each claimed answer it found unapplied
/// ([`close_gates_for_resume`]). It never applies a pending answer to the rerun's gate.
pub(crate) fn resume_with_fallback(
    project_root: &Path,
    phase: PhaseId,
    agent: Option<AgentKind>,
    legacy_claude_launch: bool,
    fallback_agent: Option<AgentKind>,
) -> Result<(), CliError> {
    let _phase_holder = crate::commands::claim_foreground_holder(project_root, phase)?;
    let reserved = crate::commands::reserve_launch(
        project_root,
        phase,
        crate::commands::LaunchVerb::Resume,
        None,
        devflow_core::registry::cache_dir().as_deref(),
    )?;
    hold_reservation(reserved.reservation);
    #[cfg(test)]
    launch_test_hold("holder_claimed");
    let result = resume_loaded(
        project_root,
        phase,
        reserved.state,
        agent,
        legacy_claude_launch,
        fallback_agent,
    );
    abandon_reservation(project_root, phase, reserved.created);
    result
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
    let ship_caution = if stage == Stage::Ship {
        "; a merge, version bump or tag may already have run, so check them before answering again"
    } else {
        ""
    };
    format!(
        "resume: {subject} was claimed, with no record that it was applied{ship_caution}; the rerun asks again"
    )
}

/// Every line `resume` prints about one stage it closed. `gate_id` is what the stage's gate
/// file named before the close; a stage with no gate file names an answer by the
/// quarantined file's own label.
///
/// `at_current_stage` is whether the run's state names this stage. A claimed pair there is
/// usually an answer whose consumer ended before acting on it, whatever `gate_pending` reads:
/// the consumer writes `gate_pending = false` before its caller acts (R2-05). It can also
/// have been acted on with the gate left open (an `--until` stop, a kill during Ship
/// finalization, a close that failed), so the line says only what is known. A claimed pair at
/// any other stage is the leftover of an answer the run acted on and moved past, so it gets
/// no line here; `Gates::close` has logged its removal either way.
fn resume_gate_lines(
    phase: PhaseId,
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
    // The answer just set aside is out of `devflow ship`'s reach, so this names only what
    // is left: the rerun's gate, and `devflow ship` for a later stored approval (D-06).
    if stage == Stage::Ship && !report.quarantined.is_empty() {
        lines.push(format!(
            "resume: the rerun's Ship gate asks again — answer it with `devflow gate approve \
             {phase} ship`; next time, to apply a stored Ship approval without a rerun, run \
             `devflow ship --phase {phase}` instead of `resume`"
        ));
    }
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
    // Read before the close, which removes the files that name them.
    let gate_ids = [
        Stage::Define,
        Stage::Plan,
        Stage::Code,
        Stage::Validate,
        Stage::Ship,
    ]
    .map(|stage| {
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
        lines.extend(resume_gate_lines(
            phase,
            *stage,
            gate_id,
            report,
            at_current_stage,
        ));
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

/// [`resume_with_fallback`] after its reservation: `state` is the copy that wrote, which may already
/// be behind the file.
fn resume_loaded(
    project_root: &Path,
    phase: PhaseId,
    mut state: State,
    agent: Option<AgentKind>,
    legacy_claude_launch: bool,
    fallback_agent: Option<AgentKind>,
) -> Result<(), CliError> {
    crate::commands::refuse_unprepared_run(&state)?;
    // D-01, D-02, D-03: every fallback check runs here, BEFORE `commit_notify_env`
    // below, so a refused resume commits nothing at all (review B-3).
    refuse_fallback_for_resume(&state, agent, legacy_claude_launch, fallback_agent)?;
    // CR-03 (49-REVIEW.md): an exported (or emptied) notify command reaches
    // the chain this resume launches through the state file.
    commit_notify_env(&mut state)?;
    if let Some(requested) = agent
        && requested != state.agent
    {
        // The shared substitution (D-04): every check runs before anything is
        // mutated, so a refusal leaves the state byte-identical.
        match hand_off_agent(
            project_root,
            &mut state,
            requested,
            HandoffReason::ResumeAgent,
        )? {
            HandoffOutcome::HandedOff { from } => println!("handoff: {from} → {requested}"),
            HandoffOutcome::Refused(HandoffRefusal::Preflight(reason)) => {
                return Err(CliError::Message(format!(
                    "handoff to {requested} refused at saved {} stage: {reason}",
                    state.stage
                )));
            }
            HandoffOutcome::Refused(HandoffRefusal::Unavailable(err)) => {
                return Err(CliError::Message(format!(
                    "handoff to {requested} refused: {err}"
                )));
            }
        }
    }
    // Combined BEFORE the commit below, so the persisted value exists before the
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
    // before the commit below, so the repair is complete before anything
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
    // The stopped test reads the file as it is now: the operator's `resume` is the
    // later word on a stop that landed after this copy loaded, and a run launched
    // while still marked stopped could never advance.
    let legacy_claude_launch = state.legacy_claude_launch;
    // D-06: the rerun asks again, so the gate this run abandons is closed before the
    // relaunch and its unused answer set aside. After every refusal above, so a refused
    // resume leaves the gate as it found it.
    for line in close_gates_for_resume(project_root, phase, state.stage) {
        println!("{line}");
    }
    commit(&mut state, |s| {
        if s.stopped {
            s.stopped = false;
            s.stop_reason = None;
            s.stop_until = None;
        }
        s.gate_pending = false;
        s.legacy_claude_launch = legacy_claude_launch;
        // D-01's re-arm, in the commit every resume makes, so it applies to a stopped
        // run and a cron-paused one alike. A plain resume leaves a spent fallback
        // spent (D-05).
        if let Some(fallback) = fallback_agent {
            s.fallback_agent = Some(fallback);
            s.fallback_consumed = false;
        }
    })?;
    if let Some(fallback) = fallback_agent {
        print_fallback_notice(fallback, state.agent);
    }
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
            let _ = commit(&mut state, |s| {
                s.stopped = true;
                s.stop_reason = Some(format!("resume launch failed: {err}"));
            });
            Err(err)
        }
    }
}

/// The fallback checks `resume` makes before it writes anything (D-01, D-02, D-03).
///
/// The run is judged as it would launch: its agent after `--agent`, and its legacy
/// opt-out after `--legacy-claude-launch` and the environment. Then:
/// - a `--fallback-agent` is held to [`validate_fallback_agent`], as on `start`;
/// - with none, a saved fallback that is still unused and equals that agent is refused
///   (D-02): `--agent` onto the run's own fallback would leave the two coinciding.
///
/// Reads no file and writes nothing; `state` is the copy `resume` loaded.
fn refuse_fallback_for_resume(
    state: &State,
    agent: Option<AgentKind>,
    legacy_claude_launch: bool,
    fallback_agent: Option<AgentKind>,
) -> Result<(), CliError> {
    let mut candidate = state.clone();
    candidate.agent = agent.unwrap_or(state.agent);
    // Only the combined value is read here; the notice is `resume_loaded`'s to print.
    let _env_alone = apply_legacy_launch_opt_out(&mut candidate, legacy_claude_launch);
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

/// The hand-run `devflow advance`: hold the phase for the whole advance (D-17), so
/// `stop` can reach this process and no other foreground process acts on the phase,
/// refuse beside a live supervisor (D-08), then advance. A supervisor's own advance calls
/// [`advance_for_stage`] and claims nothing.
pub(crate) fn advance_command(
    project_root: &Path,
    phase: Option<PhaseId>,
    stage: Option<Stage>,
) -> Result<(), CliError> {
    let phase = resolve_advance_phase(project_root, phase)?;
    let _phase_holder = crate::commands::claim_foreground_holder(project_root, phase)?;
    // After the claim, so no `start` or `resume` can reserve until this advance returns,
    // and its own launch meets no live pending record.
    crate::commands::refuse_advance_over_a_live_supervisor(project_root, phase)?;
    if let Ok(state) = workflow::load_state(project_root, phase) {
        crate::commands::refuse_unprepared_run(&state)?;
    }
    #[cfg(test)]
    launch_test_hold("holder_claimed");
    advance_for_stage(project_root, Some(phase), stage)
}

/// The phase an advance acts on: the given one, or the sole active one.
///
/// 13-DEFERRED-CR-03 fix shape #2: the phase is threaded in by the monitor (recorded at
/// spawn time), so advance's identity never depends on a shared state singleton — under
/// `devflow parallel`, each monitor advances exactly its own phase. The Option fallback
/// only serves monitors spawned by an older binary.
fn resolve_advance_phase(project_root: &Path, phase: Option<PhaseId>) -> Result<PhaseId, CliError> {
    if let Some(phase) = phase {
        return Ok(phase);
    }
    resolve_sole_active_phase(project_root).inspect_err(|err| {
        // 14-CR-06: a legacy monitor's bare `advance` failing here would otherwise be
        // invisible (its output goes to /dev/null) and its phase silently stalls — record
        // the failure in events.jsonl. Phase 0 is the "could not attribute a phase"
        // sentinel; no real phase is 0.
        events::emit(
            project_root,
            PhaseId::new(0),
            "advance_failed",
            serde_json::json!({ "reason": err.to_string() }),
        );
    })
}

pub(crate) fn advance_for_stage(
    project_root: &Path,
    phase: Option<PhaseId>,
    stage: Option<Stage>,
) -> Result<(), CliError> {
    let phase = resolve_advance_phase(project_root, phase)?;
    advance_with(project_root, phase, stage)
}

pub(crate) fn advance_with(
    project_root: &Path,
    phase: PhaseId,
    stage: Option<Stage>,
) -> Result<(), CliError> {
    // No long lock (D-08): a fresh supervisor's advance is ordered behind its launcher's
    // publish by the promotion wait, and each write takes the state write lock for
    // itself. A duplicate advance of this phase is refused by stage binding below once
    // the first has moved the stage on.
    let mut state = workflow::load_state(project_root, phase)?;
    // Safe point 1 (50-13): a stop taken while this supervisor waited for
    // its promotion ends the advance before it evaluates anything.
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
        commit(&mut state, |s| s.session_id = Some(session_id))?;
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
                                    // CR-01: close the gate now. The approval
                                    // was bound to this gate's id and is spent.
                                    let _ = close_gate(project_root, phase, stage);
                                    let approval = recorded_approval(&current);
                                    commit(&mut state, |s| {
                                        s.gate_pending = false;
                                        s.checkpoint_approval = approval;
                                    })?;
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
                                    // unattended. D-07: this rejection answered
                                    // "approve the changed checkpoint set?". The
                                    // stage-failure gate that follows asks a
                                    // different question, where any non-abort
                                    // answer relaunches the stage, so it opens
                                    // under its own id and waits for its own
                                    // answer.
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
            // SUPV-02: the failover branch sits ahead of the cron arm's infra
            // bump (D-06), and only `RateLimited` reaches it; the wildcard below
            // never fails over.
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
        let control_reservation = monitor::Reservation::new(&subcommand, &control).unwrap();
        let control_result = monitor::spawn_monitor(
            &control,
            "claude",
            &[],
            &[],
            prompt(),
            &subcommand,
            &control_reservation,
        );
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
        let mut state = State::new(phase, AgentKind::Claude, mode, root.to_path_buf());
        state.stage = stage;
        state.gate_pending = true;
        state.stopped = true;
        state.stop_reason = Some("stage failed; gate open".to_string());
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let opened = Gates::open_gate(
            root,
            phase,
            stage,
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
            &gate_response(false, Some("abort: stale answer nobody polled")),
        )
    }

    /// A gate at `stage`, answered with an approval and claimed, as a consumer that then
    /// died leaves it. Returns the gate's id.
    fn plant_claimed_answer(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> devflow_core::gates::GateId {
        let opened = Gates::open_gate(root, phase, stage, "planted claimed answer").unwrap();
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
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(91);
        let (gate, answer_before) =
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
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(93);
        let (gate, answer) = seed_code_gate_with_pending_answer(root, phase, Mode::Supervise);

        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let quarantined = Gates::quarantined(root);
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(std::fs::read(&quarantined[0].path).unwrap(), answer);
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
        assert!(!workflow::load_state(root, phase).unwrap().gate_pending);
        let order = event_order(root);
        let set_aside = order.iter().position(|e| e == "gate_answer_quarantined");
        let launched = order.iter().position(|e| e == "stage_launched");
        assert!(
            set_aside.is_some() && launched.is_some() && set_aside < launched,
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
            resume_gate_lines(phase, Stage::Code, Some(gate.as_str()), &unused, true),
            vec![format!(
                "resume: set aside an unused answer for the code gate (gate {gate}): {} — the \
                 rerun asks again",
                path.display()
            )]
        );
        assert_eq!(
            resume_gate_lines(phase, Stage::Code, None, &unused, true),
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
            resume_gate_lines(phase, Stage::Code, Some("G1"), &approved, true),
            vec![
                "resume: an answer for the code gate (gate G1: approved) was claimed, with no \
                 record that it was applied; the rerun asks again"
                    .to_string()
            ]
        );
        assert_eq!(
            resume_gate_lines(phase, Stage::Ship, Some("G1"), &approved, true),
            vec![
                "resume: an answer for the ship gate (gate G1: approved) was claimed, with no \
                 record that it was applied; a merge, version bump or tag may already have run, \
                 so check them before answering again; the rerun asks again"
                    .to_string()
            ],
            "a kill during Ship finalization leaves a claim after its merge may have run"
        );
        assert_eq!(
            resume_gate_lines(phase, Stage::Code, Some("G1"), &approved, false),
            Vec::<String>::new(),
            "a claimed pair at a stage the run left is an applied answer's leftover"
        );
        let rejected = claimed(Some(GateAnswer {
            gate_id: "G1".to_string(),
            response: gate_response(false, Some("fix it")),
        }));
        assert_eq!(
            resume_gate_lines(phase, Stage::Code, None, &rejected, true),
            vec![
                "resume: an answer for the code gate (gate G1: rejected, note \"fix it\") was claimed, with no record that it was applied; the rerun asks again"
                    .to_string()
            ],
            "the claimed entry names itself, whatever the gate file said"
        );
        assert_eq!(
            resume_gate_lines(phase, Stage::Code, None, &claimed(None), true),
            vec![
                "resume: an answer of unknown content for the code gate (gate G1) was claimed, with no record that it was applied; the rerun asks again"
                    .to_string()
            ]
        );
        assert_eq!(
            resume_gate_lines(phase, Stage::Code, None, &CloseReport::default(), true),
            Vec::<String>::new(),
            "a stage with nothing to report prints nothing"
        );
    }

    /// D-06: once `resume` has set a Ship answer aside, `devflow ship` cannot apply it, so
    /// the report names what is still available. The Code lines above carry none of this.
    #[test]
    fn resume_names_only_the_actions_left_for_a_ship_answer_it_set_aside() {
        const NAME: &str = "pipeline_launch::tests::resume_names_only_the_actions_left_for_a_ship_answer_it_set_aside";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
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
        let phase = PhaseId::new(94);
        let (gate, answer) = seed_gate_with_pending_answer(
            root,
            phase,
            Stage::Ship,
            Mode::Supervise,
            &gate_response(true, None),
        );

        let result = resume(root, phase, None, false);
        {
            let reloaded = workflow::load_state(root, phase).ok();
            let _reap_guard = reloaded.as_ref().map(ReapMonitorOnDrop::after_launch);
        }
        drop(result);

        let quarantined = Gates::quarantined(root);
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(quarantined[0].label, gate.as_str());
        assert_eq!(std::fs::read(&quarantined[0].path).unwrap(), answer);
        let err = crate::pipeline_gate::ship_override(root, phase, false)
            .expect_err("the answer set aside can no longer be applied by `devflow ship`");
        assert!(err.to_string().contains("no Ship gate response"), "{err}");

        let report = CloseReport {
            quarantined: vec![quarantined[0].path.clone()],
            ..CloseReport::default()
        };
        let lines = resume_gate_lines(phase, Stage::Ship, Some(gate.as_str()), &report, true);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines[0].contains("the ship gate") && lines[0].contains("the rerun asks again"),
            "{lines:?}"
        );
        assert_eq!(
            lines[1],
            format!(
                "resume: the rerun's Ship gate asks again — answer it with `devflow gate approve \
                 {phase} ship`; next time, to apply a stored Ship approval without a rerun, run \
                 `devflow ship --phase {phase}` instead of `resume`"
            )
        );
        assert!(
            resume_gate_lines(phase, Stage::Code, Some(gate.as_str()), &report, true)
                .iter()
                .all(|line| !line.contains("devflow ship")),
            "control: only a Ship answer gets the Ship line"
        );
    }

    /// R-02's condition, end to end: a claimed pair at a stage the run left is an applied
    /// answer's leftover. `resume` closes it, and core's event is all that is said of it.
    #[test]
    fn resume_says_nothing_of_a_claimed_pair_at_another_stage() {
        const NAME: &str =
            "pipeline_launch::tests::resume_says_nothing_of_a_claimed_pair_at_another_stage";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(95);
        seed_code_gate_with_pending_answer(root, phase, Mode::Supervise);
        let left_behind = plant_claimed_answer(root, phase, Stage::Plan);

        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
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
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(96);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.stopped = true;
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();
        let gate = plant_claimed_answer(root, phase, Stage::Code);
        std::fs::remove_file(Gates::gate_path(root, phase, Stage::Code)).unwrap();

        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        assert!(!Gates::claim_path(root, phase, Stage::Code, &gate).exists());
        let found = events_of_kind(root, "resume_found_unapplied_answer");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0]["stage"], "code");
        assert_eq!(found[0]["gate_id"], gate.as_str());
        assert_eq!(found[0]["approved"], true);
    }

    /// D-06 (V-17): an answer whose waiter was killed before it consumed the answer
    /// never decides the gate a `resume` reaches next. Mode::Auto at Code with the legacy
    /// launch forced is refused by `preflight_unattended_launch_check`, so the
    /// rerun opens the same stage's gate in this process. It opens under a new id, the
    /// old answer is set aside, and the new gate waits: its short timeout is what ends
    /// this test.
    #[test]
    fn resume_quarantines_an_unused_gate_answer_and_opens_a_new_gate_id() {
        const NAME: &str = "pipeline_launch::tests::resume_quarantines_an_unused_gate_answer_and_opens_a_new_gate_id";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_dir_with_agent_stub("claude");
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

        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);

        assert!(
            events_of_kind(root, "gate_resolved").is_empty(),
            "the killed waiter's answer must not decide the rerun's gate: {:?}",
            events_of_kind(root, "gate_resolved")
        );
        let err = result.expect_err("the rerun's gate has no answer, so its wait times out");
        assert!(
            err.to_string().contains("timed out awaiting a response"),
            "{err}"
        );
        assert_eq!(events_of_kind(root, "gate_timeout").len(), 1);
        assert_eq!(
            stage_launched_count(root, phase),
            0,
            "a refused preflight with an unanswered gate launches nothing"
        );

        let new_gate = live_gate_id(root, phase, Stage::Code);
        assert_ne!(new_gate, old_gate, "the rerun's gate has its own id");
        assert_eq!(
            Gates::read_answer(root, phase, Stage::Code, &new_gate),
            devflow_core::gates::AnswerRead::None,
            "nothing answers the rerun's gate"
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
        state.verification_baseline_captured = true;
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
        state.verification_baseline_captured = true;
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
        const NAME: &str = "pipeline_launch::tests::\
             resume_with_agent_refuses_before_touching_state_when_target_cannot_run_the_stage";
        enter_agent_free_child!(NAME, "codex");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(76);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Define;
        state.verification_baseline_captured = true;
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
        const NAME: &str = "pipeline_launch::tests::\
             resume_with_agent_refuses_auto_mode_handoff_that_would_fail_the_later_unattended_launch_check";
        enter_agent_free_child!(NAME, "codex");
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

    /// A stopped Code-stage run of `phase` under Claude on the capture shape, in a fresh
    /// repo at `root`, with `fallback` saved as its fallback and `consumed` as its spent mark.
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
        state.stage = Stage::Code;
        state.stopped = true;
        state.stop_reason = Some("rate limited".to_string());
        state.fallback_agent = fallback;
        state.fallback_consumed = consumed;
        // The capture shape: a stub `claude` cannot answer the stream launch's canary.
        state.legacy_claude_launch = true;
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

        let result = resume_with_fallback(root, phase, None, false, Some(AgentKind::Codex));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(!reloaded.fallback_consumed, "the spent mark is cleared");
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert!(!reloaded.stopped);
        assert!(reloaded.supervisor.is_some(), "the run relaunched");
    }

    /// D-01 (re-arm, cron-paused): a run the cron arm paused is not stopped, and still
    /// names the supervisor that exited. `resume --fallback-agent` re-arms it all the same.
    #[test]
    fn resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_cron_paused_run() {
        const NAME: &str = "pipeline_launch::tests::\
                            resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_cron_paused_run";
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
        state.stage = Stage::Code;
        state.fallback_agent = Some(AgentKind::Codex);
        state.fallback_consumed = true;
        // The capture shape: a stub `claude` cannot answer the stream launch's canary.
        state.legacy_claude_launch = true;
        // The record of the supervisor that exited on the rate limit: the cron arm
        // leaves it behind, and its socket is gone.
        const EXITED_LAUNCH: &str = "exited-launch";
        state.supervisor = Some(SupervisorRecord::new(
            root.join("gone.sock"),
            EXITED_LAUNCH.to_string(),
            1,
            0,
            "boot".to_string(),
        ));
        workflow::save_state(&state).unwrap();
        crate::pipeline_outcomes::handle_rate_limited_outcome(
            root,
            &mut state,
            phase,
            Stage::Code,
            Some("rate limited until 2026-06-18T15:45:30Z".into()),
        )
        .unwrap();

        let paused = workflow::load_state(root, phase).unwrap();
        let cron = devflow_core::ship::load_cron_instructions(root, phase)
            .expect("the cron arm wrote cron-instructions-5148.json");
        assert!(
            !cron.hermes_cron.schedule.is_empty(),
            "the cron arm scheduled a resume"
        );
        assert!(!paused.stopped, "a cron-paused run is not stopped");
        assert_eq!(paused.infra_failures, 1);
        assert_eq!(
            paused.supervisor.as_ref().map(|r| r.launch_id.as_str()),
            Some(EXITED_LAUNCH),
            "the exited supervisor's record is left behind"
        );

        let result = resume_with_fallback(root, phase, None, false, Some(AgentKind::Codex));
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .filter(|state| {
                state.supervisor.as_ref().map(|r| r.launch_id.as_str()) != Some(EXITED_LAUNCH)
            })
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(!reloaded.fallback_consumed, "the spent mark is cleared");
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert_eq!(reloaded.agent, AgentKind::Claude);
        let relaunched = reloaded.supervisor.expect("the run relaunched");
        assert_ne!(
            relaunched.launch_id, EXITED_LAUNCH,
            "a new supervisor replaced the exited one's record"
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

        let result = resume(root, phase, None, false);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(reloaded.fallback_consumed, "a plain resume does not re-arm");
        assert_eq!(reloaded.fallback_agent, Some(AgentKind::Codex));
        assert!(reloaded.supervisor.is_some(), "the run relaunched");
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

        let result = resume_with_fallback(root, phase, Some(AgentKind::Codex), false, None);
        let reloaded = workflow::load_state(root, phase).unwrap();
        let _reap_guard = ReapMonitorOnDrop::after_launch(&reloaded);
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
        assert_eq!(reloaded.supervisor, None, "nothing launched");
        assert_eq!(
            reloaded.pending_supervisor, None,
            "the reservation is given back"
        );
    }

    /// D-02 through `resume --fallback-agent`: a fallback equal to the run's agent is
    /// refused before any write, the notify command included (review B-3). The
    /// reservation `reserve_launch` writes first is the one write, and it is given back.
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

        let result = resume_with_fallback(root, phase, None, false, Some(AgentKind::Claude));
        let reloaded = workflow::load_state(root, phase).unwrap();
        let _reap_guard = ReapMonitorOnDrop::after_launch(&reloaded);
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
        assert_eq!(reloaded.supervisor, None, "nothing launched");
        assert_eq!(
            reloaded.pending_supervisor, None,
            "the reservation is given back"
        );
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

            answer_open_gate(
                root,
                phase,
                Stage::Code,
                &serde_json::from_str(
                    r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
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

        preanswer_gate(
            root,
            phase,
            Stage::Code,
            &serde_json::from_str(
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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

        let err = advance_with(root, phase, Some(Stage::Define))
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

    /// A Validate launch's dispatch stamp reaches disk with its first commit, and only
    /// its fields do: a revoke another writer made since the caller loaded is kept.
    #[test]
    fn a_validate_launch_commits_its_dispatch_stamp() {
        const NAME: &str = "pipeline_launch::tests::a_validate_launch_commits_its_dispatch_stamp";
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(93);
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-stamp", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded())),
            "verdict: pass\n",
        )
        .unwrap();
        let observed = (
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase),
            devflow_core::agent_result::phase_verification_mtime_nanos(&worktree, phase),
        );
        assert!(
            observed.0.is_some(),
            "premise: the artifact has a fingerprint"
        );

        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Validate;
        state.worktree_path = Some(worktree);
        // The capture shape: no delivery canary.
        state.legacy_claude_launch = true;
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.yes_ship = false;
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        let result = launch_stage_inner(&mut state, None, None);
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);
        result.unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.verification_run_nonce, Some(1));
        assert_eq!(after.last_verification_fingerprint, observed.0);
        assert_eq!(after.last_verification_mtime_nanos, observed.1);
        assert!(after.verification_baseline_captured);
        assert!(!after.yes_ship, "the concurrent revoke is kept");
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
        preanswer_gate(
            root,
            phase,
            stage,
            &serde_json::from_str(
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );
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

    /// A response in one of the three shapes `write_gate_response` documents.
    fn gate_response(approved: bool, note: Option<&str>) -> devflow_core::gates::GateResponse {
        devflow_core::gates::GateResponse {
            approved,
            note: note.map(str::to_string),
            responded_by: Some("test".to_string()),
        }
    }

    /// Pre-answer the next gate at `stage` so its wait returns immediately.
    ///
    /// `GateAction::from_response` maps these three shapes: approved ->
    /// `Advance`; not approved with a note containing "abort" -> `Abort`;
    /// anything else not approved -> `LoopBack`.
    fn preanswer_gate_response(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        approved: bool,
        note: Option<&str>,
    ) {
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "preanswer_gate_response must run inside a child test with an agent-free PATH (999.80)"
        );
        preanswer_gate(root, phase, stage, &gate_response(approved, note));
    }

    /// Answer the gate that opens next at `stage`, as a human would once they see
    /// it. A gate still holding an earlier answer refuses this one, and no gate may
    /// be open yet, so this retries until a gate takes it.
    fn answer_the_next_open_gate(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        response: &devflow_core::gates::GateResponse,
    ) {
        const NEXT_GATE_OPENS_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);
        let started = std::time::Instant::now();
        while answer_open_gate(root, phase, stage, response).is_err() {
            assert!(
                started.elapsed() < NEXT_GATE_OPENS_WITHIN,
                "no {stage} gate took the answer within {NEXT_GATE_OPENS_WITHIN:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
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

        preanswer_gate_response(root, phase, Stage::Code, true, None);

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
        preanswer_gate_response(root, phase, Stage::Code, true, None);

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
            answer_the_next_open_gate(
                &writer_root,
                phase,
                Stage::Code,
                &gate_response(false, Some("abort")),
            );
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
        preanswer_gate_response(
            root,
            phase,
            Stage::Code,
            false,
            Some("the rewritten task needs supervised repair"),
        );

        let writer_root = root.to_path_buf();
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
                // The pre-answer is read before the answer files because the
                // gate publishes the answer before it removes the pre-answer.
                let response_gone = !Gates::preanswer_path(&writer_root, phase, Stage::Code)
                    .exists()
                    && gate_answer_files(&writer_root, phase, Stage::Code).is_empty();
                let state = workflow::load_state(&writer_root, phase);
                if state.as_ref().is_ok_and(|state| state.stopped) {
                    return;
                }
                if response_gone
                    && answer_open_gate(
                        &writer_root,
                        phase,
                        Stage::Code,
                        &gate_response(false, Some("abort")),
                    )
                    .is_ok()
                {
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
                && gate_answer_files(root, phase, Stage::Code).is_empty()
                && gate_claim_files(root, phase, Stage::Code).is_empty(),
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

    /// D-07 (V-22) and 48-15 Task 1 / T-48-15-03. A non-abort rejection must record
    /// NOTHING: widening the approved set on a rejection is the repudiation path. It
    /// also no longer answers the stage-failure gate that follows. That gate asks a
    /// different question ("stage failed: retry, loop to Code, or abort?") where any
    /// non-abort answer relaunches the stage, so it opens with its own id and waits;
    /// its short timeout is what ends this test. The plan file the operator objected
    /// to has to appear in what that gate shows them.
    #[test]
    fn rejecting_the_rescan_gate_leaves_the_follow_on_gate_waiting_for_its_own_answer() {
        const NAME: &str = "pipeline_launch::tests::rejecting_the_rescan_gate_leaves_the_follow_on_gate_waiting_for_its_own_answer";
        const ROOT_ENV: &str = "DEVFLOW_PIPELINE_LAUNCH_RESCAN_REJECT_ROOT";
        let phase = PhaseId::new(102);

        if !devflow_core::test_support::in_child_test(NAME) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            // Supervise: D-07's route. Auto parks for repair instead
            // (`park_auto_rescan_repair`), with no follow-on gate.
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
                &[
                    (ROOT_ENV, root.as_os_str()),
                    ("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("1")),
                ],
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

        // Not approved, and the note deliberately does NOT say "abort":
        // `GateAction::from_response` maps that to LoopBack, the "I will fix the plan"
        // answer.
        preanswer_gate_response(
            root,
            phase,
            Stage::Code,
            false,
            Some("the rewritten task needs review first"),
        );

        let result = advance_for_stage(root, Some(phase), Some(Stage::Code));
        let reloaded = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded.as_ref().map(ReapMonitorOnDrop::after_launch);

        // One answer decided one gate.
        let resolved = events_of_kind(root, "gate_resolved");
        assert_eq!(
            resolved.len(),
            1,
            "the rejection decides the re-scan gate and nothing else: {resolved:?}"
        );
        assert_eq!(resolved[0]["action"], "loop_back");
        let err = result.expect_err("the follow-on gate has no answer, so its wait times out");
        assert!(
            err.to_string().contains("timed out awaiting a response"),
            "{err}"
        );
        assert_eq!(events_of_kind(root, "gate_timeout").len(), 1);

        // The follow-on gate is its own gate, waiting.
        let follow_on = live_gate_id(root, phase, Stage::Code);
        assert_ne!(
            resolved[0]["gate_id"],
            follow_on.as_str(),
            "the follow-on gate opens with its own id"
        );
        assert_eq!(
            Gates::read_answer(root, phase, Stage::Code, &follow_on),
            devflow_core::gates::AnswerRead::None,
            "no answer is bound to the follow-on gate"
        );
        assert_eq!(
            stage_launched_count(root, phase),
            0,
            "nothing relaunches on the plan the operator just rejected"
        );

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

        // A rejection must REACH the existing non-auto-decide dispatch, not stop at the
        // re-scan gate. `handle_stage_failure` is the only thing that emits a
        // `[never-silent]` context, and the augmented reason it renders must name the
        // plan file the human objected to: that gate is where they will actually read
        // it.
        //
        // Asserting on the never-silent context specifically, NOT merely on "some gate
        // mentions the plan file": the re-scan gate's own context already names it, so
        // the weaker assertion would measure nothing.
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
            "the follow-on gate must name `{plan_file}` in an augmented \
             unresolved-checkpoint reason; contexts were {gate_contexts:?}"
        );
        assert_eq!(
            gate_contexts.len(),
            2,
            "the re-scan gate and the follow-on gate: {gate_contexts:?}"
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

        preanswer_gate_response(
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
    /// and the fixture's `stop_until` ends the chain after N+1. A `descendant` file
    /// there makes N+1's run first start a `sleep 300` in its own process group.
    fn handoff_claude_dir(first_marker: &str) -> tempfile::TempDir {
        let path_dir = agent_free_dir_with_agent_stub("claude");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
             if [ ! -e first-done ]; then\n  : > first-done\n  gate=go\n  marker='{first_marker}'\n\
             else\n  if [ -e descendant ]; then sleep 300 & fi\n  gate=go2\n  marker='{SUCCESS_MARKER}'\nfi\n\
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
        run_handoff_child_with_stubs(name, hold, first_marker, &[]);
    }

    /// [`run_handoff_child`], with each `(program, script)` in `stubs` written
    /// into the child's PATH dir after the default `claude` stub, replacing
    /// a stub of the same name. A second agent's stub lets a child hand a
    /// stage over to another driver (SUPV-02).
    fn run_handoff_child_with_stubs(
        name: &str,
        hold: Option<&str>,
        first_marker: &str,
        stubs: &[(&str, String)],
    ) {
        run_handoff_child_with_stubs_and_env(name, hold, first_marker, stubs, &[]);
    }

    /// [`run_handoff_child_with_stubs`], with each `(name, value)` in
    /// `extra_env` also passed to the child (51-10: a test seam variable).
    fn run_handoff_child_with_stubs_and_env(
        name: &str,
        hold: Option<&str>,
        first_marker: &str,
        stubs: &[(&str, String)],
        extra_env: &[(&str, &std::ffi::OsStr)],
    ) {
        use std::os::unix::fs::PermissionsExt;
        let path_dir = handoff_claude_dir(first_marker);
        for (program, script) in stubs {
            let path = path_dir.path().join(program);
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
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
        env.extend_from_slice(extra_env);
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
            state.verification_baseline_captured = true;
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

    /// Names a holder-test child's project root and which arm it runs. Not DevFlow
    /// settings, so they carry no `DEVFLOW_` prefix the doc check would require documented.
    const HOLDER_ROOT_ENV: &str = "P50_1_29_ROOT";
    const HOLDER_CASE_ENV: &str = "P50_1_29_CASE";
    const FOREGROUND_GATE_PHASE: PhaseId = PhaseId::new(2901);

    /// One foreground gate wait in a child process, signalled once its gate file exists:
    /// how it ended, reaped, with its output.
    fn signalled_foreground_gate_wait(name: &str, root: &Path, case: &str) -> std::process::Output {
        let phase = FOREGROUND_GATE_PHASE;
        workflow::save_state(&State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        ))
        .unwrap();
        let path_dir = tempfile::tempdir().unwrap();
        let mut child = devflow_core::test_support::spawn_test_in_child(
            name,
            path_dir.path(),
            &[
                (HOLDER_ROOT_ENV, root.as_os_str()),
                (HOLDER_CASE_ENV, std::ffi::OsStr::new(case)),
            ],
        );
        let gate = Gates::gate_path(root, phase, Stage::Code);
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !gate.exists() {
            if std::time::Instant::now() > deadline || child.try_wait().unwrap().is_some() {
                let _ = child.kill();
                panic!(
                    "{case}: the child never opened its gate: {:?}",
                    child.wait_with_output()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(devflow_core::agent::terminate(child.id()), "{case}: signal");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                panic!(
                    "{case}: the child did not end within 10 s of the signal: {:?}",
                    child.wait_with_output()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    }

    /// D-18: a foreground gate wait ends at safe point 4 when its holder is signalled: the
    /// gate closes and the stop is recorded. The control has no handler: it dies and
    /// leaves its gate open, the outcome D-18 removes.
    #[test]
    fn a_holder_stop_ends_a_foreground_gate_wait_at_safe_point_4() {
        use std::os::unix::process::ExitStatusExt;
        const NAME: &str =
            "pipeline_launch::tests::a_holder_stop_ends_a_foreground_gate_wait_at_safe_point_4";
        let phase = FOREGROUND_GATE_PHASE;
        if devflow_core::test_support::in_child_test(NAME) {
            let root = PathBuf::from(std::env::var_os(HOLDER_ROOT_ENV).unwrap());
            let _hold = if std::env::var(HOLDER_CASE_ENV).unwrap() == "handler" {
                crate::commands::claim_foreground_holder(&root, phase).unwrap()
            } else {
                devflow_core::liveness::claim_phase_holder(&root, phase).unwrap()
            };
            let mut state = workflow::load_state(&root, phase).unwrap();
            let end = crate::pipeline_gate::run_gate_with_timeout(
                &root,
                &mut state,
                Stage::Code,
                "a foreground gate",
                300,
                None,
            );
            let error = end.expect_err("a stopped wait is an error").to_string();
            assert!(error.contains(HOLDER_STOP_REASON), "{error}");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let output = signalled_foreground_gate_wait(NAME, root, "handler");
        eprintln!("handler: {:?}", output.status);
        assert_eq!(output.status.signal(), None, "{output:?}");
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "the gate closes with the wait"
        );
        let state = workflow::load_state(root, phase).unwrap();
        assert!(state.stopped, "the stop is recorded");
        assert!(!state.gate_pending, "and the gate no longer pending");
        assert_eq!(state.stop_reason.as_deref(), Some(HOLDER_STOP_REASON));
        let stopped = events_named(root, "advance_stopped");
        assert_eq!(stopped.len(), 1, "{stopped:?}");
        assert_eq!(stopped[0]["safe_point"], 4, "{}", stopped[0]);
        assert_eq!(stopped[0]["source"], "foreground_holder", "{}", stopped[0]);

        let control = tempfile::tempdir().unwrap();
        let root = control.path();
        let output = signalled_foreground_gate_wait(NAME, root, "no-handler");
        eprintln!("no handler: {:?}", output.status);
        assert_eq!(output.status.signal(), Some(libc::SIGTERM), "{output:?}");
        assert!(
            Gates::gate_path(root, phase, Stage::Code).exists(),
            "control: a holder the signal ends leaves its gate open"
        );
        assert!(
            workflow::load_state(root, phase).unwrap().gate_pending,
            "control: and still pending"
        );
    }

    /// The refusal a process meets when another foreground process holds the phase: it
    /// names the holder's pid and `stop`, never `resume`.
    fn assert_refused_by_the_holder(error: &str, phase: PhaseId) {
        assert!(
            error.contains(&format!("(pid {})", std::process::id())),
            "the refusal names the holder's pid: {error}"
        );
        assert!(
            error.contains(&format!("devflow stop --phase {phase}")),
            "{error}"
        );
        assert!(!error.contains("devflow resume"), "{error}");
    }

    /// D-17 (V-31): a hand-run `advance` refuses while another process holds the phase,
    /// and writes nothing.
    #[test]
    fn a_hand_run_advance_refuses_while_another_process_holds_the_phase() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2904);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
        state.stage = Stage::Plan;
        workflow::save_state(&state).unwrap();
        let _hold = devflow_core::liveness::claim_phase_holder(root, phase).unwrap();
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();

        let error = advance_command(root, Some(phase), Some(Stage::Plan))
            .expect_err("a held phase refuses")
            .to_string();

        assert_refused_by_the_holder(&error, phase);
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "nothing is written"
        );
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

        let error = resume(root, phase, None, true)
            .expect_err("a held phase refuses")
            .to_string();

        assert_refused_by_the_holder(&error, phase);
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "nothing is reserved"
        );
    }

    /// Runs `body` in a child test process of its own, with a hermetic cache: a released
    /// flock must read released at once, which another test's fork would falsify.
    fn in_own_hermetic_process(name: &str, body: impl FnOnce()) {
        if devflow_core::test_support::in_child_test(name) {
            body();
            return;
        }
        let path_dir = agent_free_git_only_path_dir();
        let output = crate::test_support::run_hermetic_test_in_child(name, path_dir.path(), &[]);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
    }

    /// A short-pathed socket directory and the fixture the two live-supervisor refusal
    /// tests share: a recorded supervisor that is really serving, and a reservation whose
    /// liveness lock this test holds, with no socket.
    struct LiveSupervisorFixture {
        _sockets: tempfile::TempDir,
        supervisor: devflow_core::supervisor::runtime::Supervisor,
        record: SupervisorRecord,
        pending: devflow_core::state::PendingSupervisor,
        pending_hold: devflow_core::liveness::Hold,
    }

    const LIVE_RECORDED_LAUNCH: &str = "0000000000002908";
    const LIVE_PENDING_LAUNCH: &str = "0000000000002909";

    fn live_supervisor_fixture(root: &Path, phase: PhaseId) -> LiveSupervisorFixture {
        let sockets = tempfile::Builder::new()
            .prefix("dfs")
            .tempdir_in("/tmp")
            .unwrap();
        let (supervisor, record) =
            fake_supervisor(sockets.path(), root, phase, LIVE_RECORDED_LAUNCH);
        let liveness_path =
            devflow_core::liveness::launch_liveness_path(root, LIVE_PENDING_LAUNCH).unwrap();
        std::fs::create_dir_all(liveness_path.parent().unwrap()).unwrap();
        let pending_hold = devflow_core::liveness::Hold::take(&liveness_path).unwrap();
        let pending = devflow_core::state::PendingSupervisor::new(
            LIVE_PENDING_LAUNCH.to_string(),
            sockets.path().join("never-published.sock"),
            liveness_path,
        );
        LiveSupervisorFixture {
            _sockets: sockets,
            supervisor,
            record,
            pending,
            pending_hold,
        }
    }

    /// `error` is the live-supervisor refusal for `launch_id`.
    fn assert_refused_over_a_live_supervisor(error: &str, phase: PhaseId, launch_id: &str) {
        assert!(error.contains("live supervisor"), "{error}");
        assert!(error.contains(launch_id), "names the launch: {error}");
        assert!(
            error.contains(&format!("devflow stop --phase {phase}")),
            "{error}"
        );
        assert!(!error.contains("devflow resume"), "{error}");
    }

    /// D-08, D-17 (V-32): a hand-run `advance` refuses beside a live supervisor of its
    /// phase, recorded or pending. The state is at Plan and the advance binds to Define,
    /// so the control proves that with the supervisor gone the same call reaches the
    /// advance's own stage check, which writes nothing either.
    #[test]
    fn a_hand_run_advance_refuses_while_a_supervisor_of_the_phase_is_live() {
        const NAME: &str = "pipeline_launch::tests::a_hand_run_advance_refuses_while_a_supervisor_of_the_phase_is_live";
        in_own_hermetic_process(NAME, || {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(2908);
            let fixture = live_supervisor_fixture(root, phase);
            let state_file = workflow::state_path(root, phase);
            let advance = || {
                advance_command(root, Some(phase), Some(Stage::Define))
                    .expect_err("this advance never succeeds")
                    .to_string()
            };
            let mut state = State::new(phase, AgentKind::Claude, Mode::Supervise, root.into());
            state.stage = Stage::Plan;

            state.supervisor = Some(fixture.record.clone());
            workflow::save_state(&state).unwrap();
            let before = std::fs::read(&state_file).unwrap();
            let error = advance();
            eprintln!("recorded: {error}");
            assert_refused_over_a_live_supervisor(&error, phase, LIVE_RECORDED_LAUNCH);
            assert_eq!(std::fs::read(&state_file).unwrap(), before, "recorded");
            assert!(Gates::list_open(root).is_empty(), "no gate opens");
            let failed = events_named(root, "advance_failed");
            assert_eq!(failed.len(), 1, "{failed:?}");
            assert_eq!(failed[0]["reason"], "live supervisor", "{}", failed[0]);

            state.supervisor = None;
            state.pending_supervisor = Some(fixture.pending.clone());
            workflow::save_state(&state).unwrap();
            let before = std::fs::read(&state_file).unwrap();
            let error = advance();
            eprintln!("pending: {error}");
            assert_refused_over_a_live_supervisor(&error, phase, LIVE_PENDING_LAUNCH);
            assert_eq!(std::fs::read(&state_file).unwrap(), before, "pending");
            assert!(Gates::list_open(root).is_empty(), "no gate opens");

            let LiveSupervisorFixture {
                _sockets,
                supervisor,
                record,
                pending,
                pending_hold,
            } = fixture;
            supervisor.finish();
            drop(pending_hold);
            assert_eq!(
                devflow_core::supervisor::probe(&record.socket_path).unwrap(),
                devflow_core::supervisor::Liveness::Gone
            );
            state.supervisor = Some(record);
            state.pending_supervisor = Some(pending);
            workflow::save_state(&state).unwrap();
            let before = std::fs::read(&state_file).unwrap();
            let error = advance();
            eprintln!("gone: {error}");
            assert!(error.contains("advance stage mismatch"), "{error}");
            assert_eq!(
                events_named(root, "advance_failed").last().unwrap()["reason"],
                "stage mismatch",
                "the call reached the advance"
            );
            assert_eq!(std::fs::read(&state_file).unwrap(), before, "gone");
        });
    }

    /// R-17: `devflow ship` refuses beside a live supervisor of its phase, before it reads
    /// or claims the answer that supervisor can still consume, and before a loop-back
    /// could launch beside it. The control answers with an abort, because a loop-back that
    /// passes the guard launches a real agent; the guard runs before any answer is read,
    /// so the answer's kind cannot affect it.
    #[test]
    fn devflow_ship_refuses_while_a_supervisor_of_the_phase_is_live() {
        const NAME: &str =
            "pipeline_launch::tests::devflow_ship_refuses_while_a_supervisor_of_the_phase_is_live";
        in_own_hermetic_process(NAME, || {
            use devflow_core::gates::GateResponse;
            use devflow_core::test_support::gate_answer_files;
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(2910);
            let fixture = live_supervisor_fixture(root, phase);
            let state_file = workflow::state_path(root, phase);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.into());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();
            let gate = Gates::open_gate(root, phase, Stage::Ship, "ctx")
                .unwrap()
                .gate_id;
            let answer = |note: &str| GateResponse {
                approved: false,
                note: Some(note.to_string()),
                responded_by: Some("test".to_string()),
            };
            answer_open_gate(root, phase, Stage::Ship, &answer("review: fix it")).unwrap();
            let claim = Gates::claim_path(root, phase, Stage::Ship, &gate);
            let answer_file = gate_answer_files(root, phase, Stage::Ship)
                .pop()
                .expect("the answer is on disk");
            let answer_bytes = std::fs::read(&answer_file).unwrap();
            let live_files = || {
                std::fs::read_dir(root.join(".devflow"))
                    .unwrap()
                    .filter(|entry| {
                        let name = entry.as_ref().unwrap().file_name();
                        name.to_string_lossy().ends_with(".live")
                    })
                    .count()
            };

            for (supervisor, pending, launch) in [
                (Some(fixture.record.clone()), None, LIVE_RECORDED_LAUNCH),
                (None, Some(fixture.pending.clone()), LIVE_PENDING_LAUNCH),
            ] {
                state.supervisor = supervisor;
                state.pending_supervisor = pending;
                workflow::save_state(&state).unwrap();
                let before = std::fs::read(&state_file).unwrap();
                let files = live_files();
                let error = crate::pipeline_gate::ship_override(root, phase, false)
                    .expect_err("a live supervisor refuses `devflow ship`")
                    .to_string();
                eprintln!("{launch}: {error}");
                assert_refused_over_a_live_supervisor(&error, phase, launch);
                assert_eq!(std::fs::read(&state_file).unwrap(), before, "{launch}");
                assert_eq!(
                    std::fs::read(&answer_file).unwrap(),
                    answer_bytes,
                    "{launch}: the live supervisor can still consume its answer"
                );
                assert!(!claim.exists(), "{launch}: the answer is not claimed");
                assert_eq!(live_files(), files, "{launch}: no launch is reserved");
            }
            assert_eq!(
                events_named(root, "ship_override_refused").len(),
                2,
                "each refusal is recorded"
            );

            let LiveSupervisorFixture {
                _sockets,
                supervisor,
                record,
                pending,
                pending_hold,
            } = fixture;
            supervisor.finish();
            drop(pending_hold);
            state.supervisor = Some(record);
            state.pending_supervisor = Some(pending);
            workflow::save_state(&state).unwrap();
            std::fs::remove_file(&answer_file).unwrap();
            answer_open_gate(root, phase, Stage::Ship, &answer("abort this run")).unwrap();

            // `note_claim` parks the consumer until a second claim outcome is counted, so
            // the claim file can be read before the abort closes the gate and removes it.
            let probe = std::sync::Arc::new(crate::pipeline_gate::ClaimProbe::default());
            let shipped = std::thread::scope(|scope| {
                let shipper = scope.spawn(|| {
                    crate::pipeline_gate::CLAIM_PROBE.set(Some(probe.clone()));
                    crate::pipeline_gate::ship_override(root, phase, false)
                });
                assert!(
                    poll_until(Duration::from_secs(10), || claim.exists()),
                    "with the supervisor gone the guard passes and the answer is claimed"
                );
                eprintln!("gone: the claim file appeared");
                probe
                    .already_consumed
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                shipper.join().unwrap()
            });
            shipped.expect("the abort answer is driven through");
        });
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
        // Launch ids in `new_launch_id`'s format, the only one a sidecar takes.
        let (obs_n, obs_next) = ("0000000000000b5a", "0000000000000b5b");
        let (sup_n, rec_n) = fake_supervisor(sock_dir.path(), root, phase, obs_n);
        let (sup_next, rec_next) = fake_supervisor(sock_dir.path(), root, phase, obs_next);
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
            let gaps = handoff_gaps(&seen, obs_n, obs_next)
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

    // ---- 51-01: rate-limit failover at Code (SUPV-02) ----

    /// The capture-shape rate-limit envelope a rate-limited stub prints. Its
    /// retry time is the one
    /// `pipeline_outcomes::tests::primary_loop_rate_limited_writes_single_agent_cron_instructions`
    /// shows yields a scheduled (non-empty) cron resume rather than a gate.
    const RATE_LIMIT_ENVELOPE: &str =
        r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#;

    /// A `claude` stub that waits for `go` in its working directory, commits
    /// once, records that commit's id in `primary-commit` and prints the
    /// rate-limit envelope.
    fn rate_limited_claude_script() -> String {
        format!(
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
             i=0\nwhile [ ! -e go ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
             git commit -q --allow-empty -m 'primary work' || exit 1\n\
             git rev-parse HEAD > primary-commit\n\
             printf '%s\\n' '{RATE_LIMIT_ENVELOPE}'\nexit 0\n"
        )
    }

    /// A stub for a second agent that commits once and prints the success
    /// marker, which Layer 1 reads whichever driver launched it.
    fn committing_success_script() -> String {
        format!(
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
             git commit -q --allow-empty -m 'fallback work' || exit 1\n\
             printf '%s\\n' '{SUCCESS_MARKER}'\nexit 0\n"
        )
    }

    /// `git <args>` in `root`, trimmed stdout; panics on a failed git.
    fn git_out(root: &Path, args: &[&str]) -> String {
        let output = devflow_core::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed: {output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// Every event in the run's log, in order.
    fn all_events(root: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(devflow_core::events::events_path(root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .collect()
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
        for stage in [
            Stage::Define,
            Stage::Plan,
            Stage::Code,
            Stage::Validate,
            Stage::Ship,
        ] {
            let expected = match stage {
                Stage::Code => Some(AgentKind::Codex),
                Stage::Define | Stage::Plan | Stage::Validate | Stage::Ship => None,
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

    /// D-11 criterion 1 (SUPV-02), with D-04, D-05, D-06 and D-10: a Code
    /// stage whose `claude` commits once and is then rate-limited is handed to
    /// the unused Codex fallback by the stage's own supervisor, relaunched in
    /// the same root on the same branch, and completes there. The fallback's
    /// commit sits on the primary's, so nothing was reset or rebased, and no
    /// branch or worktree was added. Negative control in the same run: no
    /// cron instruction and no `rate_limit_resume_scheduled` event.
    ///
    /// Supervise mode, as `HandoffRun::launch_with` builds it: under Auto the
    /// Code preflight refuses a Codex fallback on the unattended launch shape.
    #[test]
    fn a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage() {
        const NAME: &str = "pipeline_launch::tests::a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child_with_stubs(
                NAME,
                None,
                SUCCESS_MARKER,
                &[
                    ("claude", rate_limited_claude_script()),
                    ("codex", committing_success_script()),
                ],
            );
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(5101), |_, state| {
            state.stage = Stage::Code;
            state.stop_until = Some(Stage::Code);
            state.fallback_agent = Some(AgentKind::Codex);
        });
        let branch_before = git_out(&run.root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let branches_before = git_out(&run.root, &["branch", "--list", "--format=%(refname)"]);
        let worktrees = |root: &Path| -> Vec<String> {
            git_out(root, &["worktree", "list", "--porcelain"])
                .lines()
                .filter(|line| line.starts_with("worktree "))
                .map(str::to_string)
                .collect()
        };
        let worktrees_before = worktrees(&run.root);
        run.go();

        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                !events_named(&run.root, "workflow_finished").is_empty()
                    || !events_named(&run.root, "rate_limit_resume_scheduled").is_empty()
            }),
            "the run neither finished nor scheduled a resume; events:\n{}",
            run.events()
        );

        let handoffs = events_named(&run.root, "agent_handoff");
        assert_eq!(handoffs.len(), 1, "one handoff; events:\n{}", run.events());
        let handoff = &handoffs[0];
        assert_eq!(handoff["stage"], "code", "{handoff}");
        assert_eq!(handoff["from_agent"], "claude", "{handoff}");
        assert_eq!(handoff["to_agent"], "codex", "{handoff}");
        assert_eq!(handoff["reason"], "rate_limited", "{handoff}");

        let events = all_events(&run.root);
        let position = |name: &str| events.iter().position(|e| e["event"] == name);
        let finished = position("workflow_finished").expect("the fallback's Code completes");
        assert_eq!(
            events[finished]["reason"], "stopped_at",
            "{}",
            events[finished]
        );
        assert!(
            position("agent_handoff").is_some_and(|handed| handed < finished),
            "the completion follows the handoff; events:\n{}",
            run.events()
        );

        let state = run.state();
        assert_eq!(
            state.agent,
            AgentKind::Codex,
            "the fallback now drives the run"
        );
        assert!(state.fallback_consumed, "the hop is consumed (D-05)");
        assert_eq!(state.stage, Stage::Code);
        assert!(state.stopped, "the --until code stop fired");
        assert!(
            state
                .stop_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("code completed")),
            "{:?}",
            state.stop_reason
        );
        assert_eq!(
            state.infra_failures, 0,
            "a handoff is not an infra fault (D-06)"
        );

        let primary = std::fs::read_to_string(run.root.join("primary-commit"))
            .expect("the primary committed before it was rate-limited");
        assert_eq!(git_out(&run.root, &["rev-parse", "HEAD^"]), primary.trim());
        assert_ne!(
            git_out(&run.root, &["rev-parse", "HEAD"]),
            primary.trim(),
            "the fallback committed on top of the primary's commit"
        );
        assert_eq!(
            git_out(&run.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
            branch_before
        );
        assert_eq!(
            git_out(&run.root, &["branch", "--list", "--format=%(refname)"]),
            branches_before,
            "no branch was added"
        );
        assert_eq!(
            worktrees(&run.root),
            worktrees_before,
            "no worktree was added"
        );

        assert!(
            !devflow_core::ship::cron_instructions_path(&run.root, run.phase).exists(),
            "a handed-off rate limit writes no cron instruction"
        );
        assert!(
            events_named(&run.root, "rate_limit_resume_scheduled").is_empty(),
            "a handed-off rate limit schedules no resume; events:\n{}",
            run.events()
        );
    }

    /// Write the rate-limit envelope as the phase's capture, for an advance
    /// run with no agent.
    fn write_rate_limit_capture(root: &Path, phase: PhaseId) {
        let stdout = devflow_core::agent_result::stdout_path(root, phase);
        std::fs::create_dir_all(stdout.parent().unwrap()).unwrap();
        std::fs::write(&stdout, format!("{RATE_LIMIT_ENVELOPE}\n")).unwrap();
    }

    /// Assert `phase`'s cron instruction resumes it with `devflow resume
    /// --phase N` on a real schedule.
    fn assert_cron_resume_written(root: &Path, phase: PhaseId) {
        let instructions = devflow_core::ship::load_cron_instructions(root, phase)
            .expect("the cron arm writes cron-instructions-NN.json");
        assert_eq!(instructions.resume.command, "devflow");
        assert_eq!(
            instructions.resume.args,
            ["resume", "--phase", &phase.to_string()]
        );
        assert!(
            !instructions.hermes_cron.schedule.is_empty(),
            "the retry time yields a schedule"
        );
    }

    /// D-11 criterion 2 (SUPV-02): with no fallback, a rate-limited Code stage
    /// takes today's cron arm through the real supervisor: the cron
    /// instruction resumes the phase with `devflow resume --phase N`, and
    /// nothing hands off. Negative control: zero `agent_handoff` events, the
    /// agent unchanged, and no stage launched after N's. Every successor is
    /// launched from inside N's advance, so once N's socket is gone no later
    /// one can appear; the launch count rules out one that came and went.
    #[test]
    fn a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off() {
        const NAME: &str = "pipeline_launch::tests::a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child_with_stubs(
                NAME,
                None,
                SUCCESS_MARKER,
                &[("claude", rate_limited_claude_script())],
            );
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(5102), |_, state| {
            state.stage = Stage::Code;
            state.stop_until = Some(Stage::Code);
        });
        run.go();

        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                !events_named(&run.root, "rate_limit_resume_scheduled").is_empty()
            }),
            "the cron arm never ran; events:\n{}",
            run.events()
        );
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                devflow_core::supervisor::probe(&run.n.socket_path)
                    .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
            }),
            "N's supervisor ends once its advance has returned"
        );

        assert_cron_resume_written(&run.root, run.phase);
        assert_eq!(
            events_named(&run.root, "rate_limit_resume_scheduled").len(),
            1,
            "{}",
            run.events()
        );
        assert!(
            events_named(&run.root, "agent_handoff").is_empty(),
            "no fallback, no handoff; events:\n{}",
            run.events()
        );
        let state = run.state();
        assert_eq!(state.agent, AgentKind::Claude);
        assert_eq!(state.fallback_agent, None);
        assert!(
            run.other_sockets().is_empty(),
            "no supervisor besides N's was published: {:?}",
            run.other_sockets()
        );
        assert_eq!(
            stage_launched_count(&run.root, run.phase),
            1,
            "only stage N launched; events:\n{}",
            run.events()
        );
    }

    /// D-07: a handoff refused at runtime — here the fallback's binary is
    /// missing — says so in `agent_handoff_refused`, naming the fallback and
    /// the refusal, then takes the cron arm unchanged with the original
    /// retry time. The fallback stays unconsumed and the agent unchanged; the
    /// cron arm's own bump is the only `infra_failures` change.
    ///
    /// Supervise mode: under Auto the candidate preflight refuses Codex at
    /// Code on the unattended launch shape before the binary check runs, and
    /// the reason would not name the missing binary.
    #[test]
    fn a_refused_handoff_takes_the_cron_arm_and_names_the_refusal() {
        const NAME: &str =
            "pipeline_launch::tests::a_refused_handoff_takes_the_cron_arm_and_names_the_refusal";
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

        advance(root, Some(phase)).unwrap();

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
        assert_cron_resume_written(root, phase);
        assert_eq!(events_named(root, "rate_limit_resume_scheduled").len(), 1);
        assert!(
            events_named(root, "agent_handoff").is_empty(),
            "a refused handoff is not a handoff"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Claude);
        assert!(
            !reloaded.fallback_consumed,
            "only a committed hop consumes it"
        );
        assert_eq!(reloaded.infra_failures, 1, "the cron arm's own bump");
    }

    /// R-2: a rate limit at Plan with an unused fallback configured takes the
    /// cron arm and leaves the fallback alone. A `codex` stub is on PATH, so
    /// the absence of a hop is not explained by a missing binary.
    ///
    /// Supervise mode, so that with the stage term removed from
    /// [`failover_target`] this run reaches a committed hop rather than a
    /// preflight refusal (the RED-equivalent this control was checked with).
    #[test]
    fn a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm() {
        const NAME: &str =
            "pipeline_launch::tests::a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm";
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

        advance(root, Some(phase)).unwrap();

        // The hop checks come first so that a hop at Plan is the failure
        // reported, not its knock-on missing cron file.
        assert!(
            events_named(root, "agent_handoff").is_empty(),
            "no handoff at Plan: {:?}",
            events_named(root, "agent_handoff")
        );
        assert!(
            events_named(root, "agent_handoff_refused").is_empty(),
            "Plan never attempts a handoff"
        );
        assert_cron_resume_written(root, phase);
        assert_eq!(events_named(root, "rate_limit_resume_scheduled").len(), 1);
        let reloaded = workflow::load_state(root, phase).unwrap();
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
        use std::os::unix::fs::PermissionsExt;
        let path_dir = agent_free_dir_with_agent_stub("claude");
        for program in ["claude", "codex"] {
            let path = path_dir.path().join(program);
            std::fs::write(
                &path,
                "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
                 i=0\nwhile [ ! -e never-released ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
                 exit 0\n",
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
    /// recorded canary and Claude session id, so an Antigravity fallback whose own
    /// canary reads `Absent` is refused, after exactly one canary run, instead of
    /// launching on Claude's `Confirmed`. Control first: with no handoff, the same
    /// recorded `Confirmed` admits an Antigravity launch without running a canary,
    /// so the refusal below is the handoff's doing.
    #[test]
    fn a_handoff_reruns_the_delivery_canary_for_the_new_agent() {
        const NAME: &str =
            "pipeline_launch::tests::a_handoff_reruns_the_delivery_canary_for_the_new_agent";
        enter_agent_free_child!(NAME, "agy");
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5161);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.canary = Some(canary::CanaryOutcome::Confirmed);
        state.session_id = Some("s1".to_string());
        workflow::save_state(&state).unwrap();

        let calls = std::cell::Cell::new(0usize);
        let mut without_handoff = state.clone();
        without_handoff.agent = AgentKind::Antigravity;
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
            AgentKind::Antigravity,
            HandoffReason::RateLimited,
        )
        .unwrap();
        assert_eq!(
            outcome,
            HandoffOutcome::HandedOff {
                from: AgentKind::Claude
            }
        );
        let mut reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.agent, AgentKind::Antigravity);
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

        let result = advance(root, Some(phase));
        let reloaded = workflow::load_state(root, phase).unwrap();
        let _reap_guard = ReapMonitorOnDrop::after_launch(&reloaded);
        result.unwrap();

        let handoffs = events_named(root, "agent_handoff");
        assert_eq!(handoffs.len(), 1, "one handoff: {handoffs:?}");
        let handoff = &handoffs[0];
        assert_eq!(handoff["stage"], "code", "{handoff}");
        assert_eq!(handoff["to_agent"], "codex", "{handoff}");
        assert_eq!(handoff["commit_count"], 0, "{handoff}");
        assert_eq!(handoff["tree_dirty"], false, "{handoff}");
        assert_eq!(reloaded.agent, AgentKind::Codex);
        assert!(reloaded.fallback_consumed);
        assert!(
            reloaded.supervisor.is_some(),
            "the fallback's relaunch is recorded"
        );
        assert!(
            events_named(root, "rate_limit_resume_scheduled").is_empty(),
            "a handed-off rate limit schedules no resume"
        );
    }

    /// A Supervise-mode Code run of `phase` under Claude in a fresh repo at `root`,
    /// with `fallback` and `consumed` saved, its `infra_failures` at `infra`, and a
    /// rate-limit capture for `advance` to read. The capture shape, so a Claude
    /// relaunch needs no delivery canary.
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
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();
        write_rate_limit_capture(root, phase);
    }

    /// D-06 at its boundary, at Code (R-2): a hop is not an infra fault. With
    /// `infra_failures` one below the ceiling, a rate limit with an unused
    /// fallback hands off and leaves the counter where it was. Control in the same
    /// test: the same state with no fallback takes the cron arm's bump, reaches the
    /// ceiling and aborts, which removes the run's state.
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
        let _reap_guard = reloaded.as_ref().ok().map(ReapMonitorOnDrop::after_launch);
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
        let outcome = advance(control_root, Some(control_phase));
        assert!(
            events_named(control_root, "agent_handoff").is_empty(),
            "control: no fallback, no handoff"
        );
        assert!(
            matches!(
                workflow::load_state(control_root, control_phase),
                Err(workflow::WorkflowError::MissingState(_))
            ),
            "control: the cron arm's bump reaches the ceiling and aborts (advance returned \
             {outcome:?})"
        );
        outcome.unwrap();
    }

    /// D-05: one hop per run, no automatic switch back. Once the fallback is
    /// consumed, a later Code rate limit takes the cron arm on the current agent.
    /// Second fixture, the adjacency edge at runtime: an unconsumed fallback equal
    /// to the agent is no handoff either. A `codex` stub is on PATH, so a missing
    /// binary does not explain the absent hop.
    #[test]
    fn a_rate_limit_after_the_one_hop_takes_the_cron_arm() {
        const NAME: &str =
            "pipeline_launch::tests::a_rate_limit_after_the_one_hop_takes_the_cron_arm";
        enter_agent_free_child!(NAME, "codex");
        let _guard = env_lock();
        for (phase, fallback, consumed) in [
            (PhaseId::new(5166), AgentKind::Codex, true),
            (PhaseId::new(5167), AgentKind::Claude, false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            save_rate_limited_code_run(root, phase, Some(fallback), consumed, 0);

            advance(root, Some(phase)).unwrap();

            assert!(
                events_named(root, "agent_handoff").is_empty(),
                "phase {phase}: no hop; events: {:?}",
                events_named(root, "agent_handoff")
            );
            assert_cron_resume_written(root, phase);
            assert_eq!(
                events_named(root, "rate_limit_resume_scheduled").len(),
                1,
                "phase {phase}"
            );
            let reloaded = workflow::load_state(root, phase).unwrap();
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

        let result = advance(root, Some(phase));
        let reloaded = workflow::load_state(root, phase).unwrap();
        let _reap_guard = ReapMonitorOnDrop::after_launch(&reloaded);
        result.unwrap();

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
        assert!(
            reloaded.supervisor.is_some(),
            "the same agent was re-driven"
        );
    }

    /// Run `name` in a 50-13 handoff child whose `claude` commits once and is
    /// rate-limited, beside an `agy` that exits 0 at once (its delivery canary
    /// reads an empty capture, so `Absent`), held at `after_failover_commit`.
    fn run_failover_relaunch_child(name: &str) {
        run_handoff_child_with_stubs(
            name,
            Some("after_failover_commit"),
            SUCCESS_MARKER,
            &[
                ("claude", rate_limited_claude_script()),
                ("agy", "#!/bin/sh\nexit 0\n".to_string()),
            ],
        );
    }

    /// A stage-N Code run under Claude with an unused Antigravity fallback, launched
    /// by the test and stopping after Code.
    fn launch_code_run_with_an_antigravity_fallback(phase: PhaseId) -> HandoffRun {
        HandoffRun::launch_with(phase, |_, state| {
            state.stage = Stage::Code;
            state.stop_until = Some(Stage::Code);
            state.fallback_agent = Some(AgentKind::Antigravity);
        })
    }

    /// D-04 (never silent) with R2-D1, inside a running supervisor with the D-03
    /// fence active: a handoff commit followed by a relaunch that fails (the
    /// fallback's own delivery canary reads `Absent`, a real production refusal)
    /// emits `agent_handoff_relaunch_failed` and ends at the never-silent Code gate,
    /// opened by the supervisor the state records. Not the cron arm: a recorded
    /// `Absent` refuses every later launch, so a scheduled resume cannot succeed.
    ///
    /// Non-vacuity: `wait_held` returns only because `test_hold` reached
    /// `after_failover_commit`, which it does only inside an engaged supervisor
    /// advance, so the writes below were fenced.
    #[test]
    fn a_handoff_whose_relaunch_fails_ends_at_a_gate() {
        const NAME: &str = "pipeline_launch::tests::a_handoff_whose_relaunch_fails_ends_at_a_gate";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_failover_relaunch_child(NAME);
            return;
        }
        let _guard = env_lock();
        let run = launch_code_run_with_an_antigravity_fallback(PhaseId::new(5169));
        run.go();
        run.wait_held();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                events_named(&run.root, "gate_fired")
                    .iter()
                    .any(|gate| gate["stage"] == "code")
            }),
            "no Code gate fired; events:\n{}",
            run.events()
        );

        let events = all_events(&run.root);
        let position = |name: &str| events.iter().position(|e| e["event"] == name);
        let handed = position("agent_handoff").expect("the handoff committed");
        let failed = position("agent_handoff_relaunch_failed")
            .unwrap_or_else(|| panic!("no relaunch-failure event; events:\n{}", run.events()));
        let gated = position("gate_fired").expect("polled above");
        assert!(
            handed < failed && failed < gated,
            "handoff, then the failure, then the gate; events:\n{}",
            run.events()
        );
        let handoff = &events[handed];
        assert_eq!(handoff["stage"], "code", "{handoff}");
        assert_eq!(handoff["to_agent"], "antigravity", "{handoff}");
        assert_eq!(handoff["reason"], "rate_limited", "{handoff}");
        let failure = &events[failed];
        assert_eq!(failure["stage"], "code", "{failure}");
        assert_eq!(failure["to_agent"], "antigravity", "{failure}");
        assert!(
            failure["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("ABSENT")),
            "the failure names the canary's refusal: {failure}"
        );
        assert!(
            events[gated]["context"]
                .as_str()
                .is_some_and(|context| context.contains("handoff to antigravity")),
            "the gate names the fallback: {}",
            events[gated]
        );
        assert!(
            events[handed..]
                .iter()
                .all(|event| event["event"] != "stage_launched"),
            "nothing launched after the handoff; events:\n{}",
            run.events()
        );
        assert!(
            events_named(&run.root, "rate_limit_resume_scheduled").is_empty(),
            "a failed relaunch never takes the cron arm (R2-D1)"
        );
        assert!(
            !devflow_core::ship::cron_instructions_path(&run.root, run.phase).exists(),
            "a failed relaunch writes no cron instruction (R2-D1)"
        );
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "the supervisor kept its own run; events:\n{}",
            run.events()
        );

        let state = run.state();
        assert_eq!(state.agent, AgentKind::Antigravity);
        assert!(state.fallback_consumed);
        assert_eq!(
            state.infra_failures, 1,
            "handle_infra_outcome's one bump; the hop itself charges nothing (D-06)"
        );
        assert_eq!(state.canary, Some(canary::CanaryOutcome::Absent));
        assert!(state.gate_pending, "the never-silent gate is open");
        assert!(Gates::gate_path(&run.root, run.phase, Stage::Code).exists());
        assert_eq!(
            state
                .supervisor
                .as_ref()
                .map(|record| record.launch_id.as_str()),
            Some(run.n.launch_id.as_str()),
            "the supervisor waiting at the gate is the run's recorded owner"
        );
    }

    /// Phase 50 D-03 is intact: a supervisor that another launch replaced between
    /// the handoff commit and the relaunch writes nothing. Its relaunch's first
    /// owned write is refused with `NotOwner`, which `fail_over_on_rate_limit`
    /// returns unchanged, so it ends superseded naming the newer launch: no
    /// relaunch-failure event, no gate, no cron arm, and the newer record intact.
    /// Paired with [`a_handoff_whose_relaunch_fails_ends_at_a_gate`], this tells a
    /// real replacement (`recorded` names another launch) from a self-inflicted one.
    #[test]
    fn a_failover_superseded_mid_relaunch_writes_nothing() {
        const NAME: &str =
            "pipeline_launch::tests::a_failover_superseded_mid_relaunch_writes_nothing";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_failover_relaunch_child(NAME);
            return;
        }
        let _guard = env_lock();
        const NEWER: &str = "newer-supervisor";
        let run = launch_code_run_with_an_antigravity_fallback(PhaseId::new(5170));
        run.go();
        run.wait_held();
        let mut newer = run.n.clone();
        newer.launch_id = NEWER.to_string();
        newer.socket_path = run.root.join("newer-supervisor.sock");
        workflow::update_state(&run.root, run.phase, |s| {
            s.supervisor = Some(newer);
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                !events_named(&run.root, "supervisor_superseded").is_empty()
            }),
            "the replaced supervisor never ended superseded; events:\n{}",
            run.events()
        );

        let superseded = events_named(&run.root, "supervisor_superseded");
        assert_eq!(superseded.len(), 1, "{superseded:?}");
        assert_eq!(superseded[0]["ours"], run.n.launch_id.as_str());
        assert_eq!(superseded[0]["recorded"], NEWER);
        for name in [
            "agent_handoff_relaunch_failed",
            "gate_fired",
            "rate_limit_resume_scheduled",
        ] {
            assert!(
                events_named(&run.root, name).is_empty(),
                "a replaced supervisor emits no {name}; events:\n{}",
                run.events()
            );
        }
        assert!(!devflow_core::ship::cron_instructions_path(&run.root, run.phase).exists());
        let state = run.state();
        assert!(!state.gate_pending);
        assert_eq!(
            state
                .supervisor
                .as_ref()
                .map(|record| record.launch_id.as_str()),
            Some(NEWER),
            "the newer launch's record is untouched"
        );
    }

    // ---- 51-07: a same-stage relaunch replays the interrupted fix pass (D-09) ----

    /// [`waiting_agents_dir`] whose `claude` and `codex` first write their argv,
    /// NUL-separated, to `<program>-argv.bin` in their working directory, then
    /// create `<program>-argv.done`, then wait (bounded, about 60 s) like the
    /// stubs they replace. The marker is created after the write completes, so a
    /// reader that sees it reads the whole argv.
    fn argv_recording_agents_dir() -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let path_dir = waiting_agents_dir();
        for program in ["claude", "codex"] {
            let path = path_dir.path().join(program);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
                     for arg in \"$@\"; do printf '%s\\0' \"$arg\"; done > {program}-argv.bin\n\
                     : > {program}-argv.done\n\
                     i=0\nwhile [ ! -e never-released ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
                     exit 0\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
    /// argv the spawned `codex` received and a reaper for its monitor.
    fn fail_over_from_code(
        root: &Path,
        phase: PhaseId,
        fix: Option<prompt::FixType>,
    ) -> (Vec<String>, ReapMonitorOnDrop) {
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

        let result = advance(root, Some(phase));
        let reloaded = workflow::load_state(root, phase).unwrap();
        let reap = ReapMonitorOnDrop::after_launch(&reloaded);
        result.unwrap();
        assert_eq!(reloaded.agent, AgentKind::Codex, "the run handed off");
        (recorded_argv(root, "codex"), reap)
    }

    /// D-09 (Code, review B-1): a Code failover in the middle of an audit-fix pass
    /// hands the fallback the audit-fix workflow, rendered by the FALLBACK's
    /// driver, and the assertion is on the argv the spawned `codex` received.
    /// Control in the same test: the same failover from a plain Code pass
    /// delivers the plain execute-phase rendering.
    #[test]
    fn a_code_failover_during_an_audit_fix_pass_replays_audit_fix() {
        const NAME: &str =
            "pipeline_launch::tests::a_code_failover_during_an_audit_fix_pass_replays_audit_fix";
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
            codex.render_prompt(&prompt::StageIntent::Code { phase, fix })
        };

        let fix_dir = tempfile::tempdir().unwrap();
        let fix_phase = PhaseId::new(5171);
        let (argv, _reap_fix) =
            fail_over_from_code(fix_dir.path(), fix_phase, Some(prompt::FixType::AuditFix));
        let audit_fix = code_prompt(fix_phase, Some(prompt::FixType::AuditFix));
        assert_ne!(audit_fix, code_prompt(fix_phase, None));
        assert!(
            argv.contains(&audit_fix),
            "the fallback receives the audit-fix pass in Codex's own rendering: {argv:?}"
        );
        assert!(
            !argv.contains(&code_prompt(fix_phase, None)),
            "the fallback does not receive the plain execute-phase prompt: {argv:?}"
        );

        let plain_dir = tempfile::tempdir().unwrap();
        let plain_phase = PhaseId::new(5175);
        let (argv, _reap_plain) = fail_over_from_code(plain_dir.path(), plain_phase, None);
        assert!(
            argv.contains(&code_prompt(plain_phase, None)),
            "control: a plain Code failover delivers execute-phase: {argv:?}"
        );
        assert!(
            !argv.contains(&code_prompt(plain_phase, Some(prompt::FixType::AuditFix))),
            "control: a plain Code failover delivers no fix prompt: {argv:?}"
        );
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

    /// Whether any live process is left in group `pgid`. Zombies do not
    /// count: a CI job container's PID 1 never reaps an orphaned one
    /// (`test_support::live_group_members`).
    fn group_alive(pgid: u32) -> bool {
        !devflow_core::test_support::live_group_members(pgid).is_empty()
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

    /// R-2, safe point 3 (T-50-54): a stop taken between N+1's ping and its
    /// publish shuts N+1 down through its socket and does not publish it. The
    /// state keeps N's record and is stopped with N's safe-point reason; no
    /// agent of N+1 survives; and N answers `stopped` in under
    /// `TERMINATE_VERIFY_WAIT + 3 s`, below N's final timeout, so a stopped N+1
    /// that sat out its promotion wait instead of ending fails the timing.
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
            "N's stop took {took:?}, not under {bound:?}: N+1 did not end when asked"
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
        assert_eq!(
            last["group_signalled"], true,
            "N+1's agent group was signalled for this stop: {last}"
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

            let result = advance_with(root, phase, Some(Stage::Define));
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

    /// Whether `gate`'s waiter registration no longer reads live.
    fn registration_ended(root: &Path, phase: PhaseId, stage: Stage, gate: &GateId) -> bool {
        Gates::waiter_status(root, phase, stage, gate)
            .is_ok_and(|status| status != devflow_core::gates::WaiterStatus::Live)
    }

    /// Launch a gate fixture and return once N is parked at its Define gate:
    /// the gate file exists and N holds its registration (its owner phase is the
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
        assert_eq!(
            Gates::waiter_status(
                &run.root,
                phase,
                Stage::Define,
                &gate_id_at(&run.root, phase, Stage::Define)
            )
            .unwrap(),
            devflow_core::gates::WaiterStatus::Live,
            "N holds the gate's registration across the wait"
        );
        run
    }

    /// V-15 (R-2, T-50-33): SIGTERM to a supervisor parked at a gate aborts
    /// the wait (safe point 4): within 5 s the gate's registration no longer reads
    /// live, the state is stopped with the safe-point reason, the socket is gone and no next
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
        let gate = gate_id_at(&run.root, run.phase, Stage::Define);
        signal_supervisor(&run, libc::SIGTERM);
        let ended = poll_until(Duration::from_secs(5), || {
            registration_ended(&run.root, run.phase, Stage::Define, &gate)
                && devflow_core::supervisor::probe(&run.n.socket_path)
                    .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
        });
        assert!(
            ended,
            "within 5 s of SIGTERM the registration ends and the socket is gone: {}",
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

    /// Park N at its Define gate, approve the gate, and wait for N to reach the configured
    /// hold with its claim published. Returns the run and the gate's id.
    fn parked_at_a_hold_with_the_gate_claimed(phase: PhaseId) -> (HandoffRun, GateId) {
        let run = launch_parked_at_gate(phase);
        let gate = gate_id_at(&run.root, phase, Stage::Define);
        answer_open_gate(&run.root, phase, Stage::Define, &gate_response(true, None)).unwrap();
        run.wait_held();
        assert!(
            Gates::claim_path(&run.root, phase, Stage::Define, &gate).exists(),
            "precondition: the claim is published: {}",
            run.events()
        );
        (run, gate)
    }

    /// SIGKILL N and wait until its socket reads STALE: it died with nothing cleaned up.
    fn kill_supervisor_until_stale(run: &HandoffRun) {
        signal_supervisor(run, libc::SIGKILL);
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                devflow_core::supervisor::probe(&run.n.socket_path)
                    .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Stale)
            }),
            "N's socket reads STALE after SIGKILL: {}",
            run.events()
        );
    }

    /// What a kill-point fixture requires of `resume`: it reports the approval it found
    /// claimed at Define, keeps no file for that gate, sets nothing aside for it (the
    /// answer was claimed, not unused), and reruns the stage.
    fn assert_resume_reported_the_claimed_approval(run: &HandoffRun, gate: &GateId) {
        let found = events_named(&run.root, "resume_found_unapplied_answer");
        assert_eq!(found.len(), 1, "{}", run.events());
        assert_eq!(found[0]["stage"], "define", "{}", found[0]);
        assert_eq!(found[0]["gate_id"], gate.as_str(), "{}", found[0]);
        assert_eq!(found[0]["approved"], true, "{}", found[0]);
        assert!(!Gates::gate_path(&run.root, run.phase, Stage::Define).exists());
        assert!(!Gates::answer_path(&run.root, run.phase, Stage::Define, gate).exists());
        assert!(!Gates::claim_path(&run.root, run.phase, Stage::Define, gate).exists());
        let quarantined = Gates::quarantined(&run.root);
        assert!(
            quarantined
                .iter()
                .all(|answer| answer.label != gate.as_str()),
            "a claimed answer is not an unused one: {quarantined:?}"
        );
        assert!(!run.state().gate_pending);
        assert_eq!(
            stage_launched_count(&run.root, run.phase),
            2,
            "the rerun launched: {}",
            run.events()
        );
    }

    /// R-02's crash window: the supervisor claimed the answer and died before it recorded
    /// anything, so the state still reads `gate_pending`. Without `resume`'s line the
    /// operator would meet a new gate for a question they answered.
    #[test]
    fn resume_reports_an_answer_claimed_but_never_applied() {
        const NAME: &str =
            "pipeline_launch::tests::resume_reports_an_answer_claimed_but_never_applied";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_claim"), FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        let (run, gate) = parked_at_a_hold_with_the_gate_claimed(PhaseId::new(1312));
        let state = run.state();
        assert!(state.gate_pending, "precondition: nothing recorded yet");
        assert_eq!(state.stage, Stage::Define);
        kill_supervisor_until_stale(&run);

        resume(&run.root, run.phase, None, false).unwrap();

        assert_resume_reported_the_claimed_approval(&run, &gate);
        let removed = events_named(&run.root, "gate_claimed_answer_removed");
        assert_eq!(removed.len(), 1, "{}", run.events());
        assert_eq!(removed[0]["gate_id"], gate.as_str(), "{}", removed[0]);
    }

    /// R2-05's crash window: the supervisor recorded `gate_pending = false` and died before
    /// the retry was dispatched. The flag alone would call this answer spent.
    #[test]
    fn resume_reports_an_answer_committed_but_never_acted_on() {
        const NAME: &str =
            "pipeline_launch::tests::resume_reports_an_answer_committed_but_never_acted_on";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_commit"), FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        let (run, gate) = parked_at_a_hold_with_the_gate_claimed(PhaseId::new(1313));
        let state = run.state();
        assert!(
            !state.gate_pending,
            "precondition: the decision is recorded"
        );
        assert_eq!(state.stage, Stage::Define, "precondition: not yet acted on");
        assert_eq!(
            stage_launched_count(&run.root, run.phase),
            1,
            "precondition: the retry was not dispatched: {}",
            run.events()
        );
        kill_supervisor_until_stale(&run);

        resume(&run.root, run.phase, None, false).unwrap();

        assert_resume_reported_the_claimed_approval(&run, &gate);
    }

    /// The V-15 control: with no signal the gate wait holds (registration live, N in
    /// `gate_wait`) after 3 s, and answering the gate lets it proceed: an
    /// abort answer ends the registration and the supervisor.
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
        let gate = gate_id_at(&run.root, run.phase, Stage::Define);
        std::thread::sleep(Duration::from_secs(3));
        assert!(
            !registration_ended(&run.root, run.phase, Stage::Define, &gate),
            "control: still parked with its registration live after 3 s"
        );
        assert!(
            matches!(
                devflow_core::supervisor::probe(&run.n.socket_path),
                Ok(devflow_core::supervisor::Liveness::Alive(ref reply)) if reply.owner == "gate_wait"
            ),
            "control: N still answers gate_wait"
        );
        assert!(!run.state().stopped, "control: not stopped while waiting");
        answer_open_gate(
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
            registration_ended(&run.root, run.phase, Stage::Define, &gate)
                && devflow_core::supervisor::probe(&run.n.socket_path)
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

    // ---- D-16 (999.166 item 6): the exit file records a safe-point stop ----

    /// The exit file's contents, trimmed, or the read error: the next stage's launch archives
    /// the file, so it can be missing.
    fn exit_file_of(run: &HandoffRun) -> String {
        let path = devflow_core::agent_result::exit_code_path(&run.root, run.phase);
        std::fs::read_to_string(path)
            .map_or_else(|error| format!("<{error}>"), |code| code.trim().to_string())
    }

    /// `stop`'s success line for a final `stopped` reply. A missing field panics.
    fn stop_line_for(run: &HandoffRun, last: &serde_json::Value) -> String {
        crate::commands::stopped_line(
            &format!("phase {}", run.phase),
            i32::try_from(last["exit_code"].as_i64().expect("exit_code")).unwrap(),
            last["advance_suppressed"]
                .as_bool()
                .expect("advance_suppressed"),
            last["group_signalled"].as_bool().expect("group_signalled"),
        )
    }

    /// V-28 (D-16): an agent that completed with `0`, stopped while its supervisor advances.
    /// The exit file records the stop's 143, not the agent's own code, and the line `stop`
    /// prints names the safe point. Precondition: it read `0` before the stop.
    #[test]
    fn a_safe_point_stop_records_143_in_the_exit_file_and_names_the_safe_point() {
        const NAME: &str = "pipeline_launch::tests::a_safe_point_stop_records_143_in_the_exit_file_and_names_the_safe_point";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1317));
        run.go();
        run.wait_held();
        assert_eq!(
            exit_file_of(&run),
            "0",
            "precondition: the agent's own code"
        );
        let (accepted, last) = shutdown_accepted_then(
            &run.n.socket_path,
            || run.release(),
            Duration::from_secs(30),
        );
        assert_eq!(accepted["owner"], "advancing", "{accepted}");
        assert_eq!(last["stopped"], true, "{last}");
        assert_eq!(last["group_signalled"], false, "{last}");
        let state = run.state();
        assert!(state.stopped, "{state:?}");
        assert_eq!(state.stop_reason.as_deref(), Some(ADVANCE_STOP_REASON));
        let line = stop_line_for(&run, &last);
        assert!(line.contains("honoured at an advance safe point"), "{line}");
        assert!(!line.contains("agent group ended"), "{line}");
        assert_eq!(
            exit_file_of(&run),
            "143",
            "the exit file records the stop, as the reply does: {}",
            run.events()
        );
    }

    /// The 143 a safe-point stop leaves is not read as the next run's result: `resume` runs
    /// the stage again, and its own success advances.
    #[test]
    fn a_resume_after_a_safe_point_stop_is_judged_on_its_own_exit_code() {
        const NAME: &str = "pipeline_launch::tests::a_resume_after_a_safe_point_stop_is_judged_on_its_own_exit_code";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1324));
        run.go();
        run.wait_held();
        let (_, last) = shutdown_accepted_then(
            &run.n.socket_path,
            || run.release(),
            Duration::from_secs(30),
        );
        assert_eq!(last["stopped"], true, "{last}");
        assert_eq!(exit_file_of(&run), "143", "{}", run.events());

        resume(&run.root, run.phase, None, false).unwrap();

        let evaluated = |status: &str| {
            events_named(&run.root, "advance_evaluated")
                .iter()
                .any(|event| event["status"] == status)
        };
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || evaluated("success")
                && events_named(&run.root, "transition").len() == 1),
            "the resumed stage succeeds and moves on: {}",
            run.events()
        );
        assert!(!evaluated("failed"), "{}", run.events());
    }

    /// The control for V-28: the same hold and no stop, with an `--until define` halt ending
    /// the run where it is, leaves the agent's own code.
    #[test]
    fn without_a_stop_the_exit_file_keeps_the_agents_code() {
        const NAME: &str =
            "pipeline_launch::tests::without_a_stop_the_exit_file_keeps_the_agents_code";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(1319), |_, state| {
            state.stop_until = Some(Stage::Define);
        });
        run.go();
        run.wait_held();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                devflow_core::supervisor::probe(&run.n.socket_path)
                    .is_ok_and(|alive| alive == devflow_core::supervisor::Liveness::Gone)
            }),
            "the run ends at its halt: {}",
            run.events()
        );
        assert_eq!(exit_file_of(&run), "0", "{}", run.events());
        assert!(events_named(&run.root, "advance_stopped").is_empty());
    }

    /// A stop that lands after a Validate gate was answered and its decision committed, while
    /// the run still advances. The answer's one transition happens, the next safe point stops
    /// the run before anything launches, and `stop`'s line and the exit file both say so.
    #[test]
    fn a_stop_after_a_gate_answer_stops_after_its_transition_and_reports_it() {
        const NAME: &str = "pipeline_launch::tests::a_stop_after_a_gate_answer_stops_after_its_transition_and_reports_it";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_commit"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(1320), |_, state| {
            state.stage = Stage::Validate;
            state.stop_until = None;
        });
        run.go();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                events_named(&run.root, "gate_fired").len() == 1
            }),
            "N parks at the Validate gate: {}",
            run.events()
        );
        answer_open_gate(
            &run.root,
            run.phase,
            Stage::Validate,
            &gate_response(true, None),
        )
        .unwrap();
        run.wait_held();
        let (accepted, last) = shutdown_accepted_then(
            &run.n.socket_path,
            || run.release(),
            Duration::from_secs(30),
        );
        assert_eq!(accepted["owner"], "advancing", "{accepted}");
        assert_eq!(last["stopped"], true, "{last}");
        let transitions = events_named(&run.root, "transition");
        assert_eq!(transitions.len(), 1, "{}", run.events());
        assert_eq!(transitions[0]["to"], "ship", "{}", transitions[0]);
        assert_eq!(
            stage_launched_count(&run.root, run.phase),
            1,
            "nothing launches after the stop: {}",
            run.events()
        );
        let line = stop_line_for(&run, &last);
        assert!(line.contains("honoured at an advance safe point"), "{line}");
        assert_eq!(exit_file_of(&run), "143", "{}", run.events());
        let state = run.state();
        assert!(state.stopped, "{state:?}");
        assert_eq!(state.stop_reason.as_deref(), Some(ADVANCE_STOP_REASON));
    }

    /// A socket stop taken during a gate wait ends the wait at safe point 4, so an answer
    /// written after it finds no gate and moves nothing. `stop`'s line and the exit file
    /// name the stop.
    #[test]
    fn a_stop_during_a_gate_wait_beats_a_later_answer_and_reports_the_safe_point() {
        const NAME: &str = "pipeline_launch::tests::a_stop_during_a_gate_wait_beats_a_later_answer_and_reports_the_safe_point";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, None, FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = launch_parked_at_gate(PhaseId::new(1323));
        let (accepted, last) =
            shutdown_accepted_then(&run.n.socket_path, || {}, Duration::from_secs(30));
        assert_eq!(accepted["owner"], "gate_wait", "{accepted}");
        assert_eq!(last["stopped"], true, "{last}");
        let stopped = events_named(&run.root, "advance_stopped");
        assert_eq!(stopped.len(), 1, "{}", run.events());
        assert_eq!(stopped[0]["safe_point"], 4, "{}", stopped[0]);
        let line = stop_line_for(&run, &last);
        assert!(line.contains("honoured at an advance safe point"), "{line}");
        assert_eq!(exit_file_of(&run), "143", "{}", run.events());
        assert!(
            answer_open_gate(
                &run.root,
                run.phase,
                Stage::Define,
                &gate_response(true, None)
            )
            .is_err(),
            "the stop closed the gate, so a later answer has nothing to answer"
        );
        assert!(events_named(&run.root, "transition").is_empty());
        assert_eq!(stage_launched_count(&run.root, run.phase), 1);
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
            assert!(gate_answer_files(&run.root, run.phase, Stage::Ship).is_empty());
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

    // ---- 50.1-17: the stage handoff records its next supervisor first (D-01) ----

    /// The `<stage>:<path>` hold core's supervisor reads before its agent spawn
    /// (50.1-16): liveness lock adopted, no socket, no agent.
    const AGENT_SPAWN_HOLD_ENV: &str = "DEVFLOW_TEST_AGENT_SPAWN_HOLD";

    /// The parent half of a test whose next supervisor (Plan) is held before its
    /// agent spawn, on the advance-running monitor entry.
    fn run_spawn_held_handoff_child(name: &str) {
        let path_dir = handoff_claude_dir(SUCCESS_MARKER);
        let hold_dir = tempfile::Builder::new()
            .prefix("dfh")
            .tempdir_in("/tmp")
            .unwrap();
        let hold = std::ffi::OsString::from(format!(
            "plan:{}",
            hold_dir.path().join("release").display()
        ));
        let env: Vec<(&str, &std::ffi::OsStr)> = vec![
            (TEST_MONITOR_FULL_ENV, std::ffi::OsStr::new("1")),
            ("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("120")),
            (AGENT_SPAWN_HOLD_ENV, hold.as_os_str()),
        ];
        let output = run_hermetic_test_in_child(name, path_dir.path(), &env);
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, name);
    }

    /// The release file of the agent-spawn hold. Dropping it releases the hold, so a
    /// failing assertion does not leave a supervisor waiting out its bound.
    struct SpawnHold(PathBuf);

    impl SpawnHold {
        fn from_env() -> Self {
            let hold = std::env::var(AGENT_SPAWN_HOLD_ENV).expect("the parent names the hold");
            let (_, path) = hold.split_once(':').expect("<stage>:<path>");
            Self(PathBuf::from(path))
        }

        fn wait_held(&self, run: &HandoffRun) {
            let held = PathBuf::from(format!("{}.held", self.0.display()));
            assert!(
                poll_until(HANDOFF_STEP_WAIT, || held.exists()),
                "the next supervisor never reached its agent-spawn hold; events:\n{}",
                run.events()
            );
        }

        fn release(&self) {
            std::fs::write(&self.0, "").unwrap();
        }
    }

    impl Drop for SpawnHold {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, "");
        }
    }

    /// SIGKILL stage N's supervisor and wait for its socket to read Stale.
    fn kill_stage_n(run: &HandoffRun) {
        signal_supervisor(run, libc::SIGKILL);
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || matches!(
                devflow_core::supervisor::probe(&run.n.socket_path),
                Ok(devflow_core::supervisor::Liveness::Stale)
            )),
            "N's leftover socket probes Stale after the kill"
        );
    }

    /// V-02 (crash-table row C4, T-50-91): S(N) SIGKILLed after spawning N+1 and before
    /// promoting it leaves a state that names N+1, so `resume` refuses. N+1 is held
    /// before its agent spawn and N's agent has exited, so the agent pid file cannot be
    /// what refuses.
    #[test]
    fn resume_refuses_while_the_unpromoted_next_supervisor_lives() {
        use devflow_core::liveness;
        const NAME: &str =
            "pipeline_launch::tests::resume_refuses_while_the_unpromoted_next_supervisor_lives";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_spawn_held_handoff_child(NAME);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1701));
        let hold = SpawnHold::from_env();
        run.go();
        hold.wait_held(&run);
        kill_stage_n(&run);
        assert!(
            crate::commands::agent_pid_from_file(&run.root, run.phase)
                .is_none_or(|pid| !devflow_core::agent::agent_running(pid)),
            "the agent pid file must not be able to cause the refusal"
        );
        let state_file = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&state_file).unwrap();

        let result = resume(&run.root, run.phase, None, true);
        // A resume that wrongly launched is ended before the assertions below fail.
        let after = run.state();
        let _reap = ReapMonitorOnDrop::after_launch(&after);

        let text = result
            .expect_err("resume must refuse while the unpromoted next supervisor lives")
            .to_string();
        let phase = run.phase;
        assert!(
            text.contains(&format!("devflow stop --phase {phase}")),
            "the refusal names stop: {text}"
        );
        let pending = after
            .pending_supervisor
            .clone()
            .expect("the state names N+1 as pending");
        assert_ne!(pending.launch_id, run.n.launch_id, "the pending one is N+1");
        assert!(
            text.contains(&pending.launch_id),
            "the refusal names N+1's launch id: {text}"
        );
        assert!(
            !text.contains("devflow resume"),
            "no resume hint while N+1 lives: {text}"
        );
        assert!(
            !text.contains("agent pid"),
            "the refusal must not come from the agent pid file: {text}"
        );
        assert_eq!(
            after.supervisor.as_ref().map(|r| r.launch_id.as_str()),
            Some(run.n.launch_id.as_str()),
            "the state still names the dead N, beside its pending N+1"
        );
        assert_eq!(
            liveness::probe(&pending.liveness_path).unwrap(),
            liveness::Held::Live,
            "N+1 alone holds its lock now that N is dead"
        );
        assert!(
            !pending.socket_path.exists(),
            "N+1 is held before it publishes a socket"
        );
        assert_eq!(
            std::fs::read(&state_file).unwrap(),
            before,
            "a refused resume writes no state"
        );

        hold.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || matches!(
                devflow_core::supervisor::probe(&pending.socket_path),
                Ok(devflow_core::supervisor::Liveness::Alive(_))
            )),
            "N+1 publishes once released; events:\n{}",
            run.events()
        );
        let outcome =
            devflow_core::supervisor::request_shutdown(&pending.socket_path, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                outcome,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "N+1 ends through its socket: {outcome:?}"
        );
    }

    /// D-01: both supervisors may be live during a handoff, and the state names both
    /// until the publish save promotes the next one. N+1 has answered its ping by then.
    #[test]
    fn a_handoff_names_both_supervisors_until_the_promotion() {
        const NAME: &str =
            "pipeline_launch::tests::a_handoff_names_both_supervisors_until_the_promotion";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1702));
        assert_eq!(
            run.state().pending_supervisor,
            None,
            "precondition: N's own launch was promoted"
        );
        run.go();
        run.wait_held();

        let held = run.state();
        assert_eq!(
            held.supervisor.as_ref().map(|r| r.launch_id.as_str()),
            Some(run.n.launch_id.as_str()),
            "before the publish the state still records N"
        );
        let pending = held
            .pending_supervisor
            .expect("before the publish the state names N+1 as pending");
        assert_ne!(pending.launch_id, run.n.launch_id, "the pending one is N+1");
        match devflow_core::supervisor::probe(&pending.socket_path) {
            Ok(devflow_core::supervisor::Liveness::Alive(reply)) => {
                assert_eq!(reply.launch_id, pending.launch_id);
            }
            other => panic!("N+1 answered its ping before the publish, got {other:?}"),
        }

        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || run
                .state()
                .supervisor
                .is_some_and(|r| r.launch_id == pending.launch_id)),
            "the publish records N+1; events:\n{}",
            run.events()
        );
        assert_eq!(
            run.state().pending_supervisor,
            None,
            "the publish save that records N+1 also ends its reservation"
        );
    }

    /// D-01's prohibition: the publish promotes only a launch the state still names. A
    /// save for a launch whose reservation was cleared, or replaced by another's, writes
    /// nothing. Controls: the launch it names is promoted, and the second save, for a
    /// stop, finds it recorded.
    #[test]
    fn the_publish_save_refuses_a_launch_the_state_no_longer_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1703);
        let prior = fabricated_prior_record(root);
        let launch_id = "aaaa0000aaaa1703";
        let mut launcher = state_with_pending(root, phase, "bbbb0000bbbb1703");
        launcher.supervisor = Some(SupervisorRecord::new(
            root.join("pending.sock"),
            launch_id.to_string(),
            4_294_967_000,
            1,
            String::new(),
        ));
        workflow::update_state(root, phase, |on_disk| {
            on_disk.supervisor = Some(prior.clone());
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        let refused_replaced = save_publish_fields(&launcher);
        let after_replaced = workflow::load_state(root, phase).unwrap();
        assert!(refused_replaced.is_err(), "another launch's reservation");
        assert_eq!(after_replaced.supervisor.as_ref(), Some(&prior));

        workflow::update_state(root, phase, |on_disk| {
            on_disk.pending_supervisor = None;
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        let refused_cleared = save_publish_fields(&launcher);
        let after_cleared = workflow::load_state(root, phase).unwrap();
        assert!(refused_cleared.is_err(), "a cleared reservation");
        assert_eq!(after_cleared.supervisor.as_ref(), Some(&prior));

        state_with_pending(root, phase, launch_id);
        save_publish_fields(&launcher).expect("control: the launch it names");
        let promoted = workflow::load_state(root, phase).unwrap();
        assert_eq!(promoted.supervisor, launcher.supervisor);
        assert_eq!(promoted.pending_supervisor, None, "promoted");

        launcher.stop_reason = Some(NEXT_STOPPED_BEFORE_SAVE_REASON.to_string());
        save_successor_stop(&launcher).expect("control: the stop's save finds it recorded");
        assert!(workflow::load_state(root, phase).unwrap().stopped);
    }

    /// A launch context a reservation can be taken from: nothing here spawns.
    fn reservation_only_context(cache: &Path) -> monitor::LaunchContext {
        monitor::LaunchContext::new(
            cache.to_path_buf(),
            monitor::MonitorExec::Subcommand {
                exe: PathBuf::from("/nonexistent/devflow"),
            },
        )
    }

    /// A pending record whose liveness lock is held stops the launch, naming both launch
    /// ids and changing no state, and the failure path that follows keeps the record.
    /// Control: the same record with its lock released is replaced and its file removed.
    #[test]
    fn a_launch_over_a_live_pending_record_is_refused_and_keeps_it() {
        use devflow_core::liveness;
        const LIVE: &str = "aaaa0000aaaa1704";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1704);
        let cache = tempfile::tempdir().unwrap();
        let ctx = reservation_only_context(cache.path());
        let path = liveness::launch_liveness_path(root, LIVE).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let held = liveness::Hold::take(&path).unwrap();
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.pending_supervisor = Some(devflow_core::state::PendingSupervisor::new(
            LIVE.to_string(),
            root.join("live.sock"),
            path.clone(),
        ));
        workflow::save_state(&state).unwrap();
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();

        let reservation = monitor::Reservation::new(&ctx, &state).unwrap();
        let text = commit_pending_record(&mut state, &reservation)
            .expect_err("a live pending record stops the launch")
            .to_string();
        for id in [LIVE, reservation.launch_id()] {
            assert!(text.contains(id), "the refusal names {id}: {text}");
        }
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "a refused launch writes no state"
        );
        record_failed_launch(&mut state).unwrap();
        assert_eq!(
            workflow::load_state(root, phase)
                .unwrap()
                .pending_supervisor
                .map(|pending| pending.launch_id)
                .as_deref(),
            Some(LIVE),
            "the failure path keeps the record of the launch that is live"
        );
        let refused = events_named(root, "launch_refused_over_live_pending");
        assert_eq!(refused.len(), 1, "the refusal is on the record");
        assert_eq!(refused[0]["pending_launch_id"], LIVE);
        reservation.discard();

        drop(held);
        let reservation = monitor::Reservation::new(&ctx, &state).unwrap();
        commit_pending_record(&mut state, &reservation).expect("control: an abandoned one");
        assert_eq!(
            workflow::load_state(root, phase)
                .unwrap()
                .pending_supervisor
                .map(|pending| pending.launch_id)
                .as_deref(),
            Some(reservation.launch_id()),
            "control: this launch's record replaced it"
        );
        assert!(!path.exists(), "control: the abandoned file is removed");
        reservation.discard();
    }

    /// Names which run of `a_next_stage_spawn_that_fails_clears_its_pending_record` a
    /// child is. Not a DevFlow setting.
    const PENDING_SPAWN_CASE_ENV: &str = "P50_1_17_SPAWN_CASE";

    /// The `.devflow/supervisor-*.live` files under `root`.
    fn liveness_files(root: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(root.join(".devflow")) else {
            return Vec::new();
        };
        entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("supervisor-") && name.ends_with(".live"))
            .collect()
    }

    /// A next-stage spawn that fails leaves no reservation, on either side of the
    /// pending save: `socket_path` fails before it (the over-long cache dir of
    /// `a_failed_monitor_spawn_clears_the_supervisor_record`), and `no_publish` after it
    /// (the socket directory is a file, so the spawned supervisor exits unpublished).
    #[test]
    fn a_next_stage_spawn_that_fails_clears_its_pending_record() {
        const NAME: &str =
            "pipeline_launch::tests::a_next_stage_spawn_that_fails_clears_its_pending_record";
        if !devflow_core::test_support::in_child_test(NAME) {
            for case in ["socket_path", "no_publish"] {
                let path_dir = go_waiting_claude_dir();
                let short = crate::test_support::hermetic_cache_dir();
                let cache = if case == "socket_path" {
                    let long = short.path().join("c".repeat(80));
                    std::fs::create_dir_all(&long).unwrap();
                    long
                } else {
                    let sock = devflow_core::registry::socket_dir_in(short.path());
                    std::fs::create_dir_all(sock.parent().unwrap()).unwrap();
                    std::fs::write(sock, "not a directory").unwrap();
                    short.path().to_path_buf()
                };
                let env: Vec<(&str, &std::ffi::OsStr)> = vec![
                    ("DEVFLOW_CACHE_DIR", cache.as_os_str()),
                    (PENDING_SPAWN_CASE_ENV, std::ffi::OsStr::new(case)),
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
        let case = std::env::var(PENDING_SPAWN_CASE_ENV).expect("the parent names the case");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(1706);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.legacy_claude_launch = true;
        state.supervisor = Some(fabricated_prior_record(root));
        workflow::save_state(&state).unwrap();

        let result = launch_stage(&mut state, None, None);
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);
        let text = result
            .expect_err("this fixture's next-stage spawn fails")
            .to_string();
        let names_its_failure = match case.as_str() {
            "socket_path" => text.contains("sun_path"),
            _ => text.contains("before publishing"),
        };
        assert!(names_its_failure, "{case}: the error names it: {text}");
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(state.pending_supervisor, None, "{case}: in memory");
        assert_eq!(reloaded.pending_supervisor, None, "{case}: on disk");
        assert_eq!(reloaded.supervisor, None, "{case}: the prior record too");
        assert_eq!(
            liveness_files(root),
            Vec::<String>::new(),
            "{case}: no liveness file for the launch that failed"
        );
    }

    /// S(N) killed after its pending save and before the spawn: nothing else holds the
    /// lock, so the reservation reads abandoned, and a following `resume` proceeds,
    /// clears it and says so.
    #[test]
    fn a_launcher_killed_after_its_pending_save_leaves_an_abandoned_reservation() {
        use devflow_core::liveness;
        const NAME: &str = "pipeline_launch::tests::\
                            a_launcher_killed_after_its_pending_save_leaves_an_abandoned_reservation";
        const ABANDONED_MARKER: &str = "reservation under test: ";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = handoff_claude_dir(SUCCESS_MARKER);
            let hold_dir = tempfile::Builder::new()
                .prefix("dfh")
                .tempdir_in("/tmp")
                .unwrap();
            let hold = std::ffi::OsString::from(format!(
                "after_pending_save:{}",
                hold_dir.path().join("release").display()
            ));
            let output = run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[
                    (TEST_MONITOR_FULL_ENV, std::ffi::OsStr::new("1")),
                    ("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("120")),
                    (HOLD_ENV, hold.as_os_str()),
                    // libtest would otherwise swallow the line this test is about.
                    ("RUST_TEST_NOCAPTURE", std::ffi::OsStr::new("1")),
                ],
            );
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let id = stdout
                .lines()
                .find_map(|line| line.split(ABANDONED_MARKER).nth(1))
                .expect("the child names its reservation");
            let line = format!("cleared an abandoned launch reservation {id}");
            assert!(stdout.contains(&line), "missing {line:?}: {stdout}");
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1705));
        run.go();
        run.wait_held();
        let pending = run
            .state()
            .pending_supervisor
            .expect("the pending save precedes this hold");
        println!("{ABANDONED_MARKER}{}", pending.launch_id);
        assert_eq!(
            run.other_sockets(),
            Vec::<PathBuf>::new(),
            "N+1 is not spawned yet"
        );
        kill_stage_n(&run);
        assert!(
            poll_until(Duration::from_secs(5), || {
                liveness::probe(&pending.liveness_path)
                    .is_ok_and(|h| h == liveness::Held::Abandoned)
            }),
            "nothing else holds the dead launcher's lock"
        );

        let result = resume(&run.root, run.phase, None, true);
        let after = run.state();
        let _reap = ReapMonitorOnDrop::after_launch(&after);
        result.expect("an abandoned reservation blocks nothing");
        let recorded = after.supervisor.expect("the resume's launch is recorded");
        assert_ne!(recorded.launch_id, pending.launch_id);
        assert_ne!(recorded.launch_id, run.n.launch_id);
        assert_eq!(
            after.pending_supervisor, None,
            "the resume's own is promoted"
        );
        assert!(
            !pending.liveness_path.exists(),
            "the abandoned file is removed"
        );
    }

    /// A stop at safe point 3 ends the unpublished N+1 through its socket and clears
    /// its pending record, so the state names only what may still be live.
    #[test]
    fn a_stop_at_safe_point_3_clears_the_unpublished_next_pending_record() {
        const NAME: &str = "pipeline_launch::tests::\
                            a_stop_at_safe_point_3_clears_the_unpublished_next_pending_record";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_publish"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1707));
        run.go();
        run.wait_held();
        let pending = run
            .state()
            .pending_supervisor
            .expect("the state names N+1 before safe point 3");

        let (_, last) = shutdown_accepted_then(
            &run.n.socket_path,
            || run.release(),
            Duration::from_secs(30),
        );
        assert_eq!(last["stopped"], true, "N answers stopped: {last}");
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                devflow_core::supervisor::probe(&pending.socket_path)
                    .is_ok_and(|liveness| liveness == devflow_core::supervisor::Liveness::Gone)
            }),
            "N+1 was shut down through its socket"
        );
        let state = run.state();
        assert_eq!(state.pending_supervisor, None, "its record is cleared");
        assert!(state.stopped, "state.stopped after a stop at safe point 3");
        assert_eq!(state.stop_reason.as_deref(), Some(ADVANCE_STOP_REASON));
        assert_eq!(
            state.supervisor.as_ref().map(|r| r.launch_id.as_str()),
            Some(run.n.launch_id.as_str()),
            "N+1 was never published"
        );
        assert!(
            poll_until(Duration::from_secs(5), || !pending.liveness_path.exists()),
            "no liveness file is left for N+1"
        );
    }

    /// A shutdown safe point 3 cannot confirm keeps the pending record, since that next
    /// supervisor may still run, and it still records the stop. Once nothing holds the
    /// lock it reads abandoned, so the next launch clears it. The confirmed case is
    /// `a_stop_at_safe_point_3_clears_the_unpublished_next_pending_record`.
    #[test]
    fn an_unconfirmed_stop_at_safe_point_3_keeps_the_pending_record() {
        use devflow_core::liveness;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1708);
        let cache = tempfile::tempdir().unwrap();
        let ctx = reservation_only_context(cache.path());
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&state).unwrap();
        let reservation = monitor::Reservation::new(&ctx, &state).unwrap();
        commit_pending_record(&mut state, &reservation).unwrap();
        let pending = reservation.pending_record();
        let no_answer = SupervisorRecord::new(
            root.join("never-bound.sock"),
            reservation.launch_id().to_string(),
            4_294_967_000,
            1,
            String::new(),
        );

        end_unpublished(&mut state, &no_answer, reservation).unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.pending_supervisor.as_ref(), Some(&pending));
        assert!(after.stopped, "the stop is recorded either way");
        assert_eq!(
            liveness::probe(&pending.liveness_path).unwrap(),
            liveness::Held::Abandoned,
            "the launcher released its copy and kept the file"
        );
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

    /// D-01 through the real launch path: the launcher reserves, the `__monitor`
    /// re-exec adopts the descriptor at fd 3, and after `launch_stage` has
    /// returned (the launcher's copy dropped) the supervisor alone holds the
    /// lock, under its own intact identity record (R-01), with no take temp left
    /// behind (R-06). The supervisor removes the file as its last act.
    #[test]
    fn a_launched_supervisor_holds_its_liveness_lock_after_the_launcher_drops_it() {
        use devflow_core::liveness::{self, Held};
        const NAME: &str = "pipeline_launch::tests::a_launched_supervisor_holds_its_liveness_lock_after_the_launcher_drops_it";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = go_waiting_claude_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let mut state = State::new(
            PhaseId::new(5007),
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.legacy_claude_launch = true;
        workflow::save_state(&state).unwrap();

        let result = launch_stage(&mut state, None, None);
        let _reap_guard = ReapMonitorOnDrop::after_launch(&state);
        result.expect("the launch succeeds");
        let record = state.supervisor.clone().expect("the launch records it");
        let path = liveness::launch_liveness_path(root, &record.launch_id)
            .expect("a recorded launch id has the launch id format");

        assert_eq!(
            liveness::probe(&path).expect("probe"),
            Held::Live,
            "the supervisor must hold the lock after the launcher dropped its copy"
        );
        let socket = devflow_core::supervisor::SocketPath::new(record.socket_path.clone())
            .expect("the recorded socket path");
        let supervisor_pid = devflow_core::supervisor::read_sidecar(&socket.sidecar_path())
            .expect("read the sidecar")
            .supervisor_pid;
        assert_ne!(
            supervisor_pid,
            std::process::id(),
            "the supervisor is its own process"
        );
        let holder = liveness::live_holder(&path).expect("the holder's record reads");
        assert!(
            matches!(holder, Some((pid, Some(_))) if pid == supervisor_pid),
            "the adopter's record must name supervisor {supervisor_pid}, got {holder:?}"
        );
        let temps: Vec<String> = std::fs::read_dir(root.join(".devflow"))
            .expect("read .devflow")
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(liveness::HOLD_TEMP_SUFFIX))
            .collect();
        assert_eq!(temps, Vec::<String>::new(), "no take temp is left behind");

        let stopped =
            devflow_core::supervisor::request_shutdown(&record.socket_path, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                stopped,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "the supervisor stops through its socket: {stopped:?}"
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while liveness::probe(&path).expect("probe") != Held::Missing {
            assert!(
                std::time::Instant::now() < deadline,
                "the exited supervisor must have removed its liveness file, probe reads {:?}",
                liveness::probe(&path)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn state_with_pending(root: &Path, phase: PhaseId, launch_id: &str) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.pending_supervisor = Some(devflow_core::state::PendingSupervisor::new(
            launch_id.to_string(),
            root.join("pending.sock"),
            root.join("pending.live"),
        ));
        workflow::save_state(&state).unwrap();
        state
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

    const CLEAR_SPAWN_NOT_FOUND: &str =
        "spawn_agent_and_record not found in the non-test code (or under 40 lines)";
    const CLEAR_SPAWN_CLEARS: &str =
        "spawn_agent_and_record clears the supervisor record in its own body";
    const CLEAR_SPAWN_NO_HELPER: &str = "spawn_agent_and_record never calls record_failed_launch";
    const CLEAR_REFUSE_NOT_FOUND: &str =
        "refuse_launch not found in the non-test code (or under 3 lines)";
    const CLEAR_REFUSE_NO_CLEAR: &str = "refuse_launch does not clear the supervisor record";
    const CLEAR_GUARD_OUTSIDE_FAILOVER: &str =
        "FailoverRelaunchGuard::engage( is called outside fail_over_on_rate_limit";

    /// 50-13's rule as a pure scan over `source` (R6-G4): in the non-test,
    /// comment-stripped code, `spawn_agent_and_record` never clears the
    /// supervisor record itself (only its failure helper does) and calls
    /// `record_failed_launch`, and `refuse_launch` clears the record. And
    /// (51-06) every line whose code part engages `FailoverRelaunchGuard` lies
    /// inside `fail_over_on_rate_limit`, the one relaunch allowed to keep the record.
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
        const FAILOVER_HEADER: &str = "fn fail_over_on_rate_limit(";
        let failover = lines
            .iter()
            .position(|line| line.starts_with(FAILOVER_HEADER))
            .map(|start| {
                let len =
                    item_lines(&lines[start..], FAILOVER_HEADER, "}").map_or(0, |item| item.len());
                start..start + len
            });
        for (index, line) in lines.iter().enumerate() {
            if code_part(line).contains("FailoverRelaunchGuard::engage(")
                && !failover.as_ref().is_some_and(|span| span.contains(&index))
            {
                violations.push(CLEAR_GUARD_OUTSIDE_FAILOVER.to_string());
            }
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

        // The failover guard keeps the record through one relaunch only, so it is
        // engaged in `fail_over_on_rate_limit` and nowhere else. Only a line's code
        // part counts: the text before its first `//` outside a string literal.
        let good = format!("{good_spawn}{good_refuse}");
        let engage = "FailoverRelaunchGuard::engage(";
        assert_eq!(
            launch_record_clear_violations(&format!(
                "{good}fn fail_over_on_rate_limit(\n    let _keep = {engage});\n}}\n"
            )),
            Vec::<String>::new(),
            "the one sanctioned site"
        );
        assert_eq!(
            launch_record_clear_violations(&format!(
                "{good}fn other() {{\n    let _keep = {engage});\n}}\n"
            )),
            only(CLEAR_GUARD_OUTSIDE_FAILOVER),
            "an engage call in another fn"
        );
        assert_eq!(
            launch_record_clear_violations(&format!(
                "{good}fn other() {{\n    let u = \"http://x\"; {engage});\n}}\n"
            )),
            only(CLEAR_GUARD_OUTSIDE_FAILOVER),
            "a `//` inside a string literal does not end the code part"
        );
        assert_eq!(
            launch_record_clear_violations(&format!(
                "{good}fn other() {{\n    let x = 1; // {engage} stays in failover\n}}\n"
            )),
            Vec::<String>::new(),
            "a trailing comment is not a call"
        );
    }

    /// Resume a stopped Code run of `phase` at `root` under Claude whose Code pass
    /// was entered with `fix`, and return the argv the spawned `claude` received
    /// and a reaper for its monitor. `legacy_claude_launch` puts the prompt in argv.
    fn resume_code_run(
        root: &Path,
        phase: PhaseId,
        fix: Option<prompt::FixType>,
    ) -> (Vec<String>, ReapMonitorOnDrop) {
        init_repo(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.code_fix = fix;
        state.legacy_claude_launch = true;
        state.stopped = true;
        state.stop_reason = Some("rate limited".to_string());
        workflow::save_state(&state).unwrap();

        let result = resume(root, phase, None, false);
        let reloaded = workflow::load_state(root, phase).unwrap();
        let reap = ReapMonitorOnDrop::after_launch(&reloaded);
        result.unwrap();
        assert_eq!(reloaded.code_fix, fix, "a resume keeps the fix pass");
        (recorded_argv(root, "claude"), reap)
    }

    /// D-09: a `devflow resume` in the middle of an audit-fix pass (the cron arm's
    /// later resume) relaunches that pass, asserted on the argv the spawned
    /// `claude` received. Control: the same resume with no fix pass recorded
    /// delivers the plain execute-phase prompt.
    #[test]
    fn a_resume_mid_audit_fix_replays_audit_fix() {
        const NAME: &str = "pipeline_launch::tests::a_resume_mid_audit_fix_replays_audit_fix";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = argv_recording_agents_dir();
            let output =
                crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }
        let _guard = env_lock();
        let claude = agents::driver_for(AgentKind::Claude);
        let code_prompt = |phase: PhaseId, fix: Option<prompt::FixType>| {
            claude.render_prompt(&prompt::StageIntent::Code { phase, fix })
        };

        let fix_dir = tempfile::tempdir().unwrap();
        let fix_phase = PhaseId::new(5173);
        let (argv, _reap_fix) =
            resume_code_run(fix_dir.path(), fix_phase, Some(prompt::FixType::AuditFix));
        assert!(
            argv.contains(&code_prompt(fix_phase, Some(prompt::FixType::AuditFix))),
            "the resumed agent receives the audit-fix pass: {argv:?}"
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
            !argv.contains(&code_prompt(plain_phase, Some(prompt::FixType::AuditFix))),
            "control: a plain resume delivers no fix prompt: {argv:?}"
        );
    }

    // ---- 51-07: one place renders a no-override stage prompt (D-09, review B-1) ----

    const DEFAULT_RENDER_NEEDLE: &str = concat!("for_stage_in_", "project(");
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
        let render =
            format!("    driver.render_prompt(&StageIntent::{DEFAULT_RENDER_NEEDLE}..));\n");
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
            let reservation = monitor::Reservation::new(&ctx, &state).expect("reserve the launch");
            // A launcher names its launch before it spawns it (D-01). A supervisor the
            // state names nowhere reads as superseded and ends at once (D-03).
            state.pending_supervisor = Some(reservation.pending_record());
            workflow::save_state(&state).unwrap();
            let published = monitor::spawn_monitor(
                &state,
                "claude",
                &[],
                &[],
                monitor::MonitorLaunch::Legacy,
                &ctx,
                &reservation,
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

    /// Whether the pending launch's agent has exited. Its exit file cannot say: a supervisor
    /// writes that only once it has a verdict.
    fn pending_agent_gone(pending: &devflow_core::state::PendingSupervisor) -> bool {
        let sidecar =
            devflow_core::supervisor::read_sidecar(&pending.socket_path.with_extension("json"))
                .expect("the pending launch's sidecar");
        devflow_core::test_support::live_group_members(sidecar.agent_pgid).is_empty()
    }

    /// V-24 (D-15, Pitfall 5): N+1's agent exits while N is held before the publish
    /// save, so its supervisor's id is only pending. Its promotion watch reads that as
    /// wait, and once the test releases N (within `PROMOTION_WAIT`) N+1 is recorded and
    /// advances to its `--until` stop.
    #[test]
    fn a_fast_agent_advances_after_its_launcher_promotes_it() {
        const NAME: &str =
            "pipeline_launch::tests::a_fast_agent_advances_after_its_launcher_promotes_it";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1901));
        std::fs::write(run.root.join("go2"), "").unwrap();
        run.go();
        run.wait_held();
        let pending = run
            .state()
            .pending_supervisor
            .expect("at the hold the state names N+1 as pending");
        assert!(
            poll_until(Duration::from_secs(3), || pending_agent_gone(&pending)),
            "N+1's agent exits while N is held; events:\n{}",
            run.events()
        );
        assert_eq!(
            run.state().supervisor.map(|r| r.launch_id),
            Some(run.n.launch_id.clone()),
            "precondition: N+1 is not recorded yet"
        );
        assert!(
            events_named(&run.root, "workflow_finished").is_empty(),
            "N+1 does not advance while it is only pending: {}",
            run.events()
        );
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || !events_named(
                &run.root,
                "workflow_finished"
            )
            .is_empty()),
            "N+1 advances to its --until stop once promoted: {}",
            run.events()
        );
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&pending.socket_path)),
            "N+1 finishes after its advance"
        );
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "no refusal: {}",
            run.events()
        );
    }

    /// The id of the gate open at `stage`.
    fn gate_id_at(root: &Path, phase: PhaseId, stage: Stage) -> GateId {
        let live = crate::commands::gate_file_id(root, phase, stage)
            .unwrap()
            .expect("a gate is open");
        GateId::parse(&live).unwrap()
    }

    /// Rewrite the state's recorded supervisor to launch `id`, as a takeover would.
    fn supersede(root: &Path, phase: PhaseId, id: &str) {
        workflow::update_state(root, phase, |on_disk| {
            on_disk.supervisor = Some(SupervisorRecord::new(
                root.join("x.sock"),
                id.to_string(),
                4_294_967_000,
                1,
                String::new(),
            ));
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
    }

    /// The one `supervisor_superseded` event, its `ours`, `recorded` and `reason` checked.
    fn superseded_event(run: &HandoffRun, recorded: &str, reason: &str) -> serde_json::Value {
        superseded_event_of(run, &run.n.launch_id, recorded, reason)
    }

    /// [`superseded_event`] for a supervisor other than N.
    fn superseded_event_of(
        run: &HandoffRun,
        ours: &str,
        recorded: &str,
        reason: &str,
    ) -> serde_json::Value {
        let events = events_named(&run.root, "supervisor_superseded");
        assert_eq!(events.len(), 1, "one event: {}", run.events());
        assert_eq!(events[0]["ours"], ours, "{}", events[0]);
        assert_eq!(events[0]["recorded"], recorded, "{}", events[0]);
        assert_eq!(events[0]["reason"], reason, "{}", events[0]);
        events[0].clone()
    }

    /// V-23 (D-03): a superseded supervisor writes nothing, consumes nothing and launches
    /// nothing, says so in one event, leaves no live agent member, and exits. Run (a) is
    /// superseded before its advance writes. Run (b) is superseded at a gate, which is
    /// then answered: no claim, and the gate and its answer stay.
    #[test]
    fn a_superseded_supervisor_writes_no_state_ack_or_cleanup_and_ends_its_agent() {
        const NAME: &str = "pipeline_launch::tests::a_superseded_supervisor_writes_no_state_ack_or_cleanup_and_ends_its_agent";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1902";

        let run = HandoffRun::launch(PhaseId::new(1902));
        run.go();
        run.wait_held();
        supersede(&run.root, run.phase, X);
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)),
            "the superseded supervisor exits: {}",
            run.events()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        assert_eq!(stage_launched_count(&run.root, run.phase), 1, "no launch");
        assert!(
            !Gates::gate_path(&run.root, run.phase, Stage::Define).exists(),
            "no gate opened: {}",
            run.events()
        );
        let event = superseded_event(&run, X, "superseded");
        assert_eq!(event["agent_group"], "already_exited", "{event}");
        assert!(
            !group_alive(run.n.agent_pgid),
            "its agent group has no live member"
        );

        let gated = launch_parked_at_gate(PhaseId::new(1903));
        supersede(&gated.root, gated.phase, X);
        let path = workflow::state_path(&gated.root, gated.phase);
        let before = std::fs::read(&path).unwrap();
        let gate_path = Gates::gate_path(&gated.root, gated.phase, Stage::Define);
        let gate_bytes = std::fs::read(&gate_path).unwrap();
        let gate = gate_id_at(&gated.root, gated.phase, Stage::Define);
        answer_open_gate(
            &gated.root,
            gated.phase,
            Stage::Define,
            &devflow_core::gates::GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".to_string()),
            },
        )
        .unwrap();
        let answer = Gates::answer_path(&gated.root, gated.phase, Stage::Define, &gate);
        let answer_bytes = std::fs::read(&answer).unwrap();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&gated.n.socket_path)),
            "the superseded waiter exits: {}",
            gated.events()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        assert!(
            !Gates::claim_path(&gated.root, gated.phase, Stage::Define, &gate).exists(),
            "no claim"
        );
        assert_eq!(std::fs::read(&gate_path).unwrap(), gate_bytes, "gate stays");
        assert_eq!(
            std::fs::read(&answer).unwrap(),
            answer_bytes,
            "answer stays"
        );
        superseded_event(&gated, X, "superseded");
        assert_eq!(stage_launched_count(&gated.root, gated.phase), 1);
    }

    /// D-03: the next stage's spawn is fenced by its `pending_supervisor` commit. A
    /// supervisor superseded just before it spawns nothing, and its reservation goes.
    #[test]
    fn a_superseded_launcher_spawns_nothing_and_gives_its_reservation_back() {
        const NAME: &str = "pipeline_launch::tests::a_superseded_launcher_spawns_nothing_and_gives_its_reservation_back";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_spawn"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1906";
        let run = HandoffRun::launch(PhaseId::new(1906));
        run.go();
        run.wait_held();
        supersede(&run.root, run.phase, X);
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)),
            "N exits: {}",
            run.events()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        superseded_event(&run, X, "superseded");
        assert!(run.other_sockets().is_empty(), "nothing was spawned");
        assert_eq!(stage_launched_count(&run.root, run.phase), 1);
        assert!(
            poll_until(Duration::from_secs(5), || liveness_files(&run.root)
                .is_empty()),
            "no reservation is left behind: {:?}",
            liveness_files(&run.root)
        );
    }

    /// D-03: the promotion is N's last write as the recorded supervisor, so it is fenced.
    /// A supervisor superseded before it promotes nothing, and the next supervisor it
    /// spawned ends through its socket.
    #[test]
    fn a_superseded_launcher_promotes_nothing_and_ends_its_unpublished_next() {
        const NAME: &str = "pipeline_launch::tests::a_superseded_launcher_promotes_nothing_and_ends_its_unpublished_next";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1907";
        let run = HandoffRun::launch(PhaseId::new(1907));
        run.go();
        run.wait_held();
        let pending = run
            .state()
            .pending_supervisor
            .expect("at the hold the state names N+1 as pending");
        supersede(&run.root, run.phase, X);
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)
                && probes_gone(&pending.socket_path)),
            "N and its unpublished next both exit: {}",
            run.events()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        superseded_event(&run, X, "superseded");
    }

    /// D-03: a hook can merge, bump, tag and push, so a supervisor the state no longer
    /// records does not start one. Superseded at the last hold before finalization, it
    /// ships nothing and keeps the feature branch.
    #[test]
    fn a_superseded_supervisor_runs_no_finalization_hooks() {
        const NAME: &str =
            "pipeline_launch::tests::a_superseded_supervisor_runs_no_finalization_hooks";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_finalization"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1915";
        let phase = PhaseId::new(1915);
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
        supersede(&run.root, run.phase, X);
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)),
            "it exits: {}",
            run.events()
        );
        assert!(
            branch_exists(&run.root, &branch),
            "no after-ship hook ran: the feature branch is kept: {}",
            run.events()
        );
        assert!(
            events_named(&run.root, "workflow_shipped").is_empty(),
            "{}",
            run.events()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        superseded_event(&run, X, "superseded");
    }

    /// V-23's control: the same run with no rewrite writes, opens its gate, claims the
    /// answer and logs no refusal.
    #[test]
    fn the_recorded_supervisor_writes_normally_and_logs_no_refusal() {
        const NAME: &str =
            "pipeline_launch::tests::the_recorded_supervisor_writes_normally_and_logs_no_refusal";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("after_safe_point_1"), FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1904));
        run.go();
        run.wait_held();
        run.release();
        let gate_path = Gates::gate_path(&run.root, run.phase, Stage::Define);
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || gate_path.exists()),
            "control: the recorded supervisor opens its gate: {}",
            run.events()
        );
        assert!(run.state().gate_pending, "control: it writes state");
        answer_open_gate(
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
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)),
            "control: it ends after the answer: {}",
            run.events()
        );
        assert_eq!(
            events_named(&run.root, "gate_resolved").len(),
            1,
            "control: it claims the answer: {}",
            run.events()
        );
        assert_eq!(events_named(&run.root, "workflow_aborted").len(), 1);
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "control: no refusal: {}",
            run.events()
        );
    }

    /// Every file under the gates directory, by name, with its bytes.
    fn gates_snapshot(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(Gates::dir(root))
            .unwrap()
            .flatten()
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read(entry.path()).unwrap_or_default(),
                )
            })
            .collect()
    }

    /// R-09, the supervisor's half: a superseded waiter changes no byte of the current
    /// gate's files, and a bound answer reaching its own old slot ends it without a claim.
    /// The closing control closes the stage and the same comparison sees it, so the
    /// comparison is not vacuous.
    #[test]
    fn a_superseded_waiter_leaves_the_next_gates_files_untouched() {
        const NAME: &str =
            "pipeline_launch::tests::a_superseded_waiter_leaves_the_next_gates_files_untouched";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, None, FAILED_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1905";
        let run = launch_parked_at_gate(PhaseId::new(1905));
        let (root, phase, stage) = (run.root.clone(), run.phase, Stage::Define);
        let g1 = gate_id_at(&root, phase, stage);
        supersede(&root, phase, X);

        let (opened, registration) = Gates::open_registered(&root, phase, stage, "g2").unwrap();
        let g2 = opened.gate_id.clone();
        let response = devflow_core::gates::GateResponse {
            approved: true,
            note: None,
            responded_by: Some("test".to_string()),
        };
        Gates::answer(&root, phase, stage, &g2, &response).unwrap();
        let snapshot = gates_snapshot(&root);
        std::thread::sleep(Duration::from_millis(3500));
        assert_eq!(
            gates_snapshot(&root),
            snapshot,
            "S1's backoff reads changed nothing"
        );

        let g2_files = |root: &Path| {
            (
                std::fs::read(Gates::gate_path(root, phase, stage)).unwrap(),
                std::fs::read(Gates::answer_path(root, phase, stage, &g2)).unwrap(),
            )
        };
        let g2_before = g2_files(&root);
        let stale = devflow_core::gates::GateAnswer {
            gate_id: g1.to_string(),
            response,
        };
        std::fs::write(
            Gates::answer_path(&root, phase, stage, &g1),
            serde_json::to_string_pretty(&stale).unwrap(),
        )
        .unwrap();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&run.n.socket_path)),
            "S1 ends: {}",
            run.events()
        );
        superseded_event(&run, X, "superseded");
        for gate in [&g1, &g2] {
            assert!(
                !Gates::claim_path(&root, phase, stage, gate).exists(),
                "no claim for {gate}"
            );
        }
        assert_eq!(g2_files(&root), g2_before, "G2's files are byte-identical");
        assert_eq!(
            Gates::waiter_status(&root, phase, stage, &g2).unwrap(),
            devflow_core::gates::WaiterStatus::Live,
            "G2's registration still reads Live"
        );

        let before_close = gates_snapshot(&root);
        Gates::close(&root, phase, stage).unwrap();
        assert_ne!(
            gates_snapshot(&root),
            before_close,
            "control: the comparison sees a closed stage"
        );
        registration.finish();
    }

    /// Whether a live member of group `pgid` is the fixture's `sleep 300` descendant.
    fn descendant_runs_in_group(pgid: u32) -> bool {
        devflow_core::test_support::live_group_members(pgid)
            .into_iter()
            .any(|pid| {
                std::fs::read(format!("/proc/{pid}/cmdline"))
                    .is_ok_and(|cmdline| cmdline.split(|byte| *byte == 0).any(|arg| arg == b"300"))
            })
    }

    /// A reservation's lock once its supervisor has ended: never live.
    fn assert_not_live(pending: &devflow_core::state::PendingSupervisor) {
        use devflow_core::liveness;
        let read = liveness::probe(&pending.liveness_path).unwrap();
        assert!(
            matches!(read, liveness::Held::Abandoned | liveness::Held::Missing),
            "the reservation reads {read:?}"
        );
    }

    /// V-27 (D-15, R-12): N is SIGKILLed before its publish save, so N+1 is an orphan
    /// whose agent never exits on its own. The promotion deadline runs from serving,
    /// beside the agent, so within the bound the agent's whole group is gone, far inside
    /// the agent's own 60 s. It writes no state and does not promote itself, says so in
    /// one event, exits, and leaves the run for `resume`.
    #[test]
    fn an_orphaned_pending_supervisor_ends_itself_when_its_promotion_wait_expires() {
        const NAME: &str = "pipeline_launch::tests::an_orphaned_pending_supervisor_ends_itself_when_its_promotion_wait_expires";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1908));
        std::fs::write(run.root.join("descendant"), "").unwrap();
        run.go();
        run.wait_held();
        let served = std::time::Instant::now();
        kill_stage_n(&run);
        let pending = run
            .state()
            .pending_supervisor
            .expect("the state names N+1 as pending");
        let sidecar =
            devflow_core::supervisor::read_sidecar(&pending.socket_path.with_extension("json"))
                .expect("N+1's sidecar");
        let pgid = sidecar.agent_pgid;
        assert!(
            poll_until(Duration::from_secs(3), || descendant_runs_in_group(pgid)),
            "precondition: the agent's descendant runs in its group"
        );
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();

        let bound =
            PROMOTION_WAIT + devflow_core::agent::TERMINATE_VERIFY_WAIT + Duration::from_secs(2);
        let gone = poll_until(bound.saturating_sub(served.elapsed()), || {
            devflow_core::test_support::live_group_members(pgid).is_empty()
        });
        assert!(
            gone,
            "the group has no live member within {bound:?} of serving: {:?}; events:\n{}",
            devflow_core::test_support::live_group_members(pgid),
            run.events()
        );
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&pending.socket_path)),
            "N+1 exits"
        );
        assert!(
            poll_until(Duration::from_secs(5), || !matches!(
                devflow_core::liveness::probe(&pending.liveness_path),
                Ok(devflow_core::liveness::Held::Live)
            )),
            "N+1's lock is released"
        );
        assert_not_live(&pending);
        let event = superseded_event_of(
            &run,
            &pending.launch_id,
            &run.n.launch_id,
            "orphaned_pending",
        );
        assert_eq!(event["agent_group"], "ended", "{event}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        let after = run.state();
        assert_eq!(
            after.supervisor.as_ref().map(|r| r.launch_id.as_str()),
            Some(run.n.launch_id.as_str()),
            "no self-promotion: the state still names N"
        );
        assert_eq!(
            after
                .pending_supervisor
                .as_ref()
                .map(|p| p.launch_id.as_str()),
            Some(pending.launch_id.as_str()),
            "N+1 stays only pending"
        );

        let resumed = resume(&run.root, run.phase, None, true);
        let state = run.state();
        let _reap = ReapMonitorOnDrop::after_launch(&state);
        resumed.expect("resume clears the abandoned reservation and proceeds");
        assert_ne!(
            state.supervisor.as_ref().map(|r| r.launch_id.as_str()),
            Some(run.n.launch_id.as_str()),
            "resume launched a new supervisor"
        );
    }

    /// D-03: a supervisor superseded while its agent still runs ends that agent's group
    /// beside it, rather than leaving the agent to keep changing the worktree. N is
    /// SIGKILLed so only N+1 can act; the takeover is written inside N+1's promotion wait,
    /// so the event's reason and `recorded` tell this from V-27's expiry, and the agent's
    /// own 60 s is far past the bound.
    #[test]
    fn a_supervisor_superseded_while_its_agent_runs_ends_the_agents_group() {
        const NAME: &str = "pipeline_launch::tests::a_supervisor_superseded_while_its_agent_runs_ends_the_agents_group";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1930";
        let run = HandoffRun::launch(PhaseId::new(1930));
        run.go();
        run.wait_held();
        kill_stage_n(&run);
        let pending = run
            .state()
            .pending_supervisor
            .expect("the state names N+1 as pending");
        let pgid =
            devflow_core::supervisor::read_sidecar(&pending.socket_path.with_extension("json"))
                .expect("N+1's sidecar")
                .agent_pgid;
        assert!(
            !devflow_core::test_support::live_group_members(pgid).is_empty(),
            "precondition: N+1's agent runs"
        );

        // One write, so N+1 never reads a state that still records the dead N.
        workflow::update_state(&run.root, run.phase, |on_disk| {
            on_disk.pending_supervisor = None;
            on_disk.supervisor = Some(SupervisorRecord::new(
                run.root.join("x.sock"),
                X.to_string(),
                4_294_967_000,
                1,
                String::new(),
            ));
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();

        let bound = devflow_core::agent::TERMINATE_VERIFY_WAIT + Duration::from_secs(2);
        assert!(
            poll_until(bound, || devflow_core::test_support::live_group_members(
                pgid
            )
            .is_empty()),
            "the group has no live member within {bound:?}: {:?}; events:\n{}",
            devflow_core::test_support::live_group_members(pgid),
            run.events()
        );
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&pending.socket_path)),
            "N+1 exits"
        );
        let event = superseded_event_of(&run, &pending.launch_id, X, "superseded");
        assert_eq!(event["agent_group"], "ended", "{event}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
    }

    /// V-27's other half, and the case 50.1-17 left at "refused at its next handoff": an
    /// orphan whose agent already exited never reaches its advance, so it never reaches
    /// a handoff to be refused at. It ends at the deadline, with nothing to signal.
    #[test]
    fn an_orphan_whose_agent_already_exited_ends_short_of_its_handoff() {
        const NAME: &str = "pipeline_launch::tests::an_orphan_whose_agent_already_exited_ends_short_of_its_handoff";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1909));
        std::fs::write(run.root.join("go2"), "").unwrap();
        run.go();
        run.wait_held();
        kill_stage_n(&run);
        let pending = run
            .state()
            .pending_supervisor
            .expect("the state names N+1 as pending");
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&pending.socket_path)),
            "N+1 exits: {}",
            run.events()
        );
        let event = superseded_event_of(
            &run,
            &pending.launch_id,
            &run.n.launch_id,
            "orphaned_pending",
        );
        assert_eq!(event["agent_group"], "already_exited", "{event}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        assert!(
            events_named(&run.root, "launch_refused_over_live_pending").is_empty(),
            "it never reached a handoff: {}",
            run.events()
        );
        assert!(
            events_named(&run.root, "workflow_finished").is_empty(),
            "it did not advance: {}",
            run.events()
        );
        assert_eq!(stage_launched_count(&run.root, run.phase), 1, "no launch");
    }

    /// A stop ends the promotion wait of a supervisor whose agent has finished: it answers
    /// `stopped` at once instead of sitting out the deadline, and its launcher, released
    /// after, finds it gone and records the stop.
    #[test]
    fn a_stop_ends_the_promotion_wait_of_a_supervisor_whose_agent_finished() {
        const NAME: &str = "pipeline_launch::tests::a_stop_ends_the_promotion_wait_of_a_supervisor_whose_agent_finished";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1910));
        std::fs::write(run.root.join("go2"), "").unwrap();
        run.go();
        run.wait_held();
        let pending = run
            .state()
            .pending_supervisor
            .expect("the state names N+1 as pending");
        assert!(
            poll_until(Duration::from_secs(3), || pending_agent_gone(&pending)),
            "precondition: N+1's agent has exited"
        );
        let started = std::time::Instant::now();
        let outcome =
            devflow_core::supervisor::request_shutdown(&pending.socket_path, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                outcome,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "N+1 answers stopped: {outcome:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "it did not sit out the deadline: {:?}",
            started.elapsed()
        );
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "a stop is not a supersession: {}",
            run.events()
        );
        release_and_wait_for_n(&run);
        let state = run.state();
        assert!(state.stopped, "the launcher records the stop: {state:?}");
        assert_eq!(
            state.stop_reason.as_deref(),
            Some(NEXT_STOPPED_BEFORE_SAVE_REASON)
        );
    }

    /// V-27's control (R-12): N is released in time, so N+1 is promoted, and it keeps its
    /// running agent past the deadline an orphan would have died at.
    #[test]
    fn a_promoted_supervisor_keeps_its_running_agent_past_the_promotion_wait() {
        const NAME: &str = "pipeline_launch::tests::a_promoted_supervisor_keeps_its_running_agent_past_the_promotion_wait";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1911));
        run.go();
        run.wait_held();
        let served = std::time::Instant::now();
        let pending = run
            .state()
            .pending_supervisor
            .expect("the state names N+1 as pending");
        let pgid =
            devflow_core::supervisor::read_sidecar(&pending.socket_path.with_extension("json"))
                .expect("N+1's sidecar")
                .agent_pgid;
        run.release();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || run
                .state()
                .supervisor
                .is_some_and(|r| r.launch_id == pending.launch_id)),
            "N+1 is promoted: {}",
            run.events()
        );
        std::thread::sleep(
            (PROMOTION_WAIT + Duration::from_secs(2)).saturating_sub(served.elapsed()),
        );
        assert!(
            !devflow_core::test_support::live_group_members(pgid).is_empty(),
            "the promoted supervisor's agent still runs: {}",
            run.events()
        );
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "no refusal: {}",
            run.events()
        );
        std::fs::write(run.root.join("go2"), "").unwrap();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || !events_named(
                &run.root,
                "workflow_finished"
            )
            .is_empty()),
            "the run reaches its --until stop: {}",
            run.events()
        );
    }

    /// Longer than several of the watch's reads after a promotion (one a second), and far
    /// short of the fake agent's own 60 s.
    const PAST_THE_RECORDED_READS: Duration = Duration::from_millis(3500);

    /// N+1 promoted, N gone, N+1's agent waiting for `go2`. Returns N+1's launch id, socket
    /// and agent group. The pause is well past the 20 ms promotion poll, so its watch has
    /// read `Recorded` before the caller changes the state; a stalled host would only send
    /// the case through the earlier verdict's arm, which ends the group the same way.
    fn promoted_next(run: &HandoffRun) -> (String, PathBuf, u32) {
        run.go();
        run.wait_held();
        let pending = run
            .state()
            .pending_supervisor
            .expect("the state names N+1 as pending");
        let pgid =
            devflow_core::supervisor::read_sidecar(&pending.socket_path.with_extension("json"))
                .expect("N+1's sidecar")
                .agent_pgid;
        release_and_wait_for_n(run);
        assert!(
            run.state()
                .supervisor
                .is_some_and(|r| r.launch_id == pending.launch_id),
            "N+1 is promoted: {}",
            run.events()
        );
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            !devflow_core::test_support::live_group_members(pgid).is_empty(),
            "precondition: N+1's agent runs"
        );
        (pending.launch_id, pending.socket_path, pgid)
    }

    /// How long a group the watch ends may stay: one read, then the terminate wait.
    fn recorded_watch_end_bound() -> Duration {
        Duration::from_secs(1) + devflow_core::agent::TERMINATE_VERIFY_WAIT + Duration::from_secs(2)
    }

    /// How long past a short absence a state that stays gone may leave the group running:
    /// the watch's own bound on an absence (3 s, private to `monitor`), then one read and
    /// the terminate wait.
    const STATE_GONE_END_BOUND: Duration = Duration::from_secs(8);

    /// Assert the group ends within [`recorded_watch_end_bound`] and the supervisor exits.
    fn assert_group_ended_and_supervisor_gone(run: &HandoffRun, socket: &Path, pgid: u32) {
        assert_group_ended_and_supervisor_gone_within(
            run,
            socket,
            pgid,
            recorded_watch_end_bound(),
        );
    }

    /// Assert the group ends within `bound` and the supervisor exits.
    fn assert_group_ended_and_supervisor_gone_within(
        run: &HandoffRun,
        socket: &Path,
        pgid: u32,
        bound: Duration,
    ) {
        assert!(
            poll_until(bound, || devflow_core::test_support::live_group_members(
                pgid
            )
            .is_empty()),
            "the group has no live member within {bound:?}: {:?}; events:\n{}",
            devflow_core::test_support::live_group_members(pgid),
            run.events()
        );
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(socket)),
            "the supervisor exits: {}",
            run.events()
        );
    }

    /// Assert the promoted agent outlives [`PAST_THE_RECORDED_READS`] with no refusal logged.
    fn assert_agent_outlives_the_reads(run: &HandoffRun, pgid: u32) {
        std::thread::sleep(PAST_THE_RECORDED_READS);
        assert!(
            !devflow_core::test_support::live_group_members(pgid).is_empty(),
            "the agent still runs: {}",
            run.events()
        );
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "no refusal: {}",
            run.events()
        );
    }

    /// Let N+1's agent finish and assert the run reaches its `--until` stop, unrefused.
    fn assert_run_finishes_normally(run: &HandoffRun) {
        std::fs::write(run.root.join("go2"), "").unwrap();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || !events_named(
                &run.root,
                "workflow_finished"
            )
            .is_empty()),
            "the run reaches its --until stop: {}",
            run.events()
        );
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "no refusal: {}",
            run.events()
        );
    }

    /// SUPV-04: a takeover that lands after the promotion ends the running agent's group.
    /// The supervisor logs it and writes nothing, and launches nothing.
    #[test]
    fn a_supervisor_superseded_after_its_promotion_ends_its_agents_group() {
        const NAME: &str = "pipeline_launch::tests::a_supervisor_superseded_after_its_promotion_ends_its_agents_group";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1931";
        let run = HandoffRun::launch(PhaseId::new(1931));
        let (ours, socket, pgid) = promoted_next(&run);
        let launched = stage_launched_count(&run.root, run.phase);

        supersede(&run.root, run.phase, X);
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();

        assert_group_ended_and_supervisor_gone(&run, &socket, pgid);
        let event = superseded_event_of(&run, &ours, X, "superseded");
        assert_eq!(event["agent_group"], "ended", "{event}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        assert_eq!(
            stage_launched_count(&run.root, run.phase),
            launched,
            "no launch"
        );
    }

    /// A state file that is gone after the promotion names no launch, so it ends the group
    /// too, and the event records that no launch replaced this one.
    #[test]
    fn a_state_gone_after_the_promotion_ends_the_agents_group() {
        const NAME: &str =
            "pipeline_launch::tests::a_state_gone_after_the_promotion_ends_the_agents_group";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1932));
        let (ours, socket, pgid) = promoted_next(&run);
        let path = workflow::state_path(&run.root, run.phase);

        std::fs::remove_file(&path).unwrap();

        std::thread::sleep(SHORT_STATE_ABSENCE);
        assert!(
            !devflow_core::test_support::live_group_members(pgid).is_empty(),
            "within the bound the agent still runs: {}",
            run.events()
        );
        assert_group_ended_and_supervisor_gone_within(&run, &socket, pgid, STATE_GONE_END_BOUND);
        let events = events_named(&run.root, "supervisor_superseded");
        assert_eq!(events.len(), 1, "one event: {}", run.events());
        assert_eq!(events[0]["ours"], ours.as_str(), "{}", events[0]);
        assert_eq!(
            events[0].get("recorded"),
            Some(&serde_json::Value::Null),
            "{}",
            events[0]
        );
        assert_eq!(events[0]["reason"], "superseded", "{}", events[0]);
        assert_eq!(events[0]["agent_group"], "ended", "{}", events[0]);
        assert!(!path.exists(), "no state write");
    }

    /// Less than the watch's bound on a state file that stays absent, and longer than its
    /// one-second read interval, so at least one read lands in the absence.
    const SHORT_STATE_ABSENCE: Duration = Duration::from_millis(1500);

    /// Move the state file away for [`SHORT_STATE_ABSENCE`], then put it back.
    fn absent_for_a_moment(path: &Path) {
        let away = path.with_extension("away");
        std::fs::rename(path, &away).unwrap();
        std::thread::sleep(SHORT_STATE_ABSENCE);
        std::fs::rename(&away, path).unwrap();
    }

    /// Something that renames the state away and back during its own save reads as absent
    /// for a moment. That says nothing about who owns the run, so the agent keeps running
    /// and the run goes on. `a_state_gone_after_the_promotion_ends_the_agents_group` is the
    /// control: the same absence, past the bound, ends the group.
    #[test]
    fn a_state_absent_for_a_moment_after_the_promotion_leaves_the_agent_running() {
        const NAME: &str = "pipeline_launch::tests::a_state_absent_for_a_moment_after_the_promotion_leaves_the_agent_running";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1937));
        let (_, _, pgid) = promoted_next(&run);

        absent_for_a_moment(&workflow::state_path(&run.root, run.phase));
        assert_agent_outlives_the_reads(&run, pgid);

        assert_run_finishes_normally(&run);
    }

    /// The absence must persist across consecutive reads: a read that finds the state again
    /// starts the count over, so two short absences never add up to the bound.
    #[test]
    fn a_state_seen_between_two_absences_starts_the_count_over() {
        const NAME: &str =
            "pipeline_launch::tests::a_state_seen_between_two_absences_starts_the_count_over";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1938));
        let (_, _, pgid) = promoted_next(&run);
        let path = workflow::state_path(&run.root, run.phase);

        absent_for_a_moment(&path);
        // Longer than the read interval, so a read finds the state between the absences.
        std::thread::sleep(Duration::from_millis(1200));
        absent_for_a_moment(&path);
        assert_agent_outlives_the_reads(&run, pgid);

        assert_run_finishes_normally(&run);
    }

    /// `git clean -fdx` under `--no-worktree` takes `.devflow`, with the state and the event
    /// log. The supervisor ends the agent and still leaves its event, in a log it recreates,
    /// because its stderr is `/dev/null` and nothing else would say why the agent ended.
    #[test]
    fn a_removed_devflow_dir_ends_the_agent_and_leaves_a_record() {
        const NAME: &str =
            "pipeline_launch::tests::a_removed_devflow_dir_ends_the_agent_and_leaves_a_record";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1939));
        let (ours, socket, pgid) = promoted_next(&run);
        let state_file = workflow::state_path(&run.root, run.phase);

        std::fs::remove_dir_all(state_file.parent().unwrap()).unwrap();

        assert_group_ended_and_supervisor_gone_within(&run, &socket, pgid, STATE_GONE_END_BOUND);
        let events = events_named(&run.root, "supervisor_superseded");
        assert_eq!(events.len(), 1, "one event: {}", run.events());
        assert_eq!(events[0]["ours"], ours.as_str(), "{}", events[0]);
        assert_eq!(events[0]["agent_group"], "ended", "{}", events[0]);
        assert!(!state_file.exists(), "no state write");
    }

    /// An agent that exits right after `.devflow` went has no verdict against it, and its
    /// exit file has nowhere to go. That failure is the supervisor's last word, and its
    /// stderr is `/dev/null`, so it goes to the log `emit` recreates.
    #[test]
    fn a_removed_devflow_dir_with_the_agent_exiting_leaves_a_record_of_the_failure() {
        const NAME: &str = "pipeline_launch::tests::a_removed_devflow_dir_with_the_agent_exiting_leaves_a_record_of_the_failure";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1940));
        let (ours, _, _) = promoted_next(&run);
        let state_file = workflow::state_path(&run.root, run.phase);

        std::fs::remove_dir_all(state_file.parent().unwrap()).unwrap();
        std::fs::write(run.root.join("go2"), "").unwrap();

        assert!(
            poll_until(HANDOFF_STEP_WAIT, || !events_named(
                &run.root,
                "supervisor_failed"
            )
            .is_empty()),
            "the failure is on record: {}",
            run.events()
        );
        let events = events_named(&run.root, "supervisor_failed");
        assert_eq!(events[0]["launch_id"], ours.as_str(), "{}", events[0]);
        assert!(
            events[0]["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{}",
            events[0]
        );
    }

    /// A state that does not parse after the promotion says nothing about who owns the run,
    /// so a healthy agent keeps running, and the run goes on once the state reads again.
    #[test]
    fn an_unparsable_state_after_the_promotion_leaves_the_agent_running() {
        const NAME: &str = "pipeline_launch::tests::an_unparsable_state_after_the_promotion_leaves_the_agent_running";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1933));
        let (_, _, pgid) = promoted_next(&run);
        let path = workflow::state_path(&run.root, run.phase);
        let good = std::fs::read(&path).unwrap();

        std::fs::write(&path, "{ not json").unwrap();
        assert_agent_outlives_the_reads(&run, pgid);

        std::fs::write(&path, &good).unwrap();
        assert_run_finishes_normally(&run);
    }

    /// A state path that cannot be examined (here a symlink loop, `ELOOP`, which also holds
    /// for root) is an I/O failure and not a state that is gone, so it leaves a healthy
    /// agent running.
    #[test]
    fn a_state_path_that_cannot_be_examined_leaves_the_agent_running() {
        const NAME: &str =
            "pipeline_launch::tests::a_state_path_that_cannot_be_examined_leaves_the_agent_running";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1934));
        let (_, _, pgid) = promoted_next(&run);
        let path = workflow::state_path(&run.root, run.phase);
        let good = std::fs::read(&path).unwrap();

        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&path, &path).unwrap();
        let loaded = workflow::load_state(&run.root, run.phase);
        assert!(
            matches!(&loaded, Err(workflow::WorkflowError::Io(_))),
            "an I/O failure, not a missing state: {loaded:?}"
        );
        assert_agent_outlives_the_reads(&run, pgid);

        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, &good).unwrap();
        assert_run_finishes_normally(&run);
    }

    /// A state that names this launch only as pending after its promotion still names it,
    /// so it is no takeover and the agent keeps running. The ownership fence still refuses
    /// what this supervisor would write next, once the agent has exited.
    #[test]
    fn a_state_naming_it_only_pending_after_the_promotion_leaves_the_agent_to_the_fence() {
        const NAME: &str = "pipeline_launch::tests::a_state_naming_it_only_pending_after_the_promotion_leaves_the_agent_to_the_fence";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        const X: &str = "aaaa0000aaaa1935";
        let run = HandoffRun::launch(PhaseId::new(1935));
        let (ours, socket, pgid) = promoted_next(&run);
        workflow::update_state(&run.root, run.phase, |on_disk| {
            on_disk.pending_supervisor = Some(devflow_core::state::PendingSupervisor::new(
                ours.clone(),
                socket.clone(),
                run.root.join("x.live"),
            ));
            on_disk.supervisor = Some(SupervisorRecord::new(
                run.root.join("x.sock"),
                X.to_string(),
                4_294_967_000,
                1,
                String::new(),
            ));
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        let path = workflow::state_path(&run.root, run.phase);
        let before = std::fs::read(&path).unwrap();

        assert_agent_outlives_the_reads(&run, pgid);

        std::fs::write(run.root.join("go2"), "").unwrap();
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || probes_gone(&socket)),
            "the supervisor exits: {}",
            run.events()
        );
        let event = superseded_event_of(&run, &ours, X, "superseded");
        assert_eq!(event["agent_group"], "already_exited", "{event}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "no state write");
        assert!(
            events_named(&run.root, "workflow_finished").is_empty(),
            "it did not advance: {}",
            run.events()
        );
    }

    /// A stop that reaches a promoted supervisor while its agent runs ends as a stop: the
    /// watch stays out of it, and the state records the stop.
    #[test]
    fn a_stop_after_the_promotion_is_not_a_supersession() {
        const NAME: &str =
            "pipeline_launch::tests::a_stop_after_the_promotion_is_not_a_supersession";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_handoff_child(NAME, Some("before_record_save"), SUCCESS_MARKER);
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch(PhaseId::new(1936));
        let (_, socket, pgid) = promoted_next(&run);
        let outcome = devflow_core::supervisor::request_shutdown(&socket, SUPERVISOR_STOP_WAIT);
        assert!(
            matches!(
                outcome,
                Ok(devflow_core::supervisor::ShutdownOutcome::Stopped { .. })
            ),
            "N+1 answers stopped: {outcome:?}"
        );
        assert!(
            poll_until(recorded_watch_end_bound(), || {
                devflow_core::test_support::live_group_members(pgid).is_empty()
            }),
            "the stop ended the group"
        );
        assert!(
            events_named(&run.root, "supervisor_superseded").is_empty(),
            "a stop is not a supersession: {}",
            run.events()
        );
        assert!(run.state().stopped, "the stop is recorded");
    }

    /// Whether the promotion wait covers a live launcher's serve-to-promotion latency:
    /// one probe's reply read, one contract-honouring write-lock holder ahead, and a 2 s
    /// allowance for the poll, the launcher's own closure and its fsyncs.
    fn promotion_margin_holds(probe: Duration, write_lock: Duration, promotion: Duration) -> bool {
        probe + write_lock + Duration::from_secs(2) <= promotion
    }

    /// D-15's margin: raising `IO_TIMEOUT` or `WRITE_LOCK_REPAIR_WAIT` by more than 1 s,
    /// or lowering the release wait below 14 s, fails here until the derivation in
    /// [`PROMOTION_WAIT_RELEASE`]'s doc is revisited.
    #[test]
    fn promotion_wait_covers_a_live_launchers_serve_to_promotion_latency() {
        let probe = devflow_core::supervisor::protocol::IO_TIMEOUT;
        assert!(promotion_margin_holds(
            probe,
            workflow::WRITE_LOCK_REPAIR_WAIT,
            PROMOTION_WAIT_RELEASE
        ));
        assert!(
            !promotion_margin_holds(
                probe,
                workflow::WRITE_LOCK_REPAIR_WAIT * 2,
                PROMOTION_WAIT_RELEASE
            ),
            "control: doubling the write-lock term breaks the margin"
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
            assert_eq!(
                last["group_signalled"], true,
                "run {label}: the successor's agent group was signalled for this stop: {last}"
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

    /// A supervisor's stop lands on the state another writer just changed: a write
    /// in progress when the stop path started is waited for and kept, because the
    /// stop re-reads under the write lock and writes only `stopped` and its reason.
    /// Today's load-and-save on the same interleaving is overwritten by that writer.
    #[test]
    fn record_supervisor_stop_commits_only_stopped_and_its_reason() {
        const LAUNCH_ID: &str = "thislaunch0000000000";
        const WRITER_HOLD: Duration = Duration::from_millis(200);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2401);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.supervisor = Some(SupervisorRecord::new(
            root.join("this-launch.sock"),
            LAUNCH_ID.to_string(),
            4_294_967_000,
            1,
            String::new(),
        ));
        workflow::save_state(&state).unwrap();

        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let writer_root = root.to_path_buf();
        let writer = std::thread::spawn(move || {
            workflow::update_state(&writer_root, phase, |on_disk| {
                held_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                on_disk.gate_notify_cmd = Some(": edited".to_string());
                Ok::<(), workflow::WorkflowError>(())
            })
            .unwrap();
        });
        held_rx.recv().unwrap();
        let stop_root = root.to_path_buf();
        let stop = std::thread::spawn(move || record_supervisor_stop(&stop_root, phase, LAUNCH_ID));
        std::thread::sleep(WRITER_HOLD);
        release_tx.send(()).unwrap();
        writer.join().unwrap();
        stop.join().unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert!(after.stopped, "the stop is recorded");
        assert_eq!(after.stop_reason.as_deref(), Some(SUPERVISOR_STOP_REASON));
        assert_eq!(
            after.gate_notify_cmd.as_deref(),
            Some(": edited"),
            "the other writer's field is kept"
        );
    }

    /// The other half of `record_supervisor_stop`'s delta: a state naming another
    /// launch is this supervisor's to leave alone, down to the byte.
    #[test]
    fn record_supervisor_stop_leaves_a_state_naming_another_launch_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2402);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.supervisor = Some(fabricated_prior_record(root));
        workflow::save_state(&state).unwrap();
        let path = workflow::state_path(root, phase);
        let before = std::fs::read(&path).unwrap();

        record_supervisor_stop(root, phase, "anotherlaunch0000000");

        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    /// The publish save writes the record and, when `publish_record` has set one, a
    /// stop. It never clears a `stop` another writer landed while the launch ran, and
    /// it keeps every other field.
    #[test]
    fn the_publish_save_commits_only_the_record_and_a_stop() {
        const CONCURRENT_STOP: &str = "stopped via `devflow stop`";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2403);
        let mut launcher = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&launcher).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.stopped = true;
            on_disk.stop_reason = Some(CONCURRENT_STOP.to_string());
            on_disk.gate_notify_cmd = Some(": edited".to_string());
            on_disk.pending_supervisor = Some(devflow_core::state::PendingSupervisor::new(
                fabricated_prior_record(root).launch_id,
                root.join("pending.sock"),
                root.join("pending.live"),
            ));
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        let record = fabricated_prior_record(root);
        launcher.supervisor = Some(record.clone());

        save_publish_fields(&launcher).unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.supervisor.as_ref(), Some(&record));
        assert!(after.stopped, "the concurrent stop is not cleared");
        assert_eq!(after.stop_reason.as_deref(), Some(CONCURRENT_STOP));
        assert_eq!(after.gate_notify_cmd.as_deref(), Some(": edited"));

        launcher.stop_reason = Some(NEXT_STOPPED_BEFORE_SAVE_REASON.to_string());

        save_successor_stop(&launcher).unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            after.stop_reason.as_deref(),
            Some(NEXT_STOPPED_BEFORE_SAVE_REASON),
            "publish_record's own stop is written"
        );
    }

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

        let err = advance_with(root, phase, Some(Stage::Define))
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
        enter_agent_free_child!(NAME, "claude");
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(2405);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        // The capture shape: no delivery canary.
        state.legacy_claude_launch = true;
        state.stopped = true;
        state.stop_reason = Some("stopped after code completed (--until code)".to_string());
        state.stop_until = Some(Stage::Code);
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.yes_ship = false;
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        let result = resume_loaded(root, phase, state, None, false, None);
        let reloaded_for_reap = workflow::load_state(root, phase).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert!(!after.yes_ship, "the revoke survives the resume");
        assert!(!after.stopped);
        assert_eq!(after.stop_reason, None);
        assert_eq!(after.stop_until, None);
        assert!(after.supervisor.is_some(), "the resume launched the stage");
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
            "devflow-core/src/recover.rs",
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

    // ---- 51-10: SUPV-01 criterion 3, the publish wait and the cron arm's read (D-12) ----

    /// How a withheld publish wait ends in a test build, as `wait_for_publish`'s
    /// `NotReady` text words it. The successor stays only pending, so its own
    /// promotion watch ends it, with its agent group, once `PROMOTION_WAIT` (5 s
    /// in a test build) expires, before the launcher's 10 s `PUBLISH_WAIT` does:
    /// the launcher then sees it exit by itself. In a release build the order is
    /// the other way round (`PROMOTION_WAIT_RELEASE` is 15 s), and the launcher's
    /// `end_unpublished_child` ends it ("did not publish within"), a path these
    /// tests therefore do not reach.
    const PUBLISH_WAIT_ENDED_IN_TEST_BUILDS: &str = "exited by itself before publishing";

    /// The test-only seam that withholds one publish wait's probe answer
    /// (`<agent>:<stage>`), read by `monitor::spawn_monitor` under the
    /// `test-support` gate.
    const WITHHOLD_PUBLISH_ENV: &str = "DEVFLOW_TEST_WITHHOLD_PUBLISH_PROBE";

    /// Where a group-recording stub writes `<pid> <pgid>`: the parent passes a
    /// path in a dir it owns, so the R2-13 positive control's numbers outlive
    /// the child (libtest discards a passing child's output).
    const GROUP_REPORT_ENV: &str = "P51_GROUP_REPORT";

    /// A stub for `program` that answers `--version`, then, on every other
    /// run, records `<pid> <pgid>` (read from `/proc/$$/stat` with shell
    /// builtins only) in `<program>-group` in its working directory, waits
    /// (bounded, about 60 s) for `release` there, prints the success marker
    /// and exits 0. `first_run_marker` set: the first run instead waits for
    /// `go` and prints that marker without recording itself (a Plan stage).
    fn group_recording_script(
        program: &str,
        release: &str,
        first_run_marker: Option<&str>,
    ) -> String {
        let first = first_run_marker.map_or_else(String::new, |marker| {
            format!(
                "if [ ! -e first-done ]; then\n  : > first-done\n  \
                 i=0\n  while [ ! -e go ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n  \
                 printf '%s\\n' '{marker}'\n  exit 0\nfi\n"
            )
        });
        format!(
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n{first}\
             read -r stat < /proc/$$/stat\nrest=${{stat##*) }}\nset -- $rest\n\
             printf '%s %s\\n' \"$$\" \"$3\" > {program}-group\n\
             i=0\nwhile [ ! -e {release} ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
             printf '%s\\n' '{SUCCESS_MARKER}'\nexit 0\n"
        )
    }

    /// The `(pid, pgid)` a [`group_recording_script`] stub recorded at `path`,
    /// once the whole line is there.
    fn recorded_group(path: &Path) -> Option<(u32, u32)> {
        let text = std::fs::read_to_string(path).ok()?;
        let line = text.strip_suffix('\n')?;
        let (pid, pgid) = line.split_once(' ')?;
        Some((pid.parse().ok()?, pgid.parse().ok()?))
    }

    /// R2-13's positive control: wait for the stub at `record` to record
    /// itself, then require its group to have a live member. Returns the
    /// recorded `(pid, pgid)` and writes them, with the live members, to the
    /// parent's report path. Fails rather than passing vacuously when the stub
    /// never recorded itself.
    fn agent_group_was_live(run: &HandoffRun, record: &Path) -> (u32, u32) {
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || recorded_group(record).is_some()),
            "the agent stub never recorded itself before the publish wait ended, so R2-13's \
             positive control cannot run; events:\n{}",
            run.events()
        );
        let (pid, pgid) = recorded_group(record).expect("polled above");
        let members = devflow_core::test_support::live_group_members(pgid);
        assert!(
            !members.is_empty(),
            "positive control: the agent's group {pgid} had no live member when it recorded \
             itself (pid {pid})"
        );
        if let Some(report) = std::env::var_os(GROUP_REPORT_ENV) {
            std::fs::write(report, format!("pid {pid} pgid {pgid} live {members:?}\n")).unwrap();
        }
        (pid, pgid)
    }

    /// R2-13: after the withheld publish, the agent group recorded at `pgid`
    /// has no live member within a bounded poll.
    fn agent_group_ended(run: &HandoffRun, pgid: u32) {
        assert!(
            poll_until(Duration::from_secs(10), || {
                devflow_core::test_support::live_group_members(pgid).is_empty()
            }),
            "the unpublished launch's agent group {pgid} outlived it: live members {:?}; \
             events:\n{}",
            devflow_core::test_support::live_group_members(pgid),
            run.events()
        );
    }

    /// The parent half of a 51-10 publish-wait test: run `name` with `stubs`,
    /// the seam set to `withhold`, and a report path for the R2-13 control;
    /// print the report (visible under `--no-capture`) and require it.
    fn run_withheld_publish_child(name: &str, stubs: &[(&str, String)], withhold: &str) {
        let report_dir = tempfile::Builder::new()
            .prefix("dfr")
            .tempdir_in("/tmp")
            .unwrap();
        let report = report_dir.path().join("group");
        run_handoff_child_with_stubs_and_env(
            name,
            None,
            SUCCESS_MARKER,
            stubs,
            &[
                (WITHHOLD_PUBLISH_ENV, std::ffi::OsStr::new(withhold)),
                (GROUP_REPORT_ENV, report.as_os_str()),
            ],
        );
        let recorded = std::fs::read_to_string(&report)
            .unwrap_or_else(|err| panic!("the child wrote no R2-13 report: {err}"));
        println!("{name}: R2-13 positive control: {}", recorded.trim());
    }

    /// D-12 (SUPV-01 criterion 3, the failover arm's publish wait), with R2-D1
    /// and R2-13. The primary Claude Code stage is rate-limited and hands off
    /// to the unused Codex fallback; the seam withholds the probe answer of
    /// the fallback's Code relaunch only (`codex:code`), so its one publish
    /// wait ends unpublished (in a test build, by the successor's own promotion
    /// expiry: see [`PUBLISH_WAIT_ENDED_IN_TEST_BUILDS`]). The hop then ends
    /// visibly: `agent_handoff_relaunch_failed` naming the publish wait's
    /// `NotReady`, then the never-silent Code gate, with no cron arm. Nothing is
    /// orphaned: the fallback stub's group had a live member when it recorded
    /// itself (positive control) and has none afterwards.
    /// Healthy baseline (no intervention, same rig):
    /// [`a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage`].
    #[test]
    fn failover_with_the_publish_probe_withheld_ends_at_a_gate() {
        const NAME: &str =
            "pipeline_launch::tests::failover_with_the_publish_probe_withheld_ends_at_a_gate";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_withheld_publish_child(
                NAME,
                &[
                    ("claude", rate_limited_claude_script()),
                    (
                        "codex",
                        group_recording_script("codex", "never-released", None),
                    ),
                ],
                "codex:code",
            );
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(5195), |_, state| {
            state.stage = Stage::Code;
            state.stop_until = Some(Stage::Code);
            state.fallback_agent = Some(AgentKind::Codex);
        });
        run.go();
        let (_, pgid) = agent_group_was_live(&run, &run.root.join("codex-group"));
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                events_named(&run.root, "gate_fired")
                    .iter()
                    .any(|gate| gate["stage"] == "code")
            }),
            "no Code gate fired; events:\n{}",
            run.events()
        );

        let events = all_events(&run.root);
        let position = |name: &str| events.iter().position(|e| e["event"] == name);
        let handed = position("agent_handoff").expect("the handoff committed");
        let failed = position("agent_handoff_relaunch_failed")
            .unwrap_or_else(|| panic!("no relaunch-failure event; events:\n{}", run.events()));
        let gated = position("gate_fired").expect("polled above");
        assert!(
            handed < failed && failed < gated,
            "handoff, then the failure, then the gate; events:\n{}",
            run.events()
        );
        let handoff = &events[handed];
        assert_eq!(handoff["stage"], "code", "{handoff}");
        assert_eq!(handoff["to_agent"], "codex", "{handoff}");
        assert_eq!(handoff["reason"], "rate_limited", "{handoff}");
        let failure = &events[failed];
        assert_eq!(failure["stage"], "code", "{failure}");
        assert_eq!(failure["to_agent"], "codex", "{failure}");
        assert!(
            failure["reason"].as_str().is_some_and(|reason| {
                reason.contains("supervision shape not ready")
                    && reason.contains(PUBLISH_WAIT_ENDED_IN_TEST_BUILDS)
            }),
            "the failure names the publish wait's NotReady end: {failure}"
        );
        assert!(
            events_named(&run.root, "rate_limit_resume_scheduled").is_empty(),
            "a failed relaunch never takes the cron arm (R2-D1); events:\n{}",
            run.events()
        );
        assert!(
            !devflow_core::ship::cron_instructions_path(&run.root, run.phase).exists(),
            "a failed relaunch writes no cron instruction (R2-D1)"
        );
        assert_eq!(
            run.other_sockets(),
            Vec::<PathBuf>::new(),
            "no Codex supervisor socket remains"
        );

        let state = run.state();
        assert_eq!(state.agent, AgentKind::Codex);
        assert!(state.fallback_consumed);
        assert!(state.gate_pending, "the never-silent gate is open");

        agent_group_ended(&run, pgid);
    }

    /// D-12's ordinary-launch control for the publish wait, observable only
    /// (R2-9). An ordinary Plan → Code transition, no fallback, with the same
    /// seam withholding the Code launch's probe (`claude:code`): Plan completes
    /// and transitions, and no Code successor is published. What the run
    /// leaves: the stage committed as Code, no supervisor record (WR-04's clear
    /// after the failed launch), no socket but stage N's, no `stage_launched`
    /// for `code`, and no live member in the Code agent's group (which had one
    /// when it recorded itself). No cause text is asserted: an in-supervisor
    /// launch failure on the ordinary path writes no event naming its cause.
    /// Both withheld launches are Code launches, so this and
    /// [`failover_with_the_publish_probe_withheld_ends_at_a_gate`] differ only
    /// in failover.
    #[test]
    fn an_ordinary_transition_with_the_publish_probe_withheld_fails_the_same_way() {
        const NAME: &str = "pipeline_launch::tests::an_ordinary_transition_with_the_publish_probe_withheld_fails_the_same_way";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_withheld_publish_child(
                NAME,
                &[(
                    "claude",
                    group_recording_script("claude", "go2", Some(SUCCESS_MARKER)),
                )],
                "claude:code",
            );
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(5196), |_, state| {
            state.stage = Stage::Plan;
            state.stop_until = Some(Stage::Code);
        });
        run.go();
        let (_, pgid) = agent_group_was_live(&run, &run.root.join("claude-group"));
        let settled = || {
            workflow::load_state(&run.root, run.phase)
                .is_ok_and(|state| state.stage == Stage::Code && state.supervisor.is_none())
                && matches!(
                    devflow_core::supervisor::probe(&run.n.socket_path),
                    Ok(devflow_core::supervisor::Liveness::Gone)
                )
        };
        assert!(
            poll_until(HANDOFF_STEP_WAIT, settled),
            "the run did not settle at Code with no supervisor record and stage N gone; \
             state: {:?}; events:\n{}",
            workflow::load_state(&run.root, run.phase).map(|s| (s.stage, s.supervisor)),
            run.events()
        );

        let state = run.state();
        assert_eq!(state.stage, Stage::Code, "the transition committed Code");
        assert_eq!(
            state.supervisor, None,
            "WR-04: the failed launch's record is cleared"
        );
        assert_eq!(
            run.other_sockets(),
            Vec::<PathBuf>::new(),
            "no Code supervisor socket remains"
        );
        assert!(
            events_named(&run.root, "stage_launched")
                .iter()
                .all(|launched| launched["stage"] != "code"),
            "no Code successor was published; events:\n{}",
            run.events()
        );
        agent_group_ended(&run, pgid);
    }

    /// A socket path `supervisor::probe` cannot answer for: its parent is a
    /// regular file, so the probe's `stat` fails with `ENOTDIR` (an error, not
    /// GONE or STALE).
    fn unprobeable_socket(dir: &Path) -> PathBuf {
        let parent = dir.join("not-a-dir");
        std::fs::write(&parent, "").unwrap();
        parent.join("supervisor.sock")
    }

    /// A Code-stage run of `phase` under Claude on the capture shape, in a fresh repo at
    /// `root`, whose state records a supervisor at `socket`.
    fn code_run_recording(root: &Path, phase: PhaseId, socket: &Path) -> State {
        init_repo(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        state.legacy_claude_launch = true;
        state.supervisor = Some(devflow_core::state::SupervisorRecord::new(
            socket.to_path_buf(),
            "18e5e63a18d5674f".to_string(),
            u32::MAX,
            0,
            String::new(),
        ));
        state
    }

    /// The state the rate-limit cron arm leaves: not stopped, one infra fault
    /// charged, a cron instruction to `devflow resume` it.
    fn save_rate_limit_paused(root: &Path, mut state: State) {
        state.infra_failures = 1;
        workflow::save_state(&state).unwrap();
        let record = devflow_core::ship::build_single_agent_cron_instructions(
            root,
            state.phase,
            "2026-06-18T15:45:30Z",
        );
        devflow_core::ship::write_cron_instructions(root, &record).unwrap();
    }

    /// D-12 (the cron arm's recorded-socket read, withheld), with its
    /// ordinary-resume control and healthy baseline. Non-vacuity first: the
    /// fixture's recorded socket makes `supervisor::probe` return an error.
    /// (b) A rate-limit-paused run (the cron arm's later `devflow resume`)
    /// recording that socket is refused, naming it, and nothing is written.
    /// (c) Control: an ordinary `resume` of a `--until`-stopped run recording
    /// the same socket is refused with the same text. (a) Baseline: a paused
    /// run whose recorded socket is GONE resumes and launches.
    #[test]
    fn a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_recorded_probe_cannot_answer() {
        const NAME: &str = "pipeline_launch::tests::a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_recorded_probe_cannot_answer";
        if parent_of_a_claude_and_codex_child(NAME, &[]) {
            return;
        }
        let _guard = env_lock();
        let sockets = tempfile::tempdir().unwrap();
        let socket = unprobeable_socket(sockets.path());
        let probed = devflow_core::supervisor::probe(&socket);
        assert!(
            probed.is_err(),
            "non-vacuity: the fixture's recorded socket must make the probe fail, got {probed:?}"
        );

        let refused = |phase: PhaseId, save: &dyn Fn(&Path, State)| -> String {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            save(root, code_run_recording(root, phase, &socket));
            let before = std::fs::read(workflow::state_path(root, phase)).unwrap();
            let result = resume(root, phase, None, false);
            let err = match result {
                Err(err) => err.to_string(),
                Ok(()) => panic!("phase {phase}: resume over an unprobeable record launched"),
            };
            assert_eq!(
                std::fs::read(workflow::state_path(root, phase)).unwrap(),
                before,
                "phase {phase}: a refused resume writes nothing"
            );
            err
        };

        let paused = PhaseId::new(5197);
        let cron = refused(paused, &|root, state| save_rate_limit_paused(root, state));
        assert!(
            cron.contains("refusing to resume")
                && cron.contains(&socket.display().to_string())
                && cron.contains("probe failed"),
            "the cron arm's resume refuses naming the socket and its probe failure: {cron}"
        );

        let stopped = PhaseId::new(5198);
        let ordinary = refused(stopped, &|_, mut state| {
            state.stop_until = Some(Stage::Code);
            state.stopped = true;
            state.stop_reason = Some("stopped after code completed (--until code)".to_string());
            workflow::save_state(&state).unwrap();
        });
        assert_eq!(
            ordinary.replace(&stopped.to_string(), "N"),
            cron.replace(&paused.to_string(), "N"),
            "an ordinary resume refuses exactly as the cron arm's does"
        );

        let baseline = PhaseId::new(5199);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let gone = sockets.path().join("gone.sock");
        save_rate_limit_paused(root, code_run_recording(root, baseline, &gone));
        let result = resume(root, baseline, None, false);
        let reloaded_for_reap = workflow::load_state(root, baseline).ok();
        let _reap_guard = reloaded_for_reap
            .as_ref()
            .map(ReapMonitorOnDrop::after_launch);
        result.unwrap_or_else(|err| panic!("baseline: a GONE recorded socket resumes: {err}"));
        let reloaded = workflow::load_state(root, baseline).unwrap();
        assert!(
            reloaded
                .supervisor
                .as_ref()
                .is_some_and(|record| record.socket_path != gone),
            "baseline: the resume recorded its own supervisor: {:?}",
            reloaded.supervisor
        );
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

    /// Every violation of the SUPV-01 evidence file's row contract in `text`,
    /// with cited files and tests resolved against `root` (R2-10). Trace mode
    /// (`complete == false`) accepts the pending marker `51-13`; complete mode
    /// (51-13) rejects it and requires the two conclusion lines. Never panics on
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
        assert_eq!(violations.len(), 7, "(b) complete mode: {violations:#?}");
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
        let complete_text = render(&planted_complete_rows(), PLANTED_CONCLUSION);
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
            &check(&render(&planted_complete_rows(), no_path), true),
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
        let fabricated = render(&fabricated, fabricated_conclusion);
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

    // ---- 51-13: the post-save successor probe answered Gone (D-12, B-2, R2-8) ----

    /// The test-only seam that answers [`publish_record`]'s post-save probe `Gone`
    /// (`<agent>:<stage>:<marker-path>`), read by `successor_probe_for` under `cfg(test)`.
    const WITHHOLD_SUCCESSOR_ENV: &str = "DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE";

    /// The parent half of a 51-13 post-save probe test: run `name` with `stubs` and the
    /// seam naming `agent_stage` and a marker path in a parent-owned dir; the marker is
    /// the seam's proof that it answered. `None` runs the same child with the seam
    /// unset (the intervention-removed run).
    fn run_withheld_successor_child(
        name: &str,
        stubs: &[(&str, String)],
        agent_stage: Option<&str>,
    ) {
        let marker_dir = tempfile::Builder::new()
            .prefix("dfm")
            .tempdir_in("/tmp")
            .unwrap();
        let marker = marker_dir.path().join("withheld");
        let value = agent_stage.map(|agent_stage| {
            std::ffi::OsString::from(format!("{agent_stage}:{}", marker.display()))
        });
        let env: Vec<(&str, &std::ffi::OsStr)> = value
            .as_ref()
            .map(|value| (WITHHOLD_SUCCESSOR_ENV, value.as_os_str()))
            .into_iter()
            .collect();
        run_handoff_child_with_stubs_and_env(name, None, SUCCESS_MARKER, stubs, &env);
    }

    /// The marker path the seam variable names in this child, if it is set.
    fn withheld_successor_marker() -> Option<PathBuf> {
        let value = std::env::var(WITHHOLD_SUCCESSOR_ENV).ok()?;
        let mut parts = value.splitn(3, ':');
        let (_, _, marker) = (parts.next()?, parts.next()?, parts.next()?);
        Some(PathBuf::from(marker))
    }

    /// Poll until the run's state reads `stopped`, failing with the events otherwise.
    fn wait_recorded_stopped(run: &HandoffRun) -> State {
        assert!(
            poll_until(HANDOFF_STEP_WAIT, || {
                workflow::load_state(&run.root, run.phase).is_ok_and(|state| state.stopped)
            }),
            "the run was never recorded stopped; state: {:?}; events:\n{}",
            workflow::load_state(&run.root, run.phase).map(|s| (
                s.stage,
                s.agent,
                s.stopped,
                s.stop_reason,
                s.supervisor
            )),
            run.events()
        );
        run.state()
    }

    /// The `stopped` shape both post-save experiments assert while their successor is
    /// still held: the stop reason, a saved record naming the successor (not stage N),
    /// the seam's marker and a live successor socket.
    fn assert_recorded_stopped_while_the_successor_runs(run: &HandoffRun, state: &State) {
        assert_eq!(
            state.stop_reason.as_deref(),
            Some(NEXT_STOPPED_BEFORE_SAVE_REASON),
            "events:\n{}",
            run.events()
        );
        let record = state
            .supervisor
            .as_ref()
            .expect("the successor's record was saved before the probe");
        assert_ne!(
            record.launch_id, run.n.launch_id,
            "the record names the successor, not stage N"
        );
        let marker = withheld_successor_marker().expect("this child runs with the seam set");
        assert!(
            marker.exists(),
            "the seam's marker {} is missing: the seam never answered",
            marker.display()
        );
        assert!(
            !run.other_sockets().is_empty(),
            "the successor the state calls stopped is still running, so its socket is there"
        );
    }

    /// D-12 (SUPV-01 criterion 3, failover's post-save probe; review B-2, R2-8). The
    /// primary Claude Code stage is rate-limited and hands off to the unused Codex
    /// fallback; the seam answers the post-save probe of the fallback's Code relaunch
    /// only (`codex:code`) `Gone`, the branch that flips the run's shape. While the
    /// fallback's agent is still held on `go-fallback`, the state records the run
    /// `stopped` with [`NEXT_STOPPED_BEFORE_SAVE_REASON`] under the successor's saved
    /// record, after one rate-limited handoff, with no relaunch failure, no gate and no
    /// cron arm. Healthy baseline (no intervention, same rig):
    /// [`a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage`].
    #[test]
    fn failover_with_the_post_save_probe_answering_gone_records_the_run_stopped() {
        const NAME: &str = "pipeline_launch::tests::failover_with_the_post_save_probe_answering_gone_records_the_run_stopped";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_withheld_successor_child(
                NAME,
                &[
                    ("claude", rate_limited_claude_script()),
                    (
                        "codex",
                        group_recording_script("codex", "go-fallback", None),
                    ),
                ],
                Some("codex:code"),
            );
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(5131), |_, state| {
            state.stage = Stage::Code;
            state.stop_until = Some(Stage::Code);
            state.fallback_agent = Some(AgentKind::Codex);
        });
        run.go();
        let state = wait_recorded_stopped(&run);

        assert_recorded_stopped_while_the_successor_runs(&run, &state);
        assert_eq!(state.agent, AgentKind::Codex, "the fallback drives the run");
        assert!(state.fallback_consumed, "the hop is consumed");
        let handoffs = events_named(&run.root, "agent_handoff");
        assert_eq!(handoffs.len(), 1, "one handoff; events:\n{}", run.events());
        assert_eq!(handoffs[0]["reason"], "rate_limited", "{}", handoffs[0]);
        for absent in [
            "agent_handoff_relaunch_failed",
            "gate_fired",
            "rate_limit_resume_scheduled",
        ] {
            assert!(
                events_named(&run.root, absent).is_empty(),
                "no `{absent}`; events:\n{}",
                run.events()
            );
        }
        assert!(
            !devflow_core::ship::cron_instructions_path(&run.root, run.phase).exists(),
            "no cron instruction"
        );
        std::fs::write(run.root.join("go-fallback"), "").unwrap();
    }

    /// D-12's ordinary-launch control for the post-save probe: an ordinary Plan → Code
    /// transition, no fallback, with the same seam answering its Code launch's
    /// post-save probe `Gone` (`claude:code`). While the Code agent is held on `go2`,
    /// the run is recorded stopped the same way, with the same reason, at stage Code.
    /// Both answered launches are Code launches, so this and
    /// [`failover_with_the_post_save_probe_answering_gone_records_the_run_stopped`]
    /// differ only in failover.
    #[test]
    fn an_ordinary_transition_with_the_post_save_probe_answering_gone_records_the_run_stopped() {
        const NAME: &str = "pipeline_launch::tests::an_ordinary_transition_with_the_post_save_probe_answering_gone_records_the_run_stopped";
        if !devflow_core::test_support::in_child_test(NAME) {
            run_withheld_successor_child(NAME, &[], Some("claude:code"));
            return;
        }
        let _guard = env_lock();
        let run = HandoffRun::launch_with(PhaseId::new(5132), |_, state| {
            state.stage = Stage::Plan;
            state.stop_until = Some(Stage::Code);
        });
        run.go();
        let state = wait_recorded_stopped(&run);

        assert_recorded_stopped_while_the_successor_runs(&run, &state);
        assert_eq!(state.stage, Stage::Code, "the transition committed Code");
        std::fs::write(run.root.join("go2"), "").unwrap();
    }

    const SUCCESSOR_SEAM_NO_LITERAL_READ: &str =
        "no literal std::env::var read of DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE";
    const SUCCESSOR_SEAM_FN_NOT_ONCE: &str = "fn successor_probe_for( is not defined exactly once";
    const SUCCESSOR_SEAM_NOT_GATED: &str =
        "fn successor_probe_for( is not directly under #[cfg(test)]";
    const SUCCESSOR_SEAM_CALL_NOT_GATED: &str =
        "a call to successor_probe_for( is not directly under #[cfg(test)]";
    const SUCCESSOR_SEAM_READ_OUTSIDE: &str =
        "DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE is named outside fn successor_probe_for(";

    /// The post-save probe seam's layout over `source`'s non-test code (cut at its
    /// first `mod tests {` line, whole-line `//` comments dropped): the variable is read
    /// literally (the form the doc check sees) and only in the seam's body, the seam is
    /// defined once directly under `#[cfg(test)]`, and every call is directly under
    /// `#[cfg(test)]` too, so the release binding names neither.
    fn successor_probe_seam_violations(source: &str) -> Vec<String> {
        const GATE: &str = "#[cfg(test)]";
        const DEFINITION: &str = "fn successor_probe_for(";
        const VARIABLE: &str = "DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE";
        let lines: Vec<&str> = source
            .lines()
            .take_while(|line| *line != "mod tests {")
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        let gated = |at: usize| {
            at.checked_sub(1)
                .is_some_and(|above| lines[above].trim() == GATE)
        };
        let mut violations = Vec::new();
        if !lines
            .iter()
            .any(|line| line.contains(&format!("std::env::var(\"{VARIABLE}\")")))
        {
            violations.push(SUCCESSOR_SEAM_NO_LITERAL_READ.to_string());
        }
        let defined: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.trim_start().starts_with(DEFINITION))
            .map(|(at, _)| at)
            .collect();
        match defined.as_slice() {
            [at] => {
                if !gated(*at) {
                    violations.push(SUCCESSOR_SEAM_NOT_GATED.to_string());
                }
                // The body runs to the first line that is exactly `}`.
                let end = lines[*at..]
                    .iter()
                    .position(|line| *line == "}")
                    .map_or(lines.len(), |offset| at + offset);
                let outside = lines
                    .iter()
                    .enumerate()
                    .any(|(index, line)| line.contains(VARIABLE) && !(*at..=end).contains(&index));
                if outside {
                    violations.push(SUCCESSOR_SEAM_READ_OUTSIDE.to_string());
                }
            }
            _ => violations.push(SUCCESSOR_SEAM_FN_NOT_ONCE.to_string()),
        }
        let ungated_call = lines.iter().enumerate().any(|(at, line)| {
            line.contains("successor_probe_for(")
                && !line.trim_start().starts_with(DEFINITION)
                && !gated(at)
        });
        if ungated_call {
            violations.push(SUCCESSOR_SEAM_CALL_NOT_GATED.to_string());
        }
        violations
    }

    /// T-51-37 (51-13, permanent source scan): a release build never compiles the
    /// post-save probe seam or its call, so no environment variable can make a release
    /// launcher read a successor as gone. The real file yields no violation; each
    /// planted source yields the violation it plants, including an ungated decoy call
    /// and an ungated decoy read.
    #[test]
    fn the_successor_probe_seam_is_compiled_out_of_non_test_builds() {
        let gate = "#[cfg(test)]\n";
        let read = "    let _ = std::env::var(\"DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE\");\n";
        let seam = |attribute: &str, body: &str| {
            format!("{attribute}fn successor_probe_for(state: &State) -> Probe {{\n{body}}}\n")
        };
        let caller = |attribute: &str| {
            format!(
                "fn publish_record() {{\n    // Test-only.\n    #[cfg(not(test))]\n    let probe = \
                 devflow_core::supervisor::probe;\n    {attribute}    let probe = \
                 successor_probe_for(state);\n}}\n"
            )
        };
        let gated_call = format!("    {gate}");
        let good = format!("{}{}", seam(gate, read), caller(&gated_call));
        let only = |violation: &str| vec![violation.to_string()];

        assert_eq!(
            successor_probe_seam_violations(&good),
            Vec::<String>::new(),
            "a correct layout (the comment line above the gate is dropped)"
        );
        assert_eq!(
            successor_probe_seam_violations(&format!("{}{}", seam("", read), caller(&gated_call))),
            only(SUCCESSOR_SEAM_NOT_GATED),
            "an ungated definition"
        );
        assert_eq!(
            successor_probe_seam_violations(&format!("{}{}", seam(gate, read), caller(""))),
            only(SUCCESSOR_SEAM_CALL_NOT_GATED),
            "an ungated decoy call (a release binding naming the seam fn)"
        );
        assert_eq!(
            successor_probe_seam_violations(&format!(
                "{}{}fn decoy() {{\n{read}}}\n",
                seam(gate, read),
                caller(&gated_call)
            )),
            only(SUCCESSOR_SEAM_READ_OUTSIDE),
            "an ungated decoy reading the variable outside the seam"
        );
        assert_eq!(
            successor_probe_seam_violations(&format!(
                "{}{}",
                seam(gate, "    let _ = std::env::var(SEAM);\n"),
                caller(&gated_call)
            )),
            only(SUCCESSOR_SEAM_NO_LITERAL_READ),
            "a read through a constant"
        );
        assert_eq!(
            successor_probe_seam_violations(&format!(
                "{}{}{}",
                seam(gate, read),
                seam(gate, ""),
                caller(&gated_call)
            )),
            only(SUCCESSOR_SEAM_FN_NOT_ONCE),
            "two definitions"
        );
        assert_eq!(
            successor_probe_seam_violations(&format!("mod tests {{\n{good}")),
            vec![
                SUCCESSOR_SEAM_NO_LITERAL_READ.to_string(),
                SUCCESSOR_SEAM_FN_NOT_ONCE.to_string()
            ],
            "the read and the seam only after the test-module cut"
        );

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline_launch.rs");
        let source =
            std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path}: {err}"));
        assert!(
            source.lines().any(|line| line == "mod tests {"),
            "pipeline_launch.rs must keep its `mod tests {{` line, or the scan would read its \
             own controls"
        );
        let real = successor_probe_seam_violations(&source);
        assert!(real.is_empty(), "{path}: {real:?}");
    }
}
