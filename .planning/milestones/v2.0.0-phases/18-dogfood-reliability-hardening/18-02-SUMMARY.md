---
phase: 18-dogfood-reliability-hardening
plan: 02
subsystem: testing
tags: [rust, cargo-test, integration-test, flake-fix, wait_for]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: typed outcomes, build provenance (context only — this plan is test-only, no source dependency)
provides:
  - "Deflaked `parallel_creates_two_worktrees_and_spawns_two_monitors` assertion ordering (WR-03)"
affects: [18-03, 18-04, 18-05, 18-06, 18-07 — all later waves run `cargo test --workspace` and rely on this test not being a source of noise]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Assert a polled resource immediately inside its own `wait_for` window, not after a later, unrelated `wait_for` call — a second poll loop gives a concurrent archiver enough time to invalidate the first resource even when both checks run before any other unrelated assertion."

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "The plan's literal <action> text (combined assertion after both `wait_for` calls) was insufficient — the 25x stability loop reproduced the flake at run 15/25 under that version. Corrected to interleave assert-immediately-after-each-wait, matching the plan's own must_haves.truths (which the <action> text under-delivered on) rather than the acceptance_criteria's more permissive literal reading."

patterns-established:
  - "Pattern: when a plan's <action> text and its must_haves.truths disagree in strictness, and empirical verification later proves the truths were right, fix to satisfy the truths (Rule 1 — auto-fix bug) and document the discrepancy rather than shipping a fix that merely passes the acceptance_criteria grep checks."

requirements-completed: [18g]

coverage:
  - id: D1
    description: "parallel_creates_two_worktrees_and_spawns_two_monitors no longer flakes on stdout-capture archive timing; each capture is asserted inside its own wait_for window"
    requirement: "18g"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#parallel_creates_two_worktrees_and_spawns_two_monitors — 25 consecutive isolated runs, all pass"
        status: pass
    human_judgment: false
  - id: D2
    description: "Full workspace test suite remains green after the fix"
    verification:
      - kind: unit
        ref: "cargo test --workspace — 0 failed across all binaries, including build_provenance (WR-07, 3 passed, ~42s)"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-07-21
status: complete
---

# Phase 18 Plan 02: WR-03 Stdout-Capture Assertion-Ordering Flake Summary

**Deflaked `parallel_creates_two_worktrees_and_spawns_two_monitors` by asserting each phase's stdout capture inside its own `wait_for` window instead of after both waits complete — and caught, via the plan's own 25x stability loop, that the plan's literal "combined assertion" instruction was itself still racy.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-07-21T03:24:39Z (approx, from prior plan's final commit)
- **Completed:** 2026-07-21T03:36:19Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Relocated `phase7_stdout`/`phase8_stdout` assertions from a trailing re-check (after unrelated `state7`/`state8` load assertions) to inside each capture's own `wait_for` window, mirroring the already-fixed `wait_for_pid` sibling pattern.
- Discovered via the plan's mandated 25x stability loop that the plan's literal `<action>` text (assert both paths in a single combined check placed after *both* `wait_for` calls) was itself still racy — `wait_for(&phase8_stdout)`'s own polling loop gives a fast monitor enough time to archive `phase7_stdout` in the interim. Reproduced this directly: run 15/25 failed with `assertion failed: phase7_stdout.exists()`.
- Applied the real fix (Rule 1 — auto-fix bug): interleave `wait_for(&phase7_stdout); assert!(...); wait_for(&phase8_stdout); assert!(...);`, matching the plan's own `must_haves.truths` bullet ("asserts each phase's stdout capture immediately after the `wait_for` call that established it") which the `<action>` text under-delivered on.
- Re-ran the 25x loop clean (25/25 pass) after the correction, then confirmed `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, and `cargo test --workspace` (0 failed, including the flaky-under-contention `build_provenance`/WR-07 test) all green.

## Task Commits

Each task was committed atomically:

1. **Task 1: Assert each stdout capture immediately after its wait, and drop the trailing re-checks** - `84afc3b` (test) — implemented the plan's literal `<action>` text (combined assertion after both waits).
2. **Task 2 (discovered during the 25x loop): correct the assertion ordering to interleave per-wait** - `8dcc9ef` (fix) — the real fix; superseded `84afc3b`'s assertion placement without reverting the WR-03 doc comment or the state-assertion ordering guarantees.

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/devflow-cli/tests/phase7_cli.rs` - `parallel_creates_two_worktrees_and_spawns_two_monitors` now asserts `phase7_stdout`/`phase8_stdout` each inside its own `wait_for` window; trailing re-checks removed; WR-03 doc comment added explaining the archive-timing race and citing the directly-observed run-15/25 reproduction.

