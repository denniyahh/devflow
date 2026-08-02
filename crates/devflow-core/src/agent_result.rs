//! Agent completion detection — parses DEVFLOW_RESULT markers and evaluates
//! exit codes to determine whether a coding agent succeeded or failed.
//!
//! Four-layer decision engine:
//! 0. Run operator-authored external post-condition probes (authoritative failure)
//! 1. Parse DEVFLOW_RESULT from agent stdout (authoritative for ordinary plans)
//! 2. Exit code + commit count gate (reliable fallback)
//! 3. Process gone + commits exist (last resort warning)

use crate::config::GitFlowConfig;
use crate::git::git_command;
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
    /// Which evaluation layer (0-3) produced this result (D-10, 17-01). Set by
    /// every constructor in this module; `None` is reserved for test-only
    /// fixture literals that don't route through the real cascade.
    #[serde(default)]
    pub decided_by_layer: Option<u8>,
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
    /// Layer 2 classified the process as killed for resource exhaustion
    /// (exit code 137, typically SIGKILL from an OOM killer) (D-07, 17b).
    #[serde(rename = "resource_killed")]
    ResourceKilled,
    /// Layer 2 classified the process as unable to start (exit code 127,
    /// typically "command not found") (D-07, 17b).
    #[serde(rename = "agent_unavailable")]
    AgentUnavailable,
}

impl AgentStatus {
    /// The wire-format name for this variant, pinned equal to
    /// `serde_json::to_string(&self)` with the surrounding quotes stripped
    /// (see the `as_wire_str_matches_serde_form` test). Exhaustive match with
    /// NO wildcard arm — adding a variant without updating this is a compile
    /// error. This is the sanctioned replacement for
    /// `format!("{:?}", status).to_ascii_lowercase()`, which collapses word
    /// boundaries on multi-word variants (review consensus #1).
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            AgentStatus::Success => "success",
            AgentStatus::Failed => "failed",
            AgentStatus::RateLimited => "ratelimited",
            AgentStatus::Unknown => "unknown",
            AgentStatus::ResourceKilled => "resource_killed",
            AgentStatus::AgentUnavailable => "agent_unavailable",
        }
    }
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

/// Read the top-level `session_id` string from a Claude JSON result envelope
/// (`--output-format json`). Returns `None` for plain-text stdout, a
/// non-JSON-object envelope, an envelope with no `session_id` key, or a
/// `session_id` of a non-string JSON type — never panics.
///
/// D-04 / T-28-04 (this plan's `<threat_model>`): deliberately reads ONLY the
/// envelope's TOP-LEVEL `session_id` key via a direct [`serde_json::Value::get`],
/// never the module's [`json_find_key`]/[`json_scan`] traversal helpers. Those
/// helpers descend into nested objects, and the agent-authored `DEVFLOW_RESULT`
/// marker payload — embedded inside this same envelope's `result` text and
/// deserialized by [`parse_marker_lines`] directly into [`AgentResult`] — is
/// reachable that way. A top-level `get` makes it true BY CONSTRUCTION that an
/// agent cannot redirect the session DevFlow later resumes into by planting a
/// different `session_id` key inside its own self-authored marker JSON.
/// Regression test: `session_id_in_devflow_result_marker_is_not_returned`.
///
/// Deliberate deviation from RESEARCH.md § "Discretion Resolutions" item 5,
/// which suggested adding a `session_id` field directly to [`AgentResult`].
/// NOT done: `parse_marker_lines` deserializes the agent's own
/// `DEVFLOW_RESULT` JSON straight into `AgentResult` via `serde_json::from_str`,
/// so a `#[serde(default)]` field there would be agent-settable — the agent
/// could name the session DevFlow resumes into (T-28-04). A standalone reader
/// over the top-level envelope key carries no such surface and is equally
/// available to every caller; D-04's persistence target (`State::session_id`)
/// is unchanged, only the carrier differs.
pub fn claude_session_id(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    value.get("session_id")?.as_str().map(str::to_string)
}

/// TDD RED stub (plan 30-03 Task 2) — always declines, so the new tests fail
/// on their assertions rather than on a compile error. Replaced in the GREEN
/// commit.
pub fn claude_stream_session_id(_stdout: &str) -> Option<String> {
    None
}

/// Thin file-reading wrapper over [`claude_session_id`]: reads the phase's
/// captured stdout file (via [`stdout_path`]) and delegates. `None` for a
/// missing capture file, never an `Err` — mirrors [`evaluate_layer1`]'s
/// lossy-read convention (CR-01: one invalid UTF-8 byte from raw `sh`
/// redirection must not silently disable this reader).
pub fn session_id_from_capture(project_root: &Path, phase: u32) -> Option<String> {
    let bytes = std::fs::read(stdout_path(project_root, phase)).ok()?;
    let stdout = String::from_utf8_lossy(&bytes);
    claude_session_id(&stdout)
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
/// drives the primary rate-limit resume cron) must win over this
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
        decided_by_layer: Some(1),
    })
}

/// The rendered VALUE of a human-blocking checkpoint's `**Gate:**` line.
///
/// **CONFIRMED against a live end-to-end run (2026-07-31).** Assumption A1 is
/// closed. A real `devflow start` run drove a synthetic phase declaring a
/// `gate="blocking-human"` task through DevFlow's own monitor process (not a
/// Claude Code agent session, which is what blocked `28-PROBE.md`'s original
/// attempt at the Bash-tool permission classifier). The checkpoint fired and
/// `.devflow/phase-NN-stdout` captured it inside the JSON envelope's `result`
/// text as:
///
/// ```text
/// **Gate:** `blocking-human`
/// ```
///
/// The VALUE is what this constant holds. The surrounding markdown — bold
/// label, and a **code span around the value** — is handled by
/// [`text_reports_human_gate`]'s trim set, not by this constant.
///
/// The code span is the part RESEARCH.md did not predict. Its § "Architecture
/// Patterns / Pattern 2" derived the literal by reading the *emitting* source
/// (`gsd-executor.md:356`, `execute-phase.md:1053`) and predicted a bare
/// `**Gate:** blocking-human`. The real relay renders the value as a code
/// span, which defeated the original matcher entirely — see
/// [`text_reports_human_gate`] for that failure and its fix. Lesson worth
/// keeping: the emitting source told us the value, not the rendering.
const HUMAN_GATE_VALUE: &str = "blocking-human";

/// Confirm whether captured stdout reports a human-blocking checkpoint, by
/// searching for a `**Gate:**`-labeled line whose VALUE is exactly
/// [`HUMAN_GATE_VALUE`] — see that constant's doc comment for the live
/// observation (2026-07-31) the matched rendering is built from.
///
/// This is the CONFIRMATION half of D-01: it is only ever consulted AFTER
/// [`crate::verify::phase_has_blocking_human_checkpoint`] has already
/// returned `true` for the stage's plan(s) (D-01's static half, plan 28-01).
/// A false negative here is the SAFE direction — it falls back to today's
/// never-silent generic gate, losing nothing. A false positive is bounded by
/// the resume ceiling (`mode::MAX_CHECKPOINT_RESUMES`, plan 28-03) and
/// unconditionally recorded by the `checkpoint_auto_decided` audit event
/// (plan 28-03) — it can never silently authorize anything.
///
/// Searches BOTH the raw stdout text and — when the stdout is a Claude JSON
/// result envelope — the unescaped inner `result` text obtained via
/// [`extract_json_result_text`], because the `Gate:` line typically crosses
/// into the capture escaped inside that envelope (RESEARCH § "Common
/// Pitfalls / Pitfall 2": two indirections, subagent emission → orchestrator
/// relay → DevFlow's captured top-level stdout). Matching is
/// case-insensitive on the `Gate` LABEL and tolerates surrounding markdown
/// emphasis (`*`) and whitespace, but the VALUE comparison is exact — this
/// deliberately does NOT widen into a general "does this look like a
/// checkpoint" heuristic (D-02 rejected that class of predicate); the scope
/// is one declared field label with one enumerated value.
pub fn blocking_human_checkpoint_reported(stdout: &str) -> bool {
    if text_reports_human_gate(stdout) {
        return true;
    }
    extract_json_result_text(stdout)
        .as_deref()
        .is_some_and(text_reports_human_gate)
}

/// Core matcher shared by both search targets (raw stdout and the unescaped
/// inner envelope text) in [`blocking_human_checkpoint_reported`]. Scans for
/// a case-insensitive `gate` label, tolerating surrounding markdown emphasis
/// (`*`), code-span backticks (`` ` ``), and whitespace up to the following
/// `:`, then compares the VALUE token immediately after the colon exactly
/// against [`HUMAN_GATE_VALUE`].
///
/// The backtick tolerance is not speculative — it is the single reason this
/// matcher failed against the first real checkpoint ever observed. The live
/// A1 run (2026-07-31) captured the value as a markdown code span,
/// ``**Gate:** `blocking-human` ``, and the original trim set (`*` and space
/// only) left the leading backtick in place, so the `take_while` below
/// terminated immediately and produced an EMPTY value token. The reader
/// returned `false` and a genuine checkpoint fell through to the generic
/// gate. Trimming the backtick is what makes the observed rendering match;
/// do not narrow this set back without re-running that live probe.
///
/// Note the closing backtick needs no handling: `take_while` already stops
/// at it, since a backtick is neither alphanumeric nor `-`.
fn text_reports_human_gate(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel_idx) = lower[search_from..].find("gate") {
        let idx = search_from + rel_idx;
        let after_label = &lower[idx + "gate".len()..];
        let after_label = after_label.trim_start_matches(['*', ' ', '`']);
        if let Some(rest) = after_label.strip_prefix(':') {
            let value_region = rest.trim_start_matches(['*', ' ', '`']);
            let value_token: String = value_region
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            if value_token == HUMAN_GATE_VALUE {
                return true;
            }
        }
        search_from = idx + "gate".len();
    }
    false
}

/// Thin file-reading wrapper over [`blocking_human_checkpoint_reported`]:
/// reads the phase's captured stdout file (via [`stdout_path`]) and
/// delegates. `false` for a missing capture file, never an error.
pub fn checkpoint_reported_in_capture(project_root: &Path, phase: u32) -> bool {
    let Ok(bytes) = std::fs::read(stdout_path(project_root, phase)) else {
        return false;
    };
    let stdout = String::from_utf8_lossy(&bytes);
    blocking_human_checkpoint_reported(&stdout)
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
        decided_by_layer: Some(1),
    })
}

/// Parse a captured stdout as JSONL: one `serde_json::Value` per non-blank,
/// parseable line. Lines that are not valid JSON are dropped, so a stream
/// interleaved with plain-text progress noise still yields its events.
///
/// Shared by [`is_claude_event_stream`] and [`last_top_level_result`], which
/// both need the same parsed vector. Deliberately NOT retrofitted into
/// [`parse_codex_event_result`], which open-codes the identical idiom: that
/// parser is correct and shipping, and rewriting it would put an unrelated
/// adapter's behavior at risk for a cosmetic dedupe.
fn claude_stream_events(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect()
}

