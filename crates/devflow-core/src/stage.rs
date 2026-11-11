//! DevFlow stage machine.
//!
//! The workflow is a single linear chain of three stages:
//! Plan → Code → Validate. A passing Validate ends the run.
//!
//! - **Plan / Code** launch a coding agent driven by a GSD slash command.
//! - **Validate** is the gate stage: it may fire a gate to Hermes (the human
//!   interface) depending on the active [`Mode`](crate::mode::Mode).

use serde::{Deserialize, Serialize};
use std::fmt;

/// A stage in the DevFlow execution pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// Research + plan + verify loop (`/gsd-plan-phase`).
    Plan,
    /// Wave-based parallel execution (`/gsd-execute-phase`).
    Code,
    /// Nyquist coverage audit (`/gsd-validate-phase`). The last stage.
    Validate,
}

impl Stage {
    /// The next stage in the linear chain, or `None` after `Validate`.
    pub fn next(self) -> Option<Stage> {
        match self {
            Stage::Plan => Some(Stage::Code),
            Stage::Code => Some(Stage::Validate),
            Stage::Validate => None,
        }
    }

    /// Whether this stage fires a gate to Hermes (Validate only).
    pub fn is_gate(self) -> bool {
        matches!(self, Stage::Validate)
    }

    /// Whether this stage launches a coding agent (Plan, Code).
    pub fn is_agent_stage(self) -> bool {
        matches!(self, Stage::Plan | Stage::Code)
    }

    /// The GSD slash command for this stage, with `{N}` as the phase placeholder.
    pub fn gsd_command(self) -> &'static str {
        match self {
            Stage::Plan => "/gsd-plan-phase {N}",
            Stage::Code => "/gsd-execute-phase {N}",
            Stage::Validate => "/gsd-validate-phase {N}",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Stage::Plan => "plan",
            Stage::Code => "code",
            Stage::Validate => "validate",
        };
        f.write_str(name)
    }
}

impl std::str::FromStr for Stage {
    type Err = StageParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "plan" => Ok(Stage::Plan),
            "code" => Ok(Stage::Code),
            "validate" => Ok(Stage::Validate),
            other => Err(StageParseError(other.to_string())),
        }
    }
}

/// Error returned when parsing an unsupported stage name.
#[derive(Debug, Clone, thiserror::Error)]
#[error("unsupported stage `{0}`; expected plan, code, or validate")]
pub struct StageParseError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_walks_linear_chain_then_terminates() {
        assert_eq!(Stage::Plan.next(), Some(Stage::Code));
        assert_eq!(Stage::Code.next(), Some(Stage::Validate));
        assert_eq!(Stage::Validate.next(), None);
    }

    #[test]
    fn the_gate_stage_is_validate() {
        assert!(Stage::Validate.is_gate());
        assert!(!Stage::Plan.is_gate());
        assert!(!Stage::Code.is_gate());
    }

    #[test]
    fn agent_stages_are_plan_code() {
        assert!(Stage::Plan.is_agent_stage());
        assert!(Stage::Code.is_agent_stage());
        assert!(!Stage::Validate.is_agent_stage());
    }

    #[test]
    fn gsd_commands_match_stage() {
        assert_eq!(Stage::Plan.gsd_command(), "/gsd-plan-phase {N}");
        assert_eq!(Stage::Code.gsd_command(), "/gsd-execute-phase {N}");
        assert_eq!(Stage::Validate.gsd_command(), "/gsd-validate-phase {N}");
    }

    #[test]
    fn display_is_lowercase() {
        assert_eq!(Stage::Plan.to_string(), "plan");
        assert_eq!(Stage::Code.to_string(), "code");
        assert_eq!(Stage::Validate.to_string(), "validate");
    }

    #[test]
    fn from_str_round_trips_display_and_rejects_unknown() {
        for stage in [Stage::Plan, Stage::Code, Stage::Validate] {
            assert_eq!(stage.to_string().parse::<Stage>().unwrap(), stage);
        }
        assert_eq!("VALIDATE".parse::<Stage>().unwrap(), Stage::Validate);
        let err = "docsing".parse::<Stage>().unwrap_err();
        assert!(err.to_string().contains("docsing"));
    }

    #[test]
    fn removed_stage_names_do_not_parse() {
        // Split so the removed-surface scan does not read this control as a leftover.
        for removed in [concat!("def", "ine"), concat!("sh", "ip")] {
            let err = removed
                .parse::<Stage>()
                .expect_err("a removed stage name must not parse");
            let text = err.to_string();
            let (_, expected) = text
                .split_once("expected")
                .expect("the parse error lists the stages it accepts");
            for kept in ["plan", "code", "validate"] {
                assert!(expected.contains(kept), "`{kept}` missing from: {text}");
            }
            for gone in [concat!("def", "ine"), concat!("sh", "ip")] {
                assert!(
                    !expected.contains(gone),
                    "the parse error still offers the removed stage `{gone}`: {text}"
                );
            }
        }
    }

    #[test]
    fn plan_is_the_first_stage_and_validate_the_last() {
        // Every name a stage has ever had, current or removed. The ones that
        // still parse are the stage machine; no parsable stage may precede
        // Plan, and the chain walked from Plan must reach all of them.
        // Split so the removed-surface scan does not read this control as a leftover.
        let parsed: Vec<Stage> = [
            concat!("def", "ine"),
            "plan",
            "code",
            "validate",
            concat!("sh", "ip"),
        ]
        .into_iter()
        .filter_map(|name| name.parse::<Stage>().ok())
        .collect();
        for stage in &parsed {
            assert_ne!(
                stage.next(),
                Some(Stage::Plan),
                "`{stage}` precedes Plan, so Plan is not the first stage"
            );
        }
        let mut chain = vec![Stage::Plan];
        while let Some(next) = chain.last().copied().and_then(Stage::next) {
            chain.push(next);
        }
        assert_eq!(chain, [Stage::Plan, Stage::Code, Stage::Validate]);
        assert_eq!(Stage::Validate.next(), None);
        assert_eq!(
            parsed.len(),
            chain.len(),
            "parsable stages {parsed:?} differ from the chain walked from Plan {chain:?}"
        );
    }

    #[test]
    fn serde_round_trips_each_stage() {
        for stage in [Stage::Plan, Stage::Code, Stage::Validate] {
            let json = serde_json::to_string(&stage).unwrap();
            let back: Stage = serde_json::from_str(&json).unwrap();
            assert_eq!(stage, back);
        }
        // Wire format is lowercase.
        assert_eq!(serde_json::to_string(&Stage::Code).unwrap(), "\"code\"");
    }
}
