---
phase: 52-subtract-and-stabilize
plan: 01
subsystem: core
tags: [rust, devflow-core, reset_time, rate-limit, parsing, tdd]

requires: []
provides:
  - "pub mod reset_time with parse_reset_time(&str) -> Option<i64> (exact UTC epoch seconds, fraction rounded up)"
  - "reset_time::civil_from_days / days_from_civil (crate-visible, relocated from ship.rs)"
  - "supervisor::rfc3339_from_unix no longer depends on ship.rs"
affects: [52-10, 52-13, 52-31, 52-32]

actuals:
  tokens: 6150
  tasks: 2
  commits: 5
plan_head_before: 938f6ac2b6a4f2733a41d9f87e9bf15ca7f0e1ae
plan_head_after: f665ae1991cb4f8c431f66025f5d960e8ec037cf

tech-stack:
  added: []
  patterns:
    - "Reset-time parsing returns exact epoch seconds; a minute round-up is the caller's job (hermes schedule)"
    - "A present but malformed offset is None, never read as UTC"

key-files:
  created:
    - crates/devflow-core/src/reset_time.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/ship.rs
    - crates/devflow-core/src/supervisor.rs

key-decisions:
  - "Epoch seconds accept digits only: a sign is None, so the old parse_unix_seconds acceptance of negative epochs is dropped"
  - "TDD commit subjects follow the GSD gate format (test(52-01)/feat(52-01)), not the plan's refactor(core) wording"

patterns-established:
  - "Pattern: compute test instants from days_from_civil instead of trusting literal epoch values"

requirements-completed: [CUT-03]

coverage:
  - id: D1
    description: "parse_reset_time reads epoch seconds exactly, rounds a fraction up, and refuses empty, signed, overflowing or malformed input"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::an_epoch_seconds_reset_parses_to_the_exact_second"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::an_empty_or_invalid_reset_is_none"
        status: pass
    human_judgment: false
  - id: D2
    description: "RFC 3339 resets with Z or +/-HH[:MM] offsets read to the same instant as their epoch form; a fractional second rounds up; a malformed offset or out-of-range field is None"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::epoch_and_rfc3339_name_the_same_instant"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::a_fractional_rfc3339_second_is_never_rounded_earlier"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::a_malformed_offset_is_none"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::parse_offset_minutes_bounds_and_forms"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::an_out_of_range_rfc3339_field_is_none"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::a_malformed_seconds_fraction_is_none"
        status: pass
    human_judgment: false
  - id: D3
    description: "Civil-date helpers relocated to reset_time; supervisor::rfc3339_from_unix calls reset_time::civil_from_days"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::civil_from_days_round_trips_days_from_civil"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::civil_from_days_names_leap_rule_boundaries"
        status: pass
      - kind: other
        ref: "cargo test -p devflow-core --lib (supervisor rfc3339_from_unix assertions included)"
        status: pass
    human_judgment: false
  - id: D4
    description: "ship.rs keeps no parser copy; hermes_schedule_from_retry_after is rebuilt on parse_reset_time with its own minute round-up and its tests stay green"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::hermes_schedule_rounds_up_to_nearest_minute"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::hermes_schedule_normalizes_negative_offset"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::hermes_schedule_formats_unix_seconds"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::cron_instructions_reject_unparseable_retry_time"
        status: pass
      - kind: other
        ref: "cargo clippy --workspace --all-targets -- -D warnings"
        status: pass
    human_judgment: false

duration: 38min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 01: Reset-time parser relocation Summary

**New `devflow_core::reset_time` module: `parse_reset_time` turns an agent's epoch or RFC 3339 reset time into exact UTC epoch seconds. It never rounds earlier, and it returns `None` for a malformed offset instead of reading it as UTC. `ship.rs` lost its parser copy, and the supervisor no longer imports from `ship`.**

## Performance

- **Duration:** 38 min
- **Started:** 2026-11-10T17:48:11Z
- **Completed:** 2026-11-10T18:26:35Z
- **Tasks:** 2
- **Files modified:** 4 (1 created)

## Accomplishments
- `reset_time::parse_reset_time(input: &str) -> Option<i64>` reads two forms after trimming: non-negative epoch seconds, and RFC 3339 with no offset, `Z`, `±HH:MM`, `±HHMM` or `±HH`. A fraction always rounds up, and no minute rounding is applied.
- `split_time_and_offset` now returns `Option<(&str, i32)>`, and `parse_rfc3339ish` propagates its `None`. So `+99:00`, `+05:60`, `+5` and `+abcd` give `None`, not a UTC reading (review C14).
- `days_from_civil` and `civil_from_days` moved into `reset_time`. `supervisor::rfc3339_from_unix` calls `crate::reset_time::civil_from_days`.
- `ship.rs` no longer has `RetryTimestamp`, `parse_retry_timestamp`, `parse_unix_seconds`, `parse_rfc3339ish`, `split_time_and_offset` or `parse_offset_minutes`. `hermes_schedule_from_retry_after` is a thin wrapper over `parse_reset_time` that does its own minute round-up.

