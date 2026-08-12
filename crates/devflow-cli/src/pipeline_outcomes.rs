//! Pipeline seam B (D-06): deciding what happens after a stage produces a
//! result — the `handle_*_outcome` family, checkout-hook execution, and the
//! gate-context rendering helpers they share. Extracted mechanically
//! (19-08, D-09 pure move) out of `main.rs` — every function below is
//! byte-identical to its pre-move body modulo an added `pub(crate)` and
//! adjusted `use` paths.
//!
//! **This module sits in the middle of the pipeline's three-way module
//! cycle (19-RESEARCH.md Pattern 1):** it is called by
//! [`crate::pipeline_launch::advance`] once an agent result has been
//! classified, and it calls onward into [`crate::pipeline_gate`]'s
//! `transition`, `loop_back_to_code`, `finish_workflow`, and `abort` to
//! actually move the state machine.

use crate::CliError;
use crate::config_parse::{checkout_lock_timeout, gate_timeout_secs};
use crate::parallel::retry_after_from_reason;
use crate::pipeline_gate::{
    LoopBackReason, abort, finish_workflow, loop_back_to_code, run_gate, run_gate_with_timeout,
    transition,
};
use crate::pipeline_launch::launch_stage;
use devflow_core::config::GitFlowConfig;
use devflow_core::gates::{GateAction, GateResponse, Gates};
use devflow_core::hooks::{self, HookContext};
use devflow_core::mode;
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt::FixType;
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::{
    agent_result,
    agent_result::{AgentStatus, Verdict},
    events, lock, workflow,
};
use std::path::{Path, PathBuf};

/// Route a `GateInfra` outcome (ResourceKilled/AgentUnavailable) — bumps
/// `state.infra_failures` (saturating, never `consecutive_failures`),
/// persists, then either aborts at the ceiling or fires the never-silent
/// gate via [`handle_stage_failure`]. Deliberately never calls
/// `handle_validate_outcome`/`handle_ship_failure` on any stage (review
/// consensus #4) — those increment `consecutive_failures`, which would
/// conflate an infrastructure fault with an agent-caused failure (D-08).
pub(crate) fn handle_infra_outcome(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    state.infra_failures = state.infra_failures.saturating_add(1);
    workflow::save_state(state)?;
    gate_or_abort_infra(project_root, state, stage, reason)
}

/// The ceiling check + gate-or-abort half of the infra path, shared by
/// [`handle_infra_outcome`] and the `AutoResume` arm's infra-ceiling branch
/// (which bumps `infra_failures` itself before calling this, so the counter
/// is never bumped twice for the same outcome).
pub(crate) fn gate_or_abort_infra(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    if state.infra_failures >= mode::MAX_INFRA_FAILURES {
        return abort(
            project_root,
            state,
            &format!(
                "infrastructure failures reached the ceiling ({} of {}) — aborting rather than gating again",
                state.infra_failures,
                mode::MAX_INFRA_FAILURES
            ),
        );
    }
    handle_stage_failure(project_root, state, stage, reason)
}

/// Route a `RateLimited` outcome from the PRIMARY advance() monitor loop
/// (D-09): writes a single-agent cron-instructions resume record (`devflow
/// resume --phase N`) and returns without firing a blocking gate — unlike
/// `sequentagent`'s existing rate-limit handling, this path never called the
/// cron machinery before this plan (Pitfall 3). Shares the same
/// `infra_failures` ceiling as [`handle_infra_outcome`] (D-08's intentional
/// shared infra counter): once bumping would reach the ceiling, auto-resume
/// stops and the outcome instead routes through the infra gate/abort path.
/// Never touches `consecutive_failures`.
pub(crate) fn handle_rate_limited_outcome(
    project_root: &Path,
    state: &mut State,
    phase: PhaseId,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let retry_after = retry_after_from_reason(reason.as_deref());
    let projected_infra_failures = state.infra_failures.saturating_add(1);
    if projected_infra_failures >= mode::MAX_INFRA_FAILURES {
        return handle_infra_outcome(project_root, state, stage, reason);
    }
    state.infra_failures = projected_infra_failures;
    workflow::save_state(state)?;

    let instructions =
        devflow_core::ship::build_single_agent_cron_instructions(project_root, phase, &retry_after);
    devflow_core::ship::write_cron_instructions(project_root, &instructions)?;
    // CR-03: an unparseable retry hint (e.g. the `"usage limit"` fallback for
    // a 429 with no retry_after) leaves the schedule empty — and it must stay
    // empty, since an empty cron expression would degrade into an
    // every-minute resume. That means auto-resume cannot happen, so returning
    // here would exit the detached monitor with the phase stalled and no
    // operator signal at all (the println below is read by nobody). Route
    // through the same gate/notify path the infra ceiling uses so the phase is
    // never silently stalled (WR-11/D-15). `infra_failures` is already bumped
    // above, so `gate_or_abort_infra` — which never bumps — is the correct
    // entry point.
    if instructions.hermes_cron.schedule.is_empty() {
        return gate_or_abort_infra(
            project_root,
            state,
            stage,
            Some(format!(
                "rate limited with no parseable retry time ({retry_after}) — auto-resume cron not scheduled; resume manually"
            )),
        );
    }
    println!(
        "rate limited — wrote {}",
        devflow_core::ship::cron_instructions_path(project_root, phase)
            .strip_prefix(project_root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| {
                devflow_core::ship::cron_instructions_path(project_root, phase)
                    .display()
                    .to_string()
            })
    );
    events::emit(
        project_root,
        phase,
        "rate_limit_resume_scheduled",
        serde_json::json!({
            "stage": stage.to_string(),
            "retry_after": retry_after,
            "infra_failures": state.infra_failures,
        }),
    );
    Ok(())
}

/// The three-way outcome of a Validate stage evaluation (18e, D-18e).
///
/// Distinct from a plain `bool`: an `external_verify`-declared Validate has
/// THREE distinguishable outcomes, not two — the probe and the agent's
/// self-reported verdict can independently agree, disagree, or leave one
/// signal missing. Collapsing disagreement or "no verdict at all" onto
/// `Failed` would route them through the counter-based auto-loop, a DELAYED
/// gate indistinguishable from an ordinary retry to the operator watching
/// it — the binding operator decision requires an IMMEDIATE one instead
/// (T-18-19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValidateOutcome {
    /// The two independent signals agree (or no `external_verify` is
    /// declared and the agent reported `verdict: pass`): advance to Ship.
    Passed,
    /// An ordinary Validate failure — the pre-existing fail-safe, unchanged:
    /// loop back to Code, or gate once `consecutive_failures` reaches the
    /// ceiling.
    Failed,
    /// The probe passed but the agent's verdict disagrees, or no verdict
    /// arrived at all. Gates for a human IMMEDIATELY, never touching
    /// `consecutive_failures`. The payload names which two signals
    /// disagreed, for the `[never-silent]` gate context.
    Ambiguous(String),
}

/// Classify a Validate-stage `AgentResult` into its three-way outcome
/// (D-18e, the binding operator decision reproduced in 18-05-PLAN.md).
///
/// Pure function over `&AgentResult` — no I/O — so the whole decision matrix is
/// directly unit-testable.
///
/// **The match is exhaustive over the `AgentStatus` position with no wildcard
/// arm** (D-06, ROADMAP criterion 3). Every one of the seven variants appears by
/// name, so an eighth is a compile error (E0004) rather than a silent join in
/// either direction. The ban is POSITIONAL: `_` in the `layer0` or `verdict`
/// position is fine — only the status position must be enumerated. It is a ban
/// in BOTH directions: a catch-all routed to `Failed` carries the same
/// compiles-untouched-against-a-new-variant weakness as one routed to `Passed`.
///
/// `layer0` is deliberately the LAYER-ONLY normalisation. The superseded
/// composite (`decided_by_layer == Some(0) && status == Success`) folded an
/// `AgentStatus` equality test back into the normaliser — the exact
/// hand-audited construct this rewrite exists to eliminate — and its `false`
/// value conflated "Layer 1, Success" with "Layer 0, Failed", so the arms could
/// not tell provenance from status (D-06's named trap).
///
/// A monitor-produced idle timeout has `decided_by_layer: Some(1)` and
/// `verdict: None`, so `layer0` is `false` and the status lands on the
/// six-variant `Failed` arm below — loop back or gate, never advance. That is
/// the intended routing for "we gave up waiting" at Validate, and it is
/// unchanged by this rewrite.
///
/// # How the Validate trust inversion is actually reached (D-15, ROADMAP criterion 5)
///
/// A superseded note here claimed the `(_, Some(Verdict::Pass))` wildcard let an
/// agent's self-reported `verdict` outrank any `status`, and flagged it as
/// pre-existing rather than fixing it. **The route named was wrong.** The
/// inversion IS reachable, but by `reconcile_layer0_verdict`'s graft in
/// `devflow-core`, not by this match:
///
/// - This function's own inputs genuinely ARE always `Success`. `decide_action`
///   routes every non-`Success` status to a gate before
///   `classify_validate_outcome` is called, and the sole production call site is
///   inside its `Action::Advance` arm. The status was **laundered upstream**:
///   the graft attached Layer 1's `verdict` to Layer 0's `Success` without
///   reading Layer 1's own status.
/// - **That graft is fixed in plan 34-01, and criterion 3's fix here does not
///   close it.** This rewrite passes cleanly over the exploit precisely because
///   the status it sees is already affirmative. Criterion 3 and criterion 4 are
///   separate deliverables; neither alone closes the pair.
/// - **The `Ambiguous` arms' safety depends on a routing decision in another
///   crate** — `outcome_policy.rs`'s deferred `Failed`/`Unknown` collapse, which
///   `decide_action`'s own comment marks revisitable.
///
/// The `(false, Success, Gaps | None)` cells stay `Failed` on purpose (D-05's
/// "what this does NOT cover"): collapsing them to `Ambiguous` would convert the
/// ordinary "validation found gaps" auto-loop into an immediate gate on cycle
/// one — the loop 999.65/999.66 and Phase 33 just repaired.
pub(crate) fn classify_validate_outcome(result: &agent_result::AgentResult) -> ValidateOutcome {
    let layer0 = result.decided_by_layer == Some(0);
    match (layer0, result.status, result.verdict) {
        // The "two independent signals agreeing" arm — layer-independent, which
        // is why `layer0` is `_` here and not `true`.
        (_, AgentStatus::Success, Some(Verdict::Pass)) => ValidateOutcome::Passed,
        (true, AgentStatus::Success, Some(Verdict::Gaps)) => ValidateOutcome::Ambiguous(
            "external verification passed but the agent reported gaps".to_string(),
        ),
        (true, AgentStatus::Success, None) => ValidateOutcome::Ambiguous(
            "external verification passed but no agent verdict arrived".to_string(),
        ),
        // No external probe decided this result, so there is no second signal to
        // disagree with — an ordinary Validate failure, routed to the
        // counter-based auto-loop (D-05).
        (false, AgentStatus::Success, Some(Verdict::Gaps) | None) => ValidateOutcome::Failed,
        // All six non-`Success` statuses share the `Failed` destination, named
        // individually so an eighth variant cannot join them silently.
        //
        // `RateLimited` and `AgentUnavailable` are the two the superseded
        // criterion 3 omitted. All six are unreachable at this call site:
        // `classify_validate_outcome` is called only inside `decide_action`'s
        // `Action::Advance` arm, and `decide_action` maps only `Success` there.
        //
        // `Failed` is chosen because it is exactly what the superseded `_` arm
        // already gave them, so this rewrite has ZERO runtime delta for these
        // cells. A divergent destination for `RateLimited` specifically would
        // contradict `outcome_policy.rs`'s live, defended
        // `AgentStatus::RateLimited => Action::AutoResume` routing if the cell
        // ever became reachable — confront that tension before changing this.
        (
            _,
            AgentStatus::Failed
            | AgentStatus::Unknown
            | AgentStatus::RateLimited
            | AgentStatus::ResourceKilled
            | AgentStatus::AgentUnavailable
            | AgentStatus::IdleTimeout,
            _,
        ) => ValidateOutcome::Failed,
    }
}

/// The two ordinary Validate outcomes left once `ValidateOutcome::Ambiguous`
/// has been handled and returned on its own (WR-03, 18-fix). Deliberately a
/// distinct, two-variant type: matching on THIS below is exhaustive without
/// a third, panic-capable arm — the compiler enforces that
/// `handle_validate_outcome`'s tail can never see an ambiguous outcome,
/// instead of that invariant being proven by hand-tracing control flow (the
/// pre-fix shape's `unreachable!()`, which was sound but fragile: a future
/// edit to either the `forced` computation or the early-return `if` could
/// have silently reintroduced reachability).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidateResult {
    Passed,
    Failed,
}

/// The single D-01 decision point every in-scope loop-back arm consults:
/// whether a Validate→Code loop-back should dispatch the plain
/// `/gsd-execute-phase {N}` command (the phase is mid-arc — no
/// `{N}-VERIFICATION.md` has been produced yet, so `--gaps-only` would match
/// zero plans and gate unresolvably) or the narrower `--gaps-only` command
/// (a `{N}-VERIFICATION.md` already exists, so this is a genuine gaps loop).
///
/// D-02 places `handle_ship_outcome`'s loop-back deliberately out of scope:
/// by the time Ship runs, the phase has already been judged complete, so it
/// is by definition not mid-arc and `FixType::GapsOnly` is already correct
/// there — that call site does not call this helper.
///
/// CR-01: the root passed in is the **evidence root**, not the main checkout.
/// In worktree mode that is the phase's worktree, because `.planning/` is a
/// tracked directory and the Validate agent runs in the worktree — so the
/// `{N}-VERIFICATION.md` it authors lands on `feature/phase-{N}` inside the
/// worktree tree and is absent from the main checkout for the phase's entire
/// in-flight duration (there is no merge-back, so this is the steady state,
/// not a race). Callers resolve it from `state.worktree_path`, falling back
/// to the project root — the same plain fallback idiom used at
/// `staleness.rs:330`, `preflight.rs:733` and `agent_result.rs:2041`, and
/// deliberately NOT `hook_context_root`'s `.exists()`-filtered variant below,
/// which answers a different question (where to WRITE, so a vanished worktree
/// must degrade to somewhere writable; here a vanished worktree means the
/// evidence is gone with it, and falling back would resurrect a stale or
/// other-branch artifact as this phase's). `--no-worktree` runs fall back to
/// the project root and are unaffected. Passing the bare `project_root` here in worktree mode
/// makes the predicate read `false` unconditionally, which is exactly the
/// defect this parameter's name now guards against.
fn select_loop_back_fix(evidence_root: &Path, phase: PhaseId, state: &mut State) -> FixType {
    // 999.79 / HARDEN-03: existence alone is no longer the answer. Nothing
    // deletes, dates or invalidates `{N}-VERIFICATION.md`, so a
    // `devflow start --phase N --force` re-run checks out a branch still
    // carrying the PREVIOUS run's committed copy, and an existence-only probe
    // reads that inherited file as this run's verdict.
    //
    // What the comparison protects against, in BOTH directions — neither
    // failure is acceptable and only the pair of tests below distinguishes
    // them:
    //
    // * Too permissive (the pre-999.79 behaviour): a forced re-run inherits a
    //   previous run's verdict and dispatches `--gaps-only` against zero
    //   matching plans, which gates unresolvably. That is the same unattended
    //   stall class as DOGFOOD-01, reached from a new direction.
    // * Too strict (an always-stale rule): `--gaps-only` never fires again.
    //   That silently reverts what Phase 33 built and trades an unresolvable
    //   gate for a loop that re-runs every plan in the phase, forever. It
    //   would pass every test asserting only the stale direction.
    let current = agent_result::phase_verification_fingerprint(evidence_root, phase);
    // WR-06: read from the SAME evidence root immediately after the content, so
    // the pair is one observation of one file.
    let current_mtime = agent_result::phase_verification_mtime_nanos(evidence_root, phase);
    // WR-05: an artifact with no captured baseline is dispatched conservatively
    // (below), but silently doing so would leave an operator who upgraded a
    // binary mid-phase wondering why a gaps-only pass became a full execute.
    // Same signalling posture as IN-02's `ValidateFailureNoBaseline`, on the
    // channel that survives the run.
    if current.is_some() && !state.verification_baseline_captured {
        events::emit(
            &state.project_root.clone(),
            phase,
            "verification_baseline_absent",
            serde_json::json!({
                "dispatch": "full_execute",
                "why": "no run-start baseline was captured for this phase's \
                        {N}-VERIFICATION.md, so its provenance is unknown",
            }),
        );
    }
    if verification_authored_this_run(
        (current, current_mtime),
        (
            state.last_verification_fingerprint,
            state.last_verification_mtime_nanos,
        ),
        state.verification_baseline_captured,
        state.verification_run_nonce,
    ) {
        // Record the new observation as the baseline, so a SUBSEQUENT
        // loop-back that finds the artifact unchanged is in turn correctly
        // treated as stale. Without this, one rewrite would mark the artifact
        // fresh for the rest of the run.
        //
        // F-11: this update does NOT reach disk through the `save_state`
        // earlier in `handle_validate_outcome` — that one runs before this
        // point. It is persisted a few statements later, by the `save_state`
        // inside `prepare_loop_back_to_code`, reached via the
        // `loop_back_to_code` call on the next line at every call site. A
        // process killed in that interval loses the update, and the
        // consequence is NOT fail-safe: a later same-run check would then
        // compare against the older baseline, read an unchanged artifact as
        // fresh, and dispatch `--gaps-only` where a full execute was correct —
        // the exact 999.79 direction this rule exists to close.
        //
        // The window is ACCEPTED, not closed. It spans a handful of statements
        // with no blocking wait in it (the gate wait precedes this mutation),
        // and the only alternative is a second `save_state` on every single
        // loop-back — doubling the persistence cost of the common path to
        // defend an interval a kill can barely land in. Do not add one.
        // WR-06: both halves of the observation advance together, or the next
        // cycle would compare a new mtime against a stale one and read every
        // artifact as rewritten.
        state.last_verification_fingerprint = current;
        state.last_verification_mtime_nanos = current_mtime;
        FixType::GapsOnly
    } else {
        FixType::FullExecute
    }
}

/// Was the phase's `{N}-VERIFICATION.md` authored during THIS run?
///
/// A pure predicate over the pair (`current` fingerprint, `run_start_baseline`),
/// deliberately separated from the file read and from the [`FixType`] mapping
/// so its four rows can be pinned by a committed, re-running test
/// (`verification_freshness_truth_table_is_exhaustive`, H-1/T-35-22b). It must
/// touch neither the filesystem nor [`State`] — the moment it does, the truth
/// table stops being able to enumerate it.
///
/// The rows, and which over-correction each one catches:
///
/// * `current` is `None` — no artifact exists, so nothing was authored. The
///   phase is mid-arc and D-01's original `FullExecute` answer stands
///   unchanged. Catches an always-fresh rule.
/// * `current` is `Some`, baseline is `None`, and the baseline WAS captured —
///   an artifact exists where the run start saw none, so this run's Validate
///   agent created it. The ordinary first-verification case. Catches an
///   always-stale rule.
/// * `current` is `Some`, baseline is `None`, and the baseline was NOT captured
///   — this state file predates the field, so nobody looked (WR-05). The
///   artifact may well be the previous run's, and reading it as this run's is
///   the 999.79 stall. Answered "inherited", the conservative direction: a full
///   execute is wasteful, an unresolvable gate is not recoverable.
/// * fingerprints are EQUAL and the mtimes are equal — the artifact is
///   byte-identical to what this run last observed AND nothing has written it
///   since, so it was inherited from a previous run and its verdict must not be
///   reused (999.79). Catches an always-fresh rule.
/// * fingerprints DIFFER — Validate rewrote it during this run. Catches an
///   always-stale rule.
/// * fingerprints are equal but the MTIME advanced (WR-06) — an IDEMPOTENT
///   rewrite: the agent re-authored byte-identical content on a later failing
///   cycle. Hash-only, this read as inherited and dispatched a full execute,
///   re-running every plan in the phase on every subsequent cycle instead of
///   the gaps-only pass Phase 33 built. That is the "too strict" over-correction
///   the paragraph above claims to guard against, and it did not.
///
/// **What this cannot establish (HARDEN-03, unclassified).** It keys on content
/// change and mtime, and has no notion of provenance. An artifact written for
/// any reason OTHER than this run's Validate agent — a mid-run branch switch, a
/// worktree merge-back, an operator editing the file — reads as
/// authored-this-run and dispatches `--gaps-only`, which is the failure
/// direction HARDEN-03 exists to prevent. **The mtime input marginally WIDENS
/// that exposure**: a checkout restoring byte-identical content used to read as
/// inherited and now reads as authored. Accepted deliberately, because the case
/// it fixes is ordinary (a deterministic verification writer on cycle 2 of an
/// unresolved gap) while the cases it widens are exotic, and because the
/// too-strict direction is otherwise unbounded. Establishing real provenance
/// would need the artifact to carry a run identifier, which is a larger change
/// than 999.79 asks for.
fn verification_authored_this_run(
    current: (Option<u64>, Option<u64>),
    dispatch_baseline: (Option<u64>, Option<u64>),
    baseline_captured: bool,
    validate_dispatch_nonce: Option<u64>,
) -> bool {
    // 35.2 D-01: a missing nonce means DevFlow never stamped a Validate
    // dispatch for this state. Return false so the dispatch is FullExecute.
    if validate_dispatch_nonce.is_none() {
        return false;
    }
    let (current_hash, current_mtime) = current;
    let (baseline_hash, baseline_mtime) = dispatch_baseline;
    // WR-06. Compared ONLY when both readings exist: an mtime is unavailable on
    // a platform without `modified()`, on unreadable metadata, or past year
    // 2554, and `Some != None` would then read as "written since" on every
    // single cycle. Absent either half this degrades to content alone, which is
    // the pre-WR-06 behaviour rather than a new failure mode.
    let written_since_baseline = match (current_mtime, baseline_mtime) {
        (Some(now), Some(baseline)) => now != baseline,
        _ => false,
    };
    match (current_hash, baseline_hash) {
        (None, _) => false,
        // WR-05: `baseline_captured` is what separates "the run looked and
        // found nothing" from "this state predates the field, so nobody
        // looked". Only the first licenses reading the artifact as this run's.
        (Some(_), None) => baseline_captured,
        (Some(now), Some(baseline)) => now != baseline || written_since_baseline,
    }
}

