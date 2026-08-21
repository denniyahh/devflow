//! Pure outcome -> action policy table (D-08/D-11/D-12, 17-01).
//!
//! [`decide_action`] is the single exhaustive policy surface `advance()`
//! (Plan 04) dispatches on. It has no I/O, no `CliError`, no filesystem, and
//! no process spawn — deterministic pure function of `(Stage, AgentStatus)`.
//! The `match` has NO wildcard arm: adding a future [`crate::agent_result::AgentStatus`]
//! variant without extending this match is a compile error, which is the
//! mechanism that prevents the D-01 regression class (a new/unhandled
//! outcome silently advancing).

use crate::agent_result::AgentStatus;
use crate::stage::Stage;

/// The action to take in response to an agent outcome at a given stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Advance to the next stage.
    Advance,
    /// Automatically resume/retry (e.g. rate limit — wait and retry).
    AutoResume,
    /// Gate for a human due to an infrastructure-class fault (OOM, agent
    /// binary unavailable) — not the agent's fault.
    GateInfra,
    /// Gate for a human due to a review-worthy outcome (agent-reported
    /// failure, or an indeterminate/unknown result that must never
    /// silently advance).
    GateReview,
}

/// Decide what to do given the outcome of a stage's agent run.
///
/// `stage` is part of the signature for Plan 04's dispatch even though the
/// current mapping is stage-independent — kept for forward compatibility,
/// not used in the match itself.
///
/// The match is exhaustive over every [`AgentStatus`] variant with NO
/// wildcard arm — see the module doc comment.
pub fn decide_action(_stage: Stage, outcome: AgentStatus) -> Action {
    match outcome {
        AgentStatus::Success => Action::Advance,
        AgentStatus::RateLimited => Action::AutoResume,
        // A2 (41-antigravity UAT): an ambiguous transport-cancel — the agent's
        // own final message self-reported success but the CLI's envelope was
        // torn down before finalization — is retried, never advanced and never
        // gated. Bounded by the same shared infra ceiling as RateLimited (see
        // `handle_ambiguous_outcome` in the CLI).
        AgentStatus::Ambiguous => Action::AutoResume,
        AgentStatus::ResourceKilled => Action::GateInfra,
        AgentStatus::AgentUnavailable => Action::GateInfra,
        // DEFERRED (Plan 01 MEDIUM, OpenCode): Failed and Unknown map
        // identically to GateReview. Intentional — both are non-advance
        // outcomes today and the current phase needs no behavioral
        // distinction between them. The distinction is NOT lost:
        // AgentResult.decided_by_layer plus the underlying AgentStatus
        // variant both survive into events.jsonl, so Phase 18's 18d
        // reconciliation can differentiate a reported failure from a
        // vanished process without a new Action variant. Revisit if 18d
        // requires divergent routing.
        AgentStatus::Failed => Action::GateReview,
        AgentStatus::Unknown => Action::GateReview,
        // 31-02 (D-06/D-08). GateReview, not GateInfra: nothing
        // infrastructural failed — DevFlow chose to stop waiting, and the
        // operator has real commits from a partly-done run to look at, which
        // is a review question, not an infra one.
        //
        // Emphatically NOT AutoResume. D-08 makes an idle timeout terminal:
        // the run's extent is unknown (the agent went quiet rather than
        // reporting), so a retry would restart on top of a dirty tree nobody
        // has surveyed. `idle_timeout_is_never_auto_resumed` pins this across
        // every stage.
        AgentStatus::IdleTimeout => Action::GateReview,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_advances() {
        assert_eq!(
            decide_action(Stage::Code, AgentStatus::Success),
            Action::Advance
        );
    }

    #[test]
    fn rate_limited_auto_resumes() {
        assert_eq!(
            decide_action(Stage::Code, AgentStatus::RateLimited),
            Action::AutoResume
        );
    }

    /// A2 (41-antigravity UAT): an ambiguous transport-cancel resolves to
    /// auto-resume, never Advance and never a gate — the agent self-reported
    /// success, so the stage is re-driven rather than reviewed.
    #[test]
    fn ambiguous_auto_resumes_never_advances() {
        assert_eq!(
            decide_action(Stage::Plan, AgentStatus::Ambiguous),
            Action::AutoResume
        );
        assert_ne!(
            decide_action(Stage::Plan, AgentStatus::Ambiguous),
            Action::Advance,
            "Ambiguous must never advance"
        );
        assert_ne!(
            decide_action(Stage::Plan, AgentStatus::Ambiguous),
            Action::GateReview,
            "Ambiguous must never gate for review"
        );
    }

    #[test]
    fn resource_killed_gates_infra() {
        assert_eq!(
            decide_action(Stage::Code, AgentStatus::ResourceKilled),
            Action::GateInfra
        );
    }

    #[test]
    fn agent_unavailable_gates_infra() {
        assert_eq!(
            decide_action(Stage::Code, AgentStatus::AgentUnavailable),
            Action::GateInfra
        );
    }

    #[test]
    fn failed_gates_review() {
        assert_eq!(
            decide_action(Stage::Code, AgentStatus::Failed),
            Action::GateReview
        );
    }

    /// D-01: Unknown must NEVER map to Advance.
    #[test]
    fn unknown_gates_review_never_advances() {
        assert_eq!(
            decide_action(Stage::Code, AgentStatus::Unknown),
            Action::GateReview
        );
    }

    /// 31-02 D-06/D-08: an idle timeout is a review-worthy outcome about an
    /// indeterminate run the operator has real commits to look at — never an
    /// advance, and never an infra gate (nothing infrastructural failed).
    #[test]
    fn idle_timeout_gates_review() {
        assert_eq!(
            decide_action(Stage::Code, AgentStatus::IdleTimeout),
            Action::GateReview
        );
    }

    /// Every stage in the chain, walked from `Define` via `Stage::next` rather
    /// than hardcoded, so a stage inserted into the chain is covered without
    /// editing this test. (Limit: a stage added OUTSIDE the linear chain would
    /// still be missed — `Stage` exposes no exhaustive iterator to key off.)
    fn every_stage() -> Vec<Stage> {
        let mut stages = vec![Stage::Define];
        while let Some(next) = stages.last().and_then(|s| s.next()) {
            stages.push(next);
        }
        stages
    }

    /// 31-02 D-08: an idle timeout is TERMINAL. Auto-resuming would restart
    /// from a dirty, partly-done state whose extent nobody has established —
    /// the run went quiet, it did not report. Asserted for every stage, not
    /// just `Code`, because `decide_action`'s mapping is stage-independent
    /// today and a future stage-sensitive arm must not quietly reintroduce a
    /// retry here.
    #[test]
    fn idle_timeout_is_never_auto_resumed() {
        let stages = every_stage();
        assert_eq!(stages.len(), 5, "stage chain changed; review this test");
        for stage in stages {
            let action = decide_action(stage, AgentStatus::IdleTimeout);
            assert_ne!(
                action,
                Action::AutoResume,
                "IdleTimeout must never auto-resume at {stage:?}"
            );
            assert_ne!(
                action,
                Action::Advance,
                "IdleTimeout must never advance at {stage:?}"
            );
        }
    }

    /// Negative control for the test above: the assertion loop has teeth only
    /// if it can actually fail. `RateLimited` is the one status that DOES
    /// auto-resume, so running the same loop over it must produce the opposite
    /// result at every stage. If this ever stops holding, the loop above is
    /// vacuous and its green is meaningless.
    #[test]
    fn the_never_auto_resume_loop_can_actually_fail() {
        for stage in every_stage() {
            assert_eq!(
                decide_action(stage, AgentStatus::RateLimited),
                Action::AutoResume,
                "negative control: RateLimited must auto-resume at {stage:?}"
            );
        }
    }
}
