---
phase: 51-rate-limit-agent-failover
plan: 05
subsystem: agent-result
status: complete
tags: [rate-limit, torn-line, provenance, idle-timeout, d-16, 999.71, t-30-12, review-r2-3]

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-03: IdleTimeoutRecordOnDisk, idle_timeout_clause, MAX_IDLE_TIMEOUT_CLAUSE_CHARS, the on-disk key-set pin"
  - phase: 30-stream-capture
    provides: "ParsedCapture, torn_json_after_last_matching, is_top_level"
provides:
  - "crate-private CaptureDenial { Denied, NotDenied, TornAfterLastEvent } and capture_rate_limit_denial(project_root, phase)"
  - "detect_claude_stream_rate_limit selects its turn boundary and its rate_limit_event among top-level events only (is_top_level)"
  - "on-disk key capture_torn_final_line (written only when true), pinned in the_on_disk_idle_timeout_record_key_set_is_pinned"
  - "the torn segment `torn final line, a quota denial cannot be ruled out` as the first segment of the idle-timeout clause"
  - "fire_idle_timeout: Denied suppresses, TornAfterLastEvent writes the flagged record and logs `[idle-timeout] … torn final line …`, NotDenied writes as before"
affects: [51-01 failover trigger, idle-timeout, gate-context]

actuals:
  tokens: 7760
  tasks: 2
  commits: 4
plan_head_before: 54eeda3105b7f8aa02fc1eb02ccdf56b3fda3e48
plan_head_after: ab00f220323336d275892e7287138a810a00daac

tech-stack:
  added: []
  patterns:
    - "A crate-private three-way answer under an unchanged published boolean (`matches!(.., Denied)`)"
    - "Case-table tests that collect every wrong case before asserting, so RED names each failing case"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/monitor.rs

key-decisions:
  - "The read side re-checks the capture on every idle-timeout read: the torn segment is added when the record carries capture_torn_final_line OR the capture now answers TornAfterLastEvent, because a killed child's partial line is teed at EOF, after the monitor's check"
  - "The torn segment is the fixed 51-character text `torn final line, a quota denial cannot be ruled out`, held in TORN_FINAL_LINE_SEGMENT and passed to idle_timeout_clause as the first segment; no second clause exists"
  - "write_idle_timeout_record takes the flag as a seventh parameter; IdleTimeoutRecordOnDisk::new keeps its signature and defaults the flag to false"
  - "The provenance fix reuses is_top_level through one local closure that pairs it with the type check; no second provenance predicate"

patterns-established:
  - "A torn tail is its own answer, never folded into 'not denied' and never promoted to 'denied'"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "Only top-level rate_limit_event and result events decide a denial, for the detector and for Layer 1 (R2-3, T-30-12)"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::only_top_level_rate_limit_events_and_results_decide_a_denial"
        status: pass
    human_judgment: false
  - id: D2
    description: "A capture ending in a torn line answers TornAfterLastEvent; its idle-timeout reason opens with the torn segment and stays IdleTimeout, never RateLimited; an intact denial is still Denied and an intact non-denial keeps today's text"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::a_torn_final_line_is_not_read_as_a_clean_silence"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::a_quota_denial_in_the_capture_is_visible_to_the_monitor"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::an_ordinary_capture_is_not_mistaken_for_a_quota_denial"
        status: pass
    human_judgment: false
  - id: D3
    description: "At the maximum-bound cause the torn segment still leads the clause, the clause stays within 200 characters and closes within the first 217 characters of the reason"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::a_torn_clause_leads_the_capped_clause_at_the_cause_bound"
        status: pass
    human_judgment: false
  - id: D4
    description: "capture_torn_final_line is pinned in the on-disk key set and absent when false"
    requirement: SUPV-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::the_on_disk_idle_timeout_record_key_set_is_pinned"
        status: pass
    human_judgment: false
  - id: D5
    description: "The monitor writes and flags the record for a torn tail and logs it; an intact capture writes no flag; a denial is still suppressed"
    requirement: SUPV-02
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_torn_final_line_at_the_idle_timeout_is_flagged_on_the_record"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_quota_denial_is_not_recorded_as_an_idle_timeout"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::the_idle_timeout_record_carries_the_last_result_cause"
        status: pass
    human_judgment: false
  - id: D6
    description: "capture_shows_rate_limit_denial keeps its signature; devflow-core needs no semver bump"
    requirement: SUPV-02
    verification:
      - kind: other
        ref: "cargo semver-checks check-release -p devflow-core (no semver update required, 229 checks pass)"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-11-07
---

# Phase 51 Plan 05: Torn Final Line and Top-Level Denials Summary

