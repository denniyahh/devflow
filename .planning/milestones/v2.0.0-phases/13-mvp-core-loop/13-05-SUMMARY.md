---
phase: 13-mvp-core-loop
plan: 05
subsystem: agent-completion-protocol
tags: [verdict, validate, advance, agent-result, prompt]
dependency-graph:
  requires: [13-01, 13-02, 13-03, 13-04]
  provides:
    - "Verdict enum (Pass/Gaps) on AgentResult"
    - "verdict-aware advance() Validate arm"
  affects:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-cli/src/main.rs
tech-stack:
  added: []
  patterns:
    - "lenient custom serde deserializer (deserialize_with) so a malformed
      optional field never fails the whole struct parse"
    - "gate-context busy-poll test pattern (thread::scope + poll for gate
      file, read context, then unblock) to test advance()'s stage dispatch
      without spawning a real agent process"
key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-cli/src/main.rs
decisions:
  - "Verdict deserializer matches only the exact lowercase wire-format
    strings (\"pass\"/\"gaps\") — a mis-cased value like \"Pass\" is NOT
    case-folded into a match; it maps to None the same as an unknown value,
    per the plan's explicit test contract (fail-safe over convenience)."
  - "Forced every new advance()-level Validate test through a gate (context
    text is the only observable signal of the passed value) rather than
    letting a passed=true case fall through to a bare transition(), since
    transition()/loop_back_to_code() both call launch_stage() which spawns
    the real configured agent CLI — must never fire from a unit test."
  - "Removed Validate from non_ship_stage_prompts_are_unchanged_single_command_template
    (renamed to non_ship_non_validate_...) since Validate now has its own
    dedicated prompt function, mirroring how Ship was already excluded."
metrics:
  duration: 15min
  completed: 2026-07-14
status: complete
---

# Phase 13 Plan 05: Verdict-vs-Ran Split for the Validate Stage Summary

Added a distinct `verdict` field (`pass`/`gaps`) to the DEVFLOW_RESULT contract, parsed it
through a lenient exact-case deserializer, made the Validate-stage prompt require it, and
made `advance()`'s Validate arm advance to Ship ONLY on an explicit `verdict: pass` — closing
the composition bug where a marker-less or gaps-finding Validate run could otherwise reach
Ship.

## What Was Built

**Task 1 — `Verdict` field on the DEVFLOW_RESULT contract** (`crates/devflow-core/src/agent_result.rs`):
- `Verdict` enum (`Pass`, `Gaps`) with `#[serde(rename_all = "lowercase")]`, mirroring `AgentStatus`.
- `AgentResult.verdict: Option<Verdict>` annotated `#[serde(default, deserialize_with = "deserialize_verdict_lenient")]`.
- `deserialize_verdict_lenient`: deserializes an `Option<String>` and maps only the exact
  strings `"pass"`/`"gaps"` to `Some(Verdict)`; anything else — absent, unknown (`"wat"`), or
  mis-cased (`"Pass"`) — maps to `Ok(None)`, never an error. This is the critical fix: with
  derived `serde` on `Option<Verdict>`, a bad value would fail `from_str::<AgentResult>` for
  the ENTIRE marker, silently dropping a valid `status` to Layer 2.
- Updated all 5 `AgentResult { .. }` struct literals in `agent_result.rs` plus the
  `run_agent_blocking` literal in `crates/devflow-cli/src/main.rs` to set `verdict: None`.
- New tests: `parse_devflow_result_reads_verdict`, `parse_devflow_result_reads_verdict_pass`,
  `parse_devflow_result_verdict_absent_is_none`,
  `parse_devflow_result_malformed_verdict_is_none_not_parse_error` (asserts BOTH `"wat"` and
  the mis-cased `"Pass"` parse successfully to `status: Success, verdict: None`).

**Task 2 — Validate prompt requires a verdict; `advance()` consults it** (`crates/devflow-core/src/prompt.rs`, `crates/devflow-cli/src/main.rs`):
- New `validate_stage_prompt()`, special-cased in `stage_prompt()` alongside the existing
  Ship special-case. Instructs the agent that its FINAL message must be exactly
  `DEVFLOW_RESULT: {"status": "success", "verdict": "pass"}` (no gaps) or
  `{"status": "success", "verdict": "gaps"}` (gaps found) — distinct from `status`.
