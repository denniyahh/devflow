---
phase: 51-rate-limit-agent-failover
plan: 03
subsystem: agent-result
tags: [idle-timeout, side-channel, serde-flatten, semver, gate-context, d-15, 999.102]

requires:
  - phase: 31-idle-timeout
    provides: monitor-written idle-timeout side channel (IdleTimeoutRecord, parse_idle_timeout_side_channel)
  - phase: 30-stream-capture
    provides: ParsedCapture and last_top_level_result (top-level-only result selection)
provides:
  - crate-private IdleTimeoutRecordOnDisk (flattened IdleTimeoutRecord + optional terminal_reason/errors/origin_kind)
  - crate-private IdleTimeoutCause and idle_timeout_cause(project_root, phase)
  - idle_timeout_clause, the one place the reason's parenthesised clause is built and capped
  - constants MAX_CAUSE_ERRORS (3), MAX_CAUSE_TEXT_CHARS (160), pub(crate) MAX_IDLE_TIMEOUT_CLAUSE_CHARS (200)
  - private stage_failure_context(stage, reason) in pipeline_outcomes.rs (same text as before)
affects: [51-05, idle-timeout, gate-context, notify-hook]

actuals:
  tokens: 8449
  tasks: 2
  commits: 4
plan_head_before: 22896f598defe08dfb30c029a769c3de11849737
plan_head_after: 1cb54081aa3839e7b944948ac1f05d05fbaf0333

tech-stack:
  added: []
  patterns:
    - "Crate-private #[serde(flatten)] wrapper adds on-disk keys without touching a published all-pub struct (semver R-1)"
    - "Key-set pin test: the flattened object's keys listed as literals, plus disjointness from the inner struct's keys"
    - "Agent-authored text bounded where it is read (count, length, control characters), then capped again where it is rendered"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs

key-decisions:
  - "Clause text: `last result: terminal_reason X; origin Y; errors [e1 | e2 | e3]`. Segments are joined with `; `, errors with ` | `"
  - "A cut field or clause ends in `…` inside its bound (a 160-char field keeps 159 characters plus `…`)"
  - "Field bounds are applied once, in idle_timeout_cause. A record read back from disk is not re-bounded: the clause cap limits what reaches the reason, and render_gate_context strips control characters from the gate context"
  - "Each RED commit carried the minimum scaffolding needed to compile: Task 1 a stub idle_timeout_cause returning no cause; Task 2 the three constants (unused) and the stage_failure_context extraction"

patterns-established:
  - "On-disk record growth for a published type goes through a crate-private flatten wrapper plus a key-set pin"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "The monitor writes terminal_reason, errors and origin_kind from the capture's last top-level result into the idle-timeout record, before the kill"
    requirement: SUPV-02
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::the_idle_timeout_record_carries_the_last_result_cause"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_quota_denial_is_not_recorded_as_an_idle_timeout"
        status: pass
    human_judgment: false
  - id: D2
    description: "The cause comes from the LAST top-level result; a later subagent result never displaces it"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::the_idle_timeout_cause_comes_from_the_last_top_level_result"
        status: pass
    human_judgment: false
  - id: D3
    description: "The idle-timeout reason names the cause in one clause directly after `idle timeout`, short fields first, and the clause survives into the gate context"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::an_idle_timeout_reason_names_the_last_result_cause"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::an_idle_timeout_gate_context_names_the_last_result_cause"
        status: pass
    human_judgment: false
  - id: D4
    description: "With no cause fields, and for a record written before Phase 51, the reason is today's text and does not read as unreadable"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::an_idle_timeout_without_cause_fields_keeps_todays_message"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::a_pre_51_idle_timeout_record_still_parses"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::an_unreadable_idle_timeout_record_still_produces_the_verdict"
        status: pass
    human_judgment: false
  - id: D5
    description: "The wrapper's on-disk key set is pinned and disjoint from IdleTimeoutRecord's keys"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::the_on_disk_idle_timeout_record_key_set_is_pinned"
        status: pass
    human_judgment: false
  - id: D6
    description: "Cause text is bounded (3 errors, 160 chars each, no control characters) and the clause is capped at 200 characters, so it closes inside the 300-char renderings at the maximum bound"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::idle_timeout_cause_text_is_bounded_and_control_free"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::the_idle_timeout_clause_is_capped_at_the_cause_bound"
        status: pass
    human_judgment: false
  - id: D7
    description: "The published IdleTimeoutRecord is unchanged and devflow-core needs no semver bump"
    requirement: SUPV-02
    verification:
      - kind: other
        ref: "cargo semver-checks check-release -p devflow-core (no semver update required, 229 checks pass)"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-11-06
