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
//! run_gate/finish_run) → pipeline_launch (launch_stage)` back to
//! where it started. This cycle is the state machine's real control flow
//! (Code → Validate, with loop-backs), and Rust permits cyclic
//! module references — only the crate dependency graph must be acyclic —
//! so this compiles cleanly. **A future change to pipeline logic is likely
//! to touch two or three of these files together:** the split buys
//! `pub(crate)` boundaries, reviewability, and wave independence for the
//! *other* clusters, not pipeline-internal parallelism (19-RESEARCH.md
//! Pitfall 1).

use crate::CliError;
use crate::config_parse::gate_timeout_secs;
use crate::pipeline_launch::{
    GateWaitEnd, clear_run_state, close_gate, commit, ensure_still_recorded, fenced, is_not_owner,
    launch_stage,
};
use crate::pipeline_outcomes::{Answer, FinishPermit, truncate_reason};
#[cfg(test)]
use devflow_core::gates::GateId;
use devflow_core::gates::{self, Claim, GateAction, GateFile, GateLabel, Gates};
use devflow_core::mode;
#[cfg(test)]
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt::{FixType, ReverifyReport};
use devflow_core::stage::Stage;
use devflow_core::state::State;
#[cfg(test)]
use devflow_core::workflow;
use devflow_core::{events, registry};
use std::path::Path;
use tracing::info;

