---
phase: 43-opencode-driver-completion
plan: 01
subsystem: agents
tags: [rust, opencode, agent-driver, jsonl-parsing, agent-completion-detection]

# Dependency graph
requires: []
provides:
  - "OpenCodeDriver::build_command emits the real headless launch argv (opencode run <prompt> --auto --format json)"
  - "parse_opencode_event_result / is_opencode_event_stream in agent_result.rs, wired into evaluate_layer1"
  - "Four git-tracked OpenCode JSONL fixtures (three real, one derived) for regression testing"
affects: [43-02, opencode-driver, agent-completion-parsing]

# Actuals (#2632)
actuals:
  tokens: 7800
  tasks: 3
  commits: 3

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "OpenCode JSONL event-stream parser modeled on parse_codex_event_result but adapted for a schema with no terminal-status event (error-presence-anywhere is the sole decisive signal, not last-event position)"

key-files:
  created:
    - crates/devflow-core/tests/fixtures/opencode/opencode_success.jsonl
    - crates/devflow-core/tests/fixtures/opencode/opencode_tool_use.jsonl
    - crates/devflow-core/tests/fixtures/opencode/opencode_error.jsonl
    - crates/devflow-core/tests/fixtures/opencode/opencode_success_with_marker.jsonl
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/agents/opencode.rs
    - crates/devflow-core/src/agents/mod.rs

key-decisions:
  - "Widened is_opencode_event_stream to also recognise OpenCode's nested error envelope shape (error.name as a string), not just step_start/step_finish — the real negative-control capture is a single-line, error-only stream with no step_start at all, and the RESEARCH-designed step-only gate would have rejected it, contradicting the plan's own must-have that this exact real capture resolves to Failed."

patterns-established:
  - "Torn-tail check, then error-event scan, then marker scan, in that fixed order — matches parse_codex_event_result's precedence so an earlier success marker can never override a later failure or an unreadable tail."

requirements-completed: [OPCD-01, OPCD-02]

coverage:
  - id: D1
    description: "OpenCodeDriver::build_command emits opencode run \"<prompt>\" --auto --format json as five separate argv elements"
    requirement: "OPCD-01"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_build_command_is_headless_json"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agents/mod.rs#agents::tests::opencode_wraps_prompt_in_run"
        status: pass
    human_judgment: false
  - id: D2
    description: "DEVFLOW_RESULT marker inside a text event's part.text resolves at Layer 1, with forged decided_by_layer:0 overwritten to Some(1)"
    requirement: "OPCD-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_marker_in_text_event_resolves_at_layer1"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_marker_cannot_forge_layer0_provenance"
        status: pass
    human_judgment: false
  - id: D3
    description: "An error event anywhere in the stream resolves to Failed with the provider's own message, and cannot be overridden by an earlier success marker"
    requirement: "OPCD-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_real_error_capture_is_failed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_error_event_overrides_earlier_success_marker"
        status: pass
    human_judgment: false
  - id: D4
    description: "A torn trailing line after the last parsed event returns indeterminate_capture_failure(), even ahead of an error event"
    requirement: "OPCD-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_torn_tail_after_marker_is_indeterminate"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_torn_tail_beats_error_event_ordering_is_stable"
        status: pass
    human_judgment: false
  - id: D5
    description: "All three real 43-evidence/ captures are vendored as git-tracked fixtures and parsed verbatim by regression tests; the real tool-use capture defers to Layer 2 rather than resolving Success off its trailing step_finish"
    requirement: "OPCD-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_real_success_capture_is_recognised_and_marker_less"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::opencode_real_tool_use_capture_defers_to_layer2"
        status: pass
    human_judgment: false

duration: ~25min
completed: 2026-08-23
status: complete
---

# Phase 43 Plan 01: OpenCode Headless Launch and Layer-1 Completion Parsing Summary

**OpenCode driver now launches `opencode run "<prompt>" --auto --format json` and its JSONL output resolves through a new `parse_opencode_event_result` (marker, error, torn-tail) regression-tested against three real live captures.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3
- **Files modified:** 7 (3 production, 4 fixtures)

## Accomplishments

- `OpenCodeDriver::build_command` emits the real headless launch argv (`opencode run "<prompt>" --auto --format json`, five separate argv elements, never `--format=json`), replacing the Phase-37 stub. Module doc comment names `--auto`'s auto-approve-permissions danger explicitly (P-01).
- New `is_opencode_event_stream` / `parse_opencode_event_result` in `agent_result.rs`, wired into `evaluate_layer1`'s `.or_else` chain right after the Codex parser. Structure: torn-tail check first (D-06) → `error`-event scan (D-05) → last `type:"text"` event's marker scan (D-04) → `None` (defer to Layer 2). No terminal-status event lookup (`step_finish` is never treated as a completion signal — RESEARCH Pitfall 1).
- `OpenCodeDriver::parse_completion` is a one-line delegation to the new function (matches Codex's pattern, no reimplementation).
- All three REAL `opencode --auto --format json` captures from `43-evidence/` vendored verbatim as git-tracked fixtures under `crates/devflow-core/tests/fixtures/opencode/`, plus a clearly-labelled DERIVED marker fixture (none of the real captures carries a marker). Leak-scanned clean (P-04).
- The two pre-existing argv assertions in `agents/mod.rs` updated for the new five-element argv (RESEARCH Pitfall 3).

## Task Commits

1. **Task 1: Tracer — launch argv through to a parsed Layer-1 verdict** - `b5fd6ee` (feat)
2. **Task 2: Failure and indeterminacy precedence — error events and torn tails** - `5caffc3` (feat)
3. **Task 3: Multi-step robustness — tool-use, marker-less fallthrough, malformed shapes** - `8a9c668` (test)

_All three tasks followed RED-then-GREEN TDD: each task's tests were written and confirmed failing (compile error for Task 1's new functions, assertion failures for Tasks 2/3) before the corresponding production code was written._

