//! Pipeline seam C (D-06): stage transitions, gate firing and resolution,
//! loop-backs, workflow completion, and abort. Extracted mechanically
//! (19-08, D-09 pure move) out of `main.rs` — every function below is
//! byte-identical to its pre-move body modulo an added `pub(crate)` and
//! adjusted `use` paths.
//!
//! **This module closes the pipeline's three-way module cycle
//! (19-RESEARCH.md Pattern 1):** [`transition`] and [`loop_back_to_code`]
//! both call [`crate::pipeline_launch::launch_stage`] at their final step —
//! that call is what closes the cycle `pipeline_launch (advance) →
//! pipeline_outcomes (handle_*_outcome) → pipeline_gate (transition/
//! run_gate/finish_workflow) → pipeline_launch (launch_stage)` back to
//! where it started. This cycle is the state machine's real control flow
//! (Code → Validate → Ship, with loop-backs), and Rust permits cyclic
//! module references — only the crate dependency graph must be acyclic —
//! so this compiles cleanly. **A future change to pipeline logic is likely
//! to touch two or three of these files together:** the split buys
//! `pub(crate)` boundaries, reviewability, and wave independence for the
//! *other* clusters, not pipeline-internal parallelism (19-RESEARCH.md
//! Pitfall 1).

use crate::CliError;
use crate::config_parse::{foreground_gate_timeout_secs, gate_timeout_secs};
use crate::pipeline_launch::{
    GateWaitEnd, clear_run_state, close_gate, commit, ensure_still_recorded, fenced, is_not_owner,
    launch_stage,
};
use crate::pipeline_outcomes::{run_checkout_hooks, truncate_reason};
use devflow_core::gates::{
    self, AnswerRead, Claim, GateAction, GateError, GateId, GateResponse, Gates,
};
use devflow_core::hooks;
use devflow_core::mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt::FixType;
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::{events, lock, registry, ship, workflow};
use std::path::Path;
use tracing::info;

/// Fire the hooks for `from → to`, persist the new stage, and launch its agent.
///
/// `infra_failures` resets unconditionally on every successful transition
/// (CR-01, 17-06 gap closure). Without this, an infra-fault ceiling meant to
/// bound a *stuck loop* (D-08, [`mode::MAX_INFRA_FAILURES`]) instead
/// accumulates across a phase's entire lifetime — several well-spaced,
/// cleanly-resolved infra faults would falsely reach the ceiling and
/// hard-abort a long-running but otherwise healthy phase.
///
/// `consecutive_failures` clears on every transition EXCEPT Code→Validate
/// (18d, [`mode::transition_resets_consecutive_failures`]): that hop is
/// crossed on every single Code↔Validate retry cycle, so unconditionally
/// clearing it there made [`mode::MAX_CONSECUTIVE_FAILURES`] unreachable for
/// the exact loop it bounds. The two counters deliberately no longer share a
/// single reset condition.
pub(crate) fn transition(
    project_root: &Path,
    state: &mut State,
    to: Stage,
) -> Result<(), CliError> {
    let from = state.stage;

    // 20c: `devflow start --until <stage>` halts cleanly once the requested
    // stage has completed — checked here, at the TOP, before anything else
    // runs. `stop_until == Some(from)` means the JUST-COMPLETED stage (the
    // one whose outcome triggered this call) is the requested stop point;
    // checking `to` instead would halt BEFORE the target stage ever ran
    // (review: Codex HIGH off-by-one). This bypasses the from→to checkout
    // hooks, `state.stage = to`, the normal `"transition"` event, and
    // `launch_stage` — no new monitor is spawned, and `loop_back_to_code`
    // (a retry, not an advance) is untouched by this check.
    if state.stop_until == Some(from) {
        commit(state, |s| {
            s.stopped = true;
            s.stop_reason = Some(format!("stopped after {from} completed (--until {from})"));
            // T-20-03b / doctor gap (D-09): clear the supervisor record and
            // gate_pending so neither check_dead_agent nor check_dead_monitor
            // misreports this intentional stop as a crashed agent or dead
            // monitor (50-13): the supervisor running this advance finishes
            // once it returns, so the record would otherwise name a socket
            // that is gone.
            s.supervisor = None;
            s.gate_pending = false;
        })?;
        events::emit(
            project_root,
            state.phase,
            "workflow_finished",
            serde_json::json!({
                "reason": "stopped_at",
                "stage": from.to_string(),
            }),
        );
        return Ok(());
    }

    ensure_still_recorded(project_root, state.phase)?;
    let _ = run_checkout_hooks(
        project_root,
        state,
        &hooks::hooks_for_transition(from, to),
        to,
    );
    commit(state, |s| {
        s.stage = to;
        if mode::transition_resets_consecutive_failures(from, to) {
            s.consecutive_failures = 0;
        }
        s.infra_failures = 0;
        s.gate_pending = false;
    })?;
    events::emit(
        project_root,
        state.phase,
        "transition",
        serde_json::json!({
            "from": from.to_string(),
            "to": to.to_string(),
        }),
    );
    launch_stage(state, None, Some(from))
}

/// Why the pipeline is looping back to Code, recorded on the `loop_back`
/// event so `events.jsonl` distinguishes cases that are indistinguishable
/// from the counters alone (IN-02, 999.78).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopBackReason {
    /// A Validate failure recorded against an existing commit-count baseline
    /// — the ordinary case.
    ValidateFailure,
    /// A Validate failure recorded while NO commit-count baseline existed for
    /// this phase (IN-02). `last_validate_failure_commit_count`'s `None` means
    /// both "genuine first failure of this phase" and "state written by a
    /// binary predating that field", and nothing in `events.jsonl` told the
    /// two apart — so an operator who upgraded a binary mid-phase got no
    /// signal that the failure budget had widened back to its full width.
    /// This reason is that signal.
    ValidateFailureNoBaseline,
    /// A human answered a gate with a loop-back — a Ship `review:` rejection,
    /// an ambiguous Validate adjudication, or a finalization retry. Makes no
    /// claim about the commit-count baseline, because none was consulted.
    GateResponse,
}

impl LoopBackReason {
    /// The stable string recorded on the `loop_back` event. Distinct per
    /// variant by construction: an operator greps these, so two variants
    /// sharing a string would silently re-merge the cases IN-02 separated.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LoopBackReason::ValidateFailure => "validate_failure",
            LoopBackReason::ValidateFailureNoBaseline => "validate_failure_no_commit_baseline",
            LoopBackReason::GateResponse => "gate_response",
        }
    }
}

/// Loop the pipeline back to Code with the given fix prompt (`GapsOnly` for a
/// Validate rejection, `AuditFix` for a Ship `review:` rejection).
pub(crate) fn loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
    reason: LoopBackReason,
) -> Result<(), CliError> {
    let from = state.stage;
    let prompt = prepare_loop_back_to_code(project_root, state, fix, reason)?;
    launch_stage(state, Some(prompt), Some(from))
}

/// The state-mutating half of `loop_back_to_code`, split out so it's
/// unit-testable without spawning a real agent process (`launch_stage`
/// invokes the actual configured agent CLI). Cleans up the stale gate for
/// the stage the gate fired on (CR-01), moves `state` to Code, persists it,
/// and returns the fix prompt the caller should launch with.
pub(crate) fn prepare_loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
    reason: LoopBackReason,
) -> Result<String, CliError> {
    // Capture the stage the gate actually fired on before it's mutated below,
    // so the close targets the right stage's gate files (see CR-01).
    let gate_stage = state.stage;
    let _ = close_gate(project_root, state.phase, gate_stage);
    // F-11 (`select_loop_back_fix`): this is the write that persists the
    // verification observation that function records in the caller's copy, one
    // write per loop-back. Nothing else writes those two fields during an advance.
    let observed_fingerprint = state.last_verification_fingerprint;
    let observed_mtime_nanos = state.last_verification_mtime_nanos;
    commit(state, |s| {
        s.stage = Stage::Code;
        s.gate_pending = false;
        s.last_verification_fingerprint = observed_fingerprint;
        s.last_verification_mtime_nanos = observed_mtime_nanos;
    })?;
    events::emit(
        project_root,
        state.phase,
        "loop_back",
        serde_json::json!({
            "from": gate_stage.to_string(),
            "consecutive_failures": state.consecutive_failures,
            "phase_validate_failures": state.phase_validate_failures,
            "reason": reason.as_str(),
            "fix": format!("{fix:?}"),
        }),
    );
    println!(
        "looping back to Code ({} validate failure(s) this phase, {} in the current streak)",
        state.phase_validate_failures, state.consecutive_failures
    );
    Ok(devflow_core::agents::driver_for(state.agent).render_prompt(
        &devflow_core::prompt::StageIntent::Code {
            phase: state.phase,
            fix: Some(fix),
        },
    ))
}

/// Run the terminal hooks (version bump + branch cleanup) and clear state.
///
/// Uses [`gate_timeout_secs`]'s multi-day production default for the
/// retry-gate wait below — safe for every caller EXCEPT the foreground
/// `ship_override` path (WR-02), which calls
/// [`finish_workflow_with_gate_timeout`] directly with a bounded timeout
/// instead.
pub(crate) fn finish_workflow(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    finish_workflow_with_gate_timeout(project_root, state, gate_timeout_secs())
}

