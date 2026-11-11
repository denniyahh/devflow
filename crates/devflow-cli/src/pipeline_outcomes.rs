//! Pipeline seam B (D-06): deciding what happens after a stage produces a
//! result — the `handle_*_outcome` family and the
//! gate-context rendering helpers they share. Extracted mechanically
//! (19-08, D-09 pure move) out of `main.rs` — every function below is
//! byte-identical to its pre-move body modulo an added `pub(crate)` and
//! adjusted `use` paths.
//!
//! **This module sits in the middle of the pipeline's three-way module
//! cycle (19-RESEARCH.md Pattern 1):** it is called by
//! [`crate::pipeline_launch::advance`] once an agent result has been
//! classified, and it calls onward into [`crate::pipeline_gate`]'s
//! `transition`, `loop_back_to_code`, `finish_run`, and `abort` to
//! actually move the state machine.

use crate::CliError;
use crate::pipeline_gate::{
    LoopBackReason, abort, finish_run, loop_back_to_code, run_gate, run_gate_with_event_context,
    transition,
};
use crate::pipeline_launch::{
    close_gate, commit, launch_stage, record_relaunch_note, recorded_canary_refusal,
};
pub(crate) use devflow_core::gates::render_gate_context;
use devflow_core::gates::{GateAction, GateFile, GateKind, GateLabel, GateResponse};
use devflow_core::gsd_reader::{self, GsdStatus, ReaderFailure, StatusReport};
use devflow_core::mode::{self, Mode};
use devflow_core::phase_id::PhaseId;
use devflow_core::prompt::{FixType, ReverifyReport};
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::{
    agent_result,
    agent_result::{AgentStatus, Verdict},
    events, reset_time,
};
use std::path::{Path, PathBuf};

/// Route a `GateInfra` outcome (ResourceKilled/AgentUnavailable) — bumps
/// `state.infra_failures` (saturating, never `consecutive_failures`),
/// persists, then either aborts at the ceiling or fires the never-silent
/// gate via [`handle_stage_failure`]. Deliberately never calls
/// `handle_validate_outcome` on any stage (review
/// consensus #4) — it increments `consecutive_failures`, which would
/// conflate an infrastructure fault with an agent-caused failure (D-08).
pub(crate) fn handle_infra_outcome(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    commit(state, |s| {
        s.infra_failures = s.infra_failures.saturating_add(1)
    })?;
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

pub(crate) fn retry_after_from_reason(reason: Option<&str>) -> String {
    reason
        .and_then(|s| s.strip_prefix("rate limited until "))
        .unwrap_or("unknown")
        .to_string()
}

/// Route a `RateLimited` outcome that no fallback can take (CUT-03, D-15,
/// D-20): the run waits at the never-silent rate-limit gate for `stage`
/// ([`GateKind::RateLimit`]), which shows in `gate list`/`gate show` and fires
/// the notify command. Its file records the not-before time when the denial
/// named a parseable reset ([`GateLabel::rate_limit`]), so `gate sweep` spares
/// it until then (D-27.2). Its context leads with the routers' way-on sentence
/// ([`rate_limit_gate_context`]). Answers route through the routers' rate-limit
/// arms ([`dispatch_answer`]): an approve relaunches the stage now on the same
/// agent, a reject relaunches it with the note, and only an abort ends the run.
///
/// Not an infra failure: `infra_failures` is untouched, so the infra ceiling
/// cannot abort a run that is only waiting for quota. No cron-instructions file
/// is written and no `rate_limit_resume_scheduled` event is emitted; nothing
/// ever acted on either (999.186). The gate's wait arms the run's one automatic
/// relaunch at the reset plus the margin while it is unused
/// ([`crate::pipeline_gate::rate_limit_timer_deadline`], D-38, D-45 rule 5), and
/// keeps the gate open past the gate timeout until then (D-27.2).
pub(crate) fn handle_rate_limited_outcome(
    project_root: &Path,
    state: &mut State,
    phase: PhaseId,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let reset_at = reset_time::parse_reset_time(&retry_after_from_reason(reason.as_deref()));
    let context = rate_limit_gate_context(state, phase, stage, reason);
    let answer = run_gate(
        project_root,
        state,
        stage,
        GateLabel::rate_limit(reset_at),
        &context,
    )?;
    dispatch_answer(project_root, state, stage, answer, false)
}

/// The never-silent gate context for a rate limit no fallback can take. The
/// routers' way-on sentence first (C2), because the notify hook and the banner
/// show only the start; then the resume command (with `--agent` when the run
/// is not on the default agent), the reset time when the denial carried one,
/// and the cause.
fn rate_limit_gate_context(
    state: &State,
    phase: PhaseId,
    stage: Stage,
    reason: Option<String>,
) -> String {
    let agent = if state.agent == AgentKind::Claude {
        String::new()
    } else {
        format!(" --agent {}", state.agent)
    };
    let retry_after = retry_after_from_reason(reason.as_deref());
    let reset = if reset_time::parse_reset_time(&retry_after).is_some() {
        format!("the limit resets at {retry_after}")
    } else {
        "the denial carried no reset time".to_string()
    };
    let lead = format!(
        "rate limited with no usable fallback; once this waiting supervisor has ended, \
         `devflow resume --phase {phase}{agent}` relaunches the stage; {reset}. Cause: "
    );
    lead_with_way_on(
        GateKind::RateLimit,
        (phase, stage),
        None,
        &stage_failure_context(stage, &lead, reason),
    )
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
    /// declared and the agent reported `verdict: pass`): finish the run.
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
/// arm** (D-06, ROADMAP criterion 3). Every one of the eight variants appears by
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
            "external verification succeeded but the agent reported gaps".to_string(),
        ),
        (true, AgentStatus::Success, None) => ValidateOutcome::Ambiguous(
            "external verification succeeded but no agent verdict arrived".to_string(),
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
            | AgentStatus::Ambiguous
            | AgentStatus::ResourceKilled
            | AgentStatus::AgentUnavailable
            | AgentStatus::IdleTimeout,
            _,
        ) => ValidateOutcome::Failed,
    }
}

/// The home GSD's launcher is looked up under for a read in `_root`: the
/// run's `HOME` (D-08).
#[cfg(not(test))]
fn gsd_home(_root: &Path) -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// A test build reads GSD only through a stub its test installed
/// (`test_support::GsdStub`, or the child-process variable it names). Without
/// one there is no home, so the read fails closed as a missing launcher and
/// never reaches a real GSD install.
#[cfg(test)]
fn gsd_home(root: &Path) -> Option<PathBuf> {
    crate::test_support::test_gsd_home(root)
}

/// GSD's verification status for `phase`, read through GSD's own reader in
/// `evidence_root` (D-08). The phase directory is resolved under the same root
/// (D-24): the worktree in worktree mode, where the Validate agent wrote.
pub(crate) fn read_verification_status(
    evidence_root: &Path,
    phase: PhaseId,
) -> Result<StatusReport, ReaderFailure> {
    let home = gsd_home(evidence_root).ok_or_else(|| ReaderFailure::LauncherNotFound {
        home: PathBuf::from("$HOME (unset)"),
    })?;
    let dir = gsd_reader::phase_dir(evidence_root, phase)?;
    gsd_reader::verification_status(&home, evidence_root, &dir)
}

/// GSD's name for `status`, as `query verification status` prints it.
pub(crate) fn gsd_status_name(status: GsdStatus) -> &'static str {
    match status {
        GsdStatus::Passed => "passed",
        GsdStatus::GapsFound => "gaps_found",
        GsdStatus::HumanNeeded => "human_needed",
        GsdStatus::Stale => "stale",
        GsdStatus::Missing => "missing",
    }
}

/// Record which GSD answers this run's reads: one `gsd_runtime` event with
/// `runtime-identity`'s version, or with the reader failure's path-free label
/// when the version could not be read. Called once per run, when it starts.
pub(crate) fn record_gsd_runtime(project_root: &Path, phase: PhaseId) {
    let version = gsd_home(project_root)
        .ok_or_else(|| ReaderFailure::LauncherNotFound {
            home: PathBuf::from("$HOME (unset)"),
        })
        .and_then(|home| gsd_reader::runtime_version(&home, project_root));
    let payload = match version {
        Ok(version) => serde_json::json!({ "version": version }),
        Err(failure) => serde_json::json!({ "version": null, "failure": failure.label() }),
    };
    events::emit(project_root, phase, "gsd_runtime", payload);
}

/// The agent's half of a Validate verdict (D-08): one input of
/// [`validate_gate_kind`] and of the route [`handle_validate_outcome`] takes
/// through [`next_step`], GSD's status being the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentVerdict {
    /// The validate agent reported pass.
    Pass,
    /// The validate agent reported fail (gaps, or no verdict).
    Fail,
    /// The external probe and the agent's verdict disagree, or no verdict
    /// arrived beside a passing probe ([`ValidateOutcome::Ambiguous`]).
    Ambiguous,
}

impl AgentVerdict {
    /// The verdict's name in events.
    fn name(self) -> &'static str {
        match self {
            AgentVerdict::Pass => "pass",
            AgentVerdict::Fail => "fail",
            AgentVerdict::Ambiguous => "ambiguous",
        }
    }
}

/// Why one Validate outcome's evidence could not be read (D-24).
#[derive(Debug)]
pub(crate) enum EvidenceFailure {
    /// GSD's reader gave no status.
    Reader(ReaderFailure),
    /// The evidence root's HEAD could not be read after an agent PASS, so the
    /// PASS cannot be recorded against its commit (D45-4). Holds git's error.
    HeadRead(String),
}

impl EvidenceFailure {
    /// The event-safe name of the failure: no path, no GSD or git message.
    fn label(&self) -> &'static str {
        match self {
            EvidenceFailure::Reader(failure) => failure.label(),
            EvidenceFailure::HeadRead(_) => "validate_head_read_failed",
        }
    }

    /// The failure in full, for the gate context and stderr only.
    fn detail(&self) -> String {
        match self {
            EvidenceFailure::Reader(failure) => failure.to_string(),
            EvidenceFailure::HeadRead(error) => {
                format!("the HEAD of the evidence root could not be read: {error}")
            }
        }
    }
}

/// What one Validate outcome read in its evidence root: GSD's status, or why it
/// could not be read. Read once per outcome, for every agent verdict.
pub(crate) type ValidateRead = Result<StatusReport, EvidenceFailure>;

/// The label a Validate gate records (52-22, D-37): the kind for the
/// outcome's agent verdict and GSD read, with `cap_reached` when a failure
/// counter is at its cap. It only labels: whether a gate opens at all is
/// [`next_step`]'s decision, except that a `failure_cap` label is the
/// streak update's own gate (D-12), and an ambiguous verdict gates at once
/// over every status (T-18-19): both open ahead of the table.
///
/// One arm per (verdict, status-or-failure) pair. A reader failure takes
/// precedence over every verdict (D-24). The cap labels the rows an unattended
/// run would otherwise continue on: a pass over `passed`, `gaps_found`, `stale`
/// and `missing`.
pub(crate) fn validate_gate_kind(
    verdict: AgentVerdict,
    read: &ValidateRead,
    cap_reached: bool,
) -> GateKind {
    use AgentVerdict::{Ambiguous, Fail, Pass};
    use GsdStatus::{GapsFound, HumanNeeded, Missing, Passed, Stale};
    let capped = |kind| {
        if cap_reached {
            GateKind::FailureCap
        } else {
            kind
        }
    };
    match (verdict, read.as_ref().map(|report| report.status)) {
        (Pass, Err(_)) => GateKind::ReaderFailure,
        (Fail, Err(_)) => GateKind::ReaderFailure,
        (Ambiguous, Err(_)) => GateKind::ReaderFailure,
        (Pass, Ok(Passed)) => capped(GateKind::CleanPass),
        (Pass, Ok(GapsFound)) => capped(GateKind::GapsFound),
        (Pass, Ok(Stale)) => capped(GateKind::ReverifyNeeded),
        (Pass, Ok(Missing)) => capped(GateKind::ReverifyNeeded),
        (Pass, Ok(HumanNeeded)) => GateKind::HumanNeeded,
        (Fail, Ok(Passed)) => GateKind::DisagreementOverPassed,
        (Fail, Ok(GapsFound)) => capped(GateKind::GapsFound),
        (Fail, Ok(Stale)) => capped(GateKind::ReverifyNeeded),
        (Fail, Ok(Missing)) => capped(GateKind::ReverifyNeeded),
        (Fail, Ok(HumanNeeded)) => GateKind::DisagreementOverHumanNeeded,
        (Ambiguous, Ok(Passed)) => GateKind::AmbiguousVerdict,
        (Ambiguous, Ok(GapsFound)) => GateKind::AmbiguousVerdict,
        (Ambiguous, Ok(Stale)) => GateKind::AmbiguousVerdict,
        (Ambiguous, Ok(Missing)) => GateKind::AmbiguousVerdict,
        (Ambiguous, Ok(HumanNeeded)) => GateKind::AmbiguousVerdict,
    }
}

/// What a routing decision is asked about (D-45 rule 2): the end of a Validate
/// in `mode`, or an answer. Only the two status-bearing triggers carry
/// [`Evidence`].
#[derive(Debug, Clone, Copy)]
pub(crate) enum Trigger<'a> {
    /// A Validate outcome, over the evidence that outcome read.
    AfterValidate {
        /// The run's mode.
        mode: Mode,
        /// The outcome's one status read and whether its PASS counts.
        evidence: &'a Evidence,
    },
    /// An approve of a verification gate, over evidence read when it was
    /// picked up.
    Approve(&'a Evidence),
    /// A reject of a verification gate, with its note.
    Reject(&'a str),
    /// An abort. The table's row for it is `End`; a production abort answer
    /// goes straight to `abort` (`Answer::Abort`), so only the rule-6 property
    /// and the way-on sentence ask the table about one.
    Abort,
}

/// The next step of a run, as D-45 rule 2's table names it.
#[derive(Debug)]
pub(crate) enum Step {
    /// End the run as finished, with the permit only [`can_finish`] makes.
    Finish(FinishPermit),
    /// Open a gate (after a Validate).
    Gate,
    /// Launch Validate again (an approve over `passed` without a PASS on HEAD).
    ValidateAgain,
    /// The gap-closure loop through Plan (D-09, D-36).
    GapLoop,
    /// The re-verify Code pass (D-08, D-32 part 1).
    Reverify,
    /// Reopen the gate with the fresh facts (an approve over `human_needed` or
    /// a reader failure).
    Reopen,
    /// `/gsd-quick` with the operator's note, then Validate (D-10).
    Quick(String),
    /// End the run (D-13).
    End,
}

impl Step {
    /// The step's name in `gate_resolved` and the route print.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Step::Finish(_) => "finish",
            Step::Gate => "gate",
            Step::ValidateAgain => "validate_again",
            Step::GapLoop => "gap_loop",
            Step::Reverify => "reverify",
            Step::Reopen => "reopen",
            Step::Quick(_) => "quick",
            Step::End => "end",
        }
    }
}

/// The D-45 table (rule 2), and the only place it lives: the next step for
/// `trigger`, from GSD's fresh status and whether the Validate agent PASS is on
/// HEAD, never from a gate's kind. A finish is reached only through
/// [`can_finish`], whose permit the [`Step::Finish`] variant carries.
pub(crate) fn next_step(trigger: Trigger<'_>) -> Step {
    use GsdStatus::{GapsFound, HumanNeeded, Missing, Passed, Stale};
    match trigger {
        // D-04: in supervise mode every Validate ends at a gate.
        Trigger::AfterValidate {
            mode: Mode::Supervise,
            evidence: _,
        } => Step::Gate,
        Trigger::AfterValidate {
            mode: Mode::Auto,
            evidence,
        } => match evidence.status() {
            Some(Passed) => can_finish(evidence).map_or(Step::Gate, Step::Finish),
            Some(GapsFound) => Step::GapLoop,
            Some(Stale) => Step::Reverify,
            Some(Missing) => Step::Reverify,
            Some(HumanNeeded) => Step::Gate,
            None => Step::Gate,
        },
        Trigger::Approve(evidence) => match evidence.status() {
            Some(Passed) => can_finish(evidence).map_or(Step::ValidateAgain, Step::Finish),
            Some(GapsFound) => Step::GapLoop,
            Some(Stale) => Step::Reverify,
            Some(Missing) => Step::Reverify,
            Some(HumanNeeded) => Step::Reopen,
            None => Step::Reopen,
        },
        Trigger::Reject(note) => Step::Quick(note.to_string()),
        Trigger::Abort => Step::End,
    }
}

/// Which answers a gate's kind takes (D-45 rule 2, D-37, D-39): the kind is a
/// label, so every verification kind is one family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnswerFamily {
    /// The ten verification kinds: routed by GSD's fresh status.
    Verification,
    /// A failed stage (D-39).
    StageFailure,
    /// A rate limit no fallback can take (D-15, D-27.4, D-37): an approve
    /// relaunches the stage now on the same agent, a reject relaunches it with
    /// the note. It keeps its own answers and can never finish a run (D-45
    /// rule 2).
    RateLimit,
    /// A failed preflight check (D-39, D-18f).
    Preflight,
    /// A human-only checkpoint declaration at Code (D-39).
    CheckpointDeclaration,
    /// A gate file with no kind: only a 3.x or hand-written file.
    Unclassified,
}

/// The family of a gate whose file records `kind`.
pub(crate) fn answer_family(kind: Option<GateKind>) -> AnswerFamily {
    match kind {
        Some(GateKind::CleanPass) => AnswerFamily::Verification,
        Some(GateKind::GapsFound) => AnswerFamily::Verification,
        Some(GateKind::ReverifyNeeded) => AnswerFamily::Verification,
        Some(GateKind::HumanNeeded) => AnswerFamily::Verification,
        Some(GateKind::DisagreementOverPassed) => AnswerFamily::Verification,
        Some(GateKind::DisagreementOverHumanNeeded) => AnswerFamily::Verification,
        Some(GateKind::ReaderFailure) => AnswerFamily::Verification,
        Some(GateKind::AmbiguousVerdict) => AnswerFamily::Verification,
        Some(GateKind::FailureCap) => AnswerFamily::Verification,
        Some(GateKind::NoGapClosurePlan) => AnswerFamily::Verification,
        Some(GateKind::StageFailure) => AnswerFamily::StageFailure,
        Some(GateKind::RateLimit) => AnswerFamily::RateLimit,
        Some(GateKind::Preflight) => AnswerFamily::Preflight,
        Some(GateKind::CheckpointDeclaration) => AnswerFamily::CheckpointDeclaration,
        None => AnswerFamily::Unclassified,
    }
}

/// A status-less gate's own answer (D-39). Never a finish: there is no such
/// variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StatusLessStep {
    /// Retry the failed stage.
    RetryStage,
    /// Relaunch the rate-limited stage now, on the same agent (D-15, D-27.4).
    RelaunchNow,
    /// Relaunch the failed stage with the reject's note (GATE-01, 52-26).
    RelaunchWithNote(String),
    /// The run's recorded delivery canary refuses its agent (999.187): stop the
    /// run naming `devflow resume --phase N --agent <other>`, keeping a reject's
    /// note for that relaunch.
    RelaunchRefused {
        /// The recorded refusal.
        refusal: String,
        /// A reject's note, kept for the resume; `None` for an approve.
        note: Option<String>,
    },
    /// The D-18f override: relaunch with the adjudicated check skipped.
    PreflightOverride,
    /// Record the scanned checkpoint set as seen and relaunch.
    RecordCheckpointSet,
}

impl StatusLessStep {
    /// The step's name in `gate_resolved`.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            StatusLessStep::RetryStage => "retry_stage",
            StatusLessStep::RelaunchNow => "relaunch_now",
            StatusLessStep::RelaunchWithNote(_) => "relaunch_with_note",
            StatusLessStep::RelaunchRefused { .. } => "relaunch_refused",
            StatusLessStep::PreflightOverride => "preflight_override",
            StatusLessStep::RecordCheckpointSet => "record_checkpoint_set",
        }
    }
}

/// An answer the routers refuse (R10): which answer, why, and the answers that
/// apply on that gate, for the reopened gate's context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    /// `approve` or `reject`.
    pub(crate) answer: &'static str,
    /// Why it is refused.
    pub(crate) why: &'static str,
    /// The answers that apply.
    pub(crate) applies: &'static str,
}

/// A reject on a gate whose reject is not an answer (D-39).
const REJECT_NOT_AN_ANSWER: Refusal = Refusal {
    answer: "reject",
    why: "a reject is not an answer to this gate",
    applies: "`devflow gate approve` or `devflow gate abort`",
};

/// A reject with an empty note whose route carries the note.
const EMPTY_NOTE: Refusal = Refusal {
    answer: "reject",
    why: "a reject must carry a note, which is what the relaunched agent acts on",
    applies: "`devflow gate reject --note <text>`, `devflow gate approve` or `devflow gate abort`",
};

/// A relaunch action where only [`GateAction::from_response`]'s actions can
/// arrive. `from_response` never returns [`GateAction::Relaunch`]; only the
/// rate-limit mapping [`GateAction::from_gate_answer`] does, and nothing here
/// calls it yet (52-32). Refused, never acted on, so the impossible case is
/// loud: the screen's refusal names the stage and this action, and so does the
/// dispatch's error for a routed one.
const UNEXPECTED_RELAUNCH: Refusal = Refusal {
    answer: "relaunch",
    why: "a relaunch action arrived where only an approve, a reject or an abort is read",
    applies: "`devflow gate abort`",
};

/// Any answer but abort on a gate file with no kind (a 3.x or hand-written
/// file).
fn unclassified(answer: &'static str) -> Refusal {
    Refusal {
        answer,
        why: "the gate file records no kind, so DevFlow cannot tell what this answer would run",
        applies: "`devflow gate abort`",
    }
}

/// How an answer reached a status-less gate.
#[derive(Debug, Clone, Copy)]
pub(crate) enum StatusLessAnswer<'a> {
    /// An approve.
    Approve,
    /// A reject with its note.
    Reject(&'a str),
}

/// A status-less gate's own answer (D-39), or its refusal. `canary_refusal` is
/// read by the stage-failure family only: when the run's recorded delivery
/// canary refuses its agent, both answers stop the run (999.187).
pub(crate) fn status_less_step(
    family: AnswerFamily,
    answer: StatusLessAnswer<'_>,
    canary_refusal: Option<&str>,
) -> Result<StatusLessStep, Refusal> {
    use StatusLessAnswer::{Approve, Reject};
    match (family, answer) {
        (AnswerFamily::StageFailure, Reject(note)) if note.trim().is_empty() => Err(EMPTY_NOTE),
        (AnswerFamily::StageFailure, answer) => Ok(match (canary_refusal, answer) {
            (Some(refusal), Approve) => StatusLessStep::RelaunchRefused {
                refusal: refusal.to_string(),
                note: None,
            },
            (Some(refusal), Reject(note)) => StatusLessStep::RelaunchRefused {
                refusal: refusal.to_string(),
                note: Some(note.to_string()),
            },
            (None, Approve) => StatusLessStep::RetryStage,
            (None, Reject(note)) => StatusLessStep::RelaunchWithNote(note.to_string()),
        }),
        // The rate-limit family reads no canary (T-52-67).
        (AnswerFamily::RateLimit, Reject(note)) if note.trim().is_empty() => Err(EMPTY_NOTE),
        (AnswerFamily::RateLimit, Approve) => Ok(StatusLessStep::RelaunchNow),
        (AnswerFamily::RateLimit, Reject(note)) => {
            Ok(StatusLessStep::RelaunchWithNote(note.to_string()))
        }
        (AnswerFamily::Preflight, Approve) => Ok(StatusLessStep::PreflightOverride),
        (AnswerFamily::Preflight, Reject(_)) => Err(REJECT_NOT_AN_ANSWER),
        (AnswerFamily::CheckpointDeclaration, Approve) => Ok(StatusLessStep::RecordCheckpointSet),
        (AnswerFamily::CheckpointDeclaration, Reject(_)) => Err(REJECT_NOT_AN_ANSWER),
        (AnswerFamily::Unclassified, Approve) => Err(unclassified("approve")),
        (AnswerFamily::Unclassified, Reject(_)) => Err(unclassified("reject")),
        // Not status-less: the verification family goes through `next_step`.
        (AnswerFamily::Verification, Approve) => Err(unclassified("approve")),
        (AnswerFamily::Verification, Reject(_)) => Err(unclassified("reject")),
    }
}

/// Where an approve goes.
#[derive(Debug)]
pub(crate) enum ApproveRoute {
    /// A verification gate's approve: the table's step over the evidence read
    /// when the approve was picked up.
    Verification {
        /// The step [`next_step`] chose.
        step: Step,
        /// The fresh evidence it chose on.
        evidence: Evidence,
    },
    /// A status-less gate's approve, which reads no GSD status (D-39).
    StatusLess(StatusLessStep),
    /// The approve is refused.
    Refused(Refusal),
}

/// Where a reject goes.
#[derive(Debug)]
pub(crate) enum RejectRoute {
    /// A verification gate's reject: `/gsd-quick` with the note (D-10).
    Verification(Step),
    /// A status-less gate's reject (D-39).
    StatusLess(StatusLessStep),
    /// The reject is refused.
    Refused(Refusal),
}

/// The route of an approve on `gate` (D-39, D-45 rule 2). It reads the gate
/// only through [`answer_family`]; `read` is called, and [`next_step`] asked,
/// only for the verification family, so a status-less approve reads no GSD
/// status.
pub(crate) fn approve_route(
    gate: &GateFile,
    canary_refusal: Option<&str>,
    read: impl FnOnce() -> Evidence,
) -> ApproveRoute {
    approve_route_by_kind(gate.gate_kind, canary_refusal, read)
}

/// [`approve_route`] over the gate's recorded kind alone, the only field of
/// the gate file it reads; the way-on sentence asks it before a gate file
/// exists.
pub(crate) fn approve_route_by_kind(
    kind: Option<GateKind>,
    canary_refusal: Option<&str>,
    read: impl FnOnce() -> Evidence,
) -> ApproveRoute {
    match answer_family(kind) {
        AnswerFamily::Verification => {
            let evidence = read();
            let step = next_step(Trigger::Approve(&evidence));
            ApproveRoute::Verification { step, evidence }
        }
        family @ (AnswerFamily::StageFailure
        | AnswerFamily::RateLimit
        | AnswerFamily::Preflight
        | AnswerFamily::CheckpointDeclaration
        | AnswerFamily::Unclassified) => {
            match status_less_step(family, StatusLessAnswer::Approve, canary_refusal) {
                Ok(step) => ApproveRoute::StatusLess(step),
                Err(refusal) => ApproveRoute::Refused(refusal),
            }
        }
    }
}

/// The route of a reject with `note` on `gate` (D-10, D-39). It reads the gate
/// only through [`answer_family`].
pub(crate) fn reject_route(
    gate: &GateFile,
    canary_refusal: Option<&str>,
    note: &str,
) -> RejectRoute {
    reject_route_by_kind(gate.gate_kind, canary_refusal, note)
}

/// [`reject_route`] over the gate's recorded kind alone, the only field of the
/// gate file it reads.
pub(crate) fn reject_route_by_kind(
    kind: Option<GateKind>,
    canary_refusal: Option<&str>,
    note: &str,
) -> RejectRoute {
    match answer_family(kind) {
        AnswerFamily::Verification if note.trim().is_empty() => RejectRoute::Refused(EMPTY_NOTE),
        AnswerFamily::Verification => RejectRoute::Verification(next_step(Trigger::Reject(note))),
        family @ (AnswerFamily::StageFailure
        | AnswerFamily::RateLimit
        | AnswerFamily::Preflight
        | AnswerFamily::CheckpointDeclaration
        | AnswerFamily::Unclassified) => {
            match status_less_step(family, StatusLessAnswer::Reject(note), canary_refusal) {
                Ok(step) => RejectRoute::StatusLess(step),
                Err(refusal) => RejectRoute::Refused(refusal),
            }
        }
    }
}

/// What `step` does, in the words the way-on sentence and the `gate approve`
/// and `gate reject` print both use: the one place a verification step's
/// description lives, so neither can drift from the variant it names.
pub(crate) fn step_phrase(step: &Step) -> &'static str {
    match step {
        Step::Finish(_) => "finishes the run",
        Step::Gate => "opens a gate",
        Step::ValidateAgain => "runs Validate again",
        Step::GapLoop => {
            "runs the gap loop (Plan `--gaps`, then Code's gaps-only pass, then Validate)"
        }
        Step::Reverify => "relaunches Code for a re-verify pass, then Validate",
        Step::Reopen => "opens this gate again over GSD's fresh status",
        Step::Quick(_) => "runs `/gsd-quick` with the note, then Validate",
        Step::End => "ends the run",
    }
}

/// What a status-less `step` at `stage` of `phase` does, in the words the
/// way-on sentence and the CLI print both use (D-39): the one place its
/// description lives.
pub(crate) fn status_less_phrase(step: &StatusLessStep, phase: PhaseId, stage: Stage) -> String {
    match step {
        StatusLessStep::RetryStage => format!("retries the {stage} stage"),
        StatusLessStep::RelaunchNow => {
            format!("relaunches the {stage} stage now on the same agent")
        }
        StatusLessStep::RelaunchWithNote(_) => {
            format!("relaunches the {stage} stage with the note")
        }
        // 999.187: the way on leads the clause, so it survives a display bound.
        StatusLessStep::RelaunchRefused { .. } => format!(
            "stops the run, and the way on is `devflow resume --phase {phase} --agent <other>`, \
             because the delivery canary recorded for this run refuses its agent"
        ),
        StatusLessStep::PreflightOverride => {
            format!("relaunches the {stage} stage with the failed check overridden")
        }
        StatusLessStep::RecordCheckpointSet => {
            format!("records the checkpoints as seen and relaunches the {stage} stage")
        }
    }
}

/// The approve clause of every verification gate (U10): the rule, not a row.
/// An approve is routed on GSD's status read when the supervisor picks it up
/// (D-45 rule 2), so a route chosen from the evidence the gate was opened over
/// could name one the approve does not run.
const VERIFICATION_APPROVE_RULE: &str = "reads GSD's verification status again and finishes \
     the run only on `passed` with a Validate pass on this commit over a clean tree, otherwise \
     taking the route the fresh read names";

/// The note a reject's route is rendered with: any non-empty note takes the
/// route a reject with a note takes, and no route's description quotes it.
const RENDERED_NOTE: &str = "the operator's note";

/// GATE-04's way on (W3, D-39, C2): one sentence naming each answer's route on
/// a gate of `kind` at `stage` of `phase`, or that the answer is not offered,
/// and that abort ends the run. Rendered from [`approve_route_by_kind`] and
/// [`reject_route_by_kind`] over the kind and `canary_refusal`, the run's
/// recorded delivery canary refusal, except that a verification gate's approve
/// clause states the finish rule (U10). Approve and reject on a stage-failure
/// gate under a refusing canary are the same route, so they share one clause.
pub(crate) fn way_on_sentence(
    kind: GateKind,
    phase: PhaseId,
    stage: Stage,
    canary_refusal: Option<&str>,
) -> String {
    let approve = match answer_family(Some(kind)) {
        AnswerFamily::Verification => Some(VERIFICATION_APPROVE_RULE.to_string()),
        AnswerFamily::StageFailure
        | AnswerFamily::RateLimit
        | AnswerFamily::Preflight
        | AnswerFamily::CheckpointDeclaration
        | AnswerFamily::Unclassified => {
            // A status-less approve reads no evidence (D-39): the closure is
            // never called for these families.
            match approve_route_by_kind(Some(kind), canary_refusal, unread_evidence) {
                ApproveRoute::StatusLess(step) => Some(status_less_phrase(&step, phase, stage)),
                ApproveRoute::Verification { step, .. } => Some(step_phrase(&step).to_string()),
                ApproveRoute::Refused(_) => None,
            }
        }
    };
    let reject = match reject_route_by_kind(Some(kind), canary_refusal, RENDERED_NOTE) {
        RejectRoute::Verification(step) => Some(step_phrase(&step).to_string()),
        RejectRoute::StatusLess(step) => Some(status_less_phrase(&step, phase, stage)),
        RejectRoute::Refused(_) => None,
    };
    let answers = match (approve, reject) {
        (Some(approve), Some(reject)) if approve == reject => {
            format!("approve or reject with a note: {approve}")
        }
        (Some(approve), Some(reject)) => {
            format!("approve: {approve}; reject with a note: {reject}")
        }
        (Some(approve), None) => format!("approve: {approve}; reject: not offered"),
        (None, Some(reject)) => format!("approve: not offered; reject with a note: {reject}"),
        (None, None) => "approve: not offered; reject: not offered".to_string(),
    };
    format!(
        "Answers — {answers}; abort: {}.",
        step_phrase(&next_step(Trigger::Abort))
    )
}

/// The evidence handed to a router that reads none: the status-less families
/// never call their `read`, so this is a reader failure nothing routes on.
pub(crate) fn unread_evidence() -> Evidence {
    Evidence {
        read: Err(EvidenceFailure::HeadRead(
            "no evidence is read for a status-less gate".to_string(),
        )),
        agent_pass_on_head: false,
        no_pass_cause: None,
    }
}

/// `cause`, led by the way-on sentence for a gate of `kind` (C2): first, after
/// a leading `[never-silent]` marker if `cause` has one, so the notify hook,
/// the banner and `gate list` show it before the cause. An empty cause yields
/// the sentence alone (after the marker, if any).
pub(crate) fn lead_with_way_on(
    kind: GateKind,
    at: (PhaseId, Stage),
    canary_refusal: Option<&str>,
    cause: &str,
) -> String {
    const MARKER: &str = "[never-silent] ";
    let sentence = way_on_sentence(kind, at.0, at.1, canary_refusal);
    let (lead, rest) = match cause.strip_prefix(MARKER) {
        Some(rest) => (format!("{MARKER}{sentence}"), rest),
        None => (sentence, cause),
    };
    if rest.is_empty() {
        lead
    } else {
        format!("{lead} {rest}")
    }
}

/// The answer screen (R10): whether `response` on a gate whose file records
/// `kind` is refused, from the gate file and the answer alone, never GSD.
/// Runs before the answer is claimed, so a refused answer is never acted on.
pub(crate) fn screen_answer(
    kind: Option<GateKind>,
    response: &GateResponse,
) -> Result<(), Refusal> {
    let family = answer_family(kind);
    match GateAction::from_response(response) {
        GateAction::Abort(_) => Ok(()),
        GateAction::Relaunch { .. } => Err(UNEXPECTED_RELAUNCH),
        GateAction::Advance => match family {
            AnswerFamily::Unclassified => Err(unclassified("approve")),
            AnswerFamily::Verification
            | AnswerFamily::StageFailure
            | AnswerFamily::RateLimit
            | AnswerFamily::Preflight
            | AnswerFamily::CheckpointDeclaration => Ok(()),
        },
        GateAction::LoopBack { note, .. } => {
            let note = note.unwrap_or_default();
            match family {
                AnswerFamily::Verification if note.trim().is_empty() => Err(EMPTY_NOTE),
                AnswerFamily::Verification => Ok(()),
                AnswerFamily::StageFailure
                | AnswerFamily::RateLimit
                | AnswerFamily::Preflight
                | AnswerFamily::CheckpointDeclaration
                | AnswerFamily::Unclassified => {
                    status_less_step(family, StatusLessAnswer::Reject(&note), None).map(|_| ())
                }
            }
        }
    }
}

/// A gate's answer, routed.
#[derive(Debug)]
pub(crate) enum Answer {
    /// An approve and its route.
    Approve(ApproveRoute),
    /// A reject and its route.
    Reject(RejectRoute),
    /// An abort, with its reason.
    Abort(String),
}

impl Answer {
    /// The route fields `gate_resolved` records: the route's name, and for a
    /// verification approve the fresh status (null for a reader failure) and
    /// whether the agent PASS was on HEAD.
    pub(crate) fn resolved_fields(&self) -> serde_json::Value {
        match self {
            Answer::Approve(ApproveRoute::Verification { step, evidence }) => serde_json::json!({
                "route": step.name(),
                "gsd_status": evidence.status().map(gsd_status_name),
                "agent_pass_on_head": evidence.agent_pass_on_head,
            }),
            Answer::Approve(ApproveRoute::StatusLess(step))
            | Answer::Reject(RejectRoute::StatusLess(step)) => {
                serde_json::json!({ "route": step.name() })
            }
            Answer::Reject(RejectRoute::Verification(step)) => {
                serde_json::json!({ "route": step.name() })
            }
            Answer::Approve(ApproveRoute::Refused(_)) | Answer::Reject(RejectRoute::Refused(_)) => {
                serde_json::json!({ "route": "refused" })
            }
            Answer::Abort(_) => serde_json::json!({ "route": "end" }),
        }
    }
}

