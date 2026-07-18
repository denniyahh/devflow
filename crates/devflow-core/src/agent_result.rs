//! Agent completion detection — parses DEVFLOW_RESULT markers and evaluates
//! exit codes to determine whether a coding agent succeeded or failed.
//!
//! Four-layer decision engine:
//! 0. Run operator-authored external post-condition probes (authoritative failure)
//! 1. Parse DEVFLOW_RESULT from agent stdout (authoritative for ordinary plans)
//! 2. Exit code + commit count gate (reliable fallback)
//! 3. Process gone + commits exist (last resort warning)

use crate::config::GitFlowConfig;
use crate::stage::Stage;
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
    /// The Validate stage's self-reported verdict — distinct from `status`.
    /// `status` reports whether the stage's task (running `/gsd-validate-phase`)
    /// completed; `verdict` reports whether validation ITSELF passed. Only
    /// `Some(Verdict::Pass)` should advance Validate to Ship; `Some(Verdict::Gaps)`
    /// and `None` both gate/loop back to Code (see `advance()`'s Validate arm).
    /// Ignored entirely for non-Validate stages.
    ///
    /// Deserialized leniently via [`deserialize_verdict_lenient`]: an absent,
    /// unknown, or mis-cased value becomes `None` rather than failing the
    /// whole `AgentResult` parse (T-13-14) — a malformed verdict must never
    /// silently drop a valid `status` to Layer 2.
    #[serde(default, deserialize_with = "deserialize_verdict_lenient")]
    pub verdict: Option<Verdict>,
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

/// The Validate stage's self-reported verdict (13b verdict-vs-ran split).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Validation found no gaps — ready to advance to Ship.
    Pass,
    /// Validation found gaps that still need fixing — must loop back to Code
    /// (or gate, depending on the consecutive-failure threshold).
    Gaps,
}

/// Deserialize `verdict` leniently: an absent, unknown, or mis-cased value
/// (e.g. `"wat"`, `"Pass"`) becomes `Ok(None)` rather than an error, so a
/// malformed verdict never fails the whole `from_str::<AgentResult>` parse
/// and silently drops a valid `status` to Layer 2 (T-13-14, consensus #5).
///
/// Matching is intentionally exact-case (only the wire-format lowercase
/// strings `"pass"`/`"gaps"` are accepted) — a mis-cased value like `"Pass"`
/// is NOT case-folded into a match; it is treated the same as an unknown
/// value and maps to `None`, so a subtly wrong-case verdict fails safe
/// (gate/loop) instead of silently passing.
///
/// WR-09 (13-REVIEW.md): decodes as `serde_json::Value` first, then only
/// pattern-matches the string case — a non-string JSON type (`true`, `123`,
/// an object) is a wrong *type*, not a malformed string value, and must
/// still fall through to `None` rather than erroring out the entire
/// `AgentResult` parse (the same guarantee this deserializer already gives
/// mis-cased/unknown string values).
fn deserialize_verdict_lenient<'de, D>(deserializer: D) -> Result<Option<Verdict>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = <Option<serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(raw.and_then(|v| {
        v.as_str().and_then(|s| match s {
            "pass" => Some(Verdict::Pass),
            "gaps" => Some(Verdict::Gaps),
            _ => None,
        })
    }))
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
    // This heuristic exists for Codex's PLAIN-TEXT output. JSONL event lines
    // are authoritative and handled by parse_codex_event_result — scanning
    // them here false-positives on document content echoed into events
    // (13-06 dogfood finding: GSD reference tables mentioning "rate limiting"
    // were read by the agent, echoed into an `item.completed` payload, and
    // this scan returned that entire multi-KB line as the "retry time").
    let stdout: String = stdout
        .lines()
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .map(|v| !v.is_object())
                .unwrap_or(true)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let stdout = stdout.as_str();
    let lower = stdout.to_ascii_lowercase();
    if let Some(idx) = lower.find("try again at ") {
        let start = idx + "try again at ".len();
        let retry = stdout[start..]
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .trim_end_matches(['.', ',', ';'])
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

// WR-12 (13-REVIEW.md), revised: these traversal helpers run on the coding
// agent's raw stdout (via detect_claude_rate_limit, which every `devflow
// advance` invocation runs through evaluate_layer1), so deeply nested JSON —
// accidental or adversarial — must not stack-overflow the process. The
// traversal is iterative (an explicit worklist), so nesting depth never
// consumes call stack and no depth cap is needed. The first WR-12 fix capped
// recursion at 64, which silently missed keys at depths 64–128 — nesting
// serde_json's default 128-level parse recursion limit (the only producer of
// these `Value`s) accepts just fine.

/// Depth-first pre-order scan over every JSON object in `value`, returning
/// the first `Some` produced by `visit` on an object's map.
fn json_scan<'a, T>(
    value: &'a serde_json::Value,
    visit: impl Fn(&'a serde_json::Map<String, serde_json::Value>) -> Option<T>,
) -> Option<T> {
    let mut stack = vec![value];
    while let Some(current) = stack.pop() {
        match current {
            serde_json::Value::Object(map) => {
                if let Some(found) = visit(map) {
                    return Some(found);
                }
                // Push in reverse so pop order preserves document order.
                for child in map.values().rev() {
                    stack.push(child);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values.iter().rev() {
                    stack.push(child);
                }
            }
            _ => {}
        }
    }
    None
}

fn json_has_str(value: &serde_json::Value, key: &str, expected: &str) -> bool {
    json_scan(value, |map| {
        (map.get(key)?.as_str()? == expected).then_some(())
    })
    .is_some()
}

fn json_has_i64(value: &serde_json::Value, key: &str, expected: i64) -> bool {
    json_scan(value, |map| {
        (map.get(key)?.as_i64()? == expected).then_some(())
    })
    .is_some()
}

fn json_find_key<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    json_scan(value, |map| map.get(key))
}

fn json_scalar_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Read the top-level `is_error` boolean (and, if present, `num_turns`) from
/// a Claude JSON result envelope (`--output-format json`) and treat
/// `is_error: true` as an authoritative Layer-1 failure.
///
/// This is checked BEFORE the `DEVFLOW_RESULT` marker path in
/// [`evaluate_layer1`], so `is_error: true` OVERRIDES a stale/echoed success
/// marker embedded in the same envelope's `result` text — the envelope is
/// authoritative for errors. `is_error` absent or `false` returns `None`,
/// deferring to the marker path and, ultimately, Layer 2. It runs AFTER
/// `detect_claude_rate_limit`, though: rate-limit envelopes also carry
/// `is_error: true`, and the specific `RateLimited` classification (which
/// drives sequentagent's handoff and the resume cron) must win over this
/// generic `Failed`.
///
/// Per RESEARCH Pitfall 5, `is_error` (not specific `subtype` strings) is
/// the documented, stable signal — this does not special-case non-success
/// subtype values beyond what already exists in `detect_claude_rate_limit`.
fn detect_claude_envelope_failure(stdout: &str) -> Option<AgentResult> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let is_error = value.get("is_error")?.as_bool()?;
    if !is_error {
        return None;
    }

    let num_turns = value.get("num_turns").and_then(serde_json::Value::as_u64);
    let base_reason = value
        .get("result")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("subtype")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "agent reported is_error".to_string());
    let reason = match num_turns {
        Some(n) => format!("{base_reason} (num_turns: {n})"),
        None => base_reason,
    };

    Some(AgentResult {
        status: AgentStatus::Failed,
        exit_code: None,
        reason: Some(reason),
        commits: None,
        summary: None,
        verdict: None,
    })
}

