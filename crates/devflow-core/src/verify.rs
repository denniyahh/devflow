//! Explicitly operator-approved external post-condition verification.
//!
//! Commands are discovered only from PLAN.md YAML frontmatter. Because those
//! files are agent-writable, execution additionally requires the parent
//! process's [`TRUST_EXTERNAL_VERIFY_ENV`] authorization.

use crate::phase_id::PhaseId;
use std::path::{Path, PathBuf};

/// Explicit operator-owned approval for executing PLAN-declared shell.
pub const TRUST_EXTERNAL_VERIFY_ENV: &str = "DEVFLOW_TRUST_EXTERNAL_VERIFY";

/// Return the exact command bytes approved by the operator.
///
/// The value is a JSON string array. Comparing it to the commands reread
/// after Code closes the review-to-execution TOCTOU: a modified PLAN fails
/// closed instead of inheriting a blanket boolean authorization.
pub fn external_verification_approval() -> Option<Vec<String>> {
    let value = std::env::var(TRUST_EXTERNAL_VERIFY_ENV).ok()?;
    parse_external_verification_approval(&value)
}

fn parse_external_verification_approval(value: &str) -> Option<Vec<String>> {
    let commands = serde_json::from_str::<Vec<String>>(value).ok()?;
    (!commands.is_empty() && commands.iter().all(|command| !command.trim().is_empty()))
        .then_some(commands)
}

/// Discover a phase's declared plan files: `.planning/phases/{NN}-*/{NN}-*-PLAN.md`,
/// sorted. Returns an empty vec if the phase directory is missing or
/// unreadable — never panics, never errors.
///
/// Shared by [`external_verify_commands`] and
/// [`phase_has_blocking_human_checkpoint`] so PLAN.md discovery lives in
/// exactly one place; a second, slightly-different implementation would
/// drift the moment one is updated and the other isn't.
pub fn phase_plan_files(project_root: &Path, phase: PhaseId) -> Vec<PathBuf> {
    let phases_dir = project_root.join(".planning/phases");
    let phase_prefix = format!("{padded}-", padded = phase.padded());
    let plan_prefix = format!("{padded}-", padded = phase.padded());
    let mut plans = Vec::<PathBuf>::new();

    let Ok(phase_entries) = std::fs::read_dir(phases_dir) else {
        return Vec::new();
    };
    for phase_entry in phase_entries.flatten() {
        if !phase_entry
            .file_name()
            .to_string_lossy()
            .starts_with(&phase_prefix)
        {
            continue;
        }
        let Ok(plan_entries) = std::fs::read_dir(phase_entry.path()) else {
            continue;
        };
        plans.extend(plan_entries.flatten().filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name.starts_with(&plan_prefix) && name.ends_with("-PLAN.md")).then(|| entry.path())
        }));
    }
    plans.sort();
    plans
}

/// Return external verification commands declared by this phase's plans.
///
/// Only the first YAML frontmatter block is inspected. This intentionally
/// small parser recognizes the scalar shape established by Phase 16:
/// `external_verify: "command"` (single-quoted and unquoted scalars are also
/// accepted). Runtime captures and agent output are never read here.
pub fn external_verify_commands(project_root: &Path, phase: PhaseId) -> Vec<String> {
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|contents| command_from_frontmatter(&contents))
        .collect()
}

fn command_from_frontmatter(contents: &str) -> Option<String> {
    let mut lines = contents.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    for line in lines {
        let line = line.trim();
        if line == "---" {
            break;
        }
        let Some(value) = line.strip_prefix("external_verify:") else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let command = if value.starts_with('"') {
            serde_json::from_str::<String>(value).ok()
        } else if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
            Some(value[1..value.len() - 1].replace("''", "'"))
        } else {
            Some(value.to_owned())
        };
        return command.filter(|command| !command.trim().is_empty());
    }
    None
}