/// The evidence root of `state`'s run: its worktree, else the project root
/// (D-24).
pub(crate) fn evidence_root(project_root: &Path, state: &State) -> PathBuf {
    state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf())
}

/// Route `response`, the claimed answer to `gate` (the file read back from
/// disk), through [`approve_route`] and [`reject_route`]: a verification
/// approve reads the evidence fresh (D-45 rule 2); nothing here reads the
/// gate's context or a status it recorded. The run's recorded delivery canary
/// refusal is passed for the stage-failure family (999.187).
pub(crate) fn route_answer(
    project_root: &Path,
    state: &State,
    gate: &GateFile,
    response: &GateResponse,
) -> Answer {
    let canary_refusal = recorded_canary_refusal(state);
    match GateAction::from_response(response) {
        GateAction::Abort(reason) => Answer::Abort(reason),
        GateAction::Advance => {
            Answer::Approve(approve_route(gate, canary_refusal.as_deref(), || {
                read_evidence(
                    &evidence_root(project_root, state),
                    state.phase,
                    state.validate_pass_head.as_deref(),
                )
            }))
        }
        GateAction::LoopBack { note, .. } => Answer::Reject(reject_route(
            gate,
            canary_refusal.as_deref(),
            note.as_deref().unwrap_or_default(),
        )),
        // `dispatch_answer` returns this refusal as an error naming the stage.
        GateAction::Relaunch { .. } => Answer::Approve(ApproveRoute::Refused(UNEXPECTED_RELAUNCH)),
    }
}

/// What GSD said in this round's read, for the re-verify pass's prompt to
/// quote verbatim (D-32 part 1): its status name, route and next action.
/// `None` when the read failed.
fn reverify_report(read: &ValidateRead) -> Option<ReverifyReport> {
    read.as_ref().ok().map(|report| ReverifyReport {
        status: gsd_status_name(report.status).to_string(),
        route: report.route.clone(),
        next_action: report.next_action.clone(),
    })
}

/// The evidence root's HEAD, through the hermetic git command, or git's error.
fn read_evidence_head(evidence_root: &Path) -> Result<String, String> {
    let output = devflow_core::git::hermetic_command("git", evidence_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("git rev-parse HEAD could not run: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git rev-parse HEAD exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if head.is_empty() {
        return Err("git rev-parse HEAD printed nothing".to_string());
    }
    Ok(head)
}

/// What the D-46 dirty-tree read found over the evidence root: one
/// `git status --porcelain --untracked-files=all` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TreeRead {
    /// The read listed nothing.
    Clean,
    /// The paths the read listed, in its order.
    Dirty(Vec<String>),
    /// The read could not run or failed; holds git's error.
    Failed(String),
}

/// The D-46 dirty-tree read: `git status --porcelain --untracked-files=all`
/// over `evidence_root`, through the hermetic git command. `.planning/` is
/// included, and untracked files are listed whatever the user, global or repo
/// config sets for `status.showUntrackedFiles`. DevFlow's own `.devflow/` is
/// ignored by the `*` `.gitignore` `workflow::ensure_devflow_dir` writes there.
pub(crate) fn read_tree(evidence_root: &Path) -> TreeRead {
    let output = match devflow_core::git::hermetic_command("git", evidence_root)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
    {
        Ok(output) => output,
        Err(error) => return TreeRead::Failed(format!("git status could not run: {error}")),
    };
    if !output.status.success() {
        return TreeRead::Failed(format!(
            "git status exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let paths: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.get(3..).unwrap_or(line).to_string())
        .collect();
    if paths.is_empty() {
        TreeRead::Clean
    } else {
        TreeRead::Dirty(paths)
    }
}

/// What a routing decision is made on (D-45 rule 2, D-46): GSD's fresh status
/// read, and whether the run's recorded Validate agent PASS names the evidence
/// root's current HEAD over a clean tree. `no_pass_cause` is why a PASS does not
/// count when the reason is a failed read or a dirty tree, worded for the gate
/// context it leads to (`[never-silent]`); it never routes.
#[derive(Debug)]
pub(crate) struct Evidence {
    /// GSD's status read, or why it could not be read.
    pub(crate) read: ValidateRead,
    /// The recorded PASS head equals HEAD and the tree read listed nothing.
    pub(crate) agent_pass_on_head: bool,
    /// Why no PASS counts: a failed HEAD read, a failed tree read, or the
    /// dirty paths. `None` when a PASS counts or simply none is recorded.
    pub(crate) no_pass_cause: Option<String>,
}

impl Evidence {
    /// GSD's status, or `None` for a reader failure.
    pub(crate) fn status(&self) -> Option<GsdStatus> {
        self.read.as_ref().ok().map(|report| report.status)
    }
}

/// `read_evidence`'s pure part: the evidence from its three reads' results.
/// `recorded_head` is the run's recorded Validate PASS head, `current_head` the
/// evidence root's HEAD read (or git's error), `tree` the D-46 read.
pub(crate) fn evidence_from_reads(
    read: ValidateRead,
    recorded_head: Option<&str>,
    current_head: Result<&str, &str>,
    tree: &TreeRead,
) -> Evidence {
    let head_cause = |error: &str| {
        format!(
            "[never-silent] the HEAD of the evidence root could not be read ({}), so no \
             Validate pass counts on it.",
            truncate_reason(error)
        )
    };
    let tree_cause = match tree {
        TreeRead::Clean => None,
        TreeRead::Dirty(paths) => Some(format!(
            "[never-silent] the tree has uncommitted changes ({}); a Validate pass counts \
             only on a clean tree.",
            truncate_reason(&paths.join(", "))
        )),
        TreeRead::Failed(error) => Some(format!(
            "[never-silent] the state of the tree could not be read ({}); a Validate pass \
             counts only on a clean tree.",
            truncate_reason(error)
        )),
    };
    let (agent_pass_on_head, no_pass_cause) = match (current_head, tree_cause) {
        (Err(error), _) => (false, Some(head_cause(error))),
        (Ok(_), Some(cause)) => (false, Some(cause)),
        (Ok(head), None) => (recorded_head == Some(head), None),
    };
    Evidence {
        read,
        agent_pass_on_head,
        no_pass_cause,
    }
}

/// The evidence an answer is routed on, read fresh when the answer is picked
/// up, never what a gate recorded (D-45 rule 2): GSD's status under the
/// evidence root, then the D-46 tree read, then the root's HEAD (tree before
/// HEAD, so a commit landing between the two reads shows as a HEAD change),
/// compared with `recorded_head`.
pub(crate) fn read_evidence(
    evidence_root: &Path,
    phase: PhaseId,
    recorded_head: Option<&str>,
) -> Evidence {
    let read = read_verification_status(evidence_root, phase).map_err(EvidenceFailure::Reader);
    let tree = read_tree(evidence_root);
    let head = read_evidence_head(evidence_root);
    evidence_from_reads(
        read,
        recorded_head,
        head.as_deref().map_err(String::as_str),
        &tree,
    )
}

/// The one finish check (D-45 rule 1) and the permit only it makes. Kept in
/// its own module so nothing else in `pipeline_outcomes` can build a permit:
/// its field is private here.
mod finish_permit {
    use super::Evidence;
    use devflow_core::gsd_reader::GsdStatus;

    /// Proof that [`can_finish`] held. Taken by value by
    /// [`crate::pipeline_gate::finish_run`], so no run is recorded finished
    /// without it.
    #[derive(Debug)]
    pub(crate) struct FinishPermit {
        _sealed: (),
    }

    /// A permit only when GSD's fresh status is `passed` and the run's Validate
    /// agent PASS is on the current HEAD over a clean tree (D-45 rule 1, D-46).
    pub(crate) fn can_finish(evidence: &Evidence) -> Option<FinishPermit> {
        (evidence.status() == Some(GsdStatus::Passed) && evidence.agent_pass_on_head)
            .then_some(FinishPermit { _sealed: () })
    }
}
pub(crate) use finish_permit::{FinishPermit, can_finish};

/// Record one Validate outcome's read as a `gsd_verification` event: the
/// agent's verdict and GSD's status, or the failure's label. Never GSD's or
/// git's message, which can hold absolute paths (Pitfall 11).
fn emit_verification_read(
    project_root: &Path,
    phase: PhaseId,
    verdict: AgentVerdict,
    read: &ValidateRead,
) {
    let payload = match read {
        Ok(report) => serde_json::json!({
            "verdict": verdict.name(),
            "status": gsd_status_name(report.status),
        }),
        Err(failure) => serde_json::json!({
            "verdict": verdict.name(),
            "failure": failure.label(),
        }),
    };
    events::emit(project_root, phase, "gsd_verification", payload);
}

/// The context of a Validate gate of `kind`, and the one its `gate_fired`
/// event carries. They differ only for a reader failure, whose detail stays
/// out of the event. Every context but a reader failure's quotes GSD's status
/// verbatim, and says `passed` only when GSD did (GATE-03). Both lead with the
/// way-on sentence the routers render (GATE-04, C2); the cause after it names
/// no answer.
fn validate_gate_context(
    kind: GateKind,
    round: (AgentVerdict, &Evidence),
    ambiguous_detail: Option<&str>,
    state: &State,
    ceiling_gate: bool,
) -> (String, String) {
    let (verdict, evidence) = round;
    let canary_refusal = recorded_canary_refusal(state);
    let lead = |cause: &str| {
        lead_with_way_on(
            kind,
            (state.phase, Stage::Validate),
            canary_refusal.as_deref(),
            cause,
        )
    };
    let report = match &evidence.read {
        Ok(report) => report,
        Err(failure) => {
            let cause = format!(
                "[never-silent] GSD verification could not be read at Validate ({})",
                failure.label()
            );
            return (
                lead(&format!("{cause}: {}.", truncate_reason(&failure.detail()))),
                lead(&format!("{cause}.")),
            );
        }
    };
    // D-46: a pass over GSD's `passed` is clean only when its PASS counts, on
    // the HEAD it read over a clean tree.
    let clean = verdict == AgentVerdict::Pass
        && report.status == GsdStatus::Passed
        && evidence.agent_pass_on_head;
    let mut message = match verdict {
        AgentVerdict::Pass if clean => {
            "Validation passed. The worktree and branch are kept.".to_string()
        }
        AgentVerdict::Pass => "The validate agent reported pass.".to_string(),
        AgentVerdict::Fail => format!(
            "Validation has failed {} time(s) for this phase ({} in the current consecutive \
             streak) — human review needed. The validate agent reported fail.",
            state.phase_validate_failures, state.consecutive_failures
        ),
        AgentVerdict::Ambiguous => format!(
            "[never-silent] validate ambiguous: {}.",
            truncate_reason(ambiguous_detail.unwrap_or("no detail"))
        ),
    };
    message.push_str(&format!(
        " GSD verification: {}.",
        gsd_status_name(report.status)
    ));
    if let Some(cause) = &evidence.no_pass_cause {
        message.push(' ');
        message.push_str(cause);
    }
    if kind == GateKind::CleanPass && !clean {
        message.push_str(" No Validate pass counts on this commit.");
    }
    message.push_str(match kind {
        GateKind::ReverifyNeeded => " The report is out of date or was never written.",
        GateKind::HumanNeeded => " GSD needs a human sign-off.",
        GateKind::DisagreementOverPassed | GateKind::DisagreementOverHumanNeeded => {
            " The validate agent and GSD disagree."
        }
        GateKind::GapsFound
        | GateKind::CleanPass
        | GateKind::AmbiguousVerdict
        | GateKind::FailureCap
        | GateKind::ReaderFailure
        | GateKind::NoGapClosurePlan
        | GateKind::StageFailure
        | GateKind::RateLimit
        | GateKind::Preflight
        | GateKind::CheckpointDeclaration => "",
    });
    // WR-04 (35-REVIEW): the ceiling clause belongs on every Validate gate that
    // fired at the ceiling, a passing one included, or an operator running
    // unattended Auto cannot tell why it fired. F-6: conditioned on the ceiling
    // predicate read once by the caller, never on "a gate fired".
    if ceiling_gate && clean {
        message.push_str(&format!(
            " (This phase recorded {} Validate failure(s), at the per-phase ceiling of {} — that is why this gate fired. An answer that continues the run restarts the count.)",
            state.phase_validate_failures,
            mode::MAX_PHASE_VALIDATE_FAILURES
        ));
    } else if ceiling_gate && verdict != AgentVerdict::Ambiguous {
        message.push_str(&format!(
            " The per-phase ceiling of {} is reached: this run is paused for a human, and an answer that continues the run restarts the count.",
            mode::MAX_PHASE_VALIDATE_FAILURES
        ));
    }
    let message = lead(&message);
    (message.clone(), message)
}

/// Decide what happens after a Validate stage, honoring the active mode's
/// gate policy, the consecutive-failure threshold, and (18e) the immediate
/// gate an ambiguous `external_verify` outcome forces regardless of either.
///
/// 52-22 (D-08, D-24, D45-4): every outcome reads GSD's own verification
/// status once, in the evidence root, and the pair (agent verdict, GSD read)
/// labels the gate through [`validate_gate_kind`] and picks the route through
/// [`next_step`]. A run finishes only over an
/// agent pass that GSD reports `passed`; every other status and every reader
/// failure routes away from the finish. An agent PASS is recorded in
/// `State::validate_pass_head` against the evidence root's HEAD, and every
/// other verdict records `None`; nothing else writes the field.
///
/// 999.66: on a recorded failure, the counter now decides between beginning a
/// fresh streak and continuing the existing one, instead of always
/// continuing. It asks [`mode::consecutive_failures_made_progress`], fed by
/// [`agent_result::phase_source_commit_count`] read fresh against `project_root` and
/// the baseline persisted in `state.last_validate_failure_commit_count`. A
/// `true` (progress) result means new commits that change a path outside
/// `.planning/` exist on the feature branch since the last recorded failure
/// (D-34, 52-27: GSD's own `.planning/` artifacts are not progress) — **not**
/// that those commits addressed what Validate reported. This narrows the ceiling's guarantee to loops that
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
/// be measured — [`agent_result::phase_source_commit_count`] returning `None` — is
/// treated as not-progress AND leaves the recorded baseline untouched, so the
/// next real measurement is compared against the last real observation. A
/// single transient fault can no longer buy a reset. What is still NOT claimed
/// is anything about the run boundary: `State::new` zeroes both the counter
/// and the baseline on every `devflow start`, `--force` included.
///
/// CR-01: the two root-consuming reads in this function are on deliberately
/// **different** roots, and must stay that way. The `{N}-VERIFICATION.md`
/// status read (GSD's reader, and the HEAD a PASS is recorded on) follows
/// the agent's cwd —
/// the phase's worktree when one is configured — because that is where the
/// Validate agent authors the artifact and `.planning/` is tracked, so it is
/// invisible from the main checkout until merge. The
/// [`agent_result::phase_source_commit_count`] read one branch below keeps reading
/// `project_root`, the main checkout, because git refs and the object
/// database are shared across a repository's worktrees, so a commit made in
/// the worktree is already visible from there. Retargeting the commit count
/// at the worktree would fix nothing and would break the 999.66 wiring.
pub(crate) fn handle_validate_outcome(
    project_root: &Path,
    state: &mut State,
    outcome: ValidateOutcome,
) -> Result<(), CliError> {
    // WR-08: the evidence root, resolved ONCE for every read in this function.
    // `.planning/` is tracked and the Validate agent runs in the worktree, so
    // the report it authors lands on `feature/phase-{N}` inside the worktree
    // and is invisible from the main checkout; GSD's reader and the HEAD read
    // must therefore run there (D-24). This is NOT the root the
    // `phase_source_commit_count` read below uses: see the CR-01 note above.
    let evidence_root: PathBuf = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf());
    let (verdict, ambiguous_detail) = match outcome {
        ValidateOutcome::Passed => (AgentVerdict::Pass, None),
        ValidateOutcome::Failed => (AgentVerdict::Fail, None),
        ValidateOutcome::Ambiguous(detail) => (AgentVerdict::Ambiguous, Some(detail)),
    };

    // D-08: the one GSD status read of this outcome, for every verdict.
    let mut read: ValidateRead =
        read_verification_status(&evidence_root, state.phase).map_err(EvidenceFailure::Reader);
    // D45-4: an agent PASS is recorded against the commit it ran on, read now,
    // after the Validate agent's own commits. A HEAD that cannot be read
    // records no PASS and is a reader failure (D-24).
    let pass_head = match verdict {
        AgentVerdict::Pass => match read_evidence_head(&evidence_root) {
            Ok(head) => Some(head),
            Err(error) => {
                if read.is_ok() {
                    read = Err(EvidenceFailure::HeadRead(error));
                }
                None
            }
        },
        AgentVerdict::Fail | AgentVerdict::Ambiguous => None,
    };
    // D-46 and its 2026-11-10 amendment: the outcome's one dirty-tree read,
    // after the HEAD read and before the PASS is recorded. A PASS whose read
    // lists a path or fails records no head, as a FAIL does, so a later
    // clean-up of the tree cannot revive it.
    let tree = match &pass_head {
        Some(_) => read_tree(&evidence_root),
        None => TreeRead::Clean,
    };
    let evidence = match &pass_head {
        Some(head) => evidence_from_reads(read, Some(head), Ok(head), &tree),
        None => Evidence {
            read,
            agent_pass_on_head: false,
            no_pass_cause: None,
        },
    };
    let pass_head = pass_head.filter(|_| evidence.agent_pass_on_head);
    let read = &evidence.read;
    emit_verification_read(project_root, state.phase, verdict, read);
    if let Err(failure) = read {
        eprintln!(
            "phase {}: GSD verification could not be read at Validate: {}",
            state.phase,
            failure.detail()
        );
    }
    // The only write of `validate_pass_head`: every classified verdict sets it.
    // GATE-03: a Validate that reads GSD's `passed` ends a gap-closure pass, in
    // the same commit; no other read clears the marker.
    let read_passed = matches!(read, Ok(report) if report.status == GsdStatus::Passed);
    commit(state, |s| {
        s.validate_pass_head = pass_head.clone();
        if read_passed {
            s.gap_closure_pass = false;
        }
    })?;

    // IN-02: read BEFORE the recording below, which sets the baseline to
    // `Some(current)` on its measured arm and would erase the distinction
    // within the same call. `None` here means no commit baseline existed when
    // this failure was recorded — a genuine first failure of the phase, or
    // state resumed from a binary predating the baseline field. Either way the
    // failure budget is at its full width, which is the fact the event stream
    // did not carry.
    let baseline_absent = state.last_validate_failure_commit_count.is_none();

    // D-08: a Validate passes only when the agent says pass AND GSD reports
    // `passed`, so an agent pass over any other status is a recorded failure
    // too. Without that, an agent that always says pass over a report with
    // gaps would loop with no ceiling. An ambiguous verdict, and a pass whose
    // evidence could not be read, gate at once and touch no counter (T-18-19).
    let counted_failure = match (verdict, read) {
        (AgentVerdict::Fail, _) => true,
        (AgentVerdict::Pass, Ok(report)) => report.status != GsdStatus::Passed,
        (AgentVerdict::Pass, Err(_)) | (AgentVerdict::Ambiguous, _) => false,
    };
    // F-6: read ONCE, and used for both the message's ceiling clause and the
    // reset below, so the two can never disagree about whether this gate was a
    // ceiling gate. Read AFTER the increment, so it reflects the failure being
    // handled right now. The one streak update runs at most once per outcome,
    // so a round counted for two reasons (an agent fail over `stale`) is one
    // failure, and it returns the ceiling read after its own update.
    //
    // Deliberately NOT "did a gate fire": `Stage::Validate` + `Mode::Supervise`
    // gates unconditionally, so in Supervise the two conditions overlap
    // completely. A reset keyed on gating would clear the total at every
    // Supervise failure, the accumulation would never be observable, and the
    // bound would be defeated in exactly the mode where an operator watches
    // every occurrence.
    let ceiling_gate = if counted_failure {
        record_validate_failure(project_root, state)?
    } else {
        mode::phase_failure_ceiling_reached(state.phase_validate_failures)
    };
    // A failure counter at its cap, in either mode: the predicate auto mode
    // gates on (`Mode::should_gate` puts the per-phase ceiling first).
    let cap_reached = Mode::Auto.should_gate(
        Stage::Validate,
        state.consecutive_failures,
        state.phase_validate_failures,
    );

    // 52-22's label and 52-28's streak update, on the same read as the route
    // below, so the label, the count and the route cannot disagree. A
    // `failure_cap` label is the streak update's own gate (D-12): it opens
    // ahead of the table, in either mode. So does an ambiguous verdict
    // (T-18-19), over every GSD status: it counts no failure, so routing it
    // through the table's gap loop or re-verify pass would loop with no
    // ceiling.
    let kind = validate_gate_kind(verdict, read, cap_reached);
    let step = if kind == GateKind::FailureCap || verdict == AgentVerdict::Ambiguous {
        Step::Gate
    } else {
        next_step(Trigger::AfterValidate {
            mode: state.mode,
            evidence: &evidence,
        })
    };
    match step {
        Step::Gate => {
            let (context, event_context) = validate_gate_context(
                kind,
                (verdict, &evidence),
                ambiguous_detail.as_deref(),
                state,
                ceiling_gate,
            );
            // An ambiguous verdict spends no budget, so answering its gate
            // resets none.
            let ceiling_gate = ceiling_gate && verdict != AgentVerdict::Ambiguous;
            // The status the gate was opened over is recorded as a label.
            let label = GateLabel {
                kind,
                gsd_status: evidence.status(),
                not_before: None,
            };
            open_verification_gate(
                project_root,
                state,
                (Stage::Validate, label),
                (&context, &event_context),
                ceiling_gate,
            )
        }
        // D-45 rule 2's after-Validate column: a finish, the gap loop or the
        // re-verify pass, without a gate.
        step => run_step(
            project_root,
            state,
            Stage::Validate,
            step,
            Some(&evidence),
            loop_back_reason(baseline_absent),
        ),
    }
}

/// Open a verification gate (`stage`, `label`) with its context and the one
/// its event carries, and act on its answer through [`dispatch_answer`].
/// `ceiling_gate` is the caller's one read of whether the gate opened at the
/// per-phase ceiling (F-6).
fn open_verification_gate(
    project_root: &Path,
    state: &mut State,
    at: (Stage, GateLabel),
    contexts: (&str, &str),
    ceiling_gate: bool,
) -> Result<(), CliError> {
    let (stage, label) = at;
    let answer =
        run_gate_with_event_context(project_root, state, stage, label, contexts.0, contexts.1)?;
    dispatch_answer(project_root, state, stage, answer, ceiling_gate)
}

/// The one dispatch every routed answer at `stage` goes through, by family
/// (D-45 rule 2, D-39). It owns the single per-phase reset an answer makes
/// (D-12, review 5 pi C3): before any route that continues the run, keyed on
/// the caller's `ceiling_gate`, never on abort. The preflight override and the
/// checkpoint-set record are run by their own gates' sites, which hold the
/// adjudicated check and the scanned set.
pub(crate) fn dispatch_answer(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    answer: Answer,
    ceiling_gate: bool,
) -> Result<(), CliError> {
    match answer {
        // No reset on abort: the phase is ending and `abort` clears its state
        // outright, so there is no budget left to restore.
        Answer::Abort(reason) => abort(project_root, state, &reason),
        Answer::Approve(ApproveRoute::Verification { step, evidence }) => {
            // Reset first: its `commit` replaces `state`.
            reset_phase_failures_at_ceiling(project_root, state, ceiling_gate)?;
            run_step(
                project_root,
                state,
                stage,
                step,
                Some(&evidence),
                LoopBackReason::GateResponse,
            )
        }
        Answer::Reject(RejectRoute::Verification(step)) => {
            reset_phase_failures_at_ceiling(project_root, state, ceiling_gate)?;
            run_step(
                project_root,
                state,
                stage,
                step,
                None,
                LoopBackReason::GateResponse,
            )
        }
        Answer::Approve(ApproveRoute::StatusLess(step))
        | Answer::Reject(RejectRoute::StatusLess(step)) => {
            reset_phase_failures_at_ceiling(project_root, state, ceiling_gate)?;
            run_status_less_step(project_root, state, stage, step)
        }
        // The screen refused these before the claim, so none arrives here.
        Answer::Approve(ApproveRoute::Refused(refusal))
        | Answer::Reject(RejectRoute::Refused(refusal)) => Err(CliError::Message(format!(
            "phase {}: a refused {} on the {stage} gate reached the dispatch: {}",
            state.phase, refusal.answer, refusal.why
        ))),
    }
}

/// Run a status-less gate's own answer at `stage` (D-39).
fn run_status_less_step(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    step: StatusLessStep,
) -> Result<(), CliError> {
    // CR-01: close the gate before relaunching; its answer is spent.
    let _ = close_gate(project_root, state.phase, stage);
    state.gate_pending = false;
    match step {
        StatusLessStep::RetryStage | StatusLessStep::RelaunchNow => {
            launch_stage(state, None, Some(stage))
        }
        StatusLessStep::RelaunchWithNote(note) => {
            // GATE-01 / D-39: the note is the operator's word for this
            // relaunch, committed before the launch. It is never the fix
            // request, so `code_fix` and `code_fix_note` are left alone.
            record_relaunch_note(state, Some(note))?;
            launch_stage(state, None, Some(stage))
        }
        StatusLessStep::RelaunchRefused { refusal, note } => {
            // 52-26: a reject's note is kept for the `devflow resume --agent
            // <other>` the stop names; an approve carries none.
            if let Some(note) = note {
                record_relaunch_note(state, Some(note))?;
            }
            refuse_relaunch(project_root, state, stage, &refusal)
        }
        StatusLessStep::PreflightOverride | StatusLessStep::RecordCheckpointSet => {
            Err(CliError::Message(format!(
                "phase {}: the {} step is run by its own gate's site, not at {stage}",
                state.phase,
                step.name()
            )))
        }
    }
}

/// Run a verification step at `stage`: Validate, or Plan for the gaps Plan
/// gates (52-27). Every route a Validate outcome or a verification answer
/// takes runs here, so a step means the same wherever it is taken.
/// `reason` is the loop-back reason a re-verify pass records.
fn run_step(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    step: Step,
    evidence: Option<&Evidence>,
    reason: LoopBackReason,
) -> Result<(), CliError> {
    match step {
        // D-05, D-45 rule 1: the permit is the proof `can_finish` held.
        Step::Finish(permit) => finish_run(project_root, state, permit),
        Step::GapLoop => {
            let _ = close_gate(project_root, state.phase, stage);
            state.gate_pending = false;
            match stage {
                // GATE-03 / D-09: the gap-closure pass starts with the move to
                // Plan, which sets the marker in its own commit.
                Stage::Validate => transition(project_root, state, Stage::Plan),
                // At a gaps Plan gate the pass is already under way: run the
                // gaps Plan pass again, the marker still set.
                _ => relaunch_or_refuse(project_root, state, stage),
            }
        }
        Step::Reverify => loop_back_to_code(
            project_root,
            state,
            FixType::StaleReverify,
            None,
            evidence.and_then(|evidence| reverify_report(&evidence.read)),
            reason,
        ),
        Step::ValidateAgain => {
            // CR-01: close the gate before relaunching; its answer is spent.
            let _ = close_gate(project_root, state.phase, stage);
            state.gate_pending = false;
            match stage {
                Stage::Validate => relaunch_or_refuse(project_root, state, Stage::Validate),
                _ => transition(project_root, state, Stage::Validate),
            }
        }
        Step::Reopen => match evidence {
            Some(evidence) => reopen_verification_gate(project_root, state, stage, evidence),
            None => Err(CliError::Message(format!(
                "phase {}: a reopen at {stage} has no fresh read",
                state.phase
            ))),
        },
        // GATE-02 / D-10: the operator's small fix, `/gsd-quick` with the note,
        // then Validate. The loop-back clears the gap marker in its commit.
        Step::Quick(note) => loop_back_to_code(
            project_root,
            state,
            FixType::Quick,
            Some(note),
            None,
            LoopBackReason::GateResponse,
        ),
        Step::Gate | Step::End => Err(CliError::Message(format!(
            "phase {}: the {} step has no route at {stage}",
            state.phase,
            step.name()
        ))),
    }
}

/// D-45 rule 2: an approve over `human_needed` or a reader failure reopens the
/// gate with the fresh facts, labelled by the fresh read and counting no
/// failure, and the run waits again.
fn reopen_verification_gate(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    evidence: &Evidence,
) -> Result<(), CliError> {
    let _ = close_gate(project_root, state.phase, stage);
    let (kind, context, event_context) = match &evidence.read {
        Ok(report) => {
            let context = format!(
                "GSD verification, read again: {}. GSD still needs a human sign-off.",
                gsd_status_name(report.status)
            );
            (GateKind::HumanNeeded, context.clone(), context)
        }
        Err(failure) => {
            let cause = format!(
                "[never-silent] GSD verification could not be read again ({})",
                failure.label()
            );
            (
                GateKind::ReaderFailure,
                format!("{cause}: {}.", truncate_reason(&failure.detail())),
                format!("{cause}."),
            )
        }
    };
    // GATE-04 (C2): the way on leads, rendered from the routers.
    let canary_refusal = recorded_canary_refusal(state);
    let lead = |cause: &str| {
        lead_with_way_on(kind, (state.phase, stage), canary_refusal.as_deref(), cause)
    };
    let (context, event_context) = (lead(&context), lead(&event_context));
    let label = GateLabel {
        kind,
        gsd_status: evidence.status(),
        not_before: None,
    };
    open_verification_gate(
        project_root,
        state,
        (stage, label),
        (&context, &event_context),
        false,
    )
}

/// Why a gaps Plan pass's post-check could not establish whether a new gap
/// plan exists (D-24: never silent).
#[derive(Debug)]
enum GapPlanCheckFailure {
    /// GSD's reader failed: the phase directory, or a plan's `gap_closure`.
    Reader(ReaderFailure),
    /// The verification report's or a plan's modified time could not be read.
    FileTime(String),
}

impl GapPlanCheckFailure {
    /// The event-safe name of the failure: no path, no GSD message.
    fn label(&self) -> &'static str {
        match self {
            GapPlanCheckFailure::Reader(failure) => failure.label(),
            GapPlanCheckFailure::FileTime(_) => "gap_plan_file_time_unreadable",
        }
    }

    /// The failure in full, for the gate context and stderr only.
    fn detail(&self) -> String {
        match self {
            GapPlanCheckFailure::Reader(failure) => failure.to_string(),
            GapPlanCheckFailure::FileTime(detail) => detail.clone(),
        }
    }
}

/// A file's modified time, or why it could not be read.
fn modified_time(path: &Path) -> Result<std::time::SystemTime, GapPlanCheckFailure> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| {
            GapPlanCheckFailure::FileTime(format!(
                "the modified time of {} could not be read: {error}",
                path.display()
            ))
        })
}

/// Whether `value`, a `gap_closure` frontmatter value as GSD read it, is true.
/// GSD's own plan reader compares the field with the string `"true"`
/// (`plan-document.cjs`), and YAML may also give a boolean.
fn gap_closure_is_true(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Bool(true))
        || value.as_str().is_some_and(|text| text == "true")
}

/// Whether the phase has a gap-closure plan newer than its verification
/// report (Pitfall 5, U6): a `*-PLAN.md` in the phase directory whose modified
/// time is after the report's and whose `gap_closure`, read through GSD's
/// `frontmatter get`, is true. An earlier round's gap plan predates this
/// round's report and does not count.
///
/// The comparison is on file modified times, the report's against each plan's.
/// Only plans newer than the report are read through GSD; a plan without the
/// field (GSD's `Field not found`) is not a gap plan.
fn has_new_gap_closure_plan(
    evidence_root: &Path,
    phase: PhaseId,
) -> Result<bool, GapPlanCheckFailure> {
    let dir = gsd_reader::phase_dir(evidence_root, phase).map_err(GapPlanCheckFailure::Reader)?;
    let report = dir.join(format!("{}-VERIFICATION.md", phase.padded()));
    let report_time = modified_time(&report)?;
    let mut plans: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|error| {
            GapPlanCheckFailure::FileTime(format!(
                "the phase directory {} could not be listed: {error}",
                dir.display()
            ))
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-PLAN.md"))
        })
        .collect();
    plans.sort();
    let mut newer = Vec::new();
    for plan in plans {
        if modified_time(&plan)? > report_time {
            newer.push(plan);
        }
    }
    if newer.is_empty() {
        return Ok(false);
    }
    let home = gsd_home(evidence_root).ok_or_else(|| {
        GapPlanCheckFailure::Reader(ReaderFailure::LauncherNotFound {
            home: PathBuf::from("$HOME (unset)"),
        })
    })?;
    for plan in newer {
        match gsd_reader::frontmatter_field(&home, evidence_root, &plan, "gap_closure") {
            Ok(value) if gap_closure_is_true(&value) => return Ok(true),
            Ok(_) => {}
            Err(ReaderFailure::GsdError { message }) if message == GSD_FIELD_NOT_FOUND => {}
            Err(failure) => return Err(GapPlanCheckFailure::Reader(failure)),
        }
    }
    Ok(false)
}

/// GSD's `frontmatter get` error for a field the file does not have
/// (gsd-core 1.20.1, `cmdFrontmatterGet`).
const GSD_FIELD_NOT_FOUND: &str = "Field not found";

/// After a gaps Plan pass (GATE-03, Pitfall 5): the pass must have written a
/// gap-closure plan newer than the verification report, or it counts as a
/// failure and the run gates with `no_gap_closure_plan`. A check that cannot
/// tell gates with `reader_failure`, never silently, and counts nothing. With
/// a new gap plan the run moves to Code, whose pass is gaps-only while the
/// marker is set.
///
/// The counted round goes through [`record_validate_failure`], the one streak
/// update, so it reaches the per-phase ceiling as a Validate failure does
/// (D-12, T10). At the ceiling the gate keeps its kind and says so in its own
/// words, and an approve or reject resets the count through
/// [`reset_phase_failures_at_ceiling`]; an abort resets nothing.
///
/// Both gates are verification gates, answered like every other one through
/// [`approve_route`] and [`reject_route`] (52-29). Approve reads GSD again and
/// follows [`next_step`]'s approve row at Plan: a fresh `gaps_found` reruns the
/// gaps Plan pass with the marker still set. Reject runs `/gsd-quick` with the
/// note, then Validate, and its loop-back clears the gap marker. Abort ends
/// the run.
pub(crate) fn handle_gaps_plan_outcome(
    project_root: &Path,
    state: &mut State,
) -> Result<(), CliError> {
    let evidence_root: PathBuf = state
        .worktree_path
        .clone()
        .unwrap_or_else(|| project_root.to_path_buf());
    match has_new_gap_closure_plan(&evidence_root, state.phase) {
        Ok(true) => transition(project_root, state, Stage::Code),
        Ok(false) => {
            let ceiling_gate = record_validate_failure(project_root, state)?;
            let mut cause = format!(
                "The gaps Plan pass wrote no gap-closure plan (`gap_closure: true`) newer than \
                 the phase's verification report, so there is nothing for the gaps-only pass to \
                 execute. GSD verification: {}.",
                gsd_status_name(GsdStatus::GapsFound)
            );
            if ceiling_gate {
                cause.push_str(&phase_ceiling_clause(state.phase_validate_failures));
            }
            let canary_refusal = recorded_canary_refusal(state);
            let context = lead_with_way_on(
                GateKind::NoGapClosurePlan,
                (state.phase, Stage::Plan),
                canary_refusal.as_deref(),
                &cause,
            );
            open_verification_gate(
                project_root,
                state,
                (
                    Stage::Plan,
                    GateLabel {
                        kind: GateKind::NoGapClosurePlan,
                        gsd_status: Some(GsdStatus::GapsFound),
                        not_before: None,
                    },
                ),
                (&context, &context),
                ceiling_gate,
            )
        }
        Err(failure) => {
            eprintln!(
                "phase {}: the gaps Plan post-check could not read the phase's plans: {}",
                state.phase,
                failure.detail()
            );
            let cause = format!(
                "[never-silent] the gaps Plan post-check could not tell whether a gap-closure \
                 plan was written ({})",
                failure.label()
            );
            let canary_refusal = recorded_canary_refusal(state);
            let lead = |cause: &str| {
                lead_with_way_on(
                    GateKind::ReaderFailure,
                    (state.phase, Stage::Plan),
                    canary_refusal.as_deref(),
                    cause,
                )
            };
            let (context, event_context) = (
                lead(&format!("{cause}: {}.", truncate_reason(&failure.detail()))),
                lead(&format!("{cause}.")),
            );
            open_verification_gate(
                project_root,
                state,
                (Stage::Plan, GateKind::ReaderFailure.into()),
                (&context, &event_context),
                false,
            )
        }
    }
}

