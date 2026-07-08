//! DevFlow stage machine.
//!
//! The workflow is a single linear chain of five stages:
//! Define → Plan → Code → Validate → Ship.
//!
//! - **Define / Plan / Code** launch a coding agent driven by a GSD slash command.
//! - **Validate / Ship** are gate stages: they may fire a gate to Hermes (the
//!   human interface) depending on the active [`Mode`](crate::mode::Mode).

use serde::{Deserialize, Serialize};
use std::fmt;

/// A stage in the DevFlow execution pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// Gather requirements via adaptive Q&A (`/gsd-discuss-phase`).
    Define,
    /// Research + plan + verify loop (`/gsd-plan-phase`).
    Plan,
    /// Wave-based parallel execution (`/gsd-execute-phase`).
    Code,
    /// Nyquist coverage audit (`/gsd-validate-phase`).
    Validate,
    /// PR + review + merge prep (`/gsd-ship`).
    Ship,
}

impl Stage {
    /// The next stage in the linear chain, or `None` after `Ship`.
    pub fn next(self) -> Option<Stage> {
        match self {
            Stage::Define => Some(Stage::Plan),
            Stage::Plan => Some(Stage::Code),
            Stage::Code => Some(Stage::Validate),
            Stage::Validate => Some(Stage::Ship),
            Stage::Ship => None,
        }
    }

    /// Whether this stage fires a gate to Hermes (Validate and Ship).
    pub fn is_gate(self) -> bool {
        matches!(self, Stage::Validate | Stage::Ship)
    }

    /// Whether this stage launches a coding agent (Define, Plan, Code).
    pub fn is_agent_stage(self) -> bool {
        matches!(self, Stage::Define | Stage::Plan | Stage::Code)
    }

    /// The GSD slash command for this stage, with `{N}` as the phase placeholder.
    pub fn gsd_command(self) -> &'static str {
        match self {
            Stage::Define => "/gsd-discuss-phase {N}",
            Stage::Plan => "/gsd-plan-phase {N}",
            Stage::Code => "/gsd-execute-phase {N}",
            Stage::Validate => "/gsd-validate-phase {N}",
            Stage::Ship => "/gsd-ship {N}",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Stage::Define => "define",
            Stage::Plan => "plan",
            Stage::Code => "code",
            Stage::Validate => "validate",
            Stage::Ship => "ship",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_walks_linear_chain_then_terminates() {
        assert_eq!(Stage::Define.next(), Some(Stage::Plan));
        assert_eq!(Stage::Plan.next(), Some(Stage::Code));
        assert_eq!(Stage::Code.next(), Some(Stage::Validate));
        assert_eq!(Stage::Validate.next(), Some(Stage::Ship));
        assert_eq!(Stage::Ship.next(), None);
    }

    #[test]
    fn gate_stages_are_validate_and_ship() {
        assert!(Stage::Validate.is_gate());
        assert!(Stage::Ship.is_gate());
        assert!(!Stage::Define.is_gate());
        assert!(!Stage::Plan.is_gate());
        assert!(!Stage::Code.is_gate());
    }

    #[test]
    fn agent_stages_are_define_plan_code() {
        assert!(Stage::Define.is_agent_stage());
        assert!(Stage::Plan.is_agent_stage());
        assert!(Stage::Code.is_agent_stage());
        assert!(!Stage::Validate.is_agent_stage());
        assert!(!Stage::Ship.is_agent_stage());
    }

    #[test]
    fn gsd_commands_match_stage() {
        assert_eq!(Stage::Define.gsd_command(), "/gsd-discuss-phase {N}");
        assert_eq!(Stage::Plan.gsd_command(), "/gsd-plan-phase {N}");
        assert_eq!(Stage::Code.gsd_command(), "/gsd-execute-phase {N}");
        assert_eq!(Stage::Validate.gsd_command(), "/gsd-validate-phase {N}");
        assert_eq!(Stage::Ship.gsd_command(), "/gsd-ship {N}");
    }

    #[test]
    fn display_is_lowercase() {
        assert_eq!(Stage::Define.to_string(), "define");
        assert_eq!(Stage::Plan.to_string(), "plan");
        assert_eq!(Stage::Code.to_string(), "code");
        assert_eq!(Stage::Validate.to_string(), "validate");
        assert_eq!(Stage::Ship.to_string(), "ship");
    }

    #[test]
    fn serde_round_trips_each_stage() {
        for stage in [
            Stage::Define,
            Stage::Plan,
            Stage::Code,
            Stage::Validate,
            Stage::Ship,
        ] {
            let json = serde_json::to_string(&stage).unwrap();
            let back: Stage = serde_json::from_str(&json).unwrap();
            assert_eq!(stage, back);
        }
        // Wire format is lowercase.
        assert_eq!(serde_json::to_string(&Stage::Code).unwrap(), "\"code\"");
    }
}
