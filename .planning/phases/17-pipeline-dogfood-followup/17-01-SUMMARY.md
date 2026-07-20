---
phase: 17-pipeline-dogfood-followup
plan: 01
subsystem: infra
tags: [rust, serde, completion-detection, state-machine, outcome-policy]

# Dependency graph
requires:
  - phase: 16-pipeline-reliability-hardening
    provides: four-layer AgentResult evaluation cascade (Layer 0-3), events.jsonl
provides:
  - AgentStatus::ResourceKilled (exit 137) and AgentStatus::AgentUnavailable (exit 127) variants with word-boundary-preserving wire names
  - AgentStatus::as_wire_str() — exhaustive, serde-pinned wire-name accessor
  - AgentResult.decided_by_layer: Option<u8> set by every Layer 0/1/2/3 constructor workspace-wide
  - outcome_policy::decide_action(stage, outcome) -> Action — pure, exhaustive outcome->action policy table
  - State.infra_failures: u32 (separate from consecutive_failures) + mode::MAX_INFRA_FAILURES = 5
affects: [17-02, 17-03, 17-04, 17-05, phase-18-hermes-support (18d doctor reconciliation)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Exhaustive match with no wildcard arm as a compile-time policy guard (decide_action, as_wire_str) — adding a variant is a compile error until its behavior is declared"
    - "decided_by_layer: Option<u8> tags which cascade layer produced an AgentResult, for future reconciliation tooling"

key-files:
  created:
    - crates/devflow-core/src/outcome_policy.rs
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/mode.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/log_format_env.rs

key-decisions:
  - "MAX_INFRA_FAILURES = 5, more lenient than MAX_CONSECUTIVE_FAILURES = 3, since infra faults (OOM/missing binary) are not the agent's fault"
  - "decide_action maps Failed and Unknown identically to GateReview (deferred distinction — decided_by_layer + the AgentStatus variant itself preserve the information for Phase 18's 18d reconciliation)"
  - "Marker-parsed AgentResult (via serde deserialize, not a literal) is not retrofitted with decided_by_layer — only the constructor literals enumerated in the plan carry it; left as serde-default None"

patterns-established:
  - "Pure policy functions with exhaustive matches (no wildcard) as the fail-closed mechanism against a new/unhandled enum variant silently taking the wrong path"

requirements-completed: [17b]

coverage:
  - id: D1
    description: "AgentStatus gains ResourceKilled/AgentUnavailable variants with word-boundary-preserving wire names, plus as_wire_str() pinned to the serde form for every variant"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#multi_word_variants_serialize_with_word_boundary"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#as_wire_str_matches_serde_form_for_every_variant"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#existing_variants_keep_wire_form"
        status: pass
    human_judgment: false
  - id: D2
    description: "evaluate_layer2 classifies exit 137 as ResourceKilled and exit 127 as AgentUnavailable, existing exit 0/1 behavior unchanged"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer2_exit_137_is_resource_killed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer2_exit_127_is_agent_unavailable"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer2_exit_0_zero_commits_still_failed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer2_exit_1_still_failed"
        status: pass
    human_judgment: false
  - id: D3
    description: "AgentResult.decided_by_layer field added, every construction site workspace-wide (including the cross-crate main.rs:1551 literal) updated so the workspace compiles"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "cargo build -p devflow"
        status: pass
      - kind: integration
        ref: "cargo test -p devflow-core agent_result::"
        status: pass
    human_judgment: false
  - id: D4
    description: "outcome_policy::decide_action is a pure, exhaustive function mapping all six AgentStatus outcomes to an Action, with Unknown never mapping to Advance"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/outcome_policy.rs#success_advances"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/outcome_policy.rs#unknown_gates_review_never_advances"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/outcome_policy.rs#decide_action_is_deterministic"
        status: pass
    human_judgment: false
  - id: D5
    description: "State.infra_failures added (distinct from consecutive_failures), defaults to 0, round-trips through serde, and mode::MAX_INFRA_FAILURES constant exists"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#infra_failures_round_trips_through_serde"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#infra_failures_absent_from_json_defaults_to_zero"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-07-18
status: complete
---

# Phase 17 Plan 01: Typed-Outcome Taxonomy + Fail-Closed Policy Table Summary

**Two new agent outcomes (ResourceKilled/AgentUnavailable) with a serde-pinned wire-name accessor, plus a pure exhaustive outcome->action policy table (`outcome_policy::decide_action`) and a separate infra-failure counter on `State`.**

## Performance

- **Duration:** 15 min
- **Started:** 2026-07-18T19:20:00-04:00 (approx.)
- **Completed:** 2026-07-18T19:30:52-04:00
- **Tasks:** 2
- **Files modified:** 6 (1 created)

## Accomplishments
- `AgentStatus` gained `ResourceKilled` (exit 137) and `AgentUnavailable` (exit 127) variants with explicit `#[serde(rename = ...)]` so their wire form keeps the underscore word boundary (`"resource_killed"`, `"agent_unavailable"`) instead of collapsing under the enum's blanket `rename_all = "lowercase"`.
- Added `AgentStatus::as_wire_str()` — an exhaustive match with no wildcard arm, pinned equal to `serde_json::to_string(&variant)` (quotes stripped) for all six variants via a single round-trip test. This is the sanctioned replacement for `format!("{:?}", status).to_ascii_lowercase()` that Plan 04 will use at the `advance_evaluated` emit.
- `evaluate_layer2` now classifies exit 137 and 127 before the generic `exit_code != 0 -> Failed` catch-all, reusing the already-parsed plain `i32` (no `ExitStatusExt`/signal API). Exit 0 and other non-zero codes keep their existing classification, verified by regression tests.
- Added `AgentResult.decided_by_layer: Option<u8>` and updated every construction site in the workspace (Layer 0/1/2/3 literals in `agent_result.rs` plus the cross-crate literal at `main.rs`'s sequentagent Layer-1 exit-code fallback) so the workspace compiles.
- Created `crates/devflow-core/src/outcome_policy.rs` with `Action` (`Advance`/`AutoResume`/`GateInfra`/`GateReview`) and `decide_action(stage, outcome) -> Action`: a pure, I/O-free, exhaustively-matched function. `Success->Advance`, `RateLimited->AutoResume`, `ResourceKilled|AgentUnavailable->GateInfra`, `Failed|Unknown->GateReview` — `Unknown` never maps to `Advance` (D-01), enforced by a dedicated test and by the match's lack of a wildcard arm.
- Added `State.infra_failures: u32` (serde default 0, distinct from `consecutive_failures`) and `mode::MAX_INFRA_FAILURES = 5`, with a doc comment explaining why the infra ceiling is more lenient than the 3-failure Validate-gate ceiling.

## Task Commits

1. **Task 1: Add ResourceKilled/AgentUnavailable outcomes + as_wire_str + Layer 2 exit-code classification + main.rs literal fix** - `ccce72e` (feat)
2. **Task 2: Pure outcome->action policy module + separate infra counter** - `68a1b00` (feat)

_Note: no TDD RED/GREEN split commits — `tdd="true"` tasks were executed with tests and implementation delivered together per task, matching this plan's per-task (not plan-level) TDD gate scope._

## Files Created/Modified
- `crates/devflow-core/src/outcome_policy.rs` - new module: `Action` enum + pure `decide_action`
- `crates/devflow-core/src/agent_result.rs` - two new `AgentStatus` variants, `as_wire_str()`, `decided_by_layer` field + every constructor, Layer 2 137/127 classification, new unit tests
- `crates/devflow-core/src/state.rs` - `infra_failures: u32` field, `State::new` init, round-trip + absent-default tests
- `crates/devflow-core/src/mode.rs` - `MAX_INFRA_FAILURES = 5` constant with rationale doc comment
- `crates/devflow-core/src/lib.rs` - `pub mod outcome_policy;` registration
- `crates/devflow-cli/src/main.rs` - sequentagent's Layer-1 exit-code fallback literal updated with `decided_by_layer: Some(2)`
- `crates/devflow-cli/tests/log_format_env.rs` - legacy `State {}` test fixture updated with `infra_failures: 0` (compile fix, see Deviations)

## Decisions Made
- `MAX_INFRA_FAILURES = 5` chosen as Claude's-discretion per the plan: bounds a stuck OOM/rate-limit-adjacent infra loop to at most 5 unobserved cycles while tolerating transient cloud outages that the stricter 3-failure ceiling would abort prematurely. Documented in the constant's doc comment (resolves the plan's noted LOW review item on this rationale).
- Marker-parsed `AgentResult` values (produced via `serde_json::from_str::<AgentResult>` inside `parse_marker_lines`, not a struct literal) were left with `decided_by_layer` at its serde default (`None`) rather than force-tagged — the plan's action text enumerates specific constructor literals to update (Layer 0/1/2/3 functions and `rate_limited_result`) and does not ask for the marker-deserialize path to be wrapped; doing so would be undocumented scope expansion beyond the plan's explicit instructions.
- `decide_action` intentionally keeps `Failed` and `Unknown` both mapping to `GateReview` — already the plan's own documented resolution (see Review dispositions in the plan), not a new deviation.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed `log_format_env.rs`'s legacy `State {}` literal broken by the new `infra_failures` field**
- **Found during:** Task 2 (adding `State.infra_failures`)
- **Issue:** `crates/devflow-cli/tests/log_format_env.rs` constructs a `State` struct literal directly (to simulate a legacy `.devflow/state.json`) rather than via `State::new`. Adding a new non-defaulted field to the `State` struct broke this literal's compilation — `cargo build --workspace --tests` failed.
- **Fix:** Added `infra_failures: 0` to the literal, consistent with `State::new`'s own initialization and the field's documented serde default.
- **Files modified:** `crates/devflow-cli/tests/log_format_env.rs`
- **Verification:** `cargo build --workspace --tests` and `cargo test -p devflow --test log_format_env` (3 tests) both pass.
- **Committed in:** `68a1b00` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking compile fix)
**Impact on plan:** Necessary consequence of Task 2's `State` field addition — mirrors the plan's own explicitly-called-out `main.rs:1551` fix for Task 1's `AgentResult` field addition, same class of issue (a struct literal outside the `files_modified` list breaks when a new required field is added). No scope creep.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- The typed-outcome taxonomy (`ResourceKilled`/`AgentUnavailable`), `as_wire_str()`, `decided_by_layer`, `outcome_policy::decide_action`, and `State.infra_failures`/`MAX_INFRA_FAILURES` are all in place and unit-tested, ready for Plan 03 (cascade rework) and Plan 04 (advance dispatch) to consume.
- Plan 04 must use `AgentStatus::as_wire_str()` at the `advance_evaluated` emit (`main.rs:848`) instead of the existing `format!("{:?}", result.status).to_ascii_lowercase()` — that call site was deliberately left untouched by this plan per the plan's own scope boundary (review consensus #1/#3 split).
- No blockers.

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-18*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/outcome_policy.rs
- FOUND: .planning/phases/17-pipeline-dogfood-followup/17-01-SUMMARY.md
- FOUND: commit ccce72e (Task 1)
- FOUND: commit 68a1b00 (Task 2)
- FOUND: commit f8c6403 (SUMMARY)