**A capture that ends in a torn JSON line is no longer read as a clean silence. The rate-limit check now has a third answer, "torn after the last event". The monitor writes and flags the idle-timeout record for that answer. The reason then opens with `idle timeout (torn final line, a quota denial cannot be ruled out…`. Separately, only top-level events now decide a quota denial: a subagent's `rate_limit_event` or `result` can no longer classify a stage `RateLimited` and so cannot trigger 51-01's failover.**

## Performance

- **Duration:** about 15 min
- **Started:** 2026-11-07T21:30:55Z
- **Completed:** 2026-11-07T21:46Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- **Provenance, review R2-3.** `detect_claude_stream_rate_limit` now picks the turn boundary and the deciding `rate_limit_event` among top-level events only, through `is_top_level`. Before this change, three cases went wrong:
  - a subagent's `rejected` event counted as a denial;
  - a later subagent `allowed` event masked a top-level `rejected` one;
  - a subagent `result` moved the final-turn boundary and hid a denial.

  Layer 1 and the monitor both call this function, so both changed.
- **The classifier half of D-16 / 999.71.** `capture_rate_limit_denial` returns one of three answers from a single parse: `Denied`, `NotDenied` or `TornAfterLastEvent`. The published `capture_shows_rate_limit_denial` keeps its signature and is now `matches!(.., Denied)`.
- **Read side.** The idle-timeout reason adds the torn segment as the first segment of 51-03's capped clause in two cases:
  - the record carries `capture_torn_final_line`;
  - the capture reads torn when the advance reads it. A killed child's partial line is teed at EOF, after the monitor has already checked.

  The verdict stays `IdleTimeout`.
- **Monitor side.** `fire_idle_timeout` handles each answer:
  - `Denied` suppresses the record, as before;
  - `TornAfterLastEvent` writes the record with the flag, warns, and logs `[idle-timeout] after Ns the capture ends in a torn final line; …`;
  - `NotDenied` writes the record as before.

  The kill still comes after the write.

## Task Commits

1. **Task 1 (tracer) RED:** `4e7aa14` test(51-05): add failing tests for torn tails and top-level denials
2. **Task 1 GREEN:** `8e0955e` feat(51-05): read torn tails and decide denials on top-level events
3. **Task 2 RED:** `02b0402` test(51-05): add failing test for the torn flag on the idle record
4. **Task 2 GREEN:** `ab00f22` feat(51-05): flag a torn final line on the idle-timeout record

Commits measured from the ledger: 4 (`54eeda3..ab00f22`).

## Files Created/Modified
- `crates/devflow-core/src/agent_result.rs`:
  - the top-level-only selection in `detect_claude_stream_rate_limit`;
  - `CaptureDenial`, `capture_rate_limit_denial` and `TORN_FINAL_LINE_SEGMENT`;
  - the `capture_torn_final_line` field on `IdleTimeoutRecordOnDisk`;
  - the torn segment in `parse_idle_timeout_side_channel`;
  - three new tests and the extended key-set pin.
- `crates/devflow-core/src/monitor.rs`:
  - `fire_idle_timeout` uses the three-way answer;
  - `write_idle_timeout_record` takes the flag;
  - one new test.

## Verification

| Check | Result |
|-------|--------|
| Task 1 `<automated>` gate (6 nextest targets + `cargo semver-checks check-release -p devflow-core`) | exit 0; each target `1 passed`; semver "no semver update required" |
| Task 2 `<automated>` gate (3 nextest targets + `cargo clippy --workspace --all-targets -- -D warnings`) | exit 0, gated on exit code alone |
| `cargo test -p devflow-core --lib` | 1132 passed, 0 failed, 2 ignored (1128 after 51-03, plus 4 new) |
| `scripts/check.sh all` (fmt, workspace clippy, `cargo test --workspace`, including doc_check) | exit 0; the CLI binary ran 658 passed, 0 failed |
| `cargo semver-checks check-release -p devflow-core` on the final tree | no semver update required (229 pass) |
| Existing rate-limit tests after the provenance change | all green, none changed; no existing fixture carries a subagent event |
| Acceptance: `fire_idle_timeout` no longer calls `capture_shows_rate_limit_denial` | `rg` finds only `capture_rate_limit_denial` at the call site; the remaining hit is a test doc comment |
| Acceptance: the torn text sits in one place in the reason | `a_torn_final_line_is_not_read_as_a_clean_silence` asserts it occurs exactly once |