/// `finish_workflow`'s body, parameterized on how long the retry-gate poll
/// (below) waits for a response before failing fast (WR-02, phase 20
/// review). Every caller reached through a detached monitor process should
/// keep using `finish_workflow`'s multi-day default — invisible to an
/// operator's terminal by construction. `ship_override` is the one caller
/// invoked directly from the foreground CLI, so it passes
/// [`crate::config_parse::foreground_gate_timeout_secs`] here instead,
/// bounding how long a terminal-hook failure can block the operator's shell
/// without weakening the fail-closed terminal-Ship invariant: an unanswered
/// gate still fails the operation entirely (via `run_gate_with_timeout`'s
/// existing timeout error), just after seconds instead of days.
pub(crate) fn finish_workflow_with_gate_timeout(
    project_root: &Path,
    state: &mut State,
    gate_timeout_secs: u64,
) -> Result<(), CliError> {
    #[cfg(test)]
    crate::pipeline_launch::test_hold("before_finalization");
    // Safe point 6 (CR-01, 50-REVIEW.md): a stop taken during the advance
    // after the Ship gate was answered is honoured before the merge, version
    // bump, tag and push. The consumed approval is cleared with it, so a
    // resumed run asks again. Outside a supervisor's advance (the foreground
    // `ship_override`) no stop token is engaged, and the holder's stop flag decides
    // (D-18). Once this has passed, the pass runs every hook to the end.
    if crate::pipeline_launch::stop_requested_at_safe_point() {
        let _ = close_gate(project_root, state.phase, Stage::Ship);
        crate::pipeline_launch::record_advance_stop(state, 6)?;
        return Err(CliError::Message(format!(
            "finalization did not run: {}",
            crate::pipeline_launch::safe_point_stop_reason()
        )));
    }
    loop {
        ensure_still_recorded(project_root, state.phase)?;
        if run_checkout_hooks(project_root, state, &hooks::hooks_after_ship(), Stage::Ship) {
            break;
        }
        // The original Ship approval has already been consumed. Reopen an
        // actionable gate and keep this monitor waiting so a terminal-hook
        // failure cannot turn into an invisible stalled Ship state.
        let _ = close_gate(project_root, state.phase, Stage::Ship);
        let context = format!(
            "[finalization failed] phase {} terminal hooks did not complete. Resolve the git/version error, then approve to retry; reject to loop back or abort.",
            state.phase
        );
        // This is the reopened finalization-retry gate, NEVER the routine
        // Ship approval — it must always pass `None` here. Auto-approving
        // it would mean "the merge could not be completed" gets silently
        // retried forever with no human ever seeing it (T-23-91). The one
        // and only site permitted to pass a non-`None` auto-response is
        // `handle_ship_outcome` in `pipeline_outcomes.rs`.
        match run_gate_with_timeout(
            project_root,
            state,
            Stage::Ship,
            &context,
            gate_timeout_secs,
            None,
        )? {
            GateAction::Advance => {
                let _ = close_gate(project_root, state.phase, Stage::Ship);
            }
            GateAction::LoopBack(_) => {
                return loop_back_to_code(
                    project_root,
                    state,
                    FixType::AuditFix,
                    LoopBackReason::GateResponse,
                );
            }
            GateAction::Abort(reason) => return abort(project_root, state, &reason),
        }
    }
    let _ = close_gate(project_root, state.phase, Stage::Validate);
    let _ = close_gate(project_root, state.phase, Stage::Ship);
    clear_run_state(project_root, state.phase)?;
    // 23b: the workflow is genuinely over — deregister this (project_root,
    // phase) from the machine-global registry so `devflow gate list
    // --all-roots` stops naming a phase that no longer exists.
    registry::deregister(project_root, state.phase);
    match ship::consume_cron_instructions(project_root, state.phase) {
        Ok(Some(path_kind)) => {
            let path_kind = match path_kind {
                ship::CronInstructionPathKind::PerPhase => "per-phase",
                ship::CronInstructionPathKind::Legacy => "legacy",
                ship::CronInstructionPathKind::Both => "both",
            };
            events::emit(
                project_root,
                state.phase,
                "cron_instructions_consumed",
                serde_json::json!({
                    "trigger": "ship_complete",
                    "path_kind": path_kind,
                }),
            );
        }
        Ok(None) => {}
        Err(err) => info!(phase = %state.phase, "could not consume cron instructions: {err}"),
    }
    // 23-06 / T-23-67: the ONLY site in the workspace permitted to emit
    // `workflow_shipped`. This is the strict shipped predicate `ship_evidence`
    // reads — deliberately NOT `workflow_finished` below, which is also
    // emitted by `transition`'s `--until` clean-stop branch (`pipeline_gate.rs`,
    // `state.stop_until == Some(from)` arm) with a `"stopped_at"` reason,
    // before any hook, stage assignment, or launch runs. A phase halted after
    // one stage must never read as shipped. Ordering here is load-bearing in
    // both directions: after the hook-success loop `break` above, so it can
    // only fire once the entire `hooks_after_ship` batch has succeeded; and
    // before `workflow_finished` below, because existing tests assert a
    // phase's event stream ENDS in `workflow_finished` — do not move this
    // emission after it.
    events::emit(
        project_root,
        state.phase,
        "workflow_shipped",
        serde_json::json!({
            "stage": Stage::Ship.to_string(),
        }),
    );
    events::emit(
        project_root,
        state.phase,
        "workflow_finished",
        serde_json::Value::Null,
    );
    println!("phase {} shipped — workflow complete", state.phase);
    Ok(())
}

// A gate wait is a plain call in a normal build. The test build routes it through
// the wrapper below, the only path that can hold the write lock across a wait.
#[cfg(not(test))]
use crate::pipeline_launch::wait_for_gate_response as gate_wait_call;

#[cfg(test)]
thread_local! {
    /// Set on a waiter thread to make its gate wait hold the state write lock
    /// (V-12's control). See [`gate_wait_call`].
    static WAIT_HOLDS_WRITE_LOCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Told once this thread is at its gate wait, after [`gate_wait_call`] has taken
    /// whatever it holds, so a probe never reads the moment before.
    static WAIT_ENTERED: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> =
        const { std::cell::RefCell::new(None) };
}

/// The test build's route to a gate wait, and V-12's control (50.1-10): with
/// [`WAIT_HOLDS_WRITE_LOCK`] set, the wait call runs inside a
/// `workflow::update_state_with` closure, the waiter that holds the lock across
/// its wait. `a_gate_wait_never_holds_the_write_lock` runs the same path both ways,
/// which is what shows its probe can see a held lock. A release build has no such
/// path. Every later rewrite of the wait (50.1-14) keeps this wrapper around its
/// wait call.
///
/// The wait writes no state, so the variant cannot deadlock on its own lock; the
/// commits before and after the wait stay outside it.
#[cfg(test)]
fn gate_wait_call(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    gate_id: &GateId,
    timeout_secs: u64,
) -> Result<GateWaitEnd, CliError> {
    let wait = || {
        WAIT_ENTERED.with_borrow_mut(|entered| {
            if let Some(entered) = entered.take() {
                let _ = entered.send(());
            }
        });
        crate::pipeline_launch::wait_for_gate_response(
            project_root,
            phase,
            stage,
            gate_id,
            timeout_secs,
        )
    };
    if WAIT_HOLDS_WRITE_LOCK.get() {
        return workflow::update_state_with(project_root, phase, |_| {
            Ok::<_, workflow::WorkflowError>((None, wait()))
        })
        .expect("the control's waiter takes the write lock");
    }
    wait()
}