/// Determine whether a set of parsed JSONL lines look like a Codex `--json`
/// event stream (as opposed to a single-document Claude envelope or plain
/// text) — i.e. at least one line is a `thread.started` or `turn.*` event.
fn is_codex_event_stream(events: &[serde_json::Value]) -> bool {
    events.iter().any(|v| {
        v.get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|t| t == "thread.started" || t.starts_with("turn."))
    })
}

/// Parse a Codex `--json` JSONL event stream (one JSON object per line) and
/// look at the LAST terminal event (`turn.completed` / `turn.failed`).
///
/// Only decisive when the captured stdout is actually a Codex event stream
/// (per [`is_codex_event_stream`]) — a single-document Claude envelope
/// (`type: "result"`, no `turn.*` lines) is not consumed here and returns
/// `None`, so the Claude envelope/marker paths handle it instead.
///
/// `turn.failed` is decisive: returns `AgentStatus::Failed` with `reason`
/// from `error.message`. A final `turn.completed` with no `DEVFLOW_RESULT`
/// marker returns `None` (defers to Layer 2) rather than an unconditional
/// Success — a marker-less turn must not silently advance a stage (this is
/// the composition fix that keeps a marker-less Validate run from
/// false-passing to Ship).
///
/// NOTE: written against the documented `--json` event schema (thread.started
/// / turn.started / item.* / turn.completed with usage / turn.failed with
/// error.message) but not yet verified against the installed Codex CLI
/// version — the 13-06 dogfood run captures real output and reconciles any
/// delta, the same empirical practice 12-12-SUMMARY.md used for Claude.
fn parse_codex_event_result(stdout: &str) -> Option<AgentResult> {
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();

    if !is_codex_event_stream(&events) {
        return None;
    }

    // Codex delivers the agent's DEVFLOW_RESULT self-report inside an
    // `agent_message` item's `text` — never as a raw stdout line — so the
    // top-level marker scan cannot see it (13-06 dogfood finding: a Codex
    // `DEVFLOW_RESULT: failed` was invisible and the run fell through to
    // heuristics). The decoded `text` is a plain marker line; reuse the
    // marker parser on it. Last marker wins, matching parse_marker_lines.
    let marker = events.iter().rev().find_map(|v| {
        if v.get("type").and_then(serde_json::Value::as_str) != Some("item.completed") {
            return None;
        }
        let item = v.get("item")?;
        if item.get("type").and_then(serde_json::Value::as_str) != Some("agent_message") {
            return None;
        }
        let text = item.get("text").and_then(serde_json::Value::as_str)?;
        parse_marker_lines(text)
    });
    if marker.is_some() {
        return marker;
    }

    let terminal = events.iter().rev().find(|v| {
        matches!(
            v.get("type").and_then(serde_json::Value::as_str),
            Some("turn.completed") | Some("turn.failed")
        )
    })?;

    if terminal.get("type").and_then(serde_json::Value::as_str) != Some("turn.failed") {
        // turn.completed (or any other terminal we don't recognize) defers
        // to Layer 2 rather than an unconditional Success.
        return None;
    }

    let reason = terminal
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "codex turn failed".to_string());

    Some(AgentResult {
        status: AgentStatus::Failed,
        exit_code: None,
        reason: Some(reason),
        commits: None,
        summary: None,
        verdict: None,
    })
}

/// Scan the last ~4000 characters of `stdout` in reverse line order.
///
/// `DEVFLOW_RESULT` markers are ASCII. Searching the bounded tail and returning
/// the last valid marker ensures the agent's final status wins over an earlier
/// prompt echo without requiring the surrounding output to be ASCII.
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

