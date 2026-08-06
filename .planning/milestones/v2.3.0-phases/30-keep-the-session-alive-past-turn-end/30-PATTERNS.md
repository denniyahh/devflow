# Phase 30: Keep the Session Alive Past Turn End - Pattern Map

**Mapped:** 2026-08-02
**Files analyzed:** 3 (1 source file modified in-place, 2 new standalone experiment harnesses)
**Analogs found:** 3 / 3 — all in-repo, no external pattern needed

All line numbers below were independently re-read this session from the live
source at `crates/devflow-core/src/agent_result.rs` (not trusted from
RESEARCH.md's citations alone) and confirmed to match RESEARCH.md exactly.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|----------------|
| `crates/devflow-core/src/agent_result.rs` — new `parse_claude_event_result` fn | utility (pure parser) | streaming / transform (JSONL → `Option<AgentResult>`) | `parse_codex_event_result` (same file, lines 551-612) | exact — same file, same role, same data flow, sibling in the same `.or_else()` cascade |
| `crates/devflow-core/src/agent_result.rs` — new `is_claude_event_stream` fn | utility (gate predicate) | transform | `is_codex_event_stream` (same file, lines 523-529) | exact |
| `crates/devflow-core/src/agent_result.rs` — new tests in `mod tests` | test | transform (unit) | `codex_event_stream_parses_turn_failed` (lines 1730-1741) and the `blocking_human_checkpoint_reported_*` regression-test cluster (lines 1613-1728) | exact — same test module, same `concat!`-literal convention |
| `.planning/phases/30-.../30c-<name>.py` (env-replication harness) | utility (standalone script, NOT shipped code) | event-driven / process orchestration | `.planning/phases/30-.../30a-evidence/run_experiment_v3.py` (existing harness) + `crates/devflow-core/src/monitor.rs:45-179` (`spawn_monitor`/`spawn_monitor_inner`, read-only reference for env-scrub parity) | role-match — `run_experiment_v3.py` is the direct ancestor to extend/copy; `spawn_monitor` is a **read-only reference**, not a Rust file to touch |
| `.planning/phases/30-.../30d-<name>.py` (exit-timing re-measurement) | utility (standalone script) | batch / measurement | `.planning/phases/30-.../30a-evidence/run_experiment_v3.py` | exact — same harness family, likely a direct extension per RESEARCH.md's own suggestion |

## Pattern Assignments

### `parse_claude_event_result` / `is_claude_event_stream` (new, `agent_result.rs`)

**Analog:** `parse_codex_event_result` / `is_codex_event_stream`, same file,
**lines 523-612** (re-read and confirmed verbatim this session — unchanged from
RESEARCH.md's citation).

**Gate function pattern to mirror** (lines 523-529):
```rust
fn is_codex_event_stream(events: &[serde_json::Value]) -> bool {
    events.iter().any(|v| {
        v.get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|t| t == "thread.started" || t.starts_with("turn."))
    })
}
```
The Claude sibling (`is_claude_event_stream`) should take the same
`&[serde_json::Value]` shape and key off `type: "system", subtype: "init"`
(present at v3 lines 5, 32, 47 — confirmed this session via direct read of
`30a-evidence/raw_output_v3.jsonl`) or `type: "result"` with a `session_id`
field, per RESEARCH.md's recommendation.

**Core parse pattern to mirror** (lines 551-612 — full function body):
```rust
fn parse_codex_event_result(stdout: &str) -> Option<AgentResult> {
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();

    if !is_codex_event_stream(&events) {
        return None;
    }

    // reverse-iterate for last DEVFLOW_RESULT marker inside a decoded event
    // field, deferring to the terminal-event check only if no marker found;
    // terminal-but-ambiguous (`turn.completed`) returns None (defer to Layer 2)
    // rather than guessing Success.
    let marker = events.iter().rev().find_map(|v| { /* ... */ });
    if marker.is_some() { return marker; }

    let terminal = events.iter().rev().find(|v| {
        matches!(v.get("type").and_then(serde_json::Value::as_str),
            Some("turn.completed") | Some("turn.failed"))
    })?;
    // ... only turn.failed is decisive; else None ...
}
```
**Shape to copy exactly for the Claude sibling:** (1) `.lines().filter_map(serde_json::from_str)` collect into `Vec<Value>`; (2) gate via `is_claude_event_stream`, return `None` early if false; (3) reverse-iterate (`.iter().rev().find_map`/`.find`) for **last-result-wins** — RESEARCH.md's H1 finding requires this explicitly, since v3 has 3 `result` events; (4) locate the LAST top-level `result` event (`type: "result"`, and per Pattern 2 below, no `parent_tool_use_id` key — confirmed absent from all `result` events in the archived captures), extract its `.result` string field, and run it through the EXISTING `parse_marker_lines` (lines 619-647, unmodified — do not duplicate its logic); (5) defer (`None`) rather than guess when the terminal signal is ambiguous, matching Codex's own convention.

**`AgentResult` construction pattern** (used identically by both
`parse_codex_event_result` line 603-611 and `detect_claude_envelope_failure`
line 396-404 — copy this literal shape, all six fields, `decided_by_layer:
Some(1)`):
```rust
Some(AgentResult {
    status: AgentStatus::Failed,
    exit_code: None,
    reason: Some(reason),
    commits: None,
    summary: None,
    verdict: None,
    decided_by_layer: Some(1),
})
```
`AgentResult`'s full field list (struct at line 18-~44) and `AgentStatus`'s
variants (enum at line 47-~60, includes `Success`/`Failed`/`RateLimited`/
`Unknown`/`ResourceKilled`) were re-read this session — no new fields are
needed for 30b per RESEARCH.md's explicit "Deliberate deviation" note (session_id
stays a separate top-level reader, not an `AgentResult` field).

**Cascade wiring pattern** (`evaluate_layer1`, lines 660-675 — full function,
re-read this session, confirmed unchanged from RESEARCH.md's citation):
```rust
pub fn evaluate_layer1(project_root: &Path, phase: u32) -> Option<AgentResult> {
    let stdout_path = devflow_dir(project_root).join(format!("phase-{:02}-stdout", phase));
    let bytes = std::fs::read(&stdout_path).ok()?;
    let stdout = String::from_utf8_lossy(&bytes);
    detect_claude_rate_limit(&stdout)
        .map(rate_limited_result)
        .or_else(|| detect_claude_envelope_failure(&stdout))
        .or_else(|| parse_devflow_result(&stdout))
        .or_else(|| parse_codex_event_result(&stdout))
        .or_else(|| detect_codex_rate_limit(&stdout).map(rate_limited_result))
}
```
Insert `parse_claude_event_result` into this `.or_else()` chain per
RESEARCH.md's diagram — the exact insertion point (before or after
`parse_codex_event_result`) is Claude's discretion at plan time, but it must
NOT precede `detect_claude_rate_limit`/`detect_claude_envelope_failure` (those
stay authoritative for the single-doc envelope, unmodified, per Pitfall 1) and
should come before the Codex JSONL parser since `is_codex_event_stream`
already returns `false` for Claude JSONL by construction (no collision, but
ordering should still favor the more specific/adapter-matched gate first).

**Session-id extraction discipline to preserve** (`claude_session_id`, lines
266-273 — the D-04/T-28-04 security-relevant precedent, re-read this session):
```rust
pub fn claude_session_id(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    value.get("session_id")?.as_str().map(str::to_string)
}
```
30b's LAST-`init`-event `session_id` extraction must replicate this same
**top-level-only `.get("session_id")`** discipline — never route through
`json_scan`/`json_find_key` (lines 298-341, the generic recursive traversal
helpers), which would make an agent-forged nested `session_id` inside its own
`DEVFLOW_RESULT` marker text reachable. This is a locked security constraint
from V3 Session Management in RESEARCH.md, not a style preference.

**Test convention to copy exactly** (lines 1730-1741, confirmed verbatim
against live source this session — identical to RESEARCH.md's citation):
```rust
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
```
30b's tests must use the same `concat!("...\n", "...\n", ...)` literal shape,
one JSONL line per string segment, but the STRING CONTENT should be trimmed
excerpts copied verbatim from `30a-evidence/raw_output_v3.jsonl` (confirmed
this session: line 5 = `init`, **line 54** = the terminal `result` event with
`"origin":{"kind":"task-notification"}`) — with a doc comment citing exact
source line numbers, per Pitfall 5's resolution (fixtures live in-crate as
literals, never `include_str!`'d from `.planning/`).

> **[CORRECTED 2026-08-02, at plan time]** This paragraph originally cited
> "line 78". `raw_output_v3.jsonl` is **54 lines** long, so line 78 does not
> exist. Re-derived directly from the capture: `init` events at lines **5, 32,
> 47** (all three carrying the identical `session_id`
> `559fef4d-2053-459e-b7a7-f3200c3b3790`); `result` events at lines **19, 37,
> 54**, where line 19 carries **no** `origin` key and lines 37 and 54 carry
> `origin.kind == "task-notification"`; the sole `rate_limit_event` at line
> **15**; `background_tasks_changed` draining to an empty array at line **44**.
> 30-RESEARCH.md's line numbers were correct; this file's were not.
> `30a-evidence/README.md`'s "lines 36 and 53" are the same events 0-indexed.

---

### `text_reports_human_gate` scoping fix (Pitfall 3 regression, same file)

**Analog:** the existing `blocking_human_checkpoint_reported`/
`text_reports_human_gate` pair, lines 459-507, and its regression test
cluster, lines 1613-1728 (8 tests, re-confirmed present this session,
including `blocking_human_checkpoint_reported_matches_live_observed_rendering`
at line 1674 — a live-run-anchored regression test, the exact pattern the new
JSONL-scoped regression test should follow).

**Pattern to copy:** `text_reports_human_gate` (lines 487-507) is the pure
matcher — do not modify it. What 30b needs is a NEW caller-side scoping step
(per Pitfall 3's fix) that, when the input is a Claude JSONL stream, restricts
which event's text gets passed into `text_reports_human_gate`: only
`assistant`/`result` event text, with `parent_tool_use_id == null` (Pattern 2
below), excluding all `type: "user"` events (echoed prompt risk). Follow the
existing thin-wrapper convention (`checkpoint_reported_in_capture`, lines
512-518) for how a file-reading wrapper delegates to the pure matcher.

**Discrimination pattern (`parent_tool_use_id == null`)** — not yet present in
source (new for 30b), but RESEARCH.md's Pattern 2 gives the exact shape to add:
```rust
let is_top_level = event
    .get("parent_tool_use_id")
    .map(|v| v.is_null())
    .unwrap_or(true); // key absent (e.g. on `result`/`init`) = top-level
```
Confirmed against `30a-evidence/raw_output_v3.jsonl` — **corrected 2026-08-02
at plan time**, the original text cited non-existent lines 66/67/78 in a
54-line file. Re-derived by parsing every line: `parent_tool_use_id` is
**null** on lines 6, 7, 10, 12, 16, 17, 35, 36, 50, 51, 52, 53 (top-level);
**a `toolu_...` string** on lines 11, 18, 20, 22, 27, 28, 39, 40, 42, 43
(subagent-forwarded); and **absent entirely** from every `system`, `init` and
`result` event — including all three `result` events at lines 19, 37 and 54,
which are therefore top-level by the `unwrap_or(true)` default. Note that
`user` events appear on both sides (10, 16, 52 top-level; 27, 39, 42
forwarded), so event type alone never establishes provenance.

---

### `.devflow-core::monitor::spawn_monitor` (read-only reference for 30c, NOT modified)

**Analog:** `crates/devflow-core/src/monitor.rs`, `spawn_monitor`/
`spawn_monitor_inner` (confirmed this session: `spawn_monitor` at line 45,
`spawn_monitor_inner` at line 54; `hermetic_command("sh", workdir_path)` build
at line 162; `.stdin(Stdio::null())`/`.stdout(Stdio::null())`/
`.stderr(Stdio::null())` at lines 171-173).

**Env-scrub list to replicate (read-only, re-grep at implementation time per
RESEARCH.md's own caveat):** `crates/devflow-core/src/git.rs` —
`REPO_LOCAL_GIT_VARS` (const at line 27) and `ALSO_REDIRECTING_GIT_VARS`
(const at line 55), consumed by `hermetic_command` (line 87, `for var in
REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS)` at line 90).

**Constraint (locked decision, do not violate):** 30c's harness must be a
**new standalone Python script** replicating this environment shape (detached,
scrubbed env vars, stderr to its own file, `sh -c` launch) while keeping ITS
OWN Python-side stdin pipe open — it must NOT edit `monitor.rs` or call
`spawn_monitor` directly (that coupling is explicitly forbidden).

**Direct ancestor to extend/copy:**
`.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/run_experiment_v3.py`
already holds the Python-side "keep stdin open, launch `claude` via
`subprocess.Popen`, capture JSONL" pattern — confirmed present in the evidence
directory listing this session (`raw_output.jsonl`, `raw_output_v2.jsonl`,
`raw_output_v3.jsonl`, `README.md`, `run_experiment.py`, `run_experiment_v2.py`,
`run_experiment_v3.py`). 30c's harness is this file's process-shape
successor (add env-scrub + `sh -c` + detached-process launch on top of the
existing stdin-open + JSONL-capture core); 30d's harness is a re-measurement
extension of the same file, per RESEARCH.md's explicit note that
`run_experiment_v3.py` "may" be extended for 30d.

---

## Shared Patterns

### `Option<T>`-returning, never `Result<T, E>` (module-wide convention)
**Source:** every parsing function in `agent_result.rs`
(`parse_devflow_result`, `detect_claude_rate_limit`,
`detect_claude_envelope_failure`, `claude_session_id`,
`parse_codex_event_result`, `parse_marker_lines`) — confirmed by direct read
this session, all six return `Option<T>`, none return `Result`.
**Apply to:** `parse_claude_event_result` and `is_claude_event_stream` (return
`bool`, matching `is_codex_event_stream`'s signature). Do not introduce a new
error type — this would be inconsistent with the module's own established
convention, per RESEARCH.md's explicit warning.

### Reverse-iterate for last-wins semantics
**Source:** `parse_codex_event_result` lines 568 (`events.iter().rev().find_map`)
and 583 (`events.iter().rev().find`).
**Apply to:** All new Claude-stream logic that must pick the terminal/most
recent event — required by H1 (multiple `result` events per process is normal;
last-result semantics, never first-result).

### `AgentResult` literal construction (all six fields, `decided_by_layer: Some(1)`)
**Source:** `parse_codex_event_result` lines 603-611,
`detect_claude_envelope_failure` lines 396-404, `rate_limited_result` lines
678-688 — all three construct `AgentResult` with the identical six-field
literal shape.
**Apply to:** Every `Some(AgentResult { ... })` return site in
`parse_claude_event_result`.

### Lossy UTF-8 file read before parsing (CR-01 precedent)
**Source:** `evaluate_layer1` lines 667-668 —
`std::fs::read(&stdout_path).ok()?` then `String::from_utf8_lossy(&bytes)`,
never `read_to_string`.
**Apply to:** Any new file-reading wrapper 30b adds around
`parse_claude_event_result` (mirroring `session_id_from_capture` line 280-284
and `checkpoint_reported_in_capture` line 512-518's existing wrapper shape) —
must use the same lossy-read pattern, not `read_to_string`, per the CR-01
regression this convention exists to prevent.

## No Analog Found

None. All three implementation surfaces (parser addition, checkpoint-gate
scoping fix, experiment harness) have a direct, exact-or-role-match analog
already in the codebase. RESEARCH.md's own "Key insight" (this phase's core
technical risk is a second instance of a problem already solved once for
Codex) is corroborated by this session's independent re-read of the source.

## Metadata

**Analog search scope:** `crates/devflow-core/src/agent_result.rs` (full
2964-line file, targeted reads at lines 18-60, 246-530, 551-675, 1730-1741),
`crates/devflow-core/src/monitor.rs` (grep for `spawn_monitor`/`Stdio::null`/
`hermetic_command`), `crates/devflow-core/src/git.rs` (grep for
`REPO_LOCAL_GIT_VARS`/`ALSO_REDIRECTING_GIT_VARS`/`hermetic_command`),
`.planning/phases/30-.../30a-evidence/` (directory listing +
`raw_output_v3.jsonl` lines 1-10, 30-40, direct read).
**Files scanned:** 3 Rust source files (1 primary, 2 read-only reference),
1 evidence directory (5 files listed, 1 read).
**Pattern extraction date:** 2026-08-02 (all line numbers independently
re-verified against live source this session, not copied from RESEARCH.md's
citations without re-check — all matched exactly, no drift found).
