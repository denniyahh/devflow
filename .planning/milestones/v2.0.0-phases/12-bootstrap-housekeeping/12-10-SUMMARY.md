---
phase: 12-bootstrap-housekeeping
plan: 10
subsystem: infra
tags: [rust, ship, timezone, rfc3339, shell-quoting, cron]

# Dependency graph
requires:
  - phase: 12-bootstrap-housekeeping
    provides: "12-02's cron/ship.rs edits (same file, sequenced to avoid a wave collision)"
provides:
  - "Documented timezone-safety rationale for parse_rfc3339ish's second-restoration"
  - "Widened shell_quote safe-unquoted character set (~ : @ + = %)"
  - "Negative UTC-offset test coverage for parse_rfc3339ish via cron_schedule_from_retry_after"
affects: [12-bootstrap-housekeeping]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "shell_quote: allowlist-based unquoted safe set, unsafe input always falls through to single-quote wrapping"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/ship.rs

key-decisions:
  - "Widened shell_quote's safe set additively only (no character removed from the existing safe set), preserving the existing fail-safe fallback to single-quoting for anything not explicitly listed"
  - "Drove the negative-offset test through the public cron_schedule_from_retry_after rather than parse_rfc3339ish directly, since the latter is private"

patterns-established: []

requirements-completed: [WR-05, WR-08, 12f-rfc3339-negative-offset]

coverage:
  - id: D1
    description: "parse_rfc3339ish's second-restoration documented as timezone-safe (no behavior change)"
    requirement: "WR-05"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::cron_schedule_rounds_up_to_nearest_minute"
        status: pass
    human_judgment: false
  - id: D2
    description: "shell_quote's safe-unquoted set widened to include ~ : @ + = % ; unsafe input (spaces, quotes) still single-quoted"
    requirement: "WR-08"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::shell_quote_leaves_common_safe_chars_unquoted"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::shell_quote_quotes_unsafe_input"
        status: pass
    human_judgment: false
  - id: D3
    description: "parse_rfc3339ish negative UTC-offset normalization proven via cron_schedule_from_retry_after"
    requirement: "12f-rfc3339-negative-offset"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::cron_schedule_normalizes_negative_offset"
        status: pass
    human_judgment: false

# Metrics
duration: 15min
completed: 2026-07-08
status: complete
---

# Phase 12 Plan 10: Ship.rs Timezone Docs, Shell-Quote Widening, Negative-Offset Test Summary

**Documented parse_rfc3339ish's second-restoration as timezone-safe, widened shell_quote's unquoted-safe character set (`~ : @ + = %`), and added a negative-UTC-offset cron test proving correct normalization.**

## Performance

- **Duration:** ~15 min
- **Completed:** 2026-07-08T23:55:15Z
- **Tasks:** 3 completed
- **Files modified:** 1

## Accomplishments
- WR-05: `parse_rfc3339ish` now carries a comment explaining why restoring `second` verbatim after UTC-minute normalization is timezone-safe (minute-granularity normalization never touches sub-minute components) — no behavior change.
- WR-08: `shell_quote`'s unquoted-safe character set widened additively with `~ : @ + = %`, reducing over-quoting of common path/version/identifier strings while every unsafe character still falls through to single-quote wrapping.
- 12f: Added `cron_schedule_normalizes_negative_offset`, proving `-05:00` (with second-level rounding) and `-05:30` (non-zero offset-minutes component) both normalize correctly to UTC before the cron minute is computed.

## Task Commits

Each task was committed atomically:

1. **Task 1: WR-05 — document parse_rfc3339ish second-restoration** - `ad3b37d` (docs)
2. **Task 2: WR-08 — widen shell_quote safe-unquoted character set** - `f655142` (fix)
3. **Task 3: 12f — parse_rfc3339ish negative UTC offset coverage** - `bb7e367` (test)

**Plan metadata:** committed separately per `<final_commit>` protocol.

## Files Created/Modified
- `crates/devflow-core/src/ship.rs` - Added timezone-safety doc comment to `parse_rfc3339ish`; widened `shell_quote`'s safe-unquoted allowlist; added `shell_quote_leaves_common_safe_chars_unquoted`, `shell_quote_quotes_unsafe_input`, and `cron_schedule_normalizes_negative_offset` tests

## Decisions Made
- Widened `shell_quote`'s safe set purely additively (no existing safe character removed), preserving the fail-safe single-quote fallback for anything outside the allowlist — this cannot introduce a shell-injection regression, only reduce over-quoting.
- Since `parse_rfc3339ish` is private, drove the negative-offset test through the public `cron_schedule_from_retry_after`, mirroring the existing `Z`-suffix test's assertion style.
- Added a second negative-offset case (`-05:30`) beyond the plan's minimum single case, to also exercise a non-zero offset-minutes component.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None. `cargo fmt` reformatted the multi-line `shell_quote` condition and one test assertion after Task 2's edit (standard formatting, not a functional change) — verified tests/clippy/fmt all still pass afterward.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All three targeted low-priority ship.rs review items (WR-05, WR-08) and the last remaining 12f test gap (negative-offset coverage) are closed.
- `cargo test -p devflow-core ship::` (22 tests), `cargo clippy -p devflow-core -- -D warnings`, and `cargo fmt --check` all pass clean.
- No blockers for subsequent phase-12 plans.

---
*Phase: 12-bootstrap-housekeeping*
*Completed: 2026-07-08*

## Self-Check: PASSED
