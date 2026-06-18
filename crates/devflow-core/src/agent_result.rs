//! Agent completion detection — parses DEVLOW_RESULT markers and evaluates
//! exit codes to determine whether a coding agent succeeded or failed.
//!
//! Three-layer decision engine:
//! 1. Parse DEVLOW_RESULT from agent stdout (authoritative)
//! 2. Exit code + commit count gate (reliable fallback)
//! 3. Process gone + commits exist (last resort warning)

use crate::state::State;
use std::path::{Path, PathBuf};

/// Parsed agent completion result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentResult {
    pub status: AgentStatus,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub commits: Option<u32>,
    pub summary: Option<String>,
}

/// Agent completion status determined by DevFlow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    /// Agent self-reported success via DEVLOW_RESULT.
    Success,
    /// Agent self-reported failure, or exit code + commit gate indicated failure.
    Failed,
    /// No signal received — fallback to exit code / commit heuristic.
    Unknown,
}

/// Capture from an agent child process: stdout contents and exit code.
pub struct AgentCapture {
    pub stdout: String,
    pub exit_code: i32,
}

/// Errors produced by agent result evaluation.
#[derive(Debug, thiserror::Error)]
pub enum ResultError {
    #[error("I/O error reading agent output: {0}")]
    Io(#[from] std::io::Error),
    #[error("phase directory not found")]
    NoPhaseDir,
}

/// Search stdout for a DEVLOW_RESULT marker.
///
/// The marker is a single line starting with `DEVLOW_RESULT:` followed by
/// a JSON object with at minimum a `status` field. Matching is case-insensitive.
///
/// When an agent is run with `--output-format json` (e.g. Claude), its final
/// message is wrapped in a JSON result envelope with the text — and its
/// embedded newlines — escaped inside a `result` field. In that case the
/// marker never appears at the start of a line, so we first unwrap the
/// envelope and search the inner text.
pub fn parse_devlow_result(stdout: &str) -> Option<AgentResult> {
    if let Some(inner) = extract_json_result_text(stdout)
        && let Some(result) = parse_marker_lines(&inner)
    {
        return Some(result);
    }
    parse_marker_lines(stdout)
}

/// If `stdout` is a JSON result envelope, return the decoded `result` text
/// field (with escapes such as `\n` resolved). Returns `None` for plain text.
fn extract_json_result_text(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    value.get("result")?.as_str().map(str::to_string)
}

/// Scan the tail of `stdout` line-by-line for the last DEVLOW_RESULT marker.
fn parse_marker_lines(stdout: &str) -> Option<AgentResult> {
    // Only search the tail — agents may echo the marker in their prompt
    // and we want the LAST occurrence (which is their actual final status).
    let tail: String = stdout
        .chars()
        .rev()
        .take(4000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    for line in tail.lines().rev() {
        let Some(json_str) = line
            .strip_prefix("DEVLOW_RESULT: ")
            .or_else(|| line.strip_prefix("devlow_result: "))
            .or_else(|| line.strip_prefix("DEVLOW_RESULT:"))
            .or_else(|| line.strip_prefix("devlow_result:"))
        else {
            continue;
        };

        let json_str = json_str.trim();
        if let Ok(result) = serde_json::from_str::<AgentResult>(json_str) {
            return Some(result);
        }
    }
    None
}

/// Layer 1: Try to detect agent result from DEVLOW_RESULT marker in stdout.
pub fn evaluate_layer1(project_root: &Path, phase: u32) -> Option<AgentResult> {
    let stdout_path = devflow_dir(project_root).join(format!("phase-{:02}-stdout", phase));
    let stdout = std::fs::read_to_string(&stdout_path).ok()?;
    parse_devlow_result(&stdout)
}

/// Layer 2: Use exit code + commit count to determine result.
///
/// Reads exit code from `.devflow/phase-NN-exit` file.
/// Counts commits in `feature/phase-NN` branch (if it exists).
/// Decision matrix:
///   exit=0, commits>0 → advance (probable ok)
///   exit=0, commits=0 → halt "no work done"
///   exit≠0            → halt "agent failed"
///   exit unknown      → fall to Layer 3 (return None)
pub fn evaluate_layer2(
    project_root: &Path,
    phase: u32,
    state: &State,
) -> Result<Option<AgentResult>, ResultError> {
    let exit_path = devflow_dir(project_root).join(format!("phase-{:02}-exit", phase));
    let exit_code: i32 = match std::fs::read_to_string(&exit_path) {
        Ok(s) => s.trim().parse().unwrap_or(-1),
        Err(_) => return Ok(None), // fall to Layer 3
    };

    let branch = format!(
        "{}phase-{:02}",
        "feature/", // hardcoded — read from state/config if needed
        phase
    );

    // Verify branch exists before counting commits.
    let branch_exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &branch])
        .current_dir(&state.project_root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let commits: u32 = if branch_exists {
        let range = format!("develop..{branch}");
        std::process::Command::new("git")
            .args(["rev-list", "--count", &range])
            .current_dir(&state.project_root)
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(0)
    } else {
        0
    };

    Ok(Some(AgentResult {
        status: if exit_code == 0 && commits > 0 {
            AgentStatus::Success
        } else {
            AgentStatus::Failed
        },
        exit_code: Some(exit_code),
        reason: if exit_code != 0 {
            Some(format!(
                "agent exited with code {} ({} commits on {})",
                exit_code, commits, branch
            ))
        } else if commits == 0 {
            Some(format!(
                "no commits found on {} (agent exit code was {})",
                branch, exit_code
            ))
        } else {
            Some(format!(
                "{} commits on {} (agent exit code was {})",
                commits, branch, exit_code
            ))
        },
        commits: Some(commits),
        summary: None,
    }))
}

/// Layer 3: Last resort — agent process is gone, commits exist.
///
/// Returns Unknown status with a warning. This only fires when
/// neither Layer 1 nor Layer 2 produced a definitive result.
pub fn evaluate_layer3(project_root: &Path, phase: u32) -> Result<AgentResult, ResultError> {
    let branch = format!("feature/phase-{:02}", phase);
    let commits = std::process::Command::new("git")
        .args(["rev-list", "--count", &format!("develop..{branch}")])
        .current_dir(project_root)
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0);