/// Determine whether parsed JSONL lines are a Claude `--output-format
/// stream-json` event stream, as opposed to a single-document Claude envelope,
/// a Codex `--json` stream, or plain text.
///
/// **Gates on `type: "system"` + `subtype: "init"` and NOTHING ELSE.**
/// 30-RESEARCH.md offered an alternative — also gate on `type: "result"`
/// carrying a `session_id` — and that alternative is WRONG; do not "restore"
/// it. The single-document envelope that ships today is literally
/// `{"type":"result",...,"session_id":"abc"}`, so a `result`-keyed gate would
/// swallow every production capture in use and silently displace
/// [`parse_devflow_result`] in the [`evaluate_layer1`] cascade — a change to
/// the shipped Layer-1 verdict path, disguised as adding stream support
/// (T-30-02). The `init` event is both stronger and earlier: it opens the
/// stream and is present in all three archived captures
/// (`30a-evidence/raw_output_v3.jsonl` lines 5, 32 and 47).
///
/// `single_doc_envelope_not_consumed_by_claude_stream_parser` is the test that
/// fails if this gate is widened.
fn is_claude_event_stream(events: &[serde_json::Value]) -> bool {
    events.iter().any(|v| {
        v.get("type").and_then(serde_json::Value::as_str) == Some("system")
            && v.get("subtype").and_then(serde_json::Value::as_str) == Some("init")
    })
}

/// The LAST top-level `type: "result"` event in a Claude stream capture.
///
/// One capture can hold several: a session kept alive across turns emits one
/// terminal `result` per turn (the archived v3 stream carries three, at lines
/// 19, 37 and 54, produced across task-notification wake-ups). The last is the
/// session's final verdict, so an earlier turn must never decide the stage.
///
/// T-30-01: selection runs over TOP-LEVEL objects only — each value here is one
/// whole JSONL line. A `result`-shaped structure the agent writes inside its own
/// message text is inert string content and structurally unreachable from this
/// scan. Never route this through [`json_scan`]/[`json_find_key`], which descend
/// into nested objects; that is the same protection class as D-04/T-28-04's
/// top-level-only `session_id` read.
///
/// A missing `parent_tool_use_id` is treated as top-level rather than required.
/// Verified against the archived captures: no `result` event carries the key at
/// all — it appears only on subagent `assistant`/`user` events.
fn last_top_level_result(events: &[serde_json::Value]) -> Option<&serde_json::Value> {
    events
        .iter()
        .rev()
        .find(|v| v.get("type").and_then(serde_json::Value::as_str) == Some("result"))
}

/// The `rate_limit_info.status` values that mean the CLI DENIED the request.
///
/// Provenance, per entry — required reading before adding one:
///
/// - `rejected` — drawn from the observed vocabulary of this schema: it is the
///   value the CLI writes for `overageStatus` in the only archived
///   `rate_limit_event`
///   (`.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl`
///   line 15), so it is the denial token this schema actually speaks. It has
///   NOT been observed as a `status` value — no archived capture is of a
///   blocked stream, and every capture DevFlow has taken carries
///   `status: "allowed"`.
///
/// Nothing else is listed, deliberately. Speculatively adding tokens is how the
/// false positive this list exists to prevent comes back: an unrecognised
/// status must DEFER (see [`detect_claude_stream_rate_limit`]), never classify.
/// Correct this list the first time a real blocked capture is archived — that
/// is the only evidence that settles the vocabulary.
const CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES: &[&str] = &["rejected"];

/// Detect an explicit quota DENIAL in a Claude `stream-json` capture and return
/// the retry description, mirroring what [`detect_claude_rate_limit`] returns
/// for the single-document envelope.
///
/// **A `rate_limit_event` is not a rate limit.** The CLI emits these routinely
/// as quota telemetry on healthy streams: the only archived one
/// (`raw_output_v3.jsonl` line 15) says `rate_limit_info.status: "allowed"` and
/// sits in a stream that then completed three turns successfully. Classifying
/// on the event's PRESENCE would mark every healthy Claude stream stage
/// `RateLimited`, and `outcome_policy.rs` maps that to `Action::AutoResume` —
/// so every stage would be auto-resumed against a fabricated retry time
/// instead of advancing (T-30-26). Note the second trap in the same object:
/// `overageStatus` is `rejected` one level below `status: "allowed"`, so any
/// nested search for the token also false-positives. Hence every field here is
/// read with a direct [`serde_json::Value::get`] on the top-level event and its
/// `rate_limit_info` child — never [`json_find_key`]/[`json_scan`], which
/// descend into nested (and, elsewhere in the stream, agent-authored) content
/// and would let the agent supply the retry hint that drives the resume cron's
/// scheduling (T-30-12).
///
/// Two independent guards, both required, neither a substitute for the other:
///
/// 1. **Positional** — only events after the SECOND-TO-LAST `result` event are
///    eligible, i.e. the final turn. A session kept alive across turns emits one
///    `result` per turn, and rate-limit chatter from an earlier turn must never
///    outrank the outcome of a turn that finished later. (In the archived
///    capture the rate event is at line 15 and the results at 19/37/54, so it is
///    excluded on position alone.) With fewer than two `result` events the whole
///    stream IS the final turn.
/// 2. **Semantic** — only a `status` in
///    [`CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES`] classifies. A missing
///    `rate_limit_info`, a missing or non-string `status`, or any unrecognised
///    value returns `None`.
///
/// **Deferring is the deliberately safe direction, not an oversight.**
/// Under-classifying means an unknown denial status falls through to the
/// envelope-failure path and is reported `Failed` — a real degradation (the
/// operator loses automatic resume) but a never-silent one that still gates.
/// Over-classifying means a healthy stream is auto-resumed against a retry time
/// the parser invented. The asymmetry is the whole reason this function reads
/// one field instead of matching a shape.
fn detect_claude_stream_rate_limit(events: &[serde_json::Value]) -> Option<String> {
    // Index of the second-to-last `result` event: everything at or before it is
    // previous-turn history. `None` (fewer than two results) means the whole
    // stream is the final turn.
    let boundary = events
        .iter()
        .enumerate()
        .filter(|(_, v)| v.get("type").and_then(serde_json::Value::as_str) == Some("result"))
        .map(|(idx, _)| idx)
        .rev()
        .nth(1);
    let eligible = match boundary {
        Some(idx) => &events[idx + 1..],
        None => events,
    };

    // Last eligible event wins, matching the last-`result`-wins convention.
    let event = eligible
        .iter()
        .rev()
        .find(|v| v.get("type").and_then(serde_json::Value::as_str) == Some("rate_limit_event"))?;

    let info = event.get("rate_limit_info")?;
    let status = info.get("status")?.as_str()?;
    if !CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES.contains(&status) {
        return None;
    }

    // `resetsAt` is epoch seconds, rendered from the JSON number as-is: nothing
    // parses this string. `outcome_policy.rs` routes on the
    // `AgentStatus::RateLimited` variant alone and the `reason` text is
    // operator-facing. Mirrors `detect_claude_rate_limit`'s `retry_after` →
    // `message` → `error` chain; its final `"usage limit"` default has no
    // counterpart here because a matched `status` is by construction one of the
    // non-empty enumerated strings above, so a third rung would be unreachable.
    Some(
        info.get("resetsAt")
            .and_then(json_scalar_to_string)
            .unwrap_or_else(|| status.to_string()),
    )
}

/// The stream-path counterpart of [`detect_claude_envelope_failure`]: treat
/// `is_error: true` on a stream's last `result` event as an authoritative
/// Layer-1 failure.
///
/// The `reason` shape is reproduced deliberately rather than shared — `result`
/// text, else `subtype`, else `agent reported is_error`, with a
/// ` (num_turns: {n})` suffix when present. This phase's scope fence keeps the
/// four shipped single-document parsers unmodified, so factoring the common
/// body out of `detect_claude_envelope_failure` is out of bounds here; the two
/// must be kept in step by hand. `is_error` absent, non-bool, or `false`
/// returns `None`, deferring exactly as the single-document path does.
fn claude_stream_envelope_failure(result_event: &serde_json::Value) -> Option<AgentResult> {
    if !result_event.get("is_error")?.as_bool()? {
        return None;
    }

    let num_turns = result_event
        .get("num_turns")
        .and_then(serde_json::Value::as_u64);
    let base_reason = result_event
        .get("result")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            result_event
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
        decided_by_layer: Some(1),
    })
}

/// Parse a Claude `--output-format stream-json` JSONL capture and read the
/// `DEVFLOW_RESULT` marker out of its LAST `result` event.
///
/// The new sibling of [`parse_codex_event_result`], mirroring its shape. Only
/// decisive when the capture is actually a Claude event stream (per
/// [`is_claude_event_stream`]); every other shape returns `None` and falls
/// through to the parser that owns it. Before this existed, a JSONL capture
/// returned `None` from all four single-document parsers —
/// `serde_json::from_str` on the whole multi-line document is a hard "trailing
/// characters" error — so every Claude-driven stage fell through to Layer 2's
/// coarse exit-code+commit heuristic.
///
/// **Precedence, mirroring [`evaluate_layer1`]'s single-document ordering
/// rather than inventing a new one** — do not reshuffle without reading the
/// reasons:
///
/// 1. Format gate ([`is_claude_event_stream`]); every other shape declines here.
/// 2. [`detect_claude_stream_rate_limit`] — a final-turn explicit quota denial
///    wins over EVERYTHING below it, for the same reason `evaluate_layer1`
///    already puts `detect_claude_rate_limit` ahead of the generic failure
///    check: a rate-limited run classified as plain `Failed` kills the primary
///    rate-limit resume cron, the one automated path that exists to recover
///    from it (T-30-13). The precedence is narrow, not broad — the detector
///    only fires on an explicit denial inside the final turn, so it cannot
///    shadow the outcome of a stream that completed.
/// 3. The `DEVFLOW_RESULT` marker in the last `result` event. A non-success
///    marker is decisive and returns immediately; a success marker is HELD, not
///    returned, because step 4 may override it.
/// 4. [`claude_stream_envelope_failure`] — `is_error: true` on that same event
///    overrides a held success marker, matching the single-document rule that
///    the envelope is authoritative for errors and a stale or echoed success
///    marker must not win (T-30-15).
/// 5. The held success marker, else `None`.
///
/// A last `result` event with no marker and no `is_error` returns `None`
/// (defer to Layer 2) rather than an unconditional Success, matching the
/// `turn.completed` convention: a marker-less turn must never silently advance
/// a stage.
///
/// Passing the isolated `result` text to [`parse_marker_lines`] is the correct
/// scoping, not a workaround. The marker is JSON-escaped inside a
/// `"result":"..."` string value, so it can never appear as a line starting
/// with `DEVFLOW_RESULT:` in the raw capture, and that parser's 4000-character
/// tail window is smaller than a single stream `result` line. Once serde
/// decodes the field the escaped newlines become real newlines and the existing
/// tail scan works on it as designed.
fn parse_claude_event_result(stdout: &str) -> Option<AgentResult> {
    let events = claude_stream_events(stdout);
    if !is_claude_event_stream(&events) {
        return None;
    }

    if let Some(retry) = detect_claude_stream_rate_limit(&events) {
        return Some(rate_limited_result(retry));
    }

    let last_result = last_top_level_result(&events)?;

    let marker = last_result
        .get("result")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_marker_lines)
        .map(normalise_stream_marker_provenance);

    let held_success = match marker {
        // A non-success marker is the agent's own final word and nothing below
        // can improve on it.
        Some(result) if result.status != AgentStatus::Success => return Some(result),
        other => other,
    };

    if let Some(failure) = claude_stream_envelope_failure(last_result) {
        return Some(failure);
    }

    held_success
}

