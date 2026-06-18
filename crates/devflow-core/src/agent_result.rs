//! Agent completion detection — parses DEVFLOW_RESULT markers and evaluates
//! exit codes to determine whether a coding agent succeeded or failed.
//!
//! Three-layer decision engine:
//! 1. Parse DEVFLOW_RESULT from agent stdout (authoritative)
//! 2. Exit code + commit count gate (reliable fallback)
//! 3. Process gone + commits exist (last resort warning)

use crate::config::GitFlowConfig;
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
    /// Agent self-reported success via DEVFLOW_RESULT.
    Success,
    /// Agent self-reported failure, or exit code + commit gate indicated failure.
    Failed,
    /// Agent stopped because an upstream API or usage quota rate-limited it.
    RateLimited,
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

/// Search stdout for a DEVFLOW_RESULT marker.
///
/// The marker is a single line starting with `DEVFLOW_RESULT:` followed by
/// a JSON object with at minimum a `status` field. Matching is case-insensitive.
///
/// When an agent is run with `--output-format json` (e.g. Claude), its final
/// message is wrapped in a JSON result envelope with the text — and its
/// embedded newlines — escaped inside a `result` field. In that case the
/// marker never appears at the start of a line, so we first unwrap the
/// envelope and search the inner text.
pub fn parse_devflow_result(stdout: &str) -> Option<AgentResult> {
    if let Some(inner) = extract_json_result_text(stdout)
        && let Some(result) = parse_marker_lines(&inner)
    {
        return Some(result);
    }
    parse_marker_lines(stdout)
}

/// Detect agent-specific rate-limit output and return the retry description.
///
/// Claude can emit a JSON result envelope when run with `--output-format json`;
/// Codex commonly emits plain text such as "Try again at ...". This function is
/// intentionally conservative so ordinary progress text does not become a
/// false positive.
pub fn detect_rate_limit(stdout: &str) -> Option<String> {
    detect_claude_rate_limit(stdout).or_else(|| detect_codex_rate_limit(stdout))
}

fn detect_claude_rate_limit(stdout: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let rate_limited = json_has_str(&value, "subtype", "error_rate_limit")
        || json_has_i64(&value, "api_error_status", 429)
        || json_has_i64(&value, "status", 429)
        || json_has_i64(&value, "status_code", 429);
    if !rate_limited {
        return None;
    }
    json_find_key(&value, "retry_after")
        .and_then(json_scalar_to_string)
        .or_else(|| json_find_key(&value, "message").and_then(json_scalar_to_string))
        .or_else(|| json_find_key(&value, "error").and_then(json_scalar_to_string))
        .or_else(|| Some("usage limit".to_string()))
}

fn detect_codex_rate_limit(stdout: &str) -> Option<String> {
    let lower = stdout.to_ascii_lowercase();
    if let Some(idx) = lower.find("try again at ") {
        let start = idx + "try again at ".len();
        let retry = stdout[start..]
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';')
            .trim();
        if !retry.is_empty() {
            return Some(retry.to_string());
        }
    }

    if lower.contains("usage limit") || lower.contains("rate limit") || lower.contains("429") {
        stdout
            .lines()
            .find(|line| {
                let line = line.to_ascii_lowercase();
                line.contains("usage limit") || line.contains("rate limit") || line.contains("429")
            })
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .or_else(|| Some("usage limit".to_string()))
    } else {
        None
    }
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

fn json_has_str(value: &serde_json::Value, key: &str, expected: &str) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(k, v)| {
            (k == key && v.as_str() == Some(expected)) || json_has_str(v, key, expected)
        }),
        serde_json::Value::Array(values) => values.iter().any(|v| json_has_str(v, key, expected)),
        _ => false,
    }
}

fn json_has_i64(value: &serde_json::Value, key: &str, expected: i64) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(k, v)| {
            (k == key && v.as_i64() == Some(expected)) || json_has_i64(v, key, expected)
        }),
        serde_json::Value::Array(values) => values.iter().any(|v| json_has_i64(v, key, expected)),
        _ => false,
    }
}