    Ok(AgentResult {
        status: AgentStatus::Unknown,
        exit_code: None,
        reason: if commits > 0 {
            Some(format!(
                "unverified — agent process is gone but {} commits exist on {}",
                commits, branch
            ))
        } else {
            Some("no work detected — agent process is gone with no commits".into())
        },
        commits: Some(commits),
        summary: None,
    })
}

/// Full three-layer evaluation: returns the best available AgentResult.
pub fn evaluate_agent_result(
    project_root: &Path,
    state: &State,
) -> Result<AgentResult, ResultError> {
    // Layer 1: DEVLOW_RESULT marker (authoritative)
    if let Some(result) = evaluate_layer1(project_root, state.phase) {
        return Ok(result);
    }

    // Layer 2: Exit code + commit gate
    if let Some(result) = evaluate_layer2(project_root, state.phase, state)? {
        return Ok(result);
    }

    // Layer 3: Process existence + commits
    evaluate_layer3(project_root, state.phase)
}

/// Path to the .devflow directory for a project root.
fn devflow_dir(project_root: &Path) -> PathBuf {
    project_root.join(".devflow")
}

/// Path to the stdout file for a given phase.
pub fn stdout_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-stdout", phase))
}

/// Path to the exit code file for a given phase.
pub fn exit_code_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-exit", phase))
}

/// Clean up old stdout and exit code files for a phase before starting.
pub fn cleanup_phase_files(project_root: &Path, phase: u32) {
    let stdout = stdout_path(project_root, phase);
    let exit = exit_code_path(project_root, phase);
    let _ = std::fs::remove_file(&stdout);
    let _ = std::fs::remove_file(&exit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_success_marker() {
        let stdout = "some output\nDEVLOW_RESULT: {\"status\":\"success\"}\n";
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_failed_marker_with_reason() {
        let stdout =
            "work done\nDEVLOW_RESULT: {\"status\":\"failed\",\"reason\":\"clippy errors\"}\n";
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.unwrap(), "clippy errors");
    }

    #[test]
    fn parse_missing_marker_returns_none() {
        let stdout = "just some output\nno marker here\n";
        assert!(parse_devlow_result(stdout).is_none());
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        let stdout = "DEVLOW_RESULT: {not valid json}\n";
        assert!(parse_devlow_result(stdout).is_none());
    }

    #[test]
    fn parse_lowercase_marker() {
        let stdout = "devlow_result: {\"status\":\"success\"}\n";
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_marker_without_space_after_colon() {
        let stdout = "DEVLOW_RESULT:{\"status\":\"success\"}\n";
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_finds_last_marker_in_tail() {
        // Multiple markers — should find the last one.
        let stdout = "DEVLOW_RESULT: {\"status\":\"failed\"}\nsome more output\nDEVLOW_RESULT: {\"status\":\"success\"}\n";
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_marker_only_in_last_4000_chars() {
        // Marker beyond 4000 chars from end should not be found.
        let prefix = "a".repeat(5000);
        let stdout = format!("DEVLOW_RESULT: {{\"status\":\"success\"}}\n{prefix}");
        assert!(parse_devlow_result(&stdout).is_none());
    }

    #[test]
    fn parse_marker_with_commits_and_summary() {
        let stdout = r#"DEVLOW_RESULT: {"status":"success","commits":3,"summary":"added tests"}"#;
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.commits, Some(3));
        assert_eq!(result.summary.unwrap(), "added tests");
    }

    #[test]
    fn parse_marker_inside_json_result_envelope() {
        // Claude --output-format json wraps the final text in a `result` field
        // with embedded newlines escaped.
        let stdout = r#"{"type":"result","subtype":"success","result":"All done.\nDEVLOW_RESULT: {\"status\": \"success\", \"commits\": 2}","session_id":"abc"}"#;
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.commits, Some(2));
    }

    #[test]
    fn parse_failed_marker_inside_json_envelope() {
        let stdout = r#"{"result":"work\nDEVLOW_RESULT: {\"status\": \"failed\", \"reason\": \"tests failed\"}"}"#;
        let result = parse_devlow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.unwrap(), "tests failed");
    }

    #[test]
    fn parse_json_envelope_without_marker_returns_none() {
        let stdout = r#"{"result":"did some work but forgot the marker","session_id":"x"}"#;
        assert!(parse_devlow_result(stdout).is_none());
    }

    #[test]
    fn cleanup_removes_phase_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(root.join(".devflow/phase-01-stdout"), "test").unwrap();
        std::fs::write(root.join(".devflow/phase-01-exit"), "0").unwrap();

        cleanup_phase_files(root, 1);
        assert!(!root.join(".devflow/phase-01-stdout").exists());
        assert!(!root.join(".devflow/phase-01-exit").exists());
    }

    #[test]
    fn cleanup_handles_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Should not panic when files don't exist.
        cleanup_phase_files(root, 1);
    }
}
