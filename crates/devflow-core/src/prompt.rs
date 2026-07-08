//! Stage-specific agent prompts.
//!
//! Prompts are minimal: each stage hands the agent its GSD slash command
//! (from [`Stage::gsd_command`]) and the `DEVFLOW_RESULT` completion contract.
//! There is no long instruction template — the GSD command carries the process,
//! and DevFlow only needs the structured completion marker back.

use crate::stage::Stage;

/// The completion contract every agent must honor as its final message.
const COMPLETION_PROTOCOL: &str = "\
## Completion Protocol (REQUIRED)\n\
\n\
When all work is done, your FINAL message must be exactly:\n\
\n\
DEVFLOW_RESULT: {\"status\": \"success\"}\n\
\n\
If something prevents completion:\n\
\n\
DEVFLOW_RESULT: {\"status\": \"failed\", \"reason\": \"specific explanation\"}\n\
\n\
DevFlow reads this line to decide whether the stage succeeded. \
Output nothing after it.";

/// A fix variant used when looping Code ↔ Validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixType {
    /// Run the GSD audit-fix pipeline over review findings.
    AuditFix,
    /// Re-run execution targeting only the gaps left by validation.
    GapsOnly,
}

/// Substitute the `{N}` phase placeholder in a GSD command string.
fn gsd_command_for(stage: Stage, phase: u32) -> String {
    stage.gsd_command().replace("{N}", &phase.to_string())
}

/// Build the prompt for a stage of a phase.
pub fn stage_prompt(stage: Stage, phase: u32) -> String {
    let command = gsd_command_for(stage, phase);
    format!(
        "Run the GSD workflow command for this stage:\n\n    {command}\n\n{COMPLETION_PROTOCOL}"
    )
}

/// Build a fix prompt used on Code → Validate loop-backs.
pub fn fix_prompt(fix_type: FixType, phase: u32) -> String {
    let command = match fix_type {
        FixType::AuditFix => format!("/gsd-audit-fix {phase}"),
        FixType::GapsOnly => format!("/gsd-execute-phase {phase} --gaps-only"),
    };
    format!(
        "Validation reported issues. Run the fix command for this loop:\n\n    {command}\n\n{COMPLETION_PROTOCOL}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_stage_prompt_carries_its_gsd_command_and_marker() {
        let cases = [
            (Stage::Define, "/gsd-discuss-phase 11"),
            (Stage::Plan, "/gsd-plan-phase 11"),
            (Stage::Code, "/gsd-execute-phase 11"),
            (Stage::Validate, "/gsd-validate-phase 11"),
            (Stage::Ship, "/gsd-ship 11"),
        ];
        for (stage, command) in cases {
            let prompt = stage_prompt(stage, 11);
            assert!(prompt.contains(command), "{stage} prompt missing {command}");
            assert!(prompt.contains("DEVFLOW_RESULT"));
        }
    }

    #[test]
    fn phase_placeholder_is_substituted() {
        assert!(stage_prompt(Stage::Code, 7).contains("/gsd-execute-phase 7"));
        assert!(!stage_prompt(Stage::Code, 7).contains("{N}"));
    }

    #[test]
    fn fix_prompts_select_the_right_command() {
        assert!(fix_prompt(FixType::AuditFix, 11).contains("/gsd-audit-fix 11"));
        assert!(fix_prompt(FixType::GapsOnly, 11).contains("--gaps-only"));
        assert!(fix_prompt(FixType::AuditFix, 11).contains("DEVFLOW_RESULT"));
    }
}