## Decisions Made
- **The plan's `<action>` text under-specified the fix; corrected to match `must_haves.truths`, not the more permissive `acceptance_criteria` grep checks.** The acceptance criteria (`rg` line-order checks) were satisfied by both the flawed intermediate version and the final version, since both place a single `.exists()` line before its corresponding `load_state` line. Only the plan's `must_haves.truths` ("asserted... immediately after the `wait_for` call that established it") and the mandated 25x stability loop caught that the intermediate version was still racy. Treated this as Rule 1 (auto-fix bug): the acceptance criteria are necessary but not sufficient; the truths and the loop are the actual proof obligation.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The plan's literal combined-assertion fix was itself racy; corrected to interleaved per-wait assertions**
- **Found during:** Task 2 (the mandated 25x stability loop)
- **Issue:** Task 1 implemented the plan's `<action>` text exactly as written: `wait_for(&phase7_stdout); wait_for(&phase8_stdout); assert!(phase7_stdout.exists()); assert!(phase8_stdout.exists());`. Run 15 of the 25x loop failed with `assertion failed: phase7_stdout.exists()` at the line implementing that combined check — proving the combined-assertion-after-both-waits shape does not fully close the WR-03 race, because `wait_for(&phase8_stdout)`'s own poll loop (up to 200 × 25ms) gives a monitor time to archive `phase7_stdout` before the combined assertion runs.
- **Fix:** Interleaved the checks so each assertion runs strictly inside its own capture's `wait_for` window: `wait_for(&phase7_stdout); assert!(phase7_stdout.exists()); wait_for(&phase8_stdout); assert!(phase8_stdout.exists());`. Updated the WR-03 doc comment to record the mechanism and the directly-observed run-15/25 reproduction (no fabrication — this is the one deviation for which a real pre-existing-shape RED was actually captured).
- **Files modified:** `crates/devflow-cli/tests/phase7_cli.rs`
- **Verification:** Re-ran the 25x loop after the fix — 25/25 pass. Full `cargo test --workspace` — 0 failed. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both exit 0.
- **Committed in:** `8dcc9ef`

---

**Total deviations:** 1 auto-fixed (1 Rule 1 — bug)
**Impact on plan:** Necessary correctness fix; without it the "fixed" test would have remained exactly as flaky as before, just with the failure moved one line down. No scope creep — the change stayed entirely within the single test function this plan targets.

## Issues Encountered
None beyond the deviation documented above.

## Flake Reproduction Record (18-VALIDATION.md honesty constraint)

This SUMMARY makes two distinct claims and keeps them separate, per 18-VALIDATION.md's instruction not to fabricate an unobserved RED:

1. **The original pre-plan WR-03 flake (trailing re-check after unrelated `state7`/`state8` assertions) was NOT directly reproduced locally in this session.** Task 1's fix was applied and passed on first run; no baseline RED was captured against the pre-fix code shape described in the plan's `<objective>`. For that specific shape, this fix is **prevention-only** — the mechanism is inferred from the already-proven-real `wait_for_pid` sibling race (`phase7_cli.rs:101-105`), not from a locally observed failure of the exact pre-fix assertion order.
2. **A related but distinct racy shape — the plan's own literal combined-assertion fix — WAS directly reproduced locally**, failing at run 15 of the mandated 25x loop with a real `assertion failed: phase7_stdout.exists()` panic (see Deviation #1 above). This is a genuine, locally-observed RED, and it validates that the WR-03 race is real and has enough margin to appear even on this workstation, not only on CI's shared runners as 18-RESEARCH.md speculated.

Net: the underlying WR-03 race mechanism is empirically confirmed (via #2), even though the specific originally-described flaky shape (#1) was not separately reproduced before being fixed. The final committed fix (`8dcc9ef`) passed 25/25 consecutive isolated runs.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `cargo test --workspace` is green (0 failed across all binaries) and this is no longer a source of assertion-ordering noise for waves 2-6.
- `build_provenance` (WR-07) remains the one known flaky-under-contention test in the suite (~42s, two full cargo builds) — out of scope for this plan, unaffected here (passed cleanly, 3/3).
- 18-03 through 18-07 (18a already done in 18-01) can proceed; a later `cargo test --workspace` failure is signal, not noise from this test.

---
*Phase: 18-dogfood-reliability-hardening*
*Completed: 2026-07-21*

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/tests/phase7_cli.rs`
- FOUND: commit `84afc3b`
- FOUND: commit `8dcc9ef`
- FOUND: `.planning/phases/18-dogfood-reliability-hardening/18-02-SUMMARY.md`