/// T-30-26: overwrite the agent-supplied `decided_by_layer` unconditionally.
///
/// [`parse_marker_lines`] deserializes the agent's own marker JSON straight
/// into [`AgentResult`], and the field is `#[serde(default)]`, so an ordinary
/// `{"status":"success"}` marker leaves it `None` while a hostile
/// `{"status":"success","decided_by_layer":0}` leaves it `Some(0)`. Neither is
/// acceptable: every other Layer-1 constructor in this module sets `Some(1)`
/// explicitly, and `Some(0)` is a Layer-0 external-probe provenance that
/// `classify_validate_outcome` (devflow-cli's `pipeline_outcomes.rs`) reads as
/// `external` when classifying a Validate stage. An agent must not be able to
/// claim a probe verdict it did not earn, so the value is derived here rather
/// than trusted.
fn normalise_stream_marker_provenance(mut result: AgentResult) -> AgentResult {
    result.decided_by_layer = Some(1);
    result
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
/// `is_error: true`, and classifying them `Failed` would kill the primary
/// rate-limit resume cron path) → Claude envelope `is_error: true` (authoritative,
/// overrides a success marker) → Claude `stream-json` JSONL event stream (the
/// last `result` event's marker decides; a marker-less last turn defers) →
/// DEVFLOW_RESULT marker (portable; works for plain text and a Claude
/// envelope's unwrapped `result` text) → Codex JSONL event stream
/// (`turn.failed` decisive; `turn.completed` defers) → Codex plain-text
/// rate-limit heuristic (least authoritative, stays last).
///
/// The Claude stream parser's position is load-bearing in BOTH directions
/// (T-30-03). The two single-document detectors stay ahead of it because they
/// remain authoritative for the `--output-format json` envelope that ships
/// today. It goes ahead of `parse_devflow_result` so that an adapter-specific
/// stream capture is owned whole by the parser that understands its framing,
/// rather than letting the generic 4000-character tail scan take a bite of a
/// mid-line window of JSONL first.
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
        .or_else(|| parse_claude_event_result(&stdout))
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
        decided_by_layer: Some(1),
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
///   exit=137                                             → ResourceKilled (ALL stages, D-07)
///   exit=127                                             → AgentUnavailable (ALL stages, D-07)
///   exit≠0 (excluding 137/127)                           → Failed (ALL stages)
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
    let branch_exists = git_command(project_root)
        .args(["rev-parse", "--verify", &branch])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let commits: u32 = if branch_exists {
        let range = format!("{}..{branch}", git_flow.develop);
        git_command(project_root)
            .args(["rev-list", "--count", &range])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(0)
    } else {
        0
    };

    let commit_gated = matches!(stage, Stage::Plan | Stage::Code);
    let no_work_done = commit_gated && commits == 0;

    // 137 (SIGKILL, typically OOM) and 127 (command not found) are classified
    // BEFORE the generic `exit_code != 0 -> Failed` catch-all, using the same
    // trusted plain-i32 already parsed above from the monitor-written exit
    // file (D-07, 17b — no ExitStatusExt/signal API per Pitfall 1a).
    let status = if exit_code == 137 {
        AgentStatus::ResourceKilled
    } else if exit_code == 127 {
        AgentStatus::AgentUnavailable
    } else if exit_code != 0 || no_work_done {
        AgentStatus::Failed
    } else {
        AgentStatus::Success
    };

    Ok(Some(AgentResult {
        status,
        exit_code: Some(exit_code),
        reason: if exit_code == 137 {
            Some(format!(
                "agent process was killed (exit code 137, likely OOM) ({} commits on {})",
                commits, branch
            ))
        } else if exit_code == 127 {
            Some(format!(
                "agent command was unavailable (exit code 127, command not found) ({} commits on {})",
                commits, branch
            ))
        } else if exit_code != 0 {
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
        decided_by_layer: Some(2),
    }))
}

/// Layer 3: Last resort — agent process is gone.
///
/// Split per D-02/D-03 case 3 (17-03): "process gone, commits exist" stays
/// `Unknown` — unverified but there is SOMETHING to account for, and Plan
/// 04's never-advance dispatch gates it downstream (D-04) rather than
/// reclassifying it here. "Process gone, zero commits, nothing declared" is
/// no longer a blanket advanceable `Unknown` — it is reclassified to
/// `Failed` so a vanished agent that produced and declared nothing cannot
/// masquerade as ambiguous-but-fine; the reason flags that human review is
/// needed. This only fires when neither Layer 1 nor Layer 2 produced a
/// definitive result.
pub fn evaluate_layer3(
    project_root: &Path,
    phase: u32,
    git_flow: &GitFlowConfig,
) -> Result<AgentResult, ResultError> {
    let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase);
    let commits = git_command(project_root)
        .args([
            "rev-list",
            "--count",
            &format!("{}..{branch}", git_flow.develop),
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0);

    let (status, reason) = if commits > 0 {
        (
            AgentStatus::Unknown,
            format!(
                "unverified — agent process is gone but {} commits exist on {}",
                commits, branch
            ),
        )
    } else {
        (
            AgentStatus::Failed,
            "no work accounted for — agent process is gone with no commits and no declared \
             external post-condition; human review needed"
                .to_string(),
        )
    };

    Ok(AgentResult {
        status,
        exit_code: None,
        reason: Some(reason),
        commits: Some(commits),
        summary: None,
        verdict: None,
        decided_by_layer: Some(3),
    })
}

/// Layer 0: run explicitly operator-approved external post-condition probes.
///
/// A failed probe outranks every agent-controlled signal. An approved,
/// all-passing set of declared probes is itself affirmative completion
/// evidence — `Success` — so a legitimately external-only stage with zero
/// commits can still complete cleanly (D-05 gap 2). Evaluated for EVERY
/// stage, not only Code (D-05 gap 1 / D-06). With no declarations (or when
/// disabled), behavior is byte-for-byte the pre-Phase-16 cascade.
///
/// Two roots are intentionally kept distinct (review Plan 03 MEDIUM,
/// OpenCode): `project_root` is used to DISCOVER the PLAN's declared
/// commands (`.planning/phases/` lives there, not in a worktree checkout),
/// while `execution_root` — the worktree, when one is set — is where probes
/// actually RUN. Conflating the two previously meant a worktree-based phase
/// could not find its own declaration and silently mis-hit the
/// "PLAN removed" veto below.
fn evaluate_layer0(
    project_root: &Path,
    state: &State,
    approved_commands: Option<&[String]>,
) -> Option<AgentResult> {
    if !crate::config::external_verify_enabled(project_root) {
        return None;
    }

    let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
    let commands = crate::verify::external_verify_commands(project_root, state.phase);
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
            decided_by_layer: Some(0),
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
            decided_by_layer: Some(0),
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
            decided_by_layer: Some(0),
        });
    }
    match commands
        .into_iter()
        .find(|command| !crate::verify::run_external_verification(command, execution_root))
    {
        Some(command) => Some(AgentResult {
            status: AgentStatus::Failed,
            exit_code: None,
            reason: Some(format!("external verification failed: {command}")),
            commits: None,
            summary: None,
            verdict: None,
            decided_by_layer: Some(0),
        }),
        // Every declared, approved probe passed — affirmative completion
        // evidence on its own (D-05 gap 2), even with zero commits.
        None => Some(AgentResult {
            status: AgentStatus::Success,
            exit_code: None,
            reason: Some(
                "external verification passed — all declared, approved probes succeeded".into(),
            ),
            commits: None,
            summary: None,
            verdict: None,
            decided_by_layer: Some(0),
        }),
    }
}