- `advance()`'s Validate success arm now computes
  `let passed = matches!(result.verdict, Some(Verdict::Pass));` and passes that into
  `handle_validate_outcome`, instead of unconditionally passing `true`. Only an explicit
  `verdict: pass` advances; `Some(Gaps)` or `None` both gate/loop back to Code.
- New tests: `validate_gaps_does_not_advance_to_ship`, `validate_missing_verdict_does_not_advance`,
  `validate_pass_advances` (via a shared `drive_validate_advance_and_read_gate_context` helper
  that forces the Validate gate open and asserts on its `context` text — the only externally
  observable signal of the computed `passed` value — without ever calling `launch_stage`), and
  `validate_stage_prompt_requires_verdict` (prompt-level assertion).
- Renamed `non_ship_stage_prompts_are_unchanged_single_command_template` to
  `non_ship_non_validate_stage_prompts_are_unchanged_single_command_template` and removed
  Validate from its cases, since Validate now has its own dedicated prompt function (mirrors
  how Ship was already excluded).

## Verification

- `cargo build --workspace` — exits 0
- `cargo test --workspace` — 165 devflow-core unit tests + 2 monitor e2e tests + 21 devflow
  unit tests + 8 phase7_cli integration tests, all pass (0 failures)
- `cargo clippy --workspace -- -D warnings` — exits 0
- `cargo fmt --check` — exits 0
- Source assertion: `rg "matches!(result.verdict, Some(Verdict::Pass))" crates/devflow-cli/src/main.rs` — found at `main.rs:414`
- `rg -c "verdict: None" crates/devflow-cli/src/main.rs` — 1 (run_agent_blocking literal updated)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Deserializer contract clarified: exact-case match, not case-folding**

- **Found during:** Task 1, writing `parse_devflow_result_malformed_verdict_is_none_not_parse_error`
- **Issue:** The plan's action prose said the deserializer "lowercases" the raw string before
  matching, which would make the mis-cased `"Pass"` match `"pass"` and deserialize to
  `Some(Verdict::Pass)`. But the plan's own `<behavior>` spec and the test itself require
  `"Pass"` to parse to `verdict: None` (same as an unknown value) — a genuine internal
  contradiction in the plan text. The explicit, testable behavior spec is authoritative.
- **Fix:** Implemented exact-case matching (`s.as_str()` against literal `"pass"`/`"gaps"`,
  no `to_ascii_lowercase()`) so only the canonical wire-format strings match; any mis-cased or
  unknown value — including a value that would trivially case-fold to a valid one — maps to
  `None`. This is the stricter, more fail-safe reading and matches the named test's assertions.
- **Files modified:** `crates/devflow-core/src/agent_result.rs`
- **Commit:** `1cc8065`

**2. [Rule 1 - Test accuracy] Excluded Validate from the "unchanged single command template" prompt test**

- **Found during:** Task 2, after adding `validate_stage_prompt`
- **Issue:** `non_ship_stage_prompts_are_unchanged_single_command_template` included
  `Stage::Validate` in its cases and asserted the stage's prompt is the plain, unmodified
  single-command template. Once Validate got its own dedicated (extended) prompt function,
  that claim was no longer accurate for Validate, even though the loose assertions (contains
  command, contains DEVFLOW_RESULT, no code-review text) still happened to pass.
- **Fix:** Removed `Stage::Validate` from the cases list, renamed the test to
  `non_ship_non_validate_stage_prompts_are_unchanged_single_command_template`, and added a
  dedicated `validate_stage_prompt_requires_verdict` test asserting the new verdict
  requirement. This matches how `Stage::Ship` was already excluded from this test for the
  same reason (Plan 02's Ship special-case), and the plan's own review_findings entry
  anticipated this ("prompt tests guard no regression").
- **Files modified:** `crates/devflow-core/src/prompt.rs`
- **Commit:** `8345bb8`

No other deviations — plan executed as written otherwise.

## Known Stubs

None.

## Threat Flags

None — this plan's threat model (T-13-13, T-13-14) was fully implemented as specified; no
new unscoped surface was introduced.

## Self-Check: PASSED

- `crates/devflow-core/src/agent_result.rs` — FOUND (modified, contains `Verdict` enum)
- `crates/devflow-core/src/prompt.rs` — FOUND (modified, contains `validate_stage_prompt`)
- `crates/devflow-cli/src/main.rs` — FOUND (modified, contains verdict-aware `advance()`)
- Commit `1cc8065` — FOUND in `git log --oneline --all`
- Commit `8345bb8` — FOUND in `git log --oneline --all`