## Task Commits

1. **Task 1 (tracer) RED:** `2b9f43f` test(52-01): add failing tests for reset_time epoch parsing
2. **Task 1 GREEN:** `8244bc2` feat(52-01): parse epoch-seconds resets in reset_time
3. **Task 2 RED:** `262af3f` test(52-01): add failing tests for RFC 3339 reset times
4. **Task 2 GREEN:** `291672a` feat(52-01): parse RFC 3339 resets and drop ship's parser copy
5. **Test hardening (mutation testing):** `f665ae1` test(52-01): pin reset_time edges that mutation testing found unpinned

## TDD Gate Compliance

The RED evidence is nextest JUnit XML (`--profile red` with a scratch nextest config that turns on JUnit). Each record went through `gsd-tools check tdd-red-evidence`.

| Target test | Verdict | Planned assertion it failed on (semantic assessment) |
|---|---|---|
| `reset_time::tests::an_epoch_seconds_reset_parses_to_the_exact_second` | RED_EVIDENCE_OK | `left: None, right: Some(1785645600)`: the stub returns None. This is the planned reason. |
| `reset_time::tests::epoch_and_rfc3339_name_the_same_instant` | RED_EVIDENCE_OK | `"2026-08-02T00:00:00Z"`: `None` vs `Some(1785628800)`. The parser was still epoch-only. |
| `reset_time::tests::a_fractional_rfc3339_second_is_never_rounded_earlier` | RED_EVIDENCE_OK | `None` vs `Some(1785628801)`. The parser was still epoch-only. |
| `reset_time::tests::a_malformed_offset_is_none` | RED_EVIDENCE_OK | This RED was recorded **after** the move and **before** the `Option` change. With the moved zero-offset fallback, `"…+99:00"` gave `Some(1785628800)` against an expected `None`. This is the C14 defect itself (not committed as a separate state). |

- Negative control on the classifier: a green run of the first target returned `INVALID_RED unexpected_green`.
- `an_empty_or_invalid_reset_is_none` passed against the stub, which is expected for a None-only test. `civil_from_days_round_trips_days_from_civil` failed against a stub (`left: 0, right: -1`) but is a relocation check, not a behaviour RED.
- Commit order: every `feat(52-01)` descends from a `test(52-01)` commit.

## Verification run

| Command | Exit |
|---|---|
| Task 1 `<automated>` chain (4 named nextest runs) | 0 |
| Task 2 `<automated>` chain (6 named nextest runs + `cargo clippy --workspace --all-targets -- -D warnings`) | 0 |
| Wrong-name control: `cargo nextest … -E 'test(=reset_time::tests::no_such_test)'` | 4 (the gate does bite) |
| `cargo test -p devflow-core --lib` | 0 (1149 passed at Task 2; 1152 after hardening) |
| `scripts/check.sh all` | 0 |
| `cargo semver-checks check-release -p devflow-core` | 0 (229 pass, "no semver update required") |
| `cargo mutants --file crates/devflow-core/src/reset_time.rs` (3 runs, `--iterate`) | 207 mutants: 205 caught or unviable, 2 missed (below) |

Acceptance-criterion greps: `ship.rs` contains none of the moved functions or `RetryTimestamp` (rg rc 1). `supervisor.rs` has `reset_time::civil_from_days` and no `ship::civil_from_days`. `lib.rs` has `pub mod reset_time;` between `registry` and `ship`.

**What this does not establish:** the tests pin the forms listed above and nothing more. A bare date (`2026-08-02`) is probed as `None`, so it would open a plain gate. A future timestamp shape an agent emits would read the same way. Mutation testing covered only `reset_time.rs`, using the `reset_time`, `ship::tests::(hermes|cron)` and `supervisor` tests.

## Files Created/Modified
- `crates/devflow-core/src/reset_time.rs` - new module: `parse_reset_time`, the private RFC 3339 and offset parsers, the civil-date helpers, and 10 tests
- `crates/devflow-core/src/lib.rs` - `pub mod reset_time;`
- `crates/devflow-core/src/ship.rs` - parser copy removed; `hermes_schedule_from_retry_after` rebuilt on `parse_reset_time`; `parse_offset_minutes_bounds_and_forms` test moved out
- `crates/devflow-core/src/supervisor.rs` - `rfc3339_from_unix` calls `crate::reset_time::civil_from_days`
- `.planning/phases/52-subtract-and-stabilize/deferred-items.md` - the day-past-month-end entry

