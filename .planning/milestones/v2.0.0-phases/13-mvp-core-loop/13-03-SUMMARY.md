---
phase: 13-mvp-core-loop
plan: 03
subsystem: completion-protocol
tags: [rust, claude, codex, jsonl, agent-result, tdd]

# Dependency graph
requires:
  - phase: 13-mvp-core-loop
    provides: "13-01/13-02's failure-branch wiring and review:-prefixed ReviewFailed contract in main.rs/ship.rs"
provides:
  - "detect_claude_envelope_failure: reads Claude's top-level is_error/num_turns as an authoritative Layer-1 failure, overriding a stale/echoed success DEVFLOW_RESULT marker in the same envelope"
  - "parse_codex_event_result: a Codex --json JSONL event-stream parser (turn.failed decisive; turn.completed defers to Layer 2, never unconditional Success)"
  - "evaluate_layer2 scoped to Code-like stages: exit != 0 is Failed for every stage; the exit=0/zero-commits gate applies only to Stage::Plan/Stage::Code"
affects: [13-mvp-core-loop plan 05 (Validate verdict requirement composes with turn.completed's deferral so a marker-less Validate cannot false-pass to Ship), 13-mvp-core-loop plan 06 (dogfood run reconciles the Codex parser against real installed-CLI output)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Layer-1 precedence chain in evaluate_layer1: Claude envelope is_error (authoritative) -> DEVFLOW_RESULT marker (portable) -> Codex JSONL event stream (turn.failed decisive) -> rate limit"
    - "Discriminator function (is_codex_event_stream) gates a format-specific parser so two per-adapter output shapes sharing the same captured-stdout file never cross-consume each other"
    - "Stage-scoped fallback gate: explicit matches!(stage, Stage::Plan | Stage::Code) instead of Stage::is_agent_stage(), because the semantically-close helper (is_agent_stage) would incorrectly include Define"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs

key-decisions:
  - "is_error: true is checked BEFORE the DEVFLOW_RESULT marker path in evaluate_layer1, so an errored Claude envelope always overrides a stale/echoed success marker embedded in its own result text (Codex 13-03 MEDIUM)"
  - "turn.completed intentionally returns None (defers), never AgentStatus::Success — a marker-less Codex turn cannot silently advance a stage; the real success signal stays the marker or Layer 2's commit gate (consensus #1 from cross-AI review)"
  - "Codex discrimination requires >=1 thread.started/turn.* line before the parser is decisive, so a single-document Claude envelope is never treated as a Codex stream (Cursor 13-03 MEDIUM)"
  - "Layer 2's stage gate uses matches!(stage, Stage::Plan | Stage::Code), not is_agent_stage(), since is_agent_stage() also includes Define, which must NOT be commit-gated (Codex/OpenCode consensus #9)"
  - "Only the exit=0/commits=0 branch is stage-scoped; exit != 0 remains Failed unconditionally for every stage, including Define and Validate"

patterns-established:
  - "Format-discrimination-before-decisive-parse: a per-adapter parser first proves the input actually matches its expected shape (is_codex_event_stream) before returning a decisive result, rather than relying on try-then-fallback alone"

requirements-completed: [13b]

coverage:
  - id: D1
    description: "Claude is_error: true detected as an authoritative Layer-1 failure, including when it overrides a same-envelope success DEVFLOW_RESULT marker; is_error: false still defers to the existing marker path unchanged"
    requirement: "13b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_envelope_is_error_detected"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_is_error_overrides_success_marker"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_envelope_is_error_false_defers"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_envelope_marker_still_wins"
        status: pass
    human_judgment: false
  - id: D2
    description: "Codex --json JSONL event stream parsed: turn.failed is decisive (Failed, reason from error.message); turn.completed with no marker defers to Layer 2 rather than unconditional Success; progress/unparseable lines ignored; a single-document Claude envelope is never consumed by this parser"
    requirement: "13b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#codex_event_stream_parses_turn_failed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#codex_turn_completed_no_marker_defers"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#codex_event_stream_ignores_progress_and_unparseable_lines"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#claude_envelope_not_consumed_by_codex_parser"
        status: pass
      - kind: other
        ref: "git diff --stat crates/devflow-core/Cargo.toml (empty — no new dependency)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Layer 2's exit=0/zero-commits gate is scoped to Stage::Plan/Stage::Code via explicit matches!, so Define/Validate zero-commit exit=0 runs are no longer mis-flagged Failed, while exit != 0 remains Failed for every stage including Define/Validate"
    requirement: "13b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#layer2_skips_commit_gate_for_define_and_validate"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#layer2_nonzero_exit_is_failed_all_stages"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer2_exit_zero_no_commits_is_failed (Code-stage regression check)"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer2_nonzero_exit_is_failed (existing regression check)"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-07-14
status: complete
---

# Phase 13 Plan 03: Native Claude/Codex Completion-Protocol Parsing Summary

**Reads Claude's `is_error`/`num_turns` envelope fields and Codex's `--json` JSONL event stream as authoritative per-adapter completion signals, and scopes Layer 2's commit-count fallback gate to Plan/Code stages so Define/Validate's legitimate zero-commit runs stop being mis-flagged as failures.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-07-14T20:36:00Z
- **Completed:** 2026-07-14T20:48:00Z
- **Tasks:** 3 completed
- **Files modified:** 1

## Accomplishments
- `detect_claude_envelope_failure` reads the Claude JSON envelope's top-level `is_error` boolean (and `num_turns` for observability) and returns an authoritative `Failed` result — checked in `evaluate_layer1` BEFORE the `DEVFLOW_RESULT` marker path, so `is_error: true` overrides a stale/echoed success marker embedded in the same envelope
- `parse_codex_event_result` parses Codex's `--json` JSONL event stream (one JSON object per line): `turn.failed` is decisive (`Failed`, reason from `error.message`); a final `turn.completed` with no marker returns `None` and defers to Layer 2 rather than an unconditional Success — closing the false-pass composition risk flagged by cross-AI review consensus #1
- `is_codex_event_stream` discriminates Codex JSONL from a single-document Claude envelope (requires >=1 `thread.started`/`turn.*` line), so the two formats sharing the same captured-stdout file never cross-consume each other
- `evaluate_layer2` gained a `stage: Stage` parameter; the commit-count "no work done" gate now uses `matches!(stage, Stage::Plan | Stage::Code)` (explicitly, not `Stage::is_agent_stage()`, since that also includes `Define`) — `exit != 0` remains `Failed` unconditionally for every stage
- Full TDD cycle per task: RED (failing test/compile error) -> GREEN (implementation) for all three tasks, verified via git log

## Task Commits

Each task was committed atomically:

1. **Task 1: Read Claude envelope is_error/num_turns as an authoritative failure signal** - `39cfc6a` (test, RED) -> `59af437` (feat, GREEN)
2. **Task 2: Parse the Codex --json JSONL event stream** - `b19a273` (test, RED) -> `db7d4f4` (feat, GREEN)
3. **Task 3: Scope the Layer-2 commit gate to Code-like stages** - `837a7a0` (test, RED) -> `1374941` (feat, GREEN)

**Plan metadata:** (this commit, docs: complete plan)

_Note: All three tasks were TDD — each test commit precedes its feat commit; no refactor commit was needed._

## Files Created/Modified
- `crates/devflow-core/src/agent_result.rs` - Added `detect_claude_envelope_failure`, `is_codex_event_stream`, `parse_codex_event_result`; rewired `evaluate_layer1`'s precedence chain; added a `stage: Stage` parameter to `evaluate_layer2` with an explicit `Stage::Plan | Stage::Code` commit-gate matrix; added 9 new tests; updated 3 pre-existing `evaluate_layer2` call sites (2 tests + 1 production caller) for the new signature

## Decisions Made
- Ordered `is_error` check strictly before the marker path in `evaluate_layer1` (per plan/Codex 13-03 MEDIUM) rather than merging the two checks, keeping each function single-purpose and independently testable
- Did not special-case specific non-success `subtype` string values beyond what `detect_claude_rate_limit` already checks — `is_error` is the sole authoritative signal per RESEARCH Pitfall 5, avoiding brittle enumeration of an undocumented schema
- Added a `stage: Stage` parameter to `evaluate_layer2` (rather than branching inside `evaluate_agent_result` before calling it) so the gate logic is unit-testable in isolation, per the plan's explicit preference
- Left the Codex parser's doc comment carrying the plan's mandated "unverified against installed CLI" note, with no literal a negative-grep gate depends on, per the plan's KNOWN-LIMITATION instruction

## Deviations from Plan

None - plan executed exactly as written. All acceptance criteria (source assertions for `is_error`, `matches!(stage, Stage::Plan | Stage::Code)`, `turn.completed`/`turn.failed` literals; behavior assertions for all named tests; full-crate test/clippy/fmt cleanliness; empty `Cargo.toml` diff) were verified directly.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- The Codex JSONL parser is written against the documented `--json` schema only (RESEARCH Pitfall 4/5) and carries an explicit unverified-schema doc note — Plan 06's dogfood run must capture a real Codex invocation and reconcile any delta, the same empirical practice 12-12-SUMMARY.md used for Claude
- Plan 05's Validate verdict requirement composes with this plan's `turn.completed`-defers rule and Layer 2's stage-scoped gate: a marker-less/verdict-less Validate run cannot silently reach Ship
- No blockers for subsequent 13-mvp-core-loop plans

---
*Phase: 13-mvp-core-loop*
*Completed: 2026-07-14*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/agent_result.rs
- FOUND: .planning/phases/13-mvp-core-loop/13-03-SUMMARY.md
- FOUND: commit 39cfc6a (Task 1 RED)
- FOUND: commit 59af437 (Task 1 GREEN)
- FOUND: commit b19a273 (Task 2 RED)
- FOUND: commit db7d4f4 (Task 2 GREEN)
- FOUND: commit 837a7a0 (Task 3 RED)
- FOUND: commit 1374941 (Task 3 GREEN)
