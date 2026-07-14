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

/// The Ship stage's dedicated prompt.
///
/// Headless-safety rationale: `/gsd-ship`'s own `optional_review` step is an
/// interactive `AskUserQuestion` with undefined behavior under
/// `--dangerously-skip-permissions` (RESEARCH Pitfall 2). Rather than relying
/// on that step being skipped, this prompt sidesteps it entirely: the agent
/// runs `/gsd-code-review {N}` first (non-interactive; writes `REVIEW.md`
/// with severity-classified findings), and MUST NOT run `/gsd-ship {N}` at
/// all if `REVIEW.md` contains any Critical-severity finding — instead it
/// reports a `review:`-prefixed failure. Only a clean (no-Critical) review
/// proceeds to `/gsd-ship {N}`. The `review:` reason prefix is the
/// ReviewFailed contract that `handle_ship_failure` matches (trimmed,
/// case-folded) to loop back to Code with `AuditFix`.
fn ship_stage_prompt(phase: u32) -> String {
    let code_review = format!("/gsd-code-review {phase}");
    let ship = format!("/gsd-ship {phase}");
    format!(
        "Run the Ship stage in two steps:\n\
        \n\
        1. Run `{code_review}` (non-interactive). This writes a `REVIEW.md` \
        artifact with severity-classified findings.\n\
        2. Check `REVIEW.md` for the Critical-severity gate:\n\
        \n\
        - If `REVIEW.md` contains ANY finding at Critical severity: do NOT \
        run `{ship}` at all. Your FINAL message must be exactly:\n\
        \n\
        DEVFLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"review: <short summary of the Critical findings>\"}}\n\
        \n\
        - If `REVIEW.md` has NO Critical-severity findings: run `{ship}` and \
        report the outcome via the normal completion protocol below.\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}

/// Build the prompt for a stage of a phase.
pub fn stage_prompt(stage: Stage, phase: u32) -> String {
    if stage == Stage::Ship {
        return ship_stage_prompt(phase);
    }
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
    fn ship_prompt_sequences_code_review_before_ship() {
        let prompt = stage_prompt(Stage::Ship, 13);
        let review_pos = prompt
            .find("/gsd-code-review 13")
            .expect("Ship prompt must run /gsd-code-review {N}");
        let ship_pos = prompt
            .find("/gsd-ship 13")
            .expect("Ship prompt must run /gsd-ship {N}");
        assert!(
            review_pos < ship_pos,
            "code-review must be sequenced before ship"
        );
    }

    #[test]
    fn ship_prompt_defines_critical_gate_and_review_failed_contract() {
        let prompt = stage_prompt(Stage::Ship, 13);
        assert!(
            prompt.contains("REVIEW.md"),
            "Ship prompt must reference the REVIEW.md artifact"
        );
        assert!(
            prompt.to_lowercase().contains("critical"),
            "Ship prompt must name the Critical-severity gate"
        );
        assert!(
            prompt.contains("do not run")
                || prompt.contains("do NOT run")
                || prompt.contains("DO NOT run"),
            "Ship prompt must instruct the agent not to run /gsd-ship on Critical findings"
        );
        assert!(
            prompt.contains("review:"),
            "Ship prompt must define the review: ReviewFailed reason convention"
        );
        assert!(prompt.contains("DEVFLOW_RESULT"));
    }

    #[test]
    fn non_ship_stage_prompts_are_unchanged_single_command_template() {
        let cases = [
            (Stage::Define, "/gsd-discuss-phase 9"),
            (Stage::Plan, "/gsd-plan-phase 9"),
            (Stage::Code, "/gsd-execute-phase 9"),
            (Stage::Validate, "/gsd-validate-phase 9"),
        ];
        for (stage, command) in cases {
            let prompt = stage_prompt(stage, 9);
            assert!(prompt.contains(command), "{stage} prompt missing {command}");
            assert!(prompt.contains("DEVFLOW_RESULT"));
            assert!(
                !prompt.contains("/gsd-code-review"),
                "{stage} prompt should not carry Ship-specific code-review sequencing"
            );
        }
    }

    #[test]
    fn fix_prompts_select_the_right_command() {
        assert!(fix_prompt(FixType::AuditFix, 11).contains("/gsd-audit-fix 11"));
        assert!(fix_prompt(FixType::GapsOnly, 11).contains("--gaps-only"));
        assert!(fix_prompt(FixType::AuditFix, 11).contains("DEVFLOW_RESULT"));
    }
}
