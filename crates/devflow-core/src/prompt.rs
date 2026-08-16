//! Stage-specific agent prompts.
//!
//! Prompts are minimal: each stage hands the agent its GSD slash command
//! (from [`Stage::gsd_command`]) and the `DEVFLOW_RESULT` completion contract.
//! There is no long instruction template — the GSD command carries the process,
//! and DevFlow only needs the structured completion marker back.

use crate::phase_id::PhaseId;
use crate::stage::Stage;
use std::path::Path;

const SHIP_REVIEW_ANGLES: &[&str] = &[
    "doc-accuracy cross-reference (do documented claims match source?)",
    "security / leaked-data (does anything commit secrets, session data, or telemetry?)",
    "CI/build correctness (can a failing step still report green?)",
    "external-state claims (does the diff claim merges, tags, or deletions that are not actually true?)",
    "one generalist deep pass",
];

/// The token that stops GSD from wiping `workflow._auto_chain_active` before
/// it is read (35.1-01, RESEARCH Pitfall 1).
///
/// `execute-phase.md:161-165` clears that flag at the top of every invocation
/// whose `$ARGUMENTS` does not carry this token. DevFlow sets the flag in the
/// monitor immediately before launching the child, so without this the flag is
/// wiped by the very command it was set for and `checkpoint_handling` never
/// auto-approves anything.
///
/// **The token alone enables nothing.** GSD's `check auto-mode` reads
/// `.planning/config.json` and nothing else — a full grep of
/// `execute-phase.md` for `--auto` / `--chain` / `AUTO_CHAIN` / `auto_advance`
/// returns exactly three hits, all inside that sync-clear block. So within
/// `execute-phase.md` this token's only effect is to skip the clear. The
/// mode gate that decides whether checkpoints may actually be auto-approved
/// lives on the config write, in `pipeline_launch::auto_chain_flag_eligible`.
///
/// Named rather than inlined so the three command strings that must carry it
/// (Code, and `fix_prompt`'s two `execute-phase` arms) cannot drift apart, and
/// so a reader of any one of them can find this explanation.
const AUTO_CHAIN_PRESERVING_FLAG: &str = "--auto";

/// The completion contract every agent must honor as its final message.
pub const COMPLETION_PROTOCOL: &str = "\
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

/// The data a stage wants rendered, with NO agent-specific syntax.
///
/// This is the de-Claude-ification artifact (999.31 / 37-01): the old
/// `Stage::gsd_command()` returned a `/gsd-*` slash-command string that
/// `prompt.rs` interpolated identically for every agent. `StageIntent` instead
/// carries the stage's *data* (phase, fix kind, review angles), and each
/// adapter's `render_prompt` turns that data into its own instruction — Claude
/// and OpenCode render the legacy slash-command text byte-for-byte, Codex
/// renders a Codex-native instruction with no `/gsd-*` string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageIntent {
    Define {
        phase: PhaseId,
    },
    Plan {
        phase: PhaseId,
    },
    Code {
        phase: PhaseId,
        fix: Option<FixType>,
    },
    Validate {
        phase: PhaseId,
    },
    Ship {
        phase: PhaseId,
        review_angles: Vec<String>,
    },
}

impl StageIntent {
    /// The stage this intent drives.
    pub fn stage(&self) -> Stage {
        match self {
            StageIntent::Define { .. } => Stage::Define,
            StageIntent::Plan { .. } => Stage::Plan,
            StageIntent::Code { .. } => Stage::Code,
            StageIntent::Validate { .. } => Stage::Validate,
            StageIntent::Ship { .. } => Stage::Ship,
        }
    }

    /// Build the intent for a stage with no fix and default review angles
    /// (callers that need a project-local review-angle override use
    /// [`StageIntent::for_stage_in_project`]).
    pub fn for_stage(stage: Stage, phase: PhaseId) -> Self {
        Self::for_stage_in_project(stage, phase, None)
    }

    /// Build the intent for a stage, resolving project-local review angles.
    pub fn for_stage_in_project(stage: Stage, phase: PhaseId, project_root: Option<&Path>) -> Self {
        match stage {
            Stage::Define => StageIntent::Define { phase },
            Stage::Plan => StageIntent::Plan { phase },
            Stage::Code => StageIntent::Code { phase, fix: None },
            Stage::Validate => StageIntent::Validate { phase },
            Stage::Ship => {
                let review_angles = project_root
                    .and_then(crate::config::review_angles)
                    .unwrap_or_else(|| {
                        SHIP_REVIEW_ANGLES
                            .iter()
                            .map(|angle| (*angle).to_owned())
                            .collect()
                    });
                StageIntent::Ship {
                    phase,
                    review_angles,
                }
            }
        }
    }
}