status: complete
---

# Phase 51 Plan 03: Idle-Timeout Cause Clause Summary

**An idle-timeout failure now says why the agent stopped. The monitor reads `terminal_reason`, `errors` and `origin.kind` from the capture's last top-level `result` event and stores them in the record through a crate-private flattened wrapper. The reason then puts them in one bounded clause right after `idle timeout`, which still fits inside the 300-character gate context. The published `IdleTimeoutRecord` is unchanged.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-11-06T16:09:09Z
- **Completed:** 2026-11-06T16:21:15Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Backlog 999.102 (D-15) is folded in. With the measured cause, a reason now opens `idle timeout (last result: terminal_reason aborted_tools; origin task-notification; errors [[ede_diagnostic] …]): the agent's output stream was silent …`.
- When no cause is present (an older CLI, or a child killed before any `result`), the text is byte-identical to the old message. Records written before Phase 51 parse as before.
- Agent-authored text is bounded where it is read. The clause is capped at 200 characters, so even the maximum-bound cause closes inside both `truncate_reason` and the pending-gate banner's 300-character rendering.
- semver: `IdleTimeoutRecord` is byte-identical to `ce08a9d` (checked by extracting the struct from both revisions; both extractions were 16 lines, then `cmp`). `cargo semver-checks` reports no update required.

## Task Commits

1. **Task 1 (tracer) RED:** `ffdc9ea` (test). Four tests, with a stub cause reader.
2. **Task 1 GREEN:** `8254fbf` (feat). Adds the wrapper and the cause reader, and wires the clause into the reason and the monitor.
3. **Task 2 RED:** `446943a` (test). Bounds, key-set pin, clause cap, gate-context test, and the `stage_failure_context` extraction.
4. **Task 2 GREEN:** `1cb5408` (feat). Applies the field bounds and the clause cap.

Commits measured from the ledger: 4 (`22896f5..1cb5408`).

## Files Created/Modified
- `crates/devflow-core/src/agent_result.rs`: adds `IdleTimeoutCause`, `IdleTimeoutRecordOnDisk`, `idle_timeout_cause`, `idle_timeout_clause`, `bound_text` and the three constants. `parse_idle_timeout_side_channel` now reads the wrapper. Adds 8 tests.
- `crates/devflow-core/src/monitor.rs`: `fire_idle_timeout` reads the cause once, after the denial check and before writing. `write_idle_timeout_record` takes the cause and serializes the wrapper. Adds 1 test.
- `crates/devflow-cli/src/pipeline_outcomes.rs`: `stage_failure_context` is extracted from `handle_stage_failure` with its text unchanged. Adds 1 test.

## Verification