fn json_find_key<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(found) = map.get(key) {
                return Some(found);
            }
            map.values().find_map(|v| json_find_key(v, key))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|v| json_find_key(v, key)),
        _ => None,
    }
}

fn json_scalar_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Scan the tail of `stdout` line-by-line for the last DEVFLOW_RESULT marker.
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
            .strip_prefix("DEVFLOW_RESULT: ")
            .or_else(|| line.strip_prefix("devflow_result: "))
            .or_else(|| line.strip_prefix("DEVFLOW_RESULT:"))
            .or_else(|| line.strip_prefix("devflow_result:"))
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

/// Layer 1: Try to detect agent result from DEVFLOW_RESULT marker in stdout.
pub fn evaluate_layer1(project_root: &Path, phase: u32) -> Option<AgentResult> {
    let stdout_path = devflow_dir(project_root).join(format!("phase-{:02}-stdout", phase));
    let stdout = std::fs::read_to_string(&stdout_path).ok()?;
    parse_devflow_result(&stdout).or_else(|| {
        detect_rate_limit(&stdout).map(|retry| AgentResult {
            status: AgentStatus::RateLimited,
            exit_code: None,
            reason: Some(format!("rate limited until {retry}")),
            commits: None,
            summary: None,
        })
    })
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
    git_flow: &GitFlowConfig,
) -> Result<Option<AgentResult>, ResultError> {
    let exit_path = devflow_dir(project_root).join(format!("phase-{:02}-exit", phase));
    let exit_code: i32 = match std::fs::read_to_string(&exit_path) {
        Ok(s) => s.trim().parse().unwrap_or(-1),
        Err(_) => return Ok(None), // fall to Layer 3
    };

    let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase);

    // Verify branch exists before counting commits.
    let branch_exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &branch])
        .current_dir(&state.project_root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let commits: u32 = if branch_exists {
        let range = format!("{}..{branch}", git_flow.develop);
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
pub fn evaluate_layer3(
    project_root: &Path,
    phase: u32,
    git_flow: &GitFlowConfig,
) -> Result<AgentResult, ResultError> {
    let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase);
    let commits = std::process::Command::new("git")
        .args([
            "rev-list",
            "--count",
            &format!("{}..{branch}", git_flow.develop),
        ])
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
    git_flow: &GitFlowConfig,
) -> Result<AgentResult, ResultError> {
    // Layer 1: DEVFLOW_RESULT marker (authoritative)
    if let Some(result) = evaluate_layer1(project_root, state.phase) {
        return Ok(result);
    }

    // Layer 2: Exit code + commit gate
    if let Some(result) = evaluate_layer2(project_root, state.phase, state, git_flow)? {
        return Ok(result);
    }

    // Layer 3: Process existence + commits
    evaluate_layer3(project_root, state.phase, git_flow)
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

/// Path to the file where the monitor records the launched agent's PID.
pub fn agent_pid_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-agent-pid", phase))
}

/// Clean up old stdout, exit code, and agent-pid files for a phase before starting.
pub fn cleanup_phase_files(project_root: &Path, phase: u32) {
    let _ = std::fs::remove_file(stdout_path(project_root, phase));
    let _ = std::fs::remove_file(exit_code_path(project_root, phase));
    let _ = std::fs::remove_file(agent_pid_path(project_root, phase));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitFlowConfig;
    use crate::state::{Agent, State};
    use std::process::Command;

    fn state_in(root: &Path, phase: u32) -> State {
        let mut state = State::new(phase, Agent::Claude, root.to_path_buf());
        state.step = crate::state::Step::Executing;
        state
    }

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo_with_feature_commit(root: &Path, phase: u32) {
        git(root, &["init"]);
        git(root, &["config", "user.email", "devflow@example.com"]);
        git(root, &["config", "user.name", "DevFlow Tests"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        git(root, &["checkout", "-b", "develop"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(root, &["add", "README.md"]);
        git(root, &["commit", "-m", "base"]);

        let branch = format!("feature/phase-{phase:02}");
        git(root, &["checkout", "-b", &branch]);
        std::fs::write(root.join("phase.txt"), "feature work\n").unwrap();
        git(root, &["add", "phase.txt"]);
        git(root, &["commit", "-m", "feature work"]);
    }

    /// Like `init_repo_with_feature_commit`, but the feature branch sits at
    /// develop's tip with **no** extra commit (0 commits ahead).
    fn init_repo_with_feature_no_commit(root: &Path, phase: u32) {
        git(root, &["init"]);
        git(root, &["config", "user.email", "devflow@example.com"]);
        git(root, &["config", "user.name", "DevFlow Tests"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        git(root, &["checkout", "-b", "develop"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git(root, &["add", "README.md"]);
        git(root, &["commit", "-m", "base"]);

        let branch = format!("feature/phase-{phase:02}");
        git(root, &["checkout", "-b", &branch]);
    }

    #[test]
    fn parse_success_marker() {
        let stdout = "some output\nDEVFLOW_RESULT: {\"status\":\"success\"}\n";
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_failed_marker_with_reason() {
        let stdout =
            "work done\nDEVFLOW_RESULT: {\"status\":\"failed\",\"reason\":\"clippy errors\"}\n";
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.unwrap(), "clippy errors");
    }

    #[test]
    fn parse_missing_marker_returns_none() {
        let stdout = "just some output\nno marker here\n";
        assert!(parse_devflow_result(stdout).is_none());
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        let stdout = "DEVFLOW_RESULT: {not valid json}\n";
        assert!(parse_devflow_result(stdout).is_none());
    }

    #[test]
    fn parse_lowercase_marker() {
        let stdout = "devflow_result: {\"status\":\"success\"}\n";
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_marker_without_space_after_colon() {
        let stdout = "DEVFLOW_RESULT:{\"status\":\"success\"}\n";
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_lowercase_no_space_marker() {
        // Lowercase prefix AND no space after the colon — the combination that
        // the Phase 6 review flagged as uncovered.
        let stdout = "devflow_result:{\"status\":\"success\"}\n";
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_finds_last_marker_in_tail() {
        // Multiple markers — should find the last one.
        let stdout = "DEVFLOW_RESULT: {\"status\":\"failed\"}\nsome more output\nDEVFLOW_RESULT: {\"status\":\"success\"}\n";
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    #[test]
    fn parse_marker_only_in_last_4000_chars() {
        // Marker beyond 4000 chars from end should not be found.
        let prefix = "a".repeat(5000);
        let stdout = format!("DEVFLOW_RESULT: {{\"status\":\"success\"}}\n{prefix}");
        assert!(parse_devflow_result(&stdout).is_none());
    }

    #[test]
    fn parse_marker_with_commits_and_summary() {
        let stdout = r#"DEVFLOW_RESULT: {"status":"success","commits":3,"summary":"added tests"}"#;
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.commits, Some(3));
        assert_eq!(result.summary.unwrap(), "added tests");
    }

    #[test]
    fn parse_marker_inside_json_result_envelope() {
        // Claude --output-format json wraps the final text in a `result` field
        // with embedded newlines escaped.
        let stdout = r#"{"type":"result","subtype":"success","result":"All done.\nDEVFLOW_RESULT: {\"status\": \"success\", \"commits\": 2}","session_id":"abc"}"#;
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.commits, Some(2));
    }

    #[test]
    fn parse_failed_marker_inside_json_envelope() {
        let stdout = r#"{"result":"work\nDEVFLOW_RESULT: {\"status\": \"failed\", \"reason\": \"tests failed\"}"}"#;
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.unwrap(), "tests failed");
    }

    #[test]
    fn parse_json_envelope_without_marker_returns_none() {
        let stdout = r#"{"result":"did some work but forgot the marker","session_id":"x"}"#;
        assert!(parse_devflow_result(stdout).is_none());
    }

    #[test]
    fn detect_claude_json_rate_limit_by_subtype() {
        let stdout = r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z","result":"rate limited"}"#;
        assert_eq!(
            detect_rate_limit(stdout).as_deref(),
            Some("2026-06-18T15:45:30Z")
        );
    }

    #[test]
    fn detect_claude_json_rate_limit_by_429() {
        let stdout = r#"{"type":"result","api_error_status":429,"error":{"message":"Too many requests. Try later."}}"#;
        assert_eq!(
            detect_rate_limit(stdout).as_deref(),
            Some("Too many requests. Try later.")
        );
    }

    #[test]
    fn detect_codex_try_again_rate_limit() {
        let stdout = "Usage limit reached. Try again at 3:45 PM.\n";
        assert_eq!(detect_rate_limit(stdout).as_deref(), Some("3:45 PM"));
    }

    #[test]
    fn detect_rate_limit_ignores_normal_stdout() {
        let stdout = "implemented feature\nDEVFLOW_RESULT: {\"status\":\"success\"}\n";
        assert!(detect_rate_limit(stdout).is_none());
    }

    #[test]
    fn evaluate_layer1_reports_rate_limited_without_marker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 7),
            r#"{"type":"result","subtype":"error_rate_limit","retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        let result = evaluate_layer1(dir.path(), 7).unwrap();

        assert_eq!(result.status, AgentStatus::RateLimited);
        assert_eq!(
            result.reason.as_deref(),
            Some("rate limited until 2026-06-18T15:45:30Z")
        );
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

    #[test]
    fn evaluate_agent_result_reads_files_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 6),
            "done\nDEVFLOW_RESULT: {\"status\":\"success\",\"commits\":2,\"summary\":\"ok\"}\n",
        )
        .unwrap();
        std::fs::write(exit_code_path(dir.path(), 6), "0").unwrap();
        let state = state_in(dir.path(), 6);

        let result = evaluate_agent_result(dir.path(), &state, &GitFlowConfig::default()).unwrap();

        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.commits, Some(2));
        assert_eq!(result.summary.as_deref(), Some("ok"));
    }

    #[test]
    fn evaluate_layer1_finds_devflow_result_in_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 3),
            "output\ndevflow_result: {\"status\":\"failed\",\"reason\":\"bad output\"}\n",
        )
        .unwrap();

        let result = evaluate_layer1(dir.path(), 3).unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.as_deref(), Some("bad output"));
    }

    #[test]
    fn evaluate_layer2_falls_back_to_exit_code_and_commit_count() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_commit(dir.path(), 4);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), 4), "0").unwrap();
        let state = state_in(dir.path(), 4);

        let result = evaluate_layer2(dir.path(), 4, &state, &GitFlowConfig::default())
            .unwrap()
            .unwrap();

        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.commits, Some(1));
        assert!(result.reason.unwrap().contains("1 commits"));
    }

    #[test]
    fn evaluate_layer2_exit_zero_no_commits_is_failed() {
        // exit=0 but the feature branch has 0 commits ahead of develop →
        // "no work done" failure (the Layer 2 middle branch).
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_no_commit(dir.path(), 4);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), 4), "0").unwrap();
        let state = state_in(dir.path(), 4);

        let result = evaluate_layer2(dir.path(), 4, &state, &GitFlowConfig::default())
            .unwrap()
            .unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.commits, Some(0));
        assert!(result.reason.unwrap().contains("no commits"));
    }

    #[test]
    fn evaluate_layer2_nonzero_exit_is_failed() {
        // Non-zero exit code → failure regardless of commit count.
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_commit(dir.path(), 4);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), 4), "1").unwrap();
        let state = state_in(dir.path(), 4);

        let result = evaluate_layer2(dir.path(), 4, &state, &GitFlowConfig::default())
            .unwrap()
            .unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.exit_code, Some(1));
        assert!(result.reason.unwrap().contains("exited with code 1"));
    }

    #[test]
    fn evaluate_layer3_falls_back_to_commit_count() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_commit(dir.path(), 5);

        let result = evaluate_layer3(dir.path(), 5, &GitFlowConfig::default()).unwrap();

        assert_eq!(result.status, AgentStatus::Unknown);
        assert_eq!(result.exit_code, None);
        assert_eq!(result.commits, Some(1));
        assert!(result.reason.unwrap().contains("1 commits"));
    }
}