/// Reconciles Layer 0's affirmative-success result with Layer 1's
/// self-reported verdict at `Stage::Validate` (18e).
///
/// Layer 0's affirmative-success arm above short-circuits the cascade before
/// Layer 1 ever runs (`evaluate_agent_result_inner` returns immediately on
/// any `Some(..)` from Layer 0), but Layer 1 is the ONLY carrier of a
/// `verdict` — `status` reports whether the stage's task ran; `verdict`
/// reports whether validation itself passed (see `AgentResult::verdict`'s
/// doc comment). At `Stage::Validate` that meant an agent's explicit
/// `verdict: pass` was silently discarded and `advance()` computed a failure
/// from it — a regression introduced by this project's own 17-03, fixed
/// here.
///
/// `decided_by_layer` deliberately stays `Some(0)` — Layer 0 still DECIDED
/// the `status`; Layer 1 only supplies the `verdict`. The CLI relies on that
/// value to tell an `external_verify` Validate apart from an ordinary one
/// (`classify_validate_outcome`, 18e).
///
/// Scoped to `Stage::Validate` only (flagged assumption in 18-05-PLAN.md): at
/// every other stage an affirmative Layer 0 success keeps `verdict: None`,
/// unchanged from current behavior. A Layer 0 FAILURE is never passed here —
/// only its affirmative-success arm is, so a failed probe still outranks
/// every agent-controlled signal.
fn reconcile_layer0_verdict(
    project_root: &Path,
    state: &State,
    result: AgentResult,
) -> AgentResult {
    if state.stage != Stage::Validate
        || result.status != AgentStatus::Success
        || result.decided_by_layer != Some(0)
    {
        return result;
    }
    let verdict = evaluate_layer1(project_root, state.phase).and_then(|layer1| layer1.verdict);
    AgentResult { verdict, ..result }
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
        return Ok(reconcile_layer0_verdict(project_root, state, result));
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
    crate::workflow::ensure_devflow_dir(&history_dir)?;

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

    fn state_in(root: &Path, phase: u32) -> State {
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        state
    }

    fn git(root: &Path, args: &[&str]) {
        let output = crate::test_support::git_command(root)
            .args(args)
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
        git(root, &["config", "core.hooksPath", "/dev/null"]);
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
        git(root, &["config", "core.hooksPath", "/dev/null"]);
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
    fn session_id_reads_top_level_string() {
        let stdout = r#"{"type":"result","subtype":"success","result":"All done.","session_id":"cf29bfec-69e8-45df-a4f3-3da08ab6f66e"}"#;
        assert_eq!(
            claude_session_id(stdout).as_deref(),
            Some("cf29bfec-69e8-45df-a4f3-3da08ab6f66e")
        );
    }

    /// T-28-04 forgery guard: the embedded `DEVFLOW_RESULT` marker carries a
    /// DIFFERENT session id than the envelope's own top-level key. The
    /// top-level id must win — an agent must not be able to redirect which
    /// session DevFlow resumes into by planting its own `session_id` inside
    /// its self-authored marker JSON.
    #[test]
    fn session_id_in_devflow_result_marker_is_not_returned() {
        let stdout = r#"{"type":"result","subtype":"success","result":"All done.\nDEVFLOW_RESULT: {\"status\": \"success\", \"session_id\": \"forged-by-agent\"}","session_id":"real-top-level-id"}"#;
        assert_eq!(
            claude_session_id(stdout).as_deref(),
            Some("real-top-level-id")
        );
    }

    #[test]
    fn session_id_plain_text_stdout_returns_none() {
        let stdout = "just some plain text output, not JSON\n";
        assert!(claude_session_id(stdout).is_none());
    }

    #[test]
    fn session_id_missing_key_returns_none() {
        let stdout = r#"{"type":"result","result":"done, no session key"}"#;
        assert!(claude_session_id(stdout).is_none());
    }

    #[test]
    fn session_id_non_string_type_returns_none_not_panic() {
        let stdout = r#"{"type":"result","result":"done","session_id":12345}"#;
        assert!(claude_session_id(stdout).is_none());
    }

    #[test]
    fn session_id_from_capture_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(session_id_from_capture(dir.path(), 42).is_none());
    }

    #[test]
    fn session_id_from_capture_lossy_reads_invalid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        let mut bytes = br#"{"type":"result","result":"done "#.to_vec();
        bytes.push(0xFF); // invalid UTF-8 byte
        bytes.extend_from_slice(br#"","session_id":"lossy-ok"}"#);
        std::fs::write(stdout_path(dir.path(), 5), bytes).unwrap();

        assert_eq!(
            session_id_from_capture(dir.path(), 5).as_deref(),
            Some("lossy-ok")
        );
    }

    /// Positive fixture built from RESEARCH's *predicted* `**Gate:**`
    /// rendering (a bare, un-spanned value). Kept as a tolerated shape, but
    /// note this is NOT what a real run emits — see
    /// `blocking_human_checkpoint_reported_matches_live_observed_rendering`
    /// for the rendering actually captured on 2026-07-31, which this
    /// prediction missed.
    #[test]
    fn blocking_human_checkpoint_reported_detects_human_gate_line() {
        let stdout = format!(
            "## CHECKPOINT REACHED\n\n**Type:** human-verify\n**Gate:** {HUMAN_GATE_VALUE} — copy the task's `gate` attribute verbatim so the orchestrator's carve-out sees it\n"
        );
        assert!(blocking_human_checkpoint_reported(&stdout));
    }

    /// The Phase 26 near-miss distinction: a plain `blocking` gate must NOT
    /// be classified as a human-blocking checkpoint. `PLAIN_GATE_VALUE` is
    /// local to this test (not a module-level const) — it has no production
    /// use, only this negative fixture's.
    #[test]
    fn blocking_human_checkpoint_reported_false_for_plain_blocking() {
        const PLAIN_GATE_VALUE: &str = "blocking";
        let stdout = format!(
            "## CHECKPOINT REACHED\n\n**Type:** human-verify\n**Gate:** {PLAIN_GATE_VALUE} — copy the task's `gate` attribute verbatim so the orchestrator's carve-out sees it\n"
        );
        assert!(!blocking_human_checkpoint_reported(&stdout));
    }

    #[test]
    fn blocking_human_checkpoint_reported_false_when_no_gate_field() {
        let stdout = "some ordinary agent failure output, no checkpoint at all\n";
        assert!(!blocking_human_checkpoint_reported(stdout));
    }

    /// The `Gate:` line arrives inside an escaped Claude JSON result
    /// envelope's `result` field — must be found via the unescaped inner
    /// text, not the raw (escaped) JSON string.
    #[test]
    fn blocking_human_checkpoint_reported_true_inside_escaped_envelope() {
        let inner = format!(
            "## CHECKPOINT REACHED\\n\\n**Gate:** {HUMAN_GATE_VALUE} — copy the task's `gate` attribute verbatim so the orchestrator's carve-out sees it\\n"
        );
        let stdout = format!(
            r#"{{"type":"result","subtype":"success","result":"{inner}","session_id":"abc"}}"#
        );
        assert!(blocking_human_checkpoint_reported(&stdout));
    }

    #[test]
    fn blocking_human_checkpoint_reported_tolerates_whitespace_and_emphasis() {
        let stdout = format!("  **Gate:**   {HUMAN_GATE_VALUE}   \n");
        assert!(blocking_human_checkpoint_reported(&stdout));
    }

    /// REGRESSION — the rendering a real headless run actually produces.
    ///
    /// Transcribed verbatim from `.devflow/phase-91-stdout` of the live A1
    /// run on 2026-07-31 (a genuine `gate="blocking-human"` task driven
    /// through DevFlow's own monitor). The value arrives as a markdown CODE
    /// SPAN, not the bare token RESEARCH.md predicted.
    ///
    /// Before the backtick was added to `text_reports_human_gate`'s trim set
    /// this returned `false`: the leading backtick survived the trim, so the
    /// value `take_while` terminated at once and yielded an empty token. A
    /// real checkpoint was therefore never recognized, and the run fell
    /// through to the generic gate. If this test ever goes red, DevFlow has
    /// stopped recognizing real checkpoints — do not "fix" it by relaxing
    /// the assertion.
    #[test]
    fn blocking_human_checkpoint_reported_matches_live_observed_rendering() {
        let stdout = format!(
            "---\n\n## Checkpoint: Decision\n\n**Plan:** 91-01 Emit the checkpoint\n**Gate:** `{HUMAN_GATE_VALUE}`\n**Progress:** 0/1 tasks complete\n**Task:** Task 1 — Ask the operator to authorize writing the marker file\n"
        );
        assert!(
            blocking_human_checkpoint_reported(&stdout),
            "the live-observed code-span rendering must be recognized; \
             a false negative here means real checkpoints fall through to \
             the generic gate (the 2026-07-31 A1 defect)"
        );
    }

    /// The same live rendering as it actually crosses into DevFlow's capture:
    /// escaped inside the Claude JSON result envelope. This is the exact
    /// path `checkpoint_reported_in_capture` reads in production.
    #[test]
    fn blocking_human_checkpoint_reported_matches_live_rendering_in_envelope() {
        let inner = format!(
            "## Checkpoint: Decision\\n\\n**Gate:** `{HUMAN_GATE_VALUE}`\\n**Progress:** 0/1 tasks complete\\n"
        );
        let stdout = format!(
            r#"{{"type":"result","subtype":"success","result":"{inner}","session_id":"live-a1"}}"#
        );
        assert!(
            blocking_human_checkpoint_reported(&stdout),
            "the code-span rendering must also be found inside the escaped envelope"
        );
    }

    /// The backtick tolerance must not erode the Phase 26 near-miss
    /// distinction: a code-spanned PLAIN `blocking` gate is still not a
    /// human-blocking checkpoint.
    #[test]
    fn blocking_human_checkpoint_reported_false_for_code_spanned_plain_blocking() {
        let stdout = "## Checkpoint: Decision\n\n**Gate:** `blocking`\n";
        assert!(!blocking_human_checkpoint_reported(stdout));
    }

    #[test]
    fn checkpoint_reported_in_capture_missing_file_returns_false() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!checkpoint_reported_in_capture(dir.path(), 42));
    }

    #[test]
    fn checkpoint_reported_in_capture_reads_true_from_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 11),
            format!("**Gate:** {HUMAN_GATE_VALUE}\n"),
        )
        .unwrap();
        assert!(checkpoint_reported_in_capture(dir.path(), 11));
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

    /// The highest-value isolation test in plan 30-01 (T-30-02).
    ///
    /// The single-document `--output-format json` envelope that ships TODAY
    /// carries `type: "result"` AND a `session_id` — precisely the gate shape
    /// 30-RESEARCH.md offered as an alternative to `system`/`init`. If anyone
    /// widens [`is_claude_event_stream`] to accept it, the stream parser starts
    /// consuming every production capture in use and silently displaces
    /// `parse_devflow_result` in the Layer-1 cascade. This test fails first.
    ///
    /// The first literal is reused verbatim from
    /// `claude_envelope_not_consumed_by_codex_parser` above so the two read as
    /// a matched pair.
    #[test]
    fn single_doc_envelope_not_consumed_by_claude_stream_parser() {
        let stdout = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":4,"result":"All done.","session_id":"abc"}"#;
        assert!(parse_claude_event_result(stdout).is_none());

        // Non-vacuity: the literal above carries no marker, so it would return
        // None even from a WRONGLY-widened gate — on its own it proves little.
        // This envelope does carry one, so it can only return None because the
        // gate declined the document, not because the marker scan came up dry.
        let with_marker = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":4,"result":"Done.\nDEVFLOW_RESULT: {\"status\":\"success\"}","session_id":"abc"}"#;
        assert!(parse_claude_event_result(with_marker).is_none());

        // ...and the shipped path still owns it, so declining costs no verdict.
        assert_eq!(
            parse_devflow_result(with_marker).unwrap().status,
            AgentStatus::Success
        );
    }

    /// Cross-adapter isolation: a Codex `--json` event stream is not consumed
    /// by the Claude stream parser. The two gates are mutually exclusive by
    /// construction — Codex keys on `thread.started`/`turn.*`, Claude on
    /// `system`/`init` — and this pins that.
    #[test]
    fn codex_stream_not_consumed_by_claude_stream_parser() {
        let stdout = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_2\",\"type\":\"agent_message\",\"text\":\"DEVFLOW_RESULT: {\\\"status\\\": \\\"success\\\"}\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}\n",
        );
        assert!(parse_claude_event_result(stdout).is_none());

        // The Codex parser still decides it — isolation costs no verdict.
        assert_eq!(
            parse_codex_event_result(stdout).unwrap().status,
            AgentStatus::Success
        );
    }

    /// The same isolation claim in the other direction: a Claude stream capture
    /// is not consumed by the Codex parser, so the two never collide.
    #[test]
    fn claude_stream_not_consumed_by_codex_parser() {
        let capture = v3_stream_capture(NO_MARKER, NO_MARKER, MARKER_SUCCESS);
        assert!(parse_codex_event_result(&capture).is_none());
    }

    /// Plain-text stdout is not consumed by the Claude stream parser.
    ///
    /// Non-vacuous by construction: the text carries a real marker, so a gate
    /// that wrongly fired on non-JSON input would change the verdict rather
    /// than merely returning None. The second assertion pins that the marker
    /// path still decides it — the cascade must lose nothing.
    #[test]
    fn plain_text_not_consumed_by_claude_stream_parser() {
        let stdout = "Running the plan...\nDEVFLOW_RESULT: {\"status\":\"success\"}\n";
        assert!(parse_claude_event_result(stdout).is_none());
        assert_eq!(
            parse_devflow_result(stdout).unwrap().status,
            AgentStatus::Success
        );
    }

    // ---- Claude `--output-format stream-json` fixtures (plan 30-01) --------
    //
    // Sourced from the archived capture
    // `.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/raw_output_v3.jsonl`,
    // a real 54-line stream from a session that survived three orchestrator
    // turns via task-notification wake-ups. The `init` event is line 5; the
    // three `result` events are lines 19, 37 and 54.
    //
    // TWO documented modifications, both labelled where they occur:
    //   1. Each envelope's `result` string value is replaced with the sentinel
    //      `__MARKER__`, which each test fills in. NO archived capture contains
    //      a real `DEVFLOW_RESULT` marker — the v3 harness produced
    //      acknowledgment prose, not GSD stage output — so every marker payload
    //      below is SYNTHETIC. Envelope shape is real; marker text is not.
    //   2. The `init` event's three inert array payloads are truncated and its
    //      `cwd` is redacted (see `V3_INIT_EVENT`).
    // Everything else is byte-for-byte as captured, including field ORDER —
    // note that `"type":"result"` appears near the END of each result line,
    // long after `result` itself, which is exactly why the parser must key on
    // the parsed object rather than on textual position.

    /// v3 line 5 — the `system`/`init` event that opens the stream and is the
    /// ONLY thing `is_claude_event_stream` gates on.
    ///
    /// Modification 2: verbatim except that `tools`, `mcp_servers` and
    /// `slash_commands` are truncated to a real prefix (verbatim they run to
    /// 5,523 characters of tool and slash-command names that no code path here
    /// reads) and `cwd` is redacted to a neutral path — the captured value
    /// embeds a developer's home directory, and `devflow-core` is published to
    /// crates.io. Both fields are inert for every function under test.
    const V3_INIT_EVENT: &str = r#"{"type":"system","subtype":"init","cwd":"/tmp/scratchpad/999.64-experiment","session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790","tools":["Task","Bash","Read","Write"],"mcp_servers":[{"name":"github","status":"pending"}],"model":"claude-opus-5[1m]","permissionMode":"bypassPermissions","slash_commands":["gsd-execute-phase"],"capabilities":["interrupt_receipt_v1","interrupt_cancel_queued_v1","msg_lifecycle_v1"],"uuid":"597e1613-77cb-4cdd-a716-2aa75dc58c0b"}"#;

    /// v3 line 19 — the FIRST turn's terminal `result` event.
    const V3_RESULT_TURN1: &str = r#"{"is_error":false,"duration_api_ms":8087,"num_turns":3,"stop_reason":"end_turn","session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790","total_cost_usd":0.2401795,"usage":{"input_tokens":4,"cache_creation_input_tokens":20120,"cache_read_input_tokens":49219,"output_tokens":574,"service_tier":"standard","inference_geo":"not_available","speed":"standard"},"permission_denials":[],"terminal_reason":"completed","fast_mode_state":"off","subtype":"success","api_error_status":null,"result":"__MARKER__","ttft_ms":1381,"time_to_request_ms":91,"type":"result","duration_ms":8315,"uuid":"3dce3044-2d33-4c4d-bfcb-80e1756a5522"}"#;

    /// v3 line 37 — the SECOND turn's terminal `result` event, produced after a
    /// task-notification wake-up. Carries the `origin` key the later turns have
    /// and the first does not.
    const V3_RESULT_TURN2: &str = r#"{"is_error":false,"duration_api_ms":27809,"num_turns":1,"stop_reason":"end_turn","session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790","total_cost_usd":0.53654625,"usage":{"input_tokens":2,"cache_creation_input_tokens":3147,"cache_read_input_tokens":35393,"output_tokens":124,"service_tier":"standard","inference_geo":"not_available","speed":"standard"},"permission_denials":[],"terminal_reason":"completed","fast_mode_state":"off","origin":{"kind":"task-notification"},"subtype":"success","api_error_status":null,"result":"__MARKER__","ttft_ms":5476,"time_to_request_ms":18,"type":"result","duration_ms":6195,"uuid":"ca58693c-2599-4eb6-955b-e9d1e7444255"}"#;

    /// v3 line 54 — the THIRD and LAST turn's terminal `result` event. This is
    /// the one whose marker must decide the stage.
    const V3_RESULT_TURN3: &str = r#"{"is_error":false,"duration_api_ms":39273,"num_turns":2,"stop_reason":"end_turn","session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790","total_cost_usd":0.6599295,"usage":{"input_tokens":4,"cache_creation_input_tokens":999,"cache_read_input_tokens":77871,"output_tokens":302,"service_tier":"standard","inference_geo":"not_available","speed":"standard"},"permission_denials":[],"terminal_reason":"completed","fast_mode_state":"off","origin":{"kind":"task-notification"},"subtype":"success","api_error_status":null,"result":"__MARKER__","ttft_ms":2099,"time_to_request_ms":14,"type":"result","duration_ms":5276,"uuid":"dc76186e-3e9a-4d52-9152-27aa5012bc41"}"#;

    // Synthetic `result`-text payloads (modification 1). Written exactly as
    // they appear INSIDE the envelope's `result` JSON string — escaped quotes
    // and an escaped newline — because that is how `claude` emits an agent's
    // final message. Once serde decodes the field the `\n` becomes a real
    // newline and `parse_marker_lines`' line scan works on it unmodified.
    const MARKER_SUCCESS: &str = r#"Plan complete.\nDEVFLOW_RESULT: {\"status\":\"success\"}"#;
    const MARKER_FAILED: &str =
        r#"Blocked.\nDEVFLOW_RESULT: {\"status\":\"failed\",\"reason\":\"earlier turn aborted\"}"#;
    const MARKER_PLANTED_LAYER: &str =
        r#"Done.\nDEVFLOW_RESULT: {\"status\":\"success\",\"decided_by_layer\":0}"#;
    const NO_MARKER: &str = r#"Acknowledged; nothing to report."#;

    /// Fill one real envelope's `result` field with a synthetic payload.
    fn v3_result_event(envelope: &str, escaped_result_text: &str) -> String {
        assert!(
            envelope.contains("__MARKER__"),
            "fixture envelope lost its result-text sentinel"
        );
        envelope.replace("__MARKER__", escaped_result_text)
    }

    /// Assemble a three-turn Claude stream capture: the real `init` event
    /// followed by all three real `result` envelopes, each carrying the given
    /// payload. Three result events (not two) is load-bearing — a two-event
    /// fixture cannot tell "last wins" apart from "highest index of two".
    fn v3_stream_capture(turn1: &str, turn2: &str, turn3: &str) -> String {
        format!(
            "{}\n{}\n{}\n{}\n",
            V3_INIT_EVENT,
            v3_result_event(V3_RESULT_TURN1, turn1),
            v3_result_event(V3_RESULT_TURN2, turn2),
            v3_result_event(V3_RESULT_TURN3, turn3),
        )
    }

    // ---- rate-limit / envelope-failure fixtures (plan 30-03) --------------

    /// v3 line 15, **VERBATIM** — the only `rate_limit_event` in any archived
    /// capture, and the reason this plan exists in its current form.
    ///
    /// Read it before touching [`detect_claude_stream_rate_limit`]: its
    /// `rate_limit_info.status` is **`allowed`**. The CLI emits these events as
    /// routine quota telemetry on healthy streams — this one sits at line 15 of
    /// a capture that then completed three turns successfully (results at 19,
    /// 37 and 54). Presence of the event type carries NO information about
    /// whether the run was blocked.
    ///
    /// Note the second trap one level down: `overageStatus` is `rejected`. Any
    /// nested search for the token `rejected` (e.g. via [`json_find_key`]) also
    /// misclassifies this healthy event, which is why the classifier reads
    /// `rate_limit_info.status` and nothing else, by direct `.get()`.
    const V3_RATE_LIMIT_EVENT_ALLOWED: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1785645600,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false},"uuid":"e73e6774-a79d-4cdf-90bd-53a695f44f5a","session_id":"559fef4d-2053-459e-b7a7-f3200c3b3790"}"#;

    /// A `rate_limit_event` with the given `rate_limit_info.status`, built by
    /// substituting one field of the real archived event above.
    ///
    /// **SYNTHETIC for every status except `allowed`.** No archived capture
    /// contains a blocked stream — the denial fixtures below are constructed,
    /// not observed, and are labelled as such at each use. Every other field
    /// (including `resetsAt`, which supplies the retry hint) is exactly as
    /// captured.
    fn v3_rate_limit_event(status: &str) -> String {
        assert!(
            V3_RATE_LIMIT_EVENT_ALLOWED.contains(r#""status":"allowed""#),
            "fixture lost its status field"
        );
        V3_RATE_LIMIT_EVENT_ALLOWED
            .replace(r#""status":"allowed""#, &format!(r#""status":"{status}""#))
    }

    /// One real `result` envelope with its captured `is_error":false` flipped
    /// to `true`, every other field untouched. The assertion makes the
    /// substitution non-silent: if the fixture text ever changes, the test
    /// fails loudly rather than quietly testing an `is_error: false` envelope.
    fn v3_result_event_is_error(envelope: &str, escaped_result_text: &str) -> String {
        let filled = v3_result_event(envelope, escaped_result_text);
        assert!(
            filled.contains(r#""is_error":false"#),
            "fixture envelope lost its is_error field"
        );
        filled.replace(r#""is_error":false"#, r#""is_error":true"#)
    }

    /// Assemble a capture from the real `init` event followed by the given
    /// lines in order. Unlike [`v3_stream_capture`] this lets a test position a
    /// `rate_limit_event` at an arbitrary index, which is the whole point of
    /// the final-turn scoping assertions.
    fn stream_capture_of(lines: &[&str]) -> String {
        let mut out = String::from(V3_INIT_EVENT);
        for line in lines {
            out.push('\n');
            out.push_str(line);
        }
        out.push('\n');
        out
    }

    /// **The mandatory negative regression.** The real archived stream — whose
    /// `rate_limit_event` says `status: "allowed"` and which then completed
    /// three turns — must NOT classify as `RateLimited`.
    ///
    /// This event is routine quota telemetry, not a block. Classifying its mere
    /// presence as a rate limit would route EVERY healthy Claude stream stage
    /// into `Action::AutoResume` (`crates/devflow-cli/src/outcome_policy.rs:41`
    /// maps `AgentStatus::RateLimited` to it) against a fabricated retry time,
    /// instead of advancing the pipeline. That is a denial of service on the
    /// whole product, produced by a one-line "detect the event type" shortcut.
    ///
    /// Two independent guards must both hold here, and the second assertion
    /// pins the one the positioning guard alone would hide: the event is placed
    /// at its real position (before the first `result`, mirroring line 15 vs
    /// 19), AND its status is not a denial. `detect_claude_stream_rate_limit`
    /// is asserted directly on a final-turn placement of the same real event so
    /// the status guard cannot be dropped without this test failing.
    #[test]
    fn claude_stream_real_allowed_rate_limit_event_is_not_rate_limited() {
        let capture = stream_capture_of(&[
            V3_RATE_LIMIT_EVENT_ALLOWED,
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN3, MARKER_SUCCESS),
        ]);

        let result = parse_claude_event_result(&capture)
            .expect("the final turn's success marker still decides this stream");
        assert_eq!(result.status, AgentStatus::Success);
        assert_ne!(result.status, AgentStatus::RateLimited);

        // The status guard on its own: the SAME real event moved into the final
        // turn (after the second-to-last `result`) is still not a rate limit.
        // Without this, deleting the status check would leave the test green.
        let final_turn = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            V3_RATE_LIMIT_EVENT_ALLOWED,
            &v3_result_event(V3_RESULT_TURN3, MARKER_SUCCESS),
        ]);
        assert!(detect_claude_stream_rate_limit(&claude_stream_events(&final_turn)).is_none());
    }

    /// The positive: an explicit quota DENIAL inside the final turn classifies
    /// as `RateLimited`, so the rate-limit resume path stays reachable under
    /// `stream-json`.
    ///
    /// **The denial fixture is SYNTHETIC.** No archived capture contains a
    /// blocked stream, so the `rejected` status is constructed from the
    /// observed vocabulary of this schema rather than observed in the wild —
    /// the same honest-fixture rule this phase applies to marker payloads. The
    /// retry hint comes from the real `resetsAt` value.
    #[test]
    fn claude_stream_final_turn_denial_rate_limit_event_is_rate_limited() {
        let denial = v3_rate_limit_event("rejected");
        let capture = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            &denial,
            &v3_result_event(V3_RESULT_TURN3, NO_MARKER),
        ]);

        let result = parse_claude_event_result(&capture)
            .expect("a final-turn quota denial must produce a Layer-1 verdict");
        assert_eq!(result.status, AgentStatus::RateLimited);
        assert_eq!(
            result.reason.as_deref(),
            Some("rate limited until 1785645600")
        );
        assert_eq!(result.decided_by_layer, Some(1));

        // Fewer than two `result` events means the whole stream IS the final
        // turn — a run blocked before it ever completed a turn must still
        // classify, or the boundary logic silently swallows the common case.
        let single_turn =
            stream_capture_of(&[&denial, &v3_result_event(V3_RESULT_TURN1, NO_MARKER)]);
        assert_eq!(
            parse_claude_event_result(&single_turn).map(|r| r.status),
            Some(AgentStatus::RateLimited)
        );
    }

    /// Scoping: a denial that predates the final turn cannot outrank the final
    /// turn's own outcome. Rate-limit chatter from an earlier turn must not
    /// decide a stream that later completed — in the real capture the rate
    /// event (line 15) precedes all three results, so an unscoped detector
    /// would let a first-turn event decide a stream that finished forty seconds
    /// later.
    ///
    /// The denial status here is the SAME one the positive test proves does
    /// classify, so this test can only pass because of the POSITION guard.
    #[test]
    fn claude_stream_denial_before_final_turn_does_not_outrank_final_result() {
        let capture = stream_capture_of(&[
            &v3_rate_limit_event("rejected"),
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN3, MARKER_SUCCESS),
        ]);

        let result = parse_claude_event_result(&capture)
            .expect("the final turn's success marker decides this stream");
        assert_eq!(result.status, AgentStatus::Success);
    }

    /// An unrecognised `rate_limit_info.status` DEFERS rather than classifying.
    ///
    /// Deferring is the deliberately safe direction: an unknown denial status
    /// falls through to the envelope/marker paths and is reported `Failed` — a
    /// real degradation (the operator loses automatic resume) but a never-silent
    /// one that still gates. The opposite error auto-resumes a healthy stream
    /// against a retry time the parser invented.
    ///
    /// Positioned in the FINAL turn, so only the status check can decline it.
    #[test]
    fn claude_stream_unrecognised_rate_limit_status_defers() {
        let capture = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            &v3_rate_limit_event("some_future_status"),
            &v3_result_event(V3_RESULT_TURN3, MARKER_SUCCESS),
        ]);

        assert!(detect_claude_stream_rate_limit(&claude_stream_events(&capture)).is_none());
        let result = parse_claude_event_result(&capture)
            .expect("the parser must fall through to the marker path");
        assert_eq!(result.status, AgentStatus::Success);
    }

    /// Precedence (T-30-13): when the detector fires, rate limit outranks the
    /// marker path. A rate-limited run classified as generic `Failed` kills the
    /// primary rate-limit resume cron — the one path that exists to recover
    /// from it — which is exactly why `evaluate_layer1` already orders
    /// `detect_claude_rate_limit` ahead of `detect_claude_envelope_failure` for
    /// the single-document path.
    ///
    /// Non-vacuous: the same capture WITHOUT the rate event yields `Failed`, so
    /// this test fails the moment the ordering is reshuffled.
    #[test]
    fn claude_stream_final_turn_denial_outranks_failed_marker() {
        let with_denial = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            &v3_rate_limit_event("rejected"),
            &v3_result_event(V3_RESULT_TURN3, MARKER_FAILED),
        ]);
        assert_eq!(
            parse_claude_event_result(&with_denial).map(|r| r.status),
            Some(AgentStatus::RateLimited)
        );

        let without_denial = v3_stream_capture(NO_MARKER, NO_MARKER, MARKER_FAILED);
        assert_eq!(
            parse_claude_event_result(&without_denial).map(|r| r.status),
            Some(AgentStatus::Failed)
        );
    }

    /// A last `result` event with `is_error: true` and NO marker is an
    /// authoritative Layer-1 failure, not a deferral to Layer 2's coarse
    /// exit-code heuristic — matching `detect_claude_envelope_failure` for the
    /// single-document envelope. The reason is drawn from the event's own
    /// `result` text with the `num_turns` suffix, the same shape that function
    /// produces.
    #[test]
    fn claude_stream_last_result_is_error_without_marker_is_failed() {
        let capture = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            &v3_result_event_is_error(V3_RESULT_TURN3, r#"Execution error: context exhausted"#),
        ]);

        let result = parse_claude_event_result(&capture)
            .expect("is_error on the last result must not defer to Layer 2");
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(
            result.reason.as_deref(),
            Some("Execution error: context exhausted (num_turns: 2)")
        );
        assert_eq!(result.decided_by_layer, Some(1));
    }

    /// Envelope-over-marker (T-30-15): `is_error: true` overrides a SUCCESS
    /// marker in the same event, matching `detect_claude_envelope_failure`'s
    /// documented precedence over a stale or echoed success marker.
    ///
    /// Non-vacuous: the identical capture with `is_error: false` yields
    /// `Success`, so the assertion below can only pass because the envelope
    /// check overrode the marker.
    #[test]
    fn claude_stream_is_error_overrides_success_marker() {
        let capture = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            &v3_result_event_is_error(V3_RESULT_TURN3, MARKER_SUCCESS),
        ]);
        let result = parse_claude_event_result(&capture)
            .expect("is_error must produce a verdict even with a success marker");
        assert_eq!(result.status, AgentStatus::Failed);

        let healthy = v3_stream_capture(NO_MARKER, NO_MARKER, MARKER_SUCCESS);
        assert_eq!(
            parse_claude_event_result(&healthy).map(|r| r.status),
            Some(AgentStatus::Success)
        );
    }

    // ---- session id from a stream capture (plan 30-03 Task 2) -------------

    /// The single `session_id` every event in the archived v3 capture carries —
    /// all three `init` events (lines 5, 32 and 47) and all three `result`
    /// events agree on it, confirmed by reading the capture.
    const V3_SESSION_ID: &str = "559fef4d-2053-459e-b7a7-f3200c3b3790";

    /// The real `init` event with its `session_id` substituted. Used only to
    /// build a SYNTHETIC mid-stream rotation — no archived capture rotates.
    fn v3_init_event_with_session(session_id: &str) -> String {
        assert!(
            V3_INIT_EVENT.contains(V3_SESSION_ID),
            "fixture lost its session_id"
        );
        V3_INIT_EVENT.replace(V3_SESSION_ID, session_id)
    }

    /// `claude_stream_session_id` reads the CLI-emitted id out of a JSONL
    /// capture built from the archived `init` events (v3 lines 5, 32 and 47 —
    /// all three carry this same value).
    ///
    /// The second half pins LAST-init-wins with a synthetic rotation: the real
    /// capture's three `init` events are identical, so first-wins and last-wins
    /// agree on today's evidence and a fixture built only from it cannot tell
    /// the two apart. Three `init` events do NOT mean three sessions.
    #[test]
    fn claude_stream_session_id_reads_cli_emitted_init_value() {
        let capture = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            V3_INIT_EVENT,
            &v3_result_event(V3_RESULT_TURN2, NO_MARKER),
            V3_INIT_EVENT,
            &v3_result_event(V3_RESULT_TURN3, MARKER_SUCCESS),
        ]);
        assert_eq!(
            claude_stream_session_id(&capture).as_deref(),
            Some(V3_SESSION_ID)
        );

        let rotated = stream_capture_of(&[
            &v3_result_event(V3_RESULT_TURN1, NO_MARKER),
            &v3_init_event_with_session("second-session-id"),
            &v3_result_event(V3_RESULT_TURN2, MARKER_SUCCESS),
        ]);
        assert_eq!(
            claude_stream_session_id(&rotated).as_deref(),
            Some("second-session-id")
        );
    }

    /// D-04 / T-28-04 forgery guard for the stream path — the analog of
    /// `session_id_in_devflow_result_marker_is_not_returned`, which pins the
    /// same contract for the single-document envelope.
    ///
    /// The fixture defeats BOTH plausible wrong implementations at once: a
    /// nested traversal (`json_find_key`/`json_scan`) would reach the
    /// `session_id` the agent planted inside its own `DEVFLOW_RESULT` marker
    /// text, and a "last event carrying a `session_id`" scan would return the
    /// final `result` event's own key. Both are wrong; only the `init` event's
    /// top-level value is CLI-emitted. The divergence between the `result`
    /// event's id and the `init` event's is synthetic — no archived capture
    /// diverges — and exists purely so those two implementations cannot pass.
    #[test]
    fn claude_stream_session_id_ignores_agent_planted_value() {
        const PLANTED_MARKER: &str =
            r#"Done.\nDEVFLOW_RESULT: {\"status\":\"success\",\"session_id\":\"forged-by-agent\"}"#;

        let last_result = v3_result_event(V3_RESULT_TURN3, PLANTED_MARKER)
            .replace(V3_SESSION_ID, "result-event-session-id");
        let capture =
            stream_capture_of(&[&v3_result_event(V3_RESULT_TURN1, NO_MARKER), &last_result]);

        // Non-vacuity: both decoys really are present in the capture text, so a
        // wrong implementation has something wrong to find.
        assert!(capture.contains("forged-by-agent"));
        assert!(capture.contains("result-event-session-id"));

        assert_eq!(
            claude_stream_session_id(&capture).as_deref(),
            Some(V3_SESSION_ID)
        );
    }

    /// The stream reader does not shadow or duplicate `claude_session_id`: it
    /// declines the single-document envelope (the exact literal
    /// `session_id_reads_top_level_string` asserts on) and plain text, so the
    /// wrapper's stream-first ordering cannot change today's behavior.
    #[test]
    fn claude_stream_session_id_declines_non_stream_shapes() {
        let envelope = r#"{"type":"result","subtype":"success","result":"All done.","session_id":"cf29bfec-69e8-45df-a4f3-3da08ab6f66e"}"#;
        assert!(claude_stream_session_id(envelope).is_none());
        // ...and the shipped reader still owns it, so declining costs nothing.
        assert_eq!(
            claude_session_id(envelope).as_deref(),
            Some("cf29bfec-69e8-45df-a4f3-3da08ab6f66e")
        );

        assert!(claude_stream_session_id("just some plain text output\n").is_none());
    }

    /// The wiring that matters: `session_id_from_capture` — the Phase 28
    /// checkpoint-resume reader (`claude --resume` needs an id DevFlow can
    /// read) — returns an id for a JSONL capture, where before this plan it
    /// returned `None` for every stream capture.
    #[test]
    fn claude_stream_session_id_from_capture_reads_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 30),
            v3_stream_capture(NO_MARKER, NO_MARKER, MARKER_SUCCESS),
        )
        .unwrap();

        assert_eq!(
            session_id_from_capture(dir.path(), 30).as_deref(),
            Some(V3_SESSION_ID)
        );
    }

    /// The other half of the wiring claim: a single-document envelope capture
    /// still yields exactly what it did before the stream reader was inserted
    /// ahead of `claude_session_id` in the fallback chain.
    #[test]
    fn claude_stream_wiring_leaves_single_document_capture_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        let envelope = r#"{"type":"result","subtype":"success","result":"All done.","session_id":"cf29bfec-69e8-45df-a4f3-3da08ab6f66e"}"#;
        std::fs::write(stdout_path(dir.path(), 8), envelope).unwrap();

        assert_eq!(
            session_id_from_capture(dir.path(), 8).as_deref(),
            claude_session_id(envelope).as_deref()
        );
        assert_eq!(
            session_id_from_capture(dir.path(), 8).as_deref(),
            Some("cf29bfec-69e8-45df-a4f3-3da08ab6f66e")
        );
    }

    /// The tracer: a real archived `stream-json` capture written to
    /// `.devflow/phase-NN-stdout` produces a Layer-1 verdict out of
    /// `evaluate_layer1`. Before plan 30-01 this returned `None` for every
    /// JSONL capture — `serde_json::from_str` on the whole multi-line document
    /// is a hard "trailing characters" error, so all four single-document
    /// parsers declined it and the stage fell through to Layer 2's coarse
    /// exit-code+commit heuristic.
    ///
    /// Fixture provenance and its two modifications are documented on
    /// `V3_INIT_EVENT` / `V3_RESULT_TURN1..3` above.
    #[test]
    fn evaluate_layer1_parses_claude_stream_capture() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 30),
            v3_stream_capture(NO_MARKER, NO_MARKER, MARKER_SUCCESS),
        )
        .unwrap();

        let result = evaluate_layer1(dir.path(), 30).unwrap();

        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.decided_by_layer, Some(1));

        // Non-vacuity guard for the assertion above: this marker omits
        // `decided_by_layer`, and the field is `#[serde(default)]`, so
        // `parse_marker_lines` alone yields `None`. `Some(1)` can therefore
        // only have come from the parser's explicit overwrite.
        assert_eq!(
            parse_marker_lines(r#"DEVFLOW_RESULT: {"status":"success"}"#)
                .unwrap()
                .decided_by_layer,
            None
        );
    }

    /// Last-result-wins. A session kept alive across turns emits one `result`
    /// event per turn; only the final one is the session's verdict.
    ///
    /// Asserts BOTH directions so the test cannot pass by a parser that merely
    /// prefers `success`: failed-then-success yields Success, and
    /// success-then-failed yields Failed. The middle event carries the same
    /// payload as the first, so a parser that stopped at index 1 would also
    /// fail.
    #[test]
    fn claude_stream_last_result_event_wins_over_earlier_results() {
        let last_success = v3_stream_capture(MARKER_FAILED, MARKER_FAILED, MARKER_SUCCESS);
        let result = parse_claude_event_result(&last_success).unwrap();
        assert_eq!(result.status, AgentStatus::Success);

        let last_failed = v3_stream_capture(MARKER_SUCCESS, MARKER_SUCCESS, MARKER_FAILED);
        let result = parse_claude_event_result(&last_failed).unwrap();
        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.reason.as_deref(), Some("earlier turn aborted"));
    }

    /// T-30-26: `decided_by_layer` is provenance, not decoration.
    /// `crates/devflow-cli/src/pipeline_outcomes.rs` (`classify_validate_outcome`)
    /// computes `external = decided_by_layer == Some(0) && status == Success`
    /// and uses it to tell an externally-probe-verified Validate stage apart
    /// from an ordinary one. An agent that writes `"decided_by_layer": 0` into
    /// its own marker is claiming a Layer-0 probe provenance it did not earn,
    /// so the stream parser overwrites the field unconditionally.
    ///
    /// This is a runtime assertion on the returned struct, not a source grep —
    /// it fails the moment the overwrite is dropped.
    #[test]
    fn claude_stream_overwrites_agent_planted_decided_by_layer() {
        // Non-vacuity guard: prove the planted value really would survive
        // deserialization, so the `Some(1)` below is the overwrite at work and
        // not an artifact of a marker that failed to parse.
        assert_eq!(
            parse_marker_lines(r#"DEVFLOW_RESULT: {"status":"success","decided_by_layer":0}"#)
                .unwrap()
                .decided_by_layer,
            Some(0)
        );

        let capture = v3_stream_capture(NO_MARKER, NO_MARKER, MARKER_PLANTED_LAYER);
        let result = parse_claude_event_result(&capture).unwrap();

        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.decided_by_layer, Some(1));
    }

    /// A marker-less final turn defers to Layer 2 rather than reporting an
    /// unconditional Success — the same convention `parse_codex_event_result`
    /// applies to a bare `turn.completed`. A marker-less turn must never
    /// silently advance a stage.
    ///
    /// The FIRST turn carries a success marker, so this also proves the parser
    /// does not fall back to an earlier turn's marker when the last one has
    /// none.
    ///
    /// Plan 30-03 addendum: the deferral must hold specifically for
    /// `is_error: false`, which is what the real captured envelope carries —
    /// asserted below so this reads as a deliberate is_error case rather than
    /// an incidental one. Only `is_error: true` may promote a marker-less turn
    /// to `Failed`.
    #[test]
    fn claude_stream_last_result_without_marker_defers() {
        let capture = v3_stream_capture(MARKER_SUCCESS, NO_MARKER, NO_MARKER);
        assert!(
            capture.contains(r#""is_error":false"#),
            "the archived envelopes carry is_error:false; this test is about that case"
        );
        assert!(parse_claude_event_result(&capture).is_none());
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
    /// must outrank the generic is_error → Failed path, or the primary
    /// rate-limit resume cron never triggers for the exact case it exists for.
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

    /// D-05 gap 1 / D-06 (17-03): Layer 0 now evaluates on every stage, not
    /// only Code. Also covers the review-flagged worktree bug (Plan 03
    /// MEDIUM, OpenCode): PLAN discovery must read `project_root` (where
    /// `.planning/phases/` actually lives), while probe execution still
    /// reads `execution_root` (the worktree) — using the worktree for
    /// discovery would find zero commands and mis-fire the "PLAN removed"
    /// veto.
    #[test]
    fn external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("phase-worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let phase_dir = dir.path().join(".planning/phases/16-reliability");
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

        // Layer 0 now fires on Plan too — the probe file does not yet exist
        // in the worktree, so this must fail on the probe itself (NOT a
        // false PLAN-removed veto, which would mean discovery silently
        // returned zero commands).
        let plan_result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(plan_result.status, AgentStatus::Failed);
        assert!(
            plan_result
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("external verification failed")),
            "expected a failing-probe reason, not a false PLAN-removed veto: {:?}",
            plan_result.reason
        );

        state.stage = Stage::Code;
        let code_result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(code_result.status, AgentStatus::Failed);

        // The probe still executes against execution_root (the worktree) —
        // only PLAN discovery moved to project_root.
        std::fs::write(worktree.join("implemented"), "done").unwrap();
        let passing = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(passing.status, AgentStatus::Success);
        assert_eq!(passing.decided_by_layer, Some(0));
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

    /// D-05 gap 2 (17-03): a declared, operator-approved external
    /// post-condition whose probe passes is affirmative Success evidence on
    /// its own — even with zero commits and on a non-Code stage (Define
    /// here). No agent stdout is written at all, so if Layer 0 did not
    /// short-circuit, there would be nothing for Layer 1 to find and Layer 2
    /// would fall through for lack of an exit-code file.
    #[test]
    fn layer0_affirmative_success_on_non_code_stage_with_zero_commits() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir.path().join(".planning/phases/16-reliability");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join("16-01-PLAN.md"),
            "---\nexternal_verify: \"test -f shipped\"\n---\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("shipped"), "done").unwrap();
        let mut state = state_in(dir.path(), 16);
        state.stage = Stage::Define;

        let approval = vec!["test -f shipped".to_string()];
        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();

        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.decided_by_layer, Some(0));
        assert_eq!(result.commits, None);
        // Off-Validate stage: verdict reconciliation does not apply (18e).
        assert_eq!(result.verdict, None);
    }

    /// Review Plan 03 LOW (Codex+OpenCode), 16a: an approved all-passing
    /// Layer 0 probe intentionally outranks a Layer 1 self-reported failure
    /// marker — proven here at the cascade level (`evaluate_agent_result_inner`),
    /// not merely in isolation on `evaluate_layer0`.
    #[test]
    fn layer0_affirmative_success_outranks_layer1_failure_marker() {
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
        std::fs::write(dir.path().join("externally-shipped"), "done").unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"failed\",\"reason\":\"agent self-reported failure\"}\n",
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

        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.decided_by_layer, Some(0));
        // Off-Validate stage (Code): verdict reconciliation does not apply,
        // even though Layer 1's marker here reports a (failure) status (18e).
        assert_eq!(result.verdict, None);
    }

    /// D-05/18e: Layer 0's affirmative-success arm at `Stage::Validate` must
    /// consult Layer 1's verdict rather than discard it — the two-signal
    /// reconciliation `reconcile_layer0_verdict` adds. Covers all three
    /// verdict states Layer 1 can produce: pass, gaps, and no marker at all.
    #[test]
    fn layer0_affirmative_success_consults_layer1_verdict_at_validate() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir.path().join(".planning/phases/16-reliability");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join("16-01-PLAN.md"),
            "---\nexternal_verify: \"test -f shipped\"\n---\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("shipped"), "done").unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        let mut state = state_in(dir.path(), 16);
        state.stage = Stage::Validate;
        let approval = vec!["test -f shipped".to_string()];

        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"success\",\"verdict\":\"pass\"}\n",
        )
        .unwrap();
        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.decided_by_layer, Some(0));
        assert_eq!(result.verdict, Some(Verdict::Pass));

        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"success\",\"verdict\":\"gaps\"}\n",
        )
        .unwrap();
        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(result.verdict, Some(Verdict::Gaps));

        std::fs::remove_file(stdout_path(dir.path(), 16)).unwrap();
        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(result.verdict, None);
    }

    /// 18e's reconciliation is scoped to `Stage::Validate` only (flagged
    /// assumption in 18-05-PLAN.md): at every other stage an affirmative
    /// Layer 0 success must keep `verdict: None`, even when Layer 1's marker
    /// carries an explicit verdict.
    #[test]
    fn layer0_affirmative_success_keeps_none_verdict_off_validate() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir.path().join(".planning/phases/16-reliability");
        std::fs::create_dir_all(&phase_dir).unwrap();
        std::fs::write(
            phase_dir.join("16-01-PLAN.md"),
            "---\nexternal_verify: \"test -f shipped\"\n---\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("shipped"), "done").unwrap();
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(
            stdout_path(dir.path(), 16),
            "DEVFLOW_RESULT: {\"status\":\"success\",\"verdict\":\"pass\"}\n",
        )
        .unwrap();
        let state = state_in(dir.path(), 16); // Stage::Code by default
        let approval = vec!["test -f shipped".to_string()];

        let result = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();

        assert_eq!(result.status, AgentStatus::Success);
        assert_eq!(result.decided_by_layer, Some(0));
        assert_eq!(result.verdict, None);
    }

    /// Ordering edge (17a): with multiple declared probes, ALL must pass for
    /// affirmative Success — the first failing probe vetoes the outcome
    /// regardless of which position it occupies among the declarations.
    #[test]
    fn multiple_declared_probes_first_failure_vetoes_regardless_of_order() {
        let dir = tempfile::tempdir().unwrap();
        let phase_dir = dir.path().join(".planning/phases/16-reliability");
        std::fs::create_dir_all(&phase_dir).unwrap();
        // 16-01 comes first alphabetically and passes; 16-02 comes second and fails.
        std::fs::write(
            phase_dir.join("16-01-PLAN.md"),
            "---\nexternal_verify: \"test -f passing-artifact\"\n---\n",
        )
        .unwrap();
        std::fs::write(
            phase_dir.join("16-02-PLAN.md"),
            "---\nexternal_verify: \"test -f never-created\"\n---\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("passing-artifact"), "done").unwrap();
        let mut state = state_in(dir.path(), 16);
        state.stage = Stage::Define;

        let approval = vec![
            "test -f passing-artifact".to_string(),
            "test -f never-created".to_string(),
        ];
        let result_a = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval),
        )
        .unwrap();
        assert_eq!(result_a.status, AgentStatus::Failed);
        assert!(
            result_a
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("never-created")),
            "unexpected reason: {:?}",
            result_a.reason
        );

        // Swap which position fails: 16-01 now fails, 16-02 passes. The
        // overall outcome must still veto — order of declaration must not
        // matter.
        std::fs::write(
            phase_dir.join("16-01-PLAN.md"),
            "---\nexternal_verify: \"test -f still-missing\"\n---\n",
        )
        .unwrap();
        std::fs::write(
            phase_dir.join("16-02-PLAN.md"),
            "---\nexternal_verify: \"test -f passing-artifact\"\n---\n",
        )
        .unwrap();
        let approval_swapped = vec![
            "test -f still-missing".to_string(),
            "test -f passing-artifact".to_string(),
        ];
        let result_b = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval_swapped),
        )
        .unwrap();
        assert_eq!(result_b.status, AgentStatus::Failed);

        // Now make BOTH pass: only then is the outcome Success.
        std::fs::write(dir.path().join("still-missing"), "done").unwrap();
        let result_c = evaluate_agent_result_inner(
            dir.path(),
            &state,
            &GitFlowConfig::default(),
            Some(&approval_swapped),
        )
        .unwrap();
        assert_eq!(result_c.status, AgentStatus::Success);
        assert_eq!(result_c.decided_by_layer, Some(0));
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
        assert_eq!(result.decided_by_layer, Some(3));
    }

    /// D-02/D-03 case 3 (17-03): "process gone, nothing accounted for" — zero
    /// commits and no declared external post-condition — is a fail-closed
    /// `Failed` outcome that flags human review, not a blanket advanceable
    /// `Unknown`. The commits-present case above stays `Unknown` (gated
    /// downstream by Plan 04's never-advance dispatch, D-04) — only the
    /// zero-commit sub-case is reclassified here.
    #[test]
    fn evaluate_layer3_zero_commits_is_failed_and_flags_human_review() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_no_commit(dir.path(), 5);

        let result = evaluate_layer3(dir.path(), 5, &GitFlowConfig::default()).unwrap();

        assert_eq!(result.status, AgentStatus::Failed);
        assert_eq!(result.exit_code, None);
        assert_eq!(result.commits, Some(0));
        assert_eq!(result.decided_by_layer, Some(3));
        let reason = result.reason.unwrap();
        assert!(reason.contains("no work"), "reason was: {reason}");
        assert!(
            reason.to_ascii_lowercase().contains("human review"),
            "reason was: {reason}"
        );
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

    /// D-07 (17-01): the two new multi-word variants must serialize with
    /// their word boundary preserved — `#[serde(rename_all = "lowercase")]`
    /// alone would collapse `ResourceKilled` to `"resourcekilled"` (Pitfall 1).
    #[test]
    fn multi_word_variants_serialize_with_word_boundary() {
        assert_eq!(
            serde_json::to_string(&AgentStatus::ResourceKilled).unwrap(),
            "\"resource_killed\""
        );
        assert_eq!(
            serde_json::to_string(&AgentStatus::AgentUnavailable).unwrap(),
            "\"agent_unavailable\""
        );
        assert_eq!(
            serde_json::from_str::<AgentStatus>("\"resource_killed\"").unwrap(),
            AgentStatus::ResourceKilled
        );
        assert_eq!(
            serde_json::from_str::<AgentStatus>("\"agent_unavailable\"").unwrap(),
            AgentStatus::AgentUnavailable
        );
    }

    /// Existing variants must keep their pre-existing lowercase wire form
    /// unchanged by the two new variants' additions.
    #[test]
    fn existing_variants_keep_wire_form() {
        assert_eq!(
            serde_json::to_string(&AgentStatus::Success).unwrap(),
            "\"success\""
        );
        assert_eq!(
            serde_json::to_string(&AgentStatus::Failed).unwrap(),
            "\"failed\""
        );
        assert_eq!(
            serde_json::to_string(&AgentStatus::RateLimited).unwrap(),
            "\"ratelimited\""
        );
        assert_eq!(
            serde_json::to_string(&AgentStatus::Unknown).unwrap(),
            "\"unknown\""
        );
    }

    /// review consensus #1: `as_wire_str()` must never diverge from the serde
    /// form for ANY variant — pin it for all six via a single round-trip
    /// assertion (quotes stripped).
    #[test]
    fn as_wire_str_matches_serde_form_for_every_variant() {
        for variant in [
            AgentStatus::Success,
            AgentStatus::Failed,
            AgentStatus::RateLimited,
            AgentStatus::Unknown,
            AgentStatus::ResourceKilled,
            AgentStatus::AgentUnavailable,
        ] {
            let serde_form = serde_json::to_string(&variant).unwrap();
            let stripped = serde_form.trim_matches('"');
            assert_eq!(
                variant.as_wire_str(),
                stripped,
                "as_wire_str() diverged from serde form for {variant:?}"
            );
        }
    }

    #[test]
    fn evaluate_layer2_exit_137_is_resource_killed() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_commit(dir.path(), 20);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), 20), "137").unwrap();
        let state = state_in(dir.path(), 20);

        let result = evaluate_layer2(dir.path(), 20, &GitFlowConfig::default(), state.stage)
            .unwrap()
            .unwrap();

        assert_eq!(result.status, AgentStatus::ResourceKilled);
        assert_eq!(result.exit_code, Some(137));
    }

    #[test]
    fn evaluate_layer2_exit_127_is_agent_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_feature_commit(dir.path(), 21);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), 21), "127").unwrap();
        let state = state_in(dir.path(), 21);

        let result = evaluate_layer2(dir.path(), 21, &GitFlowConfig::default(), state.stage)
            .unwrap()
            .unwrap();

        assert_eq!(result.status, AgentStatus::AgentUnavailable);
        assert_eq!(result.exit_code, Some(127));
    }

    // -----------------------------------------------------------------
    // 27-03 (D-01/D-03): branch-exists + commit-count evidence resolves
    // the caller's own repository under a hostile GIT_DIR, not an
    // unrelated one.
    // -----------------------------------------------------------------

    /// D-03/T-27-08: `evaluate_layer2`'s branch-exists and commit-count
    /// evidence (the two production sites at what were base-commit lines
    /// 574/583) resolves `project_root`'s own repository even when the
    /// process inherited a hostile `GIT_DIR` pointed at an unrelated
    /// repository — proven with a real spawned `git` process, not by
    /// inspecting a `Command` object alone. Mirrors
    /// `version::tests::tag_reads_resolve_caller_root_under_a_hostile_git_dir`
    /// (27-03) and `origin_main_ancestor_status_holds_under_a_hostile_git_dir`
    /// (`git.rs`, 27-01): the hostile `GIT_DIR` this test's own `<verify>`
    /// entries exercise (`GIT_DIR=<hostile>/.git cargo test ... this test`)
    /// is injected the same way any inherited-env attack reaches
    /// `evaluate_layer2` in production — via the whole process's
    /// environment, then down into the spawned child unless the
    /// constructor scrubs it.
    ///
    /// Deliberately tests the mirror direction from the plan's literal
    /// framing (real repo HAS the feature branch with a real commit;
    /// the standard hostile-`GIT_DIR` harness's throwaway repository does
    /// NOT), because the standard harness (`git init -q "$HOSTILE"`, no
    /// `feature/phase-NN` branch) cannot itself manufacture a false
    /// *positive* — an empty repository has no branch to spuriously
    /// report as present. It can, however, still prove the scrub's
    /// necessity by manufacturing a false *negative*: before this plan's
    /// migration, the two unmigrated `Command::new("git")` sites inherit
    /// the poisoned `GIT_DIR` and silently read the hostile repository
    /// instead of `project_root` — `rev-parse --verify` reports the real
    /// branch absent, the commit count is undercounted to zero, and a
    /// real agent's completed work is wrongly classified `Failed`. This
    /// is the same trust-boundary violation T-27-08 names (a foreign
    /// repository's state substituting for the real one), reached from
    /// the opposite direction; the scrub this plan adds removes `GIT_DIR`'s
    /// ability to redirect the spawned child at all, closing both
    /// directions identically.
    /// 27-REVIEW WR-01: this test previously set no hostile environment at
    /// all — it asserted ordinary-path behavior and claimed a hostile-
    /// `GIT_DIR` proof, so it passed identically with or without the scrub
    /// and could never have caught a regression back to a bare
    /// `Command::new("git")`. It now uses the spawned-child shape this
    /// phase established in `staleness.rs`
    /// (`embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`):
    /// `GIT_DIR` is never set on this process (Rust 2024 `unsafe`, unsound
    /// under threaded tests — Phase 25 D-14), only on one freshly spawned
    /// child that re-invokes this same binary filtered to this one test.
    #[test]
    fn branch_evidence_resolves_caller_root_under_a_hostile_git_dir() {
        const INNER_ROOT: &str = "DEVFLOW_27_03_BRANCH_EVIDENCE_INNER_ROOT";

        if let Ok(root) = std::env::var(INNER_ROOT) {
            // Inner mode: spawned by the outer half below with GIT_DIR
            // pointed at an unrelated foreign repository, scoped to this
            // child process only.
            let root = std::path::PathBuf::from(root);
            let phase = 27;
            let state = state_in(&root, phase);

            let result = evaluate_layer2(&root, phase, &GitFlowConfig::default(), state.stage)
                .unwrap()
                .unwrap();

            assert_eq!(
                result.status,
                AgentStatus::Success,
                "evaluate_layer2 must see project_root's own branch/commits, \
                 not a hostile GIT_DIR's repository: {result:?}"
            );
            assert_eq!(result.commits, Some(1));
            return;
        }

        // Outer mode: build the real repository (which HAS the feature
        // branch and its commit) plus a second, unrelated foreign
        // repository that has neither. Unscrubbed, the child would read the
        // foreign repo, find no branch, count zero commits, and misreport a
        // real agent's completed work as Failed.
        let dir = tempfile::tempdir().unwrap();
        let phase = 27;
        init_repo_with_feature_commit(dir.path(), phase);
        std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
        std::fs::write(exit_code_path(dir.path(), phase), "0").unwrap();

        let foreign = tempfile::tempdir().unwrap();
        git(foreign.path(), &["init", "-q"]);

        let exe = std::env::current_exe().expect("current_exe for child re-invocation");
        let out = std::process::Command::new(&exe)
            // Substring filter, NOT `--exact`: the binary's real test name is
            // module-qualified (`agent_result::tests::branch_evidence_...`),
            // so `--exact` against the bare name matches nothing, runs zero
            // tests, and still exits 0 — a false green.
            .arg("branch_evidence_resolves_caller_root_under_a_hostile_git_dir")
            .arg("--test-threads=1")
            .env(INNER_ROOT, dir.path().to_str().unwrap())
            .env("GIT_DIR", foreign.path().join(".git"))
            .output()
            .expect("spawn hostile child test process");

        let stdout = String::from_utf8_lossy(&out.stdout);
        // Assert the child actually RAN the test, not merely that it exited
        // 0. A filter matching nothing exits 0 with "0 passed".
        assert!(
            stdout.contains("1 passed"),
            "child test process must have run exactly the inner test; \
             stdout:\n{stdout}"
        );
        assert!(
            out.status.success(),
            "child test process (hostile GIT_DIR pointed at an unrelated \
             foreign repository) must still resolve project_root's own \
             branch and commits; child exit status {:?}\nstdout:\n{stdout}",
            out.status
        );
    }
}