/// Persist the new stage `to` and launch its agent.
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
    // (review: Codex HIGH off-by-one). This bypasses `state.stage = to`, the normal
    // `"transition"` event, and
    // `launch_stage` — no new monitor is spawned, and `loop_back_to_code`
    // (a retry, not an advance) is untouched by this check.
    //
    // 52-27 (orchestrator decision, reversible): never after Validate. It is
    // the last stage, so its pass ends through `finish_run`, and its only
    // transition is the gap-closure route to Plan (GATE-03), which runs under
    // `--until validate` as the loop-back to Code does: `--until validate`
    // halts nothing.
    if state.stop_until == Some(from) && from != Stage::Validate {
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
            // D-09: a run halted after its stage is no longer mid-fix, and the
            // fix pass's note goes with it.
            s.code_fix = None;
            s.code_fix_note = None;
            s.code_fix_report = None;
            // U7: the run leaves the stage, so the stage's relaunch note goes.
            s.relaunch_note = None;
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
        // D-06/D-29: the halt ends the run too, so it notifies once, after
        // `workflow_finished`. `finish_run` is the other end site; one run end
        // reaches only one of them.
        notify_run_end(
            project_root,
            state,
            from,
            "stopped_at",
            &format!(
                "phase {} stopped after {from} completed (--until {from})",
                state.phase
            ),
        );
        return Ok(());
    }

    ensure_still_recorded(project_root, state.phase)?;
    commit(state, |s| {
        s.stage = to;
        // GATE-03 / D-09: the move from Validate to Plan is the gap-closure
        // route, the only way out of Validate through here, so it starts a
        // gap-closure pass. Nothing in this function clears the marker.
        if (from, to) == (Stage::Validate, Stage::Plan) {
            s.gap_closure_pass = true;
        }
        if mode::transition_resets_consecutive_failures(from, to, s.gap_closure_pass) {
            s.consecutive_failures = 0;
        }
        s.infra_failures = 0;
        s.gate_pending = false;
        // D-09: a fix type lives exactly as long as its Code pass, so a later
        // plain Code pass never replays a stale one. Its note goes with it.
        s.code_fix = None;
        s.code_fix_note = None;
        s.code_fix_report = None;
        // U7: a relaunch note is for the stage being left, never the next one.
        s.relaunch_note = None;
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

/// The stop reason a run records when its Validate passes (D-35).
pub(crate) const FINISHED_STOP_REASON: &str =
    "finished: Validate passed; the worktree and branch are kept";

/// End the run after a passing Validate (CUT-02; D-04, D-05, D-29, D-35).
/// Validate is the last stage, so this is how every run that passes it ends:
/// in auto mode on the pass, in supervise mode when the Validate gate is
/// approved.
///
/// The state is kept, recorded `stopped` and `finished` with
/// [`FINISHED_STOP_REASON`], and the supervisor record and `gate_pending` are
/// cleared, as `transition`'s `--until` halt does, so `doctor` never reads the
/// finished run as a crashed one. The phase leaves the machine-global gate
/// registry. Nothing else is touched: no git call, no hook, no merge, push,
/// tag or branch deletion, so the operator gets the tree as Validate left it.
///
/// It takes a [`FinishPermit`] by value, which only
/// [`crate::pipeline_outcomes::can_finish`] makes (D-45 rule 1), so no run is
/// recorded finished without a fresh `passed` and a Validate pass on HEAD.
///
/// It emits `workflow_finished` with `{"reason":"finished","shipped":false}`,
/// then fires the notify command once and records `notify_fired`. It does not
/// read `stop_until`: with `--until validate` a passing Validate is a finish,
/// not a halt, because Validate is the last stage.
pub(crate) fn finish_run(
    project_root: &Path,
    state: &mut State,
    permit: FinishPermit,
) -> Result<(), CliError> {
    // D-45 rule 1: the permit is the proof `can_finish` held; it is spent here.
    let _ = permit;
    // Operator decision (2026-11-11): a permitted finish wins over a late
    // stop. A stop taken during the advance that reached this finish (after
    // the Validate gate was approved, or while an auto pass advanced) is not
    // honoured here: the run is recorded finished, and the supervisor answers
    // that stop `already_finished` (`run_monitor` marks the run finished), so
    // `devflow stop` reports the finish, never a hand-off. A stop that lands
    // where no finish is permitted keeps its handling at the next safe point.
    let _ = close_gate(project_root, state.phase, state.stage);
    commit(state, |s| {
        s.stopped = true;
        s.finished = true;
        s.stop_reason = Some(FINISHED_STOP_REASON.to_string());
        s.supervisor = None;
        s.gate_pending = false;
        s.code_fix = None;
        s.code_fix_note = None;
        s.code_fix_report = None;
        s.relaunch_note = None;
        s.gap_closure_pass = false;
    })?;
    registry::deregister(project_root, state.phase);
    events::emit(
        project_root,
        state.phase,
        "workflow_finished",
        serde_json::json!({ "reason": "finished", "shipped": false }),
    );
    notify_run_end(
        project_root,
        state,
        Stage::Validate,
        "finished",
        &format!(
            "phase {} finished: Validate passed; the worktree and branch are kept",
            state.phase
        ),
    );
    println!(
        "phase {} finished — Validate passed; the worktree and branch are kept",
        state.phase
    );
    Ok(())
}

/// Fire the notify command once for a run that has ended at `stage`, then
/// record `notify_fired` with the end's `reason` (D-06, D-29). Called after
/// `workflow_finished`, by each site that emits it.
fn notify_run_end(project_root: &Path, state: &State, stage: Stage, reason: &str, context: &str) {
    gates::fire_gate_notify_with_fallback(
        state.phase,
        stage,
        context,
        false,
        state.gate_notify_cmd.as_deref(),
    );
    events::emit(
        project_root,
        state.phase,
        "notify_fired",
        serde_json::json!({ "stage": stage.to_string(), "unexpected": false, "reason": reason }),
    );
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
    /// A human answered a gate with a loop-back — an ambiguous Validate
    /// adjudication. Makes no
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

/// Loop the pipeline back to Code with the given fix prompt (`GapsOnly`,
/// `FullExecute` or `StaleReverify` for a Validate rejection). `note` is the
/// operator's note on the rejected gate that asked for the fix (GATE-01), `None`
/// for an unattended loop-back or an approval: the fix pass carries it to its
/// agent. `reverify_report` is what GSD said when the fix is `StaleReverify`
/// (D-32 part 1); it is ignored for every other fix.
pub(crate) fn loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
    note: Option<String>,
    reverify_report: Option<ReverifyReport>,
    reason: LoopBackReason,
) -> Result<(), CliError> {
    let from = state.stage;
    let prompt =
        prepare_loop_back_to_code(project_root, state, fix, note, reverify_report, reason)?;
    launch_stage(state, Some(prompt), Some(from))
}

/// The state-mutating half of `loop_back_to_code`, split out so it's
/// unit-testable without spawning a real agent process (`launch_stage`
/// invokes the actual configured agent CLI). Cleans up the stale gate for
/// the stage the gate fired on (CR-01), moves `state` to Code, persists it,
/// and returns the fix prompt the caller should launch with. The fix type and
/// its `note` are persisted in one commit (`code_fix`, `code_fix_note`), so a
/// replay of the fix pass renders the same note.
pub(crate) fn prepare_loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
    note: Option<String>,
    reverify_report: Option<ReverifyReport>,
    reason: LoopBackReason,
) -> Result<String, CliError> {
    // Only the re-verify pass quotes GSD, so only it keeps the report.
    let reverify_report = reverify_report.filter(|_| fix == FixType::StaleReverify);
    // Capture the stage the gate actually fired on before it's mutated below,
    // so the close targets the right stage's gate files (see CR-01).
    let gate_stage = state.stage;
    let _ = close_gate(project_root, state.phase, gate_stage);
    commit(state, |s| {
        s.stage = Stage::Code;
        s.gate_pending = false;
        // D-09: the fix pass this Code entry starts, so a same-stage relaunch
        // (failover, resume, re-drive) replays it rather than plain execute.
        s.code_fix = Some(fix);
        s.code_fix_note = note.clone();
        // D-32 part 1: GSD's words for the re-verify pass, in the same commit.
        s.code_fix_report = reverify_report.clone();
        // GATE-03: another fix route replaces a gap-closure pass, in the same
        // commit; a gaps-only loop-back continues it.
        if fix != FixType::GapsOnly {
            s.gap_closure_pass = false;
        }
        // U7: this leaves Validate without `transition`, so it clears the
        // relaunch note itself; the prompt below is built with none, so the
        // prompt and the persisted state agree.
        s.relaunch_note = None;
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
            fix_note: note,
            relaunch_note: None,
            reverify_report,
        },
    ))
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
pub(crate) struct ClaimProbe {
    pub(crate) claimed: std::sync::atomic::AtomicUsize,
    pub(crate) already_consumed: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
thread_local! {
    /// Set on a consumer's thread to count its claim, see [`note_claim`].
    pub(crate) static CLAIM_PROBE: std::cell::RefCell<Option<std::sync::Arc<ClaimProbe>>> =
        const { std::cell::RefCell::new(None) };
}

/// Count a consumer's claim outcome and hold it until the other consumer has claimed
/// too, with a bound. Without the hold the winner acts at once and closes the gate, and
/// the loser meets a closed gate rather than the other's claim. Does nothing on a
/// thread with no probe installed.
#[cfg(test)]
fn note_claim(claim: &Result<Claim, devflow_core::gates::GateError>) {
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
    kind: impl Into<GateLabel>,
    context: &str,
) -> Result<Answer, CliError> {
    run_gate_with_timeout(
        project_root,
        state,
        stage,
        kind,
        context,
        gate_timeout_secs(),
    )
}

/// [`run_gate`] whose `gate_fired` event carries `event_context` instead of the
/// gate's `context`, for a gate whose context holds text an event must not
/// carry: GSD's message or git's stderr, which can name absolute paths
/// (52-22, Pitfall 11). The gate file and the notify command still get
/// `context`.
pub(crate) fn run_gate_with_event_context(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    kind: impl Into<GateLabel>,
    context: &str,
    event_context: &str,
) -> Result<Answer, CliError> {
    open_and_await(
        project_root,
        state,
        stage,
        kind.into(),
        (context, event_context),
        gate_timeout_secs(),
    )
}

/// `run_gate`'s body, parameterized on the wait's timeout (WR-02, phase 20
/// review) so a caller can pass a bounded timeout instead of
/// [`gate_timeout_secs`]'s multi-day production default.
///
/// The bound protocol (D-04, D-05): a fresh gate id with this waiter registered before
/// the gate file is published, that registration held for the whole wait, a wait for
/// that id's answer with no write lock held, and one first-wins claim before anything
/// acts on the answer.
pub(crate) fn run_gate_with_timeout(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    kind: impl Into<GateLabel>,
    context: &str,
    timeout_secs: u64,
) -> Result<Answer, CliError> {
    open_and_await(
        project_root,
        state,
        stage,
        kind.into(),
        (context, context),
        timeout_secs,
    )
}

/// The body [`run_gate_with_timeout`] and [`run_gate_with_event_context`] share.
/// `contexts` is the gate's context and the one its `gate_fired` event carries.
/// The gate's `label` is required, so no gate opens without a kind (D-39), and
/// it is written by the same atomic write as the rest of the gate file (52-22).
///
/// R10 (52-29): every answer is screened at its pickup, before the claim, the
/// `gate_pending` write and `gate_resolved`, from the gate file and the answer
/// alone ([`crate::pipeline_outcomes::screen_answer`], no GSD read). A refused
/// answer is never claimed: the gate is reopened never-silently with the label
/// this supervisor opened it with (C11), which sets the refused answer aside,
/// `gate_answer_refused` is emitted, and the wait starts again on the reopened
/// gate. Every other answer is claimed and routed, and `gate_resolved` names
/// the route.
fn open_and_await(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    label: GateLabel,
    contexts: (&str, &str),
    timeout_secs: u64,
) -> Result<Answer, CliError> {
    let (question, question_event) = contexts;
    let mut context = question.to_string();
    let mut event_context = question_event.to_string();
    commit(state, |s| s.gate_pending = true)?;
    loop {
        // A crash inside this call before the publish leaves `gate_pending` with no
        // gate file, which `doctor`'s `check_gate_pending_without_gate` reports.
        let (opened, registration) = fenced(project_root, state.phase, || {
            Ok(Gates::open_registered(
                project_root,
                state.phase,
                stage,
                label,
                &context,
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
                "context": event_context,
                "gate_kind": label.kind,
            }),
        );
        gates::fire_gate_notify_with_fallback(
            state.phase,
            stage,
            &context,
            unexpected,
            state.gate_notify_cmd.as_deref(),
        );
        events::emit(
            project_root,
            state.phase,
            "notify_fired",
            serde_json::json!({ "stage": stage.to_string(), "unexpected": unexpected }),
        );
        let wait_end = match gate_wait_call(project_root, state.phase, stage, gate_id, timeout_secs)
        {
            Ok(end) => end,
            Err(err) => {
                registration.finish();
                return Err(err);
            }
        };
        match wait_end {
            GateWaitEnd::Answered(response) => {
                // R10: the gate file as it is on disk names the answer's family.
                let gate_file = read_gate_file(project_root, state.phase, stage);
                let recorded_kind = gate_file.as_ref().and_then(|gate| gate.gate_kind);
                if let Err(refusal) =
                    crate::pipeline_outcomes::screen_answer(recorded_kind, &response)
                {
                    registration.finish();
                    events::emit(
                        project_root,
                        state.phase,
                        "gate_answer_refused",
                        serde_json::json!({
                            "stage": stage.to_string(),
                            "gate_id": gate_id.as_str(),
                            "gate_kind": recorded_kind,
                            "answer": refusal.answer,
                        }),
                    );
                    eprintln!(
                        "phase {}: the {} on the {stage} gate was refused: {}",
                        state.phase, refusal.answer, refusal.why
                    );
                    context = refused_answer_context(&refusal, question);
                    event_context = refused_answer_context(&refusal, question_event);
                    continue;
                }
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
                             another consumer acted on it, so this wait did nothing",
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
                // D-08: whatever changed on disk during the wait (a `stop`, an
                // edited notify command) is kept, because this
                // write re-reads the state and sets only this field.
                commit(state, |s| s.gate_pending = false)?;
                let action = GateAction::from_response(&response);
                // D-45 rule 2: the answer is routed here, after its claim, so
                // `gate_resolved` names the route it runs and the evidence a
                // verification approve was routed on. The screen passed, so the
                // file names a kind.
                let gate_file = gate_file.unwrap_or_else(|| GateFile {
                    phase: state.phase,
                    stage,
                    context: context.clone(),
                    timestamp: String::new(),
                    gate_id: gate_id.to_string(),
                    gate_kind: Some(label.kind),
                    gsd_status: label.gsd_status,
                });
                let answer = crate::pipeline_outcomes::route_answer(
                    project_root,
                    state,
                    &gate_file,
                    &response,
                );
                let mut resolved = serde_json::json!({
                    "stage": stage.to_string(),
                    "gate_id": gate_id.as_str(),
                    "gate_kind": gate_file.gate_kind,
                    "approved": response.approved,
                    "action": match &action {
                        GateAction::Advance => "advance",
                        GateAction::LoopBack { .. } => "loop_back",
                        GateAction::Abort(_) => "abort",
                    },
                    "responded_by": response.responded_by,
                });
                if let (Some(resolved), serde_json::Value::Object(route)) =
                    (resolved.as_object_mut(), answer.resolved_fields())
                {
                    resolved.extend(route);
                }
                events::emit(project_root, state.phase, "gate_resolved", resolved);
                // `gate_pending` reads false here, and the caller has not applied the action.
                #[cfg(test)]
                crate::pipeline_launch::test_hold("after_commit");
                return Ok(answer);
            }
            GateWaitEnd::Stopped => {
                // Safe point 4 (50-13, R-2): the supervisor running this advance
                // was stopped during the wait. The gate has no waiter any more, so
                // it is closed (an unconsumed answer is set aside, nothing outlives
                // the run in its slot), the stop is recorded, and an error unwinds
                // every caller through `?`, which releases a foreground process's holder.
                // Deliberately not `GateAction::Abort`: `abort()` deletes the state
                // file, and a stopped run keeps its state for `devflow resume`.
                let _ = close_gate(project_root, state.phase, stage);
                registration.finish();
                crate::pipeline_launch::record_advance_stop(state, 4)?;
                return Err(CliError::Message(format!(
                    "the gate wait for stage {stage} ended: {}",
                    crate::pipeline_launch::safe_point_stop_reason()
                )));
            }
            GateWaitEnd::TimedOut => {
                registration.finish();
                events::emit(
                    project_root,
                    state.phase,
                    "gate_timeout",
                    serde_json::json!({ "stage": stage.to_string() }),
                );
                return Err(CliError::Message(format!(
                    "gate for stage {stage} timed out awaiting a response"
                )));
            }
        }
    }
}

/// The gate file of `phase`'s `stage` gate as it is on disk, or `None` when it
/// cannot be read or parsed. A file with no kind parses with `gate_kind: None`.
pub(crate) fn read_gate_file(
    project_root: &Path,
    phase: devflow_core::phase_id::PhaseId,
    stage: Stage,
) -> Option<GateFile> {
    let text = std::fs::read_to_string(Gates::gate_path(project_root, phase, stage)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The context of a gate reopened after `refusal` (R10, never silent): what
/// was refused and why, the answers that apply, then the gate's question.
fn refused_answer_context(refusal: &crate::pipeline_outcomes::Refusal, question: &str) -> String {
    format!(
        "[never-silent] The {} was refused and nothing ran: {}. The answers that apply here: \
         {}. The gate asked: {question}",
        refusal.answer, refusal.why, refusal.applies
    )
}

/// Abort the workflow with a reason, clearing state.
pub(crate) fn abort(project_root: &Path, state: &State, reason: &str) -> Result<(), CliError> {
    // See CR-01: an answer for this phase+stage must not outlive the run. An unused
    // one is set aside, never deleted. D-03: a superseded supervisor ends nothing, so its
    // refusal goes up instead of the rest of the abort.
    // A supervisor's stderr is `/dev/null`, so each failure is an event too.
    if let Err(err) = close_gate(project_root, state.phase, state.stage) {
        if is_not_owner(&err) {
            return Err(err);
        }
        eprintln!(
            "warning: phase {} abort could not close its {} gate: {err}",
            state.phase, state.stage
        );
        events::emit(
            project_root,
            state.phase,
            "abort_gate_not_closed",
            serde_json::json!({ "stage": state.stage.to_string(), "error": err.to_string() }),
        );
    }
    if let Err(err) = clear_run_state(project_root, state.phase) {
        if is_not_owner(&err) {
            return Err(err);
        }
        events::emit(
            project_root,
            state.phase,
            "abort_state_not_cleared",
            serde_json::json!({ "error": err.to_string() }),
        );
        return Err(CliError::Message(format!(
            "phase {} abort could not remove its state, so the run is not reported finished: {err}",
            state.phase
        )));
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

/// Print the full pipeline that a `start` would run, without launching anything.
pub(crate) fn print_dry_run(state: &State) {
    println!(
        "dry run — phase {} | agent {} | mode {}",
        state.phase, state.agent, state.mode
    );
    println!("\nstage pipeline:");
    let mut stage = Some(Stage::Plan);
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
        // information: Supervise-mode Validate gates regardless of any
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
        // here instead of always showing the full pipeline.
        let stop_marker = if state.stop_until == Some(s) {
            " [STOPS HERE — --until]"
        } else {
            ""
        };
        println!("  {s:<9} {command}{gate}{phase_gate}{stop_marker}");
        stage = s.next();
    }
    if let Some(until) = state.stop_until {
        println!("\nnote: --until {until} — this run will halt after {until} completes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_outcomes::{
        ValidateOutcome, handle_infra_outcome, handle_validate_outcome,
    };
    use crate::test_support::*;
    use devflow_core::gates::GateKind;
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

    /// 23-06 Task 1's named blocker regression guard (BLOCKER 1,
    /// cross-AI-review-caught): a phase halted cleanly by `devflow start
    /// --until <stage>` runs `transition`'s `stop_until` clean-stop branch,
    /// which emits `workflow_finished` with a `"stopped_at"` reason and
    /// returns before any hook, stage assignment, or `launch_stage` call —
    /// `workflow_shipped` must never be emitted on this path, while the
    /// `workflow_finished` it does emit carries the `stopped_at` reason.
    #[test]
    fn until_stop_never_emits_workflow_shipped() {
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
        let finished =
            devflow_core::events::last_event_of_kind_for_phase(root, phase, "workflow_finished")
                .expect("the --until clean stop emits workflow_finished");
        assert_eq!(finished["reason"], "stopped_at", "{finished}");
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

    /// An `abort` whose state cannot be removed says so, on the event a supervisor's
    /// dead stderr cannot carry, and does not report the workflow aborted. The state path
    /// is a directory so the removal fails whichever process runs this.
    #[test]
    fn abort_reports_a_state_it_could_not_remove() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5310);
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        std::fs::create_dir_all(workflow::state_path(root, phase).join("blocked")).unwrap();

        let err = abort(root, &state, "test").expect_err("the state is still there");

        assert!(err.to_string().contains("could not remove"), "{err}");
        assert_eq!(events_named(root, "abort_state_not_cleared").len(), 1);
        assert_eq!(events_named(root, "workflow_aborted").len(), 0);
    }

    /// An `abort` whose gate cannot be closed says so and still removes the state, since
    /// the leftover answer cannot decide a later gate (D-05). Control: the abort finishes.
    #[test]
    fn abort_reports_a_gate_it_could_not_close() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5311);
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        workflow::save_state(&state).unwrap();
        Gates::open_gate(
            root,
            phase,
            state.stage,
            devflow_core::gates::GateKind::StageFailure,
            "g",
        )
        .unwrap();
        let lock = std::fs::File::options()
            .read(true)
            .write(true)
            .open(root.join(format!(".devflow/.gates-{}.lock", phase.padded())))
            .unwrap();
        lock.lock().unwrap();

        abort(root, &state, "test").unwrap();

        assert_eq!(events_named(root, "abort_gate_not_closed").len(), 1);
        assert_eq!(events_named(root, "workflow_aborted").len(), 1);
        assert!(!workflow::state_path(root, phase).exists());
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

        // Pre-answer an abort (`abort: true`) so
        // `GateAction::from_response` resolves to `Abort`.
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &GateResponse {
                approved: false,
                abort: true,
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
        let refired = Gates::open_gate(
            root,
            phase,
            Stage::Validate,
            devflow_core::gates::GateKind::StageFailure,
            "re-fired gate",
        )
        .unwrap();
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
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
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

    /// A phase parked at Stage::Validate, with its feature branch cut from `develop`.
    fn validate_parked_fixture(root: &Path, phase: PhaseId) {
        init_repo(root);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let branch_created = devflow_core::test_support::git_command(root)
            .args(["branch", &branch, "develop"])
            .status()
            .unwrap()
            .success();
        assert!(branch_created);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();
    }

    /// The wait for a recovery is bounded: a stuck write-lock holder refuses the claim and
    /// names itself, and the refused process keeps no hold on the phase.
    #[test]
    fn a_foreground_claim_gives_up_on_a_held_write_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(94);
        validate_parked_fixture(root, phase);
        let holder = devflow_core::liveness::phase_holder_path(root, phase);

        let refused = std::thread::scope(|scope| {
            let (in_lock, locked) = std::sync::mpsc::channel();
            let (release, released) = std::sync::mpsc::channel::<()>();
            scope.spawn(move || {
                workflow::update_state_with(root, phase, |_| {
                    in_lock.send(()).unwrap();
                    let _ = released.recv();
                    Ok::<_, CliError>((None, ()))
                })
                .unwrap();
            });
            locked.recv().unwrap();
            let claimed = crate::commands::claim_foreground_holder(root, phase);
            release.send(()).unwrap();
            claimed
        })
        .map(|_hold| ())
        .expect_err("the claim gives up on the held lock");

        let refused = refused.to_string();
        assert!(refused.contains("write lock is still held"), "{refused}");
        // Polled: another test's fork can keep a just-released flock until its exec.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while devflow_core::liveness::probe(&holder).unwrap() == devflow_core::liveness::Held::Live
        {
            assert!(
                std::time::Instant::now() < deadline,
                "a refused claim keeps no hold"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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
            run_gate_with_timeout(
                &root_owned,
                &mut state,
                Stage::Code,
                GateKind::StageFailure,
                "ctx",
                30,
            )
            .unwrap();
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
                abort: false,
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

    /// WR-12 (49-REVIEW.md), reproduced against the gate entry: an advance loads
    /// its copy of the state, then the operator edits `gate_notify_cmd` on disk
    /// before the gate opens. The gate fires the edited command, not the loaded
    /// one, and the edit stays on disk. Its own child with the variable removed,
    /// because `DEVFLOW_GATE_NOTIFY_CMD` in the runner's environment would replace
    /// both commands.
    #[test]
    fn gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded() {
        const NAME: &str =
            "pipeline_gate::tests::gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded";
        if !devflow_core::test_support::in_child_test(NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([NAME, "--exact", "--test-threads=1"])
                .env(devflow_core::test_support::CHILD_TEST_ENV, NAME)
                .env_remove(devflow_core::gates::GATE_NOTIFY_CMD_ENV)
                .output()
                .unwrap();
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let sentinels = tempfile::tempdir().unwrap();
        let loaded_sentinel = sentinels.path().join("loaded-copy-fired");
        let edited_sentinel = sentinels.path().join("on-disk-edit-fired");
        let loaded_cmd = format!("touch {}", loaded_sentinel.display());
        let edited_cmd = format!("touch {}", edited_sentinel.display());
        let phase = PhaseId::new(5191);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some(loaded_cmd.clone());
        workflow::save_state(&state).unwrap();

        // The advance's copy, loaded before the edit.
        let mut loaded = workflow::load_state(root, phase).unwrap();
        let mut on_disk = workflow::load_state(root, phase).unwrap();
        on_disk.gate_notify_cmd = Some(edited_cmd.clone());
        workflow::save_state(&on_disk).unwrap();

        let root_owned = root.to_path_buf();
        let handle = std::thread::spawn(move || {
            run_gate_with_timeout(
                &root_owned,
                &mut loaded,
                Stage::Code,
                GateKind::StageFailure,
                "ctx",
                30,
            )
            .map(|action| (action, loaded.gate_notify_cmd))
        });
        let gate_path = Gates::gate_path(root, phase, Stage::Code);
        for _ in 0..500 {
            if gate_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(gate_path.exists(), "the gate must be written");
        answer_open_gate(
            root,
            phase,
            Stage::Code,
            &GateResponse {
                approved: true,
                abort: false,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        let (action, in_memory) = handle.join().unwrap().expect("the gate resolves");

        assert!(
            matches!(action, crate::pipeline_outcomes::Answer::Approve(_)),
            "{action:?}"
        );
        assert!(
            edited_sentinel.exists(),
            "the gate must fire the command edited on disk after the advance loaded"
        );
        assert!(
            !loaded_sentinel.exists(),
            "the gate must not fire the advance's stale copy"
        );
        assert_eq!(in_memory.as_deref(), Some(edited_cmd.as_str()));
        assert_eq!(
            workflow::load_state(root, phase)
                .unwrap()
                .gate_notify_cmd
                .as_deref(),
            Some(edited_cmd.as_str()),
            "the gate's entry save must not overwrite the on-disk edit"
        );
    }

    /// IN-10 (49-REVIEW.md), reproduced against the gate entry: a state file left
    /// unparseable after the advance loaded its copy is neither rewritten with that
    /// copy nor passed over in silence. The gate reports the unloadable state, its
    /// bytes are unchanged, and no gate is opened.
    #[test]
    fn gate_entry_over_an_unparseable_state_does_not_rewrite_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5192);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();

        let mut loaded = workflow::load_state(root, phase).unwrap();
        let state_file = workflow::state_path(root, phase);
        std::fs::write(&state_file, "{not json").unwrap();

        let err = run_gate_with_timeout(
            root,
            &mut loaded,
            Stage::Code,
            GateKind::StageFailure,
            "ctx",
            30,
        )
        .expect_err("an unparseable state must fail the gate entry");

        let text = err.to_string();
        assert!(
            text.contains("state JSON failed"),
            "the error must say the state did not load: {text}"
        );
        assert_eq!(
            std::fs::read(&state_file).unwrap(),
            b"{not json",
            "the unparseable state file must not be rewritten"
        );
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "no gate may open over an unloadable state"
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
            run_gate_with_timeout(
                &root_owned,
                &mut state,
                Stage::Code,
                GateKind::StageFailure,
                "ctx",
                30,
            )
            .map(|_| ())
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
                abort: false,
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
    ) -> std::thread::JoinHandle<Result<crate::pipeline_outcomes::Answer, CliError>> {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();
        let root_owned = root.to_path_buf();
        std::thread::spawn(move || {
            let mut state = workflow::load_state(&root_owned, phase).unwrap();
            run_gate_with_timeout(
                &root_owned,
                &mut state,
                Stage::Code,
                GateKind::StageFailure,
                "ctx",
                30,
            )
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
            abort: false,
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
        // The waiter moves the answer first and logs the move after it.
        let mut quarantined = events_named(root, "gate_answer_quarantined");
        while quarantined.is_empty() {
            assert!(
                started.elapsed() < SET_ASIDE_WITHIN,
                "the wait must log the answer it set aside"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            quarantined = events_named(root, "gate_answer_quarantined");
        }
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(quarantined[0]["reason"], "missing_gate_id");
        assert!(
            !waiter.is_finished(),
            "an answer with no gate id does not end the wait"
        );
        assert!(events_named(root, "gate_resolved").is_empty());

        answer_open_gate(root, phase, Stage::Code, &approval()).unwrap();
        let action = waiter.join().unwrap().unwrap();
        assert!(
            matches!(action, crate::pipeline_outcomes::Answer::Approve(_)),
            "{action:?}"
        );
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
        let old = Gates::open_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "g1",
        )
        .unwrap();
        Gates::open_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "g2",
        )
        .unwrap();
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

    /// A field another writer changed after a caller loaded its copy survives that caller's
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
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.base_branch = Some("workspace/concurrent".to_string());
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        // The launch this ends in fails on the agent-free PATH, which is not this
        // test's subject.
        let _ = transition(root, &mut state, Stage::Validate);

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            after.base_branch.as_deref(),
            Some("workspace/concurrent"),
            "the other writer's change survives the transition"
        );
        assert_eq!(after.stage, Stage::Validate, "the transition's own write");
    }

    /// A PATH dir whose `codex` waits (bounded, about 60 s) for a file that never
    /// appears, with `sleep` for that wait, so a stage launched onto it stays
    /// running and its supervisor writes no state while the test reads it.
    /// Codex is a capture-shape agent, so the launch spends no delivery canary.
    fn waiting_codex_dir() -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let path_dir = agent_free_dir_with_agent_stub("codex");
        let path = path_dir.path().join("codex");
        std::fs::write(
            &path,
            "#!/bin/sh\ncase \"$1\" in --version) exit 0 ;; esac\n\
             i=0\nwhile [ ! -e never-released ] && [ \"$i\" -lt 1200 ]; do sleep 0.05; i=$((i+1)); done\n\
             exit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let real_sleep = ["/usr/bin/sleep", "/bin/sleep"]
            .into_iter()
            .map(Path::new)
            .find(|path| path.is_file())
            .expect("sleep must exist to run this test");
        std::os::unix::fs::symlink(real_sleep, path_dir.path().join("sleep")).unwrap();
        path_dir
    }

    /// D-09: a fix type lives exactly as long as its Code pass. Both of
    /// `transition`'s commits clear it: (a) the `--until` stop after Code (a run
    /// halted after its Code pass is no longer mid-fix) and (b) the ordinary move
    /// to Validate, which then launches Validate.
    #[test]
    fn a_transition_clears_the_persisted_code_fix() {
        const NAME: &str = "pipeline_gate::tests::a_transition_clears_the_persisted_code_fix";
        enter_path_isolated_child!(NAME, waiting_codex_dir());
        let _guard = env_lock();
        let code_state = |root: &Path, phase: PhaseId| {
            init_repo(root);
            let mut state =
                State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
            state.stage = Stage::Code;
            state.code_fix = Some(FixType::GapsOnly);
            state
        };

        let until_dir = tempfile::tempdir().unwrap();
        let until_root = until_dir.path();
        let until_phase = PhaseId::new(5172);
        let mut state = code_state(until_root, until_phase);
        state.stop_until = Some(Stage::Code);
        workflow::save_state(&state).unwrap();
        transition(until_root, &mut state, Stage::Validate).unwrap();
        let halted = workflow::load_state(until_root, until_phase).unwrap();
        assert!(halted.stopped, "the --until stop fired");
        assert_eq!(
            halted.code_fix, None,
            "a run halted after its Code pass is no longer mid-fix"
        );

        let moved_dir = tempfile::tempdir().unwrap();
        let moved_root = moved_dir.path();
        let moved_phase = PhaseId::new(5174);
        let mut state = code_state(moved_root, moved_phase);
        workflow::save_state(&state).unwrap();
        let result = transition(moved_root, &mut state, Stage::Validate);
        let moved = workflow::load_state(moved_root, moved_phase).unwrap();
        let _reap_guard = ReapMonitorOnDrop::after_launch(&moved);
        result.unwrap();
        assert_eq!(moved.stage, Stage::Validate);
        assert!(moved.supervisor.is_some(), "Validate was launched");
        assert_eq!(
            moved.code_fix, None,
            "the transition out of Code clears the fix pass"
        );
    }

    /// A loop-back writes its stage move and its fix pass, and keeps a field another
    /// writer changed since the caller loaded its copy.
    #[test]
    fn a_loop_back_commits_its_fields() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(89);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.gate_pending = true;
        workflow::save_state(&state).unwrap();
        workflow::update_state(root, phase, |on_disk| {
            on_disk.base_branch = Some("workspace/concurrent".to_string());
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();

        prepare_loop_back_to_code(
            root,
            &mut state,
            FixType::GapsOnly,
            None,
            None,
            LoopBackReason::GateResponse,
        )
        .unwrap();

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(
            after.base_branch.as_deref(),
            Some("workspace/concurrent"),
            "the concurrent write is kept"
        );
        assert_eq!(after.stage, Stage::Code);
        assert!(!after.gate_pending);
        assert_eq!(after.code_fix, Some(FixType::GapsOnly));
    }

    // ---- GATE-01 / U7: the relaunch note leaves with its stage ----

    /// The relaunch note the rows below seed: a newline and a tab, so a clear
    /// that kept part of it, or a control that lost part of it, shows.
    const RELAUNCH_NOTE: &str = "relaunch note:\n\tkeep the stage's scope";

    /// A Codex run of `phase` at `root`, at `stage`, in supervise mode, holding
    /// [`RELAUNCH_NOTE`] as its relaunch note. Codex is a capture-shape agent,
    /// so a launch spends no delivery canary.
    fn noted_state(root: &Path, phase: PhaseId, stage: Stage) -> State {
        init_repo(root);
        let mut state = State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
        state.verification_baseline_captured = true;
        state.stage = stage;
        state.relaunch_note = Some(RELAUNCH_NOTE.to_string());
        state
    }

    /// The prompt a no-override launch of `state` gives Codex: the builder a
    /// resume or failover replay uses.
    fn default_prompt(state: &State) -> String {
        let codex = devflow_core::agents::driver_for(AgentKind::Codex);
        crate::pipeline_launch::default_stage_prompt(state, codex.as_ref())
    }

    /// Whether `prompt` carries a relaunch-note block at all.
    fn has_relaunch_block(prompt: &str) -> bool {
        prompt.contains(devflow_core::prompt::RELAUNCH_NOTE_INTRODUCTION)
            || prompt.contains("OPERATOR NOTE")
    }

    /// U7: `relaunch_note` is `None` once the run leaves the stage it was given
    /// for, on each production way out at this wave: `transition` (both of its
    /// commits: the ordinary move and the `--until` halt), the gap-closure
    /// route's `transition` from Validate to Plan (52-27), the Validate
    /// loop-back to Code (`prepare_loop_back_to_code`, which does not call
    /// `transition`) and `finish_run`. Controls, so each cleared assertion can
    /// fail: a stop and resume at the same stage keeps the note, and a Code
    /// state saved and reloaded with no stage change keeps it in the builder the
    /// Validate row asserts on.
    #[test]
    fn leaving_the_relaunched_stage_clears_the_relaunch_note() {
        const NAME: &str =
            "pipeline_gate::tests::leaving_the_relaunched_stage_clears_the_relaunch_note";
        enter_path_isolated_child!(NAME, waiting_codex_dir());
        let _guard = env_lock();

        // `transition` out of Plan and out of Code, each launching the next stage.
        for (number, from, to) in [
            (5280, Stage::Plan, Stage::Code),
            (5281, Stage::Code, Stage::Validate),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let phase = PhaseId::new(number);
            let mut state = noted_state(dir.path(), phase, from);
            workflow::save_state(&state).unwrap();
            let result = transition(dir.path(), &mut state, to);
            let moved = workflow::load_state(dir.path(), phase).unwrap();
            let _reap_guard = ReapMonitorOnDrop::after_launch(&moved);
            result.unwrap();
            assert_eq!(moved.stage, to);
            assert_eq!(
                moved.relaunch_note, None,
                "{from} -> {to}: the transition clears the relaunch note"
            );
            assert!(
                !has_relaunch_block(&default_prompt(&moved)),
                "{from} -> {to}: the {to} intent carries no relaunch note"
            );
        }

        // Validate -> Plan, the gap-closure route's way out of Validate (52-27):
        // a state at Validate with the note (as a rejected Validate stage-failure
        // gate's relaunch leaves it) and the gap-closure marker set.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5287);
        let mut state = noted_state(dir.path(), phase, Stage::Validate);
        state.gap_closure_pass = true;
        workflow::save_state(&state).unwrap();
        let result = transition(dir.path(), &mut state, Stage::Plan);
        let moved = workflow::load_state(dir.path(), phase).unwrap();
        let _reap_gap = ReapMonitorOnDrop::after_launch(&moved);
        result.unwrap();
        assert_eq!(moved.stage, Stage::Plan);
        assert_eq!(
            moved.relaunch_note, None,
            "validate -> plan: the gap route's transition clears the relaunch note"
        );
        assert!(
            !has_relaunch_block(&default_prompt(&moved)),
            "validate -> plan: the gaps Plan intent carries no relaunch note"
        );

        // Validate -> Code through the loop-back, which does not call `transition`.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5282);
        let mut state = noted_state(dir.path(), phase, Stage::Validate);
        workflow::save_state(&state).unwrap();
        let prompt = prepare_loop_back_to_code(
            dir.path(),
            &mut state,
            FixType::GapsOnly,
            None,
            None,
            LoopBackReason::GateResponse,
        )
        .unwrap();
        let looped = workflow::load_state(dir.path(), phase).unwrap();
        assert_eq!(looped.stage, Stage::Code);
        assert_eq!(
            looped.relaunch_note, None,
            "the loop-back to Code clears Validate's relaunch note"
        );
        assert!(
            !has_relaunch_block(&prompt),
            "the loop-back's own prompt holds no relaunch note:\n{prompt}"
        );
        assert!(
            !has_relaunch_block(&default_prompt(&looped)),
            "a replay of the fix pass carries no relaunch note"
        );

        // `finish_run` from Validate.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5283);
        let mut state = noted_state(dir.path(), phase, Stage::Validate);
        workflow::save_state(&state).unwrap();
        finish_run(dir.path(), &mut state, test_permit()).unwrap();
        let finished = workflow::load_state(dir.path(), phase).unwrap();
        assert!(finished.finished, "the run finished");
        assert_eq!(
            finished.relaunch_note, None,
            "the run's end clears the note"
        );

        // The `--until` halt, `transition`'s early-return commit.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5284);
        let mut state = noted_state(dir.path(), phase, Stage::Plan);
        state.stop_until = Some(Stage::Plan);
        workflow::save_state(&state).unwrap();
        transition(dir.path(), &mut state, Stage::Code).unwrap();
        let halted = workflow::load_state(dir.path(), phase).unwrap();
        assert!(halted.stopped, "the --until halt fired");
        let finished_event = devflow_core::events::last_event_of_kind_for_phase(
            dir.path(),
            phase,
            "workflow_finished",
        )
        .expect("the halt emits workflow_finished");
        assert_eq!(finished_event["reason"], "stopped_at", "{finished_event}");
        assert_eq!(
            halted.relaunch_note, None,
            "the --until halt clears the note"
        );

        // Control: a stop and resume at Plan, with no transition, keeps it.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5285);
        let mut state = noted_state(dir.path(), phase, Stage::Plan);
        state.stopped = true;
        state.stop_reason = Some("stopped by the operator".to_string());
        workflow::save_state(&state).unwrap();
        let result = crate::pipeline_launch::resume(dir.path(), phase, None);
        let resumed = workflow::load_state(dir.path(), phase).unwrap();
        let _reap_resumed = ReapMonitorOnDrop::after_launch(&resumed);
        result.unwrap();
        assert_eq!(resumed.stage, Stage::Plan);
        assert_eq!(
            resumed.relaunch_note.as_deref(),
            Some(RELAUNCH_NOTE),
            "control: a stop and resume keeps the note"
        );
        assert!(
            default_prompt(&resumed).contains(RELAUNCH_NOTE),
            "control: the relaunched Plan intent carries the note"
        );

        // Control: Code to Code. A relaunched fix pass, saved and reloaded with
        // no stage change, keeps the note in the builder the Validate row reads.
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5286);
        let mut state = noted_state(dir.path(), phase, Stage::Code);
        state.code_fix = Some(FixType::GapsOnly);
        workflow::save_state(&state).unwrap();
        let reloaded = workflow::load_state(dir.path(), phase).unwrap();
        assert_eq!(reloaded.relaunch_note.as_deref(), Some(RELAUNCH_NOTE));
        let prompt = default_prompt(&reloaded);
        assert!(
            prompt.contains(RELAUNCH_NOTE)
                && prompt.contains(devflow_core::prompt::RELAUNCH_NOTE_INTRODUCTION),
            "control: the Code intent keeps the note:\n{prompt}"
        );
    }

    /// One write of `State.stage` the stage-write scan found: the file, the
    /// 1-based line, the function it sits in, and the receiver written through.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct StageWrite {
        file: String,
        line: usize,
        function: String,
        receiver: String,
    }

    /// The stage writes in one file's `source`, under the scan's rule: only the
    /// text before the test-module marker counts, each line from its first
    /// `//` is dropped, and a hit is `\b([A-Za-z_]\w*)\.stage\s*=(?![=>])`,
    /// matched by hand, whose receiver is not `spec`.
    ///
    /// Limits: a write through a struct literal (`State { stage, .. }`),
    /// `std::mem::replace(&mut s.stage, ..)` or a `&mut s.stage` borrow is not
    /// seen, and a `//` inside a string literal ends that line's scanned text
    /// early.
    fn stage_writes_in(file: &str, source: &str) -> Vec<StageWrite> {
        const TEST_MODULE_MARKER: &str = "\n#[cfg(test)]\nmod tests {";
        const FIELD: &str = ".stage";
        let non_test = source
            .find(TEST_MODULE_MARKER)
            .map_or(source, |at| &source[..at]);
        let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        let mut function = String::new();
        let mut writes = Vec::new();
        for (index, raw_line) in non_test.lines().enumerate() {
            let line = raw_line.find("//").map_or(raw_line, |at| &raw_line[..at]);
            if let Some(at) = line.find("fn ") {
                let name: String = line[at + 3..]
                    .chars()
                    .take_while(|character| character.is_alphanumeric() || *character == '_')
                    .collect();
                if !name.is_empty() && (at == 0 || !is_word(line.as_bytes()[at - 1])) {
                    function = name;
                }
            }
            let bytes = line.as_bytes();
            let mut from = 0;
            while let Some(offset) = line[from..].find(FIELD) {
                let dot = from + offset;
                from = dot + FIELD.len();
                let start = (0..dot)
                    .rev()
                    .take_while(|&at| is_word(bytes[at]))
                    .last()
                    .unwrap_or(dot);
                let receiver = &line[start..dot];
                let starts_like_an_identifier = receiver
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
                let mut after = from;
                while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                    after += 1;
                }
                let assigns = bytes.get(after) == Some(&b'=')
                    && !matches!(bytes.get(after + 1), Some(b'=') | Some(b'>'));
                if starts_like_an_identifier && assigns && receiver != "spec" {
                    writes.push(StageWrite {
                        file: file.to_string(),
                        line: index + 1,
                        function: function.clone(),
                        receiver: receiver.to_string(),
                    });
                }
            }
        }
        writes
    }

    /// Every `.rs` file below `dir`, recursively, sorted.
    fn rust_files_under(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in std::fs::read_dir(&next).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    /// U7's sweep pinned (V2): the only production writes of `State.stage`
    /// under `crates/devflow-cli/src` and `crates/devflow-core/src` are
    /// `transition`'s and `prepare_loop_back_to_code`'s, each of which clears
    /// the relaunch note in the same commit. A plan that adds a third write
    /// fails here and must add that write's clear site. The rule and its limits
    /// are on [`stage_writes_in`]; `State::stage` stays `pub` (published core).
    #[test]
    fn state_stage_is_written_only_by_transition_and_the_code_loop_back() {
        // Controls: any receiver name counts; comments, `spec.stage`, a
        // comparison, a match arm and a `let stage` local do not.
        let planted = "fn planted() {\n    persisted.stage = Stage::Plan;\n}\n";
        assert_eq!(
            stage_writes_in("planted.rs", planted),
            vec![StageWrite {
                file: "planted.rs".to_string(),
                line: 2,
                function: "planted".to_string(),
                receiver: "persisted".to_string(),
            }]
        );
        for control in [
            "    // state.stage = to\n",
            "    spec.stage = a;\n",
            "    if a.stage == b {}\n",
            "    x if x != state.stage => {}\n",
            "    let stage = Stage::Code;\n",
        ] {
            assert_eq!(
                stage_writes_in("control.rs", control),
                Vec::new(),
                "not a stage write: {control:?}"
            );
        }
        // Control: the test-module split. A write after the marker is test code.
        let with_tests = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { s.stage = x; }\n}\n";
        assert_eq!(stage_writes_in("split.rs", with_tests), Vec::new());

        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut writes = Vec::new();
        for crate_src in [manifest.join("src"), manifest.join("../devflow-core/src")] {
            for file in rust_files_under(&crate_src) {
                let source = std::fs::read_to_string(&file).unwrap();
                let name = file
                    .strip_prefix(manifest)
                    .unwrap_or(&file)
                    .display()
                    .to_string();
                writes.extend(stage_writes_in(&name, &source));
            }
        }
        let functions: Vec<(&str, &str)> = writes
            .iter()
            .map(|write| (write.file.as_str(), write.function.as_str()))
            .collect();
        assert_eq!(
            functions,
            vec![
                ("src/pipeline_gate.rs", "transition"),
                ("src/pipeline_gate.rs", "prepare_loop_back_to_code"),
            ],
            "every production write of State.stage, each needing a relaunch_note clear: {writes:#?}"
        );
    }

    /// One `"workflow_finished"` literal in a file's non-test text: the file,
    /// the function it sits in, and the text from the literal to the end of its
    /// statement.
    #[derive(Debug, Clone)]
    struct FinishedSite {
        file: String,
        function: String,
        payload: String,
        constant: bool,
    }

    /// The `"workflow_finished"` literals in `source`'s non-test text: the text
    /// before the test-module marker, or the whole file when it has none.
    fn workflow_finished_sites(file: &str, source: &str) -> Vec<FinishedSite> {
        const TEST_MODULE_MARKER: &str = "\n#[cfg(test)]\nmod tests {";
        let literal = concat!("\"workflow", "_finished\"");
        let non_test = source
            .find(TEST_MODULE_MARKER)
            .map_or(source, |at| &source[..at]);
        let mut sites = Vec::new();
        let mut from = 0;
        while let Some(offset) = non_test[from..].find(literal) {
            let at = from + offset;
            from = at + literal.len();
            let before = &non_test[..at];
            let function = before
                .rmatch_indices("fn ")
                .find_map(|(index, _)| {
                    let name: String = before[index + 3..]
                        .chars()
                        .take_while(|character| character.is_alphanumeric() || *character == '_')
                        .collect();
                    let boundary = index == 0
                        || !before.as_bytes()[index - 1].is_ascii_alphanumeric()
                            && before.as_bytes()[index - 1] != b'_';
                    (boundary && !name.is_empty()).then_some(name)
                })
                .unwrap_or_default();
            let line_start = before.rfind('\n').map_or(0, |index| index + 1);
            let line = &before[line_start..];
            let constant = ["const ", "static "]
                .iter()
                .any(|keyword| line.contains(keyword));
            let rest = &non_test[at..];
            let payload = rest[..rest.find(");").unwrap_or(rest.len())].to_string();
            sites.push(FinishedSite {
                file: file.to_string(),
                function,
                payload,
                constant,
            });
        }
        sites
    }

    /// Why `sites` breaks the rule, or nothing when exactly two sites exist,
    /// both in `pipeline_gate.rs`: `finish_run` with reason `finished` and
    /// `transition` with reason `stopped_at`, and no constant names the event.
    fn workflow_finished_violations(sites: &[FinishedSite]) -> Vec<String> {
        let mut violations: Vec<String> = sites
            .iter()
            .filter(|site| site.constant)
            .map(|site| format!("a constant names the event: {site:?}"))
            .collect();
        let allowed = |site: &FinishedSite| {
            site.file.ends_with("pipeline_gate.rs")
                && ((site.function == "finish_run"
                    && site.payload.contains(r#""reason": "finished""#))
                    || (site.function == "transition"
                        && site.payload.contains(r#""reason": "stopped_at""#)))
        };
        violations.extend(
            sites.iter().filter(|site| !allowed(site)).map(|site| {
                format!("an emission outside finish_run and the --until halt: {site:?}")
            }),
        );
        let functions: std::collections::BTreeSet<&str> =
            sites.iter().map(|site| site.function.as_str()).collect();
        if sites.len() != 2 || functions != ["finish_run", "transition"].into_iter().collect() {
            violations.push(format!(
                "expected exactly two sites (finish_run, transition), found {}: {sites:?}",
                sites.len()
            ));
        }
        violations
    }

    /// Every `.rs` file under `dir`, recursively, as (path, text).
    fn rust_sources(dir: &std::path::Path) -> Vec<(String, String)> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).expect("read a source dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(rust_sources(&path));
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push((
                    path.display().to_string(),
                    std::fs::read_to_string(&path).expect("read a source file"),
                ));
            }
        }
        files
    }

    /// D-45 rule 1 beside D-29: outside test code in both crates' `src/`, the
    /// `"workflow_finished"` literal occurs exactly twice, both in
    /// `pipeline_gate.rs`: in `finish_run` (reason `finished`) and in
    /// `transition`'s `--until` halt (reason `stopped_at`). The tree is walked at
    /// test time, so a new file is covered. Controls in the same test: a planted
    /// third emission in another function and a planted constant naming the
    /// event each report a violation; and since the real tree must yield exactly
    /// two sites, a walk that reads no file fails too.
    #[test]
    fn workflow_finished_is_emitted_only_by_finish_run_and_the_until_halt() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut sites = Vec::new();
        for dir in [manifest.join("src"), manifest.join("../devflow-core/src")] {
            for (file, source) in rust_sources(&dir) {
                sites.extend(workflow_finished_sites(&file, &source));
            }
        }
        let violations = workflow_finished_violations(&sites);
        assert!(violations.is_empty(), "{violations:#?}");

        let emit = concat!("\"workflow", "_finished\"");
        let planted_emit = format!(
            "fn elsewhere() {{\n    events::emit(root, phase, {emit}, json!({{ \"reason\": \"finished\" }}));\n}}\n"
        );
        let mut with_third = sites.clone();
        with_third.extend(workflow_finished_sites(
            "src/pipeline_gate.rs",
            &planted_emit,
        ));
        assert!(
            !workflow_finished_violations(&with_third).is_empty(),
            "control: a third emission site is a violation"
        );
        let planted_const = format!("const EVENT: &str = {emit};\n");
        let mut with_const = sites.clone();
        with_const.extend(workflow_finished_sites(
            "src/pipeline_launch.rs",
            &planted_const,
        ));
        assert!(
            !workflow_finished_violations(&with_const).is_empty(),
            "control: a constant naming the event is a violation"
        );
        assert!(
            !workflow_finished_violations(&[]).is_empty(),
            "control: a walk that read no file is a violation"
        );
    }

    /// A permit through the one finish check, over evidence that passes it:
    /// GSD's `passed` with the Validate PASS on HEAD over a clean tree.
    fn test_permit() -> FinishPermit {
        let evidence = crate::pipeline_outcomes::Evidence {
            read: Ok(devflow_core::gsd_reader::StatusReport {
                status: devflow_core::gsd_reader::GsdStatus::Passed,
                route: String::new(),
                next_action: None,
                moved_files: Vec::new(),
            }),
            agent_pass_on_head: true,
            no_pass_cause: None,
        };
        crate::pipeline_outcomes::can_finish(&evidence).expect("the evidence passes can_finish")
    }
}