/// Return `true` if any plan declared for `phase` carries a task with the
/// human-blocking checkpoint gate attribute.
///
/// Reads declared plan content only — never any runtime capture or agent
/// output (D-02: no re-implementation of "what does the agent mean"). This
/// is the PRIMARY gate for the auto-decide path added in plan 28-03: an
/// agent cannot route itself into that path for a phase whose plans never
/// declared a `gate="blocking-human"` checkpoint, because this scan runs
/// first and is read from `.planning/phases/`. Those files are agent-writable
/// during Code, but that is the SAME trust boundary [`external_verify_commands`]
/// already documents and accepts — reused, not newly introduced.
///
/// **The CALLER is responsible for passing the root the phase's plans actually
/// live under** — the execution root (the worktree) in worktree mode, the
/// project root otherwise. `.planning/` is tracked content, so an in-flight
/// phase's `{N}-PLAN.md` sits on `feature/phase-{N}` inside the worktree and
/// is absent from the main checkout; a caller passing the main checkout gets a
/// silent `false` rather than an error (999.76, ROADMAP criterion 6). The
/// caller that does this correctly is `pipeline_launch.rs`'s
/// `Action::GateReview` arm, which resolves `state.worktree_path` first.
pub fn phase_has_blocking_human_checkpoint(project_root: &Path, phase: PhaseId) -> bool {
    const HUMAN_BLOCKING_GATE: &str = r#"gate="blocking-human""#;
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|contents| contents.contains(HUMAN_BLOCKING_GATE))
}

/// The two checkpoint markers GSD will not auto-approve in ANY mode, each
/// assembled from the attribute name, an equals sign and the quoted value
/// exactly as GSD writes it onto a `<task>` opening tag.
///
/// - `gate="blocking-human"` — `checkpoints.md` rule 6: "a checkpoint carrying
///   this gate stops for a human in *every* mode, including auto-mode,
///   regardless of its type. Rule 5 does not apply to it."
/// - `type="checkpoint:human-action"` — `checkpoints.md` rule 5, which
///   auto-approves human-verify and auto-selects decision but says
///   "human-action still stops (auth gates cannot be automated)". The type
///   alone is sufficient; such a task need carry no `gate` attribute.
///
/// The CLOSING QUOTE is load-bearing on the first literal. Without it the
/// match would fire on the ordinary `gate="blocking"` — precisely the
/// checkpoint class phase 35.1 exists to make auto-approvable — and every
/// unattended launch of a phase planning one would be refused.
const HUMAN_ONLY_CHECKPOINT_MARKERS: [&str; 2] = [
    concat!("gate=", "\"", "blocking-human", "\""),
    concat!("type=", "\"", "checkpoint:human-action", "\""),
];

/// The opening bytes of a task element, the anchor every marker match must
/// also satisfy.
const TASK_ELEMENT_OPENING: &str = "<task";

/// Return `true` if any plan declared for `phase` DECLARES a checkpoint task
/// that GSD will not auto-approve in any mode.
///
/// **The match is anchored to a task element's own opening tag**, and that is
/// the whole difference between this function and
/// [`phase_has_blocking_human_checkpoint`] above: a marker qualifies only on a
/// line that also opens a `<task`. A plan that merely *discusses* a marker — in
/// a findings section, in a table, in a fenced example — has not declared one.
///
/// The concrete failures the anchoring prevents (F-14) were measured against
/// this repository's own plan files, not assumed: `34-04-PLAN.md:245` and
/// `33-02-PLAN.md:109` each quote `gate="blocking-human"` inside a sentence
/// while declaring no such task. Under an unanchored whole-file `contains`,
/// `preflight.rs`'s unattended-launch check reads phases 33 and 34 as carrying a
/// checkpoint no mode can approve and refuses an overnight run that was fine. A
/// false refusal has no in-product recovery (D-09), so a false positive here is
/// not a cosmetic defect. (F-14 itself names `35.1-03-PLAN.md` as the instance;
/// that file describes the markers only in English and never writes either
/// literal, so it would not match an unanchored scan either. The finding stands;
/// the example it cited does not.)
///
/// **Known limit, pinned by `human_only_checkpoint_still_matches_a_task_tag_
/// inside_a_fenced_example`:** the anchor is line-level and has no notion of
/// markdown fences, so a plan documenting a COMPLETE example `<task ...>` tag
/// inside a fenced block still matches. No plan file in this repository has that
/// shape today.
///
/// **[`phase_has_blocking_human_checkpoint`] is deliberately left alone, and the
/// pair is not an accident.** That function serves the plan-28-03 auto-decide
/// route, where a looser, over-inclusive match fails SAFE — it routes more
/// checkpoints to a human. Here an over-inclusive match fails toward refusing a
/// launch. Different consequence, different predicate; widening the older one to
/// serve both would silently change the behaviour its seven tests pin.
///
/// Returns `false` for a phase with no plan files at all. That is NOT the same
/// fact as "plans exist and declare no such checkpoint", and a caller that needs
/// to tell them apart asks [`phase_plan_files`] separately rather than reading a
/// third state out of one bit.
///
/// **The CALLER owns root resolution**, exactly as for
/// [`phase_has_blocking_human_checkpoint`]: in worktree mode the phase's plans
/// live on the feature branch inside the worktree and are absent from the main
/// checkout (999.76).
pub fn phase_has_human_only_checkpoint(project_root: &Path, phase: PhaseId) -> bool {
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|contents| contents.lines().any(line_declares_human_only_checkpoint))
}

