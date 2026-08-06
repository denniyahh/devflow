//! Execution mode and the mode-driven gate decision.
//!
//! Mode is a per-session CLI flag on `devflow start` — there is no config file
//! and no per-phase toggling.
//!
//! - **Auto:** Define and Plan run once. Code ↔ Validate auto-loop until clean.
//!   Then Ship. The only human gate is at Ship — unless Validate fails
//!   [`MAX_CONSECUTIVE_FAILURES`] times in a row, which forces a gate.
//! - **Supervise:** Same pipeline, but Validate always fires a gate to Hermes →
//!   Human before advancing to Ship.

use crate::stage::Stage;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Number of consecutive Validate failures in Auto mode before a gate is forced.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Ceiling for [`crate::state::State::infra_failures`] before an
/// infrastructure-class fault chain (OOM/`ResourceKilled`, missing agent
/// binary/`AgentUnavailable`) forces a terminal gate (D-08, 17-01).
///
/// Deliberately more lenient than [`MAX_CONSECUTIVE_FAILURES`] (3): infra
/// faults are not the agent's fault, so a higher ceiling tolerates transient
/// cloud outages/OOM blips that a 3-ceiling would abort prematurely, while
/// still bounding a stuck loop to at most 5 unobserved cycles before a
/// terminal abort. Any increment of `infra_failures` must use
/// `saturating_add` so a long-running stuck loop cannot overflow `u32`. The
/// CLI's `transition()` resets `infra_failures` to 0 unconditionally on
/// every successful stage transition (CR-01, 17-06 gap closure) — this
/// reset is what makes the "5 unobserved cycles" ceiling bound a stuck loop
/// rather than a phase's entire lifetime. Unlike `infra_failures`,
/// `consecutive_failures`' reset is conditional — see
/// [`transition_resets_consecutive_failures`] — the two counters no longer
/// share a single reset condition (18d, WR-11).
pub const MAX_INFRA_FAILURES: u32 = 5;

/// Ceiling for [`crate::state::State::preflight_retries`] before a
/// preflight gate's `GateAction::LoopBack` recursion aborts rather than
/// polling another 7-day gate timeout (18f, D-18f backstop). A failing
/// preflight is a readiness problem the operator is actively being asked
/// about right now, not a transient infrastructure blip, so this takes the
/// tighter [`MAX_CONSECUTIVE_FAILURES`]-style ceiling rather than the more
/// lenient [`MAX_INFRA_FAILURES`]. Unlike those two counters, this one is
/// NOT reset by `transition()` — it is reset by preflight success and by
/// human approval (`GateAction::Advance`), both inside `run_preflight`
/// (`devflow-cli/src/main.rs`).
pub const MAX_PREFLIGHT_RETRIES: u32 = 3;

/// Ceiling for [`crate::state::State::checkpoint_resumes`] before a
/// checkpoint auto-decide relaunch (D-03/D-04, 28-03) stops resuming and
/// falls through to the never-silent gate instead, its context naming the
/// exhaustion. Bounds consecutive `claude --resume` relaunches for one
/// stage's agent run against a checkpoint that keeps re-firing.
///
/// Takes the tighter [`MAX_CONSECUTIVE_FAILURES`]-style ceiling rather than
/// the more lenient [`MAX_INFRA_FAILURES`]: a re-firing checkpoint is a
/// decision the agent is failing to close on its own, not a transient
/// infrastructure blip, so it does not deserve the same tolerance an OOM
/// blip or a missing binary gets. An unbounded resume loop here would be
/// structurally the same "gates hang forever" failure class D-09
/// (`28-CONTEXT.md`) documents — this ceiling is what keeps it from becoming
/// that.
///
/// Any increment of `checkpoint_resumes` must use `saturating_add`, exactly
/// like [`Self::infra_failures`] and [`Self::preflight_retries`], so a stuck
/// loop cannot overflow `u32`. Reset to 0 by every ORDINARY fresh stage
/// launch (`pipeline_launch::launch_stage_inner`) — never by `transition()`
/// — so the ceiling bounds one stage's resume budget, not a phase's entire
/// lifetime, the same distinction [`MAX_INFRA_FAILURES`]'s doc comment draws
/// for `infra_failures`. On exhaustion: fall through to the never-silent
/// gate with a reason naming the exhaustion — never a silent stop, never an
/// unbounded loop.
pub const MAX_CHECKPOINT_RESUMES: u32 = 3;

/// 28-03 (Task 1): the ceiling must be a small, positive, bounded number —
/// greater than zero (or a checkpoint could never resume even once) and no
/// larger than the more lenient infra ceiling (a re-firing checkpoint gets
/// LESS tolerance than a transient infra blip, not more). A compile-time
/// assertion rather than a runtime `#[test]` because both operands are
/// `const` — clippy's `assertions_on_constants` correctly flags a runtime
/// test here as unable to ever fail at runtime; this const block still
/// fails the BUILD if a future edit violates the invariant.
const _: () = assert!(MAX_CHECKPOINT_RESUMES > 0 && MAX_CHECKPOINT_RESUMES <= MAX_INFRA_FAILURES);