/// Layer 1: Try to detect agent result from the native per-adapter envelope
/// or the DEVFLOW_RESULT marker in stdout.
///
/// Precedence: Claude rate-limit envelope (a SPECIFIC failure that must
/// outrank the generic `is_error` check — rate-limit envelopes carry
/// `is_error: true`, and classifying them `Failed` would kill sequentagent's
/// handoff/cron path) → Claude envelope `is_error: true` (authoritative,
/// overrides a success marker) → DEVFLOW_RESULT marker (portable; works for
/// plain text and a Claude envelope's unwrapped `result` text) → Codex JSONL
/// event stream (`turn.failed` decisive; `turn.completed` defers) → Codex
/// plain-text rate-limit heuristic (least authoritative, stays last).
pub fn evaluate_layer1(project_root: &Path, phase: u32) -> Option<AgentResult> {
    let stdout_path = devflow_dir(project_root).join(format!("phase-{:02}-stdout", phase));
    // Read lossily: in monitor mode the agent's stdout reaches this file via
    // raw sh redirection, so one invalid UTF-8 byte in a strict
    // read_to_string would silently disable ALL Layer-1 detection (marker,
    // envelope, rate limit) — the same failure class CR-01 (13-REVIEW.md)
    // fixed in the blocking-mode capture.
    let bytes = std::fs::read(&stdout_path).ok()?;
    let stdout = String::from_utf8_lossy(&bytes);
    detect_claude_rate_limit(&stdout)
        .map(rate_limited_result)
        .or_else(|| detect_claude_envelope_failure(&stdout))
        .or_else(|| parse_devflow_result(&stdout))
        .or_else(|| parse_codex_event_result(&stdout))
        .or_else(|| detect_codex_rate_limit(&stdout).map(rate_limited_result))
}

/// Build the `RateLimited` result Layer 1 reports for a detected retry hint.
fn rate_limited_result(retry: String) -> AgentResult {
    AgentResult {
        status: AgentStatus::RateLimited,
        exit_code: None,
        reason: Some(format!("rate limited until {retry}")),
        commits: None,
        summary: None,
        verdict: None,
    }
}

/// Layer 2: Use exit code + commit count to determine result.
///
/// Reads exit code from `.devflow/phase-NN-exit` file.
/// Counts commits in `feature/phase-NN` branch (if it exists).
///
/// The commit-count gate ("no commits → failed") is scoped to `stage` — it
/// only applies to `Stage::Plan`/`Stage::Code` (checked via an explicit
/// `matches!`, NOT `Stage::is_agent_stage()`, since that also includes
/// `Define`, which legitimately produces zero commits). `exit≠0` is ALWAYS
/// `Failed`, for every stage — only the `exit=0`/zero-commits branch is
/// stage-scoped.
///
/// Decision matrix:
///   exit≠0                                              → Failed (ALL stages)
///   exit=0, stage in {Plan, Code}, commits=0             → Failed ("no work done")
///   exit=0, stage in {Plan, Code}, commits>0             → Success
///   exit=0, stage NOT in {Plan, Code} (Define/Validate/Ship), commits=0 → Success
///           (not commit-gated; Validate's real pass signal is its verdict,
///           not a bare zero-commit — see Task 2's turn.completed deferral)
///   exit unknown                                         → fall to Layer 3 (return None)
///
/// WR-06 (13-REVIEW.md): takes only the explicit `project_root` parameter
/// for both the `.devflow/` file paths and the git subprocess `current_dir`
/// — previously it also accepted `state: &State` and used `state.project_root`
/// for the git calls, which every caller happened to pass consistently with
/// `project_root` but which the function itself had no way to enforce.
pub fn evaluate_layer2(
    project_root: &Path,
    phase: u32,
    git_flow: &GitFlowConfig,
    stage: Stage,
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
        .current_dir(project_root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let commits: u32 = if branch_exists {
        let range = format!("{}..{branch}", git_flow.develop);
        std::process::Command::new("git")
            .args(["rev-list", "--count", &range])
            .current_dir(project_root)
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(0)
    } else {
        0
    };

    let commit_gated = matches!(stage, Stage::Plan | Stage::Code);
    let no_work_done = commit_gated && commits == 0;

    Ok(Some(AgentResult {
        status: if exit_code != 0 || no_work_done {
            AgentStatus::Failed
        } else {
            AgentStatus::Success
        },
        exit_code: Some(exit_code),
        reason: if exit_code != 0 {
            Some(format!(
                "agent exited with code {} ({} commits on {})",
                exit_code, commits, branch
            ))
        } else if no_work_done {
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
        verdict: None,
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
        verdict: None,
    })
}

/// Layer 0: run explicitly operator-approved external post-condition probes.
///
/// A failed probe outranks every agent-controlled signal. Successful probes
/// defer to the existing Layer 1/2/3 cascade so ordinary completion evidence
/// is still required. With no declarations (or when disabled), behavior is
/// byte-for-byte the pre-Phase-16 cascade.
fn evaluate_layer0(
    project_root: &Path,
    state: &State,
    approved_commands: Option<&[String]>,
) -> Option<AgentResult> {
    if state.stage != Stage::Code || !crate::config::external_verify_enabled(project_root) {
        return None;
    }

    let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
    let commands = crate::verify::external_verify_commands(execution_root, state.phase);
    if commands.is_empty() {
        return approved_commands.map(|_| AgentResult {
            status: AgentStatus::Failed,
            exit_code: None,
            reason: Some(
                "external verification approval mismatch; PLAN declaration was removed".into(),
            ),
            commits: None,
            summary: None,
            verdict: None,
        });
    }
    let Some(approved_commands) = approved_commands else {
        return Some(AgentResult {
            status: AgentStatus::Failed,
            exit_code: None,
            reason: Some(format!(
                "external verification is not approved; set {} to the reviewed JSON command array",
                crate::verify::TRUST_EXTERNAL_VERIFY_ENV
            )),
            commits: None,
            summary: None,
            verdict: None,
        });
    };
    if commands != approved_commands {
        return Some(AgentResult {
            status: AgentStatus::Failed,
            exit_code: None,
            reason: Some("external verification approval mismatch; PLAN commands changed".into()),
            commits: None,
            summary: None,
            verdict: None,
        });
    }
    commands
        .into_iter()
        .find(|command| !crate::verify::run_external_verification(command, execution_root))
        .map(|command| AgentResult {
            status: AgentStatus::Failed,
            exit_code: None,
            reason: Some(format!("external verification failed: {command}")),
            commits: None,
            summary: None,
            verdict: None,
        })
}

/// Full four-layer evaluation: returns the best available AgentResult.
pub fn evaluate_agent_result(
    project_root: &Path,
    state: &State,
    git_flow: &GitFlowConfig,
) -> Result<AgentResult, ResultError> {
    let approval = crate::verify::external_verification_approval();
    evaluate_agent_result_inner(project_root, state, git_flow, approval.as_deref())
}

fn evaluate_agent_result_inner(
    project_root: &Path,
    state: &State,
    git_flow: &GitFlowConfig,
    approved_commands: Option<&[String]>,
) -> Result<AgentResult, ResultError> {
    // Layer 0: operator-authored external post-condition (authoritative failure)
    if let Some(result) = evaluate_layer0(project_root, state, approved_commands) {
        return Ok(result);
    }

    // Layer 1: DEVFLOW_RESULT marker (authoritative)
    if let Some(result) = evaluate_layer1(project_root, state.phase) {
        return Ok(result);
    }

    // Layer 2: Exit code + commit gate
    if let Some(result) = evaluate_layer2(project_root, state.phase, git_flow, state.stage)? {
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

/// Path where the agent's stderr is captured for a given phase.
/// Lives alongside `stdout_path` under `.devflow/`.
pub fn stderr_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{phase:02}-stderr.log"))
}

/// Path to the exit code file for a given phase.
pub fn exit_code_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-exit", phase))
}

/// Path to the file where the monitor records the launched agent's PID.
pub fn agent_pid_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-agent-pid", phase))
}

/// Path to the archived-capture-history directory for a phase (16b).
///
/// `.devflow/history/phase-NN/` holds retained per-stage capture generations
/// so a false-positive self-report can be diagnosed after the fact. Exposed
/// as a constructor (rather than inlined at each call site) so downstream
/// tooling (16h in 16-07's correlation, 16i in 16-05's enumeration) always
/// derives the path from here instead of hardcoding it.
pub fn history_dir(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root)
        .join("history")
        .join(format!("phase-{:02}", phase))
}

/// Monotonically increasing tie-breaker appended to the nanosecond timestamp
/// used to stamp archived generations, so two archives issued within the
/// same nanosecond (possible in a tight test loop) never collide.
static ARCHIVE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A stamp unique within this process, used to name an archived generation.
/// The outgoing stage's name is not available at the `archive_phase_files`
/// call site (see `launch_stage` in main.rs), so a monotonic timestamp is
/// used instead — sufficient to order and identify generations.
fn archive_stamp() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = ARCHIVE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{nanos}-{seq}")
}

