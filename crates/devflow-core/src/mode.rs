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
/// CLI's `transition()` resets `infra_failures` to 0 on every successful
/// stage transition (CR-01, 17-06 gap closure) — this reset is what makes
/// the "5 unobserved cycles" ceiling bound a stuck loop rather than a
/// phase's entire lifetime.
pub const MAX_INFRA_FAILURES: u32 = 5;

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
}