/// Whether `transition()` should zero
/// [`crate::state::State::consecutive_failures`] when moving from `from` to
/// `to`.
///
/// `consecutive_failures` is meant to count repeated Code↔Validate CYCLES —
/// each cycle is a full loop through Code, then Validate, then (on failure)
/// back to Code again. But the Code→Validate hop is crossed on *every
/// single cycle*, including the ones that are about to fail. Resetting the
/// counter on that specific hop means it can never accumulate past 1, so
/// [`MAX_CONSECUTIVE_FAILURES`] — the ceiling that exists specifically to
/// bound this loop — is unreachable (18d). Every other transition is
/// genuine forward progress out of the Code↔Validate loop (or the initial
/// Define→Plan→Code entry into it) and correctly clears the counter.
///
/// This rule deliberately does NOT apply to
/// [`crate::state::State::infra_failures`], whose unconditional reset in
/// `transition()` is correct for its own semantics: infra faults accumulate
/// within a single stage's repeated failures and are routed through
/// `handle_infra_outcome` → `gate_or_abort_infra` → `handle_stage_failure`,
/// whose retry arms call `launch_stage` directly and never cross
/// `transition()` at all. Widening this predicate's shape onto
/// `infra_failures` would silently convert [`MAX_INFRA_FAILURES`] from a
/// stuck-loop bound into a phase-lifetime bound — the exact regression
/// 17-06 was written to prevent.
pub fn transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool {
    !matches!((from, to), (Stage::Code, Stage::Validate))
}

/// Whether a Validate failure represents forward progress since the last
/// recorded failure (999.66, D-03) — i.e. whether Code produced new commits
/// on the phase's feature branch since
/// [`crate::state::State::last_validate_failure_commit_count`] was last
/// observed.
///
/// `previous` is the baseline recorded at the prior failure;
/// `current` is the commit count observed at THIS failure.
///
/// `None` for `previous` reports progress: it means no prior failure has
/// been recorded, so there is no streak to continue — the first failure of
/// a phase, and the first failure observed after resuming state written
/// before this baseline field existed, must both begin a fresh streak
/// rather than extend a nonexistent one.
///
/// The comparison is strictly greater, not merely not-equal: a count that
/// went DOWN means the branch was rewound or rebuilt, which is not evidence
/// that the problem Validate reported was addressed. Treating a decrease as
/// progress would hand a free counter reset to exactly the situation least
/// likely to deserve one.
///
/// **What this predicate does not establish.** A `true` result means new
/// commits exist, not that those commits addressed anything. An agent that
/// commits something trivial on every cycle resets the streak every cycle
/// and never reaches [`MAX_CONSECUTIVE_FAILURES`]. This is the accepted,
/// documented weakness of the commit-count signal recorded in
/// `33-RESEARCH.md`'s D-03 Recommendation and Assumptions Log A1 — the same
/// weakness `evaluate_layer2`'s own "no work done" gate already carries,
/// which a single trivial commit also already defeats today. It is a real
/// narrowing of the guarantee that `MAX_CONSECUTIVE_FAILURES` bounds a
/// genuinely stuck loop, and it is deliberately NOT strengthened here with a
/// lines-changed or files-touched threshold — that is a follow-up if the
/// assumption proves wrong, not a speculative heuristic to add to the
/// safety gate's path now.
pub fn consecutive_failures_made_progress(previous: Option<u32>, current: u32) -> bool {
    previous.is_none_or(|p| current > p)
}

/// How DevFlow drives the pipeline for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Run the pipeline without human gates until Ship (or repeated failure).
    Auto,
    /// Fire a Validate gate to Hermes → Human before Ship.
    Supervise,
}

impl Mode {
    /// Whether `stage` should fire a gate, given how many consecutive Validate
    /// failures have already occurred this session.
    ///
    /// - Ship always gates (both modes).
    /// - Supervise gates at every Validate.
    /// - Auto gates at Validate only after [`MAX_CONSECUTIVE_FAILURES`] failures.
    pub fn should_gate(self, stage: Stage, consecutive_failures: u32) -> bool {
        match stage {
            Stage::Ship => true,
            Stage::Validate => match self {
                Mode::Supervise => true,
                Mode::Auto => consecutive_failures >= MAX_CONSECUTIVE_FAILURES,
            },
            _ => false,
        }
    }

    /// Whether a failed Validate at `stage` may auto-loop back to Code without a
    /// human gate. Auto loops Code↔Validate; Supervise requires human approval.
    pub fn should_auto_loop(self, stage: Stage) -> bool {
        matches!(stage, Stage::Validate) && matches!(self, Mode::Auto)
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Mode::Auto => "auto",
            Mode::Supervise => "supervise",
        };
        f.write_str(name)
    }
}