**What these checks do not establish**
- How often a torn final line occurs in production. Operator ruling R-4 returned that measurement to backlog 999.71, and nothing here measures it.
- The torn clause changes the wording of the verdict and adds a flag to the record. It does not change routing: an idle timeout is still a gate, never a resume or a handoff.
- The monitor test produces its torn line with a stub that prints a complete but malformed line. No test covers the real tear source: a child killed mid-line, whose partial line is teed at EOF. The read-side tests cover that path only by writing the torn capture directly.
- The provenance cases use synthetic events with `parent_tool_use_id` set by hand. No archived real capture contains a subagent `rate_limit_event`. The rule that an absent key counts as top-level is still unproven, as `is_top_level`'s doc already records.

## TDD Gate Compliance

The commit order is RED `4e7aa14` → GREEN `8e0955e` → RED `02b0402` → GREEN `ab00f22`. Each GREEN descends from its RED. There was no REFACTOR.

RED evidence came from nextest JUnit reports, written by a scratch `red` profile and passed through `gsd-tools check tdd-red-evidence`. Every record returned `RED_EVIDENCE_OK` (`target_test_failed`). The records are `target/red-evidence/51-05-{t1-provenance,t1-torn,t1-bound,t1-keyset,t2-monitor}.json` (gitignored).

| Test | Failed on | Semantic assessment |
|------|-----------|---------------------|
| `only_top_level_rate_limit_events_and_results_decide_a_denial` | The collected failure list named (e1), (e2), (b1), Layer 1 on e1 (RateLimited) and Layer 1 on e2 (not RateLimited). (e3) and (b2) were not listed | Intended. This is exactly the RED set the plan predicted, and the two controls held |
| `a_torn_final_line_is_not_read_as_a_clean_silence` | `left: NotDenied, right: TornAfterLastEvent` | Intended. The stub reproduced today's boolean answer |
| `a_torn_clause_leads_the_capped_clause_at_the_cause_bound` | `the torn segment must lead the clause; got: last result: terminal_reason ttt…` | Intended. The clause closed inside the bound but had no torn segment |
| `the_on_disk_idle_timeout_record_key_set_is_pinned` | The key set lacked `capture_torn_final_line` | Intended. The scaffold field was `#[serde(skip)]` |
| `a_torn_final_line_at_the_idle_timeout_is_flagged_on_the_record` | `left: None, right: Some(Bool(true))` on a record that was written | Intended. The record was written, so the torn line was not mistaken for a denial, but it had no flag |

**Negative control on the record-flag path.** I added case (e) to the torn test: a record flagged `capture_torn_final_line: true` over a capture that reads intact. To prove it bites, I temporarily replaced `on_disk.capture_torn_final_line` with `false` in the reader. The test then failed with `the record's torn flag must add the segment; got: idle timeout: …`. I restored the file from a saved copy and confirmed the restore with `cmp`.

## Decisions Made
- See `key-decisions` in the frontmatter. All four were inside this plan's remit. None needed an operator decision.
- No checkpoints. The tracer gate ran in interactive mode (`end-of-phase`, `<automated>` only). Task 1's verify passed (exit 0) before Task 2 started.

## Deviations from Plan

**1. [Rule 3 - Blocking] The Task 1 RED commit carried compile scaffolding**
- **Found during:** Task 1 RED.
- **Issue:** Tests that name `CaptureDenial`, `capture_rate_limit_denial` and the new field do not compile until those exist. The classifier refuses that as `build_failure`.
- **Fix:** `4e7aa14` added the enum, a stub `capture_rate_limit_denial` that returns today's boolean answer, and the field as `#[serde(skip)]`. The build had dead-code warnings only. GREEN replaced all three.

**2. [Rule 2 - Coverage] Added case (e) to `a_torn_final_line_is_not_read_as_a_clean_silence`**
- **Found during:** Task 1 GREEN.
- **Issue:** The planned cases (a)–(d) exercise only the read-time capture check. The key link "or the record's flag, adds the torn clause" had no test of its own.
- **Fix:** Case (e) checks a flagged record over an intact capture, with the negative control above. Committed in `8e0955e`.

**Total deviations:** 2. Neither changes behavior beyond the plan. **Impact:** none on scope.

## Issues Encountered
- No DevFlow defect was hit, so there is no 999.x candidate.
- The misplaced doc comment that 51-03 recorded is still in place: `parse_idle_timeout_side_channel`'s long doc sits above `capture_shows_rate_limit_denial`. I left it as found. The new items are inserted after `capture_shows_rate_limit_denial`, so that doc did not move onto them.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 51-01's failover trigger can no longer be fired by a subagent's quota event.
- The production rate of torn final lines is still unmeasured (backlog 999.71).

## Self-Check: PASSED
- Modified files exist: `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/monitor.rs`.
- Commits `4e7aa14`, `8e0955e`, `02b0402`, `ab00f22` are ancestors of HEAD (checked with `git merge-base --is-ancestor`).

---
*Phase: 51-rate-limit-agent-failover*
*Completed: 2026-11-07*