/// Archive the prior stage's stdout/exit captures into bounded per-phase
/// history instead of wiping them outright, so a false-positive self-report
/// can be diagnosed after the fact (16b). Replaces the old
/// `cleanup_phase_files`, which deleted these files unconditionally.
///
/// At most `retain` capture generations are kept per phase; older ones are
/// pruned (see [`prune_history`]). The agent-pid file is still removed
/// outright — it is process bookkeeping, not diagnostic output. When there
/// is nothing to archive (first launch), this is a no-op success.
pub fn archive_phase_files(
    project_root: &Path,
    evidence_root: &Path,
    phase: u32,
    retain: usize,
) -> Result<Option<String>, std::io::Error> {
    archive_phase_files_with_stamp(project_root, evidence_root, phase, retain, &archive_stamp())
}

fn archive_phase_files_with_stamp(
    project_root: &Path,
    evidence_root: &Path,
    phase: u32,
    retain: usize,
    stamp: &str,
) -> Result<Option<String>, std::io::Error> {
    let _ = std::fs::remove_file(agent_pid_path(project_root, phase));

    let stdout_src = stdout_path(project_root, phase);
    let exit_src = exit_code_path(project_root, phase);
    let stdout_exists = stdout_src.exists();
    let exit_exists = exit_src.exists();
    if !stdout_exists && !exit_exists {
        return Ok(None); // Nothing to archive — first launch.
    }

    let history_dir = history_dir(project_root, phase);
    std::fs::create_dir_all(&history_dir)?;

    let staging_dir = history_dir.join(format!(".pending-{stamp}"));
    std::fs::create_dir(&staging_dir)?;
    let stdout_stage = staging_dir.join("stdout");
    let exit_stage = staging_dir.join("exit");
    let review_stage = staging_dir.join("REVIEW.md");
    let stdout_dest = history_dir.join(format!("{stamp}-stdout"));
    let exit_dest = history_dir.join(format!("{stamp}-exit"));
    let review_dest = history_dir.join(format!("{stamp}-REVIEW.md"));
    let review_src = phase_review_path(evidence_root, phase);

    let mut stdout_staged = false;
    let mut exit_staged = false;
    let mut stdout_published = false;
    let mut exit_published = false;
    let mut review_published = false;

    let archive_result = (|| -> Result<(), std::io::Error> {
        if stdout_exists {
            std::fs::rename(&stdout_src, &stdout_stage)?;
            stdout_staged = true;
        }
        if exit_exists {
            std::fs::rename(&exit_src, &exit_stage)?;
            exit_staged = true;
        }
        if let Some(review) = &review_src {
            std::fs::copy(review, &review_stage)?;
        }

        if stdout_exists {
            std::fs::rename(&stdout_stage, &stdout_dest)?;
            stdout_staged = false;
            stdout_published = true;
        }
        if exit_exists {
            std::fs::rename(&exit_stage, &exit_dest)?;
            exit_staged = false;
            exit_published = true;
        }
        if review_src.is_some() {
            std::fs::rename(&review_stage, &review_dest)?;
            review_published = true;
        }
        Ok(())
    })();

    if let Err(error) = archive_result {
        let mut rollback_error = None;
        let mut restore = |from: &Path, to: &Path| {
            if let Err(error) = std::fs::rename(from, to)
                && rollback_error.is_none()
            {
                rollback_error = Some(error);
            }
        };
        if stdout_published {
            restore(&stdout_dest, &stdout_src);
        } else if stdout_staged {
            restore(&stdout_stage, &stdout_src);
        }
        if exit_published {
            restore(&exit_dest, &exit_src);
        } else if exit_staged {
            restore(&exit_stage, &exit_src);
        }
        if review_published {
            let _ = std::fs::remove_file(&review_dest);
        }
        let _ = std::fs::remove_dir_all(&staging_dir);

        if let Some(rollback_error) = rollback_error {
            return Err(std::io::Error::new(
                error.kind(),
                format!("{error}; archive rollback failed: {rollback_error}"),
            ));
        }
        return Err(error);
    }

    let _ = std::fs::remove_dir(&staging_dir);

    prune_history(&history_dir, retain);
    Ok(Some(stamp.to_string()))
}

