//! Stage-specific agent prompts.
//!
//! Prompts are minimal: each stage hands the agent its GSD slash command
//! (from [`Stage::gsd_command`]) and the `DEVFLOW_RESULT` completion contract.
//! There is no long instruction template — the GSD command carries the process,
//! and DevFlow only needs the structured completion marker back.

use crate::stage::Stage;
use std::path::Path;

const SHIP_REVIEW_ANGLES: &[&str] = &[
    "doc-accuracy cross-reference (do documented claims match source?)",
    "security / leaked-data (does anything commit secrets, session data, or telemetry?)",
    "CI/build correctness (can a failing step still report green?)",
    "external-state claims (does the diff claim merges, tags, or deletions that are not actually true?)",
    "one generalist deep pass",
];

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
fn ship_stage_prompt(phase: u32, review_angles: &[String]) -> String {
    let code_review = format!("/gsd-code-review {phase}");
    let ship = format!("/gsd-ship {phase}");
    let review_angles = review_angles
        .iter()
        .map(|angle| format!("- {angle}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Run the Ship stage in two steps:\n\
        \n\
        1. Run `{code_review}` (non-interactive). This writes a `REVIEW.md` \
        artifact with severity-classified findings. Review at high depth from \
        every angle below:\n\
        \n\
        {review_angles}\n\
        \n\
        If your harness supports parallel finder subagents, dispatch one per \
        angle; otherwise run each angle as a focused sequential pass. Merge \
        and deduplicate every angle's findings into one `REVIEW.md`.\n\
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

/// The Validate stage's dedicated prompt.
///
/// 13b verdict-vs-ran: `status` only reports whether the stage's task (running
/// `/gsd-validate-phase {N}`) completed — it says nothing about whether
/// validation itself passed. This prompt REQUIRES a distinct `verdict` field
/// so `advance()`'s Validate arm can tell "the agent ran validation" apart
/// from "validation passed," and never advances to Ship on a bare `status:
/// success` for this stage.
fn validate_stage_prompt(phase: u32) -> String {
    let command = gsd_command_for(Stage::Validate, phase);
    format!(
        "Run the GSD workflow command for this stage:\n\n    {command}\n\n\
        ## Completion Protocol (REQUIRED)\n\
        \n\
        When all work is done, your FINAL message must be exactly one of:\n\
        \n\
        DEVFLOW_RESULT: {{\"status\": \"success\", \"verdict\": \"pass\"}}\n\
        \n\
        if validation found NO gaps, or:\n\
        \n\
        DEVFLOW_RESULT: {{\"status\": \"success\", \"verdict\": \"gaps\"}}\n\
        \n\
        if validation found gaps that still need fixing. The `verdict` field \
        is REQUIRED for this stage — it is distinct from `status` (which only \
        reports whether the validation task itself completed) and MUST be \
        exactly the lowercase string `pass` or `gaps`.\n\
        \n\
        If something prevents completion:\n\
        \n\
        DEVFLOW_RESULT: {{\"status\": \"failed\", \"reason\": \"specific explanation\"}}\n\
        \n\
        DevFlow reads this line to decide whether the stage succeeded. \
        Output nothing after it."
    )
}

/// The Define and Plan stages' idempotency contract.
///
/// Headless-safety rationale (13-06 dogfood finding, Codex leg): GSD's
/// discuss-phase demands an interactive "Overwrite/Append/Cancel" decision
/// when the phase's CONTEXT.md already exists, and headless Codex cannot
/// answer it (`request_user_input is unavailable`) — the stage would fail on
/// every retry, forever. When the stage's deliverable already exists, the
/// stage's work is done: re-running it must be a no-op success, not an
/// interactive dead end. This is idempotency for a completed stage, NOT the
/// v1 skip-stage config flags removed by the 2026-06-19 architecture
/// decision — a stage with no pre-existing artifact still runs in full.
fn idempotent_stage_prompt(stage: Stage, phase: u32) -> String {
    let artifact = match stage {
        Stage::Define => "CONTEXT.md",
        _ => "PLAN.md",
    };
    let command = gsd_command_for(stage, phase);
    let padded = format!("{phase:02}");
    format!(
        "First check whether this stage's deliverable already exists:\n\
        \n\
        ls .planning/phases/{padded}-*/{padded}-*{artifact} 2>/dev/null\n\
        \n\
        - If it EXISTS: the stage's work is already done. Do NOT run the GSD \
        command, do NOT ask for input, and do NOT modify the existing \
        artifacts. Your FINAL message must be exactly:\n\
        \n\
        DEVFLOW_RESULT: {{\"status\": \"success\"}}\n\
        \n\
        - If it does NOT exist: run the GSD workflow command for this stage:\n\
        \n\
        \x20   {command}\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}

/// Build the prompt for a stage of a phase.
pub fn stage_prompt(stage: Stage, phase: u32) -> String {
    stage_prompt_with_project(stage, phase, None)
}

/// Build a stage prompt with project-local configuration applied.
///
/// The CLI uses this entry point after resolving the canonical project root;
/// library callers that have no project context keep using [`stage_prompt`]
/// and receive built-in defaults.
pub fn stage_prompt_for_project(stage: Stage, phase: u32, project_root: &Path) -> String {
    stage_prompt_with_project(stage, phase, Some(project_root))
}

fn stage_prompt_with_project(stage: Stage, phase: u32, project_root: Option<&Path>) -> String {
    if stage == Stage::Ship {
        let review_angles = project_root
            .and_then(crate::config::review_angles)
            .unwrap_or_else(|| {
                SHIP_REVIEW_ANGLES
                    .iter()
                    .map(|angle| (*angle).to_owned())
                    .collect()
            });
        return ship_stage_prompt(phase, &review_angles);
    }
    if stage == Stage::Validate {
        return validate_stage_prompt(phase);
    }
    if matches!(stage, Stage::Define | Stage::Plan) {
        return idempotent_stage_prompt(stage, phase);
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
    fn ship_prompt_includes_multi_angle_conditional_review() {
        let prompt = stage_prompt(Stage::Ship, 13);
        for angle in [
            "doc-accuracy cross-reference",
            "security / leaked-data",
            "CI/build correctness",
            "external-state claims",
            "generalist deep pass",
        ] {
            assert!(prompt.contains(angle), "Ship prompt missing angle: {angle}");
        }
        assert!(prompt.contains("parallel finder subagents"));
        assert!(prompt.contains("focused sequential pass"));
        assert!(prompt.contains("Merge and deduplicate"));
        assert!(prompt.contains("REVIEW.md"));
    }

    #[test]
    fn ship_prompt_uses_project_review_angle_override() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("devflow.toml"),
            "review_angles = [\"custom release evidence\", \"custom threat boundary\"]\n",
        )
        .unwrap();

        let prompt = stage_prompt_for_project(Stage::Ship, 13, dir.path());

        assert!(prompt.contains("custom release evidence"));
        assert!(prompt.contains("custom threat boundary"));
        assert!(!prompt.contains("doc-accuracy cross-reference"));
    }

    #[test]
    fn code_stage_prompt_is_unchanged_single_command_template() {
        // Validate is excluded here (Task 2, 13-05): it now gets its own
        // dedicated prompt requiring a verdict — see
        // `validate_stage_prompt_requires_verdict` below. Define and Plan
        // are excluded too (13-06 dogfood): they carry the idempotency
        // contract — see `define_and_plan_prompts_are_idempotent` below.
        let prompt = stage_prompt(Stage::Code, 9);
        assert!(prompt.contains("/gsd-execute-phase 9"));
        assert!(prompt.contains("DEVFLOW_RESULT"));
        assert!(
            !prompt.contains("/gsd-code-review"),
            "Code prompt should not carry Ship-specific code-review sequencing"
        );
        assert!(
            !prompt.contains("already exists"),
            "Code prompt should not carry the Define/Plan idempotency contract"
        );
    }

    /// 13-06 dogfood regression (Codex leg): GSD's discuss-phase demands an
    /// interactive decision when CONTEXT.md already exists, which headless
    /// Codex can never answer — Define/Plan must no-op with success when
    /// their deliverable pre-exists.
    #[test]
    fn define_and_plan_prompts_are_idempotent() {
        let cases = [
            (Stage::Define, "/gsd-discuss-phase 9", "09-*CONTEXT.md"),
            (Stage::Plan, "/gsd-plan-phase 9", "09-*PLAN.md"),
        ];
        for (stage, command, artifact_glob) in cases {
            let prompt = stage_prompt(stage, 9);
            assert!(prompt.contains(command), "{stage} prompt missing {command}");
            assert!(
                prompt.contains(artifact_glob),
                "{stage} prompt must check for its pre-existing artifact"
            );
            assert!(
                prompt.contains("Do NOT run the GSD command"),
                "{stage} prompt must no-op when the artifact exists"
            );
            assert!(
                prompt.contains("do NOT ask for input"),
                "{stage} prompt must forbid interactive input"
            );
            assert!(prompt.contains("DEVFLOW_RESULT"));
        }
    }

    #[test]
    fn validate_stage_prompt_requires_verdict() {
        let prompt = stage_prompt(Stage::Validate, 13);
        assert!(
            prompt.contains("/gsd-validate-phase 13"),
            "Validate prompt missing its GSD command"
        );
        assert!(
            prompt.contains("\"verdict\": \"pass\""),
            "Validate prompt must name the exact lowercase pass verdict"
        );
        assert!(
            prompt.contains("\"verdict\": \"gaps\""),
            "Validate prompt must name the exact lowercase gaps verdict"
        );
        assert!(prompt.contains("REQUIRED"));
        assert!(prompt.contains("DEVFLOW_RESULT"));
    }

    #[test]
    fn fix_prompts_select_the_right_command() {
        assert!(fix_prompt(FixType::AuditFix, 11).contains("/gsd-audit-fix 11"));
        assert!(fix_prompt(FixType::GapsOnly, 11).contains("--gaps-only"));
        assert!(fix_prompt(FixType::AuditFix, 11).contains("DEVFLOW_RESULT"));
    }
}