/// V-21's view of one approval's two consumers, shared by the threads that install it.
#[cfg(test)]
#[derive(Debug, Default)]
struct ClaimProbe {
    claimed: std::sync::atomic::AtomicUsize,
    already_consumed: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
thread_local! {
    /// Set on a consumer's thread to count its claim, see [`note_claim`].
    static CLAIM_PROBE: std::cell::RefCell<Option<std::sync::Arc<ClaimProbe>>> =
        const { std::cell::RefCell::new(None) };
}

/// Count a consumer's claim outcome and hold it until the other consumer has claimed
/// too, with a bound. Without the hold the winner acts at once and closes the gate, and
/// the loser meets a closed gate rather than the other's claim. Does nothing on a
/// thread with no probe installed.
#[cfg(test)]
fn note_claim(claim: &Result<Claim, GateError>) {
    use std::sync::atomic::Ordering::SeqCst;
    const OTHER_CONSUMER_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);
    let Some(probe) = CLAIM_PROBE.with_borrow(Clone::clone) else {
        return;
    };
    match claim {
        Ok(Claim::Claimed) => probe.claimed.fetch_add(1, SeqCst),
        Ok(Claim::AlreadyConsumed) => probe.already_consumed.fetch_add(1, SeqCst),
        Err(_) => 0,
    };
    let started = std::time::Instant::now();
    while probe.claimed.load(SeqCst) + probe.already_consumed.load(SeqCst) < 2
        && started.elapsed() < OTHER_CONSUMER_WITHIN
    {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Open a gate and block (in the detached monitor) until its answer or the long
/// timeout, consuming the answer through its claim. See [`run_gate_with_timeout`].
pub(crate) fn run_gate(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    context: &str,
) -> Result<GateAction, CliError> {
    run_gate_with_timeout(
        project_root,
        state,
        stage,
        context,
        gate_timeout_secs(),
        None,
    )
}

/// `run_gate`'s body, parameterized on the wait's timeout (WR-02, phase 20
/// review) so [`finish_workflow_with_gate_timeout`]'s foreground retry-gate
/// wait can pass a bounded timeout instead of [`gate_timeout_secs`]'s
/// multi-day production default.
///
/// The bound protocol (D-04, D-05): a fresh gate id with this waiter registered before
/// the gate file is published, that registration held for the whole wait, a wait for
/// that id's answer with no write lock held, and one first-wins claim before anything
/// acts on the answer.
pub(crate) fn run_gate_with_timeout(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    context: &str,
    timeout_secs: u64,
    // 23-09 (D-04/D-05/D-06): a pre-authorized response to write for this
    // gate, or `None` for a real human gate. This function must NEVER
    // derive this value from `state` (e.g. `state.yes_ship`) — the decision
    // belongs to the caller precisely so the reopened finalization-retry
    // gate in `finish_workflow_with_gate_timeout` cannot inherit an
    // authorization meant only for the routine Ship approval. The regression
    // test `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`
    // (pipeline_outcomes.rs) fails loudly if a future refactor reads the
    // flag from `state` inside this function instead of receiving it here.
    auto_response: Option<&GateResponse>,
) -> Result<GateAction, CliError> {
    commit(state, |s| s.gate_pending = true)?;
    // A crash inside this call before the publish leaves `gate_pending` with no gate
    // file, which `doctor`'s `check_gate_pending_without_gate` reports.
    let (opened, registration) = fenced(project_root, state.phase, || {
        Ok(Gates::open_registered(
            project_root,
            state.phase,
            stage,
            context,
        )?)
    })?;
    let gate_id = &opened.gate_id;
    println!(
        "gate written: .devflow/gates/{}-{stage}.json — awaiting response",
        state.phase.padded()
    );
    // A gate is "unexpected" when the active mode would not normally fire
    // one for this stage (e.g. a Define/Plan/Code failure in Auto mode) —
    // WR-11's never-silent path gates unconditionally, independent of mode.
    let unexpected = !state.mode.should_gate(
        stage,
        state.consecutive_failures,
        state.phase_validate_failures,
    );
    if unexpected {
        info!(
            "never-silent gate: {stage} failed in {:?} mode — surfacing an unattended gate this mode would not normally fire",
            state.mode
        );
    }
    events::emit(
        project_root,
        state.phase,
        "gate_fired",
        serde_json::json!({
            "stage": stage.to_string(),
            "unexpected": unexpected,
            "context": context,
        }),
    );
    gates::fire_gate_notify_with_fallback(
        state.phase,
        stage,
        context,
        unexpected,
        state.gate_notify_cmd.as_deref(),
    );
    events::emit(
        project_root,
        state.phase,
        "notify_fired",
        serde_json::json!({ "stage": stage.to_string(), "unexpected": unexpected }),
    );
    // The auto-response, if any, is written here: after the gate opened and both
    // `gate_fired`/`notify_fired` events, so the event stream still reads as a real gate
    // that was really answered, and before the wait below, whose first read finds it.
    if let Some(response) = auto_response {
        // Safe point 5 (CR-01, 50-REVIEW.md): an answer already on disk
        // wins over a stop in the wait below, so a stop taken during the
        // advance is honoured before the pre-authorized one is written. As
        // at safe point 4, the gate is closed, the stop recorded and an
        // error unwinds every caller.
        if crate::pipeline_launch::stop_requested_at_safe_point() {
            let _ = close_gate(project_root, state.phase, stage);
            registration.finish();
            crate::pipeline_launch::record_advance_stop(state, 5)?;
            return Err(CliError::Message(format!(
                "the gate for stage {stage} was not auto-answered: {}",
                crate::pipeline_launch::safe_point_stop_reason()
            )));
        }
        let answered = fenced(project_root, state.phase, || {
            match Gates::answer(project_root, state.phase, stage, gate_id, response) {
                Ok(_) => Ok(()),
                // A human or 23b's stale-gate sweep may have answered first;
                // first-writer-wins is the correct resolution, not an error.
                Err(GateError::AlreadyResponded { .. }) => Ok(()),
                Err(err) => Err(err.into()),
            }
        });
        if let Err(err) = answered {
            registration.finish();
            return Err(err);
        }
    }
    let wait_end = match gate_wait_call(project_root, state.phase, stage, gate_id, timeout_secs) {
        Ok(end) => end,
        Err(err) => {
            registration.finish();
            return Err(err);
        }
    };
    match wait_end {
        GateWaitEnd::Answered(response) => {
            let claim = fenced(project_root, state.phase, || {
                Ok(Gates::claim(project_root, state.phase, stage, gate_id))
            });
            let claim = match claim {
                Ok(claim) => claim,
                // D-03: not the recorded supervisor, so nothing was claimed.
                Err(err) => {
                    registration.finish();
                    return Err(err);
                }
            };
            #[cfg(test)]
            note_claim(&claim);
            match claim {
                Ok(Claim::Claimed) => {}
                Ok(Claim::AlreadyConsumed) => {
                    registration.finish();
                    return Err(CliError::Message(format!(
                        "phase {}: the answer to {stage} gate {gate_id} was already consumed: \
                         another consumer (`devflow ship`) acted on it, so this wait did nothing",
                        state.phase
                    )));
                }
                // R-08: the gate closed or was replaced after the answer was read, or
                // its lock was held too long. Nothing here records a decision.
                Err(err) => {
                    registration.finish();
                    return Err(CliError::Message(format!(
                        "phase {}: the answer to {stage} gate {gate_id} could not be claimed, \
                         so nothing was consumed: {err}",
                        state.phase
                    )));
                }
            }
            registration.finish();
            #[cfg(test)]
            crate::pipeline_launch::test_hold("after_claim");
            // D-08: whatever changed on disk during the wait (a `stop`, a
            // `yes_ship` revoke, an edited notify command) is kept, because this
            // write re-reads the state and sets only this field.
            commit(state, |s| s.gate_pending = false)?;
            let action = GateAction::from_response(&response);
            events::emit(
                project_root,
                state.phase,
                "gate_resolved",
                serde_json::json!({
                    "stage": stage.to_string(),
                    "gate_id": gate_id.as_str(),
                    "approved": response.approved,
                    "action": match &action {
                        GateAction::Advance => "advance",
                        GateAction::LoopBack(_) => "loop_back",
                        GateAction::Abort(_) => "abort",
                    },
                    "responded_by": response.responded_by,
                }),
            );
            // `gate_pending` reads false here, and the caller has not applied the action.
            #[cfg(test)]
            crate::pipeline_launch::test_hold("after_commit");
            Ok(action)
        }
        GateWaitEnd::Stopped => {
            // Safe point 4 (50-13, R-2): the supervisor running this advance
            // was stopped during the wait. The gate has no waiter any more, so
            // it is closed (an unconsumed answer is set aside, nothing outlives
            // the run in its slot), the stop is recorded, and an error unwinds
            // every caller through `?`, which releases the per-phase lock.
            // Deliberately not `GateAction::Abort`: `abort()` deletes the state
            // file, and a stopped run keeps its state for `devflow resume`.
            let _ = close_gate(project_root, state.phase, stage);
            registration.finish();
            crate::pipeline_launch::record_advance_stop(state, 4)?;
            Err(CliError::Message(format!(
                "the gate wait for stage {stage} ended: {}",
                crate::pipeline_launch::safe_point_stop_reason()
            )))
        }
        GateWaitEnd::TimedOut => {
            registration.finish();
            events::emit(
                project_root,
                state.phase,
                "gate_timeout",
                serde_json::json!({ "stage": stage.to_string() }),
            );
            Err(CliError::Message(format!(
                "gate for stage {stage} timed out awaiting a response"
            )))
        }
    }
}

/// Abort the workflow with a reason, clearing state.
pub(crate) fn abort(project_root: &Path, state: &State, reason: &str) -> Result<(), CliError> {
    // See CR-01: an answer for this phase+stage must not outlive the run. An unused
    // one is set aside, never deleted. D-03: a superseded supervisor ends nothing, so its
    // refusal goes up instead of the rest of the abort.
    if let Err(err) = close_gate(project_root, state.phase, state.stage)
        && is_not_owner(&err)
    {
        return Err(err);
    }
    if let Err(err) = clear_run_state(project_root, state.phase)
        && is_not_owner(&err)
    {
        return Err(err);
    }
    println!("workflow aborted for phase {}: {reason}", state.phase);
    // 23b: an abort also ends the workflow — deregister the same as the
    // success path so an aborted phase does not linger in the registry.
    registry::deregister(project_root, state.phase);
    events::emit(
        project_root,
        state.phase,
        "workflow_aborted",
        serde_json::json!({ "reason": truncate_reason(reason) }),
    );
    Ok(())
}

/// Manual ship override (20e, D-01): a second, out-of-process consumer of the same
/// Ship answer `run_gate`'s live wait consumes. `devflow gate approve` only WRITES an
/// answer (`Gates::answer`); a live monitor waiting on the gate is what normally
/// advances the workflow. If that monitor died before consuming the answer, the
/// approval sits unconsumed forever; this function reads the answer directly, claims
/// it, and on `GateAction::Advance` drives the SAME `finish_workflow` the live wait
/// would have called (D-01) — not a reimplementation of the after-ship hook batch.
///
/// Guard order (D-02, D-04):
/// 1. Claim the D-17 holder, with the D-18 stop handler installed first
///    ([`crate::commands::claim_foreground_holder`]), so `stop` can reach this process and
///    no other foreground process acts on the phase.
/// 2. Acquire the per-phase lock ([`lock::acquire`]) BEFORE touching state.
/// 3. `state.stage` must be EXACTLY `Stage::Ship`; any earlier stage is refused, naming
///    the stage to resolve first.
/// 4. The live Ship gate must hold an answer bound to its id. A file that does not bind
///    is refused and left where it is: this command reads and moves nothing (R-09).
/// 5. The answer must be claimed before anything acts on it. A claim already there means
///    another consumer took it and may have died mid-`finish_workflow`, and any claim
///    error (the gate closed or was replaced since this command read it, R-08, or its
///    lock was held too long) means nothing was consumed. All of them refuse here, so
///    no claim error reaches `finish_workflow`, `loop_back_to_code` or `abort`.
///
/// `force` is accepted and echoed in the CLI output for explicit operator
/// auditability (Hermes LOW: make `--force` semantics explicit), but is
/// deliberately NOT consulted by any guard above — D-02 scopes `--force` to
/// "skip Ship-gate re-verification," never to bypassing the stage, lock,
/// gate-existence, or claim checks themselves. This design has no separate
/// Ship-gate re-verification step beyond those guards, so `--force`
/// currently changes no observable behavior; it exists so the flag is never
/// silently ignored and cannot later be wired to widen scope without an
/// explicit, reviewed change to this function.
pub(crate) fn ship_override(
    project_root: &Path,
    phase: PhaseId,
    force: bool,
) -> Result<(), CliError> {
    let _phase_holder = crate::commands::claim_foreground_holder(project_root, phase)?;
    let _lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, .. }) => {
            return Err(CliError::Message(format!(
                "phase {phase}: another devflow process (pid {pid}) holds the per-phase lock — \
                 refusing to race its poll of the Ship gate response"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };

    let mut state = workflow::load_state(project_root, phase)?;

    if state.stage != Stage::Ship {
        return Err(CliError::Message(format!(
            "phase {phase} is at stage {} — `devflow ship` requires state.stage == Stage::Ship; \
             resolve stage {} first (--force does not skip stages)",
            state.stage, state.stage
        )));
    }

    let response = claim_ship_answer(project_root, phase)?;

    println!(
        "phase {phase}: manual ship override (--force={force}) — driving the already-written \
         Ship response through the same terminal path the live monitor would have used"
    );

    match GateAction::from_response(&response) {
        GateAction::Advance => {
            // WR-02 (phase 20 review): finish_workflow's retry-gate wait
            // (on a terminal-hook failure) normally uses gate_timeout_secs'
            // multi-day production default, invisible to an operator
            // because every OTHER caller runs inside a detached monitor.
            // ship_override runs in the FOREGROUND CLI — bound the wait so
            // a hook failure fails fast with an actionable message instead
            // of blocking this shell for days.
            let timeout = foreground_gate_timeout_secs();
            println!(
                "phase {phase}: if terminal-hook finalization fails, this foreground command \
                 will wait up to {timeout}s for the reopened Ship gate before failing (vs. the \
                 multi-day background default) — set DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS to \
                 change this"
            );
            finish_workflow_with_gate_timeout(project_root, &mut state, timeout)
        }
        GateAction::LoopBack(_) => {
            // Antigravity LOW: `loop_back_to_code` → `launch_stage` forks a
            // NEW detached monitor daemon — say so explicitly, so `devflow
            // ship` is not a silently long-running process the operator
            // can't account for.
            println!(
                "phase {phase}: Ship response loops back to Code — launching a new, detached \
                 monitor agent to drive the retry"
            );
            loop_back_to_code(
                project_root,
                &mut state,
                FixType::AuditFix,
                LoopBackReason::GateResponse,
            )
        }
        GateAction::Abort(reason) => abort(project_root, &state, &reason),
    }
}

/// Read the live Ship gate's answer and claim it: guards 4 and 5 of [`ship_override`].
/// Every refusal here consumes nothing.
fn claim_ship_answer(project_root: &Path, phase: PhaseId) -> Result<GateResponse, CliError> {
    let stage = Stage::Ship;
    let no_answer = || {
        CliError::Message(format!(
            "phase {phase}: no Ship gate response written yet — nothing to ship (a dead monitor \
             never wrote or received one; wait for `devflow gate approve` or resolve the pipeline first)"
        ))
    };
    let gate_id = crate::commands::gate_file_id(project_root, phase, stage)?
        .ok_or_else(no_answer)
        .and_then(|id| {
            GateId::parse(&id).map_err(|_| {
                CliError::Message(format!(
                    "phase {phase}: the Ship gate file has no valid gate id, so no answer can \
                     bind to it; nothing was consumed. Run `devflow doctor`"
                ))
            })
        })?;
    let answer = match Gates::read_answer(project_root, phase, stage, &gate_id) {
        AnswerRead::None => return Err(no_answer()),
        AnswerRead::Unbound(reason) => {
            return Err(CliError::Message(format!(
                "phase {phase}: the file in Ship gate {gate_id}'s answer slot does not bind to it \
                 ({reason}); nothing was consumed. `devflow gate approve` or `devflow gate reject` \
                 sets that file aside before publishing a valid answer"
            )));
        }
        AnswerRead::Bound(answer) => answer,
    };
    let claim = fenced(project_root, phase, || {
        Ok(Gates::claim(project_root, phase, stage, &gate_id))
    })?;
    #[cfg(test)]
    note_claim(&claim);
    match claim {
        Ok(Claim::Claimed) => Ok(answer.response),
        Ok(Claim::AlreadyConsumed) => Err(CliError::Message(format!(
            "phase {phase}: the Ship gate response was already consumed (gate {gate_id} has a \
             claim) — the phase may be mid-finalization from a monitor that died partway through; \
             run `devflow doctor` to inspect it rather than re-running terminal hooks"
        ))),
        Err(GateError::NoOpenGate { .. } | GateError::GateIdMismatch { .. }) => {
            Err(CliError::Message(format!(
                "phase {phase}: the Ship gate {gate_id} this command read was closed or replaced \
                 before it could be claimed; nothing was consumed. Run `devflow status`"
            )))
        }
        Err(GateError::GateLockTimeout { waited, .. }) => Err(CliError::Message(format!(
            "phase {phase}: could not take the gate lock to claim Ship gate {gate_id} within \
             {waited:?}; nothing was consumed. Run `devflow ship` again"
        ))),
        Err(err) => Err(CliError::Message(format!(
            "phase {phase}: could not claim Ship gate {gate_id}'s answer; nothing was consumed: \
             {err}"
        ))),
    }
}

/// Print the full pipeline that a `start` would run, without launching anything.
pub(crate) fn print_dry_run(state: &State) {
    println!(
        "dry run — phase {} | agent {} | mode {}",
        state.phase, state.agent, state.mode
    );
    println!("\nstage pipeline:");
    let mut stage = Some(Stage::Define);
    while let Some(s) = stage {
        let command = s.gsd_command().replace("{N}", &state.phase.to_string());
        // F-7: these are PREDICTION probes, not live reads. Every one passes
        // literals rather than `state.consecutive_failures` /
        // `state.phase_validate_failures` on purpose — the preview answers
        // "what will this pipeline do", not "where is this run right now",
        // and a dry run is printed before a run has any position to report.
        // Widening `should_gate` forces these open; closing them with a
        // placeholder would leave `--dry-run` silently no longer predicting
        // the per-phase ceiling gate, which is the operator's only advance
        // account of where a run will stop.
        let unconditional = state.mode.should_gate(s, 0, 0);
        let gate = if unconditional {
            " [GATE]".to_string()
        } else if state.mode.should_gate(s, mode::MAX_CONSECUTIVE_FAILURES, 0) {
            format!(
                " [GATE after {} consecutive failures]",
                mode::MAX_CONSECUTIVE_FAILURES
            )
        } else {
            String::new()
        };
        // The per-phase ceiling probe is a SEPARATE clause, not a third `else
        // if`. As an `else if` after the streak probe it is unreachable for
        // Validate in both modes — Auto's streak probe already returns true
        // and Supervise's unconditional probe already returns true — so the
        // preview would silently never name the new gate, which is precisely
        // the T-35-20c harm F-7 exists to prevent. Measured before the fix:
        // `devflow start --phase 7 --mode auto --dry-run` printed only
        // `[GATE after 3 consecutive failures]` on the Validate line.
        //
        // `!unconditional` suppresses it where it would be noise rather than
        // information: Ship and Supervise-mode Validate gate regardless of any
        // failure count, so naming a failure ceiling there tells the operator
        // nothing about where this run will stop.
        let phase_gate = if !unconditional
            && state
                .mode
                .should_gate(s, 0, mode::MAX_PHASE_VALIDATE_FAILURES)
        {
            format!(
                " [GATE at {} validate failures for this phase]",
                mode::MAX_PHASE_VALIDATE_FAILURES
            )
        } else {
            String::new()
        };
        // WR-01 (phase 20 review): annotate the stage matching `--until` so
        // the dry-run preview reflects that a real invocation would halt
        // here instead of always showing the full Define→Ship pipeline.
        let stop_marker = if state.stop_until == Some(s) {
            " [STOPS HERE — --until]"
        } else {
            ""
        };
        println!("  {s:<9} {command}{gate}{phase_gate}{stop_marker}");
        if let Some(next) = s.next() {
            let transition_hooks = hooks::hooks_for_transition(s, next);
            if !transition_hooks.is_empty() {
                println!("            ↳ hooks: {transition_hooks:?}");
            }
        }
        stage = s.next();
    }
    if let Some(until) = state.stop_until {
        println!("\nnote: --until {until} — this run will halt after {until} completes");
    }
    // D-12 (`28-CONTEXT.md`): the resolved combine of `--yes-ship` and a
    // `devflow.toml` `yes_ship` key, reported here so a dry run gives the
    // operator a stable, observable preview of whether the Ship gate is
    // pre-authorized — this line reports `state.yes_ship` only, it does not
    // itself distinguish flag- from config-sourced authorization (that
    // distinction is the separate never-silent notice `commands::start`
    // prints for the config-sourced case).
    println!(
        "\nship gate: {}",
        if state.yes_ship {
            "pre-authorized"
        } else {
            "not pre-authorized"
        }
    );
    println!("\nafter ship: {:?}", hooks::hooks_after_ship());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_launch::advance;
    use crate::pipeline_outcomes::{
        ValidateOutcome, handle_infra_outcome, handle_validate_outcome,
    };
    use crate::test_support::*;
    use devflow_core::agent_result;
    use devflow_core::gates::GateResponse;
    use devflow_core::mode::Mode;
    use devflow_core::state::AgentKind;
    use devflow_core::test_support::{answer_open_gate, gate_answer_files, preanswer_gate};

    macro_rules! enter_path_isolated_child {
        ($name:expr, $path_dir:expr) => {
            if !devflow_core::test_support::in_child_test($name) {
                let path_dir = $path_dir;
                let output =
                    crate::test_support::run_hermetic_test_in_child($name, path_dir.path(), &[]);
                devflow_core::test_support::assert_child_ran_exactly_one_passing_test(
                    &output, $name,
                );
                return;
            }
        };
    }

    /// `advance()` over a Ship-stage success with an approved Ship gate must run
    /// the terminal `finish_workflow` path (after-ship hooks + gate cleanup +
    /// state cleared) — the only non-spawning branch of `advance`'s orchestration
    /// (11-VALIDATION.md 12f). The gate response is pre-seeded on disk so
    /// `run_gate`'s poll returns immediately instead of blocking.
    #[test]
    fn advance_ship_success_runs_finish_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(21);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let branch_created = devflow_core::test_support::git_command(root)
            .args(["branch", &branch, "develop"])
            .status()
            .unwrap()
            .success();
        assert!(branch_created);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        // Seed a DEVFLOW_RESULT success marker so `evaluate_agent_result` resolves
        // at Layer 1 without needing the exit-code/commit-count fallback.
        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();

        // Pre-write an approved Ship gate response so `run_gate` returns
        // `GateAction::Advance` immediately instead of polling.
        preanswer_gate(
            root,
            phase,
            Stage::Ship,
            &serde_json::from_str(r#"{"approved":true,"note":null,"responded_by":"test"}"#)
                .unwrap(),
        );

        advance(root, Some(phase)).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(gate_answer_files(root, phase, Stage::Ship).is_empty());
        assert!(gate_claim_files(root, phase, Stage::Ship).is_empty());
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
    }

    /// 23-06 Task 1 acceptance: a real Ship finalization emits the
    /// terminal-only `workflow_shipped` event, and
    /// `ship_evidence::collect` reads it as `shipped: true` — proving the
    /// event this module emits is exactly the one the oracle consumes.
    #[test]
    fn advance_ship_success_emits_workflow_shipped_and_ship_evidence_reports_shipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(23);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let branch_created = devflow_core::test_support::git_command(root)
            .args(["branch", &branch, "develop"])
            .status()
            .unwrap()
            .success();
        assert!(branch_created);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();
        let cron = ship::build_single_agent_cron_instructions(root, phase, "2026-06-18T15:45:30Z");
        ship::write_cron_instructions(root, &cron).unwrap();

        std::fs::write(
            agent_result::stdout_path(root, phase),
            "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();

        preanswer_gate(
            root,
            phase,
            Stage::Ship,
            &serde_json::from_str(r#"{"approved":true,"note":null,"responded_by":"test"}"#)
                .unwrap(),
        );

        advance(root, Some(phase)).unwrap();

        assert!(
            devflow_core::events::has_event_for_phase(root, phase, "workflow_shipped"),
            "a real Ship finalization must emit the terminal-only workflow_shipped event"
        );
        let evidence = devflow_core::ship_evidence::collect(root, phase);
        assert!(
            evidence.shipped,
            "ship_evidence must read the just-emitted workflow_shipped event as shipped"
        );
        // The pre-existing invariant several other tests in this module
        // depend on: the phase's event stream still ends in
        // workflow_finished, unmodified by this task's additive emission.
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("events recorded for phase");
        assert_eq!(last["event"], "workflow_finished");
        assert!(!ship::cron_instructions_path(root, phase).exists());
        let consumed = std::fs::read_to_string(devflow_core::events::events_path(root))
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|event| event["event"] == "cron_instructions_consumed")
            .expect("ship completion must audit cron consumption");
        assert_eq!(consumed["trigger"], "ship_complete");
    }

    /// 23-06 Task 1's named blocker regression guard (BLOCKER 1,
    /// cross-AI-review-caught): a phase halted cleanly by `devflow start
    /// --until <stage>` runs `transition`'s `stop_until` clean-stop branch,
    /// which emits `workflow_finished` with a `"stopped_at"` reason and
    /// returns before any hook, stage assignment, or `launch_stage` call —
    /// `workflow_shipped` must never be emitted on this path, and
    /// `ship_evidence::collect(..).shipped` must read false even though
    /// `workflow_finished_seen` is true.
    #[test]
    fn until_stop_never_emits_workflow_shipped_and_ship_evidence_reports_not_shipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(24);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        workflow::save_state(&state).unwrap();

        transition(root, &mut state, Stage::Code).unwrap();

        assert!(
            !devflow_core::events::has_event_for_phase(root, phase, "workflow_shipped"),
            "the --until clean-stop branch must never emit workflow_shipped"
        );
        let evidence = devflow_core::ship_evidence::collect(root, phase);
        assert!(
            !evidence.shipped,
            "a phase that only stopped after one stage must not read as shipped"
        );
        assert!(evidence.workflow_finished_seen);
        assert_eq!(evidence.finished_reason.as_deref(), Some("stopped_at"));
        assert!(devflow_core::ship_evidence::is_stopped_at(&evidence));
    }

    /// 50-13: the `--until` stop clears the supervisor record, because the
    /// supervisor running this advance finishes once the advance returns.
    /// The record is set first, so a stop that leaves it standing fails here.
    #[test]
    fn until_stop_clears_the_supervisor_record() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(1313);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.stop_until = Some(Stage::Plan);
        state.supervisor = Some(devflow_core::state::SupervisorRecord::new(
            root.join("until.sock"),
            "until00000000000".to_string(),
            4242,
            1,
            String::new(),
        ));
        workflow::save_state(&state).unwrap();

        transition(root, &mut state, Stage::Code).unwrap();

        let reloaded = workflow::load_state(root, phase).unwrap();
        assert!(reloaded.stopped, "the --until stop marks the run stopped");
        assert_eq!(
            reloaded.supervisor, None,
            "the --until stop clears the supervisor record"
        );
    }

    #[test]
    fn terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let git = |args: &[&str]| {
            let output = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["checkout", "-q", "-b", "feature/phase-22"]);
        std::fs::write(root.join("conflict.txt"), "feature\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "feature change"]);
        git(&["checkout", "-q", "develop"]);
        std::fs::write(root.join("conflict.txt"), "develop\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "develop change"]);

        let mut state = State::new(
            PhaseId::new(22),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let root_owned = root.to_path_buf();
        let handle = std::thread::spawn(move || {
            let mut state = workflow::load_state(&root_owned, PhaseId::new(22)).unwrap();
            finish_workflow(&root_owned, &mut state)
        });
        let gate_path = Gates::gate_path(root, PhaseId::new(22), Stage::Ship);
        for _ in 0..100 {
            if gate_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            gate_path.exists(),
            "finalization failure must reopen Ship gate"
        );
        assert!(
            workflow::load_state(root, PhaseId::new(22))
                .unwrap()
                .gate_pending
        );
        answer_open_gate(
            root,
            PhaseId::new(22),
            Stage::Ship,
            &GateResponse {
                approved: false,
                note: Some("abort after merge conflict".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        handle.join().unwrap().unwrap();

        assert_ne!(
            events::last_event_for_phase(root, PhaseId::new(22))
                .and_then(|event| event["event"].as_str().map(str::to_owned))
                .as_deref(),
            Some("workflow_finished")
        );
        let tags = devflow_core::test_support::git_command(root)
            .arg("tag")
            .output()
            .unwrap();
        assert!(tags.stdout.is_empty());
    }

    /// 23-09 Task 2 (T-23-91's regression guard, cross-AI-review HIGH): with
    /// `state.yes_ship` set, a terminal-hook failure that reopens the
    /// finalization gate must NOT be auto-approved — the reopened gate is a
    /// different call site (passing `None`) than the routine Ship approval
    /// in `handle_ship_outcome` (passing `Some`), and this is the concrete
    /// proof. If a future refactor folds `state.yes_ship` into
    /// `run_gate_with_timeout`'s own body instead of threading it as a
    /// caller argument (see that parameter's doc comment), this test fails
    /// loudly: the reopened gate would then find an unexpected response and
    /// never block for a human.
    ///
    /// `VersionBump` is failed deterministically via `version::compute_version`
    /// itself refusing (D-10's `UnreachableBaseline`), rather than by
    /// pre-creating a tag name for `git.tag(&tag)` to collide with
    /// (`hooks.rs:286-287`).
    ///
    /// **Why not a tag-name collision (25-01 rewrite, D-07/D-08):** the prior
    /// (pre-25-01) algorithm derived MINOR from a raw, reachability-blind git
    /// tag *count*, so pre-creating one extra tag deterministically
    /// incremented that count by exactly one, and the fixture could predict
    /// and pre-create the exact resulting tag name. The 25-01 algorithm is
    /// no longer count-based: `compute_version` derives its baseline from
    /// the highest semver tag *reachable from HEAD*, and always bumps
    /// strictly past that baseline (D-10's no-bump-collapses-to-patch floor
    /// guarantees this even when nothing in the range warrants a bump — see
    /// `apply_bump`'s doc comment in `version.rs`). Consequently, pre-creating
    /// ANY tag reachable from HEAD makes it become the new baseline the next
    /// time `compute_version` runs, and the recomputed result is
    /// *unconditionally different* from (one patch past) whatever we just
    /// created — there is no reachable tag this fixture could create that
    /// `compute_version` would ever predict again, so a tag-name collision
    /// can no longer be constructed this way. (Confirmed empirically during
    /// this rewrite: pre-creating the tag `compute_version` returned before
    /// any tag existed did not collide — `VersionBump`'s own later call saw
    /// that tag as the new reachable baseline and computed the next patch
    /// past it instead, so `finish_workflow` shipped successfully instead of
    /// re-opening the gate this test exists to exercise.)
    ///
    /// Instead, this fixture tags an **unreachable** orphan commit
    /// (`git tag` sees it via [`devflow_core::version::highest_semver_tag`]'s
    /// reachability-blind repo-wide scan; `develop`'s own reachable baseline
    /// stays `None` because the tag is not an ancestor of `develop`) — D-10's
    /// refusal fires unconditionally in that state, independent of any
    /// version arithmetic, so `VersionBump` fails deterministically before it
    /// ever reaches its `git.tag(&tag)` call. `Merge` still runs first in
    /// `hooks_after_ship` and succeeds — the feature branch is created
    /// identical to `develop`, so `is_merged_into_develop` is immediately
    /// true and `merge_feature` takes its already-merged no-op success
    /// path — exactly the "Merge has already succeeded" state the reopened
    /// gate exists for (no rollback policy, `hooks.rs` doc comment on
    /// `merge_feature`).
    #[test]
    fn finalization_retry_gate_never_auto_approves_even_with_yes_ship_set() {
        const NAME: &str = "pipeline_gate::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set";
        enter_path_isolated_child!(NAME, agent_free_git_only_path_dir());
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "abort-fixture test must run in a child with an agent-free PATH (999.80)"
        );
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        // Tag an orphan commit unreachable from `develop` — this makes
        // `version::compute_version` refuse unconditionally (D-10's
        // `UnreachableBaseline`) once `VersionBump` calls it, regardless of
        // any subsequent state. `develop` is where `init_repo` leaves HEAD
        // checked out, and is restored as the checkout before continuing.
        let git_cmd = |args: &[&str]| {
            assert!(
                devflow_core::test_support::git_command(root)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        git_cmd(&["checkout", "--orphan", "unreachable-release"]);
        git_cmd(&["commit", "--allow-empty", "-q", "-m", "chore: orphan"]);
        git_cmd(&["tag", "v9.9.9"]);
        git_cmd(&["checkout", "develop"]);
        assert!(
            devflow_core::version::compute_version(root).is_err(),
            "the orphan tag must make compute_version refuse (D-10) before \
             VersionBump ever gets to its own git.tag(&tag) call"
        );

        let phase = PhaseId::new(60);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let branch_created = devflow_core::test_support::git_command(root)
            .args(["branch", &branch, "develop"])
            .status()
            .unwrap()
            .success();
        assert!(branch_created);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();

        let root_owned = root.to_path_buf();
        let handle = std::thread::spawn(move || {
            let mut state = workflow::load_state(&root_owned, phase).unwrap();
            finish_workflow(&root_owned, &mut state)
        });

        let gate_path = Gates::gate_path(root, phase, Stage::Ship);
        for _ in 0..150 {
            if gate_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            gate_path.exists(),
            "a terminal-hook failure must reopen the Ship gate even with yes_ship set"
        );

        // Give any (incorrect) auto-write a moment to have happened if it
        // were ever going to.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            gate_answer_files(root, phase, Stage::Ship).is_empty(),
            "yes_ship must NEVER auto-approve the reopened finalization-retry gate \
             (T-23-91) — a failing finalization must wait for a human"
        );
        assert!(
            workflow::load_state(root, phase).unwrap().gate_pending,
            "the reopened gate must be recorded as pending, awaiting a human"
        );
        assert!(
            devflow_core::events::has_event_for_phase(root, phase, "merge_result"),
            "Merge must have succeeded before VersionBump failed — the reopened gate must be \
             exercised in the state it actually occurs in"
        );

        // Unblock the poll (reject, not approve — this test's subject is
        // that nothing auto-approves it) so the spawned thread can finish.
        answer_open_gate(
            root,
            phase,
            Stage::Ship,
            &GateResponse {
                approved: false,
                note: Some("abort: test cleanup".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        handle.join().unwrap().unwrap();
    }

    /// 13-DEFERRED-CR-03 acceptance: two phases advancing their Ship stages
    /// CONCURRENTLY must each finish their own stage machine — per-phase
    /// state files prevent cross-phase clobbering, and the coarse checkout
    /// lock serializes both `finish_workflow`s' git operations on the shared
    /// primary checkout. Gate responses are pre-seeded so neither advance
    /// blocks polling on its *first* Ship gate.
    ///
    /// 17-09 gap closure (GAP-2): both phases compute their next version from
    /// the same starting git state, and on some runs genuinely race to
    /// create the same version tag — confirmed directly during this plan's
    /// RED phase via temporary debug instrumentation, which caught both
    /// threads inside `version_bump` with the identical computed version
    /// (`2.0.1`) within ~1.8ms of each other, and the loser's `git tag`
    /// failing with git's own "reference already exists". That failure
    /// reopens the loser's Ship gate for human review (`finish_workflow`'s
    /// retry loop) — but only ONE gate response was ever pre-written per
    /// phase (consumed by its first gate open), so the reopened gate has
    /// nothing to consume. Unbounded, the gate wait then waits on the
    /// 7-day production default (`DEVFLOW_GATE_TIMEOUT_SECS`) with no
    /// response ever arriving — that is the wedge this plan closes.
    ///
    /// The binding constraint is "never hangs," not "always both succeed."
    /// This test does not try to make the race loser also succeed (that
    /// would require re-answering a gate reactively and still not rule out
    /// a second, equally rare collision) — instead it bounds the reopened
    /// gate's poll to a few seconds via `DEVFLOW_GATE_TIMEOUT_SECS`
    /// (overridden ONLY for this test's poll, under the established
    /// `ENV_MUTEX` guard — the 7-day production default is never touched)
    /// and asserts the loser's documented behavior: a bounded timeout error,
    /// state left intact (not cleared), and an actionable Ship gate still on
    /// disk awaiting a human. The common case (no collision) still asserts
    /// both phases finish independently, exactly as before.
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_GATE_TIMEOUT_SECS` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn concurrent_ship_advances_finish_both_phases_independently() {
        let _guard = env_lock();
        let original_gate_timeout = std::env::var_os("DEVFLOW_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX. Bounds a reopened Ship gate's
        // poll to a few seconds instead of the 7-day production default.
        // Every OTHER test that reaches `run_gate` pre-writes its response
        // before calling in, so the wait finds it on the very first
        // read regardless of this value — only a *reopened*, unanswered
        // gate (this test's race-loser path) ever actually waits it out.
        unsafe {
            std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phases = [PhaseId::new(31), PhaseId::new(32)];
        for &phase in &phases {
            let branch = format!("feature/phase-{padded}", padded = phase.padded());
            let branch_created = devflow_core::test_support::git_command(root)
                .args(["branch", &branch, "develop"])
                .status()
                .unwrap()
                .success();
            assert!(branch_created);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();
            std::fs::write(
                agent_result::stdout_path(root, phase),
                "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
            )
            .unwrap();
            preanswer_gate(
                root,
                phase,
                Stage::Ship,
                &serde_json::from_str(r#"{"approved":true,"note":null,"responded_by":"test"}"#)
                    .unwrap(),
            );
        }

        let results: Vec<(PhaseId, Result<(), CliError>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = phases
                .iter()
                .map(|&phase| (phase, scope.spawn(move || advance(root, Some(phase)))))
                .collect();
            handles
                .into_iter()
                .map(|(phase, handle)| (phase, handle.join().expect("advance thread")))
                .collect()
        });

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_gate_timeout {
                Some(value) => std::env::set_var("DEVFLOW_GATE_TIMEOUT_SECS", value),
                None => std::env::remove_var("DEVFLOW_GATE_TIMEOUT_SECS"),
            }
        }

        let succeeded = results.iter().filter(|(_, r)| r.is_ok()).count();
        assert!(
            succeeded == 1 || succeeded == 2,
            "at least one phase must finish independently of the other; got {succeeded}/2 successes"
        );

        for (phase, result) in &results {
            match result {
                Ok(()) => {
                    assert!(
                        matches!(
                            workflow::load_state(root, *phase),
                            Err(workflow::WorkflowError::MissingState(_))
                        ),
                        "phase {phase} must be finished (state cleared)"
                    );
                    assert!(!Gates::gate_path(root, *phase, Stage::Ship).exists());
                    let last = devflow_core::events::last_event_for_phase(root, *phase)
                        .expect("events recorded for phase");
                    assert_eq!(
                        last["event"], "workflow_finished",
                        "phase {phase}'s own event stream must end in workflow_finished"
                    );
                }
                Err(err) => {
                    // The documented loser behavior (GAP-2): a version-tag
                    // race lost by VersionBump reopens the Ship gate for a
                    // human; with no second response pre-written, the
                    // bounded poll above times out rather than hanging.
                    assert!(
                        err.to_string().contains("timed out"),
                        "phase {phase}'s only non-success outcome must be a bounded gate \
                         timeout, not some other failure: {err}"
                    );
                    let state = workflow::load_state(root, *phase)
                        .expect("a timed-out gate leaves state intact, not cleared");
                    assert!(
                        state.gate_pending,
                        "phase {phase} must leave an actionable, still-open gate for a human"
                    );
                    assert!(
                        Gates::gate_path(root, *phase, Stage::Ship).exists(),
                        "phase {phase}'s reopened Ship gate file must remain on disk"
                    );
                }
            }
        }
    }

    /// Regression test for CR-01: `abort()` must clean up the gate's
    /// response/ack files for the stage the gate actually fired on. Without
    /// that cleanup, a later gate for the same phase+stage would find the
    /// old, already-consumed response still on disk and the gate wait
    /// would resolve from it instantly instead of waiting for a fresh human
    /// decision.
    ///
    /// 33-04: the seeded 999.66 forward-progress baseline, the
    /// child-local neutralized PATH and the two branch-pinning assertions below exist
    /// because this test once silently left its intended path — 999.66's
    /// reset-vs-accumulate change reset the directly-seeded streak to 1, so no
    /// gate fired and the test fell through to a real agent launch while every
    /// gate-file assertion it owned still passed (`prepare_loop_back_to_code`
    /// calls the same `Gates::close` that `abort()` does).
    #[test]
    fn abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response() {
        const NAME: &str = "pipeline_gate::tests::abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response";
        enter_path_isolated_child!(NAME, agent_free_git_only_path_dir());
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "abort-fixture test must run in a child with an agent-free PATH (999.80)"
        );
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = PhaseId::new(23);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
        // 999.66 (33-03) made the counter reset-vs-accumulate against a
        // persisted baseline. This test seeds the streak directly rather than
        // driving it through repeated `handle_validate_outcome` calls, so it
        // must also seed the baseline — a `None` baseline reads as
        // "first-ever failure" and resets the streak to 1, which drops this
        // test out of the gated path it asserts against and into
        // `loop_back_to_code` -> `launch_stage` (a real agent spawn).
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();

        // Pre-answer a rejection whose note says "abort" so
        // `GateAction::from_response` resolves to `Abort`.
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &GateResponse {
                approved: false,
                note: Some("abort: requirements changed".into()),
                responded_by: Some("test".into()),
            },
        );

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert!(
            state.consecutive_failures >= mode::MAX_CONSECUTIVE_FAILURES,
            "gate threshold must have been reached (got {}) — a reset means the gate never fired",
            state.consecutive_failures
        );
        assert_eq!(
            state.stage,
            Stage::Validate,
            "gate must have fired and aborted — Stage::Code means it silently looped back and tried to launch an agent"
        );

        // The gate, answer and claim files for the stage the gate fired on
        // (Validate) must all be gone after the Abort path runs, and the answer it
        // consumed is not left set aside either.
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
        assert_eq!(
            gate_answer_files(root, phase, Stage::Validate),
            Vec::<std::path::PathBuf>::new(),
            "no answer may outlive an aborted gate"
        );
        assert_eq!(
            gate_claim_files(root, phase, Stage::Validate),
            Vec::<std::path::PathBuf>::new()
        );
        assert_eq!(Gates::quarantined(root), Vec::new());

        // The phase reaching the same gate again later (e.g. after a restart) waits
        // for a fresh human decision.
        let refired = Gates::open_gate(root, phase, Stage::Validate, "re-fired gate").unwrap();
        let started = std::time::Instant::now();
        let end = crate::pipeline_launch::wait_for_gate_response(
            root,
            phase,
            Stage::Validate,
            &refired.gate_id,
            1,
        )
        .unwrap();
        assert!(
            matches!(end, GateWaitEnd::TimedOut),
            "the re-fired gate must wait for its own answer: {end:?}"
        );
        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }

    /// CR-01 regression (17-06 gap closure): `transition()` resets
    /// `infra_failures` to 0 alongside `consecutive_failures` — both in the
    /// in-memory `State` and the persisted `state.json` — and a subsequent
    /// infra fault after a clean transition starts counting from 1, not the
    /// pre-transition count. The test re-executes in a child with only a
    /// `git` symlink on PATH, so `agent_binary_available` has zero possible
    /// agent matches without mutating the parallel parent test process. The
    /// resulting `Err` from `ensure_agent_binary` is expected and ignored:
    /// the counter reset happens earlier in `transition()` and is unaffected
    /// by that downstream failure.
    #[test]
    fn transition_resets_infra_failures() {
        const NAME: &str = "pipeline_gate::tests::transition_resets_infra_failures";
        enter_path_isolated_child!(NAME, agent_free_git_only_path_dir());
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "abort-fixture test must run in a child with an agent-free PATH (999.80)"
        );
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(80);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        let _ = transition(root, &mut state, Stage::Validate);

        assert_eq!(
            state.infra_failures, 0,
            "transition() must reset infra_failures in-memory, not just consecutive_failures"
        );
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            reloaded.infra_failures, 0,
            "transition() must persist the infra_failures reset to state.json"
        );

        // A fresh infra fault after the clean transition starts counting
        // from 1, not resuming the pre-transition MAX_INFRA_FAILURES - 1
        // count toward a false premature abort.
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

        handle_infra_outcome(root, &mut state, Stage::Validate, Some("killed".into())).unwrap();

        assert_eq!(state.infra_failures, 1);
    }

    /// 18d idempotency edge: a repeated Code→Validate transition leaves
    /// `consecutive_failures` unchanged rather than zeroing it. `state.stage`
    /// is reset to `Code` before each call so both calls exercise the exact
    /// hop under test.
    #[test]
    fn repeated_code_to_validate_transition_is_idempotent_on_the_counter() {
        const NAME: &str = "pipeline_gate::tests::repeated_code_to_validate_transition_is_idempotent_on_the_counter";
        enter_path_isolated_child!(NAME, agent_free_git_only_path_dir());
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(83);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.consecutive_failures = 2;
        workflow::save_state(&state).unwrap();

        let _ = transition(root, &mut state, Stage::Validate);
        state.stage = Stage::Code;
        let _ = transition(root, &mut state, Stage::Validate);

        assert_eq!(state.consecutive_failures, 2);
    }

    /// 20e Task 1: the tracer end-to-end case — a phase parked at
    /// `Stage::Ship` with a gate and its approved answer, no claim, and no live
    /// process waiting. `ship_override` must reach the SAME terminal state
    /// `finish_workflow` produces: state cleared, gate, answer and claim files gone,
    /// `workflow_finished` emitted.
    #[test]
    fn ship_override_advances_via_written_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(90);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let branch_created = devflow_core::test_support::git_command(root)
            .args(["branch", &branch, "develop"])
            .status()
            .unwrap()
            .success();
        assert!(branch_created);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        Gates::open_gate(root, phase, Stage::Ship, "Ship complete — approve merge?").unwrap();
        answer_open_gate(
            root,
            phase,
            Stage::Ship,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();

        ship_override(root, phase, false).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(gate_answer_files(root, phase, Stage::Ship).is_empty());
        assert!(gate_claim_files(root, phase, Stage::Ship).is_empty());
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("events recorded for phase");
        assert_eq!(last["event"], "workflow_finished");
    }

    /// WR-02 (phase 20 review): `ship_override` runs in the FOREGROUND CLI,
    /// unlike every other `finish_workflow` caller (which runs inside a
    /// detached monitor). A terminal-hook failure (merge conflict) reopens
    /// the Ship gate; with no response ever written for the REOPENED gate,
    /// this must fail fast within `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`
    /// (bounded here to a couple seconds) rather than block the caller's
    /// shell for `DEVFLOW_GATE_TIMEOUT_SECS`' multi-day production default —
    /// which is left untouched, proving the two timeouts are genuinely
    /// independent knobs.
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn ship_override_bounds_foreground_wait_on_terminal_hook_failure() {
        let _guard = env_lock();
        let original_foreground_timeout = std::env::var_os("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS");
        // SAFETY: serialized under ENV_MUTEX. Bounds ONLY the foreground
        // knob — DEVFLOW_GATE_TIMEOUT_SECS (the background default) is
        // never touched by this test, so a regression that made
        // `ship_override` fall back to the multi-day default would hang
        // this test instead of silently passing.
        unsafe {
            std::env::set_var("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS", "2");
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let git = |args: &[&str]| {
            let output = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        let phase = PhaseId::new(96);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        git(&["checkout", "-q", "-b", &branch]);
        std::fs::write(root.join("conflict.txt"), "feature\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "feature change"]);
        git(&["checkout", "-q", "develop"]);
        std::fs::write(root.join("conflict.txt"), "develop\n").unwrap();
        git(&["add", "conflict.txt"]);
        git(&["commit", "-q", "-m", "develop change"]);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        Gates::open_gate(root, phase, Stage::Ship, "Ship complete — approve merge?").unwrap();
        answer_open_gate(
            root,
            phase,
            Stage::Ship,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();

        let started = std::time::Instant::now();
        let result = ship_override(root, phase, false);
        let elapsed = started.elapsed();

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_foreground_timeout {
                Some(value) => {
                    std::env::set_var("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS", value);
                }
                None => std::env::remove_var("DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS"),
            }
        }

        assert!(
            result.is_err(),
            "an unresolved reopened Ship gate must fail closed, not silently advance"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "the foreground wait must be bounded by DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS, \
             not gate_timeout_secs' multi-day default — took {elapsed:?}"
        );
        assert!(
            Gates::gate_path(root, phase, Stage::Ship).exists(),
            "the merge failure must reopen an actionable Ship gate for a human, not silently \
             drop the phase"
        );
        assert!(
            workflow::load_state(root, phase).is_ok(),
            "state must NOT be cleared — finish_workflow_with_gate_timeout must fail before \
             reaching workflow::clear_state"
        );
    }

    /// 20e Task 3: an `Abort`-routing Ship response (a rejection whose note
    /// says "abort") must reach the SAME shared `abort` helper the live
    /// `handle_ship_outcome` path uses — no special-cased branch inside
    /// `ship_override`. Asserts the shared path's own effects: state
    /// cleared, gate files cleaned up, `workflow_aborted` emitted.
    #[test]
    fn ship_override_abort_routes_through_abort() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(95);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        Gates::open_gate(root, phase, Stage::Ship, "Ship complete — approve merge?").unwrap();
        answer_open_gate(
            root,
            phase,
            Stage::Ship,
            &GateResponse {
                approved: false,
                note: Some("abort: found a blocking regression during manual review".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();

        ship_override(root, phase, false).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(gate_answer_files(root, phase, Stage::Ship).is_empty());
        assert!(gate_claim_files(root, phase, Stage::Ship).is_empty());
        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("events recorded for phase");
        assert_eq!(last["event"], "workflow_aborted");
    }

    /// 20e Task 2 (D-02, EoP regression): `--force` must never let
    /// `ship_override` reach `finish_workflow` from a non-Ship stage. Checked
    /// for every earlier `Stage` with `--force` both true and false.
    #[test]
    fn ship_override_refuses_when_not_at_ship_stage() {
        for stage in [Stage::Define, Stage::Plan, Stage::Code, Stage::Validate] {
            for force in [true, false] {
                let dir = tempfile::tempdir().unwrap();
                let root = dir.path();
                let phase = PhaseId::new(91);

                let mut state =
                    State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
                state.stage = stage;
                workflow::save_state(&state).unwrap();

                let err = ship_override(root, phase, force).unwrap_err();
                let msg = err.to_string();
                assert!(
                    msg.contains(&stage.to_string()),
                    "error for stage {stage} (force={force}) must name the stage: {msg}"
                );

                // finish_workflow was never reached: state is untouched, not cleared.
                let reloaded = workflow::load_state(root, phase)
                    .expect("state must survive a stage-mismatch refusal, not be cleared");
                assert_eq!(reloaded.stage, stage);
            }
        }
    }

    /// 20e Task 2 (edge-probe 20e/empty): `state.stage == Stage::Ship` but no
    /// Ship gate request/response exists on disk — a dead monitor never
    /// wrote or received one. Must fail-closed with a clear error, both with
    /// `--force` true and false.
    #[test]
    fn ship_override_refuses_when_no_response_written() {
        for force in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(92);

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();

            // Neither request nor response written yet.
            let err = ship_override(root, phase, force).unwrap_err();
            assert!(
                err.to_string().contains("no Ship gate response"),
                "force={force}: {err}"
            );

            // A request written but no response yet is the same fail-closed case.
            Gates::open_gate(root, phase, Stage::Ship, "ctx").unwrap();
            let err = ship_override(root, phase, force).unwrap_err();
            assert!(
                err.to_string().contains("no Ship gate response"),
                "force={force}: {err}"
            );
            let _ = Gates::close(root, phase, Stage::Ship);
        }
    }

    /// 20e Task 2 (review: Codex HIGH — lock race): a contended per-phase
    /// lock refuses fail-closed, naming the holder pid, with `--force` both
    /// true and false — `ship_override` must never race a live holder's
    /// gate wait.
    #[test]
    fn ship_override_refuses_when_lock_contended() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(93);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let _held = lock::acquire(root, phase).expect("hold the per-phase lock");
        let this_pid = std::process::id().to_string();

        for force in [true, false] {
            let err = ship_override(root, phase, force).unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains(&this_pid), "force={force}: {msg}");
        }
    }

    /// 20e Task 2 (review: Hermes ack-race): a Ship answer that already has a claim
    /// was already consumed, by a monitor that may have died mid-finalization.
    /// `ship_override` refuses and directs to `devflow doctor` rather than re-running
    /// terminal hooks, with `--force` both true and false. `finish_workflow` must never
    /// be reached: state stays intact.
    #[test]
    fn ship_override_refuses_when_the_answer_was_already_claimed() {
        for force in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(94);

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Ship;
            workflow::save_state(&state).unwrap();

            let gate = Gates::open_gate(root, phase, Stage::Ship, "ctx")
                .unwrap()
                .gate_id;
            answer_open_gate(root, phase, Stage::Ship, &approval()).unwrap();
            assert_eq!(
                Gates::claim(root, phase, Stage::Ship, &gate).unwrap(),
                gates::Claim::Claimed
            );

            let err = ship_override(root, phase, force).unwrap_err();
            assert!(
                err.to_string().contains("devflow doctor"),
                "force={force}: {err}"
            );

            let reloaded = workflow::load_state(root, phase)
                .expect("an already-claimed refusal must never clear state");
            assert_eq!(reloaded.stage, Stage::Ship);
        }
    }

    /// R-09: `ship_override` reads and moves nothing. A file in the live gate's slot
    /// that does not bind is refused, named, and left for `devflow gate approve` or
    /// `reject`, which set it aside before publishing a valid answer.
    #[test]
    fn ship_override_refuses_an_answer_that_does_not_bind_and_moves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(98);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();
        let gate = Gates::open_gate(root, phase, Stage::Ship, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(root, phase, Stage::Ship, &gate);
        std::fs::write(&slot, br#"{"approved": true}"#).unwrap();

        let message = ship_override(root, phase, false).unwrap_err().to_string();
        assert!(message.contains("missing_gate_id"), "{message}");
        assert!(message.contains("devflow gate approve"), "{message}");
        assert_eq!(std::fs::read(&slot).unwrap(), br#"{"approved": true}"#);
        assert_eq!(
            gate_claim_files(root, phase, Stage::Ship),
            Vec::<std::path::PathBuf>::new()
        );
        assert_eq!(
            workflow::load_state(root, phase).unwrap().stage,
            Stage::Ship
        );
    }

    /// A phase parked at Stage::Ship in a repo whose terminal hooks can finish it.
    fn ship_ready_fixture(root: &Path, phase: PhaseId) {
        init_repo(root);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let branch_created = devflow_core::test_support::git_command(root)
            .args(["branch", &branch, "develop"])
            .status()
            .unwrap()
            .success();
        assert!(branch_created);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();
    }

    /// D-17 (V-31): `devflow ship` refuses while another process holds the phase, before
    /// it reads or claims the answer, and writes nothing.
    #[test]
    fn ship_override_refuses_while_another_process_holds_the_phase() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2906);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();
        let gate = Gates::open_gate(root, phase, Stage::Ship, "ctx")
            .unwrap()
            .gate_id;
        answer_open_gate(root, phase, Stage::Ship, &approval()).unwrap();
        let _hold = devflow_core::liveness::claim_phase_holder(root, phase).unwrap();
        let before = std::fs::read(workflow::state_path(root, phase)).unwrap();

        let error = ship_override(root, phase, false)
            .expect_err("a held phase refuses")
            .to_string();

        assert!(
            error.contains(&format!("(pid {})", std::process::id())),
            "the refusal names the holder's pid: {error}"
        );
        assert!(
            error.contains(&format!("devflow stop --phase {phase}")),
            "{error}"
        );
        assert!(!error.contains("devflow resume"), "{error}");
        assert!(
            !Gates::claim_path(root, phase, Stage::Ship, &gate).exists(),
            "the answer is not claimed"
        );
        assert_eq!(
            std::fs::read(workflow::state_path(root, phase)).unwrap(),
            before,
            "nothing is written"
        );
    }

    /// The writer and the reader of a Ship approval agree: what `devflow gate approve`
    /// publishes is what `devflow ship` finds, claims and finishes on.
    #[test]
    fn ship_override_ships_an_approval_written_by_gate_approve() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(99);
        ship_ready_fixture(root, phase);
        Gates::open_gate(root, phase, Stage::Ship, "Ship complete — approve merge?").unwrap();

        crate::commands::gate_respond(root, phase, Some(Stage::Ship), true, None, None).unwrap();
        ship_override(root, phase, false).unwrap();

        assert!(matches!(
            workflow::load_state(root, phase),
            Err(workflow::WorkflowError::MissingState(_))
        ));
        assert_eq!(events_named(root, "workflow_shipped").len(), 1);
    }

    /// D-04's consumption claim (V-21), as a defence-in-depth backstop (R2-11). This
    /// waiter is an in-process gate wait with no supervisor record, a state 50.1-29's
    /// R-17 guard and D-17's holder leave no production path to; the test pins that the
    /// claim alone still lets only one of the two consumers act on one approval, if a
    /// waiter exists that neither guard sees.
    #[test]
    fn ship_override_and_a_live_waiter_cannot_both_consume_one_approval() {
        use std::sync::atomic::Ordering::SeqCst;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(97);
        ship_ready_fixture(root, phase);
        let probe = std::sync::Arc::new(ClaimProbe::default());

        let waiter = {
            let root = root.to_path_buf();
            let probe = probe.clone();
            std::thread::spawn(move || {
                CLAIM_PROBE.set(Some(probe));
                let mut state = workflow::load_state(&root, phase).unwrap();
                // What `handle_ship_outcome` does with an approval.
                match run_gate_with_timeout(&root, &mut state, Stage::Ship, "ctx", 30, None)? {
                    GateAction::Advance => finish_workflow_with_gate_timeout(&root, &mut state, 2),
                    other => panic!("the approval must read as an advance: {other:?}"),
                }
            })
        };
        wait_for_open_gate(root, phase, Stage::Ship);
        answer_open_gate(root, phase, Stage::Ship, &approval()).unwrap();
        let shipper = {
            let root = root.to_path_buf();
            let probe = probe.clone();
            std::thread::spawn(move || {
                CLAIM_PROBE.set(Some(probe));
                ship_override(&root, phase, false)
            })
        };
        let waited = waiter.join().unwrap();
        let shipped = shipper.join().unwrap();

        assert_eq!(
            (
                probe.claimed.load(SeqCst),
                probe.already_consumed.load(SeqCst)
            ),
            (1, 1),
            "exactly one consumer claims: waiter {waited:?}, ship {shipped:?}"
        );
        assert_eq!(
            events_named(root, "workflow_shipped").len(),
            1,
            "finalization ran once: waiter {waited:?}, ship {shipped:?}"
        );
        let refused = match (&waited, &shipped) {
            (Ok(()), Err(refused)) | (Err(refused), Ok(())) => refused,
            _ => panic!("exactly one consumer acts: waiter {waited:?}, ship {shipped:?}"),
        };
        assert!(
            refused.to_string().contains("consumed"),
            "the other consumer says the answer was consumed: {refused}"
        );
    }

    /// 18d concurrency edge: two concurrently-active phases' `consecutive_failures`
    /// counters are independent — a Code→Validate hop on one phase must not
    /// reset a sibling phase's counter.
    #[test]
    fn consecutive_failures_are_independent_across_phases() {
        const NAME: &str =
            "pipeline_gate::tests::consecutive_failures_are_independent_across_phases";
        enter_path_isolated_child!(NAME, agent_free_git_only_path_dir());
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut state_a = State::new(
            PhaseId::new(84),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        state_a.stage = Stage::Code;
        state_a.consecutive_failures = 1;
        workflow::save_state(&state_a).unwrap();

        let mut state_b = State::new(
            PhaseId::new(85),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        state_b.stage = Stage::Code;
        state_b.consecutive_failures = 2;
        workflow::save_state(&state_b).unwrap();

        let _ = transition(root, &mut state_a, Stage::Validate);

        let reloaded_a = workflow::load_state(root, PhaseId::new(84)).unwrap();
        let reloaded_b = workflow::load_state(root, PhaseId::new(85)).unwrap();

        assert_eq!(
            reloaded_a.consecutive_failures, 1,
            "the Code->Validate hop must not reset consecutive_failures"
        );
        assert_eq!(
            reloaded_b.consecutive_failures, 2,
            "an untouched sibling phase's counter must be unaffected"
        );
    }

    /// WR-10 (49-REVIEW.md): an edit to the run's notify command made on disk
    /// while a gate waits survives the gate's resolution save.
    #[test]
    fn gate_resolution_keeps_a_notify_cmd_edited_during_the_wait() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(23);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();

        let root_owned = root.to_path_buf();
        let handle = std::thread::spawn(move || {
            let mut state = workflow::load_state(&root_owned, phase).unwrap();
            run_gate_with_timeout(&root_owned, &mut state, Stage::Code, "ctx", 30, None).unwrap();
            state.gate_notify_cmd
        });
        let gate_path = Gates::gate_path(root, phase, Stage::Code);
        for _ in 0..500 {
            if gate_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(gate_path.exists(), "the gate must be written");

        let mut on_disk = workflow::load_state(root, phase).unwrap();
        on_disk.gate_notify_cmd = Some(": edited".to_string());
        workflow::save_state(&on_disk).unwrap();
        answer_open_gate(
            root,
            phase,
            Stage::Code,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();

        let in_memory = handle.join().unwrap();
        assert_eq!(in_memory.as_deref(), Some(": edited"));
        assert_eq!(
            workflow::load_state(root, phase)
                .unwrap()
                .gate_notify_cmd
                .as_deref(),
            Some(": edited"),
            "the resolution save must not overwrite the on-disk edit"
        );
    }

    /// Runs one waiter on a saved state and answers its gate after `during_wait`.
    /// Returns what `during_wait` returned, once the waiter has finished.
    fn with_a_waiter_at_its_gate<T>(
        root: &Path,
        phase: PhaseId,
        hold_across_wait: bool,
        during_wait: impl FnOnce() -> T,
    ) -> T {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();

        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let root_owned = root.to_path_buf();
        let waiter = std::thread::spawn(move || {
            WAIT_HOLDS_WRITE_LOCK.set(hold_across_wait);
            WAIT_ENTERED.set(Some(entered_tx));
            let mut state = workflow::load_state(&root_owned, phase).unwrap();
            run_gate_with_timeout(&root_owned, &mut state, Stage::Code, "ctx", 30, None).map(|_| ())
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the waiter reaches its gate wait");
        assert!(
            Gates::gate_path(root, phase, Stage::Code).exists(),
            "the gate is open while the waiter waits"
        );

        let seen = during_wait();
        assert!(
            !waiter.is_finished(),
            "what ran during the wait ran with the waiter still inside it"
        );

        answer_open_gate(
            root,
            phase,
            Stage::Code,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        waiter.join().unwrap().unwrap();
        seen
    }

    /// D-08's lost update (V-13): a `stop` written while a gate waits survives the
    /// write the waiter makes after its wait, because that write re-reads the state
    /// and sets only `gate_pending`.
    #[test]
    fn a_stop_written_during_a_gate_wait_survives_the_waiters_write() {
        const STOP_REASON: &str = "stopped via `devflow stop`";
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(86);

        with_a_waiter_at_its_gate(root, phase, false, || {
            workflow::update_state(root, phase, |state| {
                state.stopped = true;
                state.stop_reason = Some(STOP_REASON.to_string());
                Ok::<(), workflow::WorkflowError>(())
            })
            .unwrap();
        });

        let after = workflow::load_state(root, phase).unwrap();
        assert!(after.stopped, "the stop survives the waiter's write");
        assert_eq!(after.stop_reason.as_deref(), Some(STOP_REASON));
        assert!(!after.gate_pending, "the waiter's own write landed");
    }

    /// Three write-lock probes, 100 ms apart, taken while a waiter is inside its gate
    /// wait. `true` means free. `hold_across_wait` selects V-12's control.
    fn in_wait_write_lock_probes(hold_across_wait: bool) -> Vec<bool> {
        const PROBES: usize = 3;
        const PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(87);
        with_a_waiter_at_its_gate(root, phase, hold_across_wait, || {
            (0..PROBES)
                .map(|probe| {
                    if probe > 0 {
                        std::thread::sleep(PROBE_INTERVAL);
                    }
                    workflow::write_lock_is_free(root, phase)
                })
                .collect()
        })
    }

    /// The assertion V-12 makes, kept apart so its control can show it fails.
    fn no_probe_saw_the_lock_held(probes: &[bool]) -> Result<(), String> {
        if probes.iter().all(|free| *free) {
            Ok(())
        } else {
            Err(format!(
                "the write lock read held during a gate wait: {probes:?}"
            ))
        }
    }

    /// Criterion 3 (V-12): no gate wait holds the write lock, so a `stop` or a
    /// `gate respond` is never blocked for the length of a wait. The control runs the
    /// same waiter with the wait call inside an `update_state_with` closure, and the
    /// same probes then read held, so the check the production path passes fails on
    /// a waiter that holds the lock.
    #[test]
    fn a_gate_wait_never_holds_the_write_lock() {
        let production = in_wait_write_lock_probes(false);
        println!("production in-wait probes (true = free): {production:?}");
        assert_eq!(no_probe_saw_the_lock_held(&production), Ok(()));

        let control = in_wait_write_lock_probes(true);
        println!("control in-wait probes (true = free): {control:?}");
        assert_eq!(
            control,
            vec![false, false, false],
            "control: every probe sees the lock the waiter holds"
        );
        assert!(
            no_probe_saw_the_lock_held(&control).is_err(),
            "control: the check fails on a waiter that holds the lock"
        );
    }

    /// A gate wait on another thread, on a saved Code-stage state.
    fn spawn_code_gate_waiter(
        root: &Path,
        phase: PhaseId,
    ) -> std::thread::JoinHandle<Result<GateAction, CliError>> {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();
        let root_owned = root.to_path_buf();
        std::thread::spawn(move || {
            let mut state = workflow::load_state(&root_owned, phase).unwrap();
            run_gate_with_timeout(&root_owned, &mut state, Stage::Code, "ctx", 30, None)
        })
    }

    /// Wait for the stage's gate file, then return the id it names.
    fn wait_for_open_gate(root: &Path, phase: PhaseId, stage: Stage) -> gates::GateId {
        const GATE_OPENS_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);
        let started = std::time::Instant::now();
        while !Gates::gate_path(root, phase, stage).exists() {
            assert!(
                started.elapsed() < GATE_OPENS_WITHIN,
                "the {stage} gate did not open within {GATE_OPENS_WITHIN:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        live_gate_id(root, phase, stage)
    }

    fn events_named(root: &Path, name: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(devflow_core::events::events_path(root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["event"] == name)
            .collect()
    }

    fn approval() -> GateResponse {
        GateResponse {
            approved: true,
            note: None,
            responded_by: Some("test".into()),
        }
    }

    /// D-05, waiter half (V-15 CLI): an answer with no gate id, written to the live
    /// gate's own slot, is set aside with an event and the wait goes on. A valid answer
    /// then decides the gate through the claim, and the registration is gone.
    #[test]
    fn a_gate_wait_quarantines_an_answer_without_a_gate_id_and_accepts_a_valid_one() {
        const SET_ASIDE_WITHIN: std::time::Duration = std::time::Duration::from_secs(20);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(88);
        let waiter = spawn_code_gate_waiter(root, phase);
        let gate = wait_for_open_gate(root, phase, Stage::Code);

        // Renamed into place, so the waiter never reads a partly written file and the
        // reason below is the missing id alone.
        let slot = Gates::answer_path(root, phase, Stage::Code, &gate);
        let partial = root.join("idless-answer");
        std::fs::write(&partial, br#"{"approved": true}"#).unwrap();
        std::fs::rename(&partial, &slot).unwrap();

        let started = std::time::Instant::now();
        while slot.exists() {
            assert!(
                started.elapsed() < SET_ASIDE_WITHIN,
                "the wait must move an answer with no gate id out of its slot"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let quarantined = events_named(root, "gate_answer_quarantined");
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(quarantined[0]["reason"], "missing_gate_id");
        assert!(
            !waiter.is_finished(),
            "an answer with no gate id does not end the wait"
        );
        assert!(events_named(root, "gate_resolved").is_empty());

        answer_open_gate(root, phase, Stage::Code, &approval()).unwrap();
        let action = waiter.join().unwrap().unwrap();
        assert_eq!(action, GateAction::Advance);
        assert!(
            Gates::claim_path(root, phase, Stage::Code, &gate).exists(),
            "the answer was consumed through the claim"
        );
        assert_eq!(
            Gates::waiter_status(root, phase, Stage::Code, &gate).unwrap(),
            gates::WaiterStatus::Absent
        );
    }

    /// D-04: a gate's waiter is registered from the first moment the gate file can be
    /// seen until the wait ends. This positive check cannot catch a reordering
    /// deterministically; 50.1-08's
    /// `a_writer_answering_the_instant_the_gate_file_appears_finds_its_waiter_registered`
    /// and its control do.
    #[test]
    fn a_gate_wait_registers_its_waiter_for_the_whole_wait() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(89);
        let waiter = spawn_code_gate_waiter(root, phase);
        let gate = wait_for_open_gate(root, phase, Stage::Code);
        assert_eq!(
            Gates::waiter_status(root, phase, Stage::Code, &gate).unwrap(),
            gates::WaiterStatus::Live,
            "registered as soon as the gate file is visible"
        );

        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!waiter.is_finished());
        assert_eq!(
            Gates::waiter_status(root, phase, Stage::Code, &gate).unwrap(),
            gates::WaiterStatus::Live,
            "registered while it waits"
        );

        answer_open_gate(root, phase, Stage::Code, &approval()).unwrap();
        waiter.join().unwrap().unwrap();
        assert_eq!(
            Gates::waiter_status(root, phase, Stage::Code, &gate).unwrap(),
            gates::WaiterStatus::Absent,
            "not registered once the wait is over"
        );
    }

    /// D-04: when the claim for the live gate already exists as the answer arrives,
    /// another consumer acted on it, and this waiter does nothing but say so.
    #[test]
    fn a_waiter_whose_answer_was_consumed_elsewhere_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(85);
        let waiter = spawn_code_gate_waiter(root, phase);
        let gate = wait_for_open_gate(root, phase, Stage::Code);

        assert_eq!(
            Gates::claim(root, phase, Stage::Code, &gate).unwrap(),
            gates::Claim::Claimed
        );
        answer_open_gate(root, phase, Stage::Code, &approval()).unwrap();

        let err = waiter
            .join()
            .unwrap()
            .expect_err("a consumed answer must not decide this waiter's gate");
        let message = err.to_string();
        assert!(message.contains("another consumer"), "{message}");
        assert!(message.contains(gate.as_str()), "{message}");
        assert!(events_named(root, "gate_resolved").is_empty());
        assert!(
            workflow::load_state(root, phase).unwrap().gate_pending,
            "the waiter recorded nothing about a decision it did not take"
        );
        assert_eq!(
            Gates::waiter_status(root, phase, Stage::Code, &gate).unwrap(),
            gates::WaiterStatus::Absent
        );
    }

    /// R-09, wait arm: a set-aside acts only while the gate file names this gate. When
    /// the gate was replaced or closed, the wait ends with that error and the file
    /// stays where it is.
    #[test]
    fn a_wait_whose_gate_changed_ends_with_the_error_and_moves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(84);
        let old = Gates::open_gate(root, phase, Stage::Code, "g1").unwrap();
        Gates::open_gate(root, phase, Stage::Code, "g2").unwrap();
        let slot = Gates::answer_path(root, phase, Stage::Code, &old.gate_id);
        let idless = br#"{"approved": true}"#;
        std::fs::write(&slot, idless).unwrap();

        let assert_refused = |refusal: &str| {
            let err = crate::pipeline_launch::wait_for_gate_response(
                root,
                phase,
                Stage::Code,
                &old.gate_id,
                5,
            )
            .expect_err("a set-aside the gate refuses ends the wait");
            let message = err.to_string();
            assert!(message.contains(old.gate_id.as_str()), "{message}");
            assert!(message.contains("was not set aside"), "{message}");
            assert!(message.contains(refusal), "{message}");
            assert_eq!(std::fs::read(&slot).unwrap(), idless);
            assert_eq!(Gates::quarantined(root), Vec::new());
        };

        assert_refused(old.gate_id.as_str());
        std::fs::remove_file(Gates::gate_path(root, phase, Stage::Code)).unwrap();
        assert_refused("no open gate");
    }

    /// A `yes_ship` revoke made after a caller loaded its copy survives that caller's
    /// transition, which writes only the transition's own fields (D-08, T-50.1-34).
    #[test]
    fn transition_commits_only_its_fields() {
        const NAME: &str = "pipeline_gate::tests::transition_commits_only_its_fields";
        enter_path_isolated_child!(NAME, agent_free_git_only_path_dir());
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(88);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.yes_ship = false;
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        // The launch this ends in fails on the agent-free PATH, which is not this
        // test's subject.
        let _ = transition(root, &mut state, Stage::Validate);

        let after = workflow::load_state(root, phase).unwrap();
        assert!(!after.yes_ship, "the revoke survives the transition");
        assert_eq!(after.stage, Stage::Validate, "the transition's own write");
    }

    /// A loop-back writes its stage move and the verification observation
    /// `select_loop_back_fix` recorded (F-11: this is the write that persists it), and
    /// keeps a field another writer changed since the caller loaded its copy.
    #[test]
    fn a_loop_back_commits_its_fields_and_the_verification_observation() {
        const FINGERPRINT: u64 = 0xF1;
        const MTIME_NANOS: u64 = 0xF2;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(89);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.gate_pending = true;
        state.yes_ship = true;
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.yes_ship = false;
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        state.last_verification_fingerprint = Some(FINGERPRINT);
        state.last_verification_mtime_nanos = Some(MTIME_NANOS);

        prepare_loop_back_to_code(
            root,
            &mut state,
            FixType::GapsOnly,
            LoopBackReason::GateResponse,
        )
        .unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert!(!after.yes_ship, "the concurrent revoke is kept");
        assert_eq!(after.stage, Stage::Code);
        assert!(!after.gate_pending);
        assert_eq!(after.last_verification_fingerprint, Some(FINGERPRINT));
        assert_eq!(after.last_verification_mtime_nanos, Some(MTIME_NANOS));
    }
}
