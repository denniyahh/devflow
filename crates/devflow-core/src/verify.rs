//! Explicitly operator-approved external post-condition verification.
//!
//! Commands are discovered only from PLAN.md YAML frontmatter. Because those
//! files are agent-writable, execution additionally requires the parent
//! process's [`TRUST_EXTERNAL_VERIFY_ENV`] authorization.

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

/// Return external verification commands declared by this phase's plans.
///
/// Only the first YAML frontmatter block is inspected. This intentionally
/// small parser recognizes the scalar shape established by Phase 16:
/// `external_verify: "command"` (single-quoted and unquoted scalars are also
/// accepted). Runtime captures and agent output are never read here.
pub fn external_verify_commands(project_root: &Path, phase: u32) -> Vec<String> {
    let phases_dir = project_root.join(".planning/phases");
    let phase_prefix = format!("{phase:02}-");
    let plan_prefix = format!("{phase:02}-");
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
pub fn phase_has_blocking_human_checkpoint(_project_root: &Path, _phase: u32) -> bool {
    unimplemented!("GREEN phase not yet implemented")
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
            external_verify_commands(dir.path(), 16),
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

        assert!(external_verify_commands(dir.path(), 16).is_empty());
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
                external_verify_commands(dir.path(), 16).is_empty(),
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

        assert!(phase_has_blocking_human_checkpoint(dir.path(), 91));
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_false_for_plain_blocking_gate() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nphase: 91\n---\n\n<task type=\"checkpoint:decision\" gate=\"{PLAIN_GATE_VALUE}\">\n</task>\n"
        );
        write_phase_file(dir.path(), "91-probe", "91-01-PLAN.md", &body);

        assert!(
            !phase_has_blocking_human_checkpoint(dir.path(), 91),
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

        assert!(!phase_has_blocking_human_checkpoint(dir.path(), 91));
    }

    #[test]
    fn phase_has_blocking_human_checkpoint_false_for_missing_phase_directory() {
        let dir = tempfile::tempdir().unwrap();

        assert!(!phase_has_blocking_human_checkpoint(dir.path(), 404));
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
            phase_has_blocking_human_checkpoint(dir.path(), 91),
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
            !phase_has_blocking_human_checkpoint(dir.path(), 91),
            "only *-PLAN.md files are scanned, not SUMMARY/RESEARCH files"
        );
    }
}