/// Whether one line both opens a task element and carries a human-only marker.
fn line_declares_human_only_checkpoint(line: &str) -> bool {
    line.contains(TASK_ELEMENT_OPENING)
        && HUMAN_ONLY_CHECKPOINT_MARKERS
            .iter()
            .any(|marker| line.contains(marker))
}

/// Run one explicitly operator-approved external verification command.
///
/// `sh -c` is intentional because probes may contain pipelines. The caller
/// must source `cmd` from [`external_verify_commands`] and first require
/// [`external_verification_approval`]. Spawn failures and non-zero exits fail
/// closed.
pub fn run_external_verification(cmd: &str, project_root: &Path) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(project_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_plan(root: &std::path::Path, contents: &str) {
        let phase_dir = root.join(".planning/phases/16-pipeline-reliability-hardening");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(phase_dir.join("16-03-PLAN.md"), contents).unwrap();
    }

    #[test]
    fn approval_parser_accepts_only_nonempty_json_command_arrays() {
        assert_eq!(
            parse_external_verification_approval(r#"["test -f shipped", "cargo test"]"#),
            Some(vec!["test -f shipped".into(), "cargo test".into()])
        );
        for invalid in [
            "",
            "true",
            "{}",
            "[]",
            r#"[""]"#,
            r#"["   "]"#,
            r#"["ok", 1]"#,
        ] {
            assert_eq!(
                parse_external_verification_approval(invalid),
                None,
                "approval must fail closed for {invalid:?}"
            );
        }
    }

    #[test]
    fn reads_external_verify_only_from_plan_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        write_plan(
            dir.path(),
            "---\nphase: 16\nexternal_verify: \"test -f shipped.txt\"\n---\n\n# Plan\n",
        );
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            dir.path().join(".devflow/phase-16-stdout"),
            "external_verify: \"touch agent-controlled\"\nDEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();

        assert_eq!(
            external_verify_commands(dir.path(), PhaseId::new(16)),
            vec!["test -f shipped.txt"]
        );
    }

    #[test]
    fn ignores_external_verify_outside_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        write_plan(
            dir.path(),
            "---\nphase: 16\n---\n\nexternal_verify: \"false\"\n",
        );

        assert!(external_verify_commands(dir.path(), PhaseId::new(16)).is_empty());
    }

    #[test]
    fn ignores_empty_external_verify_commands() {
        for value in [r#""""#, "''"] {
            let dir = tempfile::tempdir().unwrap();
            write_plan(
                dir.path(),
                &format!("---\nphase: 16\nexternal_verify: {value}\n---\n"),
            );
            assert!(
                external_verify_commands(dir.path(), PhaseId::new(16)).is_empty(),
                "empty command {value:?} must not count as affirmative verification"
            );
        }
    }

    #[test]
    fn runs_probe_from_project_root_and_reports_exit_status() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shipped.txt"), "ok").unwrap();

        assert!(run_external_verification("test -f shipped.txt", dir.path()));
        assert!(!run_external_verification(
            "test -f missing.txt",
            dir.path()
        ));
    }

    // The gate value is assembled at runtime from this const, not written as a
    // literal in fixture bodies below, so this test file itself never contains
    // the raw `gate="blocking-human"` string (28-01 Task 2 action note).
    const HUMAN_GATE_VALUE: &str = "blocking-human";
    const PLAIN_GATE_VALUE: &str = "blocking";
    /// The second marker GSD never auto-approves in any mode
    /// (`checkpoints.md` rule 5: "human-action still stops").
    const HUMAN_ACTION_TYPE_VALUE: &str = "checkpoint:human-action";

    fn write_phase_file(root: &std::path::Path, phase_dir: &str, file_name: &str, contents: &str) {
        let dir = root.join(".planning/phases").join(phase_dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file_name), contents).unwrap();
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_detects_declared_gate() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 91\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
        );
        write_phase_file(dir.path(), "91-probe", "91-01-PLAN.md", &body);

        assert!(phase_has_blocking_human_checkpoint(
            dir.path(),
            PhaseId::new(91)
        ));
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_false_for_plain_blocking_gate() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 91\n---\n\n<task type=\"checkpoint:decision\" gate=\"{PLAIN_GATE_VALUE}\">\n</task>\n"
        );
        write_phase_file(dir.path(), "91-probe", "91-01-PLAN.md", &body);

        assert!(
            !phase_has_blocking_human_checkpoint(dir.path(), PhaseId::new(91)),
            "the plain `blocking` gate (no -human suffix) must not match — Phase 26 near-miss distinction"
        );
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_false_when_no_gate_attribute() {
        let dir = tempfile::tempdir().unwrap();
        write_phase_file(
            dir.path(),
            "91-probe",
            "91-01-PLAN.md",
            "---\nphase: 91\n---\n\n<task type=\"auto\">\n</task>\n",
        );

        assert!(!phase_has_blocking_human_checkpoint(
            dir.path(),
            PhaseId::new(91)
        ));
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_false_for_missing_phase_directory() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!phase_has_blocking_human_checkpoint(
            dir.path(),
            PhaseId::new(404)
        ));
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_true_when_only_second_plan_carries_attribute() {
        let dir = tempfile::tempdir().unwrap();
        write_phase_file(
            dir.path(),
            "91-probe",
            "91-01-PLAN.md",
            "---\nphase: 91\n---\n\n<task type=\"auto\">\n</task>\n",
        );
        let body = format!(
            "---\nphase: 91\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
        );
        write_phase_file(dir.path(), "91-probe", "91-02-PLAN.md", &body);

        assert!(
            phase_has_blocking_human_checkpoint(dir.path(), PhaseId::new(91)),
            "every plan must be inspected, not just the first"
        );
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_ignores_non_plan_files() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 91\n---\n\nRecorded checkpoint gate=\"{HUMAN_GATE_VALUE}\" in the executor return.\n"
        );
        write_phase_file(dir.path(), "91-probe", "91-01-SUMMARY.md", &body);

        assert!(
            !phase_has_blocking_human_checkpoint(dir.path(), PhaseId::new(91)),
            "only *-PLAN.md files are scanned, not SUMMARY/RESEARCH files"
        );
    }

    /// 999.76 (ROADMAP criterion 6), the second call site.
    ///
    /// This function reads whatever root it is handed, so the CALLER decides
    /// whether it can see the phase's plans at all. `pipeline_launch.rs`'s
    /// `Action::GateReview` arm passed `project_root` unconditionally; in
    /// worktree mode the phase's `{N}-PLAN.md` lives on `feature/phase-{N}`
    /// inside the worktree and is absent from the main checkout, so this
    /// returned `false` and the plan-28-03 checkpoint auto-decide path was
    /// silently dead for the phase's whole duration.
    ///
    /// This test and its mirror below pin the property that makes the caller's
    /// choice matter: the answer DEPENDS on the root. Each carries the
    /// opposite-root assertion, because a pair that returned `true` for both
    /// roots would only be measuring that a PLAN exists somewhere.
    #[test]
    fn phase_has_blocking_human_checkpoint_reads_the_execution_root_in_worktree_mode() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("phase-worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let body = format!(
            "---\nphase: 91\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
        );
        // The PLAN exists ONLY inside the worktree — the project root's own
        // `.planning/phases/` is deliberately never created.
        write_phase_file(&worktree, "91-probe", "91-01-PLAN.md", &body);

        assert!(
            phase_has_blocking_human_checkpoint(&worktree, PhaseId::new(91)),
            "the execution root holds the PLAN, so the declaration must be found"
        );
        assert!(
            !phase_has_blocking_human_checkpoint(dir.path(), PhaseId::new(91)),
            "opposite-result case: the project root has no PLAN and must return false — \
             if both roots returned true, this pair would be measuring the presence of a \
             file somewhere rather than which root is read"
        );
    }

    /// The main-checkout mirror of the test above: with no worktree the two
    /// roots coincide, so 999.76's call-site change leaves this path untouched.
    #[test]
    fn phase_has_blocking_human_checkpoint_still_reads_the_project_root_without_a_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let empty_sibling = dir.path().join("phase-worktree");
        std::fs::create_dir_all(&empty_sibling).unwrap();
        let body = format!(
            "---\nphase: 91\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
        );
        write_phase_file(dir.path(), "91-probe", "91-01-PLAN.md", &body);

        assert!(
            phase_has_blocking_human_checkpoint(dir.path(), PhaseId::new(91)),
            "without a worktree the execution root IS the project root"
        );
        assert!(
            !phase_has_blocking_human_checkpoint(&empty_sibling, PhaseId::new(91)),
            "opposite-result case: a root without the PLAN must return false, so the \
             assertion above is about which root is read and not about the file existing"
        );
    }

    // -----------------------------------------------------------------
    // 35.1-03 Task 1: `phase_has_human_only_checkpoint` — the ANCHORED
    // scan for the two markers GSD never auto-approves in any mode.
    // -----------------------------------------------------------------

    #[test]
    fn human_only_checkpoint_detects_the_gate_marker_on_a_task_tag() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 92\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
        );
        write_phase_file(dir.path(), "92-probe", "92-01-PLAN.md", &body);

        assert!(phase_has_human_only_checkpoint(
            dir.path(),
            PhaseId::new(92)
        ));
    }

    #[test]
    fn human_only_checkpoint_detects_the_human_action_type_on_a_task_tag() {
        let dir = tempfile::tempdir().unwrap();
        // No `gate` attribute at all: `checkpoints.md` rule 5 stops a
        // human-action checkpoint in auto-mode on its TYPE alone, so the
        // second marker must be sufficient by itself.
        let body =
            format!("---\nphase: 92\n---\n\n<task type=\"{HUMAN_ACTION_TYPE_VALUE}\">\n</task>\n");
        write_phase_file(dir.path(), "92-probe", "92-01-PLAN.md", &body);

        assert!(phase_has_human_only_checkpoint(
            dir.path(),
            PhaseId::new(92)
        ));
    }

    /// The control that keeps this function off the checkpoint class phase
    /// 35.1 exists to make auto-approvable. A substring match that dropped the
    /// closing quote would match `blocking` inside `blocking-human` and, worse,
    /// report every ordinary blocking checkpoint as human-only — refusing every
    /// unattended launch of a phase that plans one.
    #[test]
    fn human_only_checkpoint_ignores_an_ordinary_blocking_gate() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 92\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{PLAIN_GATE_VALUE}\">\n</task>\n"
        );
        write_phase_file(dir.path(), "92-probe", "92-01-PLAN.md", &body);

        assert!(
            !phase_has_human_only_checkpoint(dir.path(), PhaseId::new(92)),
            "the ordinary `blocking` gate is exactly the class auto-mode may approve \
             (checkpoints.md rule 5) — matching it here would refuse launches that are fine"
        );
    }

    /// F-14's motivating case, and it is not hypothetical — but the instance is
    /// NOT the one F-14 names. `35.1-03-PLAN.md` describes the markers only in
    /// English ("the human-only gate value") and never writes either literal, so
    /// it would not match even an unanchored scan. The real instances were found
    /// by scanning this repository's own `*-PLAN.md` files:
    ///
    /// - `34-04-PLAN.md:245` — an acceptance-criteria bullet reading
    ///   ``A phase whose PLAN declaring `gate="blocking-human"` lives only under
    ///   a worktree-standing-in directory ...``
    /// - `33-02-PLAN.md:109` — a findings paragraph reading ``... with
    ///   `gate="blocking-human"` has no mechanism for receiving an operator's
    ///   answer ...``
    ///
    /// Neither declares such a task; an unanchored scan reports both phases NOT
    /// viable, refusing an unattended launch for a reason that does not exist.
    /// Three further files (`19-05`, `19-11`, `15-05`) carry the markers on a
    /// genuine `<task ...>` line, and both implementations agree on those — which
    /// is what makes the first two the discriminating cases rather than the whole
    /// scan being over-eager.
    ///
    /// The fixture below mirrors those two shapes: an inline-code mention inside
    /// a sentence, plus a fenced block carrying the bare attribute with no task
    /// tag on the line.
    #[test]
    fn human_only_checkpoint_ignores_a_marker_mentioned_only_in_prose() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 92\n---\n\n\
             A phase whose PLAN declares `gate=\"{HUMAN_GATE_VALUE}\"` has no mechanism \
             for receiving an operator's answer, and the same goes for \
             `type=\"{HUMAN_ACTION_TYPE_VALUE}\"`.\n\n\
             ```text\n\
             gate=\"{HUMAN_GATE_VALUE}\"\n\
             type=\"{HUMAN_ACTION_TYPE_VALUE}\"\n\
             ```\n\n\
             <task type=\"auto\">\n</task>\n"
        );
        write_phase_file(dir.path(), "92-probe", "92-01-PLAN.md", &body);

        assert!(
            !phase_has_human_only_checkpoint(dir.path(), PhaseId::new(92)),
            "a marker discussed in prose is not a marker declared on a task — \
             34-04-PLAN.md:245 and 33-02-PLAN.md:109 are the real instances (F-14)"
        );
    }

    /// The anchoring's KNOWN LIMIT, asserted rather than left to be discovered.
    ///
    /// The anchor is a single line: "contains a marker AND opens a task
    /// element". It has no notion of markdown fences, so a plan that documents
    /// a complete example `<task ...>` tag inside a fenced block DOES match, and
    /// the phase is refused. No `*-PLAN.md` in this repository currently has that
    /// shape — the three that carry a marker on a `<task` line are all genuine
    /// declarations — so this is a live gap, not a live defect.
    ///
    /// Fence tracking was not added: it is materially more parsing than F-14
    /// asked for, and it fails toward the SAME consequence (a false refusal) if
    /// the fence detection is itself wrong. This test exists so the limit is
    /// recorded as a measured property rather than rediscovered by an operator
    /// whose overnight run refused.
    #[test]
    fn human_only_checkpoint_still_matches_a_task_tag_inside_a_fenced_example() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 92\n---\n\n\
             Here is what such a task looks like:\n\n\
             ```xml\n\
             <task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n\
             </task>\n\
             ```\n"
        );
        write_phase_file(dir.path(), "92-probe", "92-01-PLAN.md", &body);

        assert!(
            phase_has_human_only_checkpoint(dir.path(), PhaseId::new(92)),
            "documented limit: line-level anchoring cannot see markdown fences, \
             so a complete example task tag reads as a declaration"
        );
    }

    /// "No plans at all" and "plans that declare no such checkpoint" are
    /// DIFFERENT facts, and this boolean deliberately reports both as `false`.
    /// The caller that needs to tell them apart asks [`phase_plan_files`]
    /// separately rather than reading a third state out of this one bit —
    /// `preflight.rs`'s unattended-launch check does exactly that, because at
    /// Define an unplanned phase is pending, not failing.
    #[test]
    fn human_only_checkpoint_is_false_for_a_phase_with_no_plans() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!phase_has_human_only_checkpoint(
            dir.path(),
            PhaseId::new(404)
        ));
        assert!(
            phase_plan_files(dir.path(), PhaseId::new(404)).is_empty(),
            "the companion fact the caller reads to distinguish the two cases"
        );
    }
}