fn phase_review_path(project_root: &Path, phase: u32) -> Option<PathBuf> {
    let phases = std::fs::read_dir(project_root.join(".planning/phases")).ok()?;
    let prefix = format!("{phase:02}-");
    for entry in phases.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&prefix))
        {
            let review = entry.path().join(format!("{phase:02}-REVIEW.md"));
            if review.exists() {
                return Some(review);
            }
        }
    }
    None
}

/// Keep only the newest `retain` capture generations under `history_dir`,
/// deleting older ones. Generations are grouped by their stamp (the shared
/// prefix of a `{stamp}-stdout`/`{stamp}-exit` pair, split off the trailing
/// `-stdout`/`-exit` suffix via `rsplit_once`) and ordered lexicographically,
/// which matches numeric/chronological order for the fixed-width nanosecond
/// stamps `archive_stamp` produces. Ordering parses both numeric components;
/// the process-local sequence is intentionally not fixed-width.
fn prune_history(history_dir: &Path, retain: usize) {
    let Ok(entries) = std::fs::read_dir(history_dir) else {
        return;
    };

    let mut stamps: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            name.rsplit_once('-')
                .map(|(stamp, _suffix)| stamp.to_string())
        })
        .collect();
    stamps.sort_by_key(|stamp| {
        let mut parts = stamp.split('-');
        let nanos = parts
            .next()
            .and_then(|part| part.parse::<u128>().ok())
            .unwrap_or(0);
        let sequence = parts
            .next()
            .and_then(|part| part.parse::<u64>().ok())
            .unwrap_or(0);
        (nanos, sequence)
    });
    stamps.dedup();

    if stamps.len() <= retain {
        return;
    }

    let to_remove = stamps.len() - retain;
    for stamp in &stamps[..to_remove] {
        let _ = std::fs::remove_file(history_dir.join(format!("{stamp}-stdout")));
        let _ = std::fs::remove_file(history_dir.join(format!("{stamp}-exit")));
        let _ = std::fs::remove_file(history_dir.join(format!("{stamp}-REVIEW.md")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitFlowConfig;
    use crate::mode::Mode;
    use crate::stage::Stage;
    use crate::state::{AgentKind, State};
    use std::process::Command;

    fn state_in(root: &Path, phase: u32) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
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
    fn parse_marker_lines_returns_last_marker_in_long_output() {
        let stdout = format!(
            "{}\nDEVFLOW_RESULT: {{\"status\":\"failed\"}}\n{}\n\
             DEVFLOW_RESULT: {{\"status\":\"success\"}}\n",
            "prefix".repeat(900),
            "tail output".repeat(100)
        );

        let result = parse_marker_lines(&stdout).unwrap();

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

    /// WR-12 (13-REVIEW.md), revised: `json_has_str`/`json_has_i64`/
    /// `json_find_key` run on the coding agent's raw stdout via
    /// `detect_claude_rate_limit`, which every `devflow advance` invocation
    /// goes through. Deeply nested JSON — accidental or adversarial — must
    /// not stack-overflow the process, and a real marker at any depth
    /// serde_json will parse (its default recursion limit is exactly 128)
    /// must still be FOUND — the first WR-12 fix capped traversal at 64 and
    /// silently misclassified rate-limit markers at depths 64–128.
    #[test]
    fn detect_rate_limit_finds_marker_in_deeply_nested_json_without_overflow() {
        // 100 levels: parseable by serde_json (limit 128), deeper than the
        // removed 64-level traversal cap that used to hide the marker.
        const DEPTH: usize = 100;
        let mut stdout = String::new();
        for _ in 0..DEPTH {
            stdout.push_str(r#"{"nested":"#);
        }
        stdout.push_str(r#"{"type":"result","subtype":"error_rate_limit","retry_after":"deep"}"#);
        for _ in 0..DEPTH {
            stdout.push('}');
        }

        // Must return promptly without crashing AND find the buried marker —
        // the iterative worklist traversal has no silent-miss window.
        assert_eq!(detect_rate_limit(&stdout).as_deref(), Some("deep"));
    }

    #[test]
    fn detect_rate_limit_ignores_normal_stdout() {
        let stdout = "implemented feature\nDEVFLOW_RESULT: {\"status\":\"success\"}\n";
        assert!(detect_rate_limit(stdout).is_none());
    }

    #[test]
    fn claude_envelope_is_error_detected() {
        let stdout = r#"{"type":"result","subtype":"error","is_error":true,"num_turns":2,"result":"tool call failed","session_id":"abc"}"#;
        let result = detect_claude_envelope_failure(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
    }

    #[test]
    fn claude_is_error_overrides_success_marker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 9),
            r#"{"type":"result","is_error":true,"num_turns":3,"result":"oops\nDEVFLOW_RESULT: {\"status\":\"success\"}","session_id":"abc"}"#,
        )
        .unwrap();

        let result = evaluate_layer1(dir.path(), 9).unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
    }

    #[test]
    fn claude_envelope_is_error_false_defers() {
        let stdout = r#"{"type":"result","is_error":false,"num_turns":1,"result":"did some work","session_id":"abc"}"#;
        assert!(detect_claude_envelope_failure(stdout).is_none());
    }

    #[test]
    fn claude_envelope_marker_still_wins() {
        let stdout = r#"{"type":"result","is_error":false,"result":"done\nDEVFLOW_RESULT: {\"status\":\"success\",\"commits\":2}","session_id":"abc"}"#;
        assert!(detect_claude_envelope_failure(stdout).is_none());
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.commits, Some(2));
    }

    #[test]
    fn codex_event_stream_parses_turn_failed() {
        let stdout = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "{\"type\":\"turn.started\"}\n",
            "{\"type\":\"item.started\",\"item\":{}}\n",
            "{\"type\":\"turn.failed\",\"error\":{\"message\":\"sandbox denied write\"}}\n",
        );
        let result = parse_codex_event_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.as_deref(), Some("sandbox denied write"));
    }

    #[test]
    fn codex_turn_completed_no_marker_defers() {
        let stdout = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "{\"type\":\"turn.started\"}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\n",
        );
        assert!(parse_codex_event_result(stdout).is_none());
    }

    /// 13-06 dogfood regression: Codex delivers the DEVFLOW_RESULT marker
    /// inside an `agent_message` item's text, never as a raw stdout line. A
    /// self-reported failure followed by a bare `turn.completed` must parse
    /// as Failed with the agent's reason — not defer to Layer 2 (which would
    /// see exit 0 and call it a success).
    #[test]
    fn codex_agent_message_marker_failed_wins_over_bare_turn_completed() {
        let stdout = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_7\",\"type\":\"agent_message\",\"text\":\"DEVFLOW_RESULT: {\\\"status\\\": \\\"failed\\\", \\\"reason\\\": \\\"interactive input unavailable\\\"}\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\n",
        );
        let result = parse_codex_event_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(
            result.reason.as_deref(),
            Some("interactive input unavailable")
        );
    }

    #[test]
    fn codex_agent_message_marker_success_short_circuits() {
        let stdout = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_2\",\"type\":\"agent_message\",\"text\":\"DEVFLOW_RESULT: {\\\"status\\\": \\\"success\\\"}\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\n",
        );
        let result = parse_codex_event_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
    }

    /// 13-06 dogfood regression: document content echoed into a JSONL event
    /// (GSD reference tables mentioning "rate limiting") must not trip the
    /// plain-text rate-limit heuristic — it returned the entire multi-KB
    /// event line as the "retry time" and that reached the desktop
    /// notification verbatim.
    #[test]
    fn detect_rate_limit_ignores_json_event_lines() {
        let stdout = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_4\",\"type\":\"command_execution\",\"aggregated_output\":\"| API keys | Rate limiting per key? |\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\n",
        );
        assert_eq!(detect_rate_limit(stdout), None);
    }

    #[test]
    fn detect_rate_limit_still_reads_codex_plain_text() {
        let stdout = "Rate limit reached.\nTry again at 3:45 PM.\n";
        assert_eq!(detect_rate_limit(stdout).as_deref(), Some("3:45 PM"));
    }

    #[test]
    fn codex_event_stream_ignores_progress_and_unparseable_lines() {
        let stdout = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "not json at all\n",
            "{\"type\":\"item.started\",\"item\":{}}\n",
            "{\"type\":\"item.updated\",\"item\":{}}\n",
            "{\"type\":\"turn.failed\",\"error\":{\"message\":\"boom\"}}\n",
        );
        let result = parse_codex_event_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.as_deref(), Some("boom"));
    }

    #[test]
    fn claude_envelope_not_consumed_by_codex_parser() {
        let stdout = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":4,"result":"All done.","session_id":"abc"}"#;
        assert!(parse_codex_event_result(stdout).is_none());
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

    /// A real Claude rate-limit envelope carries `is_error: true` alongside
    /// `subtype: "error_rate_limit"`. The specific RateLimited classification
    /// must outrank the generic is_error → Failed path, or sequentagent's
    /// handoff/cron machinery never triggers for the exact case it exists for.
    #[test]
    fn evaluate_layer1_rate_limit_envelope_with_is_error_is_rate_limited() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 7),
            r#"{"type":"result","subtype":"error_rate_limit","is_error":true,"retry_after":"2026-06-18T15:45:30Z"}"#,
        )
        .unwrap();

        let result = evaluate_layer1(dir.path(), 7).unwrap();

        assert_eq!(result.status, AgentStatus::RateLimited);
        assert_eq!(
            result.reason.as_deref(),
            Some("rate limited until 2026-06-18T15:45:30Z")
        );
    }

    /// CR-01 (13-REVIEW.md) completion: the monitor path writes raw agent
    /// bytes to the stdout file via sh redirection, so evaluate_layer1 must
    /// tolerate invalid UTF-8 rather than silently disabling all Layer-1
    /// detection (the blocking-mode capture was fixed; the file read here is
    /// the other half of the same bug).
    #[test]
    fn evaluate_layer1_finds_marker_despite_invalid_utf8_bytes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        let mut bytes = b"progress \xff\xfe garbage\n".to_vec();
        bytes.extend_from_slice(
            b"DEVFLOW_RESULT: {\"status\":\"failed\",\"reason\":\"review: bad\"}\n",
        );
        std::fs::write(stdout_path(dir.path(), 5), bytes).unwrap();

        let result = evaluate_layer1(dir.path(), 5).unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.as_deref(), Some("review: bad"));
    }

    #[test]
    fn failing_external_probe_outranks_success_marker() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir
            .path()
            .join(".planning/phases/16-pipeline-reliability-hardening");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join("16-03-PLAN.md"),
            "---\nphase: 16\nexternal_verify: \"test -f externally-shipped\"\n---\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();
        let state = state_in(dir.path(), 16);

        let approval = vec!["test -f externally-shipped".to_string()];
        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert!(
            result
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("external verification failed"))
        );
    }

    #[test]
    fn external_probe_runs_only_after_code_and_reads_execution_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("phase-worktree");
        let phase_dir = worktree.join(".planning/phases/16-reliability");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join("16-01-PLAN.md"),
            "---\nexternal_verify: \"test -f implemented\"\n---\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();
        let mut state = state_in(dir.path(), 16);
        state.worktree_path = Some(worktree.clone());
        state.stage = Stage::Plan;

        let approval = vec!["test -f implemented".to_string()];
        let plan_result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(plan_result.status, AgentStatus::Success);

        state.stage = Stage::Code;
        let code_result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(code_result.status, AgentStatus::Failed);

        std::fs::write(worktree.join("implemented"), "done").unwrap();
        let passing = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(passing.status, AgentStatus::Success);
    }

    #[test]
    fn changed_external_probe_never_inherits_prior_approval() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir.path().join(".planning/phases/16-reliability");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join("16-01-PLAN.md"),
            "---\nexternal_verify: \"touch escaped\"\n---\n",
        )
        .unwrap();
        let state = state_in(dir.path(), 16);
        let approved = vec!["test -f reviewed-artifact".to_string()];

        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approved),
        )
        .unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert!(result.reason.unwrap().contains("approval mismatch"));
        assert!(!dir.path().join("escaped").exists());
    }

    #[test]
    fn removed_external_probe_fails_closed_against_prior_approval() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
        )
        .unwrap();
        let state = state_in(dir.path(), 16);
        let approved = vec!["test -f shipped".to_string()];

        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approved),
        )
        .unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert!(result.reason.unwrap().contains("declaration was removed"));
    }

    #[test]
    fn no_external_declaration_preserves_layer1_result() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"success\",\"commits\":2,\"summary\":\"done\"}\n",
        )
        .unwrap();
        let state = state_in(dir.path(), 16);
        let layer1 = evaluate_layer1(dir.path(), 16).unwrap();

        let full = evaluate_agent_result(dir.path(), &state, &GitFlowConfig::default()).unwrap();

        assert_eq!(
            serde_json::to_value(full).unwrap(),
            serde_json::to_value(layer1).unwrap()
        );
    }

    #[test]
    fn archive_moves_captures_into_history_and_removes_pid_file() {
        // 16b: prior-stage captures must survive a simulated next-launch by
        // appearing under .devflow/history/phase-NN/, not be wiped outright.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(root.join(".devflow/phase-01-stdout"), "prior stdout").unwrap();
        std::fs::write(root.join(".devflow/phase-01-exit"), "0").unwrap();
        std::fs::write(root.join(".devflow/phase-01-agent-pid"), "1234").unwrap();

        archive_phase_files(root, root, 1, 5).unwrap();

        // The live capture paths are gone (moved, not merely deleted).
        assert!(!root.join(".devflow/phase-01-stdout").exists());
        assert!(!root.join(".devflow/phase-01-exit").exists());
        // Agent-pid is bookkeeping, not diagnostic — still removed outright.
        assert!(!root.join(".devflow/phase-01-agent-pid").exists());

        let history = history_dir(root, 1);
        let archived: Vec<_> = std::fs::read_dir(&history)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let archived_stdout = archived
            .iter()
            .find(|name| name.ends_with("-stdout"))
            .expect("stdout capture should be archived into history");
        assert!(archived.iter().any(|name| name.ends_with("-exit")));
        let contents = std::fs::read_to_string(history.join(archived_stdout)).unwrap();
        assert_eq!(contents, "prior stdout");
    }

    #[test]
    fn archive_is_noop_when_nothing_to_archive() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Should not panic when there is nothing to archive (first launch).
        archive_phase_files(root, root, 1, 5).unwrap();
        assert!(!history_dir(root, 1).exists());
    }

    #[test]
    fn archive_handles_missing_devflow_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // No .devflow dir at all — should not panic.
        archive_phase_files(root, root, 1, 5).unwrap();
    }

    #[test]
    fn archive_failure_preserves_live_capture_for_retry() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(stdout_path(root, 1), "evidence").unwrap();
        // A file where the history directory must be forces create_dir_all
        // to fail before the live capture is moved or a monitor can truncate it.
        std::fs::write(root.join(".devflow/history"), "blocked").unwrap();

        assert!(archive_phase_files(root, root, 1, 5).is_err());
        assert_eq!(
            std::fs::read_to_string(stdout_path(root, 1)).unwrap(),
            "evidence"
        );
    }

    #[test]
    fn archive_second_publish_failure_rolls_back_complete_live_pair() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(stdout_path(root, 1), "stdout evidence").unwrap();
        std::fs::write(exit_code_path(root, 1), "17").unwrap();
        let history = history_dir(root, 1);
        std::fs::create_dir_all(history.join("fixed-exit/blocker")).unwrap();

        assert!(archive_phase_files_with_stamp(root, root, 1, 5, "fixed").is_err());

        assert_eq!(
            std::fs::read_to_string(stdout_path(root, 1)).unwrap(),
            "stdout evidence"
        );
        assert_eq!(
            std::fs::read_to_string(exit_code_path(root, 1)).unwrap(),
            "17"
        );
        assert!(!history.join("fixed-stdout").exists());
        assert!(!history.join(".pending-fixed").exists());
    }

    #[test]
    fn archive_review_copy_failure_rolls_back_complete_live_pair() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let evidence_root = root.join("phase-worktree");
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(stdout_path(root, 1), "stdout evidence").unwrap();
        std::fs::write(exit_code_path(root, 1), "23").unwrap();
        let review = evidence_root.join(".planning/phases/01-example/01-REVIEW.md");
        std::fs::create_dir_all(&review).unwrap();

        assert!(archive_phase_files_with_stamp(root, &evidence_root, 1, 5, "review-copy").is_err());

        assert_eq!(
            std::fs::read_to_string(stdout_path(root, 1)).unwrap(),
            "stdout evidence"
        );
        assert_eq!(
            std::fs::read_to_string(exit_code_path(root, 1)).unwrap(),
            "23"
        );
        let history = history_dir(root, 1);
        assert!(!history.join("review-copy-stdout").exists());
        assert!(!history.join("review-copy-exit").exists());
        assert!(!history.join(".pending-review-copy").exists());
    }

    #[test]
    fn archive_snapshots_current_review_into_same_generation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let evidence_root = root.join("phase-worktree");
        std::fs::create_dir_all(root.join(".devflow")).unwrap();
        std::fs::write(stdout_path(root, 1), "attempt").unwrap();
        let phase_dir = evidence_root.join(".planning/phases/01-example");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(phase_dir.join("01-REVIEW.md"), "review one").unwrap();

        let stamp = archive_phase_files(root, &evidence_root, 1, 5)
            .unwrap()
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(history_dir(root, 1).join(format!("{stamp}-REVIEW.md")))
                .unwrap(),
            "review one"
        );
    }

    #[test]
    fn archive_prunes_history_to_retain_count() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".devflow")).unwrap();

        for i in 0..7 {
            std::fs::write(root.join(".devflow/phase-01-stdout"), format!("gen {i}")).unwrap();
            std::fs::write(root.join(".devflow/phase-01-exit"), "0").unwrap();
            archive_phase_files(root, root, 1, 3).unwrap();
        }

        let history = history_dir(root, 1);
        let stdout_count = std::fs::read_dir(&history)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with("-stdout"))
            .count();
        let exit_count = std::fs::read_dir(&history)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with("-exit"))
            .count();
        assert_eq!(stdout_count, 3, "expected at most 3 retained generations");
        assert_eq!(exit_count, 3, "expected at most 3 retained generations");
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

        let result = evaluate_layer2(dir.path(), 4, &GitFlowConfig::default(), state.stage)
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

        let result = evaluate_layer2(dir.path(), 4, &GitFlowConfig::default(), state.stage)
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

        let result = evaluate_layer2(dir.path(), 4, &GitFlowConfig::default(), state.stage)
            .unwrap()
            .unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.exit_code, Some(1));
        assert!(result.reason.unwrap().contains("exited with code 1"));
    }

    #[test]
    fn layer2_nonzero_exit_is_failed_all_stages() {
        // Non-zero exit is Failed regardless of stage — including Define and
        // Validate, which are exempt from the zero-commit gate but NOT from
        // the exit-code check.
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_no_commit(dir.path(), 10);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), 10), "1").unwrap();

        for stage in [
            Stage::Define,
            Stage::Plan,
            Stage::Code,
            Stage::Validate,
            Stage::Ship,
        ] {
            let result = evaluate_layer2(dir.path(), 10, &GitFlowConfig::default(), stage)
                .unwrap()
                .unwrap();
            assert_eq!(
                result.status,
                AgentStatus::Failed,
                "stage {stage:?} should be Failed on nonzero exit"
            );
        }
    }

    #[test]
    fn layer2_skips_commit_gate_for_define_and_validate() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_no_commit(dir.path(), 11);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), 11), "0").unwrap();

        for stage in [Stage::Define, Stage::Validate] {
            let result = evaluate_layer2(dir.path(), 11, &GitFlowConfig::default(), stage)
                .unwrap()
                .unwrap();
            assert_ne!(
                result.status,
                AgentStatus::Failed,
                "stage {stage:?} should not be Failed for zero commits"
            );
        }

        // Code stage with the same zero-commit inputs is still Failed
        // (existing behavior preserved).
        let result = evaluate_layer2(dir.path(), 11, &GitFlowConfig::default(), Stage::Code)
            .unwrap()
            .unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
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

    #[test]
    fn parse_devflow_result_reads_verdict() {
        let stdout = r#"DEVFLOW_RESULT: {"status":"success","verdict":"gaps"}"#;
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, Some(Verdict::Gaps));
    }

    #[test]
    fn parse_devflow_result_reads_verdict_pass() {
        let stdout = r#"DEVFLOW_RESULT: {"status":"success","verdict":"pass"}"#;
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, Some(Verdict::Pass));
    }

    #[test]
    fn parse_devflow_result_verdict_absent_is_none() {
        let stdout = r#"DEVFLOW_RESULT: {"status":"success"}"#;
        let result = parse_devflow_result(stdout).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, None);
    }

    #[test]
    fn parse_devflow_result_malformed_verdict_is_none_not_parse_error() {
        // An unknown verdict string must not fail the whole marker parse —
        // status must still come through as Success with verdict None (T-13-14).
        let unknown = r#"DEVFLOW_RESULT: {"status":"success","verdict":"wat"}"#;
        let result = parse_devflow_result(unknown).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, None);

        // Mis-cased ("Pass" instead of "pass") must also be lenient, not an error.
        let miscased = r#"DEVFLOW_RESULT: {"status":"success","verdict":"Pass"}"#;
        let result = parse_devflow_result(miscased).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, None);
    }

    /// WR-09 (13-REVIEW.md): a `verdict` field present with a non-string
    /// JSON *type* (bool, number, object) must be just as lenient as a
    /// malformed string value — before the fix, deserializing straight to
    /// `Option<String>` errored out the entire `AgentResult` parse for a
    /// type mismatch, defeating the doc comment's "a malformed verdict must
    /// never silently drop a valid status" guarantee for this specific case.
    #[test]
    fn parse_devflow_result_non_string_verdict_type_is_none_not_parse_error() {
        let bool_verdict = r#"DEVFLOW_RESULT: {"status":"success","verdict":true}"#;
        let result = parse_devflow_result(bool_verdict).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, None);

        let numeric_verdict = r#"DEVFLOW_RESULT: {"status":"success","verdict":123}"#;
        let result = parse_devflow_result(numeric_verdict).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, None);

        let object_verdict = r#"DEVFLOW_RESULT: {"status":"success","verdict":{"x":1}}"#;
        let result = parse_devflow_result(object_verdict).unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.verdict, None);
    }
}