/// The clause a counted Plan-pass gate carries when its round brought the
/// phase to the per-phase ceiling (T10): in its own words, never the Validate
/// failure sentence, since this gate did not open over a Validate.
fn phase_ceiling_clause(phase_validate_failures: u32) -> String {
    format!(
        " This phase has counted {phase_validate_failures} failure(s), reaching its per-phase \
         failure ceiling of {}: an answer that continues the run restarts the count, and an \
         answer that ends the run does not.",
        mode::MAX_PHASE_VALIDATE_FAILURES
    )
}

/// Record one counted failure on the counters: the per-phase total, the
/// consecutive streak (999.66, 999.77) and D-34's progress rule. The one streak
/// update: every counted round calls it, a Validate outcome (at most once per
/// outcome) and a gaps Plan pass that wrote no gap-closure plan alike, so each
/// reaches the per-phase ceiling the same way. Returns
/// [`mode::phase_failure_ceiling_reached`] read after the update.
fn record_validate_failure(project_root: &Path, state: &mut State) -> Result<bool, CliError> {
    // 999.78/WR-01 (D-07): the per-phase total accumulates on EVERY recorded
    // failure — including the arm below where the commit count could not be
    // measured, because a failure is a failure whether or not it could be
    // counted. Placed here, once, ahead of the measured/unmeasured split, so
    // neither arm can record a failure without it and neither can record one
    // twice.
    //
    // Saturating, like every other counter on `State`: an unbounded loop that
    // wrapped this to zero would silently restore an exhausted budget, which is
    // the same unreachable-ceiling class of bug 18d fixed for
    // `consecutive_failures`, just slower to show up.
    //
    // Unlike `consecutive_failures` below, nothing in this function ever resets
    // it — a commit count cannot clear it, which is the whole point: the Code
    // stage's fix command is a GSD command that commits `.planning/` artifacts
    // on cycles that changed no source.
    //
    // Measured before the commit: it runs git, which a write-lock closure must
    // not do.
    // D-34 (52-27): only commits that change a path outside `.planning/` are
    // progress, for every Validate failure. A fix or gap-closure pass whose
    // commits are only GSD's `.planning/` artifacts continues the streak.
    let measured_commit_count = agent_result::phase_source_commit_count(
        project_root,
        &devflow_core::config::git_flow_for_project(project_root),
        state.phase,
    );
    commit(state, |s| {
        s.phase_validate_failures = s.phase_validate_failures.saturating_add(1);
        match measured_commit_count {
            Some(current) => {
                if mode::consecutive_failures_made_progress(
                    s.last_validate_failure_commit_count,
                    current,
                ) {
                    // New commits landed since the last recorded failure — this
                    // failure is the first of a new streak, not a continuation.
                    // Set to 1, not 0: the gate context interpolates the counter
                    // into a message naming how many times validation has failed,
                    // and zeroing it would make that message read zero on a real
                    // failure.
                    s.consecutive_failures = 1;
                } else {
                    // Now that the counter genuinely accumulates (18d), an
                    // unbounded loop could otherwise overflow it and wrap to 0,
                    // silently restoring the unreachable-ceiling bug in a slower
                    // form.
                    s.consecutive_failures = s.consecutive_failures.saturating_add(1);
                }
                // The baseline advances on every recorded failure whose count was
                // actually MEASURED, regardless of which branch ran above —
                // updating it only on the progress branch would let a stale low
                // baseline report progress forever.
                s.last_validate_failure_commit_count = Some(current);
            }
            // 999.77 / A-04: the count could not be measured. Treat the cycle as
            // not-progress — an absent measurement is not evidence that work
            // landed — and, crucially, do NOT touch the baseline. Writing a
            // forged zero here is the defect: the next successful measurement
            // would then compare a real count against that zero, read it as
            // forward progress, and hand back one free reset of the
            // MAX_CONSECUTIVE_FAILURES ceiling. Leaving the baseline alone means
            // the next real measurement compares against the last real
            // observation.
            //
            // `mode::consecutive_failures_made_progress` is deliberately not
            // called on this arm and its signature is deliberately not widened —
            // there is no count to compare.
            None => {
                s.consecutive_failures = s.consecutive_failures.saturating_add(1);
            }
        }
    })?;
    Ok(mode::phase_failure_ceiling_reached(
        state.phase_validate_failures,
    ))
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
/// cycle. The reset commits itself (D-08): `transition` and `loop_back_to_code`,
/// which both arms go on to, write only their own fields.
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
/// `loop_back_to_code` and adding a write — is NOT taken, and the reason
/// is specific: `loop_back_to_code` reaches `run_preflight`, which can `abort()`
/// (clearing the phase's state file) and return `Ok(false)`, so a following
/// write would land on an aborted phase. The reset stays ahead of that call.
fn reset_phase_failures_at_ceiling(
    project_root: &Path,
    state: &mut State,
    ceiling_gate: bool,
) -> Result<(), CliError> {
    if !ceiling_gate {
        return Ok(());
    }
    let spent = state.phase_validate_failures;
    commit(state, |s| s.phase_validate_failures = 0)?;
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
    Ok(())
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

/// Handle a non-Validate stage failure (Define/Plan/Code). WR-11: this path must never
/// be silent — it unconditionally fires a gate + notify via [`run_gate`]
/// (independent of `Mode::should_gate`; `run_gate` marks it as an unexpected
/// gate and notifies accordingly), then lets the operator retry, loop back,
/// or abort. Deliberately kept separate from `handle_validate_outcome`: it
/// does not touch `consecutive_failures` and never auto-loops.
/// The characters of a reason kept when it is written into a gate context or
/// an event (GATE-04, 999.206). The reason is stored whole up to this bound, so
/// `devflow gate show` prints what the gate fired on; the bound only stops an
/// unbounded agent output from filling the gate file (at most 16 KiB of UTF-8).
/// Counted in characters, as [`render_gate_context`] counts them. What an
/// operator is shown is cut later, where it is displayed or notified.
pub(crate) const GATE_CONTEXT_STORED_MAX_CHARS: usize = 4096;

/// Bound a failure reason before it enters a gate context or an event: one
/// sanitized line of at most [`GATE_CONTEXT_STORED_MAX_CHARS`] characters.
/// Reasons are agent- or parser-derived and can embed arbitrary output (13-06
/// dogfood finding: a multi-KB raw JSONL line reached the desktop notification
/// verbatim), so the bound stays; the full output stays in
/// `.devflow/phase-NN-stdout`. The banner, `gate list` and the notify hook cut
/// the whole context to their own display bound.
pub(crate) fn truncate_reason(reason: &str) -> String {
    render_gate_context(reason, GATE_CONTEXT_STORED_MAX_CHARS)
}

/// The never-silent gate context for a failed `stage`, with its `reason`
/// stored whole up to [`truncate_reason`]'s bound. `lead` goes before the
/// reason, so it survives the pending-gate banner's and `gate list`'s display
/// bound whatever the reason's length.
fn stage_failure_context(stage: Stage, lead: &str, reason: Option<String>) -> String {
    format!(
        "[never-silent] stage {stage} failed: {lead}{}",
        truncate_reason(&reason.unwrap_or_else(|| "no details available".into()))
    )
}

/// The gate context for a failed stage: the way-on sentence the routers render
/// for a stage-failure gate, then the cause (GATE-04, C2). When the run's
/// recorded delivery canary already refuses its agent, both non-abort answers
/// stop the run instead of relaunching (the routers' relaunch-refused step,
/// [`refuse_relaunch`]), and the sentence, rendered from that same
/// `canary_refusal` input, says so and names `devflow resume --phase N --agent
/// <other>`. It leads, before the reason, because the notify hook carries this
/// context to an operator who decides from it alone, and the banner and
/// `gate list` show only its start.
pub(crate) fn failure_gate_context(state: &State, stage: Stage, reason: Option<String>) -> String {
    let canary_refusal = recorded_canary_refusal(state);
    lead_with_way_on(
        GateKind::StageFailure,
        (state.phase, stage),
        canary_refusal.as_deref(),
        &stage_failure_context(stage, "", reason),
    )
}

/// Fire the never-silent failure gate with [`failure_gate_context`].
pub(crate) fn handle_stage_failure(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let context = failure_gate_context(state, stage, reason);
    answer_stage_failure_gate(project_root, state, stage, &context)
}

/// Fire the never-silent gate for a failed `stage` with `context` and act on
/// its answer through [`dispatch_answer`]: the routers' stage-failure family
/// retries the stage on an approve, relaunches it with the note on a reject,
/// or stops the run when the recorded canary refuses its agent (D-39); an
/// abort ends the run.
fn answer_stage_failure_gate(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    context: &str,
) -> Result<(), CliError> {
    let answer = run_gate(project_root, state, stage, GateKind::StageFailure, context)?;
    dispatch_answer(project_root, state, stage, answer, false)
}

/// Relaunch `stage` for a verification step that reruns a stage (Validate
/// again, or the gaps Plan pass again), unless the run's recorded delivery
/// canary refuses its agent, which [`refuse_relaunch`] reports. The
/// stage-failure family's choice between the two is its routers' (D-39), not
/// this function's.
fn relaunch_or_refuse(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
) -> Result<(), CliError> {
    match recorded_canary_refusal(state) {
        Some(refusal) => refuse_relaunch(project_root, state, stage, &refusal),
        None => launch_stage(state, None, Some(stage)),
    }
}

/// The relaunch-refused step (backlog 999.187): a delivery canary the run
/// already recorded as refusing its agent refuses every relaunch under that
/// agent, and that refusal would clear the supervisor record and end the
/// supervisor with no gate, no event and no cause. So the relaunch is not
/// tried: the run is marked stopped, with a reason naming the refusal and
/// `devflow resume --phase N --agent <other>` as the way on, a
/// `gate_relaunch_refused` event is written, and the supervisor ends. Not a new
/// gate: a supervisor waiting at a gate is live, and `resume` refuses over a
/// live run. All of it is written while the supervisor is still the one the
/// state records.
fn refuse_relaunch(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    refusal: &str,
) -> Result<(), CliError> {
    let phase = state.phase;
    let agent = state.agent;
    let stop_reason = format!(
        "the {stage} stage cannot relaunch under {agent}: the delivery canary recorded for \
         this run refuses it ({refusal}); run `devflow resume --phase {phase} --agent <other>` \
         to continue on another agent"
    );
    commit(state, |s| {
        s.stopped = true;
        s.stop_reason = Some(stop_reason.clone());
        s.gate_pending = false;
    })?;
    events::emit(
        project_root,
        phase,
        "gate_relaunch_refused",
        serde_json::json!({
            "stage": stage.to_string(),
            "agent": agent.to_string(),
            "reason": refusal,
        }),
    );
    tracing::warn!("phase {phase}: {stop_reason}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use devflow_core::gates::Gates;
    // Test-only: production code resolves its branch model from the
    // project (45-01), so the defaults survive here as fixture input only.
    use crate::pipeline_gate::transition;
    use crate::pipeline_launch::advance;
    use crate::test_support::*;
    use devflow_core::config::GitFlowConfig;
    use devflow_core::mode::Mode;
    use devflow_core::state::AgentKind;
    use devflow_core::test_support::{answer_open_gate, gate_answer_files, preanswer_gate};
    use devflow_core::workflow;

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

        // Pre-write an abort response (`abort: true`) so
        // `GateAction::from_response` resolves to `Abort` rather than a
        // loop-back-to-Code.
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: requirements changed","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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

    /// A Validate failure at the threshold, ready for `handle_validate_outcome` to
    /// open the Validate gate.
    fn state_one_failure_from_the_validate_gate(root: &Path, phase: PhaseId) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = mode::MAX_CONSECUTIVE_FAILURES - 1;
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();
        state
    }

    /// The pre-answer seam through the real waiter (`run_gate` and its poll, no
    /// supervisor): the answer written before the gate opens decides that gate, and
    /// no answer outlives it. Control in the same run: a pre-answer for another
    /// stage decides nothing, so that gate times out with the pre-answer still
    /// waiting. Runs in a child so the short gate timeout never touches this process.
    #[test]
    fn a_preanswered_gate_is_decided_by_its_preanswer_through_the_real_waiter() {
        const NAME: &str = "pipeline_outcomes::tests::a_preanswered_gate_is_decided_by_its_preanswer_through_the_real_waiter";
        const ABORT_NOTE: &str = "abort: decided by the pre-answer";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("1"))],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }
        let abort = devflow_core::gates::GateResponse {
            approved: false,
            abort: true,
            note: Some(ABORT_NOTE.to_string()),
            responded_by: Some("test".to_string()),
        };
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let decided = PhaseId::new(4701);
        let mut state = state_one_failure_from_the_validate_gate(root, decided);
        preanswer_gate(root, decided, Stage::Validate, &abort);
        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        let err = workflow::load_state(root, decided).unwrap_err();
        assert!(
            matches!(err, workflow::WorkflowError::MissingState(_)),
            "the abort must clear the state, got {err:?}"
        );
        let last = devflow_core::events::last_event_for_phase(root, decided).unwrap();
        assert_eq!(last["event"], "workflow_aborted", "{last}");
        assert_eq!(last["reason"], ABORT_NOTE, "{last}");
        assert!(
            gate_answer_files(root, decided, Stage::Validate).is_empty(),
            "no answer may outlive the gate it decided"
        );
        assert!(
            !Gates::preanswer_path(root, decided, Stage::Validate).exists(),
            "the gate that opened must have taken the pre-answer"
        );

        let undecided = PhaseId::new(4702);
        let mut state = state_one_failure_from_the_validate_gate(root, undecided);
        preanswer_gate(root, undecided, Stage::Code, &abort);
        let waited = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed)
            .expect_err("a pre-answer for Code must not decide the Validate gate");
        assert!(
            waited.to_string().contains("timed out"),
            "the Validate gate must wait until its timeout, got: {waited}"
        );
        assert!(
            Gates::preanswer_path(root, undecided, Stage::Code).exists(),
            "the Code pre-answer must still be waiting for a Code gate"
        );
        assert!(
            workflow::load_state(root, undecided).is_ok(),
            "the Code pre-answer must not have aborted the run"
        );
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
    /// must never fire from a unit test.
    fn drive_validate_advance_and_read_gate_context(
        root: &Path,
        phase: PhaseId,
        consecutive_failures: u32,
        verdict_json: Option<&str>,
    ) -> String {
        // 999.80 / T-48-08-01, FIRST statement: this helper writes an abort
        // fixture, and a magic substring in a JSON note is the only thing
        // keeping the gate it drives off `launch_stage`. Reword that note or
        // change the response parser and the gate resolves as LoopBack
        // instead, which spawns whatever `claude` the developer has on PATH.
        // Refusing outside a child makes the protection structural rather
        // than a property of the fixture's text.
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "drive_validate_advance_and_read_gate_context must run inside a child test with an agent-free PATH (999.80)"
        );
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

            answer_open_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(
                    r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
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
        // Converted with the helper it calls:
        // `drive_validate_advance_and_read_gate_context` now refuses to run
        // outside a child (999.80).
        const NAME: &str = "pipeline_outcomes::tests::validate_gaps_does_not_advance_to_ship";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // 52-22: GSD's status routes the outcome.
        let _gsd = gsd_gaps_found(root, PhaseId::new(60), 1);
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
        // Converted with the helper it calls:
        // `drive_validate_advance_and_read_gate_context` now refuses to run
        // outside a child (999.80).
        const NAME: &str = "pipeline_outcomes::tests::validate_missing_verdict_does_not_advance";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // 52-22: GSD's status routes the outcome.
        let _gsd = gsd_gaps_found(root, PhaseId::new(61), 1);
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
        // Converted with the helper it calls:
        // `drive_validate_advance_and_read_gate_context` now refuses to run
        // outside a child (999.80).
        const NAME: &str = "pipeline_outcomes::tests::validate_pass_advances";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // 52-22: a pass needs GSD's `passed` too.
        let _gsd = validate_evidence(root, PhaseId::new(62), &[gsd_says(GsdStatus::Passed)]);
        let context = drive_validate_advance_and_read_gate_context(
            root,
            PhaseId::new(62),
            mode::MAX_CONSECUTIVE_FAILURES,
            Some("pass"),
        );
        assert!(
            context.contains("Validation passed"),
            "an explicit pass verdict must reach the passed gate: {context}"
        );
    }

    /// D-18e's "two independent signals agreeing" arm: a probe pass plus an
    /// explicit `verdict: pass` classify as `ValidateOutcome::Passed` and, in
    /// Auto mode with `consecutive_failures == 0`, finish the run through
    /// `finish_run` (D-05): no forced gate, no counter touched, no transition
    /// to Ship. PATH is neutralized under `ENV_MUTEX` (matching
    /// `consecutive_failures_reaches_ceiling_across_cycles`) so a regression
    /// back to a transition could not spawn a real agent CLI.
    #[test]
    fn external_verify_agreement_finishes_the_run() {
        const NAME: &str = "pipeline_outcomes::tests::external_verify_agreement_finishes_the_run";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(90);
        // 52-22: a pass needs GSD's `passed` too; 52-29: the approve reads it
        // again, fresh.
        let passed = gsd_says(GsdStatus::Passed);
        let _gsd = validate_evidence(root, phase, &[passed, passed]);
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
            checkpoint: None,
        };
        let outcome = classify_validate_outcome(&result);
        assert_eq!(outcome, ValidateOutcome::Passed);

        handle_validate_outcome(root, &mut state, outcome).expect("the finish succeeds");

        assert_eq!(state.stage, Stage::Validate, "no transition to Ship");
        assert!(state.finished && state.stopped, "{state:?}");
        assert_eq!(
            state.consecutive_failures, 0,
            "an agreeing outcome must never touch the failure counter"
        );
    }

    /// The negative control for every `CHILD_TEST_ENV` assertion added by this
    /// plan (D-10).
    ///
    /// Counting guarded functions proves only that the keyword is present; it
    /// cannot show the guard FIRES. This calls a guarded helper directly in
    /// the parent — where the developer's real `PATH` is live — and requires
    /// the panic. `expected` is a substring of the helper's own assertion
    /// message, so it cannot be satisfied by an unrelated panic on the way.
    #[test]
    #[should_panic(expected = "must run inside a child test")]
    fn abort_fixture_helper_refuses_to_run_outside_a_child() {
        let dir = tempfile::tempdir().unwrap();
        // Deliberately called from the PARENT, with no child marker set. The
        // arguments are irrelevant: the assertion is this helper's first
        // statement, so it panics before touching any of them.
        let _ = drive_validate_advance_and_read_gate_context(dir.path(), PhaseId::new(1), 0, None);
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
        const NAME: &str =
            "pipeline_outcomes::tests::external_verify_disagreement_gates_immediately";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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
            checkpoint: None,
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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
        const NAME: &str = "pipeline_outcomes::tests::external_verify_no_verdict_gates_immediately";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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
            checkpoint: None,
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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
                checkpoint: None,
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
            checkpoint: None,
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
                                "external verification succeeded but the agent reported gaps"
                                    .to_string(),
                            )
                        }
                        (true, AgentStatus::Success, None) => ValidateOutcome::Ambiguous(
                            "external verification succeeded but no agent verdict arrived"
                                .to_string(),
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
    /// one criterion 4 claims). The `Passed` → finish direction is pinned by
    /// `external_verify_agreement_finishes_the_run`, which runs under a
    /// neutralized `PATH`; reproducing that here would add a child-process
    /// rerun to an otherwise pure test for no additional coverage.
    #[test]
    fn grafted_failure_shape_gates_instead_of_shipping() {
        const NAME: &str =
            "pipeline_outcomes::tests::grafted_failure_shape_gates_instead_of_shipping";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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

        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

        handle_validate_outcome(root, &mut state, post_fix).unwrap();

        assert!(
            !state.finished,
            "the post-fix shape must never finish the run — that finish is the \
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
        const NAME: &str = "pipeline_outcomes::tests::resource_killed_on_code_bumps_infra_failures_not_consecutive_failures";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(73);
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "137").unwrap();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.consecutive_failures = 1;
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
        const NAME: &str = "pipeline_outcomes::tests::resource_killed_on_validate_bumps_infra_not_consecutive_failures";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(74);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.consecutive_failures = 2;
        workflow::save_state(&state).unwrap();

        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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
    /// `Gates::close(.., Stage::Validate)` — which fires on every ordinary
    /// loop-back cycle once `state.stage` is `Validate` and would otherwise
    /// remove a response written only once up front before the final,
    /// gate-triggering cycle ever gets to read it. With it re-seeded every
    /// iteration, the forced gate on the final cycle resolves immediately via
    /// the gate wait finding an already-written file, instead of
    /// waiting out the (default 7-day) gate timeout. PATH is neutralized
    /// under `ENV_MUTEX` so neither `handle_validate_outcome`'s loop-back nor
    /// `transition()`'s own `launch_stage` call risk spawning a real agent
    /// CLI, following `transition_resets_infra_failures`' established
    /// approach.
    #[test]
    fn consecutive_failures_reaches_ceiling_across_cycles() {
        const NAME: &str =
            "pipeline_outcomes::tests::consecutive_failures_reaches_ceiling_across_cycles";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(81);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            preanswer_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(
                    r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
            );
            // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
            let _gsd = gsd_gaps_found(root, state.phase, 1);
            // 52-27: the outcome is handled at Validate, saved, because its
            // gap-closure route (`transition`) moves on from the saved stage.
            state.stage = Stage::Validate;
            workflow::save_state(&state).unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
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
    /// feature branch before every failure (a file outside `.planning/`, which
    /// D-34 counts as progress; `.planning/`-only rounds are
    /// `three_gap_rounds_without_progress_force_the_gate`), must not trip the
    /// ceiling —
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
        const NAME: &str =
            "pipeline_outcomes::tests::healthy_multi_wave_progress_does_not_reach_the_ceiling";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(87);
        init_repo(root);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        for i in 0..(mode::MAX_CONSECUTIVE_FAILURES + 1) {
            // A real new commit BEFORE the failure is recorded, on every
            // cycle — the count observed at each failure strictly exceeds
            // the previously recorded baseline.
            commit_on_feature_branch(root, phase, &format!("wave-{i}"));
            preanswer_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(
                    r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
            );
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
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
        const NAME: &str = "pipeline_outcomes::tests::repeated_failure_without_new_commits_still_reaches_the_ceiling";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            preanswer_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(
                    r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
            );
            // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
            let _gsd = gsd_gaps_found(root, state.phase, 1);
            // 52-27: the outcome is handled at Validate, saved, because its
            // gap-closure route (`transition`) moves on from the saved stage.
            state.stage = Stage::Validate;
            workflow::save_state(&state).unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
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
    /// **A child for cycle 1, a command-local git-only PATH for cycle 2.**
    /// Cycle 1 needs `git` to be UNRESOLVABLE — only a spawn that fails makes
    /// `.output()` return `Err`, which is the sole could-not-measure condition
    /// (F-1); a shim that ran and exited non-zero would be a real observation
    /// and would exercise the already-correct `Some(0)` path (NC-4). The
    /// child owns that empty `PATH`, so no sibling test can observe it. Cycle
    /// 2 needs a real `git` but still no resolvable agent CLI, which is exactly
    /// what a child `Command` with a git-only PATH provides without exposing it
    /// to sibling tests.
    ///
    /// **What this does NOT establish.** A forced-run boundary. `State::new`
    /// zeroes both `consecutive_failures` and the baseline on every `devflow
    /// start --force`; the save/load control below covers the child boundary,
    /// not an entirely new run.
    #[test]
    fn validate_failure_with_unmeasurable_count_accumulates_the_streak() {
        const NAME: &str = "pipeline_outcomes::tests::\
                            validate_failure_with_unmeasurable_count_accumulates_the_streak";
        // Named once and referenced, never written as a literal inside the
        // `var_os(...)` call — matching every other child-test root var in
        // this crate. `doc_check::source_read_env_vars` flags a `DEVFLOW_*`
        // name only when it appears as a string literal directly inside
        // `std::env::var`/`var_os`/`env_value`, and then requires it to be in
        // the scoped operator docs or `doc-check-allowlist.toml`. This is a
        // test fixture root, not an operator-facing knob, so it belongs in
        // neither. Inlining this constant back into the call below re-breaks
        // `doc_check::source_devflow_env_vars_and_subcommands_are_documented`.
        const ROOT_ENV: &str = "DEVFLOW_TEST_AGENT_FREE_ROOT";
        let phase = PhaseId::new(89);

        if devflow_core::test_support::in_child_test(NAME) {
            let root = std::env::var_os(ROOT_ENV)
                .map(std::path::PathBuf::from)
                .expect("agent-free child receives the fixture root");
            let mut state = workflow::load_state(&root, phase).unwrap();
            let _ = handle_validate_outcome(&root, &mut state, ValidateOutcome::Failed);
            workflow::save_state(&state).unwrap();
            return;
        }

        if let Some(root) = child_no_git_root() {
            // CHILD: run exactly the first cycle under an empty PATH, then
            // persist its state for the parent to exercise the real cycle.
            let mut state = workflow::load_state(&root, phase).unwrap();
            let _ = handle_validate_outcome(&root, &mut state, ValidateOutcome::Failed);

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
            workflow::save_state(&state).unwrap();
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
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

        let seed_gate_response = || {
            preanswer_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(
                    r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
            );
        };
        seed_gate_response();

        // CYCLE 1 — the measurement fails in a child whose PATH cannot affect
        // this test or any sibling. The child persists the resulting state.
        let out = run_test_without_git(NAME, root);
        assert_child_ran_exactly_one_passing_test(&out, NAME);

        // The re-assertion is the round-trip control: the child passing is not
        // enough if either field is dropped or defaulted while it is saved.
        let mut state = workflow::load_state(root, phase).unwrap();

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
        workflow::save_state(&state).unwrap();

        // CYCLE 2 — `git` runs again and reports the same real count as
        // before. Nothing was committed in between, so this is not progress.
        let path_dir = agent_free_git_only_path_dir();
        let out = crate::test_support::run_hermetic_test_in_child(
            NAME,
            path_dir.path(),
            &[(ROOT_ENV, root.as_os_str())],
        );
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
        let state = workflow::load_state(root, phase).unwrap();

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
    /// NOT the `"abort": true` cleanup answer the older fixtures use:
    /// `GateAction::from_response` routes that to `GateAction::Abort`, which
    /// clears the phase's state — and "the run stays alive" (D-07) is exactly
    /// what these tests have to observe.
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
    /// therefore unreachable by construction here. Since 52-27 (D-34) only a
    /// commit outside `.planning/` is progress, so each cycle's commit is a
    /// trivial source file: GSD's own `.planning/` artifacts no longer reset
    /// the streak, but a trivial source change still does. The per-phase total
    /// is the only thing that can bound this loop.
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
        const NAME: &str = "pipeline_outcomes::tests::phase_validate_failure_ceiling_gates_despite_trivial_commit_progress";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(90);
        init_repo(root);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();

        for cycle in 1..=mode::MAX_PHASE_VALIDATE_FAILURES {
            // One trivial commit per cycle — the whole premise of 999.78.
            commit_on_feature_branch(root, phase, &format!("trivial-{cycle}"));
            // Re-seeded every iteration: `prepare_loop_back_to_code` cleans up
            // the Validate gate on every ordinary loop-back, which deletes a
            // response written only once up front.
            preanswer_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
            );
            state.stage = Stage::Validate;
            // 52-27: saved, because the gap-closure route moves on from the
            // saved stage.
            workflow::save_state(&state).unwrap();
            // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
            let _gsd = gsd_gaps_found(root, state.phase, 1);
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
        const NAME: &str =
            "pipeline_outcomes::tests::validate_gate_message_leads_with_the_per_phase_total";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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

        let mut first_gate_context = String::new();
        for cycle in 1..=5 {
            commit_on_feature_branch(root, phase, &format!("trivial-{cycle}"));
            preanswer_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
            );
            state.stage = Stage::Validate;
            // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
            let _gsd = gsd_gaps_found(root, state.phase, 1);
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            if cycle == 1 {
                first_gate_context = last_gate_context(root, phase)
                    .expect("Supervise gates on every Validate failure");
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
        const NAME: &str = "pipeline_outcomes::tests::ceiling_clause_appears_only_at_the_ceiling_even_in_supervise_mode";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let real_sh = std::env::var_os("PATH")
                .and_then(|paths| {
                    std::env::split_paths(&paths).find_map(|dir| {
                        let candidate = dir.join("sh");
                        candidate.is_file().then_some(candidate)
                    })
                })
                .expect("sh must be resolvable on the parent PATH");
            std::os::unix::fs::symlink(real_sh, path_dir.path().join("sh"))
                .expect("add sh to the isolated child PATH");
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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

        let ceiling_clause = format!(
            "The per-phase ceiling of {} is reached",
            mode::MAX_PHASE_VALIDATE_FAILURES
        );

        // BELOW the ceiling. This gates — Supervise always does — so the
        // message exists to be inspected.
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
        // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
        let _gsd = gsd_gaps_found(root, state.phase, 1);
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let below = last_gate_context(root, phase).expect("Supervise gates on every Validate");

        // AT the ceiling: one more recorded failure takes the total to the
        // ceiling exactly.
        state.phase_validate_failures = mode::MAX_PHASE_VALIDATE_FAILURES - 1;
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
        // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
        let _gsd = gsd_gaps_found(root, state.phase, 1);
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let at = last_gate_context(root, phase).expect("the ceiling failure must also gate");

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
        const NAME: &str =
            "pipeline_outcomes::tests::loop_back_reason_is_distinct_when_no_commit_baseline_exists";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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

        // GSD's status routes the outcome; `stale` keeps the auto loop back to
        // Code (52-27: `gaps_found` goes through Plan, with no `loop_back`).
        phase_dir_fixture(root, state.phase);
        let _gsd = GsdStub::answering(root, &[gsd_says(GsdStatus::Stale)]);
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
        // GSD's status routes the outcome; `stale` keeps the auto loop back to
        // Code (52-27: `gaps_found` goes through Plan, with no `loop_back`).
        phase_dir_fixture(root, state.phase);
        let _gsd = GsdStub::answering(root, &[gsd_says(GsdStatus::Stale)]);
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let with_baseline =
            devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
                .expect("the second failure also loops back")["reason"]
                .as_str()
                .expect("the loop_back event must carry a reason")
                .to_string();

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
        const NAME: &str = "pipeline_outcomes::tests::phase_validate_failures_reset_on_operator_approval_at_the_ceiling_gate";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

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
        preanswer_gate(
            root,
            at_ceiling_phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
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
        preanswer_gate(
            root,
            below_phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
        let _ = handle_validate_outcome(root, &mut below, ValidateOutcome::Failed);

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
        const NAME: &str =
            "pipeline_outcomes::tests::a_passing_validate_at_the_ceiling_explains_why_it_gated";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        // 52-22: both passes need GSD's `passed` too, one read each.
        let _gsd = validate_evidence(
            root,
            PhaseId::new(88),
            &[gsd_says(GsdStatus::Passed), gsd_says(GsdStatus::Passed)],
        );
        phase_dir_fixture(root, PhaseId::new(89));

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
        preanswer_gate(
            root,
            auto_phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
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
        preanswer_gate(
            root,
            supervise_phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
        let _ = handle_validate_outcome(root, &mut supervise, ValidateOutcome::Passed);

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

    /// The response an operator gives to approve a gate.
    const APPROVE_RESPONSE: &str = r#"{"approved":true,"note":null,"responded_by":"test"}"#;

    /// D-04: in supervise mode a passing Validate ends at a gate, and approving
    /// it finishes the run through `finish_run`: the state is recorded
    /// finished and the stage stays Validate. The gate asks to approve
    /// finishing, not shipping. Run in a child with an agent-free `PATH`, so a
    /// regression back to a transition cannot launch a real agent.
    #[test]
    fn approving_a_passed_validate_gate_finishes_the_run() {
        const NAME: &str =
            "pipeline_outcomes::tests::approving_a_passed_validate_gate_finishes_the_run";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(97);
        // 52-22: a pass needs GSD's `passed` too; 52-29: the approve reads it
        // again, fresh.
        let passed = gsd_says(GsdStatus::Passed);
        let _gsd = validate_evidence(root, phase, &[passed, passed]);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(APPROVE_RESPONSE).unwrap(),
        );

        let outcome = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);

        let context = last_gate_context(root, phase)
            .expect("premise: supervise gates on every Validate, a pass included");
        assert_eq!(
            state.stage,
            Stage::Validate,
            "an approved pass finishes the run; it never transitions to Ship ({outcome:?})"
        );
        assert!(outcome.is_ok(), "the finish succeeds: {outcome:?}");
        let kept = workflow::load_state(root, phase).expect("a finished run keeps its state");
        assert!(kept.finished && kept.stopped, "{kept:?}");
        assert!(
            !context.contains("approve to ship"),
            "the gate no longer asks to ship: {context:?}"
        );
        assert!(
            context.contains("finish"),
            "the gate asks to approve finishing: {context:?}"
        );
    }

    /// D-45 rule 1, the ambiguous arm (52-29): an ambiguous verdict records no
    /// Validate PASS, so approving its gate over GSD's `passed` does not finish
    /// the run: it runs Validate again. 52-08 finished here; `can_finish` now
    /// decides, as 52-22 noted.
    #[test]
    fn approving_an_ambiguous_validate_gate_runs_validate_again() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::approving_an_ambiguous_validate_gate_runs_validate_again",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(98);
        phase_dir_fixture(root, phase);
        let passed = gsd_says(GsdStatus::Passed);
        let _gsd = GsdStub::answering(root, &[passed, passed]);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(APPROVE_RESPONSE).unwrap(),
        );

        let _ = handle_validate_outcome(
            root,
            &mut state,
            ValidateOutcome::Ambiguous(
                "external verification succeeded but no agent verdict arrived".into(),
            ),
        );

        let resolved = events_named(root, phase, "gate_resolved");
        assert_eq!(resolved.len(), 1, "premise: an ambiguous outcome gates");
        assert_eq!(resolved[0]["route"], "validate_again", "{}", resolved[0]);
        assert!(
            events_named(root, phase, "workflow_finished").is_empty(),
            "no finish without a Validate PASS: {:?}",
            phase_events(root, phase)
        );
        assert!(!state.finished, "{state:?}");
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
        const NAME: &str = "pipeline_outcomes::tests::the_ceiling_reset_records_the_total_it_spent";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);

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
        preanswer_gate(
            root,
            at_ceiling_phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
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
        preanswer_gate(
            root,
            below_phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
        let _ = handle_validate_outcome(root, &mut below, ValidateOutcome::Failed);

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
        const NAME: &str = "pipeline_outcomes::tests::phase_validate_failures_increment_saturates";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(94);
        init_repo(root);

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        state.phase_validate_failures = u32::MAX;
        workflow::save_state(&state).unwrap();

        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );

        // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
        let _gsd = gsd_gaps_found(root, state.phase, 1);
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);

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
    /// **An unresolvable `git` is unavoidable here**, unlike the Layer 3 tests
    /// which use an unspawnable working directory: `evaluate_layer2` reads its
    /// exit file from `project_root`, so a non-existent root would make the
    /// exit read fail and return `Ok(None)` for the WRONG reason — an
    /// unreadable exit file rather than an unmeasurable count — and the test
    /// would pass against the unfixed code. The root must EXIST and `git` must
    /// still be unresolvable, and only an emptied `PATH` delivers that
    /// combination.
    ///
    /// That emptied `PATH` lives in a CHILD process
    /// ([`run_test_without_git`], 46-06/46-07, review finding C-01), not in
    /// this one. A process-global emptied `PATH` made every sibling test that
    /// shelled out to `git` inside the window fail spuriously; a child's
    /// `PATH` is invisible to the parent, so there is nothing left to
    /// serialise and this test holds no [`env_lock`]. The fixture is built in
    /// the parent — it needs no `git`, but a parent-side failure is far easier
    /// to attribute than one buried in captured child output.
    ///
    /// `evaluate_layer2` is `pub`, so this drives exactly the same function
    /// with exactly the same inputs; only the binary it runs in differs.
    #[test]
    fn evaluate_layer2_unrunnable_git_falls_through_to_layer3() {
        const NAME: &str = "pipeline_outcomes::tests::\
                            evaluate_layer2_unrunnable_git_falls_through_to_layer3";

        if let Some(root) = child_no_git_root() {
            // CHILD half: `PATH` is an empty directory, so `git` cannot be
            // SPAWNED at all — the sole could-not-measure condition — and the
            // fixture is the one the parent built, reached through the shared
            // root rather than recreated here.
            let result = agent_result::evaluate_layer2(
                &root,
                PhaseId::new(4),
                &GitFlowConfig::default(),
                Stage::Code,
            )
            .unwrap();

            assert!(
                result.is_none(),
                "an unmeasurable commit count must fall through to Layer 3, got: {result:?}"
            );
            // Asserted separately and explicitly, so a future change that
            // returns some other non-`Failed` classification from this layer
            // still has to confront this test rather than slipping past an
            // `is_none()` check.
            assert_ne!(
                result.as_ref().map(|r| r.status),
                Some(devflow_core::agent_result::AgentStatus::Failed),
                "Layer 2 must never classify an unmeasurable count as absent work"
            );
            return;
        }

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

        let out = run_test_without_git(NAME, root);
        assert_child_ran_exactly_one_passing_test(&out, NAME);
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
    /// is this test's NC-4 negative control and vice versa. They cross the
    /// **same** child-process boundary against the **same** fixture shape and
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
        const NAME: &str = "pipeline_outcomes::tests::\
                            evaluate_layer2_unrunnable_git_still_classifies_exit_137_as_resource_killed";

        if let Some(root) = child_no_git_root() {
            // CHILD half: `PATH` is an empty directory, so the commit count is
            // unmeasurable while the exit file the parent wrote is still
            // readable.
            let result = agent_result::evaluate_layer2(
                &root,
                PhaseId::new(4),
                &GitFlowConfig::default(),
                Stage::Code,
            )
            .unwrap();

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
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Identical to the fall-through test's fixture except for the exit
        // code: the root EXISTS and the exit file is readable, so `git` being
        // unrunnable is the only difference from a healthy run.
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, PhaseId::new(4)), "137").unwrap();
        assert!(
            agent_result::exit_code_path(root, PhaseId::new(4)).exists(),
            "the exit file must be readable, or the 137 verdict has no input to come from"
        );

        let out = run_test_without_git(NAME, root);
        assert_child_ran_exactly_one_passing_test(&out, NAME);
    }

    /// CR-01's second, independent harm, in the same shape: a NON-commit-gated
    /// stage that exited 0 never consulted the commit count in the first
    /// place, so an unmeasurable count must not turn its documented `Success`
    /// into `Unknown` → `GateReview`. For `Stage::Validate` that mis-routing
    /// dispatches a cleanly-exited Validate agent as `ValidateOutcome::Failed`.
    ///
    /// The negative control is
    /// `evaluate_layer2_unrunnable_git_falls_through_to_layer3`: same
    /// child-process boundary,
    /// same exit code 0, and the ONLY difference is the stage — `Code` (commit
    /// gated, must fall through) versus `Validate` (not gated, must succeed).
    /// If both stages produced the same answer, the stage scoping would be
    /// doing nothing and neither test would mean anything.
    #[test]
    fn evaluate_layer2_unrunnable_git_keeps_success_for_a_non_commit_gated_stage() {
        const NAME: &str = "pipeline_outcomes::tests::\
                            evaluate_layer2_unrunnable_git_keeps_success_for_a_non_commit_gated_stage";

        if let Some(root) = child_no_git_root() {
            // CHILD half: `PATH` is an empty directory, so the count is
            // unmeasurable — which a stage that never consults it must ignore.
            let result = agent_result::evaluate_layer2(
                &root,
                PhaseId::new(4),
                &GitFlowConfig::default(),
                Stage::Validate,
            )
            .unwrap();

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
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, PhaseId::new(4)), "0").unwrap();
        assert!(
            agent_result::exit_code_path(root, PhaseId::new(4)).exists(),
            "the exit file must be readable, or the Success verdict has no input to come from"
        );

        let out = run_test_without_git(NAME, root);
        assert_child_ran_exactly_one_passing_test(&out, NAME);
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
        const NAME: &str = "pipeline_outcomes::tests::\
                            evaluate_agent_result_with_unrunnable_git_does_not_report_failed";
        let phase = PhaseId::new(96);

        if let Some(root) = child_no_git_root() {
            // CHILD half: `PATH` is an empty directory, so the cascade runs
            // against a real repo it cannot measure. `State` is pure in-memory
            // construction, so it is rebuilt here rather than carried across
            // the boundary.
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Code;

            let result =
                agent_result::evaluate_agent_result(&root, &state, &GitFlowConfig::default())
                    .unwrap();

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
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A real repo with the feature branch present. Built in the PARENT,
        // because building it shells out to git — the ordering constraint the
        // old in-process guard expressed as "before the guard goes on" is now
        // the process boundary itself.
        init_repo(root);
        commit_on_feature_branch(root, phase, "seed");
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(agent_result::exit_code_path(root, phase), "0").unwrap();
        // No stdout capture, so Layer 1 declines and the cascade reaches
        // Layer 2 at all. Read in the parent: it needs no `git`, and a
        // parent-side failure is easier to attribute than one buried in
        // captured child output.
        assert!(
            !agent_result::stdout_path(root, phase).exists(),
            "Layer 1 must decline, or this test never reaches the cascade under study"
        );

        let out = run_test_without_git(NAME, root);
        assert_child_ran_exactly_one_passing_test(&out, NAME);
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
        const NAME: &str = "pipeline_outcomes::tests::evaluate_agent_result_with_real_git_and_empty_branch_still_reports_failed";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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

        let result =
            agent_result::evaluate_agent_result(root, &state, &GitFlowConfig::default()).unwrap();

        assert_eq!(
            result.status,
            devflow_core::agent_result::AgentStatus::Failed,
            "a MEASURED zero on a commit-gated stage is still absent work — the fix must \
             not have widened into 'never report Failed'"
        );
        assert_eq!(result.commits, Some(0));
        assert_eq!(result.decided_by_layer, Some(2));
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
        const NAME: &str =
            "pipeline_outcomes::tests::external_verify_cycles_reach_ceiling_without_unbounded_loop";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }
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
        // 999.80 / T-48-08-01, FIRST statement — see
        // `drive_validate_advance_and_read_gate_context` for why this is
        // structural rather than a property of the fixture text.
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "arm_a_ambiguous_outcome_gates_on_cycle_one must run inside a child test with an agent-free PATH (999.80)"
        );
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
            checkpoint: None,
        };
        let outcome = classify_validate_outcome(&result);
        assert!(matches!(outcome, ValidateOutcome::Ambiguous(_)));

        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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
        // 999.80 / T-48-08-01, FIRST statement — see
        // `drive_validate_advance_and_read_gate_context` for why this is
        // structural rather than a property of the fixture text.
        assert!(
            std::env::var_os(devflow_core::test_support::CHILD_TEST_ENV).is_some(),
            "arm_b_genuine_failures_reach_the_ceiling must run inside a child test with an agent-free PATH (999.80)"
        );
        let _guard = env_lock();

        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
            preanswer_gate(
                root,
                phase,
                Stage::Validate,
                &serde_json::from_str(
                    r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
                )
                .unwrap(),
            );
            // 52-22: GSD's status routes the outcome; `gaps_found` keeps the auto loop.
            let _gsd = gsd_gaps_found(root, state.phase, 1);
            // 52-27: the outcome is handled at Validate, saved, because its
            // gap-closure route (`transition`) moves on from the saved stage.
            state.stage = Stage::Validate;
            workflow::save_state(&state).unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
            state.stage = Stage::Code;
            let _ = transition(root, &mut state, Stage::Validate);
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
        const NAME: &str = "pipeline_outcomes::tests::consecutive_failures_increment_saturates";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

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

        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

        handle_validate_outcome(root, &mut state, ValidateOutcome::Failed).unwrap();

        assert_eq!(state.consecutive_failures, u32::MAX);
    }

    /// CUT-03 (interim): a primary-loop `RateLimited` outcome with no fallback
    /// waits at the never-silent Code gate. At the gate the stage is unchanged
    /// and neither counter moved; no resume is scheduled.
    #[test]
    fn primary_loop_rate_limited_waits_at_the_code_gate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(76);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gate_notify_cmd = Some("true".to_string());
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(
            agent_result::stdout_path(root, phase),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2099-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        let at_gate = state_on_disk_at_gate(root, phase, Stage::Code, || {
            advance(root, Some(phase)).unwrap();
        });

        assert_eq!(at_gate.stage, Stage::Code);
        assert_eq!(at_gate.infra_failures, 0);
        assert_eq!(at_gate.consecutive_failures, 0);
        let events =
            std::fs::read_to_string(devflow_core::events::events_path(root)).unwrap_or_default();
        assert!(events.contains("gate_fired"), "{events}");
        assert!(!events.contains("rate_limit_resume_scheduled"), "{events}");
    }

    /// CR-03: a rate-limit reason whose retry hint is unparseable (e.g. the
    /// `"usage limit"` fallback `detect_claude_rate_limit` produces for a 429
    /// with no retry_after) still fires the never-silent gate and notify
    /// (WR-11/D-15), and its context says no reset time is known rather than
    /// inventing one.
    #[test]
    fn rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently() {
        const NAME: &str = "pipeline_outcomes::tests::rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(81);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        // Pre-seed an Abort response so `run_gate`'s poll resolves immediately.
        preanswer_gate(
            root,
            phase,
            Stage::Code,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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

        assert!(
            events.contains("the denial carried no reset time"),
            "the gate says no reset time is known, and guesses none: {events}"
        );
    }

    /// 13-06 dogfood regression: a multi-KB parser-derived reason reached
    /// the operator's desktop notification verbatim. A stored reason stays
    /// bounded, at the stored bound (GATE-04).
    #[test]
    fn truncate_reason_caps_long_reasons_and_keeps_short_ones() {
        assert_eq!(truncate_reason("short reason"), "short reason");
        let long = "x".repeat(5000);
        let capped = truncate_reason(&long);
        assert_eq!(capped.chars().count(), GATE_CONTEXT_STORED_MAX_CHARS);
        assert!(capped.ends_with("[truncated; full output in .devflow/]"));
    }

    /// Write an idle-timeout record with `commits` long-subject commits and
    /// the given cause keys, then return the reason Layer 1 builds from it.
    fn idle_timeout_reason_with(cause: serde_json::Value, commits: usize) -> String {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        let phase = PhaseId::new(71);
        let record = devflow_core::agent_result::IdleTimeoutRecord {
            status: "idle_timeout".to_string(),
            idle_secs: 120,
            agent_pid: 4242,
            written_at: 1_700_000_000,
            commits: (0..commits)
                .map(|n| devflow_core::agent_result::IdleTimeoutCommit {
                    sha: format!("{n:040x}"),
                    subject: format!("feat: partial work item number {n}"),
                })
                .collect(),
        };
        let mut value = serde_json::to_value(record).unwrap();
        for (key, field) in cause.as_object().unwrap() {
            value[key] = field.clone();
        }
        std::fs::write(
            devflow_core::agent_result::idle_timeout_path(root, phase),
            serde_json::to_string(&value).unwrap(),
        )
        .unwrap();
        devflow_core::agent_result::evaluate_layer1(root, phase)
            .expect("the record must produce a verdict")
            .reason
            .expect("an idle timeout must explain itself")
    }

    /// Whether `text` carries the reason's cause clause opened AND closed.
    /// The fixtures' field text holds no `)`, so the close is the clause's.
    fn holds_whole_clause(text: &str) -> bool {
        text.find("idle timeout (")
            .is_some_and(|open| text[open..].contains("): "))
    }

    /// D-15: the idle-timeout cause survives into what the operator sees —
    /// the stored reason, and the pending-gate banner's and `gate list`'s
    /// rendering of the whole failure gate context at the display bound — even
    /// behind a long commit list, and even at the maximum bound of every cause
    /// field.
    #[test]
    fn an_idle_timeout_gate_context_names_the_last_result_cause() {
        let shown = |reason: &str| {
            let dir = tempfile::tempdir().unwrap();
            let state = State::new(
                PhaseId::new(71),
                AgentKind::Claude,
                Mode::Auto,
                dir.path().to_path_buf(),
            );
            render_gate_context(
                &failure_gate_context(&state, Stage::Validate, Some(reason.to_string())),
                crate::commands::GATE_CONTEXT_DISPLAY_CHARS,
            )
        };
        // (a) the measured 999.102 cause with 40 commits.
        let measured = serde_json::json!({
            "terminal_reason": "aborted_tools",
            "errors": ["[ede_diagnostic] result_type=user last_content_type=n/a stop_reason=tool_use"],
            "origin_kind": "task-notification",
        });
        let reason = idle_timeout_reason_with(measured, 40);
        assert!(
            shown(&reason).contains("aborted_tools"),
            "the cause was truncated away: {}",
            shown(&reason)
        );

        // CONTROL: the same reason with the clause moved to the end loses the
        // cause to the display cut, so the assertion above depends on placement.
        let rest = reason
            .strip_prefix("idle timeout (")
            .expect("the reason opens with the clause");
        let close = rest.find("): ").expect("the clause closes");
        let moved = format!("idle timeout: {} ({})", &rest[close + 3..], &rest[..close]);
        assert!(
            !shown(&moved).contains("aborted_tools"),
            "control is vacuous: a trailing clause also survived the display cut"
        );

        // (b) every cause field at its bound: 160 characters each, 3 errors.
        let max_bound = serde_json::json!({
            "terminal_reason": "t".repeat(160),
            "origin_kind": "o".repeat(160),
            "errors": vec!["e".repeat(160); 3],
        });
        let reason = idle_timeout_reason_with(max_bound, 40);
        assert_eq!(
            truncate_reason(&reason),
            reason,
            "a reason inside the stored bound is stored whole"
        );
        let banner = shown(&reason);
        assert!(
            holds_whole_clause(&banner),
            "the banner cut the clause before it closed: {banner}"
        );
    }

    /// GATE-04 (999.206): a failure cause far longer than the display bound is
    /// stored whole in the failure gate's context, after the way-on sentence.
    #[test]
    fn a_long_failure_reason_is_stored_whole() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::new(
            PhaseId::new(5230),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        let cause = format!("{}END", "a check kept failing; ".repeat(50))
            .chars()
            .take(1000)
            .collect::<String>();
        assert_eq!(cause.chars().count(), 1000);
        let context = failure_gate_context(&state, Stage::Code, Some(cause.clone()));
        assert!(
            context.ends_with(&format!(" stage code failed: {cause}")),
            "the whole cause is stored, last: {context}"
        );
        assert!(
            context.chars().count() > crate::commands::GATE_CONTEXT_DISPLAY_CHARS,
            "control: the context is longer than what the banner shows"
        );
    }

    /// The stored bound's edges, counted in characters, never bytes: a reason
    /// of exactly [`GATE_CONTEXT_STORED_MAX_CHARS`] characters, multi-byte ones
    /// among them, is stored unchanged; one character more, with multi-byte
    /// characters at the cut, is cut at a character boundary to exactly the
    /// bound, ending in the truncation suffix.
    #[test]
    fn a_context_at_the_stored_cap_is_unchanged_and_one_over_is_cut_at_a_char_boundary() {
        const SUFFIX: &str = "… [truncated; full output in .devflow/]";
        let at_cap: String = "aé…"
            .chars()
            .cycle()
            .take(GATE_CONTEXT_STORED_MAX_CHARS)
            .collect();
        assert!(
            at_cap.len() > GATE_CONTEXT_STORED_MAX_CHARS,
            "control: the text is longer in bytes than in characters"
        );
        assert_eq!(truncate_reason(&at_cap), at_cap);

        let one_over = "é".repeat(GATE_CONTEXT_STORED_MAX_CHARS + 1);
        let cut = truncate_reason(&one_over);
        assert_eq!(cut.chars().count(), GATE_CONTEXT_STORED_MAX_CHARS);
        assert!(cut.ends_with(SUFFIX), "the cut must end in the suffix");
        let head = cut.strip_suffix(SUFFIX).unwrap();
        assert_eq!(
            head,
            "é".repeat(GATE_CONTEXT_STORED_MAX_CHARS - SUFFIX.chars().count()),
            "the cut keeps whole characters"
        );
        assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
    }

    /// C2: the failure gate's context leads with the sentence the routers
    /// render for that gate, after the never-silent marker; an empty cause
    /// yields that sentence alone.
    #[test]
    fn the_failure_context_leads_with_the_way_on() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(5231);
        let state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        let sentence = way_on_sentence(GateKind::StageFailure, phase, Stage::Code, None);
        let context = failure_gate_context(&state, Stage::Code, Some("the build broke".into()));
        assert_eq!(
            context,
            format!("[never-silent] {sentence} stage code failed: the build broke")
        );
        assert_eq!(
            lead_with_way_on(GateKind::StageFailure, (phase, Stage::Code), None, ""),
            sentence,
            "an empty cause yields the sentence alone"
        );
        assert_eq!(
            lead_with_way_on(
                GateKind::StageFailure,
                (phase, Stage::Code),
                None,
                "[never-silent] "
            ),
            format!("[never-silent] {sentence}"),
            "a marker with an empty cause yields the marker and the sentence"
        );
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

    /// A Code-stage failure must fire a gate AND run the configured notify
    /// hook — with `DEVFLOW_NON_SILENT_GATE=1` since Auto mode would not
    /// normally gate a Code failure (unexpected/never-silent gate). The
    /// notify sentinel is a side effect distinct from the gate file itself,
    /// so it survives even though `Gates::close` removes the gate/
    /// answer/claim once the gate resolves. This test sets
    /// `DEVFLOW_GATE_NOTIFY_CMD`, so it's serialized under `ENV_MUTEX`.
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_GATE_NOTIFY_CMD` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn non_validate_failure_fires_gate_and_hook() {
        const NAME: &str = "pipeline_outcomes::tests::non_validate_failure_fires_gate_and_hook";
        // Read through a const, never as a literal inside the `var_os` call:
        // `doc_check::source_read_env_vars` flags a `DEVFLOW_*` name only when
        // it appears as a literal there, and would then demand this test
        // fixture appear in the operator docs (see the 48-04 fix in c7d5971).
        const TOUCH_ENV: &str = "DEVFLOW_TEST_TOUCH_BIN";

        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            // This test needs TWO things the git-only PATH does not carry,
            // and they are needed for different reasons.
            //
            // `sh`, on the child's PATH: `gates::run_notify_command` executes
            // the operator's hook as `Command::new("sh").arg("-c")`, so with
            // no resolvable `sh` the spawn fails fail-soft (a `warn!`) and the
            // sentinel silently never appears — the hook would look "not
            // fired" for a reason that has nothing to do with the behaviour
            // under test.
            //
            // `touch`, as an ABSOLUTE path handed to the child: the hook's own
            // command string is looked up against the child's PATH, so passing
            // the resolved binary avoids widening that PATH a second time.
            //
            // Neither weakens the 999.80 guarantee. What must stay
            // unreachable is `claude`/`codex`/`opencode`; a shell and
            // coreutils cannot be mistaken for an agent by `launch_stage`.
            let resolve = |program: &str| {
                std::env::var_os("PATH")
                    .and_then(|paths| {
                        std::env::split_paths(&paths).find_map(|dir| {
                            let candidate = dir.join(program);
                            candidate.is_file().then_some(candidate)
                        })
                    })
                    .unwrap_or_else(|| {
                        panic!("{program} must be resolvable on PATH to run this test")
                    })
            };
            std::os::unix::fs::symlink(resolve("sh"), path_dir.path().join("sh")).unwrap();
            let touch = resolve("touch");
            let out = crate::test_support::run_hermetic_test_in_child(
                NAME,
                path_dir.path(),
                &[(TOUCH_ENV, touch.as_os_str())],
            );
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let _guard = env_lock();

        let touch = std::path::PathBuf::from(
            std::env::var_os(TOUCH_ENV).expect("child test must receive an absolute touch path"),
        );

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
                format!("{} {}", touch.display(), sentinel.display()),
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
        preanswer_gate(
            root,
            phase,
            Stage::Code,
            &serde_json::from_str(
                r#"{"approved":false,"abort":true,"note":"abort: test cleanup","responded_by":"test"}"#,
            )
            .unwrap(),
        );

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

    /// CR-01 regression: after a stage failure's gate resolves and the phase reaches
    /// the same gate again, the SECOND gate's wait must not resolve from the FIRST
    /// gate's consumed answer: the first gate's answer and claim are gone, and the
    /// second gate has its own id.
    #[test]
    fn stage_failure_retry_cleans_stale_response() {
        const NAME: &str = "pipeline_outcomes::tests::stage_failure_retry_cleans_stale_response";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let phase = PhaseId::new(43);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();

        // Pre-answer an abort so the first failure resolves immediately without
        // spawning a monitor.
        preanswer_gate(
            root,
            phase,
            Stage::Code,
            &devflow_core::gates::GateResponse {
                approved: false,
                abort: true,
                note: Some("abort: test cleanup".into()),
                responded_by: Some("test".into()),
            },
        );

        handle_stage_failure(root, &mut state, Stage::Code, Some("first failure".into())).unwrap();

        // abort() must have removed the gate and the answer it consumed.
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
        assert_eq!(
            gate_answer_files(root, phase, Stage::Code),
            Vec::<std::path::PathBuf>::new(),
            "no answer may outlive the gate it decided"
        );
        assert_eq!(
            crate::test_support::gate_claim_files(root, phase, Stage::Code),
            Vec::<std::path::PathBuf>::new()
        );

        // The phase reaching the same gate again waits for a fresh decision.
        let refired = Gates::open_gate(
            root,
            phase,
            Stage::Code,
            devflow_core::gates::GateKind::StageFailure,
            "re-fired gate",
        )
        .unwrap();
        let started = std::time::Instant::now();
        let end = crate::pipeline_launch::wait_for_gate_response(
            root,
            phase,
            Stage::Code,
            &refired.gate_id,
            1,
        )
        .unwrap();
        assert!(
            matches!(end, crate::pipeline_launch::GateWaitEnd::TimedOut),
            "the re-fired gate must wait for its own answer: {end:?}"
        );
        assert!(started.elapsed() >= std::time::Duration::from_secs(1));
    }

    /// A loop-back answered at the ceiling gate persists the budget reset: the reset's
    /// commit and the loop-back's commit both reach disk.
    #[test]
    fn a_ceiling_loop_back_persists_the_reset() {
        const NAME: &str = "pipeline_outcomes::tests::a_ceiling_loop_back_persists_the_reset";
        if !devflow_core::test_support::in_child_test(NAME) {
            let path_dir = agent_free_git_only_path_dir();
            let out = crate::test_support::run_hermetic_test_in_child(NAME, path_dir.path(), &[]);
            devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, NAME);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(97);
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        let phase_dir = worktree
            .join(".planning/phases")
            .join(format!("{padded}-test", padded = phase.padded()));
        std::fs::create_dir_all(&phase_dir).unwrap();

        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Validate;
        state.worktree_path = Some(worktree.clone());
        state.phase_validate_failures = mode::MAX_PHASE_VALIDATE_FAILURES - 1;
        state.verification_baseline_captured = true;
        workflow::save_state(&state).unwrap();
        preanswer_gate(
            root,
            phase,
            Stage::Validate,
            &serde_json::from_str(LOOP_BACK_RESPONSE).unwrap(),
        );
        // 52-22: GSD's status, read in the worktree, routes the outcome.
        let _gsd = GsdStub::answering(&worktree, &[gsd_says(GsdStatus::GapsFound)]);

        // The launch this ends in fails on the agent-free PATH, which is not this
        // test's subject.
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);

        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.stage, Stage::Code, "premise: the run looped back");
        assert_eq!(after.phase_validate_failures, 0, "the reset is on disk");
    }

    /// A state saved as `seed` left it, and the stale copy a handler's caller loaded
    /// before `other_writer` changed the file.
    fn stale_copy_after(
        root: &Path,
        seed: impl FnOnce(&mut State),
        other_writer: impl FnOnce(&mut State),
    ) -> (PhaseId, State) {
        let phase = PhaseId::new(90);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.gate_notify_cmd = Some("true".to_string());
        seed(&mut state);
        workflow::save_state(&state).unwrap();
        let stale = workflow::load_state(root, phase).unwrap();
        workflow::update_state(root, phase, |state| {
            other_writer(state);
            Ok::<(), workflow::WorkflowError>(())
        })
        .unwrap();
        (phase, stale)
    }

    /// D-08: a Validate failure advances the counters on disk, not the caller's copy,
    /// and leaves the field another writer changed. The temp directory has no feature
    /// branch, which measures as zero commits, so the streak accumulates only against
    /// the on-disk baseline of zero; the stale copy's absent baseline would reset it.
    /// `Supervise` gates every Validate, which holds the handler at its gate while
    /// the file is read.
    #[test]
    fn a_validate_failure_commits_its_counters_and_keeps_a_concurrent_field() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, mut stale) = stale_copy_after(
            root,
            |state| {
                state.mode = Mode::Supervise;
                state.stage = Stage::Validate;
                state.consecutive_failures = 1;
                state.phase_validate_failures = 1;
            },
            |state| {
                state.base_branch = Some("workspace/concurrent".to_string());
                state.consecutive_failures = 2;
                state.phase_validate_failures = 2;
                state.last_validate_failure_commit_count = Some(0);
            },
        );

        let on_disk = state_on_disk_at_gate(root, phase, Stage::Validate, || {
            handle_validate_outcome(root, &mut stale, ValidateOutcome::Failed).unwrap();
        });

        assert_eq!(
            on_disk.base_branch.as_deref(),
            Some("workspace/concurrent"),
            "the concurrent write survives"
        );
        assert_eq!(on_disk.consecutive_failures, 3);
        assert_eq!(on_disk.phase_validate_failures, 3);
    }

    /// D-08: an infra outcome increments the counter on disk, not the caller's copy.
    #[test]
    fn an_infra_outcome_increments_the_on_disk_counter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, mut stale) = stale_copy_after(
            root,
            |state| {
                state.stage = Stage::Code;
                state.infra_failures = 1;
            },
            |state| state.infra_failures = 2,
        );

        let on_disk = state_on_disk_at_gate(root, phase, Stage::Code, || {
            handle_infra_outcome(root, &mut stale, Stage::Code, Some("killed".into())).unwrap();
        });

        assert_eq!(on_disk.infra_failures, 3);
    }

    /// D-20: a rate limit with no usable fallback is not an infra failure. One
    /// below the infra ceiling, the outcome opens the Code gate instead of
    /// aborting, and `infra_failures` is where it was. The gate context leads
    /// with the resume command and names the reset time.
    #[test]
    fn a_rate_limit_without_a_fallback_does_not_count_as_an_infra_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let below_ceiling = mode::MAX_INFRA_FAILURES - 1;
        let (phase, mut state) = stale_copy_after(
            root,
            |state| {
                state.stage = Stage::Code;
                state.infra_failures = below_ceiling;
            },
            |_| {},
        );

        let at_gate = state_on_disk_at_gate(root, phase, Stage::Code, || {
            handle_rate_limited_outcome(
                root,
                &mut state,
                phase,
                Stage::Code,
                Some("rate limited until 2099-06-18T15:45:30Z".into()),
            )
            .unwrap();
        });

        assert_eq!(at_gate.infra_failures, below_ceiling);
        let gate_fired: Vec<serde_json::Value> =
            std::fs::read_to_string(devflow_core::events::events_path(root))
                .unwrap_or_default()
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|event| event["event"] == "gate_fired")
                .collect();
        assert_eq!(gate_fired.len(), 1, "{gate_fired:?}");
        let context = gate_fired[0]["context"].as_str().unwrap_or_default();
        let resume = context
            .find(&format!("`devflow resume --phase {phase}`"))
            .expect("the context names the resume command");
        let reset = context
            .find("the limit resets at 2099-06-18T15:45:30Z")
            .expect("the context names the reset time");
        let cause = context
            .find("Cause: rate limited until")
            .expect("the context names the cause");
        assert!(resume < reset && reset < cause, "way on first: {context}");
    }

    #[test]
    fn retry_after_from_reason_strips_prefix() {
        assert_eq!(
            retry_after_from_reason(Some("rate limited until 2026-06-18T15:45:30Z")),
            "2026-06-18T15:45:30Z"
        );
        assert_eq!(retry_after_from_reason(Some("usage limit")), "unknown");
        assert_eq!(retry_after_from_reason(None), "unknown");
    }

    // ---- 52-22: every GSD status and reader failure has its route ----------

    /// Run `name` in a child with an agent-free `PATH` and a 20 s gate timeout,
    /// so a launch fails fast and a gate nobody answers ends instead of
    /// waiting days. `true` means this process is the child: run the body.
    fn in_bounded_child(name: &str) -> bool {
        if devflow_core::test_support::in_child_test(name) {
            return true;
        }
        let path_dir = agent_free_git_only_path_dir();
        let out = crate::test_support::run_hermetic_test_in_child(
            name,
            path_dir.path(),
            &[("DEVFLOW_GATE_TIMEOUT_SECS", std::ffi::OsStr::new("20"))],
        );
        devflow_core::test_support::assert_child_ran_exactly_one_passing_test(&out, name);
        false
    }

    /// A Validate-stage state for `phase` in `mode`, saved, with the commit
    /// baseline seeded so a failure continues rather than restarts a streak.
    fn validate_state(root: &Path, phase: PhaseId, mode: Mode) -> State {
        let mut state = State::new(phase, AgentKind::Claude, mode, root.to_path_buf());
        state.stage = Stage::Validate;
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();
        state
    }

    /// Every event recorded for `phase`, in order.
    fn phase_events(root: &Path, phase: PhaseId) -> Vec<serde_json::Value> {
        std::fs::read_to_string(devflow_core::events::events_path(root))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| phase.matches_json(event.get("phase")))
            .collect()
    }

    /// The events of `phase` named `name`.
    fn events_named(root: &Path, phase: PhaseId, name: &str) -> Vec<serde_json::Value> {
        phase_events(root, phase)
            .into_iter()
            .filter(|event| event["event"] == name)
            .collect()
    }

    /// The position of the first event of `phase` named `name`.
    fn first_position(root: &Path, phase: PhaseId, name: &str) -> Option<usize> {
        phase_events(root, phase)
            .iter()
            .position(|event| event["event"] == name)
    }

    /// An abort answer, as `devflow gate abort` writes it.
    fn abort_answer() -> devflow_core::gates::GateResponse {
        devflow_core::gates::GateResponse {
            approved: false,
            abort: true,
            note: Some("test cleanup".into()),
            responded_by: Some("test".into()),
        }
    }

    /// An approval.
    fn approve_answer() -> devflow_core::gates::GateResponse {
        devflow_core::gates::GateResponse {
            approved: true,
            abort: false,
            note: None,
            responded_by: Some("test".into()),
        }
    }

    /// Run `handler` on another thread until it opens `phase`'s Validate gate;
    /// read the gate file and the state on disk at that moment, answer the gate
    /// with `answer`, and wait for `handler`.
    fn at_the_validate_gate(
        root: &Path,
        phase: PhaseId,
        answer: &devflow_core::gates::GateResponse,
        handler: impl FnOnce() + Send,
    ) -> (serde_json::Value, State) {
        at_the_gate(root, phase, Stage::Validate, answer, handler)
    }

    /// [`at_the_validate_gate`] for the gate at `stage`.
    fn at_the_gate(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        answer: &devflow_core::gates::GateResponse,
        handler: impl FnOnce() + Send,
    ) -> (serde_json::Value, State) {
        const GATE_OPENS_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);
        let gate_path = Gates::gate_path(root, phase, stage);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(handler);
            let started = std::time::Instant::now();
            while !gate_path.exists() {
                assert!(
                    !waiter.is_finished(),
                    "the handler returned without opening the {stage} gate; events: {:?}",
                    phase_events(root, phase)
                );
                assert!(
                    started.elapsed() < GATE_OPENS_WITHIN,
                    "the {stage} gate did not open within {GATE_OPENS_WITHIN:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let gate: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&gate_path).unwrap()).unwrap();
            let on_disk = workflow::load_state(root, phase).unwrap();
            answer_open_gate(root, phase, stage, answer).unwrap();
            waiter.join().unwrap();
            (gate, on_disk)
        })
    }

    /// The `fix` of the last `loop_back` event of `phase`.
    fn last_loop_back_fix(root: &Path, phase: PhaseId) -> Option<String> {
        devflow_core::events::last_event_of_kind_for_phase(root, phase, "loop_back")
            .and_then(|event| event["fix"].as_str().map(str::to_string))
    }

    /// A `stale` or `missing` report sends Code back for the re-verify pass
    /// (D-32 part 1) after either agent verdict in auto mode, with no gate and
    /// no finish. In
    /// supervise mode the same report opens a `reverify_needed` gate, nothing
    /// launches before it is answered, and approving it relaunches Code on the
    /// auto rows' route and finishes nothing.
    fn assert_a_reverify_report_routes_to_execute_phase(status: GsdStatus, first: u32) {
        // Auto, after an agent pass and after an agent fail: the controls,
        // which launch Code with no gate.
        for (offset, outcome) in [ValidateOutcome::Passed, ValidateOutcome::Failed]
            .into_iter()
            .enumerate()
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(first + offset as u32);
            let _gsd = validate_evidence(root, phase, &[gsd_says(status)]);
            let mut state = validate_state(root, phase, Mode::Auto);
            // A gate here would take this abort and fail the assertions below.
            preanswer_gate(root, phase, Stage::Validate, &abort_answer());
            let _ = handle_validate_outcome(root, &mut state, outcome.clone());
            assert!(
                events_named(root, phase, "gate_fired").is_empty(),
                "auto mode relaunches Code over {status:?} without a gate ({outcome:?}): {:?}",
                phase_events(root, phase)
            );
            assert_eq!(
                last_loop_back_fix(root, phase).as_deref(),
                Some("StaleReverify"),
                "{status:?} relaunches the re-verify pass, not --gaps-only ({outcome:?})"
            );
            assert!(events_named(root, phase, "workflow_finished").is_empty());
        }

        // Supervise: a reverify_needed gate, then the auto rows' route.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(first + 2);
        // 52-29: the approve reads GSD fresh, a second read (D-45 rule 2).
        let _gsd = validate_evidence(root, phase, &[gsd_says(status), gsd_says(status)]);
        let mut state = validate_state(root, phase, Mode::Supervise);
        preanswer_gate(root, phase, Stage::Validate, &approve_answer());
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        let fired = events_named(root, phase, "gate_fired");
        assert_eq!(fired.len(), 1, "{:?}", phase_events(root, phase));
        assert_eq!(fired[0]["gate_kind"], "reverify_needed", "{}", fired[0]);
        let resolved = first_position(root, phase, "gate_resolved").expect("the gate was answered");
        for launch in ["loop_back", "stage_launched"] {
            if let Some(at) = first_position(root, phase, launch) {
                assert!(
                    at > resolved,
                    "nothing launches before the reverify_needed gate is answered ({launch}): {:?}",
                    phase_events(root, phase)
                );
            }
        }
        assert_eq!(
            last_loop_back_fix(root, phase).as_deref(),
            Some("StaleReverify"),
            "approving reverify_needed relaunches Code on the auto route"
        );
        assert!(
            events_named(root, phase, "workflow_finished").is_empty(),
            "approving reverify_needed finishes nothing"
        );
    }

    #[test]
    fn a_stale_report_routes_to_execute_phase() {
        if in_bounded_child("pipeline_outcomes::tests::a_stale_report_routes_to_execute_phase") {
            assert_a_reverify_report_routes_to_execute_phase(GsdStatus::Stale, 5221);
        }
    }

    #[test]
    fn a_missing_report_routes_to_execute_phase() {
        if in_bounded_child("pipeline_outcomes::tests::a_missing_report_routes_to_execute_phase") {
            assert_a_reverify_report_routes_to_execute_phase(GsdStatus::Missing, 5224);
        }
    }

    /// T-18-19: an ambiguous Validate verdict gates at once in auto mode over
    /// every GSD status, launches nothing before the gate (no gap loop through
    /// Plan, no re-verify loop-back) and moves no failure counter. Controls in
    /// the same test: an ambiguous verdict over `passed` gates the same way,
    /// and an agent FAIL over `gaps_found` still takes the gap loop with no
    /// gate and counts one failure on each counter.
    #[test]
    fn an_ambiguous_verdict_gates_at_once_in_auto_over_every_status() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::an_ambiguous_verdict_gates_at_once_in_auto_over_every_status",
        ) {
            return;
        }
        let counters = |state: &State| (state.consecutive_failures, state.phase_validate_failures);
        let ambiguous = || ValidateOutcome::Ambiguous("the agent reported gaps".into());
        let rows = [
            (6100, GsdStatus::GapsFound),
            (6101, GsdStatus::Stale),
            (6102, GsdStatus::Missing),
            // Control: the row the table gates on its own.
            (6103, GsdStatus::Passed),
        ];
        for (number, status) in rows {
            let row = format!("ambiguous over {status:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let _gsd = validate_evidence(root, phase, &[gsd_says(status)]);
            let mut state = validate_state(root, phase, Mode::Auto);
            assert_eq!(counters(&state), (0, 0), "{row}: premise");
            // The state on disk is read while the gate is open, before the
            // abort answer ends the run and removes it.
            let (gate, at_gate) = at_the_validate_gate(root, phase, &abort_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, ambiguous());
            });
            assert_eq!(gate["gate_kind"], "ambiguous_verdict", "{row}: {gate}");
            assert_eq!(counters(&at_gate), (0, 0), "{row}: no counter moves");
            let fired: Vec<_> = events_named(root, phase, "gate_fired")
                .into_iter()
                .filter(|event| event["stage"] == "validate")
                .collect();
            assert_eq!(
                fired.len(),
                1,
                "{row}: one Validate gate at once; events: {:?}",
                phase_events(root, phase)
            );
            assert_eq!(fired[0]["gate_kind"], "ambiguous_verdict", "{row}");
            assert_eq!(moves_to_plan(root, phase), 0, "{row}: no gap loop");
            assert!(
                events_named(root, phase, "loop_back").is_empty(),
                "{row}: no re-verify loop-back: {:?}",
                phase_events(root, phase)
            );
            assert!(
                events_named(root, phase, "workflow_finished")
                    .iter()
                    .all(|event| event["reason"] != "finished"),
                "{row}: no finish"
            );
        }

        // Control: an agent FAIL over gaps_found takes the gap loop, ungated,
        // and counts.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(6104);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::GapsFound)]);
        let mut state = validate_state(root, phase, Mode::Auto);
        preanswer_gate(root, phase, Stage::Validate, &abort_answer());
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        assert!(
            events_named(root, phase, "gate_fired")
                .iter()
                .all(|event| event["stage"] != "validate"),
            "control: an agent FAIL over gaps_found opens no Validate gate: {:?}",
            phase_events(root, phase)
        );
        assert_eq!(moves_to_plan(root, phase), 1, "control: the gap loop");
        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(counters(&after), (1, 1), "control: one counted failure");
    }

    /// Write the phase's `{N}-VERIFICATION.md` as GSD's verifier does at the
    /// tail of `/gsd-execute-phase`: during Code, before Validate launches.
    fn verifier_tail_report(evidence_root: &Path, phase: PhaseId) {
        let report = phase_dir_fixture(evidence_root, phase)
            .join(format!("{}-VERIFICATION.md", phase.padded()));
        std::fs::write(&report, "---\nstatus: written-by-code\n---\n").unwrap();
        // D-46 (52-29): the report is committed, as the verifier's own pass
        // commits it, so the evidence root's tree is clean.
        git_in(evidence_root, &["add", "-A", ".planning"]);
        git_in(
            evidence_root,
            &["commit", "-q", "-m", "the verifier's report"],
        );
    }

    /// 999.192: a report the verifier wrote during Code routes by GSD's status,
    /// never by when it was written. `gaps_found` takes the gaps route in auto
    /// mode, and a supervise reject runs `/gsd-quick` (52-29, D-10); `passed`
    /// takes the finish path. No row relaunches the full execute command.
    #[test]
    fn a_verifier_written_report_from_code_routes_by_gsd_status() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_verifier_written_report_from_code_routes_by_gsd_status",
        ) {
            return;
        }
        let no_full_execute = |root: &Path, phase: PhaseId, row: &str| {
            let full = events_named(root, phase, "loop_back")
                .into_iter()
                .filter(|event| event["fix"] == "FullExecute")
                .count();
            assert_eq!(
                full,
                0,
                "{row}: a verifier-written report never relaunches a full-phase execute: {:?}",
                phase_events(root, phase)
            );
        };

        // Auto, agent pass over gaps_found: the gaps loop, no gate.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5240);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::GapsFound)]);
        verifier_tail_report(root, phase);
        let mut state = validate_state(root, phase, Mode::Auto);
        preanswer_gate(root, phase, Stage::Validate, &abort_answer());
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        assert!(
            events_named(root, phase, "gate_fired")
                .iter()
                .all(|event| event["stage"] != "validate"),
            "auto gaps_found leaves Validate without a gate: {:?}",
            phase_events(root, phase)
        );
        // 52-27 (GATE-03): the gaps route is the gap-closure pass through Plan.
        assert!(
            events_named(root, phase, "transition")
                .iter()
                .any(|event| event["from"] == "validate" && event["to"] == "plan"),
            "auto gaps_found takes the gaps route through Plan: {:?}",
            phase_events(root, phase)
        );
        no_full_execute(root, phase, "auto gaps_found");

        // Supervise, the operator rejects the gaps_found gate: since 52-29
        // (GATE-02, D-10) the quick pass with the note, never the full execute.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5241);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::GapsFound)]);
        verifier_tail_report(root, phase);
        let mut state = validate_state(root, phase, Mode::Supervise);
        let reject = devflow_core::gates::GateResponse {
            approved: false,
            abort: false,
            note: Some("re-validate after the review fixes".into()),
            responded_by: Some("test".into()),
        };
        let (gate, _) = at_the_validate_gate(root, phase, &reject, || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        });
        assert_eq!(gate["gate_kind"], "gaps_found", "{gate}");
        assert_eq!(
            last_loop_back_fix(root, phase).as_deref(),
            Some("Quick"),
            "a rejected gaps_found gate runs /gsd-quick with the note: {:?}",
            phase_events(root, phase)
        );
        no_full_execute(root, phase, "supervise reject over gaps_found");

        // Auto, agent pass over passed: the finish path, no loop-back.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5242);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Passed)]);
        verifier_tail_report(root, phase);
        let mut state = validate_state(root, phase, Mode::Auto);
        preanswer_gate(root, phase, Stage::Validate, &abort_answer());
        let outcome = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(state.finished, "passed finishes the run: {state:?}");
        assert_eq!(
            events_named(root, phase, "workflow_finished").len(),
            1,
            "{:?}",
            phase_events(root, phase)
        );
        assert!(
            events_named(root, phase, "loop_back").is_empty(),
            "passed launches nothing: {:?}",
            phase_events(root, phase)
        );
        no_full_execute(root, phase, "auto passed");
    }

    /// `human_needed` after an agent pass gates in auto mode too (D-08), and the
    /// gate names the status.
    #[test]
    fn a_human_needed_report_gates_in_auto_mode() {
        if !in_bounded_child("pipeline_outcomes::tests::a_human_needed_report_gates_in_auto_mode") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5227);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::HumanNeeded)]);
        let mut state = validate_state(root, phase, Mode::Auto);
        let (gate, _) = at_the_validate_gate(root, phase, &abort_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        });
        let context = gate["context"].as_str().unwrap();
        assert!(context.contains("human_needed"), "{context}");
        assert_eq!(gate["gate_kind"], "human_needed", "{gate}");
        assert!(events_named(root, phase, "workflow_finished").is_empty());
    }

    /// An agent fail over a GSD `passed` report is a disagreement: a gate that
    /// names both verdicts, and an approval of it reruns Validate rather than
    /// finishing (D-45 rule 1: an agent fail records no PASS).
    #[test]
    fn an_agent_fail_over_a_passed_report_gates_as_a_disagreement() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::an_agent_fail_over_a_passed_report_gates_as_a_disagreement",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5228);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Passed)]);
        let mut state = validate_state(root, phase, Mode::Auto);
        let (gate, _) = at_the_validate_gate(root, phase, &approve_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        });
        let context = gate["context"].as_str().unwrap();
        assert!(
            context.contains("agent reported fail") && context.contains("passed"),
            "the gate names both verdicts: {context}"
        );
        assert_eq!(gate["gate_kind"], "disagreement_over_passed", "{gate}");
        assert!(
            events_named(root, phase, "workflow_finished").is_empty(),
            "an approved disagreement does not finish: {:?}",
            phase_events(root, phase)
        );
        let kept = workflow::load_state(root, phase).expect("the run keeps its state");
        assert!(!kept.finished, "{kept:?}");
    }

    /// D-24: the reader runs in the worktree evidence root. With the phase
    /// directory only under the worktree, GSD is asked about the worktree's
    /// phase directory, from the worktree. Control in the same test: a read
    /// pointed at the project root finds no phase directory there, and the
    /// stub answers `missing` for the project root's would-be directory.
    #[test]
    fn the_reader_reads_the_worktree_evidence_root() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::the_reader_reads_the_worktree_evidence_root",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let phase = PhaseId::new(5229);
        let worktree = root.join(format!(".worktrees/phase-{phase}"));
        std::fs::create_dir_all(&worktree).unwrap();
        let gsd = validate_evidence(&worktree, phase, &[gsd_says(GsdStatus::GapsFound)]);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.clone());
        state.stage = Stage::Validate;
        state.worktree_path = Some(worktree.clone());
        state.last_validate_failure_commit_count = Some(0);
        workflow::save_state(&state).unwrap();

        let _ = handle_validate_outcome(&root, &mut state, ValidateOutcome::Failed);

        let phase_dir = worktree.join(format!(".planning/phases/{}-fixture", phase.padded()));
        let read = format!("query verification status {}", phase_dir.display());
        assert!(
            gsd.calls()
                .iter()
                .any(|(cwd, argv)| argv == &read && cwd == &worktree),
            "GSD was asked `{read}` from the worktree; calls: {:?}",
            gsd.calls()
        );

        // Control: the project root holds no phase directory.
        assert!(gsd_reader::phase_dir(&root, phase).is_err());
        let beside = gsd_reader::verification_status(
            &gsd.home(),
            &root,
            &root.join(format!(".planning/phases/{}-fixture", phase.padded())),
        )
        .expect("the stub answers");
        assert_eq!(beside.status, GsdStatus::Missing);
    }

    /// D-24: a reader failure at Validate opens a never-silent gate naming the
    /// failure, never finishes, and its events carry only the failure's
    /// path-free label: for no launcher (after an agent pass) and for a
    /// non-zero exit (after an agent fail).
    #[test]
    fn a_reader_failure_at_validate_fires_a_never_silent_gate() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_reader_failure_at_validate_fires_a_never_silent_gate",
        ) {
            return;
        }
        // No launcher: no stub at all.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5230);
        phase_dir_fixture(root, phase);
        repo_with_head(root);
        let mut state = validate_state(root, phase, Mode::Auto);
        let (gate, _) = at_the_validate_gate(root, phase, &abort_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        });
        let context = gate["context"].as_str().unwrap();
        assert!(context.starts_with("[never-silent]"), "{context}");
        assert!(context.contains("gsd_launcher_not_found"), "{context}");
        assert_eq!(gate["gate_kind"], "reader_failure", "{gate}");
        let reads = events_named(root, phase, "gsd_verification");
        assert_eq!(reads.len(), 1, "{reads:?}");
        assert_eq!(
            reads[0]["failure"], "gsd_launcher_not_found",
            "{}",
            reads[0]
        );
        assert!(reads[0].get("status").is_none(), "{}", reads[0]);
        assert!(events_named(root, phase, "workflow_finished").is_empty());

        // A non-zero exit: a stub with no reply exits 97 with a message.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5231);
        let _gsd = validate_evidence(root, phase, &[]);
        let mut state = validate_state(root, phase, Mode::Auto);
        let (gate, _) = at_the_validate_gate(root, phase, &abort_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        });
        let context = gate["context"].as_str().unwrap();
        assert!(context.starts_with("[never-silent]"), "{context}");
        assert!(
            context.contains("gsd_reader_nonzero_exit") && context.contains("no scripted reply"),
            "the gate names the failure and GSD's message: {context}"
        );
        let reads = events_named(root, phase, "gsd_verification");
        assert_eq!(reads.len(), 1, "{reads:?}");
        assert_eq!(
            reads[0]["failure"], "gsd_reader_nonzero_exit",
            "{}",
            reads[0]
        );
        for event in phase_events(root, phase) {
            assert!(
                !event.to_string().contains("no scripted reply"),
                "an event carries GSD's message: {event}"
            );
        }
        assert!(events_named(root, phase, "workflow_finished").is_empty());
    }

    /// Every gate the Validate path opens records its kind in the gate file.
    /// Control in the same test: the agent-pass `passed` gate and the
    /// agent-fail `passed` gate both have stage Validate and quote `passed`,
    /// and their kinds differ.
    #[test]
    fn each_validate_gate_records_its_kind() {
        if !in_bounded_child("pipeline_outcomes::tests::each_validate_gate_records_its_kind") {
            return;
        }
        let rows: [(Mode, ValidateOutcome, Option<GsdStatus>, u32, &str); 9] = [
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::Passed),
                0,
                "clean_pass",
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::GapsFound),
                0,
                "gaps_found",
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::Stale),
                0,
                "reverify_needed",
            ),
            (
                Mode::Auto,
                ValidateOutcome::Passed,
                Some(GsdStatus::HumanNeeded),
                0,
                "human_needed",
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::Passed),
                0,
                "disagreement_over_passed",
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::HumanNeeded),
                0,
                "disagreement_over_human_needed",
            ),
            (
                Mode::Auto,
                ValidateOutcome::Passed,
                None,
                0,
                "reader_failure",
            ),
            (
                Mode::Auto,
                ValidateOutcome::Ambiguous("the agent reported gaps".into()),
                Some(GsdStatus::Passed),
                0,
                "ambiguous_verdict",
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::GapsFound),
                mode::MAX_CONSECUTIVE_FAILURES - 1,
                "failure_cap",
            ),
        ];
        let mut gates = Vec::new();
        for (index, (mode, outcome, status, streak, kind)) in rows.into_iter().enumerate() {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(5240 + index as u32);
            let replies: Vec<&str> = status.into_iter().map(gsd_says).collect();
            let gsd = match status {
                Some(_) => Some(validate_evidence(root, phase, &replies)),
                None => {
                    phase_dir_fixture(root, phase);
                    repo_with_head(root);
                    None
                }
            };
            let mut state = validate_state(root, phase, mode);
            state.consecutive_failures = streak;
            workflow::save_state(&state).unwrap();
            let (gate, _) = at_the_validate_gate(root, phase, &abort_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, outcome.clone());
            });
            drop(gsd);
            assert_eq!(
                gate["gate_kind"], kind,
                "row {index} ({mode:?}, {outcome:?}, {status:?}): {gate}"
            );
            gates.push(gate);
        }
        let (clean, disagreement) = (&gates[0], &gates[4]);
        for gate in [clean, disagreement] {
            assert_eq!(gate["stage"], "validate", "{gate}");
            assert!(
                gate["context"].as_str().unwrap().contains("passed"),
                "{gate}"
            );
        }
        assert_ne!(clean["gate_kind"], disagreement["gate_kind"]);
    }

    /// D45-4: an agent PASS is recorded with the evidence root's HEAD, in the
    /// state on disk, and survives a stop, a transition and a supervisor
    /// takeover's publish write. An agent FAIL, an ambiguous verdict and a HEAD
    /// that cannot be read record `None`; the last also opens a never-silent
    /// gate naming the HEAD read. Control: two PASS rows on two commits record
    /// two different shas.
    #[test]
    fn a_validate_pass_is_recorded_with_the_head_it_ran_on() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_validate_pass_is_recorded_with_the_head_it_ran_on",
        ) {
            return;
        }
        let raw_head = |root: &Path, phase: PhaseId| -> serde_json::Value {
            let raw: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(workflow::state_path(root, phase)).unwrap(),
            )
            .unwrap();
            raw["validate_pass_head"].clone()
        };
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5250);
        let _gsd = validate_evidence(
            root,
            phase,
            &[
                gsd_says(GsdStatus::Passed),
                gsd_says(GsdStatus::GapsFound),
                gsd_says(GsdStatus::Passed),
                gsd_says(GsdStatus::Passed),
                gsd_says(GsdStatus::Passed),
            ],
        );
        let first = repo_with_head(root);

        // An agent PASS over `passed`: auto finishes, and the HEAD is recorded.
        let mut state = validate_state(root, phase, Mode::Auto);
        handle_validate_outcome(root, &mut state, ValidateOutcome::Passed).expect("the finish");
        assert_eq!(
            raw_head(root, phase),
            first,
            "the PASS is recorded with its HEAD"
        );

        // A stop, as `devflow stop` writes it.
        workflow::update_state(root, phase, |s| {
            s.stopped = true;
            s.stop_reason = Some("stopped by devflow stop".into());
            Ok::<_, workflow::WorkflowError>(())
        })
        .unwrap();
        assert_eq!(raw_head(root, phase), first, "a stop keeps the PASS");

        // A transition.
        let mut loaded = workflow::load_state(root, phase).unwrap();
        loaded.stopped = false;
        let _ = transition(root, &mut loaded, Stage::Code);
        assert_eq!(
            workflow::load_state(root, phase).unwrap().stage,
            Stage::Code,
            "premise: the transition was written"
        );
        assert_eq!(raw_head(root, phase), first, "a transition keeps the PASS");

        // A supervisor takeover: a second launch's record published over the
        // first, through the publish's own save.
        let mut taken = workflow::load_state(root, phase).unwrap();
        let next = devflow_core::state::SupervisorRecord::new(
            root.join("next.sock"),
            "nextlaunch0000000000".into(),
            4_294_967_001,
            1,
            String::new(),
        );
        taken.supervisor = Some(next.clone());
        workflow::update_state(root, phase, |s| {
            s.supervisor = Some(devflow_core::state::SupervisorRecord::new(
                root.join("prior.sock"),
                "nextlaunch0000000000".into(),
                4_294_967_000,
                1,
                String::new(),
            ));
            Ok::<_, workflow::WorkflowError>(())
        })
        .unwrap();
        crate::pipeline_launch::save_publish_fields(&taken).expect("the publish save");
        assert_eq!(
            workflow::load_state(root, phase).unwrap().supervisor,
            Some(next),
            "premise: the takeover's record was published"
        );
        assert_eq!(raw_head(root, phase), first, "a takeover keeps the PASS");

        // An agent FAIL on the same HEAD records none (gaps_found: auto loops).
        let mut state = workflow::load_state(root, phase).unwrap();
        state.stage = Stage::Validate;
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        assert_eq!(
            raw_head(root, phase),
            serde_json::Value::Null,
            "a FAIL records no PASS"
        );

        // A PASS again, then an ambiguous verdict: none.
        let mut state = workflow::load_state(root, phase).unwrap();
        state.stage = Stage::Validate;
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        assert_eq!(
            raw_head(root, phase),
            first,
            "premise: a PASS is recorded again"
        );
        let mut state = workflow::load_state(root, phase).unwrap();
        state.stage = Stage::Validate;
        let (_, at_gate) = at_the_validate_gate(root, phase, &abort_answer(), || {
            let _ = handle_validate_outcome(
                root,
                &mut state,
                ValidateOutcome::Ambiguous("the agent reported gaps".into()),
            );
        });
        assert_eq!(
            at_gate.validate_pass_head, None,
            "an ambiguous verdict records no PASS"
        );

        // Control: a PASS on a second commit records the second sha.
        devflow_core::test_support::git_command(root)
            .args(["commit", "-q", "--allow-empty", "-m", "second"])
            .status()
            .unwrap();
        let second = repo_with_head(root);
        assert_ne!(first, second);
        let mut state = validate_state(root, phase, Mode::Auto);
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        assert_eq!(
            raw_head(root, phase),
            second,
            "the field follows HEAD, not a constant"
        );

        // A HEAD that cannot be read: none, and a never-silent gate naming it.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5251);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Passed)]);
        std::fs::remove_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git"), "").unwrap();
        let mut state = validate_state(root, phase, Mode::Auto);
        state.validate_pass_head = Some("a-previous-pass".into());
        workflow::save_state(&state).unwrap();
        let (gate, at_gate) = at_the_validate_gate(root, phase, &abort_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        });
        assert_eq!(
            at_gate.validate_pass_head, None,
            "a failed HEAD read records no PASS"
        );
        let context = gate["context"].as_str().unwrap();
        assert!(
            context.starts_with("[never-silent]") && context.contains("HEAD"),
            "the gate names the HEAD read: {context}"
        );
        assert!(events_named(root, phase, "workflow_finished").is_empty());
    }

    // ---- 52-27 (GATE-03, D-36): the supervise gaps gate and the gaps Plan post-check ----

    /// A supervise Validate state whose GSD stub answers `gaps_found`, with a
    /// git HEAD so an agent PASS is recorded.
    fn supervise_gaps_state(root: &Path, phase: PhaseId) -> (State, GsdStub) {
        let gaps = gsd_says(GsdStatus::GapsFound);
        let gsd = validate_evidence(root, phase, &[gaps, gaps, gaps, gaps]);
        (validate_state(root, phase, Mode::Supervise), gsd)
    }

    /// D-36 / D-45 rule 2 (interim arm): approving a supervise gate whose
    /// `gate_kind` is `gaps_found` runs the gap loop: the run moves to Plan with
    /// the gap-closure marker set, the gaps Plan intent is what Plan launches,
    /// and no `workflow_finished` is recorded. The `--until validate` row does
    /// the same, recording neither `finished` nor `stopped_at`. Control: an
    /// approved gate of kind `human_needed` whose round read `gaps_found` (so a
    /// context that quotes `gaps_found`) does not set the marker and does not
    /// move to Plan.
    #[test]
    fn approving_a_gaps_gate_runs_the_gap_loop() {
        if !in_bounded_child("pipeline_outcomes::tests::approving_a_gaps_gate_runs_the_gap_loop") {
            return;
        }
        for (number, until) in [(5310, None), (5311, Some(Stage::Validate))] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);
            let phase = PhaseId::new(number);
            let (mut state, _gsd) = supervise_gaps_state(root, phase);
            state.stop_until = until;
            workflow::save_state(&state).unwrap();
            let (gate, _) = at_the_validate_gate(root, phase, &approve_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
            });
            let row = format!("until {until:?}");
            assert_eq!(gate["gate_kind"], "gaps_found", "{row}: {gate}");
            let context = gate["context"].as_str().expect("a gate context");
            // GATE-03 holds for how the gate reports GSD's status, the cause
            // after the way-on sentence; the sentence's finish rule names
            // `passed` on every verification gate (U10).
            let sentence = way_on_sentence(GateKind::GapsFound, phase, Stage::Validate, None);
            let cause = context
                .strip_prefix(&sentence)
                .unwrap_or_else(|| panic!("{row}: the gate leads with its way on: {context}"));
            assert!(
                cause.contains("gaps_found") && !cause.contains("passed"),
                "{row}: the gate names GSD's status and never says passed: {context}"
            );
            assert!(
                !context.to_lowercase().contains("approve to finish"),
                "{row}: the gate does not offer finishing: {context}"
            );
            let moved = workflow::load_state(root, phase).unwrap();
            assert_eq!(moved.stage, Stage::Plan, "{row}: approve moves to Plan");
            assert!(
                moved.gap_closure_pass,
                "{row}: the gap-closure marker is set"
            );
            assert!(!moved.finished && !moved.stopped, "{row}: {moved:?}");
            let claude = devflow_core::agents::driver_for(AgentKind::Claude);
            let launched = crate::pipeline_launch::default_stage_prompt(&moved, claude.as_ref());
            assert!(
                launched.contains(&format!("/gsd-plan-phase {phase} --gaps")),
                "{row}: Plan launches the gaps Plan pass: {launched}"
            );
            assert!(
                events_named(root, phase, "workflow_finished").is_empty(),
                "{row}: approving a gaps gate records no workflow_finished: {:?}",
                phase_events(root, phase)
            );
        }

        // Control: the route follows the fresh read, not the kind. The same
        // `gaps_found` gate approved while the fresh read answers
        // `human_needed` reopens a gate labelled `human_needed`, with no gap
        // marker and no gaps Plan launch.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5312);
        let _gsd = validate_evidence(
            root,
            phase,
            &[
                gsd_says(GsdStatus::GapsFound),
                gsd_says(GsdStatus::HumanNeeded),
            ],
        );
        let mut state = validate_state(root, phase, Mode::Supervise);
        let gates = at_the_gates(
            root,
            phase,
            Stage::Validate,
            &[approve_answer(), abort_answer()],
            || {
                let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
            },
        );
        assert_eq!(gates[0]["gate_kind"], "gaps_found", "control: {}", gates[0]);
        assert_eq!(
            gates[1]["gate_kind"], "human_needed",
            "control: the reopened gate is labelled by the fresh read: {}",
            gates[1]
        );
        assert!(
            !events_named(root, phase, "transition")
                .iter()
                .any(|event| event["to"] == "plan"),
            "control: no move to Plan: {:?}",
            phase_events(root, phase)
        );
        assert!(
            !events_named(root, phase, "stage_launched")
                .iter()
                .any(|event| event["stage"] == "plan"),
            "control: no gaps Plan launch"
        );
    }

    /// Run `handler` on another thread; answer each gate it opens at `stage`,
    /// in order, with `answers` (each new gate is told apart by its id), and
    /// wait for `handler`. Returns each gate file as it was when answered.
    fn at_the_gates(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        answers: &[devflow_core::gates::GateResponse],
        handler: impl FnOnce() + Send,
    ) -> Vec<serde_json::Value> {
        at_the_gates_with(root, phase, stage, answers, false, handler)
            .into_iter()
            .map(|(gate, _)| gate)
            .collect()
    }

    /// [`at_the_gates`], returning the state on disk at each gate too. With
    /// `strip_first_kind`, the first gate's file is overwritten with a
    /// 3.x-shaped copy without `gate_kind` before it is answered (C11).
    fn at_the_gates_with(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        answers: &[devflow_core::gates::GateResponse],
        strip_first_kind: bool,
        handler: impl FnOnce() + Send,
    ) -> Vec<(serde_json::Value, State)> {
        const GATE_OPENS_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);
        let gate_path = Gates::gate_path(root, phase, stage);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(handler);
            let mut seen: Vec<(serde_json::Value, State)> = Vec::new();
            for answer in answers {
                let started = std::time::Instant::now();
                let gate = loop {
                    let current = std::fs::read_to_string(&gate_path)
                        .ok()
                        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
                    if let Some(gate) = current
                        && seen
                            .last()
                            .is_none_or(|(last, _)| last["gate_id"] != gate["gate_id"])
                    {
                        break gate;
                    }
                    assert!(
                        !waiter.is_finished(),
                        "the handler returned before gate {} at {stage}; events: {:?}",
                        seen.len() + 1,
                        phase_events(root, phase)
                    );
                    assert!(
                        started.elapsed() < GATE_OPENS_WITHIN,
                        "gate {} at {stage} did not open within {GATE_OPENS_WITHIN:?}",
                        seen.len() + 1
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                };
                let at_gate = workflow::load_state(root, phase).unwrap();
                if strip_first_kind && seen.is_empty() {
                    let mut old = gate.clone();
                    old.as_object_mut().unwrap().remove("gate_kind");
                    std::fs::write(&gate_path, serde_json::to_string_pretty(&old).unwrap())
                        .unwrap();
                }
                answer_open_gate(root, phase, stage, answer).unwrap();
                seen.push((gate, at_gate));
            }
            waiter.join().unwrap();
            seen
        })
    }

    /// D-36: aborting the supervise gaps gate ends the run: the state is
    /// cleared and nothing moves to Plan.
    #[test]
    fn aborting_a_gaps_gate_ends_the_run() {
        if !in_bounded_child("pipeline_outcomes::tests::aborting_a_gaps_gate_ends_the_run") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5313);
        let (mut state, _gsd) = supervise_gaps_state(root, phase);
        let (gate, _) = at_the_validate_gate(root, phase, &abort_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        });
        assert_eq!(gate["gate_kind"], "gaps_found", "{gate}");
        assert!(
            workflow::load_state(root, phase).is_err(),
            "abort clears the run's state"
        );
        assert!(
            !events_named(root, phase, "transition")
                .iter()
                .any(|event| event["to"] == "plan"),
            "abort runs no gap loop: {:?}",
            phase_events(root, phase)
        );
    }

    /// How far apart the post-check fixture's plans and report are, in commit
    /// date and in file modified time: a minute or more, so both comparisons
    /// order them the same way.
    const FIXTURE_SPACING_SECS: i64 = 120;

    /// Write `name` in `phase`'s directory under `root` with `contents`,
    /// commit it with committer date `at` (seconds since the epoch) and set
    /// its modified time to `at`.
    fn write_dated(root: &Path, phase: PhaseId, name: &str, contents: &str, at: i64) {
        let dir = phase_dir_fixture(root, phase);
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let date = format!("@{at} +0000");
        for args in [
            vec!["add", relative.as_str()],
            vec!["commit", "-q", "-m", name],
        ] {
            let output = devflow_core::test_support::git_command(root)
                .args(&args)
                .env("GIT_COMMITTER_DATE", &date)
                .env("GIT_AUTHOR_DATE", &date)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?}: {output:?}");
        }
        let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(at as u64);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    /// A plan's text, with `gap_closure: true` in its frontmatter when `gap`.
    fn plan_text(phase: PhaseId, plan: &str, gap: bool) -> String {
        let padded = phase.padded();
        let gap = if gap { "gap_closure: true\n" } else { "" };
        format!(
            "---\nphase: {padded}-fixture\nplan: {plan}\ntype: execute\nautonomous: true\n{gap}---\n\n<tasks>\n<task type=\"auto\"><name>stub</name></task>\n</tasks>\n"
        )
    }

    /// A gaps Plan state at Plan for `phase` under `root`, saved, with the
    /// phase's first plan and, a spacing later, its verification report.
    /// Returns the state and the report's time.
    fn gaps_plan_fixture(root: &Path, phase: PhaseId, earlier_gap_plan: bool) -> (State, i64) {
        init_repo(root);
        let report_at = 1_900_000_000;
        let padded = phase.padded();
        write_dated(
            root,
            phase,
            &format!("{padded}-01-PLAN.md"),
            &plan_text(phase, "01", false),
            report_at - 2 * FIXTURE_SPACING_SECS,
        );
        if earlier_gap_plan {
            // An earlier round's gap plan, written before this round's report.
            write_dated(
                root,
                phase,
                &format!("{padded}-02-PLAN.md"),
                &plan_text(phase, "02", true),
                report_at - FIXTURE_SPACING_SECS,
            );
        }
        write_dated(
            root,
            phase,
            &format!("{padded}-VERIFICATION.md"),
            "---\nstatus: gaps_found\n---\n",
            report_at,
        );
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Plan;
        state.gap_closure_pass = true;
        workflow::save_state(&state).unwrap();
        (state, report_at)
    }

    /// Run the gaps Plan outcome of `state` until its Plan gate opens, answer it
    /// with an abort, and return the gate file.
    fn gaps_plan_gate(root: &Path, state: &mut State) -> serde_json::Value {
        let phase = state.phase;
        let (gate, _) = at_the_gate(root, phase, Stage::Plan, &abort_answer(), || {
            let _ = handle_gaps_plan_outcome(root, state);
        });
        gate
    }

    /// Pitfall 5 (GATE-03, U6): after a gaps Plan pass, a plan with
    /// `gap_closure: true` newer than the verification report must exist, or
    /// the pass gates with `no_gap_closure_plan`; a reader failure on that read
    /// gates with `reader_failure`, never silently. Rows: no new plan; an
    /// earlier round's gap plan that predates the report and no newer one; a
    /// newer plan whose read fails. Control: the same tree plus a gap plan
    /// written after the report proceeds to Code, whose pass is gaps-only.
    /// The check compares file modified times; the fixture spaces commit dates
    /// and file times alike, so a commit-order check would order them the same.
    #[test]
    fn a_gaps_plan_pass_without_a_new_gap_closure_plan_gates() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_gaps_plan_pass_without_a_new_gap_closure_plan_gates",
        ) {
            return;
        }
        // No plan newer than the report: no GSD read is needed, and none is
        // scripted, so a read would surface as a reader failure instead.
        for (number, earlier_gap_plan) in [(5320, false), (5321, true)] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let (mut state, _) = gaps_plan_fixture(root, phase, earlier_gap_plan);
            let _gsd = GsdStub::answering(root, &[]);
            let gate = gaps_plan_gate(root, &mut state);
            assert_eq!(
                gate["gate_kind"], "no_gap_closure_plan",
                "earlier gap plan {earlier_gap_plan}: {gate}"
            );
            assert!(
                events_named(root, phase, "transition").is_empty(),
                "earlier gap plan {earlier_gap_plan}: no Code launch: {:?}",
                phase_events(root, phase)
            );
        }

        // A newer plan whose read fails: a never-silent reader-failure gate.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5322);
        let (mut state, report_at) = gaps_plan_fixture(root, phase, true);
        write_dated(
            root,
            phase,
            &format!("{}-03-PLAN.md", phase.padded()),
            &plan_text(phase, "03", true),
            report_at + FIXTURE_SPACING_SECS,
        );
        let _gsd = GsdStub::answering(root, &["not json"]);
        let gate = gaps_plan_gate(root, &mut state);
        assert_eq!(gate["gate_kind"], "reader_failure", "{gate}");
        assert!(
            gate["context"]
                .as_str()
                .is_some_and(|context| context.starts_with("[never-silent]")),
            "a reader failure is never silent: {gate}"
        );

        // Control: the earlier gap plan, plus one written after the report.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5323);
        let (mut state, report_at) = gaps_plan_fixture(root, phase, true);
        let newer = format!("{}-03-PLAN.md", phase.padded());
        write_dated(
            root,
            phase,
            &newer,
            &plan_text(phase, "03", true),
            report_at + FIXTURE_SPACING_SECS,
        );
        let gsd = GsdStub::answering(root, &[r#"{"gap_closure": true}"#]);
        let _ = handle_gaps_plan_outcome(root, &mut state);
        assert!(
            events_named(root, phase, "gate_fired").is_empty(),
            "control: a new gap plan opens no gate: {:?}",
            phase_events(root, phase)
        );
        let moved = workflow::load_state(root, phase).unwrap();
        assert_eq!(moved.stage, Stage::Code, "control: the run moves to Code");
        assert!(moved.gap_closure_pass, "control: the marker stays set");
        let claude = devflow_core::agents::driver_for(AgentKind::Claude);
        let code = crate::pipeline_launch::default_stage_prompt(&moved, claude.as_ref());
        assert!(
            code.contains(&format!("/gsd-execute-phase {phase} --gaps-only")),
            "control: Code launches the gaps-only pass: {code}"
        );
        let reads: Vec<String> = gsd.calls().into_iter().map(|(_, argv)| argv).collect();
        assert_eq!(
            reads.len(),
            1,
            "control: only the plan newer than the report is read: {reads:?}"
        );
        assert!(
            reads[0].ends_with(&format!("{newer} --field gap_closure")),
            "control: the newer plan's gap_closure is read: {reads:?}"
        );
    }

    // ---- 52-27 (D-12, D-25, D-34): gap rounds keep the streak ----

    /// Land one commit on `phase`'s feature branch that changes only
    /// `.planning/` (a gap round's GSD artifacts), or also `src/<label>.rs` when
    /// `source`.
    fn commit_gap_round(root: &Path, phase: PhaseId, label: &str, source: bool) {
        let git = |args: &[&str]| {
            let output = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?}: {output:?}");
        };
        let branch = format!("feature/phase-{}", phase.padded());
        let exists = devflow_core::test_support::git_command(root)
            .args(["rev-parse", "--verify", &branch])
            .output()
            .is_ok_and(|output| output.status.success());
        if exists {
            git(&["checkout", "-q", &branch]);
        } else {
            git(&["checkout", "-q", "-b", &branch]);
        }
        let planning = root.join(".planning/rounds");
        std::fs::create_dir_all(&planning).unwrap();
        std::fs::write(planning.join(format!("{label}.md")), label).unwrap();
        git(&["add", ".planning/rounds"]);
        if source {
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::write(root.join(format!("src/{label}.rs")), label).unwrap();
            git(&["add", "src"]);
        }
        git(&["commit", "-q", "-m", label]);
    }

    /// One auto gap round at `root`: the round's commit, then a Validate
    /// outcome `outcome` over GSD's `gaps_found`, handled at Validate. The
    /// Validate gate is pre-answered with an abort in case the round gates.
    fn auto_gap_round(
        root: &Path,
        state: &mut State,
        label: &str,
        source: bool,
        outcome: ValidateOutcome,
    ) {
        commit_gap_round(root, state.phase, label, source);
        state.stage = Stage::Validate;
        workflow::save_state(state).unwrap();
        preanswer_gate(root, state.phase, Stage::Validate, &abort_answer());
        let _gsd = GsdStub::answering(root, &[gsd_says(GsdStatus::GapsFound)]);
        let _ = handle_validate_outcome(root, state, outcome);
    }

    /// The moves to Plan recorded for `phase`: one per gap-closure pass begun.
    fn moves_to_plan(root: &Path, phase: PhaseId) -> usize {
        events_named(root, phase, "transition")
            .iter()
            .filter(|event| event["from"] == "validate" && event["to"] == "plan")
            .count()
    }

    /// D-12, D-25, D-34 (U2), T-52-37: three auto gap rounds whose commits
    /// touch only `.planning/` reach `MAX_CONSECUTIVE_FAILURES` and gate, with
    /// the Validate agent failing every round and with it passing every round
    /// over GSD's `gaps_found`. Each round moves `consecutive_failures` and
    /// `phase_validate_failures` by exactly one, the gate opens after the third
    /// round, and no fourth gaps Plan launches.
    #[test]
    fn three_gap_rounds_without_progress_force_the_gate() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::three_gap_rounds_without_progress_force_the_gate",
        ) {
            return;
        }
        for (number, verdict) in [(5330, "fail"), (5331, "pass")] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);
            let phase = PhaseId::new(number);
            phase_dir_fixture(root, phase);
            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
            state.stage = Stage::Validate;
            workflow::save_state(&state).unwrap();
            for round in 1..=mode::MAX_CONSECUTIVE_FAILURES {
                let outcome = match verdict {
                    "fail" => ValidateOutcome::Failed,
                    _ => ValidateOutcome::Passed,
                };
                auto_gap_round(root, &mut state, &format!("round-{round}"), false, outcome);
                assert_eq!(
                    (state.consecutive_failures, state.phase_validate_failures),
                    (round, round),
                    "agent {verdict}, round {round}: each round counts one failure on each \
                     counter; events: {:?}",
                    phase_events(root, phase)
                );
                let gated = events_named(root, phase, "gate_fired")
                    .iter()
                    .any(|event| event["stage"] == "validate");
                assert_eq!(
                    gated,
                    round == mode::MAX_CONSECUTIVE_FAILURES,
                    "agent {verdict}, round {round}: the gate opens after the third round only"
                );
            }
            let gate = events_named(root, phase, "gate_fired");
            assert_eq!(
                gate.last().map(|event| event["gate_kind"].clone()),
                Some(serde_json::json!("failure_cap")),
                "agent {verdict}: the third round gates at the cap: {gate:?}"
            );
            assert_eq!(
                moves_to_plan(root, phase),
                (mode::MAX_CONSECUTIVE_FAILURES - 1) as usize,
                "agent {verdict}: no gaps Plan after the round that gated"
            );
        }
    }

    /// D-34 control: a gap round that also changes a source file counts as
    /// progress, so the streak starts again at 1 while the per-phase total
    /// keeps counting.
    #[test]
    fn a_gap_round_that_changes_a_source_file_resets_the_streak() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_gap_round_that_changes_a_source_file_resets_the_streak",
        ) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5332);
        phase_dir_fixture(root, phase);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();
        auto_gap_round(root, &mut state, "round-1", false, ValidateOutcome::Failed);
        auto_gap_round(root, &mut state, "round-2", false, ValidateOutcome::Failed);
        assert_eq!(
            state.consecutive_failures, 2,
            "premise: two planning-only rounds build a streak"
        );
        auto_gap_round(root, &mut state, "round-3", true, ValidateOutcome::Failed);
        assert_eq!(
            (state.consecutive_failures, state.phase_validate_failures),
            (1, 3),
            "a round that changes a source file restarts the streak; the total keeps counting"
        );
        assert!(
            events_named(root, phase, "gate_fired").is_empty(),
            "no gate: the streak was reset: {:?}",
            phase_events(root, phase)
        );
    }

    /// D-12 (U2), supervise counting: with GSD answering `gaps_found` at every
    /// read and gap rounds that commit only `.planning/`, the first Validate
    /// outcome's gate opens with each counter at exactly 1 on disk, approving it
    /// starts the gaps Plan with each still 1, and the second round's gate opens
    /// with each at 2, with the agent failing every round and with it passing
    /// every round. Control: the same supervise run with GSD answering `passed`
    /// and the agent passing reaches its gate with both counters at 0.
    #[test]
    fn a_supervise_gaps_round_counts_once_and_its_approve_counts_nothing() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_supervise_gaps_round_counts_once_and_its_approve_counts_nothing",
        ) {
            return;
        }
        let counters = |state: &State| (state.consecutive_failures, state.phase_validate_failures);
        for (number, outcome) in [
            (5333, ValidateOutcome::Failed),
            (5334, ValidateOutcome::Passed),
        ] {
            let row = format!("{outcome:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            init_repo(root);
            let phase = PhaseId::new(number);
            phase_dir_fixture(root, phase);
            // 52-29: three reads: round 1, the approve's fresh read (D-45
            // rule 2), round 2.
            let _gsd = GsdStub::answering(
                root,
                &[
                    gsd_says(GsdStatus::GapsFound),
                    gsd_says(GsdStatus::GapsFound),
                    gsd_says(GsdStatus::GapsFound),
                ],
            );
            let mut state = State::new(
                phase,
                AgentKind::Claude,
                Mode::Supervise,
                root.to_path_buf(),
            );
            state.stage = Stage::Validate;
            workflow::save_state(&state).unwrap();

            commit_gap_round(root, phase, "round-1", false);
            let first = outcome.clone();
            let (gate, at_gate) = at_the_validate_gate(root, phase, &approve_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, first);
            });
            assert_eq!(gate["gate_kind"], "gaps_found", "{row}: {gate}");
            assert_eq!(counters(&at_gate), (1, 1), "{row}: round 1 counts once");
            let after_approve = workflow::load_state(root, phase).unwrap();
            assert_eq!(
                after_approve.stage,
                Stage::Plan,
                "{row}: approve starts the gaps Plan"
            );
            assert_eq!(
                counters(&after_approve),
                (1, 1),
                "{row}: the approve counts nothing"
            );

            commit_gap_round(root, phase, "round-2", false);
            let mut state = after_approve;
            state.stage = Stage::Validate;
            workflow::save_state(&state).unwrap();
            let second = outcome.clone();
            let (_, at_second) = at_the_validate_gate(root, phase, &abort_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, second);
            });
            assert_eq!(
                counters(&at_second),
                (2, 2),
                "{row}: round 2 counts once more"
            );
        }

        // Control: GSD's `passed` and an agent pass count nothing.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5335);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Passed)]);
        let mut state = validate_state(root, phase, Mode::Supervise);
        let (gate, at_gate) = at_the_validate_gate(root, phase, &abort_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        });
        assert_eq!(gate["gate_kind"], "clean_pass", "control: {gate}");
        assert_eq!(
            counters(&at_gate),
            (0, 0),
            "control: a passed round counts nothing"
        );
    }

    // ---- D-32 part 1: the re-verify pass for a `stale` or `missing` report ----

    /// The Code prompt a launch of `state` with no override receives, as the
    /// Claude driver renders it: the prompt a resume, a failover or a re-drive
    /// of the stage gets (`default_stage_prompt` is the only renderer of one).
    fn claude_launch_prompt(state: &State) -> String {
        let driver = devflow_core::agents::driver_for(AgentKind::Claude);
        crate::pipeline_launch::default_stage_prompt(state, driver.as_ref())
    }

    /// Whether `prompt` is the re-verify pass over GSD's `status`: it quotes
    /// the status and the route `execute-phase`, asks for the verifier, and
    /// does not run the gaps-only command.
    fn is_reverify_prompt(prompt: &str, status: &str) -> bool {
        prompt.contains(&format!("`{status}`"))
            && prompt.contains("`execute-phase`")
            && prompt.contains("re-run the verifier")
            && !prompt.contains("--gaps-only")
    }

    /// The verification-status reads the GSD stub has answered so far.
    fn status_reads(gsd: &GsdStub) -> usize {
        gsd.calls()
            .iter()
            .filter(|(_, argv)| argv.contains("verification status"))
            .count()
    }

    /// D-32 part 1, D-45 rule 3: an auto run whose Validate agent passes over
    /// GSD's `stale` relaunches Code with the re-verify pass; when that Code
    /// pass ends (advance's Code success arm, `transition` to Validate) GSD is
    /// not read before Validate launches, and the next Validate's fresh read
    /// decides: `passed` finishes the run. Control in the same test: GSD still
    /// answering `stale` at the second Validate takes the re-verify pass again
    /// and records no `workflow_finished`.
    #[test]
    fn a_stale_validate_runs_the_reverify_pass_then_validate() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_stale_validate_runs_the_reverify_pass_then_validate",
        ) {
            return;
        }
        for (number, second, finishes) in [
            (5340, GsdStatus::Passed, true),
            (5341, GsdStatus::Stale, false),
        ] {
            let row = format!("second read {second:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let gsd =
                validate_evidence(root, phase, &[gsd_says(GsdStatus::Stale), gsd_says(second)]);
            let mut state = validate_state(root, phase, Mode::Auto);
            // A gate here would take this abort and fail the assertions below.
            preanswer_gate(root, phase, Stage::Validate, &abort_answer());
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);

            let at_code = workflow::load_state(root, phase).unwrap();
            assert_eq!(at_code.stage, Stage::Code, "{row}: stale relaunches Code");
            assert_eq!(
                at_code.code_fix,
                Some(FixType::StaleReverify),
                "{row}: the Code pass is the re-verify pass; events: {:?}",
                phase_events(root, phase)
            );
            assert_eq!(
                last_loop_back_fix(root, phase).as_deref(),
                Some("StaleReverify"),
                "{row}"
            );
            let prompt = claude_launch_prompt(&at_code);
            assert!(
                is_reverify_prompt(&prompt, "stale"),
                "{row}: the Code launch carries the re-verify prompt: {prompt}"
            );

            // The Code pass ends: advance's Code success arm.
            let reads_at_code = status_reads(&gsd);
            assert_eq!(reads_at_code, 1, "{row}: one read, at the first Validate");
            let mut state = at_code;
            let _ = transition(root, &mut state, Stage::Validate);
            assert_eq!(
                workflow::load_state(root, phase).unwrap().stage,
                Stage::Validate,
                "{row}: the re-verify pass ends at Validate like any Code pass"
            );
            assert_eq!(
                status_reads(&gsd),
                reads_at_code,
                "{row}: no GSD status read between the end of Code and Validate"
            );

            let mut state = workflow::load_state(root, phase).unwrap();
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
            assert_eq!(
                status_reads(&gsd),
                2,
                "{row}: the second Validate reads once"
            );
            let finished = events_named(root, phase, "workflow_finished");
            if finishes {
                assert_eq!(
                    finished.len(),
                    1,
                    "{row}: the fresh `passed` read finishes the run: {:?}",
                    phase_events(root, phase)
                );
            } else {
                assert!(
                    finished.is_empty(),
                    "{row}: a run never finishes over `stale`: {finished:?}"
                );
                let again = workflow::load_state(root, phase).unwrap();
                assert_eq!(again.stage, Stage::Code, "{row}");
                assert_eq!(again.code_fix, Some(FixType::StaleReverify), "{row}");
                assert_eq!(
                    events_named(root, phase, "loop_back").len(),
                    2,
                    "{row}: the re-verify pass runs again"
                );
            }
        }
    }

    /// D-08, D-12 (U2): a Validate whose fresh status is `stale` or
    /// `gaps_found` counts exactly one Validate failure, whatever the agent
    /// said: each row starts from both counts at 0 and ends with both at 1, on
    /// its route (the re-verify pass, or 52-27's gap loop through Plan). Three
    /// auto rounds over `stale` whose re-verify passes commit only `.planning/`
    /// reach the consecutive-failure gate, and no third Code pass launches.
    /// Control in the same test: an agent pass over `passed` finishes with
    /// both counts unchanged.
    #[test]
    fn a_validate_that_takes_the_reverify_route_counts_one_failure() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_validate_that_takes_the_reverify_route_counts_one_failure",
        ) {
            return;
        }
        let counters = |state: &State| (state.consecutive_failures, state.phase_validate_failures);
        let rows = [
            (5342, ValidateOutcome::Passed, GsdStatus::Stale),
            (5343, ValidateOutcome::Failed, GsdStatus::Stale),
            (5344, ValidateOutcome::Failed, GsdStatus::GapsFound),
            (5345, ValidateOutcome::Passed, GsdStatus::GapsFound),
        ];
        for (number, outcome, status) in rows {
            let row = format!("{outcome:?} over {status:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let _gsd = validate_evidence(root, phase, &[gsd_says(status)]);
            let mut state = validate_state(root, phase, Mode::Auto);
            assert_eq!(counters(&state), (0, 0), "{row}: premise");
            preanswer_gate(root, phase, Stage::Validate, &abort_answer());
            let _ = handle_validate_outcome(root, &mut state, outcome);
            let after = workflow::load_state(root, phase).unwrap();
            assert_eq!(
                counters(&after),
                (1, 1),
                "{row}: one round counts once on each counter; events: {:?}",
                phase_events(root, phase)
            );
            match status {
                GsdStatus::Stale => {
                    assert_eq!(after.code_fix, Some(FixType::StaleReverify), "{row}");
                    assert_eq!(moves_to_plan(root, phase), 0, "{row}");
                }
                _ => {
                    assert_eq!(moves_to_plan(root, phase), 1, "{row}: the gap loop");
                    assert!(after.gap_closure_pass, "{row}: the gap loop's marker");
                    assert!(
                        events_named(root, phase, "loop_back").is_empty(),
                        "{row}: no Code loop-back"
                    );
                }
            }
        }

        // Three unproductive re-verify rounds reach the consecutive-failure gate.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5346);
        phase_dir_fixture(root, phase);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        workflow::save_state(&state).unwrap();
        for round in 1..=mode::MAX_CONSECUTIVE_FAILURES {
            commit_gap_round(root, phase, &format!("reverify-{round}"), false);
            state.stage = Stage::Validate;
            workflow::save_state(&state).unwrap();
            preanswer_gate(root, phase, Stage::Validate, &abort_answer());
            let _gsd = GsdStub::answering(root, &[gsd_says(GsdStatus::Stale)]);
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
            assert_eq!(
                counters(&state),
                (round, round),
                "round {round}: each re-verify round counts one failure; events: {:?}",
                phase_events(root, phase)
            );
        }
        let gates = events_named(root, phase, "gate_fired");
        assert_eq!(
            gates.last().map(|event| event["gate_kind"].clone()),
            Some(serde_json::json!("failure_cap")),
            "the third round gates at the cap: {gates:?}"
        );
        let loop_backs = events_named(root, phase, "loop_back");
        assert_eq!(
            loop_backs.len(),
            (mode::MAX_CONSECUTIVE_FAILURES - 1) as usize,
            "no Code pass after the round that gated: {loop_backs:?}"
        );
        assert!(
            loop_backs
                .iter()
                .all(|event| event["fix"] == "StaleReverify"),
            "{loop_backs:?}"
        );

        // Control: an agent pass over `passed` finishes and counts nothing.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5347);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Passed)]);
        let mut state = validate_state(root, phase, Mode::Auto);
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        assert_eq!(
            events_named(root, phase, "workflow_finished").len(),
            1,
            "control: a passed round finishes"
        );
        assert_eq!(
            counters(&workflow::load_state(root, phase).unwrap()),
            (0, 0),
            "control: a passed round counts nothing"
        );
    }

    /// R6 (review 2): the re-verify route taken inside a gap-closure pass
    /// sets `code_fix` to the re-verify pass and clears the gap marker, so a
    /// stop and resume relaunches the re-verify pass, not `--gaps-only`.
    /// Control in the same test: the same marker-set state at Code, stopped
    /// before any Validate, relaunches `--gaps-only`.
    #[test]
    fn a_reverify_route_clears_the_gap_marker() {
        if !in_bounded_child("pipeline_outcomes::tests::a_reverify_route_clears_the_gap_marker") {
            return;
        }
        let stop = |root: &Path, phase: PhaseId| {
            let mut stopped = workflow::load_state(root, phase).unwrap();
            stopped.stopped = true;
            stopped.stop_reason = Some("stopped via `devflow stop`".to_string());
            workflow::save_state(&stopped).unwrap();
            workflow::load_state(root, phase).unwrap()
        };

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5348);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Stale)]);
        let mut state = validate_state(root, phase, Mode::Auto);
        state.gap_closure_pass = true;
        workflow::save_state(&state).unwrap();
        preanswer_gate(root, phase, Stage::Validate, &abort_answer());
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(after.code_fix, Some(FixType::StaleReverify));
        assert!(
            !after.gap_closure_pass,
            "the re-verify route clears the gap marker"
        );
        let resumed = claude_launch_prompt(&stop(root, phase));
        assert!(
            is_reverify_prompt(&resumed, "stale"),
            "a resumed re-verify pass is the re-verify pass: {resumed}"
        );

        // Control: a marker-set gaps Code pass stopped before any Validate.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5349);
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state.gap_closure_pass = true;
        workflow::save_state(&state).unwrap();
        let resumed = claude_launch_prompt(&stop(root, phase));
        assert!(
            resumed.contains("--gaps-only"),
            "control: a stopped gaps Code pass resumes gaps-only: {resumed}"
        );
    }

    /// Route I, T10 (D-12): a counted Plan-pass gate reaches the per-phase
    /// ceiling through the same streak update a Validate failure takes. From
    /// one below the ceiling, a gaps Plan pass with no new gap-closure plan
    /// opens its `no_gap_closure_plan` gate with the count at the ceiling, its
    /// context names this phase's per-phase failure ceiling in its own words
    /// (never the Validate sentence `Validation has failed N time(s)`), and an
    /// approve resets the count with one `phase_failure_budget_reset` event.
    /// The abort row resets nothing. Control in the same test: from 5, the
    /// same pass counts one (6), names no ceiling, and its approve leaves the
    /// count at 6 with no reset event.
    #[test]
    fn a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered",
        ) {
            return;
        }
        let ceiling = mode::MAX_PHASE_VALIDATE_FAILURES;
        let rows = [
            (5350, ceiling - 1, approve_answer(), true),
            (5351, ceiling - 1, abort_answer(), true),
            (5352, 5, approve_answer(), false),
        ];
        for (number, before, answer, at_ceiling) in rows {
            let row = format!(
                "from {before}, {}",
                if answer.abort { "abort" } else { "approve" }
            );
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let (mut state, _) = gaps_plan_fixture(root, phase, false);
            state.phase_validate_failures = before;
            workflow::save_state(&state).unwrap();
            // 52-29: an approve reads GSD fresh (D-45 rule 2); `human_needed`
            // reopens an uncounted gate, which the writer aborts.
            let _gsd = GsdStub::answering(root, &[gsd_says(GsdStatus::HumanNeeded)]);
            let mut answers = vec![answer.clone()];
            if !answer.abort {
                answers.push(abort_answer());
            }
            let gates = at_the_gates_with(root, phase, Stage::Plan, &answers, false, || {
                let _ = handle_gaps_plan_outcome(root, &mut state);
            });
            let (gate, at_gate) = gates[0].clone();
            assert_eq!(gate["gate_kind"], "no_gap_closure_plan", "{row}: {gate}");
            assert_eq!(
                at_gate.phase_validate_failures,
                before + 1,
                "{row}: the gate's round counts one failure; events: {:?}",
                phase_events(root, phase)
            );
            let context = gate["context"].as_str().unwrap_or_default();
            assert_eq!(
                context.contains("per-phase failure ceiling"),
                at_ceiling,
                "{row}: the context names the ceiling exactly when it is reached: {context}"
            );
            assert!(
                !context.contains("Validation has failed"),
                "{row}: a Plan-pass gate is not worded as a Validate failure: {context}"
            );
            let resets = events_named(root, phase, "phase_failure_budget_reset");
            let continues_at_ceiling = at_ceiling && !answer.abort;
            assert_eq!(
                resets.len(),
                usize::from(continues_at_ceiling),
                "{row}: one reset exactly when an answer continues the run at the ceiling: \
                 {resets:?}"
            );
            if !answer.abort {
                assert_eq!(gates[1].0["gate_kind"], "human_needed", "{row}: reopened");
            }
            // The abort removes the state file, so the counts are read from
            // the state the call held, and the resets from the events.
            let expected = match (answer.abort, at_ceiling) {
                (false, true) => 0,
                _ => before + 1,
            };
            assert_eq!(state.phase_validate_failures, expected, "{row}");
            if continues_at_ceiling {
                assert_eq!(
                    resets[0]["phase_validate_failures_before"], ceiling,
                    "{row}: {resets:?}"
                );
            }
        }
    }

    // ---- 52-29 (D-45 rules 1, 2; D-46): one finish check, one routing table ----

    /// Run git in `root` and return its trimmed stdout, asserting success.
    fn git_in(root: &Path, args: &[&str]) -> String {
        let output = devflow_core::test_support::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(output.status.success(), "git {args:?} failed: {output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// D-45 rule 1, D-46: `can_finish` gives a permit only over GSD's `passed`
    /// with the recorded Validate PASS head equal to the evidence root's HEAD
    /// over a clean tree. The first row (clean tree, equal heads, `passed`) is
    /// the control that gives a permit; each dirty row differs from it only in
    /// the tree, and names the dirty path or the failed read.
    #[test]
    fn can_finish_needs_passed_and_a_pass_on_the_current_head() {
        enum Tree {
            Clean,
            Untracked,
            UntrackedHiddenByConfig,
            ModifiedPlanning,
            UnreadableIndex,
        }
        enum Recorded {
            Head,
            Other,
            None,
        }
        // (status reply or None for a reader failure, recorded head, tree,
        // HEAD readable, permit expected, text the evidence must name)
        let passed = Some(gsd_says(GsdStatus::Passed));
        type Row<'a> = (
            &'a str,
            Option<&'a str>,
            Recorded,
            Tree,
            bool,
            bool,
            Option<&'a str>,
        );
        let rows: Vec<Row> = vec![
            (
                "control",
                passed,
                Recorded::Head,
                Tree::Clean,
                true,
                true,
                None,
            ),
            (
                "other head",
                passed,
                Recorded::Other,
                Tree::Clean,
                true,
                false,
                None,
            ),
            (
                "no record",
                passed,
                Recorded::None,
                Tree::Clean,
                true,
                false,
                None,
            ),
            (
                "HEAD unreadable",
                passed,
                Recorded::Other,
                Tree::Clean,
                false,
                false,
                Some("HEAD"),
            ),
            (
                "untracked",
                passed,
                Recorded::Head,
                Tree::Untracked,
                true,
                false,
                Some("loose.txt"),
            ),
            (
                "untracked, showUntrackedFiles=no",
                passed,
                Recorded::Head,
                Tree::UntrackedHiddenByConfig,
                true,
                false,
                Some("loose.txt"),
            ),
            (
                "modified .planning",
                passed,
                Recorded::Head,
                Tree::ModifiedPlanning,
                true,
                false,
                Some(".planning/notes.md"),
            ),
            (
                "tree read failed",
                passed,
                Recorded::Head,
                Tree::UnreadableIndex,
                true,
                false,
                Some("could not be read"),
            ),
            (
                "gaps_found",
                Some(gsd_says(GsdStatus::GapsFound)),
                Recorded::Head,
                Tree::Clean,
                true,
                false,
                None,
            ),
            (
                "human_needed",
                Some(gsd_says(GsdStatus::HumanNeeded)),
                Recorded::Head,
                Tree::Clean,
                true,
                false,
                None,
            ),
            (
                "stale",
                Some(gsd_says(GsdStatus::Stale)),
                Recorded::Head,
                Tree::Clean,
                true,
                false,
                None,
            ),
            (
                "missing",
                Some(gsd_says(GsdStatus::Missing)),
                Recorded::Head,
                Tree::Clean,
                true,
                false,
                None,
            ),
            (
                "reader failure",
                None,
                Recorded::Head,
                Tree::Clean,
                true,
                false,
                None,
            ),
        ];
        for (number, (row, reply, recorded, tree, head_readable, permit, names)) in
            (5400u32..).zip(rows)
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            phase_dir_fixture(root, phase);
            let _gsd = reply.map(|reply| GsdStub::answering(root, &[reply]));
            git_in(root, &["init", "-q"]);
            git_in(root, &["config", "user.email", "devflow@example.com"]);
            git_in(root, &["config", "user.name", "DevFlow Tests"]);
            git_in(root, &["config", "commit.gpgsign", "false"]);
            let head = if head_readable {
                std::fs::write(root.join(".planning/notes.md"), "notes\n").unwrap();
                git_in(root, &["add", ".planning/notes.md"]);
                git_in(root, &["commit", "-q", "-m", "fixture"]);
                git_in(root, &["rev-parse", "HEAD"])
            } else {
                // No commit and no file: HEAD names nothing, so its read
                // fails, over a tree the read finds clean.
                String::new()
            };
            match tree {
                Tree::Clean => {}
                Tree::Untracked => std::fs::write(root.join("loose.txt"), "x").unwrap(),
                Tree::UntrackedHiddenByConfig => {
                    git_in(root, &["config", "status.showUntrackedFiles", "no"]);
                    std::fs::write(root.join("loose.txt"), "x").unwrap();
                    let plain = git_in(root, &["status", "--porcelain"]);
                    assert!(
                        plain.is_empty(),
                        "{row}: premise: a plain porcelain read hides the file: {plain:?}"
                    );
                }
                Tree::ModifiedPlanning => {
                    std::fs::write(root.join(".planning/notes.md"), "changed\n").unwrap()
                }
                Tree::UnreadableIndex => {
                    std::fs::write(root.join(".git/index"), "not an index").unwrap()
                }
            }
            let recorded_head = match recorded {
                Recorded::Head => Some(head.clone()),
                Recorded::Other => Some("0".repeat(40)),
                Recorded::None => None,
            };
            let evidence = read_evidence(root, phase, recorded_head.as_deref());
            assert_eq!(
                can_finish(&evidence).is_some(),
                permit,
                "{row}: permit expected {permit}; evidence {evidence:?}"
            );
            if let Some(names) = names {
                let cause = evidence.no_pass_cause.as_deref().unwrap_or_default();
                assert!(
                    cause.contains(names) && cause.starts_with("[never-silent]"),
                    "{row}: the evidence names {names:?} never-silently: {evidence:?}"
                );
            }
        }
    }

    /// U4 (D-45 rule 2): a supervise Validate whose agent passes while GSD
    /// answers `stale` opens a `reverify_needed` gate, launches nothing before
    /// an answer and counts one failure on each counter; approving it while the
    /// fresh read is still `stale` relaunches Code with the re-verify pass and
    /// records no `workflow_finished`. Control in the same test: the same gate
    /// approved while the fresh read is `passed`, with the PASS on HEAD over a
    /// clean tree, finishes, so the route follows the fresh read and not the
    /// `reverify_needed` label.
    #[test]
    fn a_supervise_stale_validate_gates_and_approve_reverifies() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::a_supervise_stale_validate_gates_and_approve_reverifies",
        ) {
            return;
        }
        for (number, fresh, finishes) in [
            (5420, GsdStatus::Stale, false),
            (5421, GsdStatus::Passed, true),
        ] {
            let row = format!("fresh read {fresh:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let _gsd =
                validate_evidence(root, phase, &[gsd_says(GsdStatus::Stale), gsd_says(fresh)]);
            let mut state = validate_state(root, phase, Mode::Supervise);
            let (gate, at_gate) = at_the_validate_gate(root, phase, &approve_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
            });
            assert_eq!(gate["gate_kind"], "reverify_needed", "{row}: {gate}");
            assert_eq!(
                at_gate.stage,
                Stage::Validate,
                "{row}: nothing launched before an answer"
            );
            assert_eq!(
                at_gate.code_fix, None,
                "{row}: no fix pass before an answer"
            );
            assert_eq!(
                (
                    at_gate.consecutive_failures,
                    at_gate.phase_validate_failures
                ),
                (1, 1),
                "{row}: the stale round counts once on each counter"
            );
            let finished = events_named(root, phase, "workflow_finished");
            let after = workflow::load_state(root, phase).unwrap();
            if finishes {
                assert_eq!(
                    finished.len(),
                    1,
                    "{row}: a fresh `passed` with the PASS on HEAD finishes: {:?}",
                    phase_events(root, phase)
                );
                assert!(after.finished, "{row}: {after:?}");
            } else {
                assert!(
                    finished.is_empty(),
                    "{row}: no finish over `stale`: {finished:?}"
                );
                assert_eq!(after.stage, Stage::Code, "{row}: approve launches Code");
                assert_eq!(after.code_fix, Some(FixType::StaleReverify), "{row}");
                let prompt = claude_launch_prompt(&after);
                assert!(
                    prompt.contains("re-run the verifier"),
                    "{row}: the Code launch is the re-verify pass: {prompt}"
                );
            }
        }
    }

    /// D-45 rule 2, D-08: a Validate outcome reads GSD's status once, and that
    /// one read picks the label, the count and the route. Each row's stub gives
    /// one status at its first read and a different one at any later read.
    /// Control in the same test: approving the supervise row's gate logs
    /// exactly one more read and takes the gap loop over its `gaps_found`, so
    /// the log counts reads and the later answer is what a second read sees.
    #[test]
    fn a_validate_outcome_reads_gsd_status_once() {
        if !in_bounded_child("pipeline_outcomes::tests::a_validate_outcome_reads_gsd_status_once") {
            return;
        }
        let counters = |state: &State| (state.consecutive_failures, state.phase_validate_failures);

        // Auto, agent PASS, `stale` then `passed`: the re-verify pass.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5430);
        let gsd = validate_evidence(
            root,
            phase,
            &[gsd_says(GsdStatus::Stale), gsd_says(GsdStatus::Passed)],
        );
        let mut state = validate_state(root, phase, Mode::Auto);
        preanswer_gate(root, phase, Stage::Validate, &abort_answer());
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(status_reads(&gsd), 1, "auto stale: one read");
        assert_eq!(counters(&after), (1, 1), "auto stale: counted once");
        assert_eq!(
            after.code_fix,
            Some(FixType::StaleReverify),
            "auto stale: re-verify"
        );
        assert!(events_named(root, phase, "workflow_finished").is_empty());

        // Auto, agent FAIL, `gaps_found` then `passed`: the gaps Plan launch.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5431);
        let gsd = validate_evidence(
            root,
            phase,
            &[gsd_says(GsdStatus::GapsFound), gsd_says(GsdStatus::Passed)],
        );
        let mut state = validate_state(root, phase, Mode::Auto);
        preanswer_gate(root, phase, Stage::Validate, &abort_answer());
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        let after = workflow::load_state(root, phase).unwrap();
        assert_eq!(status_reads(&gsd), 1, "auto gaps: one read");
        assert_eq!(counters(&after), (1, 1), "auto gaps: counted once");
        assert_eq!(after.stage, Stage::Plan, "auto gaps: the gaps Plan launch");
        assert!(after.gap_closure_pass, "auto gaps: the gap marker is set");

        // Supervise, agent PASS, `passed` then `gaps_found`: a clean_pass gate.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5432);
        let gsd = validate_evidence(
            root,
            phase,
            &[gsd_says(GsdStatus::Passed), gsd_says(GsdStatus::GapsFound)],
        );
        let mut state = validate_state(root, phase, Mode::Supervise);
        let (gate, at_gate) = at_the_validate_gate(root, phase, &approve_answer(), || {
            let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
        });
        assert_eq!(gate["gate_kind"], "clean_pass", "supervise: {gate}");
        assert_eq!(
            counters(&at_gate),
            (0, 0),
            "supervise: a clean pass counts nothing"
        );
        // Control: the approve reads once more and takes the gap loop.
        assert_eq!(
            status_reads(&gsd),
            2,
            "control: the approve logs exactly one more read; events: {:?}",
            phase_events(root, phase)
        );
        let after = workflow::load_state(root, phase).unwrap();
        assert!(
            events_named(root, phase, "workflow_finished").is_empty(),
            "control: the fresh `gaps_found` does not finish"
        );
        assert_eq!(after.stage, Stage::Plan, "control: the gap loop: {after:?}");
        assert!(after.gap_closure_pass, "control: the gap marker is set");
    }

    // ---- 52-33 Task 3 (GATE-04, W3, D-39, C2): one way on per gate ---------

    /// The answer words no text after a gate's way-on sentence may hold. The
    /// removed Ship answer is split with `concat!` so the removed-surface scan
    /// (52-18) does not read it as a surviving literal.
    const ANSWER_WORDS: [&str; 7] = [
        "approve",
        "reject",
        "abort",
        "retry",
        concat!("sh", "ip"),
        "loop back",
        "loop-to-code",
    ];

    /// `Ok` when `context` starts with `sentence`, after a leading
    /// `[never-silent]` marker if it has one, and the text after the sentence
    /// names no answer; otherwise why not.
    fn leads_with_only(context: &str, sentence: &str) -> Result<(), String> {
        let body = context.strip_prefix("[never-silent] ").unwrap_or(context);
        let rest = body
            .strip_prefix(sentence)
            .ok_or_else(|| format!("the context does not start with the sentence: {context}"))?;
        let lower = rest.to_lowercase();
        match ANSWER_WORDS.iter().find(|word| lower.contains(*word)) {
            Some(word) => Err(format!(
                "the text after the sentence names `{word}`: {rest}"
            )),
            None => Ok(()),
        }
    }

    /// GATE-04, W3, D-39, C2: every gate writer leads its context with the
    /// sentence `way_on_sentence` renders for that gate from `approve_route`
    /// and `reject_route`, after any `[never-silent]` marker, and names no
    /// answer after it. Rows: each verification writer (the Validate outcome's
    /// kinds, both ceiling clauses, the two reopens and the two gaps Plan
    /// gates), the stage-failure gate at Plan, Code and Validate, the preflight
    /// gate and the checkpoint-declaration gate. A verification gate's approve
    /// clause states the finish rule (U10), so a `clean_pass` gate over
    /// `passed` with the PASS on HEAD and a `gaps_found` gate carry the same
    /// one. Canary rows: under a refusing delivery canary the stage-failure
    /// gate leads with the relaunch-refused route at every stage. Planted
    /// controls fail the same check: the old failure-cap and preflight answer
    /// texts after the sentence, the sentence after the cause, an approve
    /// clause that says approve finishes, and the admitting-canary sentence on
    /// a refusing run.
    #[test]
    fn every_gate_names_only_the_routes_its_answers_run() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::every_gate_names_only_the_routes_its_answers_run",
        ) {
            return;
        }
        let verification_sentence =
            |phase: PhaseId| way_on_sentence(GateKind::CleanPass, phase, Stage::Validate, None);
        let check = |row: &str, context: &str, sentence: &str| {
            if let Err(why) = leads_with_only(context, sentence) {
                panic!("{row}: {why}");
            }
        };
        let mut verification_contexts: Vec<(String, String)> = Vec::new();

        // The Validate outcome's verification writers.
        // (mode, outcome, GSD status, kind, streak, per-phase total)
        type WriterRow = (
            Mode,
            ValidateOutcome,
            Option<GsdStatus>,
            &'static str,
            u32,
            u32,
        );
        let rows: [WriterRow; 11] = [
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::Passed),
                "clean_pass",
                0,
                0,
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::GapsFound),
                "gaps_found",
                0,
                0,
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::Stale),
                "reverify_needed",
                0,
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Passed,
                Some(GsdStatus::HumanNeeded),
                "human_needed",
                0,
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::Passed),
                "disagreement_over_passed",
                0,
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::HumanNeeded),
                "disagreement_over_human_needed",
                0,
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Ambiguous("no verdict".into()),
                Some(GsdStatus::Passed),
                "ambiguous_verdict",
                0,
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::GapsFound),
                "failure_cap",
                mode::MAX_CONSECUTIVE_FAILURES - 1,
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Passed,
                None,
                "reader_failure",
                0,
                0,
            ),
            // The two ceiling clauses: a clean pass at the ceiling, and a
            // counted failure that reaches it.
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::Passed),
                "failure_cap",
                0,
                mode::MAX_PHASE_VALIDATE_FAILURES,
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Failed,
                Some(GsdStatus::Passed),
                "disagreement_over_passed",
                0,
                mode::MAX_PHASE_VALIDATE_FAILURES - 1,
            ),
        ];
        for (index, (mode, outcome, status, kind, streak, total)) in rows.into_iter().enumerate() {
            let row = format!("{kind} over {status:?} (total {total})");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(5700 + u32::try_from(index).unwrap());
            let replies: Vec<&str> = status.map(gsd_says).into_iter().collect();
            let _gsd = validate_evidence(root, phase, &replies);
            let mut state = validate_state(root, phase, mode);
            state.consecutive_failures = streak;
            state.phase_validate_failures = total;
            workflow::save_state(&state).unwrap();
            let (gate, _) = at_the_validate_gate(root, phase, &abort_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, outcome);
            });
            assert_eq!(gate["gate_kind"], kind, "{row}: {gate}");
            let context = gate["context"].as_str().unwrap().to_string();
            check(&row, &context, &verification_sentence(phase));
            verification_contexts.push((kind.to_string(), context));
        }

        // The reopens: an approve over a fresh `human_needed`, and over a read
        // that fails.
        for (index, (replies, reopened)) in [
            (vec![gsd_says(GsdStatus::HumanNeeded); 2], "human_needed"),
            (vec![gsd_says(GsdStatus::HumanNeeded)], "reader_failure"),
        ]
        .into_iter()
        .enumerate()
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(5720 + u32::try_from(index).unwrap());
            let _gsd = validate_evidence(root, phase, &replies);
            let mut state = validate_state(root, phase, Mode::Auto);
            let gates = at_the_gates(
                root,
                phase,
                Stage::Validate,
                &[approve_answer(), abort_answer()],
                || {
                    let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Passed);
                },
            );
            assert_eq!(gates[1]["gate_kind"], reopened, "{:?}", gates[1]);
            let context = gates[1]["context"].as_str().unwrap();
            check(
                &format!("reopen {reopened}"),
                context,
                &verification_sentence(phase),
            );
        }

        // The gaps Plan gates: no new gap plan, at and below the ceiling, and
        // a post-check that cannot read the plans.
        for (index, total) in [0, mode::MAX_PHASE_VALIDATE_FAILURES - 1]
            .into_iter()
            .enumerate()
        {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(5730 + u32::try_from(index).unwrap());
            let (mut state, _) = gaps_plan_fixture(root, phase, false);
            state.phase_validate_failures = total;
            workflow::save_state(&state).unwrap();
            let _gsd = GsdStub::answering(root, &[]);
            let gate = gaps_plan_gate(root, &mut state);
            assert_eq!(gate["gate_kind"], "no_gap_closure_plan", "{gate}");
            let context = gate["context"].as_str().unwrap();
            check(
                &format!("no_gap_closure_plan (total {total})"),
                context,
                &way_on_sentence(GateKind::NoGapClosurePlan, phase, Stage::Plan, None),
            );
            verification_contexts.push(("no_gap_closure_plan".into(), context.to_string()));
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5735);
        let (mut state, report_at) = gaps_plan_fixture(root, phase, true);
        write_dated(
            root,
            phase,
            &format!("{}-03-PLAN.md", phase.padded()),
            &plan_text(phase, "03", true),
            report_at + FIXTURE_SPACING_SECS,
        );
        let _gsd = GsdStub::answering(root, &["not json"]);
        let gate = gaps_plan_gate(root, &mut state);
        assert_eq!(gate["gate_kind"], "reader_failure", "{gate}");
        check(
            "gaps Plan reader_failure",
            gate["context"].as_str().unwrap(),
            &way_on_sentence(GateKind::ReaderFailure, phase, Stage::Plan, None),
        );

        // U10: every verification gate carries the same approve clause, the
        // rule, whatever evidence it was opened over.
        let sentence = verification_sentence(PhaseId::new(1));
        assert!(
            sentence.contains("approve: reads GSD's verification status again")
                && sentence
                    .contains("`passed` with a Validate pass on this commit over a clean tree"),
            "the approve clause states the finish rule: {sentence}"
        );
        for (kind, context) in &verification_contexts {
            assert!(
                context.contains(&sentence),
                "{kind}: the same approve clause as every verification gate: {context}"
            );
        }
        // Control: an approve clause that predicts the finish fails the check.
        let predicted = sentence.replace(VERIFICATION_APPROVE_RULE, "finishes the run");
        assert_ne!(predicted, sentence, "the planted clause differs");
        assert!(
            leads_with_only(&format!("{predicted} GSD verification: passed."), &sentence).is_err(),
            "control: a predicted approve route fails"
        );

        // The rate-limit gate (52-31): its own kind and sentence, the same
        // whatever the recorded canary. Control: the 52-09 hand-written answer
        // text after the sentence fails the check.
        for (index, stage) in [Stage::Plan, Stage::Code].into_iter().enumerate() {
            let row = format!("rate limit at {stage}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(5736 + u32::try_from(index).unwrap());
            let mut state =
                State::new(phase, AgentKind::Codex, Mode::Supervise, root.to_path_buf());
            state.stage = stage;
            state.canary = Some(devflow_core::canary::CanaryOutcome::Absent);
            workflow::save_state(&state).unwrap();
            let (gate, _) = at_the_gate(root, phase, stage, &abort_answer(), || {
                let _ = handle_rate_limited_outcome(
                    root,
                    &mut state,
                    phase,
                    stage,
                    Some("rate limited until 2099-06-18T15:45:30Z".into()),
                );
            });
            assert_eq!(gate["gate_kind"], "rate_limit", "{row}: {gate}");
            let context = gate["context"].as_str().unwrap();
            let sentence = way_on_sentence(GateKind::RateLimit, phase, stage, None);
            check(&row, context, &sentence);
            assert!(
                sentence.contains(&format!(
                    "approve: relaunches the {stage} stage now on the same agent"
                )) && sentence.contains(&format!(
                    "reject with a note: relaunches the {stage} stage with the note"
                )),
                "{row}: {sentence}"
            );
            let planted = format!(
                "{context} approving this gate relaunches it now, `devflow gate abort` ends the run"
            );
            assert!(
                leads_with_only(&planted, &sentence).is_err(),
                "{row}: control: {planted}"
            );
            // 52-32: right after the sentence come the exact resume command
            // (with `--agent`, the run is on codex) and the reset time, both
            // inside the shared display cut the banner and notify hook show.
            let resume = format!("`devflow resume --phase {phase} --agent codex`");
            let reset = "the limit resets at 2099-06-18T15:45:30Z";
            let shown_after_sentence = |context: &str| {
                let shown: String = context
                    .chars()
                    .take(devflow_core::gates::GATE_CONTEXT_DISPLAY_CHARS)
                    .collect();
                let after = shown.find(&sentence).map(|at| at + sentence.len());
                after.is_some_and(|after| {
                    let rest = &shown[after..];
                    rest.starts_with(&format!(" stage {stage} failed: "))
                        && rest.contains(&resume)
                        && rest.contains(reset)
                })
            };
            assert!(
                shown_after_sentence(context),
                "{row}: the resume command and the reset follow the sentence inside the cut: \
                 {context}"
            );
            // Control: the same text pushed past the cut by a long insertion fails.
            let pushed = context.replacen(
                &format!(" stage {stage} failed: "),
                &format!(" stage {stage} failed: {} ", "x".repeat(400)),
                1,
            );
            assert!(
                !shown_after_sentence(&pushed),
                "{row}: control: a reset past the display cut must fail: {pushed}"
            );
        }

        // The stage-failure gate at Plan, Code and Validate, with its canary
        // rows and their controls.
        let refusal = "background-task notification delivery is ABSENT";
        for (index, stage) in [Stage::Plan, Stage::Code, Stage::Validate]
            .into_iter()
            .enumerate()
        {
            for (offset, canary) in [
                devflow_core::canary::CanaryOutcome::Confirmed,
                devflow_core::canary::CanaryOutcome::Absent,
            ]
            .into_iter()
            .enumerate()
            {
                let refuses = offset == 1;
                let row = format!("stage failure at {stage}, canary refuses: {refuses}");
                let dir = tempfile::tempdir().unwrap();
                let root = dir.path();
                let phase = PhaseId::new(5740 + u32::try_from(index * 2 + offset).unwrap());
                let mut state = State::new(
                    phase,
                    AgentKind::Claude,
                    Mode::Supervise,
                    root.to_path_buf(),
                );
                state.stage = stage;
                state.canary = Some(canary);
                workflow::save_state(&state).unwrap();
                assert_eq!(recorded_canary_refusal(&state).is_some(), refuses, "{row}");
                let (gate, _) = at_the_gate(root, phase, stage, &abort_answer(), || {
                    let _ = handle_stage_failure(
                        root,
                        &mut state,
                        stage,
                        Some("the agent exited with status 1".into()),
                    );
                });
                assert_eq!(gate["gate_kind"], "stage_failure", "{row}: {gate}");
                let context = gate["context"].as_str().unwrap();
                let admitting = way_on_sentence(GateKind::StageFailure, phase, stage, None);
                let refusing = way_on_sentence(GateKind::StageFailure, phase, stage, Some(refusal));
                let resume = format!("`devflow resume --phase {phase} --agent <other>`");
                if refuses {
                    check(&row, context, &refusing);
                    assert!(
                        refusing.contains("approve or reject with a note: stops the run")
                            && refusing.contains(&resume),
                        "{row}: {refusing}"
                    );
                    // Control: the admitting sentence on a refusing run fails.
                    let planted = context.replacen(&refusing, &admitting, 1);
                    assert!(
                        leads_with_only(&planted, &refusing).is_err(),
                        "{row}: control: {planted}"
                    );
                } else {
                    check(&row, context, &admitting);
                    assert!(
                        admitting.contains(&format!("approve: retries the {stage} stage"))
                            && admitting.contains(&format!(
                                "reject with a note: relaunches the {stage} stage with the note"
                            ))
                            && !admitting.contains("--agent <other>"),
                        "{row}: {admitting}"
                    );
                }
            }
        }

        // The preflight gate and the checkpoint-declaration gate: reject is
        // not offered.
        let phase = PhaseId::new(5750);
        let preflight =
            crate::preflight::preflight_gate_context(phase, Stage::Code, "the driver check failed");
        let preflight_sentence = way_on_sentence(GateKind::Preflight, phase, Stage::Code, None);
        check("preflight", &preflight, &preflight_sentence);
        assert!(
            preflight_sentence.contains("reject: not offered"),
            "{preflight_sentence}"
        );
        let declaration = devflow_core::verify::CheckpointDeclaration {
            plan_file: format!("{}-01-PLAN.md", phase.padded()),
            element: "<task type=\"checkpoint:human-action\"></task>".into(),
            blocking_human: false,
            human_action: true,
        };
        for mode in [Mode::Auto, Mode::Supervise] {
            let rescan = crate::pipeline_launch::rescan_gate_context(
                phase,
                Stage::Code,
                &[&declaration],
                mode,
            );
            let rescan_sentence =
                way_on_sentence(GateKind::CheckpointDeclaration, phase, Stage::Code, None);
            check(
                &format!("checkpoint declaration ({mode:?})"),
                &rescan,
                &rescan_sentence,
            );
            assert!(
                rescan_sentence.contains("reject: not offered"),
                "{rescan_sentence}"
            );
        }

        // Planted controls: old answer texts after the sentence, and the
        // sentence after the cause.
        let ceiling = "The per-phase ceiling of 3 is reached: this run is paused for a human, \
                       not aborted — approve to ship, reject to loop back for another pass, or \
                       abort.";
        let cause = "The validate agent reported fail. GSD verification: passed.";
        for (row, planted) in [
            (
                "old failure-cap text",
                format!("{sentence} {cause} {ceiling}"),
            ),
            (
                "old preflight text",
                format!(
                    "[never-silent] {preflight_sentence} preflight failed for stage code: x — \
                     human review needed (retry, loop-to-code, or abort)"
                ),
            ),
            (
                "the sentence after the cause",
                format!("{cause} {sentence}"),
            ),
        ] {
            let expected = if row == "old preflight text" {
                &preflight_sentence
            } else {
                &sentence
            };
            assert!(
                leads_with_only(&planted, expected).is_err(),
                "control ({row}) must fail: {planted}"
            );
        }
    }

    // ---- 52-29 Task 2 (GATE-02, D-10, D-39, R10): one dispatch by family ----

    use crate::test_support::every_gate_kind;

    /// How a parity row is driven, by the gate's kind.
    enum ParityCase {
        /// A verification gate, opened at this stage.
        Verification(Stage),
        /// The stage-failure gate.
        StageFailure,
        /// The rate-limit gate.
        RateLimit,
        /// A gate whose reject is refused (preflight, checkpoint declaration).
        RejectRefused,
        /// A file with no kind.
        NoKind,
    }

    /// The parity case of `kind`, with no wildcard arm.
    fn parity_case(kind: Option<GateKind>) -> ParityCase {
        match kind {
            Some(GateKind::CleanPass)
            | Some(GateKind::GapsFound)
            | Some(GateKind::ReverifyNeeded)
            | Some(GateKind::HumanNeeded)
            | Some(GateKind::DisagreementOverPassed)
            | Some(GateKind::DisagreementOverHumanNeeded)
            | Some(GateKind::ReaderFailure)
            | Some(GateKind::AmbiguousVerdict)
            | Some(GateKind::FailureCap) => ParityCase::Verification(Stage::Validate),
            Some(GateKind::NoGapClosurePlan) => ParityCase::Verification(Stage::Plan),
            Some(GateKind::StageFailure) => ParityCase::StageFailure,
            Some(GateKind::RateLimit) => ParityCase::RateLimit,
            Some(GateKind::Preflight) | Some(GateKind::CheckpointDeclaration) => {
                ParityCase::RejectRefused
            }
            None => ParityCase::NoKind,
        }
    }

    /// A gate file of `kind` for the routers, which read it only through
    /// `answer_family`.
    fn parity_gate(phase: PhaseId, stage: Stage, kind: Option<GateKind>) -> GateFile {
        GateFile {
            phase,
            stage,
            context: "a parity gate".to_string(),
            timestamp: "1750000000".to_string(),
            gate_id: "g-parity".to_string(),
            gate_kind: kind,
            gsd_status: None,
            not_before: None,
        }
    }

    /// A reject carrying `note`.
    fn reject_answer(note: &str) -> devflow_core::gates::GateResponse {
        devflow_core::gates::GateResponse {
            approved: false,
            abort: false,
            note: Some(note.to_string()),
            responded_by: Some("test".into()),
        }
    }

    /// Open a verification gate `label` at `stage` as its handler does and act
    /// on its answer through the one dispatch.
    fn open_and_dispatch(
        root: &Path,
        state: &mut State,
        stage: Stage,
        label: devflow_core::gates::GateLabel,
        ceiling_gate: bool,
    ) {
        let _ = open_verification_gate(
            root,
            state,
            (stage, label),
            ("a parity gate", "a parity gate"),
            ceiling_gate,
        );
    }

    /// R10: the first answer was refused before its claim. `gates` are the
    /// gates answered in order (the refused one, then the reopened one, which
    /// the test aborted). One `gate_answer_refused` with `kind` and `answer`;
    /// the reopened gate is never-silent, names `devflow gate abort`, records
    /// `reopened_kind` and was waited on with `gate_pending` set; the refused
    /// answer was never claimed and no `gate_resolved` names it; nothing was
    /// launched.
    fn assert_refused_before_the_claim(
        root: &Path,
        phase: PhaseId,
        stage: Stage,
        gates: &[(serde_json::Value, State)],
        expected: (serde_json::Value, &str, &str),
    ) {
        let (kind, answer, reopened_kind) = expected;
        let refused = events_named(root, phase, "gate_answer_refused");
        assert_eq!(
            refused.len(),
            1,
            "one refusal: {:?}",
            phase_events(root, phase)
        );
        assert_eq!(refused[0]["gate_kind"], kind, "{}", refused[0]);
        assert_eq!(refused[0]["answer"], answer, "{}", refused[0]);
        assert_eq!(gates.len(), 2, "the gate reopened");
        let (reopened, at_reopened) = &gates[1];
        let context = reopened["context"].as_str().unwrap_or_default();
        assert!(
            context.starts_with("[never-silent]") && context.contains("devflow gate abort"),
            "the reopened gate names the answers that apply: {context}"
        );
        assert_eq!(reopened["gate_kind"], reopened_kind, "{reopened}");
        assert!(
            at_reopened.gate_pending,
            "gate_pending stays set: {at_reopened:?}"
        );
        let first_id =
            devflow_core::gates::GateId::parse(gates[0].0["gate_id"].as_str().unwrap()).unwrap();
        assert!(
            !Gates::claim_path(root, phase, stage, &first_id).exists(),
            "the refused answer was never claimed"
        );
        let resolved = events_named(root, phase, "gate_resolved");
        assert!(
            resolved
                .iter()
                .all(|event| event["gate_id"] != gates[0].0["gate_id"]),
            "no gate_resolved for the refused answer: {resolved:?}"
        );
        for launched in [
            "loop_back",
            "stage_launched",
            "transition",
            "workflow_finished",
        ] {
            assert!(
                events_named(root, phase, launched).is_empty(),
                "a refused answer launches nothing ({launched}): {:?}",
                phase_events(root, phase)
            );
        }
    }

    /// GATE-02, D-10, D-39, R10: rejecting a gate of every kind (and of a file
    /// with no kind) changes state the way `reject_route` says. A verification
    /// kind runs `/gsd-quick` with the note and clears the gap marker; a
    /// stage-failure reject relaunches its stage with the note in
    /// `relaunch_note` and leaves the fix pass alone, or stops the run when the
    /// recorded canary refuses its agent; a reject on a preflight, checkpoint
    /// declaration or kindless gate, and an empty-note reject, are refused
    /// before the claim. Control: the noted reject on a `clean_pass` gate runs
    /// `/gsd-quick` (its row below).
    #[test]
    fn the_reject_path_takes_the_route_reject_route_names() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::the_reject_path_takes_the_route_reject_route_names",
        ) {
            return;
        }
        const NOTE: &str = "rename the quiet flag, keep the old name as an alias";
        const FIX_Q: &str = "the pending quick fix";
        for (index, kind) in every_gate_kind().into_iter().enumerate() {
            let number = 5500 + u32::try_from(index).unwrap() * 10;
            let row = format!("{kind:?}");
            match parity_case(kind) {
                ParityCase::Verification(stage) => {
                    let route =
                        reject_route(&parity_gate(PhaseId::new(number), stage, kind), None, NOTE);
                    assert!(
                        matches!(&route, RejectRoute::Verification(Step::Quick(note)) if note == NOTE),
                        "{row}: {route:?}"
                    );
                    let dir = tempfile::tempdir().unwrap();
                    let root = dir.path();
                    init_repo(root);
                    let phase = PhaseId::new(number);
                    let mut state = State::new(
                        phase,
                        AgentKind::Claude,
                        Mode::Supervise,
                        root.to_path_buf(),
                    );
                    state.stage = stage;
                    state.gap_closure_pass =
                        matches!(kind, Some(GateKind::GapsFound | GateKind::NoGapClosurePlan));
                    let at_ceiling = kind == Some(GateKind::FailureCap);
                    if at_ceiling {
                        state.phase_validate_failures = mode::MAX_PHASE_VALIDATE_FAILURES;
                    }
                    workflow::save_state(&state).unwrap();
                    let label = kind.expect("a verification kind").into();
                    at_the_gate(root, phase, stage, &reject_answer(NOTE), || {
                        open_and_dispatch(root, &mut state, stage, label, at_ceiling);
                    });
                    let after = workflow::load_state(root, phase).unwrap();
                    assert_eq!(after.stage, Stage::Code, "{row}: Code runs the quick pass");
                    assert_eq!(after.code_fix, Some(FixType::Quick), "{row}");
                    assert_eq!(after.code_fix_note.as_deref(), Some(NOTE), "{row}");
                    assert!(!after.gap_closure_pass, "{row}: the gap marker is cleared");
                    assert!(
                        claude_launch_prompt(&after).contains("/gsd-quick"),
                        "{row}: the launch runs /gsd-quick"
                    );
                    assert_eq!(
                        events_named(root, phase, "phase_failure_budget_reset").len(),
                        usize::from(at_ceiling),
                        "{row}: one reset exactly at the ceiling"
                    );
                }
                ParityCase::StageFailure => {
                    for (offset, stage) in [Stage::Plan, Stage::Code, Stage::Validate]
                        .into_iter()
                        .enumerate()
                    {
                        let number = number + u32::try_from(offset).unwrap();
                        let row = format!("{row} at {stage}");
                        let route = reject_route(
                            &parity_gate(PhaseId::new(number), stage, kind),
                            None,
                            NOTE,
                        );
                        assert_eq!(
                            format!("{route:?}"),
                            format!(
                                "{:?}",
                                RejectRoute::StatusLess(StatusLessStep::RelaunchWithNote(
                                    NOTE.to_string()
                                ))
                            ),
                            "{row}"
                        );
                        let dir = tempfile::tempdir().unwrap();
                        let root = dir.path();
                        init_repo(root);
                        let phase = PhaseId::new(number);
                        let mut state = State::new(
                            phase,
                            AgentKind::Codex,
                            Mode::Supervise,
                            root.to_path_buf(),
                        );
                        state.stage = stage;
                        if stage == Stage::Code {
                            state.code_fix = Some(FixType::Quick);
                            state.code_fix_note = Some(FIX_Q.to_string());
                        }
                        workflow::save_state(&state).unwrap();
                        let before = (state.code_fix, state.code_fix_note.clone());
                        at_the_gate(root, phase, stage, &reject_answer(NOTE), || {
                            let _ = handle_stage_failure(
                                root,
                                &mut state,
                                stage,
                                Some("failed".into()),
                            );
                        });
                        let after = workflow::load_state(root, phase).unwrap();
                        assert_eq!(after.stage, stage, "{row}: the same stage relaunches");
                        assert_eq!(after.relaunch_note.as_deref(), Some(NOTE), "{row}");
                        assert_eq!(
                            (after.code_fix, after.code_fix_note.clone()),
                            before,
                            "{row}"
                        );
                        assert!(!after.stopped, "{row}");
                    }
                    // The canary row, and its admitting control.
                    for (offset, refuses) in [(3u32, true), (4, false)] {
                        let number = number + offset;
                        let row = format!("{row}, canary refuses {refuses}");
                        let dir = tempfile::tempdir().unwrap();
                        let root = dir.path();
                        init_repo(root);
                        let phase = PhaseId::new(number);
                        let mut state = State::new(
                            phase,
                            AgentKind::Claude,
                            Mode::Supervise,
                            root.to_path_buf(),
                        );
                        state.verification_baseline_captured = true;
                        state.stage = Stage::Code;
                        state.canary = Some(if refuses {
                            devflow_core::canary::CanaryOutcome::Absent
                        } else {
                            devflow_core::canary::CanaryOutcome::Confirmed
                        });
                        workflow::save_state(&state).unwrap();
                        let refusal = recorded_canary_refusal(&state);
                        assert_eq!(refusal.is_some(), refuses, "{row}: premise");
                        let route = reject_route(
                            &parity_gate(phase, Stage::Code, kind),
                            refusal.as_deref(),
                            NOTE,
                        );
                        assert_eq!(
                            matches!(
                                route,
                                RejectRoute::StatusLess(StatusLessStep::RelaunchRefused { .. })
                            ),
                            refuses,
                            "{row}: {route:?}"
                        );
                        at_the_gate(root, phase, Stage::Code, &reject_answer(NOTE), || {
                            let _ = handle_stage_failure(
                                root,
                                &mut state,
                                Stage::Code,
                                Some("failed".into()),
                            );
                        });
                        let after = workflow::load_state(root, phase).unwrap();
                        assert_eq!(after.relaunch_note.as_deref(), Some(NOTE), "{row}: kept");
                        assert_eq!(
                            events_named(root, phase, "gate_relaunch_refused").len(),
                            usize::from(refuses),
                            "{row}"
                        );
                        assert_eq!(after.stopped, refuses, "{row}: {after:?}");
                        if refuses {
                            let reason = after.stop_reason.clone().unwrap_or_default();
                            assert!(
                                reason.contains(&format!(
                                    "devflow resume --phase {phase} --agent <other>"
                                )),
                                "{row}: {reason}"
                            );
                            assert_eq!(events_named(root, phase, "gate_fired").len(), 1, "{row}");
                            assert!(events_named(root, phase, "workflow_finished").is_empty());
                        }
                    }
                }
                ParityCase::RateLimit => {
                    // The routers read no canary for this family (T-52-67):
                    // a recorded refusal changes nothing.
                    for (offset, stage) in [Stage::Plan, Stage::Code, Stage::Validate]
                        .into_iter()
                        .enumerate()
                    {
                        let number = number + u32::try_from(offset).unwrap();
                        let row = format!("{row} at {stage}");
                        let route = reject_route(
                            &parity_gate(PhaseId::new(number), stage, kind),
                            Some("a recorded refusal"),
                            NOTE,
                        );
                        assert_eq!(
                            format!("{route:?}"),
                            format!(
                                "{:?}",
                                RejectRoute::StatusLess(StatusLessStep::RelaunchWithNote(
                                    NOTE.to_string()
                                ))
                            ),
                            "{row}"
                        );
                        let dir = tempfile::tempdir().unwrap();
                        let root = dir.path();
                        init_repo(root);
                        let phase = PhaseId::new(number);
                        let mut state = State::new(
                            phase,
                            AgentKind::Codex,
                            Mode::Supervise,
                            root.to_path_buf(),
                        );
                        state.stage = stage;
                        workflow::save_state(&state).unwrap();
                        let (gate, _) =
                            at_the_gate(root, phase, stage, &reject_answer(NOTE), || {
                                let _ = handle_rate_limited_outcome(
                                    root,
                                    &mut state,
                                    phase,
                                    stage,
                                    Some("rate limited until 2099-06-18T15:45:30Z".into()),
                                );
                            });
                        assert_eq!(gate["gate_kind"], "rate_limit", "{row}: {gate}");
                        let after = workflow::load_state(root, phase).unwrap();
                        assert_eq!(after.stage, stage, "{row}: the same stage relaunches");
                        assert_eq!(after.agent, AgentKind::Codex, "{row}: on the same agent");
                        assert_eq!(after.relaunch_note.as_deref(), Some(NOTE), "{row}");
                        assert!(!after.stopped, "{row}");
                        let resolved = events_named(root, phase, "gate_resolved");
                        assert_eq!(resolved[0]["route"], "relaunch_with_note", "{row}");
                    }
                }
                ParityCase::RejectRefused => {
                    let kind = kind.expect("a kind");
                    let route = reject_route(
                        &parity_gate(PhaseId::new(number), Stage::Code, Some(kind)),
                        None,
                        NOTE,
                    );
                    assert!(matches!(route, RejectRoute::Refused(_)), "{row}: {route:?}");
                    let dir = tempfile::tempdir().unwrap();
                    let root = dir.path();
                    init_repo(root);
                    let phase = PhaseId::new(number);
                    let mut state = State::new(
                        phase,
                        AgentKind::Claude,
                        Mode::Supervise,
                        root.to_path_buf(),
                    );
                    state.stage = Stage::Code;
                    workflow::save_state(&state).unwrap();
                    let approval_before =
                        serde_json::to_string(&state.checkpoint_approval).unwrap();
                    let gates = at_the_gates_with(
                        root,
                        phase,
                        Stage::Code,
                        &[reject_answer(NOTE), abort_answer()],
                        false,
                        || {
                            let _ = crate::pipeline_gate::run_gate_with_timeout(
                                root,
                                &mut state,
                                Stage::Code,
                                kind,
                                "the gate's question",
                                20,
                            );
                        },
                    );
                    let name = serde_json::to_value(kind).unwrap();
                    let reopened = name.as_str().unwrap().to_string();
                    assert_refused_before_the_claim(
                        root,
                        phase,
                        Stage::Code,
                        &gates,
                        (name, "reject", &reopened),
                    );
                    assert_eq!(
                        serde_json::to_string(&state.checkpoint_approval).unwrap(),
                        approval_before,
                        "{row}: a refused reject widens nothing (T-48-15-03)"
                    );
                }
                ParityCase::NoKind => {
                    let route = reject_route(
                        &parity_gate(PhaseId::new(number), Stage::Validate, None),
                        None,
                        NOTE,
                    );
                    assert!(matches!(route, RejectRoute::Refused(_)), "{row}: {route:?}");
                    let dir = tempfile::tempdir().unwrap();
                    let root = dir.path();
                    init_repo(root);
                    let phase = PhaseId::new(number);
                    let mut state = validate_state(root, phase, Mode::Supervise);
                    let gates = at_the_gates_with(
                        root,
                        phase,
                        Stage::Validate,
                        &[reject_answer(NOTE), abort_answer()],
                        true,
                        || {
                            open_and_dispatch(
                                root,
                                &mut state,
                                Stage::Validate,
                                GateKind::CleanPass.into(),
                                false,
                            )
                        },
                    );
                    assert_refused_before_the_claim(
                        root,
                        phase,
                        Stage::Validate,
                        &gates,
                        (serde_json::Value::Null, "reject", "clean_pass"),
                    );
                }
            }
        }

        // An empty-note reject on a `clean_pass` gate is refused before its claim.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        let phase = PhaseId::new(5699);
        let mut state = validate_state(root, phase, Mode::Supervise);
        let gates = at_the_gates_with(
            root,
            phase,
            Stage::Validate,
            &[reject_answer("  "), abort_answer()],
            false,
            || {
                open_and_dispatch(
                    root,
                    &mut state,
                    Stage::Validate,
                    GateKind::CleanPass.into(),
                    false,
                )
            },
        );
        assert_refused_before_the_claim(
            root,
            phase,
            Stage::Validate,
            &gates,
            (
                serde_json::Value::from("clean_pass"),
                "reject",
                "clean_pass",
            ),
        );
    }

    /// The fresh facts of one approve row of the D-45 table.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Fresh {
        PassedOnHead,
        PassedWithoutPass,
        GapsFound,
        Stale,
        HumanNeeded,
    }

    /// A Code-stage Claude run of `phase` at `root` with a confirmed canary and
    /// `used` as its automatic-relaunch flag, saved and returned.
    fn rate_limit_parity_state(root: &Path, phase: PhaseId, used: bool) -> State {
        init_repo(root);
        phase_dir_fixture(root, phase);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.verification_baseline_captured = true;
        state.stage = Stage::Code;
        state.canary = Some(devflow_core::canary::CanaryOutcome::Confirmed);
        state.rate_limit_auto_relaunch_used = used;
        workflow::save_state(&state).unwrap();
        state
    }

    /// Open a rate-limit gate for a denial with a known reset over `state`,
    /// answer it with `answer`, and return the gate file as it was opened and
    /// the launches [`crate::pipeline_launch::LAUNCH_PROBE`] saw on the waiter.
    fn rate_limit_gate_answered(
        root: &Path,
        state: &mut State,
        answer: &devflow_core::gates::GateResponse,
    ) -> (GateFile, Vec<(Stage, AgentKind)>) {
        let phase = state.phase;
        let mut launches = Vec::new();
        let (gate, _) = at_the_gate(root, phase, Stage::Code, answer, || {
            crate::pipeline_launch::LAUNCH_PROBE.set(Some(Vec::new()));
            let _ = handle_rate_limited_outcome(
                root,
                state,
                phase,
                Stage::Code,
                Some("rate limited until 2099-06-18T15:45:30Z".into()),
            );
            launches = crate::pipeline_launch::LAUNCH_PROBE
                .take()
                .unwrap_or_default();
        });
        (serde_json::from_value(gate).unwrap(), launches)
    }

    /// The approve parity row's launch half for a rate-limit gate (D-38): an
    /// approve from an unspent flag launches stage S on agent A once and leaves
    /// the flag unspent; the timer then writes nothing on that gate and the
    /// count stays 1; a later denial with a known reset is armed. Controls: an
    /// abort launches nothing, and a later denial over a spent flag is not armed.
    fn rate_limit_approve_launches_once(row: &str, first: u32) {
        use crate::pipeline_gate::rate_limit_timer_deadline;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(first);
        let mut state = rate_limit_parity_state(root, phase, false);
        let (gate, launches) = rate_limit_gate_answered(root, &mut state, &approve_answer());
        assert_eq!(
            launches,
            [(Stage::Code, AgentKind::Claude)],
            "{row}: one launch, of Code on Claude"
        );
        let after = workflow::load_state(root, phase).unwrap();
        assert!(
            !after.rate_limit_auto_relaunch_used,
            "{row}: an approve does not spend the flag"
        );
        let observed = devflow_core::gates::GateId::parse(&gate.gate_id).unwrap();
        crate::pipeline_launch::LAUNCH_PROBE.set(Some(Vec::new()));
        let timer = Gates::rate_limit_timer_answer(root, phase, Stage::Code, &observed);
        assert!(
            matches!(
                timer,
                Err(devflow_core::gates::GateError::AlreadyResponded { .. }
                    | devflow_core::gates::GateError::NoOpenGate { .. })
            ),
            "{row}: the timer writes nothing on an answered gate: {timer:?}"
        );
        assert_eq!(
            crate::pipeline_launch::LAUNCH_PROBE.take(),
            Some(Vec::new()),
            "{row}: the launch count stays 1"
        );
        // A later denial with a known reset from that state is armed.
        let mut state = after;
        let (later, _) = rate_limit_gate_answered(root, &mut state, &abort_answer());
        assert!(later.not_before.is_some(), "{row}: {later:?}");
        assert_eq!(
            rate_limit_timer_deadline(&state, &later),
            later.not_before,
            "{row}: a manual approve does not spend the automatic relaunch"
        );

        // Control: an abort on the same kind of gate launches nothing.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = rate_limit_parity_state(root, PhaseId::new(first + 1), false);
        let (_, launches) = rate_limit_gate_answered(root, &mut state, &abort_answer());
        assert!(
            launches.is_empty(),
            "{row}: control: an abort launches nothing"
        );

        // Control: a later denial over a spent flag records not_before, unarmed.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = rate_limit_parity_state(root, PhaseId::new(first + 2), true);
        let (spent, _) = rate_limit_gate_answered(root, &mut state, &abort_answer());
        assert!(spent.not_before.is_some(), "{row}: {spent:?}");
        assert_eq!(
            rate_limit_timer_deadline(&state, &spent),
            None,
            "{row}: control: a spent flag arms nothing"
        );
    }

    /// D-45 rule 2, D-39, R10: approving a gate of every kind (and of a file
    /// with no kind) changes state the way `approve_route` says. Each
    /// verification kind is approved over each table row (the stub's fresh
    /// status and the recorded HEAD): finish, Validate again, the gap loop, the
    /// re-verify pass, or a reopened gate labelled by the fresh read; every
    /// kind takes the same route on the same row. Decoys: a gate recording
    /// `passed` whose fresh read is `gaps_found` runs the gap loop, and one
    /// recording `gaps_found` whose fresh read is `passed` with the PASS on
    /// HEAD finishes. Status-less approves read no GSD status; a canary-refused
    /// stage failure stops the run with no note; a kindless approve is refused
    /// before the claim.
    #[test]
    fn the_approve_path_takes_the_route_approve_route_names() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::the_approve_path_takes_the_route_approve_route_names",
        ) {
            return;
        }
        let rows = [
            Fresh::PassedOnHead,
            Fresh::PassedWithoutPass,
            Fresh::GapsFound,
            Fresh::Stale,
            Fresh::HumanNeeded,
        ];
        let mut routes_per_row: Vec<std::collections::BTreeSet<String>> =
            vec![std::collections::BTreeSet::new(); rows.len()];
        let mut number = 5700u32;
        for kind in every_gate_kind() {
            let row = format!("{kind:?}");
            number += 10;
            match parity_case(kind) {
                ParityCase::Verification(stage) => {
                    for (index, fresh) in rows.into_iter().enumerate() {
                        let phase = PhaseId::new(number + u32::try_from(index).unwrap());
                        let row = format!("{row} over {fresh:?}");
                        let status = match fresh {
                            Fresh::PassedOnHead | Fresh::PassedWithoutPass => GsdStatus::Passed,
                            Fresh::GapsFound => GsdStatus::GapsFound,
                            Fresh::Stale => GsdStatus::Stale,
                            Fresh::HumanNeeded => GsdStatus::HumanNeeded,
                        };
                        let resolved =
                            approve_row(stage, kind.unwrap().into(), phase, fresh, status, &row);
                        routes_per_row[index].insert(resolved);
                    }
                }
                ParityCase::StageFailure => {
                    for (offset, refuses) in [(0u32, false), (1, true)] {
                        let row = format!("{row}, canary refuses {refuses}");
                        let dir = tempfile::tempdir().unwrap();
                        let root = dir.path();
                        init_repo(root);
                        let phase = PhaseId::new(number + offset);
                        phase_dir_fixture(root, phase);
                        let gsd = GsdStub::answering(root, &[gsd_says(GsdStatus::Passed)]);
                        let mut state = State::new(
                            phase,
                            AgentKind::Claude,
                            Mode::Supervise,
                            root.to_path_buf(),
                        );
                        state.verification_baseline_captured = true;
                        state.stage = Stage::Code;
                        state.canary = Some(if refuses {
                            devflow_core::canary::CanaryOutcome::Absent
                        } else {
                            devflow_core::canary::CanaryOutcome::Confirmed
                        });
                        workflow::save_state(&state).unwrap();
                        let refusal = recorded_canary_refusal(&state);
                        let route = approve_route(
                            &parity_gate(phase, Stage::Code, kind),
                            refusal.as_deref(),
                            || panic!("{row}: a status-less approve reads no GSD status"),
                        );
                        let expected = if refuses {
                            "relaunch_refused"
                        } else {
                            "retry_stage"
                        };
                        assert!(
                            matches!(&route, ApproveRoute::StatusLess(step) if step.name() == expected),
                            "{row}: {route:?}"
                        );
                        at_the_gate(root, phase, Stage::Code, &approve_answer(), || {
                            let _ = handle_stage_failure(
                                root,
                                &mut state,
                                Stage::Code,
                                Some("failed".into()),
                            );
                        });
                        let after = workflow::load_state(root, phase).unwrap();
                        assert_eq!(status_reads(&gsd), 0, "{row}: no status read");
                        assert_eq!(after.stopped, refuses, "{row}: {after:?}");
                        assert_eq!(
                            after.relaunch_note, None,
                            "{row}: an approve carries no note"
                        );
                        assert_eq!(
                            events_named(root, phase, "gate_relaunch_refused").len(),
                            usize::from(refuses),
                            "{row}"
                        );
                        let resolved = events_named(root, phase, "gate_resolved");
                        assert_eq!(resolved[0]["route"], expected, "{row}: {}", resolved[0]);
                    }
                }
                ParityCase::RateLimit => {
                    for (offset, refuses) in [(0u32, false), (1, true)] {
                        let row = format!("{row}, canary refuses {refuses}");
                        let dir = tempfile::tempdir().unwrap();
                        let root = dir.path();
                        init_repo(root);
                        let phase = PhaseId::new(number + offset);
                        phase_dir_fixture(root, phase);
                        let gsd = GsdStub::answering(root, &[gsd_says(GsdStatus::Passed)]);
                        let mut state = State::new(
                            phase,
                            AgentKind::Claude,
                            Mode::Supervise,
                            root.to_path_buf(),
                        );
                        state.verification_baseline_captured = true;
                        state.stage = Stage::Code;
                        state.canary = Some(if refuses {
                            devflow_core::canary::CanaryOutcome::Absent
                        } else {
                            devflow_core::canary::CanaryOutcome::Confirmed
                        });
                        workflow::save_state(&state).unwrap();
                        let refusal = recorded_canary_refusal(&state);
                        let route = approve_route(
                            &parity_gate(phase, Stage::Code, kind),
                            refusal.as_deref(),
                            || panic!("{row}: a rate-limit approve reads no GSD status"),
                        );
                        assert!(
                            matches!(
                                &route,
                                ApproveRoute::StatusLess(StatusLessStep::RelaunchNow)
                            ),
                            "{row}: {route:?}"
                        );
                        at_the_gate(root, phase, Stage::Code, &approve_answer(), || {
                            let _ = handle_rate_limited_outcome(
                                root,
                                &mut state,
                                phase,
                                Stage::Code,
                                Some("rate limited until usage limit".into()),
                            );
                        });
                        let after = workflow::load_state(root, phase).unwrap();
                        assert_eq!(status_reads(&gsd), 0, "{row}: no status read");
                        assert!(!after.stopped, "{row}: {after:?}");
                        assert_eq!(after.relaunch_note, None, "{row}: no note");
                        assert!(
                            events_named(root, phase, "gate_relaunch_refused").is_empty(),
                            "{row}"
                        );
                        let resolved = events_named(root, phase, "gate_resolved");
                        assert_eq!(
                            resolved[0]["route"], "relaunch_now",
                            "{row}: {}",
                            resolved[0]
                        );
                    }
                    rate_limit_approve_launches_once(&row, number + 2);
                }
                ParityCase::RejectRefused => {
                    let expected = match kind {
                        Some(GateKind::Preflight) => StatusLessStep::PreflightOverride,
                        _ => StatusLessStep::RecordCheckpointSet,
                    };
                    let route = approve_route(
                        &parity_gate(PhaseId::new(number), Stage::Code, kind),
                        None,
                        || panic!("{row}: a status-less approve reads no GSD status"),
                    );
                    assert!(
                        matches!(&route, ApproveRoute::StatusLess(step) if *step == expected),
                        "{row}: {route:?}"
                    );
                }
                ParityCase::NoKind => {
                    let route = approve_route(
                        &parity_gate(PhaseId::new(number), Stage::Validate, None),
                        None,
                        || panic!("{row}: a kindless approve reads no GSD status"),
                    );
                    assert!(
                        matches!(route, ApproveRoute::Refused(_)),
                        "{row}: {route:?}"
                    );
                    let dir = tempfile::tempdir().unwrap();
                    let root = dir.path();
                    init_repo(root);
                    let phase = PhaseId::new(number);
                    let mut state = validate_state(root, phase, Mode::Supervise);
                    let gates = at_the_gates_with(
                        root,
                        phase,
                        Stage::Validate,
                        &[approve_answer(), abort_answer()],
                        true,
                        || {
                            open_and_dispatch(
                                root,
                                &mut state,
                                Stage::Validate,
                                GateKind::CleanPass.into(),
                                false,
                            )
                        },
                    );
                    assert_refused_before_the_claim(
                        root,
                        phase,
                        Stage::Validate,
                        &gates,
                        (serde_json::Value::Null, "approve", "clean_pass"),
                    );
                }
            }
        }
        for (fresh, routes) in rows.iter().zip(&routes_per_row) {
            assert_eq!(
                routes.len(),
                1,
                "every verification kind routes {fresh:?} alike: {routes:?}"
            );
        }

        // Decoys: the route follows the fresh read, never the recorded status.
        let decoy = |status: Option<GsdStatus>| devflow_core::gates::GateLabel {
            kind: GateKind::CleanPass,
            gsd_status: status,
            not_before: None,
        };
        let gap = approve_row(
            Stage::Validate,
            decoy(Some(GsdStatus::Passed)),
            PhaseId::new(5990),
            Fresh::GapsFound,
            GsdStatus::GapsFound,
            "decoy: recorded passed, fresh gaps_found",
        );
        assert_eq!(gap, "gap_loop");
        let mut gaps_label = decoy(Some(GsdStatus::GapsFound));
        gaps_label.kind = GateKind::GapsFound;
        let finish = approve_row(
            Stage::Validate,
            gaps_label,
            PhaseId::new(5991),
            Fresh::PassedOnHead,
            GsdStatus::Passed,
            "decoy: recorded gaps_found, fresh passed with the PASS on HEAD",
        );
        assert_eq!(finish, "finish");
    }

    /// One approve row: a verification gate `label` at `stage`, approved while
    /// the stub's fresh read answers `status` and the recorded PASS is on HEAD
    /// exactly for [`Fresh::PassedOnHead`]. Asserts the state the row's route
    /// leaves, and returns the route `gate_resolved` named.
    fn approve_row(
        stage: Stage,
        label: devflow_core::gates::GateLabel,
        phase: PhaseId,
        fresh: Fresh,
        status: GsdStatus,
        row: &str,
    ) -> String {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let _gsd = validate_evidence(root, phase, &[gsd_says(status)]);
        let head = repo_with_head(root);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = stage;
        state.gap_closure_pass = stage == Stage::Plan;
        state.phase_validate_failures = 4;
        state.validate_pass_head = (fresh == Fresh::PassedOnHead).then(|| head.clone());
        workflow::save_state(&state).unwrap();
        let mut answers = vec![approve_answer()];
        if fresh == Fresh::HumanNeeded {
            answers.push(abort_answer());
        }
        let gates = at_the_gates_with(root, phase, stage, &answers, false, || {
            open_and_dispatch(root, &mut state, stage, label, false);
        });
        let resolved = events_named(root, phase, "gate_resolved");
        let route = resolved[0]["route"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert_eq!(resolved[0]["gsd_status"], gsd_status_name(status), "{row}");
        assert_eq!(
            resolved[0]["agent_pass_on_head"],
            fresh == Fresh::PassedOnHead,
            "{row}: {}",
            resolved[0]
        );
        let finished = events_named(root, phase, "workflow_finished");
        match fresh {
            Fresh::PassedOnHead => {
                assert_eq!(route, "finish", "{row}");
                assert_eq!(
                    finished
                        .iter()
                        .map(|event| (event["reason"].clone(), event["shipped"].clone()))
                        .collect::<Vec<_>>(),
                    vec![(serde_json::json!("finished"), serde_json::json!(false))],
                    "{row}"
                );
                let after = workflow::load_state(root, phase).unwrap();
                assert!(after.finished && after.stopped, "{row}: {after:?}");
                assert_eq!(
                    after.stop_reason.as_deref(),
                    Some(crate::pipeline_gate::FINISHED_STOP_REASON),
                    "{row}"
                );
            }
            Fresh::PassedWithoutPass => {
                assert_eq!(route, "validate_again", "{row}");
                assert!(finished.is_empty(), "{row}");
                assert_eq!(
                    workflow::load_state(root, phase).unwrap().stage,
                    Stage::Validate
                );
            }
            Fresh::GapsFound => {
                assert_eq!(route, "gap_loop", "{row}");
                assert!(finished.is_empty(), "{row}");
                let after = workflow::load_state(root, phase).unwrap();
                assert_eq!(after.stage, Stage::Plan, "{row}");
                assert!(after.gap_closure_pass, "{row}");
            }
            Fresh::Stale => {
                assert_eq!(route, "reverify", "{row}");
                assert!(finished.is_empty(), "{row}");
                let after = workflow::load_state(root, phase).unwrap();
                assert_eq!(after.code_fix, Some(FixType::StaleReverify), "{row}");
            }
            Fresh::HumanNeeded => {
                assert_eq!(route, "reopen", "{row}");
                assert!(finished.is_empty(), "{row}");
                assert_eq!(gates.len(), 2, "{row}: the gate reopened");
                assert_eq!(gates[1].0["gate_kind"], "human_needed", "{row}");
                assert_eq!(
                    gates[1].1.phase_validate_failures, 4,
                    "{row}: counted nothing"
                );
            }
        }
        route
    }

    /// 52-29: every verification gate records the GSD status it was opened
    /// over (a label), and none for a reader failure; a stage-failure gate
    /// opened while the stub answers `passed` records none. Control in the
    /// same test: an `ambiguous_verdict` gate over `passed` and one over
    /// `gaps_found` record different statuses.
    #[test]
    fn each_verification_gate_records_the_status_that_chose_it() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::each_verification_gate_records_the_status_that_chose_it",
        ) {
            return;
        }
        let rows: [(Mode, ValidateOutcome, Option<GsdStatus>, &str, u32); 10] = [
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::Passed),
                "clean_pass",
                0,
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::GapsFound),
                "gaps_found",
                0,
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Passed,
                Some(GsdStatus::Stale),
                "reverify_needed",
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Passed,
                Some(GsdStatus::HumanNeeded),
                "human_needed",
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::Passed),
                "disagreement_over_passed",
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::HumanNeeded),
                "disagreement_over_human_needed",
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Ambiguous("no verdict".into()),
                Some(GsdStatus::Passed),
                "ambiguous_verdict",
                0,
            ),
            (
                Mode::Supervise,
                ValidateOutcome::Ambiguous("no verdict".into()),
                Some(GsdStatus::GapsFound),
                "ambiguous_verdict",
                0,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Failed,
                Some(GsdStatus::GapsFound),
                "failure_cap",
                mode::MAX_CONSECUTIVE_FAILURES - 1,
            ),
            (
                Mode::Auto,
                ValidateOutcome::Passed,
                None,
                "reader_failure",
                0,
            ),
        ];
        let mut ambiguous = Vec::new();
        for (index, (mode, outcome, status, kind, streak)) in rows.into_iter().enumerate() {
            let row = format!("{kind} over {status:?}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(5600 + u32::try_from(index).unwrap());
            let replies: Vec<&str> = status.map(gsd_says).into_iter().collect();
            let _gsd = validate_evidence(root, phase, &replies);
            let mut state = validate_state(root, phase, mode);
            state.consecutive_failures = streak;
            workflow::save_state(&state).unwrap();
            let (gate, _) = at_the_validate_gate(root, phase, &abort_answer(), || {
                let _ = handle_validate_outcome(root, &mut state, outcome);
            });
            assert_eq!(gate["gate_kind"], kind, "{row}: {gate}");
            match status {
                Some(status) => {
                    assert_eq!(gate["gsd_status"], gsd_status_name(status), "{row}: {gate}")
                }
                None => assert!(gate.get("gsd_status").is_none(), "{row}: {gate}"),
            }
            if kind == "ambiguous_verdict" {
                ambiguous.push(gate["gsd_status"].clone());
            }
        }
        assert_eq!(ambiguous.len(), 2);
        assert_ne!(
            ambiguous[0], ambiguous[1],
            "control: two ambiguous gates, two statuses"
        );

        // 52-27's no_gap_closure_plan gate, over the round's gaps_found.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (mut state, _) = gaps_plan_fixture(root, PhaseId::new(5620), false);
        let gate = gaps_plan_gate(root, &mut state);
        assert_eq!(gate["gate_kind"], "no_gap_closure_plan", "{gate}");
        assert_eq!(gate["gsd_status"], "gaps_found", "{gate}");

        // 52-27's reader-failure gate at Plan records no status.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5621);
        let (mut state, report_at) = gaps_plan_fixture(root, phase, true);
        write_dated(
            root,
            phase,
            &format!("{}-03-PLAN.md", phase.padded()),
            &plan_text(phase, "03", true),
            report_at + FIXTURE_SPACING_SECS,
        );
        let _gsd = GsdStub::answering(root, &["not json"]);
        let gate = gaps_plan_gate(root, &mut state);
        assert_eq!(gate["gate_kind"], "reader_failure", "{gate}");
        assert!(gate.get("gsd_status").is_none(), "{gate}");

        // A stage-failure gate records none, whatever GSD would say.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(5622);
        let _gsd = validate_evidence(root, phase, &[gsd_says(GsdStatus::Passed)]);
        let mut state = State::new(
            phase,
            AgentKind::Claude,
            Mode::Supervise,
            root.to_path_buf(),
        );
        state.stage = Stage::Code;
        workflow::save_state(&state).unwrap();
        let (gate, _) = at_the_gate(root, phase, Stage::Code, &abort_answer(), || {
            let _ = handle_stage_failure(root, &mut state, Stage::Code, Some("failed".into()));
        });
        assert_eq!(gate["gate_kind"], "stage_failure", "{gate}");
        assert!(gate.get("gsd_status").is_none(), "{gate}");
    }

    // ---- 52-29 Task 3 (D-45 rule 6): the table as a property ----

    /// The trigger of one property input.
    #[derive(Debug, Clone)]
    enum PropTrigger {
        AfterValidate(Mode),
        Approve,
        Reject(String),
        Abort,
    }

    /// The tree of one property input (D-46).
    #[derive(Debug, Clone)]
    enum PropTree {
        Clean,
        Dirty(Vec<String>),
        Failed,
    }

    /// One input of the rule-6 property: GSD's status (`None` for a reader
    /// failure), the recorded PASS head, the current HEAD (`None` for a failed
    /// read), the tree, the trigger, the gate's kind and the run's recorded
    /// canary refusal.
    #[derive(Debug, Clone)]
    struct PropInput {
        status: Option<GsdStatus>,
        recorded: Option<String>,
        current: Option<String>,
        tree: PropTree,
        trigger: PropTrigger,
        kind: Option<GateKind>,
        canary: Option<String>,
    }

    impl PropInput {
        /// The evidence `read_evidence`'s pure part builds from this input.
        fn evidence(&self) -> Evidence {
            let read = match self.status {
                Some(status) => Ok(StatusReport {
                    status,
                    route: String::new(),
                    next_action: None,
                    moved_files: Vec::new(),
                }),
                None => Err(EvidenceFailure::HeadRead("a planted reader failure".into())),
            };
            let tree = match &self.tree {
                PropTree::Clean => TreeRead::Clean,
                PropTree::Dirty(paths) => TreeRead::Dirty(paths.clone()),
                PropTree::Failed => TreeRead::Failed("a planted tree failure".into()),
            };
            evidence_from_reads(
                read,
                self.recorded.as_deref(),
                self.current.as_deref().ok_or("a planted HEAD failure"),
                &tree,
            )
        }

        /// Whether the PASS is on HEAD, spelled independently of
        /// `evidence_from_reads`: the recorded head equals a readable current
        /// HEAD over a clean tree.
        fn pass_on_head(&self) -> bool {
            matches!(self.tree, PropTree::Clean)
                && self.current.is_some()
                && self.recorded == self.current
        }

        /// The gate file the routers see.
        fn gate(&self) -> GateFile {
            parity_gate(PhaseId::new(1), Stage::Validate, self.kind)
        }
    }

    /// The property's strategy over every input class.
    fn prop_inputs() -> impl proptest::strategy::Strategy<Value = PropInput> {
        use proptest::prelude::*;
        let hex = || "[0-9a-f]{40}";
        let status = prop_oneof![
            Just(Some(GsdStatus::Passed)),
            Just(Some(GsdStatus::GapsFound)),
            Just(Some(GsdStatus::HumanNeeded)),
            Just(Some(GsdStatus::Stale)),
            Just(Some(GsdStatus::Missing)),
            Just(None),
        ];
        let heads =
            (proptest::option::of(hex()), 0..3u8, hex()).prop_map(|(recorded, which, other)| {
                let current = match which {
                    0 => recorded.clone().or(Some(other)),
                    1 => Some(other),
                    _ => None,
                };
                (recorded, current)
            });
        let tree = prop_oneof![
            Just(PropTree::Clean),
            proptest::collection::vec("[a-z]{1,8}(/[a-z]{1,8})?", 1..4).prop_map(PropTree::Dirty),
            Just(PropTree::Failed),
        ];
        let trigger = prop_oneof![
            Just(PropTrigger::AfterValidate(Mode::Auto)),
            Just(PropTrigger::AfterValidate(Mode::Supervise)),
            Just(PropTrigger::Approve),
            any::<String>().prop_map(PropTrigger::Reject),
            Just(PropTrigger::Abort),
        ];
        let kind = (0..every_gate_kind().len()).prop_map(|index| every_gate_kind()[index]);
        let canary = proptest::option::of("[a-z ]{1,20}");
        (status, heads, tree, trigger, kind, canary).prop_map(
            |(status, (recorded, current), tree, trigger, kind, canary)| PropInput {
                status,
                recorded,
                current,
                tree,
                trigger,
                kind,
                canary,
            },
        )
    }

    /// The route the real dispatch names for `input`: `next_step` after a
    /// Validate, the routers for an answer.
    fn real_route(input: &PropInput) -> String {
        let evidence = input.evidence();
        match &input.trigger {
            PropTrigger::AfterValidate(mode) => next_step(Trigger::AfterValidate {
                mode: *mode,
                evidence: &evidence,
            })
            .name()
            .to_string(),
            PropTrigger::Approve => {
                match approve_route(&input.gate(), input.canary.as_deref(), || input.evidence()) {
                    ApproveRoute::Verification { step, .. } => step.name().to_string(),
                    ApproveRoute::StatusLess(step) => step.name().to_string(),
                    ApproveRoute::Refused(_) => "refused".to_string(),
                }
            }
            PropTrigger::Reject(note) => {
                match reject_route(&input.gate(), input.canary.as_deref(), note) {
                    RejectRoute::Verification(step) => step.name().to_string(),
                    RejectRoute::StatusLess(step) => step.name().to_string(),
                    RejectRoute::Refused(_) => "refused".to_string(),
                }
            }
            PropTrigger::Abort => next_step(Trigger::Abort).name().to_string(),
        }
    }

    /// The control mutant: a copy of `next_step`'s table, by name, that
    /// finishes on `passed` without an agent PASS on HEAD. Answers to
    /// status-less gates take the real routers.
    fn mutant_route(input: &PropInput) -> String {
        let passed = input.status == Some(GsdStatus::Passed);
        let row = |after_validate: bool| -> &'static str {
            match input.status {
                Some(GsdStatus::Passed) => "finish",
                Some(GsdStatus::GapsFound) => "gap_loop",
                Some(GsdStatus::Stale | GsdStatus::Missing) => "reverify",
                Some(GsdStatus::HumanNeeded) | None if after_validate => "gate",
                Some(GsdStatus::HumanNeeded) | None => "reopen",
            }
        };
        match &input.trigger {
            PropTrigger::AfterValidate(Mode::Supervise) => "gate".to_string(),
            PropTrigger::AfterValidate(Mode::Auto) => row(true).to_string(),
            PropTrigger::Approve
                if answer_family(input.kind) == AnswerFamily::Verification && passed =>
            {
                row(false).to_string()
            }
            _ => real_route(input),
        }
    }

    /// The D-45 table's row for a Validate outcome or a verification-family
    /// answer, spelled out independently of `next_step`.
    fn table_row(input: &PropInput) -> String {
        let pass = input.pass_on_head();
        let row = |after_validate: bool| -> &'static str {
            match (input.status, pass, after_validate) {
                (Some(GsdStatus::Passed), true, _) => "finish",
                (Some(GsdStatus::Passed), false, true) => "gate",
                (Some(GsdStatus::Passed), false, false) => "validate_again",
                (Some(GsdStatus::GapsFound), _, _) => "gap_loop",
                (Some(GsdStatus::Stale | GsdStatus::Missing), _, _) => "reverify",
                (Some(GsdStatus::HumanNeeded) | None, _, true) => "gate",
                (Some(GsdStatus::HumanNeeded) | None, _, false) => "reopen",
            }
        };
        match &input.trigger {
            PropTrigger::AfterValidate(Mode::Supervise) => "gate".to_string(),
            PropTrigger::AfterValidate(Mode::Auto) => row(true).to_string(),
            PropTrigger::Approve => row(false).to_string(),
            PropTrigger::Reject(note) if note.trim().is_empty() => "refused".to_string(),
            PropTrigger::Reject(_) => "quick".to_string(),
            PropTrigger::Abort => "end".to_string(),
        }
    }

    /// A status-less or kindless gate's own answer, spelled out independently
    /// of `status_less_step`.
    fn status_less_row(input: &PropInput) -> String {
        let refused_canary = input.canary.is_some();
        match (&input.trigger, input.kind) {
            (PropTrigger::Abort, _) => "end",
            (PropTrigger::Approve, Some(GateKind::StageFailure)) if refused_canary => {
                "relaunch_refused"
            }
            (PropTrigger::Approve, Some(GateKind::StageFailure)) => "retry_stage",
            (PropTrigger::Reject(note), Some(GateKind::StageFailure)) if note.trim().is_empty() => {
                "refused"
            }
            (PropTrigger::Reject(_), Some(GateKind::StageFailure)) if refused_canary => {
                "relaunch_refused"
            }
            (PropTrigger::Reject(_), Some(GateKind::StageFailure)) => "relaunch_with_note",
            (PropTrigger::Approve, Some(GateKind::RateLimit)) => "relaunch_now",
            (PropTrigger::Reject(note), Some(GateKind::RateLimit)) if note.trim().is_empty() => {
                "refused"
            }
            (PropTrigger::Reject(_), Some(GateKind::RateLimit)) => "relaunch_with_note",
            (PropTrigger::Approve, Some(GateKind::Preflight)) => "preflight_override",
            (PropTrigger::Approve, Some(GateKind::CheckpointDeclaration)) => {
                "record_checkpoint_set"
            }
            _ => "refused",
        }
        .to_string()
    }

    /// D-45 rule 6, over a routing function: the route is a finish only when
    /// `can_finish` holds, and only after a Validate in auto mode or on an
    /// approve of a verification gate; after a Validate and on any answer to
    /// a verification gate it is the table's row whatever the kind; on a
    /// status-less or kindless gate it is that gate's own answer (or a
    /// refusal) and never a finish.
    fn rule_six(route: fn(&PropInput) -> String, input: &PropInput) -> Result<(), String> {
        let routed = route(input);
        let verification = answer_family(input.kind) == AnswerFamily::Verification;
        if routed == "finish" {
            if can_finish(&input.evidence()).is_none() {
                return Err(format!("a finish without can_finish: {input:?}"));
            }
            let finish_trigger = matches!(input.trigger, PropTrigger::AfterValidate(Mode::Auto))
                || (matches!(input.trigger, PropTrigger::Approve) && verification);
            if !finish_trigger {
                return Err(format!("a finish on the wrong trigger: {input:?}"));
            }
        }
        let expected = match &input.trigger {
            PropTrigger::AfterValidate(_) => table_row(input),
            PropTrigger::Approve | PropTrigger::Reject(_) | PropTrigger::Abort if verification => {
                table_row(input)
            }
            PropTrigger::Approve | PropTrigger::Reject(_) | PropTrigger::Abort => {
                status_less_row(input)
            }
        };
        if routed != expected {
            return Err(format!(
                "routed {routed}, the table says {expected}: {input:?}"
            ));
        }
        Ok(())
    }

    /// Run [`rule_six`] over `route` with persistence off, so the control's
    /// expected counterexample writes no `proptest-regressions/` file.
    fn run_rule_six(
        route: fn(&PropInput) -> String,
    ) -> Result<(), Box<proptest::test_runner::TestError<PropInput>>> {
        let config = proptest::test_runner::Config {
            cases: 1024,
            failure_persistence: None,
            ..proptest::test_runner::Config::default()
        };
        let mut runner = proptest::test_runner::TestRunner::new(config);
        runner
            .run(&prop_inputs(), |input| {
                rule_six(route, &input).map_err(proptest::test_runner::TestCaseError::fail)
            })
            .map_err(Box::new)
    }

    /// D-45 rule 6: the routing yields a finish only when `can_finish` holds,
    /// and each step is the D-45 table's row (property over status, recorded
    /// and current HEAD, the tree, the trigger with an arbitrary note, the gate
    /// kind or none, and the recorded canary refusal). Control in the same
    /// test: the same property over a routing function forced to finish on
    /// `passed` without an agent PASS on HEAD finds a counterexample holding
    /// `passed` and no PASS on HEAD. Both runs persist no failure.
    /// D-15, D-27.4, D-37, D-39: for a gate file whose `gate_kind` is
    /// `rate_limit`, `approve_route` relaunches its stage now and
    /// `reject_route` relaunches it with the note, neither reading GSD, and a
    /// whitespace-only note is refused. Control in the same test: a
    /// `clean_pass` gate at the same stage with the same context, over a fresh
    /// `passed` with the Validate PASS on HEAD, finishes on approve and runs
    /// `/gsd-quick` on reject; a `preflight` gate there takes the override and
    /// refuses the reject. So the rate-limit arms are not another kind's.
    #[test]
    fn a_rate_limit_gate_routes_approve_and_reject_to_a_relaunch_of_its_stage() {
        const NOTE: &str = "wait for the quota, then keep the smaller diff";
        const CONTEXT: &str = "[never-silent] stage validate failed: a shared context";
        let gate = |kind: GateKind| GateFile {
            phase: PhaseId::new(5320),
            stage: Stage::Validate,
            context: CONTEXT.to_string(),
            timestamp: "1750000000".to_string(),
            gate_id: "g-rate-limit".to_string(),
            gate_kind: Some(kind),
            gsd_status: None,
            not_before: None,
        };
        let passed_on_head = || {
            evidence_from_reads(
                Ok(StatusReport {
                    status: GsdStatus::Passed,
                    route: String::new(),
                    next_action: None,
                    moved_files: Vec::new(),
                }),
                Some("abc123"),
                Ok("abc123"),
                &TreeRead::Clean,
            )
        };

        let rate_limit = gate(GateKind::RateLimit);
        let approve = approve_route(&rate_limit, None, || {
            panic!("a rate-limit approve reads no GSD status")
        });
        assert!(
            matches!(
                approve,
                ApproveRoute::StatusLess(StatusLessStep::RelaunchNow)
            ),
            "{approve:?}"
        );
        let reject = reject_route(&rate_limit, None, NOTE);
        assert!(
            matches!(&reject, RejectRoute::StatusLess(StatusLessStep::RelaunchWithNote(note)) if note == NOTE),
            "{reject:?}"
        );
        assert!(
            matches!(
                reject_route(&rate_limit, None, " \t "),
                RejectRoute::Refused(_)
            ),
            "a whitespace-only note is refused"
        );

        let clean_pass = gate(GateKind::CleanPass);
        let approve = approve_route(&clean_pass, None, passed_on_head);
        assert!(
            matches!(
                &approve,
                ApproveRoute::Verification {
                    step: Step::Finish(_),
                    ..
                }
            ),
            "control: {approve:?}"
        );
        let reject = reject_route(&clean_pass, None, NOTE);
        assert!(
            matches!(&reject, RejectRoute::Verification(Step::Quick(note)) if note == NOTE),
            "control: {reject:?}"
        );

        let preflight = gate(GateKind::Preflight);
        let approve = approve_route(&preflight, None, || panic!("no GSD read"));
        assert!(
            matches!(
                approve,
                ApproveRoute::StatusLess(StatusLessStep::PreflightOverride)
            ),
            "control: {approve:?}"
        );
        assert!(
            matches!(
                reject_route(&preflight, None, NOTE),
                RejectRoute::Refused(_)
            ),
            "control: a preflight reject is refused"
        );
    }

    #[test]
    fn workflow_finished_is_reached_only_when_can_finish_holds() {
        run_rule_six(real_route).unwrap_or_else(|error| panic!("the real routing: {error}"));
        match run_rule_six(mutant_route).map_err(|error| *error) {
            Err(proptest::test_runner::TestError::Fail(reason, input)) => {
                assert_eq!(input.status, Some(GsdStatus::Passed), "{reason}: {input:?}");
                assert!(
                    !input.evidence().agent_pass_on_head,
                    "the counterexample has no PASS on HEAD: {input:?}"
                );
            }
            other => panic!("control: the mutant must fail the property: {other:?}"),
        }
    }

    /// D-12, D-45 rule 2: a `failure_cap` gate at the per-phase ceiling,
    /// approved, resets the count through the dispatch's one call (exactly one
    /// `phase_failure_budget_reset`) and then follows the table: a fresh
    /// `passed` with the PASS on HEAD finishes, one without the PASS runs
    /// Validate again, a fresh `gaps_found` runs the gap loop. Control in the
    /// same test: an abort resets nothing and the count stays at the ceiling.
    #[test]
    fn approving_a_failure_cap_gate_resets_the_cap_and_follows_the_table() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::approving_a_failure_cap_gate_resets_the_cap_and_follows_the_table",
        ) {
            return;
        }
        let ceiling = mode::MAX_PHASE_VALIDATE_FAILURES;
        let rows = [
            (6000, GsdStatus::Passed, true, false, "finish"),
            (6001, GsdStatus::Passed, false, false, "validate_again"),
            (6002, GsdStatus::GapsFound, false, false, "gap_loop"),
            (6003, GsdStatus::Passed, true, true, "end"),
        ];
        for (number, status, on_head, abort, route) in rows {
            let row = format!("{status:?}, PASS on HEAD {on_head}, abort {abort}");
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let _gsd = validate_evidence(root, phase, &[gsd_says(status)]);
            let head = repo_with_head(root);
            let mut state = validate_state(root, phase, Mode::Auto);
            state.phase_validate_failures = ceiling;
            state.validate_pass_head = on_head.then_some(head);
            workflow::save_state(&state).unwrap();
            let answer = if abort {
                abort_answer()
            } else {
                approve_answer()
            };
            at_the_validate_gate(root, phase, &answer, || {
                open_and_dispatch(
                    root,
                    &mut state,
                    Stage::Validate,
                    GateKind::FailureCap.into(),
                    true,
                );
            });
            let resets = events_named(root, phase, "phase_failure_budget_reset");
            let finished = events_named(root, phase, "workflow_finished");
            assert_eq!(
                events_named(root, phase, "gate_resolved")[0]["route"],
                route,
                "{row}"
            );
            if abort {
                assert!(
                    resets.is_empty(),
                    "control: an abort resets nothing: {resets:?}"
                );
                assert_eq!(state.phase_validate_failures, ceiling, "control: {row}");
                continue;
            }
            assert_eq!(resets.len(), 1, "{row}: exactly one reset: {resets:?}");
            assert_eq!(state.phase_validate_failures, 0, "{row}");
            match route {
                "finish" => assert_eq!(
                    finished
                        .iter()
                        .map(|event| (event["reason"].clone(), event["shipped"].clone()))
                        .collect::<Vec<_>>(),
                    vec![(serde_json::json!("finished"), serde_json::json!(false))],
                    "{row}"
                ),
                "gap_loop" => {
                    assert!(finished.is_empty(), "{row}");
                    let after = workflow::load_state(root, phase).unwrap();
                    assert_eq!(after.stage, Stage::Plan, "{row}");
                    assert!(after.gap_closure_pass, "{row}");
                }
                _ => {
                    assert!(finished.is_empty(), "{row}: no finish without the PASS");
                    assert_eq!(
                        workflow::load_state(root, phase).unwrap().stage,
                        Stage::Validate
                    );
                }
            }
        }
    }

    /// D-39: a Code stage failure over an old `passed` report with the PASS on
    /// HEAD, approved, retries Code: no finish, and the answer reads no GSD
    /// status. Control in the same test: a `clean_pass` gate over the same
    /// evidence, approved, finishes.
    #[test]
    fn approving_a_code_stage_failure_over_an_old_passed_report_retries_code() {
        if !in_bounded_child(
            "pipeline_outcomes::tests::approving_a_code_stage_failure_over_an_old_passed_report_retries_code",
        ) {
            return;
        }
        for (number, stage_failure) in [(6010, true), (6011, false)] {
            let row = if stage_failure {
                "stage_failure"
            } else {
                "control: clean_pass"
            };
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let phase = PhaseId::new(number);
            let passed = gsd_says(GsdStatus::Passed);
            let gsd = validate_evidence(root, phase, &[passed, passed]);
            let head = repo_with_head(root);
            let stage = if stage_failure {
                Stage::Code
            } else {
                Stage::Validate
            };
            let mut state = State::new(
                phase,
                AgentKind::Claude,
                Mode::Supervise,
                root.to_path_buf(),
            );
            state.stage = stage;
            state.validate_pass_head = Some(head);
            workflow::save_state(&state).unwrap();
            at_the_gate(root, phase, stage, &approve_answer(), || {
                if stage_failure {
                    let _ =
                        handle_stage_failure(root, &mut state, Stage::Code, Some("failed".into()));
                } else {
                    open_and_dispatch(root, &mut state, stage, GateKind::CleanPass.into(), false);
                }
            });
            let finished = events_named(root, phase, "workflow_finished");
            if stage_failure {
                assert!(finished.is_empty(), "{row}: a stage failure never finishes");
                assert_eq!(
                    status_reads(&gsd),
                    0,
                    "{row}: no GSD status read for the answer"
                );
                let after = workflow::load_state(root, phase).unwrap();
                assert_eq!(after.stage, Stage::Code, "{row}: Code is retried");
                assert_eq!(
                    events_named(root, phase, "gate_resolved")[0]["route"],
                    "retry_stage",
                    "{row}"
                );
            } else {
                assert_eq!(finished.len(), 1, "{row}: {:?}", phase_events(root, phase));
            }
        }
    }
}