/// Decide what happens after a Validate stage, honoring the active mode's
/// gate policy, the consecutive-failure threshold, and (18e) the immediate
/// gate an ambiguous `external_verify` outcome forces regardless of either.
///
/// 999.66: on a recorded failure, the counter now decides between beginning a
/// fresh streak and continuing the existing one, instead of always
/// continuing. It asks [`mode::consecutive_failures_made_progress`], fed by
/// [`agent_result::phase_commit_count`] read fresh against `project_root` and
/// the baseline persisted in `state.last_validate_failure_commit_count`. A
/// `true` (progress) result means new commits exist on the feature branch
/// since the last recorded failure — **not** that those commits addressed
/// what Validate reported. This narrows the ceiling's guarantee to loops that
/// produce no commits at all; it does not disable the ceiling. That is the
/// accepted weakness of the commit-count signal recorded in
/// `33-RESEARCH.md`'s D-03 Recommendation and Assumptions Log A1.
///
/// 999.77: this comment used to claim the failure direction was toward gating,
/// on the grounds that an unrunnable `git` "counts zero every cycle" so the
/// counter accumulates. **That guarantee held only while `git` stayed broken,
/// and was false for exactly one transient failure — the likelier event.** One
/// unmeasurable cycle wrote a `Some(0)` baseline; the next real count then
/// exceeded it, read as forward progress, and reset the streak to 1, buying a
/// free extension of the [`mode::MAX_CONSECUTIVE_FAILURES`] ceiling.
///
/// The guarantee the code now delivers instead: a cycle whose count could not
/// be measured — [`agent_result::phase_commit_count`] returning `None` — is
/// treated as not-progress AND leaves the recorded baseline untouched, so the
/// next real measurement is compared against the last real observation. A
/// single transient fault can no longer buy a reset. What is still NOT claimed
/// is anything about the run boundary: `State::new` zeroes both the counter
/// and the baseline on every `devflow start`, `--force` included.
///
/// CR-01: the two root-consuming reads in this function are on deliberately
/// **different** roots, and must stay that way. The `{N}-VERIFICATION.md`
/// existence probe (via [`select_loop_back_fix`]) follows the agent's cwd —
/// the phase's worktree when one is configured — because that is where the
/// Validate agent authors the artifact and `.planning/` is tracked, so it is
/// invisible from the main checkout until merge. The
/// [`agent_result::phase_commit_count`] read one branch below keeps reading
/// `project_root`, the main checkout, because git refs and the object
/// database are shared across a repository's worktrees, so a commit made in
/// the worktree is already visible from there. Retargeting the commit count
/// at the worktree would fix nothing and would break the 999.66 wiring.
pub(crate) fn handle_validate_outcome(
    project_root: &Path,
    state: &mut State,
    outcome: ValidateOutcome,
) -> Result<(), CliError> {
    // WR-08: the evidence root, resolved ONCE for every loop-back arm in this
    // function. `.planning/` is tracked and the Validate agent runs in the
    // worktree, so the `{N}-VERIFICATION.md` it authors lands on
    // `feature/phase-{N}` inside the worktree and is invisible from the main
    // checkout; the probe must therefore follow the agent's cwd. Resolving it
    // per-arm made a fourth arm's correctness depend on its author noticing
    // the pattern — the original CR-01 was exactly one call site passing the
    // wrong root. Owned (`PathBuf`, not `&Path`) so it holds no borrow of
    // `state` across the `&mut state` calls in each arm. This is NOT the root
    // the `phase_commit_count` read below uses: see the CR-01 note above.
    let evidence_root: PathBuf = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf());

    // 18e / T-18-19: an ambiguous outcome must gate IMMEDIATELY — it is
    // being adjudicated right now, not retried, so it must never fall
    // through to the counter-based `should_gate` check below and must never
    // touch `consecutive_failures`. Handled in its own arm, up front, and
    // converted to `ValidateResult` for the two variants that share the
    // rest of this function's logic (WR-03).
    let result = match outcome {
        ValidateOutcome::Ambiguous(detail) => {
            let context = format!(
                "[never-silent] validate ambiguous: {}",
                truncate_reason(&detail)
            );
            return match run_gate(project_root, state, Stage::Validate, &context)? {
                GateAction::Advance => transition(project_root, state, Stage::Ship),
                GateAction::LoopBack(_) => {
                    // Evidence root: see the single binding at the top.
                    let fix = select_loop_back_fix(&evidence_root, state.phase, state);
                    // A human adjudicated an ambiguous outcome; no commit
                    // baseline was consulted, so this arm makes no claim
                    // about one.
                    loop_back_to_code(project_root, state, fix, LoopBackReason::GateResponse)
                }
                GateAction::Abort(reason) => abort(project_root, state, &reason),
            };
        }
        ValidateOutcome::Passed => ValidateResult::Passed,
        ValidateOutcome::Failed => ValidateResult::Failed,
    };

    // IN-02: read BEFORE the recording below, which sets the baseline to
    // `Some(current)` on its measured arm and would erase the distinction
    // within the same call. `None` here means no commit baseline existed when
    // this failure was recorded — a genuine first failure of the phase, or
    // state resumed from a binary predating the baseline field. Either way the
    // failure budget is at its full width, which is the fact the event stream
    // did not carry.
    let baseline_absent = state.last_validate_failure_commit_count.is_none();

    if result == ValidateResult::Failed {
        // 999.78/WR-01 (D-07): the per-phase total accumulates on EVERY
        // recorded failure — including the arm below where the commit count
        // could not be measured, because a failure is a failure whether or
        // not it could be counted. Placed here, once, ahead of the
        // measured/unmeasured split, so neither arm can record a failure
        // without it and neither can record one twice.
        //
        // Saturating, like every other counter on `State`: an unbounded loop
        // that wrapped this to zero would silently restore an exhausted
        // budget, which is the same unreachable-ceiling class of bug 18d
        // fixed for `consecutive_failures`, just slower to show up.
        //
        // Unlike `consecutive_failures` below, nothing in this function ever
        // resets it — a commit count cannot clear it, which is the whole
        // point: the Code stage's fix command is a GSD command that commits
        // `.planning/` artifacts on cycles that changed no source.
        state.phase_validate_failures = state.phase_validate_failures.saturating_add(1);
        match agent_result::phase_commit_count(project_root, &GitFlowConfig::default(), state.phase)
        {
            Some(current) => {
                if mode::consecutive_failures_made_progress(
                    state.last_validate_failure_commit_count,
                    current,
                ) {
                    // New commits landed since the last recorded failure —
                    // this failure is the first of a new streak, not a
                    // continuation. Set to 1, not 0: the gate context rendered
                    // below interpolates the counter into a message naming how
                    // many times validation has failed, and zeroing it would
                    // make that message read zero on a real failure.
                    state.consecutive_failures = 1;
                } else {
                    // Now that the counter genuinely accumulates (18d), an
                    // unbounded loop could otherwise overflow it and wrap to
                    // 0, silently restoring the unreachable-ceiling bug in a
                    // slower form.
                    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
                }
                // The baseline advances on every recorded failure whose count
                // was actually MEASURED, regardless of which branch ran above
                // — updating it only on the progress branch would let a stale
                // low baseline report progress forever.
                state.last_validate_failure_commit_count = Some(current);
            }
            // 999.77 / A-04: the count could not be measured. Treat the cycle
            // as not-progress — an absent measurement is not evidence that
            // work landed — and, crucially, do NOT touch the baseline. Writing
            // a forged zero here is the defect: the next successful
            // measurement would then compare a real count against that zero,
            // read it as forward progress, and hand back one free reset of the
            // MAX_CONSECUTIVE_FAILURES ceiling. Leaving the baseline alone
            // means the next real measurement compares against the last real
            // observation.
            //
            // `mode::consecutive_failures_made_progress` is deliberately not
            // called on this arm and its signature is deliberately not widened
            // — there is no count to compare.
            None => {
                state.consecutive_failures = state.consecutive_failures.saturating_add(1);
            }
        }
        workflow::save_state(state)?;
        // 35.2: stamp the nonce on every Validate failure so the
        // loop-back cycle below sees it. launch_stage_inner stamps on
        // the launch path; this stamps on the already-running path
        // where launch_stage_inner already ran for THIS dispatch.
        state.verification_run_nonce =
            Some(state.verification_run_nonce.unwrap_or(0).saturating_add(1));
        workflow::save_state(state)?;
    }

    // F-6: read ONCE, and used for both the message's ceiling clause and the
    // reset below, so the two can never disagree about whether this gate was a
    // ceiling gate. Read AFTER the increment above, so it reflects the failure
    // being handled right now.
    //
    // Deliberately NOT "did a gate fire": `Stage::Validate` + `Mode::Supervise`
    // gates unconditionally, so in Supervise the two conditions overlap
    // completely. A reset keyed on gating would clear the total at every
    // Supervise failure, the accumulation would never be observable, and the
    // bound would be defeated in exactly the mode where an operator watches
    // every occurrence.
    let ceiling_gate = mode::phase_failure_ceiling_reached(state.phase_validate_failures);

    if state.mode.should_gate(
        Stage::Validate,
        state.consecutive_failures,
        state.phase_validate_failures,
    ) {
        let context = match result {
            // WR-04 (35-REVIEW): the ceiling clause belongs in BOTH arms.
            // `ceiling_gate` and `should_gate` are evaluated regardless of
            // `result`, and `Mode::should_gate` puts the ceiling predicate
            // ahead of its per-mode match — so once the budget is exhausted a
            // *passing* Validate gates too, including in Auto. Without the
            // clause the whole context read "Validation passed — approve to
            // ship?", and an operator running unattended-Auto saw a gate this
            // mode is not supposed to fire on a pass, with nothing explaining
            // why; answering it then spent the accumulated budget.
            //
            // Explaining the gate rather than suppressing it (D-07's "gate,
            // do not abort" posture): a phase that burned the whole budget
            // before passing is exactly the one worth a human look before it
            // ships, and removing an operator checkpoint is the riskier of the
            // two directions the review offered.
            ValidateResult::Passed => {
                let mut message = "Validation passed — approve to ship?".to_string();
                if ceiling_gate {
                    message.push_str(&format!(
                        " (This phase recorded {} Validate failure(s), at the per-phase ceiling of {} — that is why this gate fired. Answering it restarts the count.)",
                        state.phase_validate_failures,
                        mode::MAX_PHASE_VALIDATE_FAILURES
                    ));
                }
                message
            }
            // WR-04: the CUMULATIVE per-phase total leads, and is named as a
            // per-phase quantity. The streak follows as a clearly subordinate
            // parenthetical. The complaint being fixed is that the old text
            // interpolated only the streak, so in Supervise mode — where every
            // Validate gates — it read "Validation failed 1 time(s)" at the
            // 2nd, 5th and 9th gate alike, in the one mode where a human sees
            // every occurrence. A reader must not be able to mistake the
            // secondary number for the headline.
            ValidateResult::Failed => {
                let mut message = format!(
                    "Validation has failed {} time(s) for this phase ({} in the current consecutive streak) — human review needed.",
                    state.phase_validate_failures, state.consecutive_failures
                );
                // F-6: conditioned on the ceiling PREDICATE read directly
                // against the total — never on "a gate fired". In Supervise
                // every Validate gates, so a gate-keyed clause would appear on
                // every message and carry no information at all. The
                // comparison is not re-derived here; there is exactly one
                // implementation of it, in `mode`, read once into
                // `ceiling_gate` above.
                if ceiling_gate {
                    message.push_str(&format!(
                        " The per-phase ceiling of {} is reached: this run is paused for a human, not aborted — approve to ship, reject to loop back for another pass, or abort.",
                        mode::MAX_PHASE_VALIDATE_FAILURES
                    ));
                }
                message
            }
        };
        return match run_gate(project_root, state, Stage::Validate, &context)? {
            // D-07: the ceiling fires a gate and the run stays alive. Every
            // arm below is the same set of choices an ordinary Validate gate
            // offers — reaching the ceiling adds no abort path, because
            // aborting is destructive and irreversible relative to gating and
            // would kill a phase that may be one cycle from converging.
            GateAction::Advance => {
                reset_phase_failures_at_ceiling(project_root, state, ceiling_gate);
                transition(project_root, state, Stage::Ship)
            }
            GateAction::LoopBack(_) => {
                // Evidence root: see the single binding at the top.
                let fix = select_loop_back_fix(&evidence_root, state.phase, state);
                reset_phase_failures_at_ceiling(project_root, state, ceiling_gate);
                loop_back_to_code(project_root, state, fix, loop_back_reason(baseline_absent))
            }
            // No reset on abort: the phase is ending and `abort` clears its
            // state outright, so there is no budget left to restore.
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }

    match result {
        ValidateResult::Passed => transition(project_root, state, Stage::Ship),
        ValidateResult::Failed => {
            // The plain-Failed tail arm — the common auto-loop path, and the
            // one the Phase 29 dogfood actually hit. Evidence root: see the
            // single binding at the top.
            let fix = select_loop_back_fix(&evidence_root, state.phase, state);
            loop_back_to_code(project_root, state, fix, loop_back_reason(baseline_absent))
        }
    }
}

/// A-11 reset event two: a human answered the CEILING gate by advancing or
/// looping back, so the per-phase budget starts again (999.78, D-07).
///
/// `ceiling_gate` is the caller's single read of
/// `mode::phase_failure_ceiling_reached`, passed in rather than recomputed so
/// this reset and the message's ceiling clause cannot disagree about whether
/// the gate just answered was a ceiling gate.
///
/// The caller must never pass "a gate fired" here. Supervise gates on every
/// Validate, so that would clear the total at every failure and the bound
/// would never accumulate at all — an unbounded loop wearing a gate on every
/// cycle. The persisting write is the caller's: both arms that call this go on
/// to `transition` or `loop_back_to_code`, each of which saves state.
///
/// # Why the reset announces itself (WR-03, WR-04, 35-REVIEW)
///
/// This runs BEFORE `loop_back_to_code`, and `prepare_loop_back_to_code` reads
/// `state.phase_validate_failures` for both its `loop_back` event and its
/// console line. Those therefore report `0` — correctly, since the budget by
/// then really is zero, but it left the one moment in the run that should mark
/// the ceiling being reached with no record of the number that was reached.
/// The `Advance` arm was worse: nothing at all marked the reset.
///
/// So the pre-reset total is reported here, at the moment it is spent, on both
/// channels. The alternative the review proposed — moving the reset after
/// `loop_back_to_code` and adding a `save_state` — is NOT taken, and the reason
/// is specific: `loop_back_to_code` reaches `run_preflight`, which can `abort()`
/// (clearing the phase's state file) and return `Ok(false)`, so a following
/// `save_state` would resurrect the state file of an aborted phase as a phantom
/// active run. Reporting rather than reordering keeps the existing single
/// persisting write.
fn reset_phase_failures_at_ceiling(project_root: &Path, state: &mut State, ceiling_gate: bool) {
    if !ceiling_gate {
        return;
    }
    let spent = state.phase_validate_failures;
    state.phase_validate_failures = 0;
    events::emit(
        project_root,
        state.phase,
        "phase_failure_budget_reset",
        serde_json::json!({
            "phase_validate_failures_before": spent,
            "ceiling": mode::MAX_PHASE_VALIDATE_FAILURES,
        }),
    );
    println!(
        "per-phase Validate-failure budget reset: {spent} failure(s) recorded, \
         ceiling {} reached and answered by a human — the count restarts at zero",
        mode::MAX_PHASE_VALIDATE_FAILURES
    );
}

/// IN-02: map "was there a commit baseline when this Validate failure was
/// recorded" onto the loop-back reason, in one place, so the gated arm and the
/// ungated tail arm cannot disagree about a fact they both observed from the
/// same binding.
fn loop_back_reason(baseline_absent: bool) -> LoopBackReason {
    if baseline_absent {
        LoopBackReason::ValidateFailureNoBaseline
    } else {
        LoopBackReason::ValidateFailure
    }
}