## Files Created/Modified

- `crates/devflow-core/src/agents/opencode.rs` - real `build_command` argv (D-01) + `parse_completion` delegation; `render_prompt` unchanged (D-02)
- `crates/devflow-core/src/agent_result.rs` - `is_opencode_event_stream`, `parse_opencode_event_result`, `evaluate_layer1` chain entry, and 19 regression tests
- `crates/devflow-core/src/agents/mod.rs` - two argv-asserting tests updated to the new five-element argv
- `crates/devflow-core/tests/fixtures/opencode/opencode_success.jsonl` - real capture, plain-text reply, no marker
- `crates/devflow-core/tests/fixtures/opencode/opencode_tool_use.jsonl` - real capture, tool-invoking multi-step turn, no marker
- `crates/devflow-core/tests/fixtures/opencode/opencode_error.jsonl` - real capture, negative control (invalid `--model`, exit 1)
- `crates/devflow-core/tests/fixtures/opencode/opencode_success_with_marker.jsonl` - DERIVED from the real success capture with a `DEVFLOW_RESULT` marker injected

## Decisions Made

- Widened `is_opencode_event_stream`'s gate to also recognise OpenCode's own nested error envelope shape (`error.name` present as a string) alongside `step_start`/`step_finish` — see Deviations below for the full rationale.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `is_opencode_event_stream` rejected the real negative-control capture**
- **Found during:** Task 2 (`opencode_real_error_capture_is_failed`, RED phase)
- **Issue:** RESEARCH.md and CONTEXT.md both specify the detector gate as "at least one event's top-level `type` is `step_start` or `step_finish`" — deliberately excluding a bare `type:"error"` to avoid cross-adapter collision (T-43-07). But the REAL `opencode_error.jsonl` evidence capture is a **single-line, error-only stream**: the process exits the instant the `error` event lands, before any `step_start` is ever emitted. Under the RESEARCH-specified gate, this genuine real capture would fail `is_opencode_event_stream` and `parse_opencode_event_result` would return `None` (defer to Layer 2) — directly contradicting the plan's own must-have truth ("A `type:\"error\"` event anywhere in the stream resolves to `AgentStatus::Failed`") and its own acceptance criterion that `opencode_real_error_capture_is_failed` must pass.
- **Fix:** Widened `is_opencode_event_stream` to ALSO match when an event's `type` is `"error"` AND that event carries OpenCode's own nested shape (`error.name` present as a string) — not a bare `type:"error"` alone. This keeps the anti-collision intent behind T-43-07 intact: no other adapter in this codebase emits that specific combination (Codex's failure event is `type:"turn.failed"`, not `type:"error"`; Claude's is `is_error: true` inside `type:"result"`). A literally bare `{"type":"error"}` with no nested `error` object still returns `false` from the detector — proven by `opencode_non_stream_input_returns_none`.
- **Files modified:** `crates/devflow-core/src/agent_result.rs`
- **Verification:** `opencode_real_error_capture_is_failed` and `opencode_non_stream_input_returns_none` both pass; full `agent_result::` suite (188 tests) and `agents::` suite (53 tests) green; no cross-adapter isolation test (`codex_stream_not_consumed_by_claude_stream_parser`, `claude_stream_not_consumed_by_codex_parser`, etc.) regressed.
- **Committed in:** `5caffc3` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Necessary for correctness — without it, the plan's own literal acceptance criterion (`opencode_real_error_capture_is_failed`) would be impossible to satisfy while still following the RESEARCH-specified detector design. No scope creep; the fix is narrowly scoped to the one nested shape OpenCode's real error envelope carries.

## Issues Encountered

None beyond the deviation above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `parse_opencode_event_result`'s marker/error/torn-tail paths are complete and regression-tested; 43-02 (health check + capability discovery, OPCD-03) can build on this driver without touching the parsing logic.
- `default_preflight_is_ok_for_built_in_adapters` in `agents/mod.rs` still asserts `driver_for(AgentKind::OpenCode).health(&state).is_ok()` against the trait's default no-op `health` — deliberately left untouched per the plan (43-02 owns removing this once `OpenCodeDriver::health` becomes a real credential check).
- No blockers.

## Self-Check: PASSED

All 7 created/modified source and fixture files confirmed present on disk. All 3 task commit hashes (`b5fd6ee`, `5caffc3`, `8a9c668`) confirmed present in `git log --oneline --all`.

---
*Phase: 43-opencode-driver-completion*
*Completed: 2026-08-23*
