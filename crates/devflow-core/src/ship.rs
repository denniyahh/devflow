//! Ship/PR bookkeeping.
//!
//! Holds the `LastShip` record written by `devflow ship` and consumed by
//! `devflow confirm` / `devflow rejectpr`, plus PR-body generation and the
//! pure document-finalization transforms (CHANGELOG, ROADMAP) used on confirm.

use crate::config::GitFlowConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Record of the most recent `devflow ship`, enabling confirm/reject later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastShip {
    /// Phase that was shipped.
    pub phase: u32,
    /// Version before the bump.
    pub version_from: String,
    /// Version after the bump.
    pub version_to: String,
    /// Release branch name.
    pub release_branch: String,
    /// PR number, if a PR was opened.
    pub pr_number: Option<u64>,
    /// PR URL, if a PR was opened.
    pub pr_url: Option<String>,
    /// Path to the file whose version was bumped.
    pub version_file: PathBuf,
    /// Whether the ship has been rejected.
    pub rejected: bool,
    /// Why the ship was rejected, if applicable.
    pub reject_reason: Option<String>,
    /// Unix timestamp (seconds) when the ship was created.
    pub created_at: String,
}

/// Errors produced by ship bookkeeping.
#[derive(Debug, thiserror::Error)]
pub enum ShipError {
    /// Filesystem operation failed.
    #[error("ship I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse or serialization failed.
    #[error("ship JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// No last-ship record exists.
    #[error("no last-ship record found — nothing to confirm or reject")]
    Missing,
}

/// Path to the last-ship record for a project.
pub fn last_ship_path(project_root: &Path) -> PathBuf {
    project_root.join(".devflow").join("last-ship.json")
}

/// Persist the last-ship record.
pub fn save(project_root: &Path, record: &LastShip) -> Result<(), ShipError> {
    let path = last_ship_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(record)?)?;
    Ok(())
}

/// Load the last-ship record, or [`ShipError::Missing`] if absent.
pub fn load(project_root: &Path) -> Result<LastShip, ShipError> {
    let path = last_ship_path(project_root);
    if !path.exists() {
        return Err(ShipError::Missing);
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(&path)?)?)
}

/// Remove the last-ship record. Idempotent.
pub fn delete(project_root: &Path) -> Result<(), ShipError> {
    let path = last_ship_path(project_root);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Build a Markdown PR body from the phase Goal, the diffstat, and a test count.
///
/// Each source is fail-soft: a missing CONTEXT.md, diff, or test output yields a
/// placeholder rather than failing the whole body.
pub fn build_pr_body(
    project_root: &Path,
    phase: u32,
    git_flow: &GitFlowConfig,
    verify_command: &str,
) -> String {
    let goal = extract_goal(project_root, phase)
        .unwrap_or_else(|| "_No phase Goal found in CONTEXT.md._".to_string());
    let changes = changed_files(project_root, &git_flow.develop)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "(no diff available)".to_string());
    let tests = test_summary(project_root, verify_command);

    format!(
        "## Summary\n\nPhase {phase}.\n\n{goal}\n\n## Changes\n\n```\n{changes}\n```\n\n## Tests\n\n{tests}\n"
    )
}

/// Extract the `## Goal` section text from a phase's CONTEXT.md.
pub fn extract_goal(project_root: &Path, phase: u32) -> Option<String> {
    let phases_dir = project_root.join(".planning").join("phases");
    let prefix = format!("{phase:02}-");
    let entry = std::fs::read_dir(&phases_dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with(&prefix))?;
    let text = std::fs::read_to_string(entry.path().join("CONTEXT.md")).ok()?;
    extract_section(&text, "## Goal")
}

/// Return the text of a Markdown section: everything after the `header` line up
/// to (but excluding) the next `## ` heading or `---` rule.
fn extract_section(text: &str, header: &str) -> Option<String> {
    let mut lines = text.lines();
    // Advance to the header line.
    lines.by_ref().find(|line| line.trim() == header)?;

    let mut body = Vec::new();
    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("## ") || line.trim() == "---" {
            break;
        }
        body.push(line);
    }
    let joined = body.join("\n").trim().to_string();
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// `git diff --stat <develop>...HEAD` run in the project root.
fn changed_files(project_root: &Path, develop: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["diff", "--stat", &format!("{develop}...HEAD")])
        .current_dir(project_root)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Run the verify command and summarize the passing test count.
fn test_summary(project_root: &Path, verify_command: &str) -> String {
    let output = Command::new("sh")
        .arg("-c")
        .arg(verify_command)
        .current_dir(project_root)
        .output();
    match output {
        Ok(out) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            match count_passed_tests(&combined) {
                Some(n) => format!("{n} tests passed (`{verify_command}`)."),
                None => format!("tests: unknown (`{verify_command}` output not parseable)."),
            }
        }
        Err(_) => "tests: unknown (verify command could not run).".to_string(),
    }
}

/// Sum the `test result: ok. N passed` counts in cargo test output.
pub fn count_passed_tests(output: &str) -> Option<u32> {
    let mut total = 0u32;
    let mut found = false;
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("test result: ok. ")
            && let Some(num) = rest.split_whitespace().next()
            && let Ok(n) = num.parse::<u32>()
        {
            total += n;
            found = true;
        }
    }
    found.then_some(total)
}