impl FromStr for Mode {
    type Err = ModeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Mode::Auto),
            "supervise" | "supervised" => Ok(Mode::Supervise),
            other => Err(ModeParseError(other.to_string())),
        }
    }
}

/// Error returned when parsing an unsupported mode name.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unsupported mode `{0}`; expected auto or supervise")]
pub struct ModeParseError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_accepts_canonical_and_alias() {
        assert_eq!("auto".parse::<Mode>().unwrap(), Mode::Auto);
        assert_eq!("AUTO".parse::<Mode>().unwrap(), Mode::Auto);
        assert_eq!("supervise".parse::<Mode>().unwrap(), Mode::Supervise);
        assert_eq!("supervised".parse::<Mode>().unwrap(), Mode::Supervise);
    }

    #[test]
    fn from_str_rejects_unknown() {
        let err = "yolo".parse::<Mode>().unwrap_err();
        assert!(err.to_string().contains("yolo"));
    }

    #[test]
    fn auto_does_not_gate_validate_until_failure_threshold() {
        assert!(!Mode::Auto.should_gate(Stage::Validate, 0));
        assert!(!Mode::Auto.should_gate(Stage::Validate, 2));
        assert!(Mode::Auto.should_gate(Stage::Validate, MAX_CONSECUTIVE_FAILURES));
        assert!(Mode::Auto.should_gate(Stage::Validate, 9));
    }

    #[test]
    fn supervise_always_gates_validate() {
        assert!(Mode::Supervise.should_gate(Stage::Validate, 0));
        assert!(Mode::Supervise.should_gate(Stage::Validate, 5));
    }

    #[test]
    fn ship_always_gates_in_both_modes() {
        assert!(Mode::Auto.should_gate(Stage::Ship, 0));
        assert!(Mode::Supervise.should_gate(Stage::Ship, 0));
    }

    #[test]
    fn non_gate_stages_never_gate() {
        for stage in [Stage::Define, Stage::Plan, Stage::Code] {
            assert!(!Mode::Auto.should_gate(stage, 99));
            assert!(!Mode::Supervise.should_gate(stage, 99));
        }
    }

    #[test]
    fn auto_loops_validate_supervise_does_not() {
        assert!(Mode::Auto.should_auto_loop(Stage::Validate));
        assert!(!Mode::Supervise.should_auto_loop(Stage::Validate));
        assert!(!Mode::Auto.should_auto_loop(Stage::Code));
    }

    #[test]
    fn display_round_trips_through_from_str() {
        for mode in [Mode::Auto, Mode::Supervise] {
            assert_eq!(mode.to_string().parse::<Mode>().unwrap(), mode);
        }
    }

    #[test]
    fn consecutive_reset_skips_the_code_to_validate_hop() {
        assert!(!transition_resets_consecutive_failures(
            Stage::Code,
            Stage::Validate
        ));
    }

    #[test]
    fn consecutive_reset_fires_on_every_other_transition() {
        // Enumerated explicitly (not a negation of the skip case above) so a
        // future Stage variant added to the linear chain doesn't silently
        // fall through un-asserted.
        assert!(transition_resets_consecutive_failures(
            Stage::Define,
            Stage::Plan
        ));
        assert!(transition_resets_consecutive_failures(
            Stage::Plan,
            Stage::Code
        ));
        assert!(transition_resets_consecutive_failures(
            Stage::Validate,
            Stage::Ship
        ));
    }

    #[test]
    fn made_progress_treats_no_prior_record_as_progress() {
        // No prior record with a zero current count: the state of a
        // brand-new phase whose feature branch does not exist yet. This is
        // the case that matters most — it must report progress so the very
        // first failure of a phase never mis-accumulates.
        assert!(consecutive_failures_made_progress(None, 0));
        // No prior record with a non-zero current count.
        assert!(consecutive_failures_made_progress(None, 5));
    }

    #[test]
    fn made_progress_requires_a_strictly_higher_count() {
        // Strictly greater: progress.
        assert!(consecutive_failures_made_progress(Some(2), 3));
        // Equal, both non-zero: no progress.
        assert!(!consecutive_failures_made_progress(Some(2), 2));
        // Equal, both zero: no progress. This is the case
        // `consecutive_failures_reaches_ceiling_across_cycles` (devflow-cli)
        // actually exercises — a repo with no feature branch, counting zero
        // commits every cycle — and it is the single case
        // MAX_CONSECUTIVE_FAILURES most depends on remaining reachable.
        assert!(!consecutive_failures_made_progress(Some(0), 0));
        // Lower: no progress. A count that went down means the branch was
        // rewound or rebuilt, not that the reported problem was addressed.
        assert!(!consecutive_failures_made_progress(Some(3), 2));
    }
}