/// A fix variant used when looping Code ↔ Validate.
///
/// `#[non_exhaustive]`: operator decision, 2026-08-04. This enum is public in
/// the published `devflow-core` crate, so adding a variant is already a
/// breaking change for any external crate matching on it exhaustively — this
/// release (`FullExecute`, added for D-01) already pays that cost. Paying for
/// `#[non_exhaustive]` at the same time makes every later variant addition
/// additive instead of breaking again, the same reasoning `State` records for
/// its own `#[non_exhaustive]` (`state.rs:30-31`). Verified empirically before
/// applying: the only `match` over a `FixType` value anywhere in the
/// workspace is `fix_prompt` below, which lives in this crate and is
/// therefore unaffected by the attribute — no wildcard arm is needed, here or
/// anywhere else in the workspace, and none should be added to `fix_prompt`
/// itself (see its doc comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FixType {
    /// Run the GSD audit-fix pipeline over review findings.
    AuditFix,
    /// Re-run execution targeting only the gaps left by validation.
    GapsOnly,
    /// Re-run the phase's remaining plans with the plain, unflagged
    /// `/gsd-execute-phase {N}` command, because the phase is mid-arc rather
    /// than defective — D-01 (33-CONTEXT.md): a phase with no
    /// `{N}-VERIFICATION.md` yet has not been judged, so `--gaps-only` would
    /// match zero plans and gate unresolvably.
    FullExecute,
}

