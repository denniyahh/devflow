//! Operator-authored external post-condition verification.
//!
//! Commands are discovered only from PLAN.md YAML frontmatter. Agent output
//! is deliberately outside this module's input boundary.

use std::path::{Path, PathBuf};

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
        if value.starts_with('"') {
            return serde_json::from_str::<String>(value).ok();
        }
        if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
            return Some(value[1..value.len() - 1].replace("''", "'"));
        }
        return Some(value.to_owned());
    }
    None
}

/// Run one trusted, operator-authored external verification command.
///
/// `sh -c` is intentional because probes may contain pipelines. The caller
/// must source `cmd` from [`external_verify_commands`]. Spawn failures and
/// non-zero exits fail closed.
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
    fn runs_probe_from_project_root_and_reports_exit_status() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shipped.txt"), "ok").unwrap();

        assert!(run_external_verification("test -f shipped.txt", dir.path()));
        assert!(!run_external_verification(
            "test -f missing.txt",
            dir.path()
        ));
    }
}
