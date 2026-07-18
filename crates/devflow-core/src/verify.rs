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
        assert!(!run_external_verification("test -f missing.txt", dir.path()));
    }
}