/// Decide what happens after the Ship stage completes — always gated.
///
/// **The only site in the crate permitted to pass a non-`None` auto-response
/// to `run_gate_with_timeout`.** This is the routine Ship approval — D-04's
/// pre-authorization exists to answer exactly this gate. It must never be
/// generalized into `run_gate_with_timeout`'s own body: the reopened
/// finalization-retry gate in `finish_workflow_with_gate_timeout` is a
/// *different call*, deliberately passing `None`, because auto-approving
/// "the merge could not be completed" would retry a failing finalization
/// forever with no human ever seeing it.
pub(crate) fn handle_ship_outcome(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    let auto_response = state.yes_ship.then(|| GateResponse {
        approved: true,
        note: Some("pre-authorized by --yes-ship".to_string()),
        responded_by: Some("--yes-ship".to_string()),
    });
    match run_gate_with_timeout(
        project_root,
        state,
        Stage::Ship,
        "Ship complete — approve merge?",
        gate_timeout_secs(),
        auto_response.as_ref(),
    )? {
        GateAction::Advance => finish_workflow(project_root, state),
        GateAction::LoopBack(_) => loop_back_to_code(
            project_root,
            state,
            FixType::GapsOnly,
            LoopBackReason::GateResponse,
        ),
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}

/// Handle a non-Validate stage failure (Define/Plan/Code, or a Ship agent
/// crash routed in via [`handle_ship_failure`]). WR-11: this path must never
/// be silent — it unconditionally fires a gate + notify via [`run_gate`]
/// (independent of `Mode::should_gate`; `run_gate` marks it as an unexpected
/// gate and notifies accordingly), then lets the operator retry, loop back,
/// or abort. Deliberately kept separate from `handle_validate_outcome`: it
/// does not touch `consecutive_failures` and never auto-loops.
/// Cap a failure reason before it enters a gate context (and from there the
/// operator's notification). Reasons are agent- or parser-derived and can
/// embed arbitrary output — 13-06 dogfood finding: a multi-KB raw JSONL line
/// reached the desktop notification verbatim. Full detail stays available in
/// `.devflow/phase-NN-stdout`; the gate only needs a readable headline.
pub(crate) fn truncate_reason(reason: &str) -> String {
    render_gate_context(reason, 300)
}

/// Render agent-controlled gate text as one bounded, terminal-safe line.
pub(crate) fn render_gate_context(context: &str, max_chars: usize) -> String {
    const TRUNCATED: &str = "… [truncated; full output in .devflow/]";
    let sanitized: String = context
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    if sanitized.chars().count() <= max_chars {
        return sanitized;
    }

    let suffix_len = TRUNCATED.chars().count().min(max_chars);
    let head_len = max_chars.saturating_sub(suffix_len);
    let head: String = sanitized.chars().take(head_len).collect();
    let suffix: String = TRUNCATED.chars().take(suffix_len).collect();
    format!("{head}{suffix}")
}

pub(crate) fn handle_stage_failure(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let context = format!(
        "[never-silent] stage {stage} failed: {} — human review needed (retry, loop-to-code, or abort)",
        truncate_reason(&reason.unwrap_or_else(|| "no details available".into()))
    );
    match run_gate(project_root, state, stage, &context)? {
        GateAction::Advance => {
            // CR-01: clean up the stale gate/response/ack before retrying so
            // the retry cannot silently consume the prior response.
            let _ = Gates::cleanup(project_root, state.phase, stage);
            state.gate_pending = false;
            launch_stage(state, None, Some(stage))
        }
        GateAction::LoopBack(_) => {
            // Retry the SAME failed stage — Code is not a valid recovery
            // target before planning exists for a Define/Plan failure
            // (Codex 13-01 MEDIUM). Only Ship's ReviewFailed path (handled
            // separately in `handle_ship_failure`) actually loops to Code.
            let _ = Gates::cleanup(project_root, state.phase, stage);
            launch_stage(state, None, Some(stage))
        }
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}

/// Handle the Ship stage's failure outcome, distinguishing an agent crash
/// (`AgentFailed`) from a review rejection (`ReviewFailed`). A `review:`-
/// prefixed reason (trimmed, case-folded) is the agent-reported convention
/// for "the change was reviewed and rejected" — that loops back to Code with
/// the `/gsd-audit-fix` prompt rather than firing a gate (consensus #7).
/// Anything else is treated as an agent crash and routed through the generic
/// never-silent gate path.
pub(crate) fn handle_ship_failure(
    project_root: &Path,
    state: &mut State,
    reason: Option<String>,
) -> Result<(), CliError> {
    if is_ship_review_failure(&reason) {
        // A Ship review rejection, not a Validate failure — the commit-count
        // baseline plays no part in this decision, so it makes no claim.
        return loop_back_to_code(
            project_root,
            state,
            FixType::AuditFix,
            LoopBackReason::GateResponse,
        );
    }
    handle_stage_failure(project_root, state, Stage::Ship, reason)
}

/// Whether a Ship-stage failure `reason` is a review rejection (`review:`
/// prefix, trimmed + case-folded) rather than an agent crash. This string
/// convention is an inherent limitation of the agent-reported DEVFLOW_RESULT
/// contract (T-13-04) — verified live against a real agent in 13-06.
pub(crate) fn is_ship_review_failure(reason: &Option<String>) -> bool {
    reason
        .as_deref()
        .map(|r| r.trim().to_ascii_lowercase().starts_with("review:"))
        .unwrap_or(false)
}

/// Run a batch of hooks against the primary checkout, serialized across
/// phases by the coarse project lock (13-DEFERRED-CR-03 fix shape #3): the
/// hooks commit/tag/delete branches in the shared main checkout, and two
/// phases doing that concurrently race git's `index.lock`/`HEAD`. Held for
/// seconds — never across a gate wait. Hook failures stay fail-soft (warn
/// and continue), as before.
///
/// 14-CR-02: a lock timeout SKIPS the batch instead of running it
/// unserialized — mutating the shared checkout concurrently is the exact
/// race this lock exists to prevent, and the hooks are individually
/// fail-soft for ordinary transitions. The return value lets terminal
/// completion fail closed and preserve state when the batch was skipped or
/// a required hook failed.
/// Which tree a hook batch operates on.
///
/// The Validate→Ship transition batch (`DocsUpdate`) authors material *about
/// the branch being shipped*, so it must write into that phase's worktree —
/// otherwise its output is stranded on the base branch, uncommitted and
/// divorced from the commits it describes (found live: Phase 17's changelog
/// entry landed on `develop` while every one of its commits sat on
/// `feature/phase-17`).
///
/// The terminal batch (`Merge`, `VersionBump`, `ChangelogAppend`,
/// `BranchCleanup`) is the exact opposite: it merges the feature branch INTO
/// the base branch, tags the base branch, and deletes the feature branch.
/// Those are primary-checkout operations and retargeting them at the
/// worktree would be a correctness regression. `ChangelogAppend` moved here
/// in 17-12 (WR-04) — a release record naming a version only becomes true
/// once `VersionBump` has tagged it, so the changelog entry belongs on the
/// base branch alongside the tag, not in the worktree. Do not restore
/// 17-10's worktree targeting to this hook.
///
/// Falls back to `project_root` whenever no worktree is configured, so
/// `--no-worktree` runs are unaffected.
pub(crate) fn hook_context_root(
    project_root: &Path,
    state: &State,
    terminal_batch: bool,
) -> PathBuf {
    if terminal_batch {
        return project_root.to_path_buf();
    }
    state
        .worktree_path
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| path.to_path_buf())
        .unwrap_or_else(|| project_root.to_path_buf())
}

pub(crate) fn run_checkout_hooks(
    project_root: &Path,
    state: &State,
    batch: &[hooks::Hook],
    stage: Stage,
) -> bool {
    if batch.is_empty() {
        return true;
    }
    let _checkout_lock = match lock::acquire_project_blocking(project_root, checkout_lock_timeout())
    {
        Ok(guard) => guard,
        Err(err) => {
            println!(
                "warning: could not acquire the checkout lock ({err}) — \
                 SKIPPING hooks {batch:?} rather than mutating the checkout \
                 unserialized. Re-run them once the holder finishes."
            );
            events::emit(
                project_root,
                state.phase,
                "checkout_lock_timeout",
                serde_json::json!({ "stage": stage.to_string(), "error": err.to_string() }),
            );
            for hook in batch {
                events::emit(
                    project_root,
                    state.phase,
                    "hook_run",
                    serde_json::json!({
                        "hook": format!("{hook:?}"),
                        "ok": false,
                        "skipped": "checkout lock timeout",
                    }),
                );
            }
            return false;
        }
    };
    let git_flow = GitFlowConfig::default();
    let mut all_succeeded = true;
    let terminal_batch = batch == hooks::hooks_after_ship().as_slice();
    let hook_root = hook_context_root(project_root, state, terminal_batch);
    // Hoisted out of the loop (GAP-7): these fields are loop-invariant, and
    // VersionBump needs to hand shipped_version forward to ChangelogAppend
    // within the same batch run, which a fresh per-iteration context would
    // discard.
    let mut ctx = HookContext {
        phase: state.phase,
        project_root: hook_root.clone(),
        stage,
        git_flow: git_flow.clone(),
        shipped_version: None,
        shipped_changelog_body: None,
    };
    for hook in batch {
        let outcome = hook.run(&mut ctx);
        if let Err(ref err) = outcome {
            println!("warning: hook {hook:?} failed: {err}");
            all_succeeded = false;
        }
        events::emit(
            project_root,
            state.phase,
            "hook_run",
            serde_json::json!({
                "hook": format!("{hook:?}"),
                "ok": outcome.is_ok(),
            }),
        );
        // Terminal finalization is ordered and fail-fast. In particular, a
        // failed version/tag operation must not delete the feature branch and
        // destroy the evidence needed for a safe retry.
        if terminal_batch && outcome.is_err() {
            break;
        }
    }
    all_succeeded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_gate::prepare_loop_back_to_code;
    use crate::pipeline_launch::advance;
    use crate::test_support::*;
    use devflow_core::git::GitFlow;
    use devflow_core::mode::Mode;
    use devflow_core::prompt;
    use devflow_core::state::AgentKind;

    /// 14-CR-02: when the checkout lock cannot be acquired, the hook batch
    /// must be SKIPPED — never run unserialized against the shared checkout
    /// — and the skip must be recorded in events.jsonl. `ChangelogAppend`
    /// would observably create `CHANGELOG.md` if the batch ran; it moved
    /// from the Validate→Ship batch into `hooks_after_ship()` in 17-12
    /// (WR-04), so this test now drives that batch instead — none of its
    /// hooks execute here regardless (the lock check short-circuits before
    /// the first hook runs), so no real merge/version state is needed.
    /// Env-mutating, so serialized under ENV_MUTEX; the "0" timeout only
    /// affects a concurrent test if it is actually contended, which none are
    /// (no other test holds the project lock).
    #[test]
    fn checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A live holder (this process) keeps the lock contended; the stale-
        // holder reclaim cannot fire.
        let _held = lock::acquire_project(root).expect("hold checkout lock");
        unsafe {
            std::env::set_var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS", "0");
        }

        let state = State::new(
            PhaseId::new(33),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            std::env::remove_var("DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS");
        }

        assert!(
            !root.join("CHANGELOG.md").exists(),
            "hooks must not run while the checkout lock is held elsewhere"
        );
        let last = devflow_core::events::last_event_for_phase(root, PhaseId::new(33))
            .expect("skip must be recorded in events.jsonl");
        assert_eq!(last["event"], "hook_run");
        assert_eq!(last["ok"], false);
        assert_eq!(last["skipped"], "checkout lock timeout");
    }

    #[test]
    fn terminal_hook_failure_stops_before_branch_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(34);
        let branch = "feature/phase-34";
        let git = |args: &[&str]| {
            let output = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["branch", branch, "develop"]);
        // Force VersionBump to fail after Merge succeeds.
        std::fs::remove_file(root.join("Cargo.toml")).unwrap();
        std::fs::create_dir(root.join("Cargo.toml")).unwrap();

        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let succeeded = run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);

        assert!(!succeeded);
        assert!(
            GitFlow::new(root).branch_exists(branch),
            "a failed terminal batch must preserve the branch for retry"
        );
    }

    /// GAP-8 (17-VALIDATION.md): GAP-7 fixed `HookContext.shipped_version`
    /// forwarding `hooks_after_ship`'s `VersionBump` tag to `ChangelogAppend`
    /// within the same batch — but only the `devflow-core::hooks` unit tests
    /// exercised it directly by hand-rolling their own context and looping
    /// over `hooks_after_ship()`. `run_checkout_hooks` is the ONLY production
    /// caller of that batch, and it must construct the `HookContext` once,
    /// above the hook loop, for the forwarding to survive into production.
    /// This test drives `run_checkout_hooks` itself (not a hand-rolled loop)
    /// against a repo with no version file, and asserts the changelog
    /// heading names the actual tagged version rather than falling back to
    /// the "unreleased" literal.
    #[test]
    fn run_checkout_hooks_keeps_changelog_in_sync_with_tag_when_no_version_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo_no_version_file(root);

        let phase = PhaseId::new(47);
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        let git = |args: &[&str]| {
            let output = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        git(&["branch", &branch, "develop"]);
        std::fs::write(root.join(".gitignore"), ".devflow/\n").unwrap();
        git(&["checkout", &branch]);
        std::fs::write(root.join("feature.txt"), "phase work\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "phase work"]);
        git(&["checkout", "develop"]);

        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        let succeeded = run_checkout_hooks(root, &state, &hooks::hooks_after_ship(), Stage::Ship);
        assert!(
            succeeded,
            "after-ship batch must succeed against a clean repo"
        );

        let all_tags = devflow_core::test_support::git_command(root)
            .arg("tag")
            .output()
            .unwrap();
        let all_tags = String::from_utf8_lossy(&all_tags.stdout);
        assert_eq!(all_tags.lines().count(), 1, "expected exactly one tag");
        let tag = all_tags.trim().to_string();
        let tag_version = tag
            .strip_prefix('v')
            .expect("tag should be prefixed with v")
            .to_string();

        let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
        let changelog_version = changelog
            .lines()
            .find(|l| l.starts_with("## "))
            .and_then(|l| l.trim_start_matches("## ").split(' ').next())
            .unwrap()
            .to_string();

        assert_ne!(
            changelog_version, "unreleased",
            "changelog heading must name the tagged version, not fall back to the literal"
        );
        assert_eq!(
            changelog_version, tag_version,
            "changelog heading must match the git tag ({tag}) produced by the same \
             run_checkout_hooks call, even with no version file"
        );
    }

    /// Reaching `MAX_CONSECUTIVE_FAILURES` on a failed Validate must force a
    /// gate (even in Auto mode, which otherwise auto-loops), and an `abort`
    /// gate response must end the workflow (state cleared) without spawning a
    /// new stage (11-VALIDATION.md 12f). The gate response is pre-seeded so the
    /// poll inside `run_gate` returns immediately.
    #[test]
    fn validate_failure_threshold_forces_gate_then_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = PhaseId::new(22);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
        // Simulate that a baseline was already recorded at the (no-repo)
        // commit count of 0 — this test pre-seeds the streak directly rather
        // than driving it through repeated handle_validate_outcome calls, so
        // it must also pre-seed the forward-progress baseline 999.66 added,
        // or the fresh None-baseline would be read as "first-ever failure"
        // and reset the streak instead of continuing it.
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();

        // Pre-write a rejected response whose note says "abort" so
        // `GateAction::from_response` resolves to `Abort` rather than a
        // loop-back-to-Code.
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: requirements changed","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        // CR-01: the forced gate's request file (along with its response and
        // ack) must be cleaned up once it resolves to Abort — previously
        // only the terminal Ship-success path cleaned up gate files, leaving
        // this one on disk to be silently reused by a later gate.
        assert!(
            !Gates::gate_path(root, phase, Stage::Validate).exists(),
            "forced gate's files must be cleaned up once it resolves to Abort"
        );
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
    }

    /// Seed a Validate-stage DEVFLOW_RESULT marker (with the given verdict
    /// JSON fragment, or `None` to omit the key entirely) and drive `advance()`
    /// on a scoped thread, busy-polling for the Validate gate file to appear
    /// so its `context` text — the only externally observable signal of the
    /// `passed` value `advance()` computed from the verdict — can be read
    /// before resolving the gate with an Abort response. Forcing a gate for
    /// every case (rather than letting a `passed=true` case fall through to a
    /// bare `transition`) is deliberate: `transition`/`loop_back_to_code` both
    /// call `launch_stage`, which spawns the real configured agent CLI and
    /// must never fire from a unit test (see `ship_review_failed_loops_to_code`).
    fn drive_validate_advance_and_read_gate_context(
        root: &Path,
        phase: PhaseId,
        consecutive_failures: u32,
        verdict_json: Option<&str>,
    ) -> String {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = consecutive_failures;
        // See validate_failure_threshold_forces_gate_then_aborts: pre-seed
        // the 999.66 baseline to match the (no-repo) commit count of 0 so a
        // directly-seeded streak isn't misread as a first-ever failure.
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();

        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let marker = match verdict_json {
            Some(verdict) => {
                format!(r#"DEVFLOW_RESULT: {{"status":"success","verdict":"{verdict}"}}"#)
            }
            None => r#"DEVFLOW_RESULT: {"status":"success"}"#.to_string(),
        };
        std::fs::write(agent_result::stdout_path(root, phase), marker).unwrap();

        let gate_path = Gates::gate_path(root, phase, Stage::Validate);
        let response_path = Gates::response_path(root, phase, Stage::Validate);
        let mut context = String::new();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                advance(root, Some(phase)).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if gate_path.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "advance() must force a Validate gate, not advance silently"
            );

            context = std::fs::read_to_string(&gate_path).unwrap();

            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });

        context
    }

    /// 13b verdict-vs-ran: a Validate agent that ran successfully but found
    /// gaps (`verdict: "gaps"`) must NOT advance to Ship — `advance()`'s
    /// Validate arm must compute `passed = false` and route through
    /// `handle_validate_outcome`'s failure path (gate/loop), never Ship.
    #[test]
    fn validate_gaps_does_not_advance_to_ship() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            PhaseId::new(60),
            mode::MAX_CONSECUTIVE_FAILURES - 1,
            Some("gaps"),
        );
        assert!(
            context.contains("Validation has failed"),
            "a gaps verdict must be treated as a failed validation, not a pass: {context}"
        );
    }

    /// 13b verdict-vs-ran (consensus #1): because the Validate prompt now
    /// REQUIRES a verdict, its absence must be treated as a fail-safe
    /// (gate/loop), NOT a silent pass — this is the composition fix that
    /// closes the marker-less/verdict-less Validate → Ship false-advance.
    #[test]
    fn validate_missing_verdict_does_not_advance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            PhaseId::new(61),
            mode::MAX_CONSECUTIVE_FAILURES - 1,
            None,
        );
        assert!(
            context.contains("Validation has failed"),
            "a missing verdict must be treated as a failed validation, not a pass: {context}"
        );
    }

    /// A Validate result with an explicit `verdict: "pass"` must advance to
    /// Ship — `consecutive_failures` is pre-seeded at the gate threshold
    /// itself (rather than `threshold - 1`) because a `passed=true` result
    /// never increments the counter, so the gate must already be at the
    /// threshold to force it open without falling through to a real
    /// `transition`/`launch_stage` spawn.
    #[test]
    fn validate_pass_advances() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let context = drive_validate_advance_and_read_gate_context(
            root,
            PhaseId::new(62),
            mode::MAX_CONSECUTIVE_FAILURES,
            Some("pass"),
        );
        assert!(
            context.contains("Validation passed"),
            "an explicit pass verdict must advance to Ship: {context}"
        );
    }

    /// D-18e's "two independent signals agreeing" arm: a probe pass plus an
    /// explicit `verdict: pass` classify as `ValidateOutcome::Passed` and
    /// drive straight through to Ship — no forced gate (Auto mode,
    /// `consecutive_failures == 0`), no counter touched. PATH is
    /// neutralized under `ENV_MUTEX` (matching
    /// `consecutive_failures_reaches_ceiling_across_cycles`) so
    /// `transition`'s own `launch_stage` call cannot spawn a real agent CLI;
    /// its resulting `Err` (agent binary not found) is discarded, since
    /// `transition` mutates `state.stage` to `Ship` before that call and the
    /// mutation survives regardless of the launch outcome.
    #[test]
    fn external_verify_agreement_advances_to_ship() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(90);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Pass),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert_eq!(outcome, ValidateOutcome::Passed);

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(root, &mut state, outcome);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.stage, Stage::Ship);
        assert_eq!(
            state.consecutive_failures, 0,
            "an agreeing outcome must never touch the failure counter"
        );
    }

    /// D-18e's disagreement arm: the probe passes but the agent reports
    /// `verdict: gaps`. Must classify `Ambiguous` and gate IMMEDIATELY on
    /// the FIRST cycle — never touching `consecutive_failures` — which is
    /// what distinguishes this from `Failed`'s counter-based delayed gate
    /// and is the precise thing the binding operator decision (D-18e,
    /// T-18-19) requires. Resolved via an Abort response so no agent is
    /// ever launched.
    #[test]
    fn external_verify_disagreement_gates_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(91);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Gaps),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "an ambiguous outcome must gate on cycle one without touching the counter"
        );
        assert!(
            !Gates::gate_path(root, phase, Stage::Validate).exists(),
            "the immediate gate must resolve (and clean up) via the same abort path as any other gate"
        );
    }

    /// D-18e's ambiguous arm: the probe passes but NO agent verdict arrived
    /// at all. Same immediate-gate contract as the disagreement case above
    /// — `consecutive_failures` must stay 0.
    #[test]
    fn external_verify_no_verdict_gates_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(92);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: None,
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "an ambiguous outcome must gate on cycle one without touching the counter"
        );
    }

    /// T-34-03-01, D-06: the ONE cell whose classification the exhaustive-match
    /// rewrite actually changes. The superseded `(_, Some(Verdict::Pass))` arm
    /// was matched first and discarded the derived status entirely, so an
    /// agent-written `verdict: pass` outranked a `status` DevFlow derived
    /// itself. The rewritten match enumerates the status position, so a
    /// non-`Success` status can no longer be discarded by a verdict.
    ///
    /// **This cell is unreachable in production** — `decide_action` routes
    /// every non-`Success` status to a gate before `classify_validate_outcome`
    /// is called (`34-CONTEXT.md` D-05's amendment). It is defence in depth,
    /// pinned here because it is the only assertion in this plan that was RED
    /// before the rewrite and GREEN after it; every other cell's destination is
    /// byte-identical across the change.
    #[test]
    fn non_success_status_never_classifies_as_passed_even_with_verdict_pass() {
        for status in [
            AgentStatus::Failed,
            AgentStatus::Unknown,
            AgentStatus::RateLimited,
            AgentStatus::ResourceKilled,
            AgentStatus::AgentUnavailable,
            AgentStatus::IdleTimeout,
        ] {
            let result = agent_result::AgentResult {
                status,
                exit_code: None,
                reason: None,
                commits: None,
                summary: None,
                verdict: Some(Verdict::Pass),
                decided_by_layer: Some(0),
            };
            assert_eq!(
                classify_validate_outcome(&result),
                ValidateOutcome::Failed,
                "an agent-written verdict:pass must not outrank the derived status {status:?}"
            );
        }
    }

    /// Every `AgentStatus` variant, in the order the production match names
    /// them. Kept as an explicit literal rather than derived: if a future
    /// variant is added, the production match is already an E0004 (NC-4), and
    /// this array going stale is caught by the sweep's cell counter.
    const ALL_STATUSES: [AgentStatus; 7] = [
        AgentStatus::Success,
        AgentStatus::Failed,
        AgentStatus::Unknown,
        AgentStatus::RateLimited,
        AgentStatus::ResourceKilled,
        AgentStatus::AgentUnavailable,
        AgentStatus::IdleTimeout,
    ];

    /// The three observable states of `AgentResult::verdict`.
    const ALL_VERDICTS: [Option<Verdict>; 3] = [Some(Verdict::Pass), Some(Verdict::Gaps), None];

    /// Builds a classifier fixture from the three matrix coordinates.
    ///
    /// `decided_by_layer` is set EXPLICITLY in both directions — `Some(0)` when
    /// `layer0`, `Some(1)` otherwise — and NEVER `None`. That is load-bearing,
    /// not stylistic (T-34-03-03): the field is `#[serde(default)]` and its own
    /// doc comment reserves `None` for fixtures that do not route through the
    /// real cascade, so omitting it (or passing `None` for the false case)
    /// would make the `layer0 = false` half indistinguishable from an omission
    /// bug — `layer0` would be false in all 42 cells, both `Ambiguous` arms
    /// would go unexercised, and a regression deleting them both would pass
    /// green. `Some(1)` says "a layer other than 0 decided this", which is the
    /// real production shape for a Layer-1 result.
    fn classifier_fixture(
        layer0: bool,
        status: AgentStatus,
        verdict: Option<Verdict>,
    ) -> agent_result::AgentResult {
        agent_result::AgentResult {
            status,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict,
            decided_by_layer: if layer0 { Some(0) } else { Some(1) },
        }
    }

    /// D-08's full matrix sweep: 2 `layer0` states × 7 statuses × 3 verdict
    /// states = 42 cells, each asserted against the decision table.
    ///
    /// The 21-cell version this replaces was under-dimensioned: without the
    /// `layer0` dimension both `Ambiguous` arms go unexercised and a regression
    /// deleting them both is green. The expected-outcome table below is written
    /// independently of the production match (and is permitted its wildcard —
    /// D-06's ban is scoped to the production match's status position); the
    /// mutation controls recorded in this plan's SUMMARY are what establish
    /// that the pair actually discriminates rather than agreeing vacuously.
    #[test]
    fn classify_validate_outcome_sweeps_all_forty_two_cells() {
        let mut visited = 0_usize;

        for layer0 in [true, false] {
            for status in ALL_STATUSES {
                for verdict in ALL_VERDICTS {
                    let expected = match (layer0, status, verdict) {
                        (_, AgentStatus::Success, Some(Verdict::Pass)) => ValidateOutcome::Passed,
                        (true, AgentStatus::Success, Some(Verdict::Gaps)) => {
                            ValidateOutcome::Ambiguous(
                                "external verification passed but the agent reported gaps"
                                    .to_string(),
                            )
                        }
                        (true, AgentStatus::Success, None) => ValidateOutcome::Ambiguous(
                            "external verification passed but no agent verdict arrived".to_string(),
                        ),
                        _ => ValidateOutcome::Failed,
                    };

                    let actual =
                        classify_validate_outcome(&classifier_fixture(layer0, status, verdict));
                    assert_eq!(
                        actual, expected,
                        "cell (layer0={layer0}, status={status:?}, verdict={verdict:?}) \
                         classified as {actual:?}, expected {expected:?}"
                    );
                    visited += 1;
                }
            }
        }

        assert_eq!(
            visited, 42,
            "the sweep must visit every cell of the 2 x 7 x 3 matrix; a truncated \
             iterator or a stale ALL_STATUSES/ALL_VERDICTS array shows up here"
        );
    }

    /// NC-1, the positive control: `(_, Success, Some(Pass))` classifies as
    /// `Passed` for BOTH `layer0` values. The layer-independence is the point —
    /// this is D-18e's "two independent signals agreeing" arm, and it is also
    /// why NC-1 alone cannot catch a fixture that collapses `layer0` to one
    /// value. NC-2 and NC-3 below are what catch that.
    #[test]
    fn verdict_pass_classifies_as_passed_regardless_of_layer() {
        for layer0 in [true, false] {
            assert_eq!(
                classify_validate_outcome(&classifier_fixture(
                    layer0,
                    AgentStatus::Success,
                    Some(Verdict::Pass),
                )),
                ValidateOutcome::Passed,
                "a passing verdict on a successful stage advances regardless of which \
                 layer decided it (layer0={layer0}); this arm is deliberately \
                 layer-independent"
            );
        }
    }

    /// NC-2 and its paired mirror, deliberately in ONE test so the pair cannot
    /// be split and half-deleted.
    ///
    /// `(true, Success, Some(Gaps))` is `Ambiguous` — an external probe passed
    /// while the agent reported gaps, which is two signals disagreeing and gates
    /// immediately. `(false, Success, Some(Gaps))` is `Failed` — no probe
    /// decided it, so there is no second signal, and it must stay on the
    /// ordinary counter-based auto-loop (D-05's "what this does NOT cover";
    /// T-34-03-02).
    ///
    /// If both halves returned the same outcome the `layer0` dimension would be
    /// decorative rather than load-bearing, and a fixture that silently
    /// collapsed it would pass.
    #[test]
    fn external_verify_gaps_is_ambiguous_only_when_layer0_decided() {
        assert!(
            matches!(
                classify_validate_outcome(&classifier_fixture(
                    true,
                    AgentStatus::Success,
                    Some(Verdict::Gaps),
                )),
                ValidateOutcome::Ambiguous(_)
            ),
            "a Layer-0 probe pass against a gaps verdict is two signals disagreeing \
             and must gate immediately"
        );
        assert_eq!(
            classify_validate_outcome(&classifier_fixture(
                false,
                AgentStatus::Success,
                Some(Verdict::Gaps),
            )),
            ValidateOutcome::Failed,
            "with no Layer-0 probe there is no second signal to disagree with; this \
             must stay the ordinary auto-loop, not become an immediate gate. If this \
             half matched the layer0=true half, the layer0 dimension would be \
             decorative rather than load-bearing"
        );
    }

    /// NC-3 and its paired mirror, same shape and same reason as NC-2 above:
    /// `(true, Success, None)` is `Ambiguous` (the probe passed but no agent
    /// verdict arrived at all), while `(false, Success, None)` is `Failed` (the
    /// verdict-less Validate fail-safe, unchanged).
    #[test]
    fn external_verify_absent_verdict_is_ambiguous_only_when_layer0_decided() {
        assert!(
            matches!(
                classify_validate_outcome(&classifier_fixture(true, AgentStatus::Success, None)),
                ValidateOutcome::Ambiguous(_)
            ),
            "a Layer-0 probe pass with no agent verdict at all must gate immediately"
        );
        assert_eq!(
            classify_validate_outcome(&classifier_fixture(false, AgentStatus::Success, None)),
            ValidateOutcome::Failed,
            "with no Layer-0 probe, a missing verdict is the ordinary fail-safe and \
             must stay on the auto-loop. If this half matched the layer0=true half, \
             the layer0 dimension would be decorative rather than load-bearing"
        );
    }

    /// The DOWNSTREAM half of ROADMAP criterion 4's demonstration: the Validate
    /// shape produced AFTER plan 34-01's graft fix routes to an immediate gate,
    /// while the pre-fix laundered shape routes to Ship.
    ///
    /// **This test covers only half the route, deliberately.** The upstream half
    /// — that `reconcile_layer0_verdict` no longer manufactures the second shape
    /// out of a `{"status":"failed","verdict":"pass"}` marker — is pinned in
    /// `devflow-core` by `layer0_verdict_graft_declines_when_layer1_status_is_not_success`
    /// (plan 34-01). The two crates' tests together cover the route.
    ///
    /// **Do not "consolidate" the pair into one crate.**
    /// `classify_validate_outcome` is `pub(crate)` to `devflow-cli` and cannot be
    /// called from `devflow-core`'s test module, and `devflow-core` cannot depend
    /// on `devflow-cli`. The split is forced by visibility, not by oversight.
    ///
    /// **What the two shapes are.** Post-fix, the graft declines to transplant a
    /// failing Layer 1's verdict, so Validate presents
    /// `(decided_by_layer: Some(0), status: Success, verdict: None)` — a Layer-0
    /// probe pass with no agent verdict, which is `Ambiguous`. Pre-fix, the same
    /// marker produced `(Some(0), Success, Some(Pass))`, which is `Passed` and
    /// advances. That second classification is NOT a defect in this classifier —
    /// by the time it runs the status genuinely IS `Success` — which is exactly
    /// why criterion 3's structural fix does not and could not close criterion 4.
    ///
    /// **The routing half** is asserted here only for the gating direction (the
    /// one criterion 4 claims). The `Passed` → `Stage::Ship` direction is pinned
    /// by `external_verify_agreement_advances_to_ship`, which needs `ENV_MUTEX`
    /// and a neutralized `PATH` to keep `transition`'s `launch_stage` from
    /// spawning a real agent CLI; reproducing that here would add env mutation to
    /// an otherwise pure test for no additional coverage.
    #[test]
    fn grafted_failure_shape_gates_instead_of_shipping() {
        // The POST-graft-fix shape.
        let post_fix =
            classify_validate_outcome(&classifier_fixture(true, AgentStatus::Success, None));
        match &post_fix {
            ValidateOutcome::Ambiguous(detail) => assert!(
                detail.contains("no agent verdict"),
                "the ambiguous payload must name the missing verdict so the \
                 [never-silent] gate context says which signal was absent: {detail}"
            ),
            other => panic!("the post-fix Validate shape must gate, not ship — got {other:?}"),
        }

        // The PRE-fix laundered shape — the downstream half of the exploit,
        // asserted as such. Opposite result from the same fixture shape, which
        // is what makes the pair a demonstration rather than a restatement.
        assert_eq!(
            classify_validate_outcome(&classifier_fixture(
                true,
                AgentStatus::Success,
                Some(Verdict::Pass),
            )),
            ValidateOutcome::Passed,
            "the laundered shape classifies as Passed — this is the downstream \
             half of 999.74's exploit, closed upstream by 34-01's graft fix, not here"
        );

        // The routing half, gating direction: drive the real
        // `handle_validate_outcome` and confirm the post-fix shape does NOT
        // reach Ship. An abort response is pre-seeded so the gate resolves
        // without spawning anything (same pattern as
        // `external_verify_disagreement_gates_immediately`).
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(93);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, post_fix).unwrap();

        assert_ne!(
            state.stage,
            Stage::Ship,
            "the post-fix shape must never advance to Ship — that advance is the \
             whole of 999.74"
        );
        assert_eq!(
            state.consecutive_failures, 0,
            "an ambiguous outcome gates on cycle one without touching the counter, \
             so the operator sees an immediate gate rather than a delayed retry"
        );
    }

    /// D-08/consensus #4: a `ResourceKilled` outcome on a non-Validate stage
    /// bumps `infra_failures` and leaves `consecutive_failures` untouched —
    /// `handle_infra_outcome` (the `GateInfra` arm) never routes through
    /// `handle_validate_outcome`. A rejected/abort response is pre-seeded so
    /// the never-silent gate resolves immediately without a spawn thread.
    #[test]
    fn resource_killed_on_code_bumps_infra_failures_not_consecutive_failures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(73);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "137").unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.consecutive_failures = 1;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        // abort() clears state entirely — assert against the terminal error
        // rather than a field, and confirm no Validate gate ever appeared.
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
        assert!(!Gates::gate_path(root, phase, Stage::Validate).exists());
    }

    /// D-08/consensus #4 (Validate-stage case): a `ResourceKilled` outcome on
    /// the VALIDATE stage still bumps `infra_failures` and leaves
    /// `consecutive_failures` unchanged — proving `GateInfra`
    /// (`handle_infra_outcome`) bypasses `handle_validate_outcome` even on
    /// the one stage that normally owns `consecutive_failures`. The rejected
    /// gate response resolves the never-silent gate to `Abort` immediately
    /// (no spawn thread needed); `consecutive_failures` is asserted on the
    /// in-memory `state`, which `abort()` never mutates (it only clears the
    /// on-disk state file and gate artifacts).
    #[test]
    fn resource_killed_on_validate_bumps_infra_not_consecutive_failures() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(74);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = 2;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_infra_outcome(
            root,
            &mut state,
            Stage::Validate,
            Some("agent process was killed (exit code 137, likely OOM)".into()),
        )
        .unwrap();

        assert_eq!(state.infra_failures, 1);
        assert_eq!(
            state.consecutive_failures, 2,
            "consecutive_failures must be untouched by the infra path"
        );
    }

    /// D-08: reaching `MAX_INFRA_FAILURES` infra outcomes aborts rather than
    /// gating again.
    #[test]
    fn infra_ceiling_aborts_instead_of_gating() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(75);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();

        handle_infra_outcome(root, &mut state, Stage::Code, Some("killed".into())).unwrap();

        assert_eq!(state.infra_failures, mode::MAX_INFRA_FAILURES);
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "at the ceiling, the run must abort rather than gate again"
        );
        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(matches!(err, workflow::WorkflowError::MissingState(_)));
    }

    /// 18d — the RED-then-GREEN core of the Code↔Validate safety-gate
    /// reachability fix. Drives `MAX_CONSECUTIVE_FAILURES` real
    /// fail/Code→Validate cycles via `handle_validate_outcome` (the +1) and
    /// `transition()` (previously an unconditional reset to 0). Before the
    /// fix, `consecutive_failures` oscillates 0/1 and never reaches the
    /// ceiling; after the fix it accumulates and forces the gate.
    ///
    /// `state.stage` is forced back to `Stage::Code` before every
    /// `transition()` call so each loop iteration exercises the exact
    /// `(Code, Validate)` hop under test, independent of which internal
    /// branch `handle_validate_outcome` took on that cycle (ordinary
    /// loop-back vs. the forced gate on the final cycle) — mirrors what
    /// `prepare_loop_back_to_code` does for real on every retry.
    ///
    /// A gate response is re-seeded at the top of every loop iteration (not
    /// just once before the loop) so it survives `prepare_loop_back_to_code`'s
    /// `Gates::cleanup(.., Stage::Validate)` — which fires on every ordinary
    /// loop-back cycle once `state.stage` is `Validate` and would otherwise
    /// delete a response written only once up front before the final,
    /// gate-triggering cycle ever gets to read it. With it re-seeded every
    /// iteration, the forced gate on the final cycle resolves immediately via
    /// `Gates::poll_response` finding an already-written file, instead of
    /// waiting out the (default 7-day) gate timeout. PATH is neutralized
    /// under `ENV_MUTEX` so neither `handle_validate_outcome`'s loop-back nor
    /// `transition()`'s own `launch_stage` call risk spawning a real agent
    /// CLI, following `transition_resets_infra_failures`' established
    /// approach.
    #[test]
    fn consecutive_failures_reaches_ceiling_across_cycles() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(81);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        assert!(
            state.mode.should_gate(
                Stage::Validate,
                state.consecutive_failures,
                state.phase_validate_failures
            ),
            "reaching the ceiling must force the Auto-mode Validate gate"
        );
        assert_eq!(
            state.infra_failures, 0,
            "infra_failures must still reset unconditionally on the same hop the consecutive reset now skips"
        );
    }

    /// 999.66, ROADMAP criterion 3: a phase running three or more
    /// Code<->Validate waves in Auto mode, with a new commit landing on its
    /// feature branch before every failure, must not trip the ceiling —
    /// `handle_validate_outcome`'s reset-vs-accumulate branch must read each
    /// cycle's new commit as forward progress and restart the streak at 1.
    ///
    /// Runs `mode::MAX_CONSECUTIVE_FAILURES + 1` cycles deliberately, one
    /// more than the ceiling: a passing assertion at exactly the ceiling is
    /// also consistent with an off-by-one in the reset condition, and the
    /// extra cycle removes that reading.
    ///
    /// **What this test does NOT establish.** It proves the counter does not
    /// accumulate when new commits land between failures — it cannot
    /// distinguish a commit that fixed what Validate reported from a commit
    /// that did not. An agent that commits anything at all on every cycle
    /// resets the streak every cycle. That is the accepted, documented
    /// weakness of the commit-count signal (33-RESEARCH.md's D-03
    /// Recommendation and Assumptions Log A1; see also
    /// `handle_validate_outcome`'s own doc comment), not a gap this test
    /// closes.
    #[test]
    fn healthy_multi_wave_progress_does_not_reach_the_ceiling() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(87);
        init_repo(root);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for i in 0..(mode::MAX_CONSECUTIVE_FAILURES + 1) {
            // A real new commit BEFORE the failure is recorded, on every
            // cycle — the count observed at each failure strictly exceeds
            // the previously recorded baseline.
            commit_on_feature_branch(root, phase, &format!("wave-{i}"));
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(
            state.consecutive_failures, 1,
            "a new commit before every failure must restart the streak at 1, not accumulate it"
        );
        assert!(
            !state.mode.should_gate(
                Stage::Validate,
                state.consecutive_failures,
                state.phase_validate_failures
            ),
            "genuine forward progress must never force the Auto-mode Validate gate"
        );
    }

    /// 999.66, ROADMAP criterion 4, and the negative control for
    /// `healthy_multi_wave_progress_does_not_reach_the_ceiling` above:
    /// removing exactly one variable (the repeated commit) from an otherwise
    /// identical setup must restore the pre-fix ceiling-reaching behavior.
    ///
    /// Unlike `consecutive_failures_reaches_ceiling_across_cycles` — which
    /// runs against a root with NO git repository, so its count is zero
    /// because the branch is missing — this repository has a real
    /// `feature/phase-NN` branch carrying one commit before the loop starts,
    /// and that commit count never changes. This is a different route to "no
    /// progress" (an existing branch returning a stable non-zero count,
    /// rather than the branch-missing fallback), and only this one proves
    /// the count comparison itself works rather than the branch-missing
    /// fallback.
    ///
    /// Confirmed to pass against the pre-fix code as well as after (see
    /// 33-03-SUMMARY.md) — a negative control that only passes after the
    /// change would not be controlling for anything.
    #[test]
    fn repeated_failure_without_new_commits_still_reaches_the_ceiling() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(88);
        init_repo(root);
        // Establish the branch with a stable, non-zero commit count. No
        // further commits land during the loop below.
        commit_on_feature_branch(root, phase, "seed");

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        assert!(
            state.mode.should_gate(
                Stage::Validate,
                state.consecutive_failures,
                state.phase_validate_failures
            ),
            "a genuinely stuck loop with no new commits must still reach the reachable ceiling"
        );
    }

    /// 999.77 / HARDEN-01, ROADMAP criterion 1 — **the two-cycle
    /// discriminating sequence, and nothing less.**
    ///
    /// A single transient `git` fault used to buy one free reset of the
    /// [`mode::MAX_CONSECUTIVE_FAILURES`] ceiling. The mechanism needs two
    /// cycles to become visible, which is why a one-cycle test is a proxy
    /// (NC-3) rather than weak coverage:
    ///
    ///   - **Cycle 1** — `git` cannot run. Pre-fix, `phase_commit_count`
    ///     collapsed that to `0` and the unconditional baseline write recorded
    ///     `Some(0)`. The streak still incremented, so a test that stopped
    ///     here would look green against the buggy code.
    ///   - **Cycle 2** — `git` runs and reports the SAME real count as before
    ///     (nothing was committed between the cycles). Compared against the
    ///     forged `Some(0)` baseline, `1 > 0` reads as forward progress, and
    ///     the streak resets to 1. That reset is the defect, and it is only
    ///     observable from the second cycle.
    ///
    /// Exactly two things vary across the cycles: whether `git` can be
    /// executed, and nothing else. The commit count itself is held constant at
    /// a real, non-zero value throughout, so a green result here cannot be
    /// explained by the branch changing underneath the test.
    ///
    /// **`NoGitPath` for cycle 1, `NeutralPath` for cycle 2.** Cycle 1 needs
    /// `git` to be UNRESOLVABLE — only a spawn that fails makes `.output()`
    /// return `Err`, which is the sole could-not-measure condition (F-1); a
    /// shim that ran and exited non-zero would be a real observation and would
    /// exercise the already-correct `Some(0)` path (NC-4). Cycle 2 needs a
    /// real `git` but still no resolvable agent CLI, which is exactly what
    /// `NeutralPath`'s git-only `PATH` provides.
    ///
    /// **What this does NOT establish.** The run boundary. `State::new` zeroes
    /// both `consecutive_failures` and the baseline on every `devflow start
    /// --force`, and nothing here shows the streak surviving a restart.
    #[test]
    fn validate_failure_with_unmeasurable_count_accumulates_the_streak() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(89);
        init_repo(root);
        // One real commit on the feature branch, and no further commits for
        // the rest of this test: the REAL count is a stable, non-zero 1.
        commit_on_feature_branch(root, phase, "seed");
        // C4 (review): `handle_validate_outcome` persists via
        // `workflow::save_state`, which routes through `write_state_atomic` ->
        // `ensure_devflow_dir` (a `create_dir_all`), so this directory is not
        // strictly required. Created anyway, matching the sibling fixtures
        // that already do so — it costs one line and stops this test's
        // correctness depending on a detail two crates away.
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        // A real baseline from a real prior observation — one commit seen at
        // the last recorded failure, one failure already on the streak.
        state.last_validate_failure_commit_count = Some(1);
        state.consecutive_failures = 1;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        let seed_gate_response = || {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        };
        seed_gate_response();

        // CYCLE 1 — the measurement fails. The guard wraps exactly this call
        // and nothing else.
        {
            let _no_git = NoGitPath::install();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        }

        assert_eq!(
            state.last_validate_failure_commit_count,
            Some(1),
            "a cycle whose commit count could NOT be measured must leave the baseline \
             byte-identical to the last real observation — overwriting it with a forged \
             zero is 999.77 itself"
        );
        assert_eq!(
            state.consecutive_failures, 2,
            "an unmeasurable count is not evidence of forward progress, so the streak \
             must continue rather than restart"
        );

        // The loop-back moved the stage to Code and cleaned up the Validate
        // gate; restore both so cycle 2 is the same shape as cycle 1.
        state.stage = Stage::Validate;
        seed_gate_response();

        // CYCLE 2 — `git` runs again and reports the same real count as
        // before. Nothing was committed in between, so this is not progress.
        {
            let _neutral = NeutralPath::install();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        }

        assert_ne!(
            state.consecutive_failures, 1,
            "the streak must never be reset to 1 by this sequence — that reset is the \
             one free ceiling reset a single transient git fault used to buy"
        );
        assert_eq!(
            state.consecutive_failures, 3,
            "failure-with-an-unmeasurable-count followed by \
             failure-with-an-unchanged-real-count must accumulate"
        );
        assert!(
            state.mode.should_gate(
                Stage::Validate,
                state.consecutive_failures,
                state.phase_validate_failures
            ),
            "the human gate must stay reachable across a transient git fault"
        );
    }

    /// The loop-back gate response used by the 999.78 tests below. Deliberately
    /// NOT the `"abort: test cleanup"` note the older fixtures use:
    /// `GateAction::from_response` routes any note containing `abort` to
    /// `GateAction::Abort`, which clears the phase's state — and "the run
    /// stays alive" (D-07) is exactly what these tests have to observe.
    const LOOP_BACK_RESPONSE: &str =
        r#"{"approved":false,"note":"loop back for another pass","responded_by":"test"}"#;

    /// Read the context string of the most recent `gate_fired` event. Read from
    /// `events.jsonl` rather than from the gate file, because
    /// `prepare_loop_back_to_code` deletes the gate file on its way back to
    /// Code — a fixture reading the file would be racing its own cleanup.
    fn last_gate_context(root: &Path, phase: PhaseId) -> Option<String> {
        devflow_core::events::last_event_of_kind_for_phase(root, phase, "gate_fired")
            .and_then(|event| event["context"].as_str().map(str::to_string))
    }

    /// 999.78/WR-01, ROADMAP criterion 2 — **the bound the commit-count
    /// progress check cannot defeat.**
    ///
    /// Every cycle lands a real new commit before the failure is recorded, so
    /// `consecutive_failures_made_progress` reports progress and the streak
    /// resets to 1 on every single cycle. `MAX_CONSECUTIVE_FAILURES` is
    /// therefore unreachable by construction here — which is not an
    /// adversarial hypothetical: the Code stage's fix command is a GSD command
    /// that commits `.planning/` artifacts on cycles that changed no source.
    /// The per-phase total is the only thing that can bound this loop.
    ///
    /// Three things are asserted, and the third is the one that distinguishes
    /// D-07's gate from the abort D-07 rejected:
    ///
    ///   1. no gate fires on any cycle below the ceiling — asserted every
    ///      cycle, not merely at the end, so a gate firing early for some
    ///      unrelated reason cannot pass unnoticed;
    ///   2. a gate DOES fire on the cycle that reaches the ceiling, and its
    ///      context names the ceiling — a bare "a gate fired" would also be
    ///      satisfied by a gate fired for any other cause;
    ///   3. the phase's persisted state still exists afterwards. The run is
    ///      paused for a human, not aborted, and its work is not discarded.
    #[test]
    fn phase_validate_failure_ceiling_gates_despite_trivial_commit_progress() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(90);
        init_repo(root);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for cycle in 1..=mode::MAX_PHASE_VALIDATE_FAILURES {
            // One trivial commit per cycle — the whole premise of 999.78.
            commit_on_feature_branch(root, phase, &format!("trivial-{cycle}"));
            // Re-seeded every iteration: `prepare_loop_back_to_code` cleans up
            // the Validate gate on every ordinary loop-back, which deletes a
            // response written only once up front.
            std::fs::write(&response_path, LOOP_BACK_RESPONSE).unwrap();
            state.stage = Stage::Validate;
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);

            assert_eq!(
                state.consecutive_failures, 1,
                "cycle {cycle}: a new commit before every failure resets the streak, so the \
                 streak ceiling is unreachable here — if this is not 1 the test is no longer \
                 exercising the case 999.78 exists for"
            );

            if cycle < mode::MAX_PHASE_VALIDATE_FAILURES {
                assert_eq!(
                    state.phase_validate_failures, cycle,
                    "cycle {cycle}: the per-phase total must accumulate once per recorded failure"
                );
                assert!(
                    last_gate_context(root, phase).is_none(),
                    "cycle {cycle}: no gate may fire below the per-phase ceiling"
                );
            }
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let context = last_gate_context(root, phase)
            .expect("reaching the per-phase ceiling must fire a Validate gate");
        assert!(
            context.contains(&format!(
                "The per-phase ceiling of {} is reached",
                mode::MAX_PHASE_VALIDATE_FAILURES
            )),
            "the gate that fires at the ceiling must say so: {context}"
        );
        assert!(
            devflow_core::workflow::state_path(root, phase).exists(),
            "D-07: the ceiling fires a gate and the run STAYS ALIVE — persisted state for the \
             phase must survive. A test asserting only that a gate fired cannot tell gating \
             from aborting"
        );
    }

    /// WR-04: the gate message leads with the cumulative per-phase total, names
    /// it as a per-phase quantity, and relegates the streak to a subordinate
    /// clause.
    ///
    /// Supervise mode, because that is where the complaint lives: every
    /// Validate gates, so a human sees every occurrence, and the old text —
    /// interpolating only the streak — read "Validation failed 1 time(s)" at
    /// the 2nd, 5th and 9th gate alike. A new commit lands before every
    /// failure, so the streak is pinned at 1 while the total climbs; the two
    /// numbers therefore genuinely differ at the later gate rather than
    /// coinciding by accident.
    ///
    /// The `assert_ne!` on the two contexts is the load-bearing one: under the
    /// old message both gates produced byte-identical text, which is the defect
    /// itself and not a proxy for it.
    #[test]
    fn validate_gate_message_leads_with_the_per_phase_total() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(91);
        init_repo(root);

        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let mut first_gate_context = String::new();
        for cycle in 1..=5 {
            commit_on_feature_branch(root, phase, &format!("trivial-{cycle}"));
            std::fs::write(&response_path, LOOP_BACK_RESPONSE).unwrap();
            state.stage = Stage::Validate;
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            if cycle == 1 {
                first_gate_context = last_gate_context(root, phase)
                    .expect("Supervise gates on every Validate failure");
            }
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let fifth_gate_context =
            last_gate_context(root, phase).expect("the fifth failure must also gate in Supervise");

        assert_ne!(
            first_gate_context, fifth_gate_context,
            "WR-04: the 1st and 5th Supervise gate must not read identically — that identity \
             IS the defect"
        );

        let total_clause = "5 time(s) for this phase";
        let streak_clause = "(1 in the current consecutive streak)";
        let total_at = fifth_gate_context.find(total_clause).unwrap_or_else(|| {
            panic!(
                "the total must be reported and named as a per-phase quantity: {fifth_gate_context}"
            )
        });
        let streak_at = fifth_gate_context.find(streak_clause).unwrap_or_else(|| {
            panic!("the streak must still appear, as a subordinate clause: {fifth_gate_context}")
        });
        assert!(
            total_at < streak_at,
            "the cumulative total must LEAD the message, ahead of the streak: {fifth_gate_context}"
        );
        assert_eq!(
            state.consecutive_failures, 1,
            "the streak must genuinely differ from the total here, or the ordering assertion \
             above is comparing a number against itself"
        );
    }

    /// F-6's control: the ceiling clause is keyed on the ceiling PREDICATE, not
    /// on the fact that a gate fired.
    ///
    /// **Supervise is the mode this must be written in.** `should_gate` returns
    /// true for every `Stage::Validate` in Supervise, so a gate fires at both
    /// points below and "a gate fired" carries no information about why. An
    /// implementation that conditioned the ceiling clause on gating would
    /// stamp it on every message here — and the same version of this test
    /// written in Auto mode would pass against exactly that bug, because in
    /// Auto the below-ceiling case does not gate at all and its message is
    /// never produced.
    #[test]
    fn ceiling_clause_appears_only_at_the_ceiling_even_in_supervise_mode() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(92);
        init_repo(root);

        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        let ceiling_clause = format!(
            "The per-phase ceiling of {} is reached",
            mode::MAX_PHASE_VALIDATE_FAILURES
        );

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        // BELOW the ceiling. This gates — Supervise always does — so the
        // message exists to be inspected.
        std::fs::write(&response_path, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let below = last_gate_context(root, phase).expect("Supervise gates on every Validate");

        // AT the ceiling: one more recorded failure takes the total to the
        // ceiling exactly.
        state.phase_validate_failures = mode::MAX_PHASE_VALIDATE_FAILURES - 1;
        state.stage = Stage::Validate;
        std::fs::write(&response_path, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let at = last_gate_context(root, phase).expect("the ceiling failure must also gate");

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(
            !below.contains(&ceiling_clause),
            "a below-ceiling Supervise gate must NOT carry the ceiling clause — if it does, the \
             clause is keyed on gating rather than on the predicate: {below}"
        );
        assert!(
            at.contains(&ceiling_clause),
            "the gate at the ceiling must carry the ceiling clause: {at}"
        );
    }

    /// IN-02: a Validate failure recorded while no commit baseline exists emits
    /// a different `loop_back` reason from one recorded against an existing
    /// baseline.
    ///
    /// Both halves run in the same test and against the same phase, so the
    /// only thing that varies between them is the baseline —
    /// `last_validate_failure_commit_count` is `None` on the first failure and
    /// `Some(_)` on the second, written by the first. A test asserting only
    /// that the first reason is the no-baseline string would pass against an
    /// implementation that emitted that string unconditionally.
    #[test]
    fn loop_back_reason_is_distinct_when_no_commit_baseline_exists() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(93);
        init_repo(root);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        assert_eq!(
            state.last_validate_failure_commit_count, None,
            "the first half's premise: no baseline recorded for this phase"
        );
        workflow::save_state(&state).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let without_baseline =
            devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
                .expect("an ungated Auto failure loops back")["reason"]
                .as_str()
                .expect("the loop_back event must carry a reason")
                .to_string();

        assert!(
            state.last_validate_failure_commit_count.is_some(),
            "the second half's premise: the first failure recorded a baseline"
        );
        state.stage = Stage::Validate;
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let with_baseline =
            devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
                .expect("the second failure also loops back")["reason"]
                .as_str()
                .expect("the loop_back event must carry a reason")
                .to_string();

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_ne!(
            without_baseline, with_baseline,
            "IN-02: the absent-baseline case must be distinguishable in events.jsonl"
        );
        assert_eq!(without_baseline, "validate_failure_no_commit_baseline");
        assert_eq!(with_baseline, "validate_failure");
    }

    /// A-11 reset event TWO: operator approval at the CEILING gate clears the
    /// per-phase budget — and an ordinary below-ceiling gate does not.
    ///
    /// **Both halves run in Supervise, and that is load-bearing.** In Auto a
    /// below-ceiling Validate failure does not gate at all, so the second half
    /// would pass without ever exercising the discrimination — it would prove
    /// only that a gate that never fired did not reset anything. Supervise
    /// gates on EVERY Validate, so both halves reach a real gate answered by a
    /// real response, and the only thing that differs between them is whether
    /// the ceiling predicate is true.
    ///
    /// Without the second half, an implementation that reset on every gate
    /// would pass — and that implementation is the T-35-20b defect: the total
    /// would never accumulate in the one mode where an operator sees every
    /// occurrence.
    #[test]
    fn phase_validate_failures_reset_on_operator_approval_at_the_ceiling_gate() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        // HALF ONE — at the ceiling. One more recorded failure takes the total
        // to exactly MAX, the gate fires, and the operator loops back.
        let at_ceiling_phase = PhaseId::new(95);
        let mut at_ceiling = State::new(
            at_ceiling_phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        at_ceiling.stage = Stage::Validate;
        at_ceiling.phase_validate_failures = mode::MAX_PHASE_VALIDATE_FAILURES - 1;
        workflow::save_state(&at_ceiling).unwrap();
        let at_ceiling_response = Gates::response_path(root, at_ceiling_phase, Stage::Validate);
        std::fs::create_dir_all(at_ceiling_response.parent().unwrap()).unwrap();
        std::fs::write(&at_ceiling_response, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut at_ceiling, ValidateOutcome::Failed);

        // HALF TWO — below the ceiling, everything else identical. Supervise
        // gates here too, and the operator answers with the same loop-back.
        let below_phase = PhaseId::new(96);
        let mut below = State::new(
            below_phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        below.stage = Stage::Validate;
        below.phase_validate_failures = 2;
        workflow::save_state(&below).unwrap();
        let below_response = Gates::response_path(root, below_phase, Stage::Validate);
        std::fs::create_dir_all(below_response.parent().unwrap()).unwrap();
        std::fs::write(&below_response, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut below, ValidateOutcome::Failed);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(
            last_gate_context(root, below_phase).is_some(),
            "premise for the second half: Supervise gates on a below-ceiling failure too. If \
             no gate fired, the half proves nothing about the discrimination"
        );
        assert_eq!(
            at_ceiling.phase_validate_failures, 0,
            "a human answered the CEILING gate, so the per-phase budget starts again"
        );
        assert_eq!(
            below.phase_validate_failures, 3,
            "an ordinary below-ceiling Supervise gate must leave the total untouched — a reset \
             on every gate would clear it at every failure and the bound would never accumulate"
        );
        assert_eq!(
            workflow::load_state(root, at_ceiling_phase)
                .expect("the ceiling gate must leave the run alive, not abort it")
                .phase_validate_failures,
            0,
            "the reset must be persisted, not merely in memory — the next process reads the file"
        );
    }

    /// WR-04 (35-REVIEW): a *passing* Validate at the ceiling gates too, and
    /// the gate must say why.
    ///
    /// `ceiling_gate` and `should_gate` are both evaluated regardless of the
    /// result, and `Mode::should_gate` puts the ceiling predicate ahead of its
    /// per-mode match — so an exhausted budget gates a pass as well as a
    /// failure. The ceiling clause was appended only inside the `Failed` arm,
    /// so an operator running unattended-Auto got a gate whose entire context
    /// was "Validation passed — approve to ship?" from a mode that is not
    /// supposed to gate on a pass, and answering it spent the budget.
    ///
    /// **The control is the Supervise half, not a below-ceiling Auto pass.**
    /// Auto below the ceiling does not gate on a pass at all, so that half
    /// would assert only that a gate which never fired printed no clause — it
    /// could not tell "clause at the ceiling" from "clause always". Supervise
    /// gates on EVERY Validate, so the below-ceiling half there produces a real
    /// gate with a real message that must NOT carry the clause.
    #[test]
    fn a_passing_validate_at_the_ceiling_explains_why_it_gated() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        // AUTO, at the ceiling, and the Validate PASSED. Nothing here is a
        // failure: the budget is spent from earlier cycles and no increment
        // happens on this call.
        let auto_phase = PhaseId::new(88);
        let mut auto = State::new(
            auto_phase,
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        auto.stage = Stage::Validate;
        auto.phase_validate_failures = mode::MAX_PHASE_VALIDATE_FAILURES;
        workflow::save_state(&auto).unwrap();
        let auto_response = Gates::response_path(root, auto_phase, Stage::Validate);
        std::fs::create_dir_all(auto_response.parent().unwrap()).unwrap();
        std::fs::write(&auto_response, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut auto, ValidateOutcome::Passed);

        // CONTROL — SUPERVISE, below the ceiling, also a pass. Gates (Supervise
        // always does), so there IS a message, and it must not blame a ceiling
        // that was never reached.
        let supervise_phase = PhaseId::new(89);
        let mut supervise = State::new(
            supervise_phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        supervise.stage = Stage::Validate;
        supervise.phase_validate_failures = 2;
        workflow::save_state(&supervise).unwrap();
        let supervise_response = Gates::response_path(root, supervise_phase, Stage::Validate);
        std::fs::create_dir_all(supervise_response.parent().unwrap()).unwrap();
        std::fs::write(&supervise_response, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut supervise, ValidateOutcome::Passed);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let auto_context = last_gate_context(root, auto_phase)
            .expect("premise: an exhausted budget gates even in Auto, even on a pass");
        assert!(
            auto_context.contains("per-phase ceiling"),
            "an Auto-mode gate on a PASS is unexplained without the ceiling clause — the \
             operator has no way to know why the run stopped: {auto_context:?}"
        );

        let supervise_context = last_gate_context(root, supervise_phase)
            .expect("premise for the control: Supervise gates on every Validate");
        assert!(
            !supervise_context.contains("per-phase ceiling"),
            "NEGATIVE CONTROL: a clause appended to every passing gate would carry no \
             information at all: {supervise_context:?}"
        );
    }

    /// WR-03 (35-REVIEW): the run's record of the ceiling being reached must
    /// carry the number that was reached.
    ///
    /// The reset runs before `loop_back_to_code`, and `prepare_loop_back_to_code`
    /// reads the already-zeroed total for both its `loop_back` event and its
    /// console line — so `events.jsonl` recorded `"phase_validate_failures": 0`
    /// for the one loop-back in the phase that spent the whole budget, and the
    /// operator read `looping back to Code (0 validate failure(s) this phase)`
    /// immediately after being told the ceiling was hit. In the `Advance` arm
    /// nothing marked the reset at all.
    ///
    /// Asserted on `events.jsonl` rather than the console, since that is the
    /// durable record and the one a later reconciliation reads.
    ///
    /// **The below-ceiling half is the negative control**, and it is what
    /// distinguishes this from an event emitted on every gate: an ordinary
    /// Supervise Validate gate spends no budget, so it must emit nothing. Both
    /// halves run in Supervise for the reason the sibling reset test documents
    /// — in Auto the below-ceiling failure would not gate at all and the
    /// control would prove only that a gate which never fired stayed silent.
    #[test]
    fn the_ceiling_reset_records_the_total_it_spent() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        // AT the ceiling: this failure takes the total to exactly MAX.
        let at_ceiling_phase = PhaseId::new(91);
        let mut at_ceiling = State::new(
            at_ceiling_phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        at_ceiling.stage = Stage::Validate;
        at_ceiling.phase_validate_failures = mode::MAX_PHASE_VALIDATE_FAILURES - 1;
        workflow::save_state(&at_ceiling).unwrap();
        let at_ceiling_response = Gates::response_path(root, at_ceiling_phase, Stage::Validate);
        std::fs::create_dir_all(at_ceiling_response.parent().unwrap()).unwrap();
        std::fs::write(&at_ceiling_response, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut at_ceiling, ValidateOutcome::Failed);

        // NEGATIVE CONTROL — below the ceiling, everything else identical.
        let below_phase = PhaseId::new(92);
        let mut below = State::new(
            below_phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        below.stage = Stage::Validate;
        below.phase_validate_failures = 2;
        workflow::save_state(&below).unwrap();
        let below_response = Gates::response_path(root, below_phase, Stage::Validate);
        std::fs::create_dir_all(below_response.parent().unwrap()).unwrap();
        std::fs::write(&below_response, LOOP_BACK_RESPONSE).unwrap();
        let _ = handle_validate_outcome(root, &mut below, ValidateOutcome::Failed);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert!(
            last_gate_context(root, below_phase).is_some(),
            "premise for the control: Supervise gates on a below-ceiling failure too"
        );

        let reset = devflow_core::events::last_event_of_kind_for_phase(
            root,
            at_ceiling_phase,
            "phase_failure_budget_reset",
        )
        .expect("spending the whole per-phase budget must leave a record of it");
        assert_eq!(
            reset["phase_validate_failures_before"].as_u64(),
            Some(u64::from(mode::MAX_PHASE_VALIDATE_FAILURES)),
            "the record must carry the total that was SPENT, not the zero it was reset to"
        );

        assert!(
            devflow_core::events::last_event_of_kind_for_phase(
                root,
                below_phase,
                "phase_failure_budget_reset",
            )
            .is_none(),
            "NEGATIVE CONTROL: an ordinary below-ceiling gate spends no budget and must \
             emit nothing — an event on every gate would carry no information"
        );
    }

    /// `HARDEN-02 precision`: the per-phase total accumulates with a saturating
    /// add, so an exhausted budget can never wrap back to zero and silently
    /// restore itself.
    ///
    /// Asserted through the OPERATOR-FACING message rather than through
    /// `state.phase_validate_failures` after the call. At `u32::MAX` the
    /// ceiling is already reached, so this failure gates, and a ceiling gate
    /// answered with a loop-back resets the total to zero by design (Task 3) —
    /// a post-call read of the field would be reading that reset, not the
    /// increment. The message is rendered from the incremented value before the
    /// gate opens, so it reports what the add produced: `u32::MAX` if it
    /// saturated, `0` if it wrapped.
    #[test]
    fn phase_validate_failures_increment_saturates() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(94);
        init_repo(root);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.phase_validate_failures = u32::MAX;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(&response_path, LOOP_BACK_RESPONSE).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let context = last_gate_context(root, phase)
            .expect("a total at u32::MAX is past the ceiling, so this must gate");
        assert!(
            context.contains(&format!("failed {} time(s) for this phase", u32::MAX)),
            "the total must saturate at u32::MAX, not wrap: {context}"
        );
        assert!(
            !context.contains("failed 0 time(s) for this phase"),
            "a wrapped total would silently restore an exhausted budget: {context}"
        );
    }

    /// HARDEN-07 / criterion 6 at the LAYER level (D-09): `exit_code = 0` +
    /// `Stage::Code` (a commit-gated stage) + an unrunnable `git` must fall
    /// through to Layer 3, not classify as `Failed — no work done`. An
    /// unmeasurable count is not evidence that no work happened, and this is
    /// the exact input that made a successful agent read as a failure.
    ///
    /// `agent_result::tests::evaluate_layer2_exit_zero_no_commits_is_failed`
    /// is the required NC-11 opposite-result control and stays byte-unchanged:
    /// real `git`, a genuinely empty branch, still `Failed`. Extending THAT
    /// test instead of adding this sibling would have been the proxy NC-11
    /// names — it covers the ordinary `commits == 0` case, which is a
    /// different and already-correct path.
    ///
    /// # Why this test lives in `devflow-cli` rather than beside its subject
    ///
    /// **`NoGitPath` is unavoidable here**, unlike the Layer 3 tests which use
    /// an unspawnable working directory: `evaluate_layer2` reads its exit file
    /// from `project_root`, so a non-existent root would make the exit read
    /// fail and return `Ok(None)` for the WRONG reason — an unreadable exit
    /// file rather than an unmeasurable count — and the test would pass
    /// against the unfixed code. The root must EXIST and `git` must still be
    /// unresolvable, and only a `PATH` guard delivers that combination.
    ///
    /// A process-global `PATH` guard is not viable in `devflow-core`'s test
    /// binary. That crate shells out to `git` from eight modules running in
    /// parallel, and its tests call production code that spawns `git`
    /// directly, so no fixture-helper lock can cover them — measured at 1-5
    /// unrelated failures per run, and still 1 in 8 runs after that module's
    /// own `git()` helper took a lock. `devflow-cli`'s binary routes every
    /// `PATH` mutation through the single [`env_lock`] its `git`-touching
    /// tests already hold, which is why the guard is safe here.
    ///
    /// `evaluate_layer2` is `pub`, so this drives exactly the same function
    /// with exactly the same inputs; only the binary it runs in differs.
    #[test]
    fn evaluate_layer2_unrunnable_git_falls_through_to_layer3() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // The root EXISTS and carries a readable exit file recording a clean
        // exit — so an `Ok(None)` return can only come from the commit count.
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, PhaseId::new(4)), "0").unwrap();
        assert!(
            agent_result::exit_code_path(root, PhaseId::new(4)).exists(),
            "the exit file must be readable, or Layer 2 returns Ok(None) for the wrong reason"
        );

        let result = {
            let _no_git = NoGitPath::install();
            agent_result::evaluate_layer2(
                root,
                PhaseId::new(4),
                &GitFlowConfig::default(),
                Stage::Code,
            )
            .unwrap()
        };

        assert!(
            result.is_none(),
            "an unmeasurable commit count must fall through to Layer 3, got: {result:?}"
        );
        // Asserted separately and explicitly, so a future change that returns
        // some other non-`Failed` classification from this layer still has to
        // confront this test rather than slipping past an `is_none()` check.
        assert_ne!(
            result.as_ref().map(|r| r.status),
            Some(devflow_core::agent_result::AgentStatus::Failed),
            "Layer 2 must never classify an unmeasurable count as absent work"
        );
    }

    /// **CR-01 (35-REVIEW), the direction whose absence let the defect ship.**
    ///
    /// The fall-through above was placed ABOVE the exit-code classification,
    /// so an unmeasurable count discarded the `ResourceKilled` verdict too.
    /// That is not a cosmetic loss: Layer 2 is the SOLE classifier for exit
    /// 137 and 127 (a SIGKILLed agent writes no `DEVFLOW_RESULT` marker, so
    /// Layer 1 declines, and Layer 3 has no `ResourceKilled` arm), and the
    /// host fault that OOM-kills an agent is the same one that makes the
    /// `fork` for `git` fail — the two arrive TOGETHER, which is why no test
    /// pairing them existed and why the case is not exotic.
    ///
    /// # This test and its predecessor are one measurement, not two
    ///
    /// `evaluate_layer2_unrunnable_git_falls_through_to_layer3` directly above
    /// is this test's NC-4 negative control and vice versa. They install the
    /// **same** `NoGitPath` guard against the **same** fixture shape and
    /// differ in exactly one byte of input — the exit code written to
    /// `phase-NN-exit`. One must return `None`; the other must return
    /// `ResourceKilled`. A suite asserting only the fall-through cannot tell
    /// those apart, which is precisely how six executors, 942 passing tests
    /// and four clean gates all missed this.
    ///
    /// `Action::GateInfra` is asserted rather than left implicit because the
    /// routing is the harm: `GateReview` sends an infra fault into
    /// `handle_validate_outcome`, which `pipeline_launch.rs` documents as
    /// forbidden (review consensus #4 / D-08) — it bumps `consecutive_failures`
    /// and the per-phase Validate budget for a fault the agent did not cause,
    /// while `infra_failures` never accumulates and the infra abort ceiling
    /// becomes unreachable.
    #[test]
    fn evaluate_layer2_unrunnable_git_still_classifies_exit_137_as_resource_killed() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Identical to the fall-through test's fixture except for the exit
        // code: the root EXISTS and the exit file is readable, so `git` being
        // unrunnable is the only difference from a healthy run.
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, PhaseId::new(4)), "137").unwrap();

        let result = {
            let _no_git = NoGitPath::install();
            agent_result::evaluate_layer2(
                root,
                PhaseId::new(4),
                &GitFlowConfig::default(),
                Stage::Code,
            )
            .unwrap()
        };

        let result = result.expect(
            "exit 137 is classified from the exit code alone — an unmeasurable commit \
             count must not discard the verdict and fall to a layer that cannot produce it",
        );
        assert_eq!(
            result.status,
            AgentStatus::ResourceKilled,
            "Layer 2 is the only classifier for 137; losing it here loses it everywhere"
        );
        assert_eq!(result.exit_code, Some(137));
        assert_eq!(
            result.commits, None,
            "'could not tell' must not be recorded as a measured zero"
        );
        assert_eq!(
            devflow_core::outcome_policy::decide_action(Stage::Code, result.status),
            devflow_core::outcome_policy::Action::GateInfra,
            "an OOM-killed agent must route to the infra gate, NOT into the Validate loop"
        );
    }

    /// CR-01's second, independent harm, in the same shape: a NON-commit-gated
    /// stage that exited 0 never consulted the commit count in the first
    /// place, so an unmeasurable count must not turn its documented `Success`
    /// into `Unknown` → `GateReview`. For `Stage::Validate` that mis-routing
    /// dispatches a cleanly-exited Validate agent as `ValidateOutcome::Failed`.
    ///
    /// The negative control is
    /// `evaluate_layer2_unrunnable_git_falls_through_to_layer3`: same guard,
    /// same exit code 0, and the ONLY difference is the stage — `Code` (commit
    /// gated, must fall through) versus `Validate` (not gated, must succeed).
    /// If both stages produced the same answer, the stage scoping would be
    /// doing nothing and neither test would mean anything.
    #[test]
    fn evaluate_layer2_unrunnable_git_keeps_success_for_a_non_commit_gated_stage() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, PhaseId::new(4)), "0").unwrap();

        let result = {
            let _no_git = NoGitPath::install();
            agent_result::evaluate_layer2(
                root,
                PhaseId::new(4),
                &GitFlowConfig::default(),
                Stage::Validate,
            )
            .unwrap()
        };

        let result = result.expect(
            "a stage that is not commit-gated never read the count, so an unmeasurable \
             count cannot change its answer",
        );
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.commits, None);
        assert_eq!(
            devflow_core::outcome_policy::decide_action(Stage::Validate, result.status),
            devflow_core::outcome_policy::Action::Advance
        );
    }

    /// **HARDEN-07 / criterion 6 as an OUTCOME rather than a property of one
    /// function (F-4).** This is the test the operator ruled must exist, and
    /// the one whose absence let the Layer 3 defect survive planning.
    ///
    /// A unit test on `evaluate_layer2` alone passes while the end-to-end
    /// answer is unchanged: `evaluate_layer1` returns `None` when there is no
    /// capture, so everything reaching Layer 2 also reaches Layer 3, and
    /// Layer 3 used to carry its own copy of the same lossy count. Driving the
    /// whole cascade is what distinguishes "the defect was removed" from "the
    /// defect was moved one layer down". NC-12 is this test's control.
    ///
    /// Both cascade shortcuts are left unarmed deliberately, so the run really
    /// does traverse Layer 2's fall-through: no operator-approved external
    /// post-condition declarations (Layer 0 declines) and no stdout capture
    /// (Layer 1 returns `None`). `decided_by_layer` is asserted for the same
    /// reason — it is what proves the cascade reached Layer 3 rather than
    /// short-circuiting somewhere harmless and passing for the wrong reason.
    ///
    /// **Nothing here asserts on `Action`, `decide_action`, or any gating
    /// consequence (F-5).** `AgentStatus::Failed` and `AgentStatus::Unknown`
    /// map identically to `Action::GateReview` today, deliberately, so such an
    /// assertion would pass against the buggy code too. What this fix changes
    /// is the recorded classification, the commit figure and the
    /// operator-facing reason — not what the run does next.
    #[test]
    fn evaluate_agent_result_with_unrunnable_git_does_not_report_failed() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(96);
        // A real repo with the feature branch present. Built BEFORE the guard
        // goes on, because building it shells out to git.
        init_repo(root);
        commit_on_feature_branch(root, phase, "seed");
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "0").unwrap();
        // No stdout capture, so Layer 1 declines and the cascade reaches
        // Layer 2 at all.
        assert!(
            !agent_result::stdout_path(root, phase).exists(),
            "Layer 1 must decline, or this test never reaches the cascade under study"
        );

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;

        let result = {
            let _no_git = NoGitPath::install();
            agent_result::evaluate_agent_result(root, &state, &GitFlowConfig::default()).unwrap()
        };

        assert_ne!(
            result.status,
            devflow_core::agent_result::AgentStatus::Failed,
            "END TO END: exit 0 + Stage::Code + an unrunnable git must not report Failed. \
             This is the criterion-6 outcome, and a passing evaluate_layer2 unit test does \
             not establish it"
        );
        assert_eq!(
            result.status,
            devflow_core::agent_result::AgentStatus::Unknown,
            "asserted positively too, so a future non-Failed value still confronts this test"
        );
        assert_eq!(
            result.decided_by_layer,
            Some(3),
            "the cascade must genuinely traverse Layer 2's fall-through into Layer 3 — \
             any other layer means this passed for the wrong reason"
        );
        assert_eq!(
            result.commits, None,
            "'could not tell' must not be recorded as a measured zero"
        );
    }

    /// The companion opposite-result case for
    /// `evaluate_agent_result_with_unrunnable_git_does_not_report_failed`: the
    /// same fixture shape with real `git` available and the feature branch
    /// genuinely empty must still report `Failed`. Without it, a cascade that
    /// returned `Unknown` unconditionally would pass the test above.
    ///
    /// Kept as its own `#[test]` rather than appended to that one, following
    /// this file's own IN-06 precedent: packed into a single function, a
    /// failure in the first half aborts before the control ever runs, and the
    /// control is the half whose loss would be least visible.
    ///
    /// The decision lands at Layer 2 here, not Layer 3 — with a real
    /// measurement the commit gate resolves the case before the fall-through
    /// is reached. That difference is the point: the two tests differ in
    /// exactly one input, whether `git` could run.
    #[test]
    fn evaluate_agent_result_with_real_git_and_empty_branch_still_reports_failed() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(97);
        init_repo(root);
        // The feature branch exists but sits at develop's tip: a real,
        // measured zero rather than an unmeasurable one.
        let branch = format!("feature/phase-{padded}", padded = phase.padded());
        assert!(
            devflow_core::test_support::git_command(root)
                .args(["checkout", "-b", &branch])
                .output()
                .unwrap()
                .status
                .success(),
            "fixture must create the empty feature branch"
        );
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "0").unwrap();
        assert!(
            !agent_result::stdout_path(root, phase).exists(),
            "Layer 1 must decline here too, matching the companion test's shape"
        );

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;

        // `NeutralPath`, not the raw environment: real `git` must resolve so
        // the count is genuinely measured, while no real agent CLI can.
        let result = {
            let _neutral = NeutralPath::install();
            agent_result::evaluate_agent_result(root, &state, &GitFlowConfig::default()).unwrap()
        };

        assert_eq!(
            result.status,
            devflow_core::agent_result::AgentStatus::Failed,
            "a MEASURED zero on a commit-gated stage is still absent work — the fix must \
             not have widened into 'never report Failed'"
        );
        assert_eq!(result.commits, Some(0));
        assert_eq!(result.decided_by_layer, Some(2));
    }

    /// D-01 (33-CONTEXT.md), ROADMAP criterion 1: a Validate failure on a
    /// phase with no `{N}-VERIFICATION.md` must loop back to Code with the
    /// plain `/gsd-execute-phase {N}` command, not `--gaps-only` (which
    /// matches zero plans and gates unresolvably on a mid-arc phase). Drives
    /// the plain-Failed tail arm directly — the common auto-loop path, and
    /// the one the Phase 29 dogfood actually hit.
    #[test]
    fn mid_arc_loop_back_issues_plain_execute_command() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(82);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // Deliberately no `.planning/phases/{phase:02}-*/{phase:02}-VERIFICATION.md`
        // — this is the mid-arc precondition.

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        // The launch fails by design under a neutralized PATH (no real agent
        // CLI can spawn) — the `loop_back` event is emitted before that, so
        // the resulting `Err` is discarded, matching the established shape
        // in `consecutive_failures_reaches_ceiling_across_cycles`.
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert_eq!(
            last["fix"], "FullExecute",
            "a mid-arc phase (no {{N}}-VERIFICATION.md) must dispatch FullExecute, not GapsOnly"
        );
    }

    /// D-01 (33-CONTEXT.md), ROADMAP criterion 2: a Validate failure on a
    /// phase whose `{N}-VERIFICATION.md` already exists must still loop back
    /// with `--gaps-only` — unchanged from the pre-fix behavior. This is the
    /// negative control for `mid_arc_loop_back_issues_plain_execute_command`:
    /// identical drive, opposite precondition, opposite outcome — neither
    /// test is meaningful without the other.
    #[test]
    fn genuine_gaps_loop_back_still_issues_gaps_only() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(83);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        // WR-05 (35-REVIEW): stands in for `start()`'s run-start capture,
        // which this fixture does not go through. A hand-built `State` leaves
        // `verification_baseline_captured` false — the shape of state written
        // by a binary predating the field — and that is a DIFFERENT scenario
        // (an artifact of unknown provenance) from the one under test here.
        state.verification_baseline_captured = true;
        state.verification_run_nonce = Some(1);
        workflow::save_state(&state).unwrap();

        let phase_dir = root
            .join(".planning/phases")
            .join(format!("{padded}-test", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded())),
            "verified\n",
        )
        .unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert_eq!(
            last["fix"], "GapsOnly",
            "a phase with an existing {{N}}-VERIFICATION.md must still dispatch GapsOnly"
        );
    }

    /// CR-01 (33-REVIEW.md / 33-VERIFICATION.md), ROADMAP criterion 2 in
    /// DevFlow's *normal* operating mode: a Validate failure on a phase whose
    /// `{N}-VERIFICATION.md` exists only inside that phase's worktree must
    /// still dispatch `--gaps-only`. `.planning/` is a tracked directory and
    /// the Validate agent runs in the worktree, so the artifact it authors
    /// lands on `feature/phase-{N}` inside the worktree tree and is simply
    /// absent from the main checkout for the phase's entire in-flight
    /// duration. There is no merge-back, so this is the steady state, not a
    /// race.
    ///
    /// This is the first test in the workspace to configure
    /// `state.worktree_path` on a `handle_validate_outcome` drive — which is
    /// precisely why CR-01 survived a green suite. Every one of Phase 33's
    /// eight pre-existing loop-back tests leaves `worktree_path` at
    /// `State::new()`'s default of `None`, making `project_root` and "the root
    /// the agent actually wrote to" identical by construction: the one
    /// condition under which this defect is invisible.
    ///
    /// Companion to `genuine_gaps_loop_back_still_issues_gaps_only` directly
    /// above (the `--no-worktree` case), never a replacement for it.
    #[test]
    fn worktree_mode_genuine_gaps_loop_back_issues_gaps_only() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(93);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        // IN-07: `handle_validate_outcome` is only reached from `advance` with
        // `stage == Validate`, so that is the only stage this fixture may
        // honestly claim — otherwise `prepare_loop_back_to_code` cleans up the
        // Code gate and emits `"from": "Code"` on a Validate loop-back.
        state.stage = Stage::Validate;
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        std::fs::create_dir_all(&worktree).unwrap();
        state.worktree_path = Some(worktree.clone());
        // WR-05 (35-REVIEW): stands in for `start()`'s run-start capture,
        // which this fixture does not go through. A hand-built `State` leaves
        // `verification_baseline_captured` false — the shape of state written
        // by a binary predating the field — and that is a DIFFERENT scenario
        // (an artifact of unknown provenance) from the one under test here.
        state.verification_baseline_captured = true;
        state.verification_run_nonce = Some(1);
        workflow::save_state(&state).unwrap();

        // The artifact is written under the WORKTREE only. The bare tempdir
        // root deliberately has no `.planning` directory at all, so it is
        // structurally impossible to satisfy the probe from the main
        // checkout — that is the entire point of this test.
        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-test", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded())),
            "verified\n",
        )
        .unwrap();

        // WR-05: RAII, so a panic inside the region restores PATH by `Drop`
        // rather than by a trailing statement the unwind would skip. Scoped so
        // the restore still happens before the assertions below, exactly as
        // the trailing-statement form did.
        {
            let _path_guard = NeutralPath::install();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        }

        // Read the event from `root`, not the worktree: `events::emit` is
        // called with `project_root`, and `state.rs`'s own doc comment says
        // state and capture files always live under the main project root.
        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert_eq!(
            last["fix"], "GapsOnly",
            "a {{N}}-VERIFICATION.md existing only in the phase's worktree must still dispatch GapsOnly"
        );
    }

    /// Scenario A of the mirrored negative control for
    /// `worktree_mode_genuine_gaps_loop_back_issues_gaps_only` directly above:
    /// identical worktree setup, opposite artifact precondition, opposite
    /// required outcome. Without it, that test cannot be told apart from an
    /// implementation that returns `GapsOnly` whenever a worktree happens to
    /// be configured at all — a control that cannot fail is not a control.
    ///
    /// Phase 94: no `{N}-VERIFICATION.md` anywhere — neither under the
    /// worktree nor under the main checkout. The mid-arc precondition, in
    /// worktree mode. ROADMAP criterion 1.
    ///
    /// IN-06: scenario B lives in its own `#[test]`
    /// (`worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator`)
    /// rather than below this test's assertion. Packed into one function, a
    /// failure HERE aborted before B ran, and B is the suite's only
    /// discriminator against a probe-both-roots implementation — the single
    /// test whose loss would be least visible is exactly the one a shared
    /// function hid.
    #[test]
    fn worktree_mode_mid_arc_loop_back_issues_plain_execute() {
        let _guard = env_lock();

        let dir_a = tempfile::tempdir().unwrap();
        let root_a = dir_a.path();
        let phase_a = PhaseId::new(94);
        let mut state_a = State::new(phase_a, AgentKind::Claude, Mode::Auto, root_a.to_path_buf());
        // IN-07: Validate is the only stage production reaches this call from.
        state_a.stage = Stage::Validate;
        let worktree_a = root_a.join(format!(".worktrees/phase-{phase_a}"));
        std::fs::create_dir_all(&worktree_a).unwrap();
        state_a.worktree_path = Some(worktree_a.clone());
        workflow::save_state(&state_a).unwrap();

        // Deliberately no `{phase:02}-VERIFICATION.md` under the worktree AND
        // deliberately none under the bare root either — this is the mid-arc
        // precondition expressed in worktree mode, not an oversight.

        // WR-05: RAII PATH neutralization. This drive reaches
        // `loop_back_to_code` -> `launch_stage`, so it must never be able to
        // resolve a real agent CLI.
        {
            let _path_guard = NeutralPath::install();
            let _ = handle_validate_outcome(root_a, &mut state_a, ValidateOutcome::Failed);
        }

        let last_a =
            devflow_core::events::last_event_of_kind_for_phase(root_a, phase_a, "loop_back")
                .expect("scenario A loop_back event must be recorded");
        assert_eq!(
            last_a["fix"], "FullExecute",
            "no {{N}}-VERIFICATION.md in the worktree (nor anywhere else) must dispatch FullExecute"
        );
    }

    /// **The only test in the workspace that fails a probe-both-roots-and-OR-them
    /// implementation.** Scenario B of the mirrored negative control, split out
    /// of `worktree_mode_mid_arc_loop_back_issues_plain_execute` per IN-06 so
    /// that a scenario-A failure can never abort the run before it is asserted.
    ///
    /// Phase 95: `{N}-VERIFICATION.md` present under the **main checkout
    /// only**, never under the worktree. This scenario is a deliberate
    /// addition beyond the verification's prescribed pair, because an
    /// implementation that probes *both* roots — a plausible and superficially
    /// safer misreading of the fix — passes the positive test, passes scenario
    /// A, and passes both `--no-worktree` tests. This case is the only one that
    /// fails it. Semantically: an artifact visible from the main checkout while
    /// this phase is in flight inside a worktree belongs to a *different* run,
    /// and treating it as this phase's evidence is CR-01 with the sign
    /// reversed.
    ///
    /// Do not merge this back into a shared `#[test]` with scenario A, and do
    /// not weaken its assertion: deleting it costs the suite nothing visible
    /// today and everything the day someone "hardens" the probe by OR-ing the
    /// two roots.
    #[test]
    fn worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator() {
        let _guard = env_lock();

        let dir_b = tempfile::tempdir().unwrap();
        let root_b = dir_b.path();
        let phase_b = PhaseId::new(95);
        let mut state_b = State::new(phase_b, AgentKind::Claude, Mode::Auto, root_b.to_path_buf());
        // IN-07: Validate is the only stage production reaches this call from.
        state_b.stage = Stage::Validate;
        let worktree_b = root_b.join(format!(".worktrees/phase-{phase_b}"));
        std::fs::create_dir_all(&worktree_b).unwrap();
        state_b.worktree_path = Some(worktree_b.clone());
        workflow::save_state(&state_b).unwrap();

        // Built from the tempdir ROOT, never from the worktree path: writing
        // this under the worktree by mistake would silently turn scenario B
        // into a duplicate of the positive test with an inverted assertion.
        let stale_dir = root_b
            .join(".planning/phases")
            .join(format!("{phase_b:02}-test"));
        std::fs::create_dir_all(&stale_dir).unwrap();
        std::fs::write(
            stale_dir.join(format!("{phase_b:02}-VERIFICATION.md")),
            "stale artifact belonging to a different run\n",
        )
        .unwrap();

        // WR-05: RAII PATH neutralization — same reason as scenario A, this
        // drive also reaches `loop_back_to_code` -> `launch_stage`.
        {
            let _path_guard = NeutralPath::install();
            let _ = handle_validate_outcome(root_b, &mut state_b, ValidateOutcome::Failed);
        }

        let last_b =
            devflow_core::events::last_event_of_kind_for_phase(root_b, phase_b, "loop_back")
                .expect("scenario B loop_back event must be recorded");
        assert_eq!(
            last_b["fix"], "FullExecute",
            "a {{N}}-VERIFICATION.md visible only from the main checkout belongs to a different run and must NOT resurrect GapsOnly"
        );
    }

    /// 999.79 / HARDEN-03, ROADMAP criterion 3 — the STALE direction. A
    /// `devflow start --phase N --force` checks out a branch that still
    /// carries the PREVIOUS run's committed `{N}-VERIFICATION.md`. That re-run
    /// is mid-arc by construction, so inheriting the artifact's verdict
    /// dispatches a `--gaps-only` pass against zero matching plans and gates
    /// unresolvably.
    ///
    /// The fixture is the forced-re-run shape exactly: the artifact is on disk
    /// AND `state.last_verification_fingerprint` already holds its fingerprint,
    /// because `start()` recorded it before this run's Validate agent had any
    /// opportunity to rewrite it. Unchanged since run start ⇒ inherited ⇒
    /// FullExecute.
    ///
    /// **Not meaningful without `verification_written_this_run_dispatches_gaps_only`
    /// below.** This test passes against a rule that reports every artifact
    /// stale forever — which would silently disable the gaps-only path Phase 33
    /// built, trading an unresolvable gate for a loop that re-runs every plan.
    /// Only the paired test detects that. One direction alone is not acceptance.
    #[test]
    fn stale_verification_artifact_dispatches_full_execute() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(86);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        // IN-07: Validate is the only stage production reaches this call from.
        state.stage = Stage::Validate;
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        std::fs::create_dir_all(&worktree).unwrap();
        state.worktree_path = Some(worktree.clone());

        // The inherited artifact, committed by the PREVIOUS run and still on
        // the branch this forced re-run checked out.
        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-test", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded())),
            "verdict: pass -- authored by a PREVIOUS run\n",
        )
        .unwrap();

        // What `start()` records once the evidence root is resolved (A-05).
        let baseline = devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase);
        assert!(
            baseline.is_some(),
            "premise: the inherited artifact must be visible from the evidence root, otherwise \
             this test would assert FullExecute for the mid-arc reason instead of the stale one"
        );
        state.last_verification_fingerprint = baseline;
        workflow::save_state(&state).unwrap();

        {
            let _path_guard = NeutralPath::install();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        }

        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert_eq!(
            last["fix"], "FullExecute",
            "an artifact unchanged since this run started was inherited from a previous run; \
             its verdict must NOT be reused, so the loop-back must dispatch FullExecute"
        );
    }

    /// 999.79 / HARDEN-03, ROADMAP criterion 3 — the FRESH direction, and the
    /// reason this fix is distinguishable from one that disables `--gaps-only`
    /// forever (A-12, NC-7, T-35-22).
    ///
    /// Two sub-cases, both of which mean "the Validate agent authored this
    /// during THIS run" and must reach the gaps-only path:
    ///
    /// 1. the artifact's fingerprint DIFFERS from the run-start baseline — a
    ///    stale copy was on disk at run start and Validate rewrote it;
    /// 2. the artifact EXISTS where the baseline recorded none — the ordinary
    ///    first-verification case, and the one every pre-999.79 test exercised
    ///    implicitly by leaving the baseline at `State::new`'s `None`.
    ///
    /// Sub-case 1 additionally closes **F-11's testable half**: it round-trips
    /// state through the persisted file rather than reading the in-memory
    /// `State`, then asserts the baseline the selector recorded is present on
    /// disk after the loop-back completed. The risk being covered is a selector
    /// that updates a value nothing ever writes out — `handle_validate_outcome`
    /// saves state BEFORE the selector runs, so the update reaches disk only
    /// via `prepare_loop_back_to_code`'s own save. Reading the in-memory
    /// `State` would not distinguish those.
    ///
    /// Sub-case 2 deliberately does NOT round-trip, and therefore establishes
    /// nothing about multi-process behaviour on its own.
    #[test]
    fn verification_written_this_run_dispatches_gaps_only() {
        let _guard = env_lock();

        // ---- sub-case 1: content differs from the run-start baseline ----
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(87);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        std::fs::create_dir_all(&worktree).unwrap();
        state.worktree_path = Some(worktree.clone());

        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-test", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        let artifact = phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded()));

        // The previous run's copy, present when this run started.
        std::fs::write(&artifact, "verdict: pass -- from a PREVIOUS run\n").unwrap();
        let run_start =
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase);
        state.last_verification_fingerprint = run_start;
        state.verification_run_nonce = Some(1);
        workflow::save_state(&state).unwrap();

        // This run's Validate agent rewrites it.
        std::fs::write(&artifact, "verdict: gaps -- authored by THIS run\n").unwrap();
        let rewritten =
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase);
        assert_ne!(
            run_start, rewritten,
            "premise: the rewrite must actually change the fingerprint, otherwise this test \
             is silently exercising the stale case with a fresh label"
        );

        // F-11: reload from disk, so the drive below operates on state that
        // genuinely crossed a process boundary rather than on the in-memory
        // value this test just mutated.
        let mut reloaded = workflow::load_state(root, phase).expect("state must persist");
        reloaded.verification_run_nonce = Some(1);
        assert_eq!(
            reloaded.last_verification_fingerprint, run_start,
            "premise: the run-start baseline must survive the save/load round trip, or the \
             comparison below is against an in-memory value the real pipeline never sees"
        );

        {
            let _path_guard = NeutralPath::install();
            let _ = handle_validate_outcome(root, &mut reloaded, ValidateOutcome::Failed);
        }

        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert_eq!(
            last["fix"], "GapsOnly",
            "an artifact whose content changed since this run started was authored by THIS \
             run's Validate agent and must still reach the gaps-only path — a rule that marks \
             everything stale would pass the stale test and fail here, which is the whole point \
             of the pair"
        );

        // F-11's testable half: the selector's baseline update must be OBSERVABLE
        // IN THE PERSISTED FILE, not merely in the in-memory State. The save in
        // `handle_validate_outcome` runs BEFORE the selector, so this can only
        // have been written by `prepare_loop_back_to_code`'s own save.
        let persisted = workflow::load_state(root, phase)
            .expect("state must still exist after the loop-back completed");
        assert_eq!(
            persisted.last_verification_fingerprint, rewritten,
            "the baseline the selector recorded must reach DISK — a selector that updates a \
             value nothing ever writes out would leave a later same-run check comparing against \
             the old baseline and reading an unchanged artifact as fresh (F-11's fail-open \
             direction)"
        );

        // ---- sub-case 2: artifact exists where the baseline recorded none ----
        let dir_b = tempfile::tempdir().unwrap();
        let root_b = dir_b.path();
        let phase_b = PhaseId::new(88);
        let mut state_b = State::new(phase_b, AgentKind::Claude, Mode::Auto, root_b.to_path_buf());
        state_b.stage = Stage::Validate;
        let worktree_b = root_b.join(format!(".worktrees/phase-{phase_b}"));
        std::fs::create_dir_all(&worktree_b).unwrap();
        state_b.worktree_path = Some(worktree_b.clone());
        // No artifact existed when this run started — and the run DID look.
        // WR-05 (35-REVIEW): both halves are premises. `None` alone is
        // ambiguous, and the unlooked variant is a different scenario with the
        // opposite required dispatch (row 2b of the truth table).
        assert_eq!(
            state_b.last_verification_fingerprint, None,
            "premise: sub-case 2 requires an absent run-start baseline"
        );
        state_b.verification_baseline_captured = true;
        workflow::save_state(&state_b).unwrap();

        let phase_dir_b = worktree_b
            .join(".planning/phases")
            .join(format!("{phase_b:02}-test"));
        std::fs::create_dir_all(&phase_dir_b).unwrap();
        std::fs::write(
            phase_dir_b.join(format!("{phase_b:02}-VERIFICATION.md")),
            "verdict: gaps -- first verification of this phase\n",
        )
        .unwrap();

        {
            let _path_guard = NeutralPath::install();
            let _ = handle_validate_outcome(root_b, &mut state_b, ValidateOutcome::Failed);
        }

        let last_b =
            devflow_core::events::last_event_of_kind_for_phase(root_b, phase_b, "loop_back")
                .expect("sub-case 2 loop_back event must be recorded");
        assert_eq!(
            last_b["fix"], "GapsOnly",
            "an artifact that exists where the run-start baseline recorded none was authored \
             this run and must dispatch GapsOnly — this is the ordinary first-verification case \
             Phase 33 built, and a too-strict freshness rule breaks it"
        );
    }

    /// H-1 / NC-7's AUTOMATED half (T-35-22b): the stale/fresh decision is a
    /// pure predicate over the pair (current fingerprint, run-start baseline),
    /// and all four of its rows are pinned here in one place.
    ///
    /// Why this exists as well as the two direction tests above: NC-7's
    /// performed mutation is a one-time act that nothing re-runs, so an
    /// always-stale regression introduced later would ship with nothing to
    /// catch it — and an always-stale rule makes the loop re-run every plan
    /// forever, which is a worse failure than the stall 999.79 fixes.
    ///
    /// The row set is chosen so BOTH over-corrections are caught, not one:
    ///
    /// | current       | baseline      | captured | fresh? | fails under |
    /// |---------------|---------------|----------|--------|-------------|
    /// | `None`        | `None`        | true     | no     | always-fresh|
    /// | `(h, t)`      | `None`        | true     | yes    | always-stale|
    /// | `(h, t)`      | `None`        | **false**| **no** | always-fresh|
    /// | `(h, t)`      | `(h, t)`      | true     | no     | always-fresh|
    /// | `(h, **t2**)` | `(h, t)`      | true     | **yes**| always-stale|
    /// | `(h2, t2)`    | `(h, t)`      | true     | yes    | always-stale|
    ///
    /// Three rows fail an always-stale stub and the others fail an always-fresh
    /// stub. A table that only fails one of the two would not be exhaustive and
    /// the row set would be wrong.
    ///
    /// Row 2b is WR-05's (35-REVIEW): it and row 2 differ in exactly one input,
    /// and a predicate ignoring `baseline_captured` collapses them — which is
    /// how an upgraded in-flight phase reproduced the 999.79 stall.
    ///
    /// Row 3b is WR-06's: it and row 3 differ in exactly one input, and a
    /// predicate ignoring the mtime collapses them — which is how an idempotent
    /// rewrite read as inherited and turned every later cycle into a full
    /// execute.
    #[test]
    fn verification_freshness_truth_table_is_exhaustive() {
        // Row 1 — no artifact at all. Not "authored this run": there is
        // nothing to have authored. The phase is mid-arc and D-01's original
        // FullExecute answer stands, unchanged by 999.79.
        assert!(
            !verification_authored_this_run((None, None), (None, None), true, Some(1)),
            "row 1: an absent artifact is never 'authored this run'"
        );

        // Row 1b — absent now, present at run start. Still nothing to inherit
        // a verdict from; grouped with row 1 rather than given its own row
        // because the predicate cannot distinguish them and must not try.
        assert!(
            !verification_authored_this_run((None, None), (Some(7), Some(100)), true, Some(1)),
            "row 1b: an artifact that is absent NOW is never 'authored this run', whatever the \
             baseline recorded"
        );

        // Row 2 — exists now, nothing recorded at run start, and the run DID
        // look. The ordinary first-verification case: Validate authored it
        // during this run.
        assert!(
            verification_authored_this_run((Some(7), Some(100)), (None, None), true, Some(1)),
            "row 2: an artifact existing where the baseline recorded none was authored this run"
        );

        // Row 2b (WR-05) — identical to row 2 except that nobody ever looked:
        // state written by a binary predating the baseline field. The artifact
        // may be the PREVIOUS run's, so its verdict must not be reused.
        assert!(
            !verification_authored_this_run((Some(7), Some(100)), (None, None), false, Some(1)),
            "row 2b: with no captured baseline the artifact's provenance is unknown, and \
             reading it as this run's is the 999.79 stall reproduced across an upgrade"
        );

        // Row 3 — unchanged since run start, in BOTH content and mtime. The
        // 999.79 case: inherited from a previous run, its verdict must not be
        // reused.
        assert!(
            !verification_authored_this_run(
                (Some(7), Some(100)),
                (Some(7), Some(100)),
                true,
                Some(1)
            ),
            "row 3: an artifact whose fingerprint AND mtime equal the run-start baseline is \
             INHERITED, not authored this run"
        );

        // Row 3b (WR-06) — identical to row 3 except that the file was written
        // since: the Validate agent re-authored byte-identical content on a
        // later failing cycle. Hash-only this read as inherited, and every
        // subsequent cycle re-ran every plan in the phase.
        assert!(
            verification_authored_this_run(
                (Some(7), Some(200)),
                (Some(7), Some(100)),
                true,
                Some(1)
            ),
            "row 3b: an IDEMPOTENT rewrite is still a rewrite — unchanged bytes with an \
             advanced mtime were written by this run's agent"
        );

        // Row 3c — the degrade. With either mtime unavailable the comparison
        // falls back to content alone, which is the pre-WR-06 behaviour; a
        // `Some != None` mtime test would instead read EVERY cycle as rewritten.
        assert!(
            !verification_authored_this_run((Some(7), None), (Some(7), Some(100)), true, Some(1)),
            "row 3c: an unavailable mtime must degrade to the content comparison, not \
             manufacture a difference"
        );

        // Row 4 — content changed since run start. Validate rewrote it.
        assert!(
            verification_authored_this_run(
                (Some(7), Some(200)),
                (Some(8), Some(100)),
                true,
                Some(1)
            ),
            "row 4: an artifact whose fingerprint differs from the run-start baseline was \
             rewritten during this run"
        );
    }

    /// WR-05 (35-REVIEW) end to end: a phase started under a binary predating
    /// `last_verification_fingerprint` and continued by this one must NOT read
    /// the inherited `{N}-VERIFICATION.md` as this run's work.
    ///
    /// The old state file deserializes the baseline to `None` while the
    /// previous run's committed artifact is already on disk, so the
    /// `(Some, None)` row fired and dispatched `--gaps-only` against zero
    /// matching plans — the DOGFOOD-01-class unresolvable stall 999.79 exists
    /// to close, reproduced for every in-flight phase across an upgrade.
    ///
    /// **Both halves under one artifact**, differing in exactly one field. The
    /// upgraded state must dispatch `FullExecute`; a state that genuinely
    /// captured the baseline and found nothing must still dispatch `GapsOnly`.
    /// Asserting only the first would be satisfied by an always-stale rule,
    /// which the selector's own comment names as the worse over-correction —
    /// it silently reverts what Phase 33 built.
    #[test]
    fn an_uncaptured_baseline_does_not_claim_an_inherited_artifact() {
        let _guard = env_lock();

        /// Drive one Validate failure against a phase whose artifact already
        /// exists, and report the dispatched fix.
        fn dispatch_with(root: &Path, phase: PhaseId, baseline_captured: bool) -> String {
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.verification_run_nonce = Some(1);
            state.stage = Stage::Validate;
            state.verification_baseline_captured = baseline_captured;
            workflow::save_state(&state).unwrap();

            let phase_dir = root
                .join(".planning/phases")
                .join(format!("{padded}-test", padded = phase.padded()));
            std::fs::create_dir_all(&phase_dir).unwrap();
            std::fs::write(
                phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded())),
                "verdict: pass -- committed by a PREVIOUS run\n",
            )
            .unwrap();

            {
                let _path_guard = NeutralPath::install();
                let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            }

            devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
                .expect("loop_back event must be recorded")["fix"]
                .as_str()
                .expect("the fix must be recorded as a string")
                .to_string()
        }

        // The upgrade case: nobody ever looked, and the artifact predates this
        // run.
        let upgraded_dir = tempfile::tempdir().unwrap();
        let upgraded_root = upgraded_dir.path();
        assert_eq!(
            dispatch_with(upgraded_root, PhaseId::new(78), false),
            "FullExecute",
            "with no captured baseline the artifact's provenance is unknown; --gaps-only \
             would match zero plans and gate unresolvably"
        );
        assert!(
            devflow_core::events::last_event_of_kind_for_phase(
                upgraded_root,
                PhaseId::new(78),
                "verification_baseline_absent",
            )
            .is_some(),
            "the operator must be able to see WHY a gaps-only pass became a full execute"
        );

        // NEGATIVE CONTROL: identical in every respect except that this run
        // performed the observation.
        let captured_dir = tempfile::tempdir().unwrap();
        let captured_root = captured_dir.path();
        assert_eq!(
            dispatch_with(captured_root, PhaseId::new(79), true),
            "GapsOnly",
            "the ordinary first-verification case must be untouched — an always-stale rule \
             re-runs every plan in the phase forever, which the selector's own comment calls \
             the worse over-correction"
        );
        assert!(
            devflow_core::events::last_event_of_kind_for_phase(
                captured_root,
                PhaseId::new(79),
                "verification_baseline_absent",
            )
            .is_none(),
            "a signal emitted on the ordinary case too would carry no information"
        );
    }

    /// WR-06 (35-REVIEW) end to end: a Validate agent that re-authors
    /// BYTE-IDENTICAL content on a later failing cycle must still reach the
    /// gaps-only path.
    ///
    /// Cycle 1 writes a verification listing gap G and the selector records its
    /// fingerprint as the baseline. Cycle 2's gaps-only fix does not close G,
    /// Validate re-runs and writes the identical document — and with a
    /// content-only rule `current == baseline` read as INHERITED, dispatching a
    /// full execute. Every subsequent cycle then re-ran every plan in the phase
    /// instead of the gaps-only pass Phase 33 built. That is the "too strict"
    /// over-correction the selector's own comment claims to guard against.
    ///
    /// **The mtime is forced rather than observed.** `File::set_modified` makes
    /// the advance exact, so the test measures the freshness rule instead of
    /// the host filesystem's timestamp granularity — a coarse-granularity FS
    /// would otherwise make this pass or fail for a reason that has nothing to
    /// do with the subject.
    ///
    /// The control is the untouched artifact: same content, same baseline, and
    /// the ONLY difference is whether anything wrote the file. Without it a rule
    /// that simply always answered "authored" would pass the first half — and
    /// that rule is the 999.79 stall.
    #[test]
    fn an_idempotent_rewrite_is_authored_not_inherited() {
        let _guard = env_lock();

        /// Seed a phase whose artifact already exists and whose baseline
        /// records it, optionally rewriting it with identical bytes afterwards.
        fn dispatch_with(root: &Path, phase: PhaseId, rewrite_identically: bool) -> String {
            let phase_dir = root
                .join(".planning/phases")
                .join(format!("{padded}-test", padded = phase.padded()));
            std::fs::create_dir_all(&phase_dir).unwrap();
            let artifact =
                phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded()));
            let contents = "verdict: gaps -- G is still open\n";
            std::fs::write(&artifact, contents).unwrap();

            // What the selector recorded on the previous cycle of THIS run.
            let baseline_hash =
                devflow_core::agent_result::phase_verification_fingerprint(root, phase);
            let baseline_mtime =
                devflow_core::agent_result::phase_verification_mtime_nanos(root, phase)
                    .expect("the fixture needs a readable mtime");

            if rewrite_identically {
                std::fs::write(&artifact, contents).unwrap();
                let advanced = std::time::UNIX_EPOCH
                    + std::time::Duration::from_nanos(baseline_mtime)
                    + std::time::Duration::from_secs(1);
                std::fs::File::options()
                    .write(true)
                    .open(&artifact)
                    .unwrap()
                    .set_modified(advanced)
                    .unwrap();
                assert_eq!(
                    devflow_core::agent_result::phase_verification_fingerprint(root, phase),
                    baseline_hash,
                    "premise: the rewrite must be byte-IDENTICAL, or this exercises row 4"
                );
                assert_ne!(
                    devflow_core::agent_result::phase_verification_mtime_nanos(root, phase),
                    Some(baseline_mtime),
                    "premise: the rewrite must advance the mtime, or there is nothing to see"
                );
            }

            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            state.verification_baseline_captured = true;
            state.last_verification_fingerprint = baseline_hash;
            state.last_verification_mtime_nanos = Some(baseline_mtime);
            state.verification_run_nonce = Some(1);
            workflow::save_state(&state).unwrap();

            {
                let _path_guard = NeutralPath::install();
                let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            }

            devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
                .expect("loop_back event must be recorded")["fix"]
                .as_str()
                .expect("the fix must be recorded as a string")
                .to_string()
        }

        let rewritten_dir = tempfile::tempdir().unwrap();
        assert_eq!(
            dispatch_with(rewritten_dir.path(), PhaseId::new(76), true),
            "GapsOnly",
            "an idempotent rewrite is still a rewrite; reading it as inherited re-runs every \
             plan in the phase on every later cycle"
        );

        // NEGATIVE CONTROL: nothing wrote the file after the baseline was taken.
        let untouched_dir = tempfile::tempdir().unwrap();
        assert_eq!(
            dispatch_with(untouched_dir.path(), PhaseId::new(77), false),
            "FullExecute",
            "an artifact nobody touched since the baseline is INHERITED — a rule that always \
             answered 'authored' would be the 999.79 stall"
        );
    }

    /// D-01/D-02: the `Ambiguous` gate's loop-back arm must also consult
    /// `select_loop_back_fix`, not only the plain-Failed tail arm Task 1
    /// wired. Seeds a rejecting `GateResponse` (note without "abort", so
    /// `GateAction::from_response` resolves `LoopBack` rather than `Abort`)
    /// so the gate resolves from an already-written file instead of
    /// blocking on the multi-day default timeout. PATH is neutralized under
    /// `ENV_MUTEX` (Task 1's shape) so the resulting `LoopBack` cannot spawn
    /// a real agent CLI — the `loop_back` event is emitted before that
    /// launch, so its `Err` is discarded.
    #[test]
    fn ambiguous_gate_loop_back_respects_the_mid_arc_check() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(84);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        // No {phase:02}-VERIFICATION.md — mid-arc precondition.

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"loop back for another pass","responded_by":"test"}"#,
        )
        .unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(
            root,
            &mut state,
            ValidateOutcome::Ambiguous("test disagreement".to_string()),
        );

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert_eq!(
            last["fix"], "FullExecute",
            "the Ambiguous gate's loop-back must respect the mid-arc check, same as the plain tail arm"
        );
    }

    /// D-01/D-02: the consecutive-failure-gated loop-back arm must also
    /// consult `select_loop_back_fix`. Sets `consecutive_failures` to the
    /// ceiling beforehand so the `should_gate` branch is the one taken
    /// (rather than the plain-Failed tail arm Task 1 already covers), and
    /// proves the gated path and the ungated tail agree on the fix — which
    /// they did not have to. PATH-neutralized under `ENV_MUTEX`, same as
    /// above.
    ///
    /// 33-04: both the seeded 999.66 baseline and the counter assertion on the
    /// `loop_back` event are required, and they do different jobs — the seed
    /// puts this test back on the gated arm (without it the counter resets to
    /// 1 and `should_gate` is false), and the assertion is what notices if a
    /// later change moves it off again. The pre-existing `fix` assertion
    /// cannot: both arms route through `select_loop_back_fix` and both emit
    /// `FullExecute`, so this test passed vacuously on the wrong arm.
    #[test]
    fn failure_gate_loop_back_respects_the_mid_arc_check() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(85);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES;
        // 999.66 (33-03): seed the forward-progress baseline alongside the
        // streak, or the `None` baseline resets the counter to 1 and this
        // test silently exercises the ungated tail arm instead of the
        // consecutive-failure-gated arm its name and doc comment claim.
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();

        // No {phase:02}-VERIFICATION.md — mid-arc precondition.

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"loop back for another pass","responded_by":"test"}"#,
        )
        .unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert!(
            last["consecutive_failures"]
                .as_u64()
                .expect("loop_back event must carry consecutive_failures")
                >= u64::from(mode::MAX_CONSECUTIVE_FAILURES),
            "must be the consecutive-failure-GATED loop-back arm (counter {}) — a value below the threshold means this ran the ungated tail arm instead",
            last["consecutive_failures"]
        );
        assert_eq!(
            last["fix"], "FullExecute",
            "the consecutive-failure-gated loop-back must respect the mid-arc check, same as the ungated tail arm"
        );
    }

    /// D-02, the negative control for the whole D-01 change: identical
    /// precondition (no `{N}-VERIFICATION.md`) as the two tests above, but
    /// driven through `handle_ship_outcome` instead — must still dispatch
    /// `GapsOnly`, proving the out-of-scope call site was not swept in as a
    /// runtime fact, not merely a claim about the diff. PATH-neutralized
    /// under `ENV_MUTEX`, same as above.
    #[test]
    fn ship_loop_back_still_issues_gaps_only_when_verification_absent() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(86);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        assert!(
            !state.yes_ship,
            "no pre-authorization must short-circuit the gate"
        );
        workflow::save_state(&state).unwrap();

        // No {phase:02}-VERIFICATION.md — same mid-arc precondition that
        // flips the two Validate arms above.

        let response_path = Gates::response_path(root, phase, Stage::Ship);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"loop back for another pass","responded_by":"test"}"#,
        )
        .unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        let _ = handle_ship_outcome(root, &mut state);

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        let last = devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .expect("loop_back event must be recorded");
        assert_eq!(
            last["fix"], "GapsOnly",
            "handle_ship_outcome must remain unaffected by the D-01 mid-arc check (D-02)"
        );
    }

    /// Combined 18d+18e scenario (18-RESEARCH.md Pitfall 1) — the only test
    /// that proves both fixes hold TOGETHER, not each in isolation: 18e's
    /// Layer-0 discard is what makes an `external_verify` Validate fail for
    /// the wrong reason, and 18d's counter reset is what made that failure
    /// loop unbounded — fixing either alone leaves the other's failure mode
    /// partially masked. Arm A (18e dominates) proves an `Ambiguous` outcome
    /// gates on the FIRST cycle, never touching `consecutive_failures`. Arm
    /// B (18d dominates) proves a genuine, non-ambiguous failure still
    /// reaches `MAX_CONSECUTIVE_FAILURES` and forces the gate — the case
    /// that, before 18d, ran forever.
    #[test]
    fn external_verify_cycles_reach_ceiling_without_unbounded_loop() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Arm A: an Ambiguous outcome gates on cycle one, never touching
        // consecutive_failures. Arm B: a genuine failure still reaches
        // MAX_CONSECUTIVE_FAILURES and forces the gate.
        arm_a_ambiguous_outcome_gates_on_cycle_one(root, PhaseId::new(93));
        arm_b_genuine_failures_reach_the_ceiling(root, PhaseId::new(94));
    }

    /// Arm A (18e dominates): an ambiguous `external_verify` outcome gates
    /// immediately — no Code↔Validate loop ever starts, so 18d's counter is
    /// irrelevant here and must stay untouched. Asserting that prevents a
    /// future refactor from quietly routing ambiguity back through the
    /// counter-based auto-loop.
    fn arm_a_ambiguous_outcome_gates_on_cycle_one(root: &Path, phase: PhaseId) {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        let result = agent_result::AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: None,
            commits: None,
            summary: None,
            verdict: Some(Verdict::Gaps),
            decided_by_layer: Some(0),
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, outcome).unwrap();

        assert_eq!(
            state.consecutive_failures, 0,
            "18e's ambiguous gate must fire on cycle one, never touching 18d's counter"
        );
    }

    /// Arm B (18d dominates): a genuine, non-ambiguous `ValidateOutcome::Failed`
    /// driven through repeated Code↔Validate cycles reaches
    /// `MAX_CONSECUTIVE_FAILURES` and forces the gate. PATH is neutralized
    /// under `ENV_MUTEX` (matching `consecutive_failures_reaches_ceiling_across_cycles`)
    /// so neither `handle_validate_outcome`'s loop-back nor `transition`'s
    /// own `launch_stage` risk spawning a real agent CLI.
    fn arm_b_genuine_failures_reach_the_ceiling(root: &Path, phase: PhaseId) {
        let _guard = env_lock();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

        let neutral_path_dir = agent_free_git_only_path_dir();
        let original_path = std::env::var_os("PATH");
        // SAFETY: serialized under ENV_MUTEX.
        unsafe {
            std::env::set_var("PATH", neutral_path_dir.path());
        }

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
        }

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            match &original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }

        assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
        assert!(
            state.mode.should_gate(
                Stage::Validate,
                state.consecutive_failures,
                state.phase_validate_failures
            ),
            "a genuine repeated failure must still reach the reachable ceiling (18d)"
        );
    }

    /// 18d precision edge: `consecutive_failures` must saturate at `u32::MAX`
    /// rather than wrap to 0 on overflow, so a long-running stuck loop can't
    /// silently restore the unreachable-ceiling bug in a slower, harder-to-
    /// diagnose form. At `u32::MAX`, `should_gate` is already true, so the
    /// failure resolves via the forced-gate path — pre-seed a response so
    /// `run_gate`'s poll doesn't wait out the timeout.
    #[test]
    fn consecutive_failures_increment_saturates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(82);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = u32::MAX;
        // See validate_failure_threshold_forces_gate_then_aborts: pre-seed
        // the 999.66 baseline to match the (no-repo) commit count of 0 so a
        // directly-seeded streak isn't misread as a first-ever failure.
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();

        let response_path = Gates::response_path(root, phase, Stage::Validate);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert_eq!(state.consecutive_failures, u32::MAX);
    }

    /// D-09: a primary-loop `RateLimited` outcome writes the single-agent
    /// cron-instructions record (`devflow resume --phase N`) and returns
    /// without firing a blocking gate.
    #[test]
    fn primary_loop_rate_limited_writes_single_agent_cron_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(76);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let instructions = devflow_core::ship::load_cron_instructions(root, phase).unwrap();
        assert_eq!(instructions.resume.command, "devflow");
        assert_eq!(
            instructions.resume.args,
            ["resume", "--phase", &phase.to_string()]
        );
        assert!(
            instructions
                .hermes_cron
                .command
                .contains(&format!("devflow resume --phase {phase}"))
        );

        // No blocking gate — state persists, stage unchanged, not gate-pending.
        let reloaded = workflow::load_state(root, phase).unwrap();
        assert_eq!(reloaded.stage, Stage::Code);
        assert!(!reloaded.gate_pending);
        assert_eq!(reloaded.infra_failures, 1);
        assert_eq!(reloaded.consecutive_failures, 0);
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
    }

    /// D-08/D-09: the RateLimited path at `infra_failures ==
    /// MAX_INFRA_FAILURES - 1` bumps to the ceiling and stops auto-resuming —
    /// it routes to the infra gate/abort path instead of writing a resume
    /// record (bounded resume, no soft-loop).
    #[test]
    fn rate_limited_at_infra_ceiling_stops_resuming_and_aborts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(77);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.infra_failures = mode::MAX_INFRA_FAILURES - 1;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        advance(root, Some(phase)).unwrap();

        let err = workflow::load_state(root, phase).unwrap_err();
        assert!(
            matches!(err, workflow::WorkflowError::MissingState(_)),
            "the infra ceiling must abort, clearing state"
        );
        assert!(
            devflow_core::ship::load_cron_instructions(root, phase).is_err(),
            "must not schedule an auto-resume once the infra ceiling stops resumption"
        );
    }

    /// CR-03: a rate-limit reason whose retry hint is unparseable (e.g. the
    /// `"usage limit"` fallback `detect_claude_rate_limit` produces for a 429
    /// with no retry_after) yields an EMPTY cron schedule — auto-resume is
    /// impossible. That must not return `Ok(())` silently (the detached
    /// monitor would exit with the phase stalled and zero operator signal);
    /// it must fire the same never-silent gate + notify the infra path uses
    /// (WR-11/D-15), and must never invent a schedule.
    #[test]
    fn rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(81);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // Pre-seed an Abort response so `run_gate`'s poll resolves immediately.
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_rate_limited_outcome(
            root,
            &mut state,
            phase,
            Stage::Code,
            Some("rate limited until usage limit".into()),
        )
        .unwrap();

        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(
            events.contains("gate_fired"),
            "an unparseable retry hint must raise a gate, not stall the phase silently: {events}"
        );
        assert!(
            events.contains("notify_fired"),
            "the operator must be notified that a manual resume is needed: {events}"
        );
        assert!(
            !events.contains("rate_limit_resume_scheduled"),
            "nothing was scheduled — emitting a resume-scheduled event would be a false signal: {events}"
        );

        // The unparseable hint must never become a schedule (an empty cron
        // expression would otherwise degrade into an every-minute resume).
        let instructions = devflow_core::ship::load_cron_instructions(root, phase).unwrap();
        assert!(instructions.hermes_cron.schedule.is_empty());
    }

    /// The Validate→Ship content hook (`DocsUpdate`) authors material about
    /// the branch being shipped, so it must run in that phase's worktree;
    /// the terminal batch merges/tags/deletes against the primary checkout
    /// and must NOT be retargeted. `ChangelogAppend` moved into the terminal
    /// batch in 17-12 (WR-04) for exactly this reason — it now targets
    /// `project_root`, not the worktree.
    ///
    /// Found live: `ChangelogAppend` wrote Phase 17's release note into
    /// `develop`'s CHANGELOG.md while all of its commits sat on
    /// `feature/phase-17`, stranding the entry on the wrong branch.
    #[test]
    fn content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let worktree = root.join(".worktrees/phase-70");
        std::fs::create_dir_all(&worktree).unwrap();

        let mut state = State::new(
            PhaseId::new(70),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        state.worktree_path = Some(worktree.clone());

        assert_eq!(
            hook_context_root(root, &state, false),
            worktree,
            "content hooks must write into the phase's worktree"
        );
        assert_eq!(
            hook_context_root(root, &state, true),
            root.to_path_buf(),
            "terminal hooks merge/tag/delete against the primary checkout"
        );

        // --no-worktree runs, and a worktree recorded but already removed,
        // both fall back to the project root rather than writing nowhere.
        let mut no_worktree = state.clone();
        no_worktree.worktree_path = None;
        assert_eq!(hook_context_root(root, &no_worktree, false), root);

        let mut missing = state.clone();
        missing.worktree_path = Some(root.join(".worktrees/gone"));
        assert_eq!(hook_context_root(root, &missing, false), root);
    }

    /// 13-06 dogfood regression: a multi-KB parser-derived reason reached
    /// the operator's desktop notification verbatim. Gate contexts must cap
    /// the reason to a readable headline.
    #[test]
    fn truncate_reason_caps_long_reasons_and_keeps_short_ones() {
        assert_eq!(truncate_reason("short reason"), "short reason");
        let long = "x".repeat(5000);
        let capped = truncate_reason(&long);
        assert!(capped.chars().count() <= 300);
        assert!(capped.ends_with("[truncated; full output in .devflow/]"));
    }

    #[test]
    fn gate_context_rendering_neutralizes_all_controls_and_obeys_limit() {
        let rendered = render_gate_context("line 1\n\u{1b}[2J\tline 2\u{7}", 100);
        assert!(!rendered.chars().any(char::is_control));
        assert_eq!(rendered, "line 1  [2J line 2 ");

        let bounded = render_gate_context(&"x".repeat(500), 100);
        assert_eq!(bounded.chars().count(), 100);
        assert!(bounded.ends_with("[truncated; full output in .devflow/]"));
    }

    /// A Ship-stage AgentFailed result (no `review:` prefix) must write a
    /// gate file and block for a response — not silently return an `Err`
    /// with nothing surfaced (WR-11; the pre-Task-2 catch-all never wrote a
    /// gate at all for this case). Runs `handle_ship_failure` on a scoped
    /// thread and busy-polls for the gate file to appear while the call is
    /// still blocked in `run_gate`'s poll, then unblocks it with an Abort
    /// response so the thread can finish without spawning a real monitor
    /// (Abort resolves via `abort()`, which never calls `launch_stage`).
    #[test]
    fn ship_agent_failed_fires_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = PhaseId::new(40);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let gate_path = Gates::gate_path(root, phase, Stage::Ship);
        let response_path = Gates::response_path(root, phase, Stage::Ship);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                handle_ship_failure(root, &mut state, Some("agent crashed".into())).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if gate_path.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(
                seen,
                "handle_ship_failure must write a gate file, not silently return an Err"
            );

            // Unblock the poll with an Abort response so the spawned thread
            // finishes (abort() cleans up on its own; no monitor spawned).
            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });
    }

    /// A Ship-stage result whose reason starts with `review:` must loop back
    /// to Code instead of firing a gate — it does not go through `run_gate`
    /// at all, so no gate file is ever written for this path.
    ///
    /// Exercises `is_ship_review_failure` (the exact dispatch predicate
    /// `handle_ship_failure` uses) plus `prepare_loop_back_to_code` (the
    /// state-mutating half of `loop_back_to_code`) directly, rather than the
    /// full `handle_ship_failure` → `loop_back_to_code` → `launch_stage`
    /// chain: `launch_stage` spawns the real configured agent CLI (e.g. real
    /// `claude -p ... --dangerously-skip-permissions` if it's on `$PATH`),
    /// which must never fire from a unit test.
    #[test]
    fn ship_review_failed_loops_to_code() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = PhaseId::new(41);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        workflow::save_state(&state).unwrap();

        let reason = Some("review: please fix naming".to_string());
        assert!(is_ship_review_failure(&reason));

        prepare_loop_back_to_code(
            root,
            &mut state,
            FixType::AuditFix,
            LoopBackReason::GateResponse,
        )
        .unwrap();

        assert_eq!(state.stage, Stage::Code);
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        // Not finished — finish_workflow would have cleared state entirely.
        assert!(workflow::load_state(root, phase).is_ok());
    }

    /// The ReviewFailed loop-back must select `FixType::AuditFix`
    /// (`/gsd-audit-fix`), not the Validate path's `FixType::GapsOnly`
    /// (consensus #7 / OpenCode HIGH #2).
    #[test]
    fn ship_review_failed_uses_audit_fix() {
        assert!(is_ship_review_failure(&Some(
            "review: needs changes".into()
        )));
        assert!(is_ship_review_failure(&Some("  Review: nitpick".into())));
        assert!(!is_ship_review_failure(&Some("agent crashed".into())));
        assert!(!is_ship_review_failure(&None));

        let prompt = prompt::fix_prompt(FixType::AuditFix, PhaseId::new(11));
        assert!(prompt.contains("/gsd-audit-fix"));
        assert!(!prompt.contains("--gaps-only"));
    }

    /// 23-09 Task 2 origin (D-05), corrected for D-12 (`28-CONTEXT.md`).
    /// D-12 reversed D-05: `devflow.toml`'s `yes_ship` key now DOES reach
    /// `state.yes_ship`, via `commands::start`'s new OR-combine
    /// (`config::yes_ship(project_root) || --yes-ship`,
    /// `crates/devflow-cli/src/commands.rs`). But this test constructs
    /// `State::new` directly, never going through `commands::start` — and
    /// `State::new` takes no project config and reads none, so its own
    /// assertion (a bare `State::new` never derives `yes_ship` from config)
    /// remains true both before and after D-12. What *is* now false is the
    /// old doc comment's broader premise ("`DevflowConfig` has no field of
    /// that name, so nothing ... could ever reach `State`") — `config.rs`
    /// gained exactly that field in this same plan (Task 1). The only path
    /// from `devflow.toml` to `state.yes_ship` is the explicit combine in
    /// `commands::start`, covered by `crates/devflow-cli/tests/
    /// yes_ship_config.rs`, not by this unit-level test.
    #[test]
    fn state_new_alone_never_derives_yes_ship_from_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("devflow.toml"), "yes_ship = true\n").unwrap();

        // The config loader must accept the file — an unknown-to-State-new
        // key must not be a load failure (fail-soft is the existing
        // contract; this just proves this specific key doesn't somehow
        // special-case that).
        let _config = devflow_core::config::load_config(root);

        // State::new takes no project_root-derived config input at all, so
        // no devflow.toml key — including yes_ship, now a real config field
        // — can reach it through this constructor.
        let state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            root.to_path_buf(),
        );
        assert!(
            !state.yes_ship,
            "State::new alone must never derive the Ship pre-authorization from \
             devflow.toml (D-05, narrowed post-D-12: only commands::start's explicit \
             OR-combine may do that — see yes_ship_config.rs)"
        );
    }

    /// 23-09 Task 1 acceptance: with `state.yes_ship` set, `handle_ship_outcome`
    /// writes the Ship gate request exactly once (never reopened) and the
    /// resolved gate carries the flag's literal `--yes-ship` attribution —
    /// proving the auto-response is written through the normal protocol
    /// (`gate_fired` → `notify_fired` → `gate_resolved`), not a bypass.
    #[test]
    fn handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

        let phase = PhaseId::new(50);
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

        handle_ship_outcome(root, &mut state).unwrap();

        // The workflow completed unattended — no human ever wrote a response.
        assert!(
            matches!(
                workflow::load_state(root, phase),
                Err(workflow::WorkflowError::MissingState(_))
            ),
            "the pre-authorized gate must let the run reach a completed Ship without a human"
        );
        assert!(!Gates::gate_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::response_path(root, phase, Stage::Ship).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Ship).exists());

        // Exactly one gate_fired for this phase+stage — the retry-gate reopen
        // path (a second gate_fired) must never have run.
        let contents =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        let gate_fired_count = contents
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| {
                event["event"] == "gate_fired"
                    && phase.matches_json(event.get("phase"))
                    && event["stage"] == "ship"
            })
            .count();
        assert_eq!(
            gate_fired_count, 1,
            "the Ship gate must be written exactly once, not reopened"
        );

        let resolved =
            devflow_core::events::last_event_of_kind_for_phase(root, phase, "gate_resolved")
                .expect("a gate_resolved event must be recorded");
        assert_eq!(resolved["stage"], "ship");
        assert_eq!(resolved["approved"], true);
        assert_eq!(resolved["action"], "advance");
        assert_eq!(
            resolved["responded_by"], "--yes-ship",
            "the gate ledger must carry the pre-authorization's literal attribution"
        );
    }

    /// 23-09 Task 1 acceptance (the negative half): with `state.yes_ship`
    /// unset (the default), `handle_ship_outcome` writes a gate request but
    /// never writes a response — the routine Ship approval still waits for a
    /// human exactly as before this flag existed.
    #[test]
    fn handle_ship_outcome_without_yes_ship_writes_gate_but_no_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = PhaseId::new(51);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        assert!(!state.yes_ship, "yes_ship must default to false");
        workflow::save_state(&state).unwrap();

        let gate_path = Gates::gate_path(root, phase, Stage::Ship);
        let response_path = Gates::response_path(root, phase, Stage::Ship);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                handle_ship_outcome(root, &mut state).unwrap();
            });

            let mut seen = false;
            for _ in 0..150 {
                if gate_path.exists() {
                    seen = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(seen, "handle_ship_outcome must write a gate request");
            // Give the (non-existent) auto-response write a moment to have
            // happened if it were ever going to — it must not.
            std::thread::sleep(std::time::Duration::from_millis(50));
            assert!(
                !response_path.exists(),
                "with yes_ship unset, no response may ever be auto-written — the run must wait for a human"
            );

            // Unblock the poll so the spawned thread finishes.
            std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
            std::fs::write(
                &response_path,
                r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap();
        });
    }

    /// A Code-stage failure must fire a gate AND run the configured notify
    /// hook — with `DEVFLOW_NON_SILENT_GATE=1` since Auto mode would not
    /// normally gate a Code failure (unexpected/never-silent gate). The
    /// notify sentinel is a side effect distinct from the gate file itself,
    /// so it survives even though `Gates::cleanup` removes the gate/
    /// response/ack once the gate resolves. This test sets
    /// `DEVFLOW_GATE_NOTIFY_CMD`, so it's serialized under `ENV_MUTEX`.
    #[test]
    fn non_validate_failure_fires_gate_and_hook() {
        let _guard = env_lock();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let sentinel = root.join("notify-sentinel");

        // SAFETY: serialized under ENV_MUTEX — no other thread in this
        // process sets/removes DEVFLOW_GATE_NOTIFY_CMD concurrently. Note
        // this only prevents races between env-*mutating* tests: any other
        // concurrently-running test that calls `run_gate` (most of them do)
        // will also read whatever we set here and may itself fire our
        // sentinel command with its own `unexpected` value. So we assert
        // only that the hook fired at all (sentinel created), not its exact
        // content — the exact DEVFLOW_NON_SILENT_GATE propagation is already
        // covered contamination-free by gates.rs's
        // `notify_hook_sets_non_silent_flag` (calls the pure
        // `run_notify_command` directly, no global env involved).
        unsafe {
            std::env::set_var(
                "DEVFLOW_GATE_NOTIFY_CMD",
                format!("touch {}", sentinel.display()),
            );
        }

        let phase = PhaseId::new(42);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // A Code-stage failure in Auto mode is exactly the "unexpected" case
        // `run_gate` computes (`!should_gate(..)`) and passes to
        // `fire_gate_notify` — asserted here as a pure, race-free check.
        assert!(!state.mode.should_gate(
            Stage::Code,
            state.consecutive_failures,
            state.phase_validate_failures
        ));

        // Pre-write an Abort response so the call resolves without spawning
        // a monitor (the notify hook already fired by the time `run_gate`
        // starts polling, so this doesn't affect what we're asserting).
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        let result =
            handle_stage_failure(root, &mut state, Stage::Code, Some("build failed".into()));

        // SAFETY: still serialized under ENV_MUTEX from above.
        unsafe {
            std::env::remove_var("DEVFLOW_GATE_NOTIFY_CMD");
        }

        result.unwrap();
        assert!(
            sentinel.exists(),
            "handle_stage_failure must fire the configured notify hook, not silently skip it"
        );
    }

    /// CR-01 regression: after a stage failure's gate resolves via Advance
    /// and the retry (also a stage failure) fires a fresh gate, the SECOND
    /// gate's poll must not instantly resolve from the FIRST gate's
    /// already-consumed response/ack — `handle_stage_failure` must clean
    /// those up before the retry launches.
    #[test]
    fn stage_failure_retry_cleans_stale_response() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = PhaseId::new(43);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // Pre-write an Abort response so the first failure resolves
        // immediately without spawning a monitor.
        let response_path = Gates::response_path(root, phase, Stage::Code);
        std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();
        std::fs::write(
            &response_path,
            r#"{"approved":false,"note":"abort: test cleanup","responded_by":"test"}"#,
        )
        .unwrap();

        handle_stage_failure(root, &mut state, Stage::Code, Some("first failure".into())).unwrap();

        // abort() must have cleaned up the gate/response/ack for Code.
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
        assert!(!Gates::response_path(root, phase, Stage::Code).exists());
        assert!(!Gates::ack_path(root, phase, Stage::Code).exists());

        // Simulate the phase reaching the same gate again later (e.g. a
        // fresh retry after abort would normally clear state, but re-fire
        // here directly to prove the CR-01 stale-response reuse regression
        // is closed): write a fresh gate but no new response.
        Gates::write_gate(root, phase, Stage::Code, "re-fired gate").unwrap();
        let started = std::time::Instant::now();
        let got = Gates::poll_response(root, phase, Stage::Code, 1);
        assert!(
            got.is_none(),
            "poll_response must not instantly resolve from a stale response after cleanup"
        );
        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }

    /// 35.2 criterion 2, Task 1 — checkout replacement with no nonce → FullExecute.
    #[test]
    fn a_checkout_between_dispatches_does_not_read_as_authored_this_run() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(87);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        std::fs::create_dir_all(&worktree).unwrap();
        state.worktree_path = Some(worktree.clone());

        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-test", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        let artifact_path =
            phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded()));

        // Run-start baseline, no nonce (pre-35.2 or never-dispatched).
        std::fs::write(&artifact_path, "verdict: pass — from a PREVIOUS run\n").unwrap();
        state.last_verification_fingerprint =
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase);
        state.last_verification_mtime_nanos =
            devflow_core::agent_result::phase_verification_mtime_nanos(&worktree, phase);
        state.verification_baseline_captured = true;
        assert!(state.verification_run_nonce.is_none());

        // Branch checkout replaces with DIFFERENT bytes.
        std::fs::write(&artifact_path, "verdict: pass — BRANCH CHECKOUT\n").unwrap();
        assert_ne!(
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase),
            state.last_verification_fingerprint,
            "premise: replacement must change fingerprint"
        );

        workflow::save_state(&state).unwrap();
        let fix = select_loop_back_fix(&worktree, phase, &mut state);
        assert_eq!(fix, FixType::FullExecute);
    }

    /// 35.2 criterion 4, D-04 — both dispatch directions in one #[test].
    #[test]
    fn both_dispatch_directions_are_demonstrated_in_one_run() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(88);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        std::fs::create_dir_all(&worktree).unwrap();
        state.worktree_path = Some(worktree.clone());

        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-test", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();
        let artifact = phase_dir.join(format!("{padded}-VERIFICATION.md", padded = phase.padded()));

        // Direction A: nonce present, agent rewrote → GapsOnly.
        std::fs::write(&artifact, "verdict: pass — authored this run\n").unwrap();
        state.last_verification_fingerprint =
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase);
        state.verification_baseline_captured = true;
        state.verification_run_nonce = Some(1);
        std::fs::write(&artifact, "verdict: gaps — rewritten by agent\n").unwrap();
        assert_ne!(
            devflow_core::agent_result::phase_verification_fingerprint(&worktree, phase),
            state.last_verification_fingerprint,
            "premise: agent rewrite must change fingerprint"
        );
        workflow::save_state(&state).unwrap();
        assert_eq!(
            select_loop_back_fix(&worktree, phase, &mut state),
            FixType::GapsOnly,
            "Direction A: nonce present, agent rewrote → GapsOnly"
        );

        // Direction B: same fixture, nonce cleared → FullExecute.
        state.verification_run_nonce = None;
        assert_eq!(
            select_loop_back_fix(&worktree, phase, &mut state),
            FixType::FullExecute,
            "Direction B: nonce absent → FullExecute"
        );
    }
}