## Decisions Made
- Negative epoch seconds are `None`. The plan lists `"-5"` as invalid; the old `parse_unix_seconds` accepted it.
- Commit subjects use the TDD gate's `test(52-01)` / `feat(52-01)` scopes, which `tdd.md`'s gate validation greps for. The plan's suggested `refactor(core): …` subjects would have failed that gate. The project's own phase-51 history uses the same scheme.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing tests] Pinned edges that mutation testing showed were unpinned**
- **Found during:** post-Task-2 mutation testing (global Rust harness rule)
- **Issue:** 22 mutants survived against the planned tests. Nothing checked the field range checks (month, day, hour, minute, second), a non-digit fraction, a zero fraction, any February day in the round trip, or the leap-rule century boundaries in `civil_from_days`.
- **Fix:** I added the tests `an_out_of_range_rfc3339_field_is_none`, `a_malformed_seconds_fraction_is_none` and `civil_from_days_names_leap_rule_boundaries` (expected day numbers from Python's `datetime.date`). I also added extra cases to the epoch-valid, epoch-invalid and round-trip tests.
- **Files modified:** `crates/devflow-core/src/reset_time.rs`
- **Verification:** `cargo mutants --iterate` brought the survivors down to 2.
- **Committed in:** `f665ae1`

**2. [Rule 1 - Bug, small tightening] Stricter seconds field**
- **Found during:** Task 2
- **Issue:** the old seconds parse dropped any fraction (`split('.').next()`), which would round `00.4` down. It also accepted extra `:` parts.
- **Fix:** the new `parse_seconds_field` requires digits on both sides of `.`, rounds a non-zero fraction up, and refuses a fourth `:` part.
- **Committed in:** `291672a`

**Accepted mutation survivors (not fixed, with reasons):**
- `split_time_and_offset` `idx > 0` → `>= 0`: this mutant is equivalent. An index of 0 means the time starts with `-`, and both versions then fail to parse the hour and return `None`.
- `split_time_and_offset` `trimmed.len() > 6` → `>= 6`: this differs only for a 6-character clock-with-offset such as `1:2+05`, which is not RFC 3339. The original returns `None` and the mutant reads the offset. No requirement says which is right, so I did not pin it.

**Behaviour changes in `hermes_schedule_from_retry_after`** (the four existing tests and `cron_schedule_parses_all_iso8601_offset_forms` stay green; the function is deleted in plan 52-10):
- A malformed offset now gives an empty schedule. Before, it gave the UTC reading.
- A non-zero fraction on a whole minute (`15:45:00.5Z`) now rounds to 15:46. Before, it was 15:45. The new result is later, which is the safe direction.
- Negative epoch seconds give an empty schedule.

**Plan literals checked:** `1785628800` = 2026-08-02T00:00:00Z is correct. The tests compute it from `days_from_civil`, and Python's `datetime` confirms it. `1785645600` is 2026-08-02T04:40:00Z. Task 1 uses it only as an arbitrary epoch value, so it is not wrong.

---

**Total deviations:** 2 auto-fixed (1 missing tests, 1 small tightening). **Impact on plan:** no scope creep. Both changes only refuse more input or pin existing behaviour.

## Issues Encountered
- The first `cargo mutants` run took about 11 s per mutant. A second `--iterate` run then blocked on its lock file. I stopped the first run by its recorded PID (1712780) part-way through. The `--iterate` runs tested every mutant it had not reached, against the new tests. No cargo-mutants temp directory is left in `/tmp`.

## Known limits (needs awareness)
- A day past the month's end is accepted: `2026-02-31T00:00:00Z` reads the same as 2026-03-03 (probed). The `day <= 31` check came over unchanged from `ship.rs`. The result is a later instant, never an earlier one. This is logged in `deferred-items.md` for plan 52-31 or 52-32.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plans 52-10 and 52-13 can delete `hermes_schedule_from_retry_after` and `ship.rs` without breaking the supervisor.
- Plans 52-31 and 52-32 can call `crate::reset_time::parse_reset_time` for the rate-limit gate deadline.

## Self-Check: PASSED
- FOUND: `crates/devflow-core/src/reset_time.rs`
- FOUND commits on HEAD: 2b9f43f, 8244bc2, 262af3f, 291672a, f665ae1 (`git merge-base --is-ancestor`). `check evaluation-scope --plan 52-01 --commits-only` resolved them from the plan subjects.
- `commits: 5` measured with `git rev-list --count 938f6ac..HEAD`

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*