/// Prepend a CHANGELOG entry for `version`, creating a standard header if the
/// file did not exist. Pure transform over the existing CHANGELOG contents.
pub fn prepend_changelog(existing: &str, version: &str, date: &str) -> String {
    const HEADER: &str = "# Changelog\n\n\
        All notable changes to this project are documented here.\n";
    let entry = format!("## {version} — {date}\n\n- Released phase via DevFlow.\n");

    if existing.trim().is_empty() {
        return format!("{HEADER}\n{entry}");
    }
    // Insert the new entry after the header block (first blank line after the
    // top-level title), or at the top if no header is recognized.
    if let Some(idx) = existing.find("\n\n") {
        let (head, tail) = existing.split_at(idx + 2);
        format!("{head}{entry}\n{tail}")
    } else {
        format!("{entry}\n{existing}")
    }
}

/// Annotate a ROADMAP `## Phase N` heading as completed, matching the existing
/// `(Priority: ... — COMPLETED vX)` style. Idempotent. Pure transform.
pub fn mark_phase_complete(roadmap: &str, phase: u32, version: &str) -> String {
    let needle = format!("## Phase {phase}");
    roadmap
        .lines()
        .map(|line| {
            if line.starts_with(&needle) && !line.contains("COMPLETED") {
                format!("{} — COMPLETED v{version}", line.trim_end())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(root: &Path) -> LastShip {
        LastShip {
            phase: 7,
            version_from: "0.5.1".into(),
            version_to: "0.5.2".into(),
            release_branch: "release/0.5.2".into(),
            pr_number: Some(42),
            pr_url: Some("https://example/pr/42".into()),
            version_file: root.join("Cargo.toml"),
            rejected: false,
            reject_reason: None,
            created_at: "1750000000".into(),
        }
    }

    #[test]
    fn save_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let record = sample(dir.path());
        save(dir.path(), &record).unwrap();
        assert_eq!(load(dir.path()).unwrap(), record);
    }

    #[test]
    fn load_missing_returns_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(load(dir.path()), Err(ShipError::Missing)));
    }

    #[test]
    fn delete_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &sample(dir.path())).unwrap();
        delete(dir.path()).unwrap();
        assert!(!last_ship_path(dir.path()).exists());
        delete(dir.path()).unwrap();
    }

    #[test]
    fn extract_section_reads_goal_until_next_heading() {
        let text = "# Title\n\n## Goal\n\nDo the thing.\nAnd more.\n\n---\n\n## Next\nignored\n";
        let goal = extract_section(text, "## Goal").unwrap();
        assert_eq!(goal, "Do the thing.\nAnd more.");
    }

    #[test]
    fn extract_section_missing_returns_none() {
        assert!(extract_section("no headings here", "## Goal").is_none());
    }

    #[test]
    fn extract_goal_reads_phase_context_file() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir.path().join(".planning/phases/07-worktrees-pr");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join("CONTEXT.md"),
            "# Phase 7\n\n## Goal\n\nEnable parallel agents.\n\n## Tasks\n- x\n",
        )
        .unwrap();
        assert_eq!(
            extract_goal(dir.path(), 7).unwrap(),
            "Enable parallel agents."
        );
    }

    #[test]
    fn build_pr_body_contains_goal_and_sections() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir.path().join(".planning/phases/07-x");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(phase_dir.join("CONTEXT.md"), "## Goal\n\nShip it well.\n").unwrap();
        // `true` produces no test output → "tests: unknown".
        let body = build_pr_body(dir.path(), 7, &GitFlowConfig::default(), "true");
        assert!(body.contains("## Summary"));
        assert!(body.contains("Ship it well."));
        assert!(body.contains("## Changes"));
        assert!(body.contains("## Tests"));
    }

    #[test]
    fn build_pr_body_without_context_uses_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let body = build_pr_body(dir.path(), 9, &GitFlowConfig::default(), "true");
        assert!(body.contains("No phase Goal found"));
        assert!(body.contains("## Tests"));
    }

    #[test]
    fn count_passed_tests_sums_across_lines() {
        let output = "test result: ok. 115 passed; 0 failed\n\
                      test result: ok. 1 passed; 0 failed\n";
        assert_eq!(count_passed_tests(output), Some(116));
        assert_eq!(count_passed_tests("no test lines"), None);
    }

    #[test]
    fn prepend_changelog_creates_header_when_empty() {
        let out = prepend_changelog("", "0.5.2", "2026-06-18");
        assert!(out.starts_with("# Changelog"));
        assert!(out.contains("## 0.5.2 — 2026-06-18"));
    }

    #[test]
    fn prepend_changelog_inserts_after_header() {
        let existing = "# Changelog\n\n## 0.5.1 — 2026-06-17\n\n- old\n";
        let out = prepend_changelog(existing, "0.5.2", "2026-06-18");
        let new_idx = out.find("0.5.2").unwrap();
        let old_idx = out.find("0.5.1").unwrap();
        assert!(new_idx < old_idx, "new entry should come before old");
        assert!(out.starts_with("# Changelog"));
    }

    #[test]
    fn mark_phase_complete_annotates_heading_idempotently() {
        let roadmap = "## Phase 7: Worktrees (Priority: HIGH, v1.0.0)\n\nbody\n";
        let once = mark_phase_complete(roadmap, 7, "0.5.2");
        assert!(once.contains("## Phase 7: Worktrees (Priority: HIGH, v1.0.0) — COMPLETED v0.5.2"));
        // Running again does not double-annotate.
        let twice = mark_phase_complete(&once, 7, "0.5.2");
        assert_eq!(once, twice);
    }
}