| Check | Result |
|-------|--------|
| Task 1 `<automated>` gate (5 nextest targets + semver-checks) | exit 0; each target `1 passed`; semver "no semver update required" |
| Task 2 `<automated>` gate (6 nextest targets + workspace clippy `-D warnings`) | exit 0 |
| `cargo test -p devflow-core --lib` | 1128 passed, 0 failed, 2 ignored (51-02 left 1120; this plan adds 8 core tests) |
| `cargo test -p devflow --bin devflow` | 647 passed, 0 failed, 2 ignored |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo semver-checks check-release -p devflow-core` (final tree) | no semver update required |
| Task 1 acceptance: no second `type == "result"` selector | the GREEN diff adds no `"result"` literal; `idle_timeout_cause` calls `last_top_level_result` |
| Task 2 acceptance: only production change in `pipeline_outcomes.rs` is the extraction | one non-test hunk (`@@ -913`), which is the extraction |

**Limits: what these checks do not establish**
- The monitor test feeds the cause from a stub's `result` event. It does not prove that a real Claude CLI still emits `terminal_reason`, `errors` and `origin.kind` under those names. The key names come from the measured 999.102 capture.
- The 300-character fit is proven for `truncate_reason` and for the pending-gate banner, which renders at 300. `gate list`-style views render the context at 100 characters (`commands.rs` ~L2031, ~L2087). There, `[never-silent] stage validate failed: idle timeout (` alone takes 52 characters, so the clause is cut. The plan's contract does not cover that view.
- I did not re-probe semver-checks' claim that adding a field to `IdleTimeoutRecord` would be a major break. That comes from RESEARCH Pitfall 4. This run only shows the unchanged struct passes.

## TDD Gate Compliance

The order is RED `ffdc9ea`, GREEN `8254fbf`, RED `446943a`, GREEN `1cb5408`. Each GREEN descends from its RED.

- **RED (Task 1):** three targets failed on their planned assertions, each exiting 100:
  - the monitor record had no `"terminal_reason"` key;
  - the selection test got `None` where it expected `Some("late")`;
  - the reason began `idle timeout:` rather than `idle timeout (`.

  For each, the nextest JUnit report went through `gsd-tools check tdd-red-evidence`, which returned `RED_EVIDENCE_OK` (`target_test_failed`). The records are `target/red-evidence/51-03-t1-{monitor,selection,reason}.json` (gitignored). Semantic assessment: each target ran and failed on the behavior under test. None was a build, load or discovery fault.

  The fourth test, `an_idle_timeout_without_cause_fields_keeps_todays_message`, passed in RED by design: it pins today's text. As a negative control on the classifier, its passing record was submitted and refused as `INVALID_RED` (`unexpected_green`).
- **RED (Task 2):** three targets failed on their planned assertions:
  - the bound test kept `10 errors`;
  - the clause-cap test found a `855`-character clause;
  - the CLI max-bound case reported that `truncate_reason` cut the clause before it closed.

  All three records returned `RED_EVIDENCE_OK`: `target/red-evidence/51-03-t2-{bound,clause,gate}.json`. `a_pre_51_idle_timeout_record_still_parses` passed on Task 1's code, as the plan expected. Its in-test control is a non-numeric `idle_secs`, which must read as unreadable, and that control held.
- **Key-set pin, RED-equivalent:** the pin passed on Task 1's code. I set `origin_kind`'s serde key to `rename = "status"`, a collision with `IdleTimeoutRecord`. The pin then failed with exit 100: `the on-disk idle-timeout key set changed`, with `origin_kind` missing from the left set. I restored the file from a backup, confirmed it with `cmp`, and the pin passed again.
- **REFACTOR:** none.

## Decisions Made
- Clause format: segments are joined with `; `, and errors with ` | ` inside `errors [..]`. A cut ends in `…` inside the bound.
- Bounds are applied once, in the reader. A hand-edited record on disk is not re-bounded. The clause cap still limits what reaches the reason, and `render_gate_context` strips control characters from the gate context.
- No checkpoints. The tracer gate (interactive, `end-of-phase`, `<automated>` only) re-ran Task 1's verify, which passed, before Task 2 started.

## Deviations from Plan

**1. [Rule 3 - Blocking] RED commits carried compile scaffolding**
- **Found during:** Task 1 RED and Task 2 RED.
- **Issue:** Tests that name functions or constants which do not exist yet fail to compile. The classifier rejects that as `build_failure`, not as a valid RED.
- **Fix:** Task 1's RED added a stub `idle_timeout_cause` that returns no cause. Task 2's RED declared the three constants (still unused) and extracted `stage_failure_context`, which is a text-preserving refactor the CLI test needs. Each RED commit built with dead-code warnings only; clippy is clean at both GREEN commits.
- **Committed in:** `ffdc9ea`, `446943a`.

**2. [Rule 1 - Convention] Commit subject over 72 characters**
- **Found during:** Task 2 RED.
- **Issue:** The `commit-msg` hook warned that the subject was 73 characters.
- **Fix:** I amended my own unpushed commit's message to a 66-character subject. The content is unchanged.

**Total deviations:** 2. Both are process-level; there is no behavior change beyond the plan. **Impact:** none on scope.

## Issues Encountered
- I found two pre-existing misplaced doc comments and did not touch them:
  - The doc for `handle_stage_failure` sits above `truncate_reason` in `pipeline_outcomes.rs`.
  - The long doc for `parse_idle_timeout_side_channel` sits above `capture_shows_rate_limit_denial` in `agent_result.rs`, so rustdoc attaches each to the wrong item.
- No DevFlow defect was hit during this run, so there is no 999.x candidate.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 51-05 can prepend its torn-final-line segment in `idle_timeout_clause` under the same 200-character cap. It should add `capture_torn_final_line` to the key-set pin in `the_on_disk_idle_timeout_record_key_set_is_pinned` and add the field to `IdleTimeoutRecordOnDisk`.

## Self-Check: PASSED
- Modified files exist: `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/monitor.rs`, `crates/devflow-cli/src/pipeline_outcomes.rs`.
- Commits `ffdc9ea`, `8254fbf`, `446943a`, `1cb5408` are ancestors of HEAD (checked with `git merge-base --is-ancestor`).

---
*Phase: 51-rate-limit-agent-failover*
*Completed: 2026-11-06*