/// Substitute the `{N}` phase placeholder in a GSD command string.
fn gsd_command_for(stage: Stage, phase: PhaseId) -> String {
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
fn ship_stage_prompt(phase: PhaseId, review_angles: &[String]) -> String {
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
fn validate_stage_prompt(phase: PhaseId) -> String {
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

/// The Plan stage's idempotency contract.
///
/// Headless-safety rationale (13-06 dogfood finding, Codex leg): GSD's
/// plan-phase demands an interactive "Overwrite/Append/Cancel" decision
/// when the phase's PLAN.md already exists, and headless Codex cannot
/// answer it (`request_user_input is unavailable`) — the stage would fail on
/// every retry, forever. When the stage's deliverable already exists, the
/// stage's work is done: re-running it must be a no-op success, not an
/// interactive dead end. This is idempotency for a completed stage, NOT the
/// v1 skip-stage config flags removed by the 2026-06-19 architecture
/// decision — a stage with no pre-existing artifact still runs in full.
///
/// D-14 update: Define used to share this branch (dispatched by a `stage`
/// parameter), but its missing-artifact arm ran an interactive interview
/// command that cannot be answered headlessly. That branch was deleted
/// rather than made conditional — see [`define_stage_prompt`] for Define's
/// actual (always-no-op) contract. This function now serves only Plan.
fn idempotent_stage_prompt(phase: PhaseId) -> String {
    let artifact = "PLAN.md";
    let command = gsd_command_for(Stage::Plan, phase);
    let padded = phase.padded();
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

/// The Define stage's dedicated prompt (D-14).
///
/// Headless-safety rationale: `idempotent_stage_prompt`'s missing-artifact
/// arm used to run an interactive interview command for Define — one that
/// hangs or errors under `claude -p` with no operator present to answer it
/// (T-28-08). D-14 settles the fix as deletion, not disambiguation: the
/// Define stage never runs that command in a DevFlow launch, whether or not
/// CONTEXT.md already exists. The operator decides whether to run the
/// interview before invoking `devflow start`; DevFlow makes no runtime
/// accommodation for that choice.
fn define_stage_prompt(phase: PhaseId) -> String {
    format!(
        "This is the Define stage of a headless DevFlow run for phase {phase}.\n\
        \n\
        There is no agent work to perform here. Whether or not this phase's \
        CONTEXT.md already exists, you must NOT run an interactive \
        discuss-phase or interview command, and you must NOT ask for input \
        — this run is headless and no operator is available to answer \
        interactive questions. Do NOT modify any existing planning \
        artifacts.\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}

/// Build the prompt for a stage of a phase.
pub fn stage_prompt(stage: Stage, phase: PhaseId) -> String {
    stage_prompt_with_project(stage, phase, None)
}

/// Build a stage prompt with project-local configuration applied.
///
/// The CLI uses this entry point after resolving the canonical project root;
/// library callers that have no project context keep using [`stage_prompt`]
/// and receive built-in defaults.
pub fn stage_prompt_for_project(stage: Stage, phase: PhaseId, project_root: &Path) -> String {
    stage_prompt_with_project(stage, phase, Some(project_root))
}

/// The Code stage's dedicated prompt.
///
/// The Code arm ONLY carries [`AUTO_CHAIN_PRESERVING_FLAG`] (D-04/D-05, 35.1-01
/// Pitfall 1): `execute-phase.md` wipes `workflow._auto_chain_active` at the top
/// of every invocation whose `$ARGUMENTS` lacks this token, which would clear
/// the flag DevFlow just set before `checkpoint_handling` ever reads it.
fn code_stage_prompt(phase: PhaseId) -> String {
    let command = format!(
        "{} {AUTO_CHAIN_PRESERVING_FLAG}",
        gsd_command_for(Stage::Code, phase)
    );
    format!(
        "Run the GSD workflow command for this stage:\n\n    {command}\n\n\
        ## Advisory incremental self-review\n\
        \n\
        After each plan or wave lands, perform a quick, shallow self-check \
        for doc accuracy, leaked data, CI/build correctness, and \
        external-state claims. Record any drift in the working output and \
        continue execution; the authoritative review happens during Ship. \
        This check must not pause execution or request human input.\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}

/// Render a [`StageIntent`] as the legacy Claude/OpenCode slash-command text.
///
/// This is the byte-identical renderer: Claude and OpenCode produce exactly
/// what `stage_prompt` produced before the migration (CONTEXT D-01 zero
/// regression). It lives here — not in the adapters — so the two agents cannot
/// drift apart, and the per-stage snapshot tests pin it.
pub fn render_claude_style(intent: &StageIntent) -> String {
    match intent {
        StageIntent::Define { phase } => define_stage_prompt(*phase),
        StageIntent::Plan { phase } => idempotent_stage_prompt(*phase),
        StageIntent::Code { phase, fix: None } => code_stage_prompt(*phase),
        StageIntent::Code {
            phase,
            fix: Some(fix),
        } => fix_prompt(*fix, *phase),
        StageIntent::Validate { phase } => validate_stage_prompt(*phase),
        StageIntent::Ship {
            phase,
            review_angles,
        } => ship_stage_prompt(*phase, review_angles),
    }
}

/// Render a [`StageIntent`] as a workflow-reference instruction for agents that
/// cannot receive the legacy `/gsd-*` slash command (Codex, Pi). The instruction
/// points at the GSD workflow file to follow, carries the `--auto` token where
/// the workflow requires it, and states the completion contract. Contains NO
/// GSD slash command.
pub fn render_workflow_style(intent: &StageIntent, agent_label: &str) -> String {
    let (workflow, args) = match intent {
        StageIntent::Define { phase } => ("discuss-phase.md", format!("phase {phase}")),
        StageIntent::Plan { phase } => ("plan-phase.md", format!("phase {phase}")),
        StageIntent::Code { phase, fix } => match fix {
            Some(FixType::AuditFix) => ("audit-fix.md", format!("phase {phase}")),
            Some(FixType::GapsOnly) => (
                "execute-phase.md",
                format!("phase {phase} --auto --gaps-only"),
            ),
            Some(FixType::FullExecute) | None => {
                ("execute-phase.md", format!("phase {phase} --auto"))
            }
        },
        StageIntent::Validate { phase } => ("validate-phase.md", format!("phase {phase}")),
        StageIntent::Ship { phase, .. } => ("ship.md", format!("phase {phase}")),
    };
    format!(
        "You are executing one stage of a headless DevFlow run as {agent_label}.\n\n\
        Read the GSD workflow file at $HOME/.codex/gsd-core/workflows/{workflow} and follow it for {args}. \
        Do not run GSD slash-command instructions; execute the workflow file's instructions directly. \
        The `--auto` flag, where present, is part of the workflow invocation and must be preserved verbatim.\n\n\
        {COMPLETION_PROTOCOL}"
    )
}

fn stage_prompt_with_project(stage: Stage, phase: PhaseId, project_root: Option<&Path>) -> String {
    render_claude_style(&StageIntent::for_stage_in_project(
        stage,
        phase,
        project_root,
    ))
}

/// The synthesized instruction sent into a resumed Claude session when a
/// confirmed human-blocking checkpoint has nobody available to answer it
/// (D-03, 28-CONTEXT.md): DevFlow's default, unconditional policy — no flag,
/// no config toggle — is for the agent to resolve the checkpoint itself,
/// using its own judgment, and record why.
///
/// Deliberately deterministic: no timestamp, no random content, no varying
/// state. Two calls for the same `phase` produce byte-identical strings, so
/// the `checkpoint_auto_decided` audit event (D-07, plan 28-03) can quote
/// this exact instruction without churning on every resume. `phase` is
/// included only for operator legibility in the captured stdout — the
/// instruction's meaning does not depend on it.
pub fn checkpoint_auto_decide_prompt(phase: PhaseId) -> String {
    format!(
        "This is phase {phase} of a headless DevFlow run. You previously \
        stopped at a human-blocking checkpoint, but no human operator is \
        available to answer it — this run is unattended, and none is \
        coming. DevFlow's policy is for you to resolve the checkpoint \
        yourself, using your own best judgment, and continue the work. You \
        MUST record your reasoning for the decision you made in your final \
        message, so the decision is auditable after the fact.\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}

/// Build a fix prompt used on Code → Validate loop-backs.
///
/// Both arms that dispatch to `execute-phase` carry
/// [`AUTO_CHAIN_PRESERVING_FLAG`], for the same reason the Code prompt does:
/// they reach `execute-phase.md`'s sync-clear step, and without the token that
/// step wipes the chain flag DevFlow just set. The `--gaps-only` loop is
/// named explicitly by ROADMAP criterion 1 — a fix pass gets exactly the same
/// treatment as the first Code pass, or the phase's unattended behaviour
/// changes the moment validation reports a gap.
///
/// `AuditFix` is deliberately left alone: it routes to `/gsd-audit-fix`, never
/// reaches `execute-phase.md`, and so never meets the sync-clear step.
///
/// Flag ORDER within the command string does not matter — GSD extracts
/// `--`-prefixed tokens position-independently
/// (`references/phase-argument-parsing.md`).
pub fn fix_prompt(fix_type: FixType, phase: PhaseId) -> String {
    let command = match fix_type {
        FixType::AuditFix => format!("/gsd-audit-fix {phase}"),
        FixType::GapsOnly => {
            format!("/gsd-execute-phase {phase} --gaps-only {AUTO_CHAIN_PRESERVING_FLAG}")
        }
        FixType::FullExecute => {
            format!("/gsd-execute-phase {phase} {AUTO_CHAIN_PRESERVING_FLAG}")
        }
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
        // Define is excluded here (D-14): its prompt never contains its GSD
        // command — see `define_prompt_never_invokes_discuss_phase` below.
        let cases = [
            (Stage::Plan, "/gsd-plan-phase 11"),
            (Stage::Code, "/gsd-execute-phase 11"),
            (Stage::Validate, "/gsd-validate-phase 11"),
            (Stage::Ship, "/gsd-ship 11"),
        ];
        for (stage, command) in cases {
            let prompt = stage_prompt(stage, PhaseId::new(11));
            assert!(prompt.contains(command), "{stage} prompt missing {command}");
            assert!(prompt.contains("DEVFLOW_RESULT"));
        }
    }

    #[test]
    fn phase_placeholder_is_substituted() {
        assert!(stage_prompt(Stage::Code, PhaseId::new(7)).contains("/gsd-execute-phase 7"));
        assert!(!stage_prompt(Stage::Code, PhaseId::new(7)).contains("{N}"));
    }

    #[test]
    fn ship_prompt_sequences_code_review_before_ship() {
        let prompt = stage_prompt(Stage::Ship, PhaseId::new(13));
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
        let prompt = stage_prompt(Stage::Ship, PhaseId::new(13));
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
        let prompt = stage_prompt(Stage::Ship, PhaseId::new(13));
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

        let prompt = stage_prompt_for_project(Stage::Ship, PhaseId::new(13), dir.path());

        assert!(prompt.contains("custom release evidence"));
        assert!(prompt.contains("custom threat boundary"));
        assert!(!prompt.contains("doc-accuracy cross-reference"));
    }

    #[test]
    fn code_stage_prompt_is_unchanged_single_command_template() {
        // Validate is excluded here (Task 2, 13-05): it now gets its own
        // dedicated prompt requiring a verdict — see
        // `validate_stage_prompt_requires_verdict` below. Define and Plan
        // are excluded too: Plan carries the idempotency contract (see
        // `plan_prompt_is_idempotent` below); Define carries its own D-14
        // always-no-op contract (see `define_prompt_never_invokes_discuss_phase`
        // below).
        let prompt = stage_prompt(Stage::Code, PhaseId::new(9));
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
        assert!(prompt.contains("Advisory incremental self-review"));
        for angle in [
            "doc accuracy",
            "leaked data",
            "CI/build correctness",
            "external-state claims",
        ] {
            assert!(prompt.contains(angle), "Code prompt missing angle: {angle}");
        }
        assert!(!prompt.contains("AskUserQuestion"));
        assert!(!prompt.contains("request_user_input"));
    }

    /// 13-06 dogfood regression (Codex leg), Plan half only after the D-14
    /// split: GSD's plan-phase demands an interactive decision when PLAN.md
    /// already exists, which headless Codex can never answer — Plan must
    /// no-op with success when its deliverable pre-exists. See T-28-09.
    #[test]
    fn plan_prompt_is_idempotent() {
        let prompt = stage_prompt(Stage::Plan, PhaseId::new(9));
        assert!(
            prompt.contains("/gsd-plan-phase 9"),
            "Plan prompt missing /gsd-plan-phase 9"
        );
        assert!(
            prompt.contains("09-*PLAN.md"),
            "Plan prompt must check for its pre-existing artifact"
        );
        assert!(
            prompt.contains("Do NOT run the GSD command"),
            "Plan prompt must no-op when the artifact exists"
        );
        assert!(
            prompt.contains("do NOT ask for input"),
            "Plan prompt must forbid interactive input"
        );
        assert!(prompt.contains("DEVFLOW_RESULT"));
    }

    /// D-14: the Define stage must never invoke the interactive
    /// discuss-phase command, whether or not CONTEXT.md exists — the branch
    /// that did so is deleted, not disambiguated. Regression guard for
    /// T-28-08 (a headless run has no operator to answer it).
    #[test]
    fn define_prompt_never_invokes_discuss_phase() {
        let prompt = stage_prompt(Stage::Define, PhaseId::new(9));
        assert!(
            !prompt.contains("/gsd-discuss-phase"),
            "Define prompt must never invoke the interactive discuss-phase command (D-14)"
        );
        assert!(
            prompt.contains("must NOT run") || prompt.contains("do NOT run"),
            "Define prompt must forbid running an interactive interview headlessly"
        );
        assert!(
            prompt.contains("do NOT ask for input") || prompt.contains("must NOT ask for input"),
            "Define prompt must forbid requesting input"
        );
        assert!(
            prompt.to_lowercase().contains("modify"),
            "Define prompt must forbid modifying existing planning artifacts"
        );
        assert!(prompt.contains("DEVFLOW_RESULT"));
    }

    #[test]
    fn validate_stage_prompt_requires_verdict() {
        let prompt = stage_prompt(Stage::Validate, PhaseId::new(13));
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
        assert!(fix_prompt(FixType::AuditFix, PhaseId::new(11)).contains("/gsd-audit-fix 11"));
        assert!(fix_prompt(FixType::GapsOnly, PhaseId::new(11)).contains("--gaps-only"));
        assert!(fix_prompt(FixType::AuditFix, PhaseId::new(11)).contains("DEVFLOW_RESULT"));

        // D-01: FullExecute renders the plain, unflagged execute command.
        let full_execute_prompt = fix_prompt(FixType::FullExecute, PhaseId::new(11));
        assert!(full_execute_prompt.contains("/gsd-execute-phase 11"));
        // Negative control: without this, FullExecute's command string would
        // just be a substring of GapsOnly's — this proves the two are
        // actually distinguishable, not that FullExecute merely contains
        // GapsOnly's prefix.
        assert!(!full_execute_prompt.contains("--gaps-only"));
    }

    /// The flag-preserving token belongs on exactly the command strings that
    /// reach `execute-phase.md`'s sync-clear step, and nowhere else.
    ///
    /// All three `FixType` arms are asserted, present AND absent, so this test
    /// distinguishes "added where it belongs" from "added everywhere" — the
    /// same habit `fix_prompts_select_the_right_command` above already uses for
    /// `--gaps-only`.
    #[test]
    fn fix_prompts_carry_the_chain_flag_token_only_where_it_reaches_execute_phase() {
        let phase = PhaseId::new(11);

        assert!(
            fix_prompt(FixType::GapsOnly, phase).contains(AUTO_CHAIN_PRESERVING_FLAG),
            "the --gaps-only fix loop reaches execute-phase.md, so it meets the \
             sync-clear step and needs the token exactly as the first Code pass does"
        );
        assert!(
            fix_prompt(FixType::FullExecute, phase).contains(AUTO_CHAIN_PRESERVING_FLAG),
            "the full-execute loop-back reaches execute-phase.md too"
        );
        assert!(
            !fix_prompt(FixType::AuditFix, phase).contains(AUTO_CHAIN_PRESERVING_FLAG),
            "audit-fix routes to /gsd-audit-fix and never reaches execute-phase.md, \
             so it never meets the sync-clear step the token exists to skip"
        );
    }

    /// The first Code pass and the fix loop must be treated identically —
    /// ROADMAP criterion 1 names the fix loop explicitly.
    #[test]
    fn the_code_prompt_carries_the_chain_flag_token() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(11));
        assert!(prompt.contains(&format!(
            "/gsd-execute-phase 11 {AUTO_CHAIN_PRESERVING_FLAG}"
        )));
    }

    /// Criterion 3a / D-04: the Plan prompt must NEVER carry the token.
    ///
    /// The flag that would enable checkpoint auto-approval at Plan is the same
    /// flag that makes `plan-phase.md` chain into `execute-phase.md`
    /// (`plan-phase.md:1564`) — which double-executes the Code stage and
    /// misattributes its commits. This is ROADMAP criterion 3, and it is why
    /// the token is appended inside the `Stage::Code` arm rather than in
    /// `gsd_command_for`, which Plan shares.
    #[test]
    fn the_plan_prompt_never_carries_the_chain_flag_token() {
        let plan = stage_prompt(Stage::Plan, PhaseId::new(11));
        assert!(
            !plan.contains(AUTO_CHAIN_PRESERVING_FLAG),
            "the Plan prompt must not chain into execute-phase (D-04)"
        );
        // Negative control: the Plan prompt DOES carry its own command, so the
        // assertion above is about the token and not about an empty string.
        assert!(plan.contains("/gsd-plan-phase 11"));
    }

    /// D-03/D-07 (28-03): the audit event quotes this instruction verbatim,
    /// so it must be byte-identical across calls for the same phase — no
    /// timestamp, no random content that would churn the recorded string.
    #[test]
    fn checkpoint_auto_decide_prompt_is_deterministic() {
        assert_eq!(
            checkpoint_auto_decide_prompt(PhaseId::new(28)),
            checkpoint_auto_decide_prompt(PhaseId::new(28))
        );
    }

    #[test]
    fn checkpoint_auto_decide_prompt_terminates_with_completion_protocol() {
        let prompt = checkpoint_auto_decide_prompt(PhaseId::new(28));
        assert!(
            prompt.ends_with(COMPLETION_PROTOCOL),
            "the resumed session's exit must still be parseable by the same \
             Layer 1 path as any other stage"
        );
        assert!(prompt.contains("DEVFLOW_RESULT"));
    }

    #[test]
    fn checkpoint_auto_decide_prompt_states_no_operator_judgment_and_record_reasoning() {
        let prompt = checkpoint_auto_decide_prompt(PhaseId::new(28)).to_lowercase();
        assert!(
            prompt.contains("no human operator") || prompt.contains("nobody"),
            "must state plainly that no operator is available"
        );
        assert!(
            prompt.contains("judgment") || prompt.contains("judgement"),
            "must instruct the agent to use its own judgment"
        );
        assert!(
            prompt.contains("record") && prompt.contains("reasoning"),
            "must require recording the reasoning in the final message, since \
             this is the ONLY record of what was decided (D-07)"
        );
    }

    #[test]
    fn checkpoint_auto_decide_prompt_substitutes_phase_for_legibility() {
        assert!(checkpoint_auto_decide_prompt(PhaseId::new(42)).contains("phase 42"));
    }
}
