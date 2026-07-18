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
        assert_ne!(
            decide_action(Stage::Code, AgentStatus::Unknown),
            Action::Advance
        );
    }

    /// Determinism: repeated calls with identical inputs return identical
    /// results (D-11/D-12 — pure function, no hidden state).
    #[test]
    fn decide_action_is_deterministic() {
        for stage in [
            Stage::Define,
            Stage::Plan,
            Stage::Code,
            Stage::Validate,
            Stage::Ship,
        ] {
            for outcome in [
                AgentStatus::Success,
                AgentStatus::Failed,
                AgentStatus::RateLimited,
                AgentStatus::Unknown,
                AgentStatus::ResourceKilled,
                AgentStatus::AgentUnavailable,
            ] {
                assert_eq!(decide_action(stage, outcome), decide_action(stage, outcome));
            }
        }
    }
}
