---
phase: 25-end-to-end-dogfood-blockers
plan: 19
subsystem: testing
tags: [rust, cargo-test, monitor-leak, raii-guard, gap-closure]

# Dependency graph
requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: "ReapMonitorOnDrop RAII guard (test_support.rs) and the five prior guarded sites from gap-closure rounds 3-4 (25-15 through 25-18)"
provides:
  - "Sixth and last unguarded monitor-spawning test site closed: resume_clears_stop_marker_and_advances_past_stop_point now reaps the wrapper resume() spawns"
  - "A runtime assertion (reloaded.monitor_pid.is_some()) that makes a future silent no-op of the guard impossible to pass unnoticed"
  - "WINDOWS.md item 5 closed via CLI: ledger reads 0 open / 1 waived / 4 fixed / 5 total"
affects: [testing, ci-hygiene]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Guard bound from state RELOADED FROM DISK (workflow::load_state(root, phase).ok()), not from a local `state` binding, when the launch-driving call (resume()) owns its own State and never writes back into the caller's local variable"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - .planning/WINDOWS.md

key-decisions:
  - "Bound ReapMonitorOnDrop from reloaded_for_reap (workflow::load_state(root, phase).ok(), mapped through .as_ref()) rather than from the test's local `state` binding, since resume() loads its own on-disk State and never writes the spawned pid back into the caller's local variable — the obvious `&state` form would have captured pid: None and silently reaped nothing"
  - "Guard is bound immediately after the PATH-restore unsafe block and before result.unwrap(), so a resume() that spawns the monitor and then fails a later ? still gets reaped"
  - "Added a new final assertion (reloaded.monitor_pid.is_some()) without reordering the three pre-existing assertions on stopped/stop_reason/stop_until"
  - "Closed WINDOWS.md item 5 exclusively via `gsd-tools windows fixed 5` (no hand-edit) to keep the YAML frontmatter counters, markdown table, and JSON mirror block in sync"

patterns-established: []

requirements-completed: ["G-25-3"]

coverage:
  - id: D1
    description: "resume_clears_stop_marker_and_advances_past_stop_point binds a ReapMonitorOnDrop guard built from disk-reloaded state (not the stale local binding) before result.unwrap(), reaping the monitor wrapper resume() spawns on every exit path"
    requirement: "G-25-3"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#tests::resume_clears_stop_marker_and_advances_past_stop_point"
        status: pass
    human_judgment: false
  - id: D2
    description: "A silent no-op of the guard is made impossible to pass unnoticed: the test now asserts reloaded.monitor_pid.is_some()"
    requirement: "G-25-3"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#tests::resume_clears_stop_marker_and_advances_past_stop_point"
        status: pass
    human_judgment: false
  - id: D3
    description: "WINDOWS.md item 5 closed via gsd-tools windows fixed 5; ledger reads 0 open / 1 waived / 4 fixed / 5 total across all three representations (YAML frontmatter, markdown table, JSON mirror)"
    verification:
      - kind: other
        ref: "gsd-tools windows status (post-fix output recorded below)"
        status: pass
    human_judgment: false

duration: 9min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 19: Guard the Sixth Monitor-Leak Site (resume()) Summary

**Bound the ReapMonitorOnDrop guard from disk-reloaded state (not the stale test-local binding) in `resume_clears_stop_marker_and_advances_past_stop_point`, closing the last of six monitor-wrapper leak sites in this crate, and closed WINDOWS.md item 5 via the CLI.**

## Performance

- **Duration:** 9 min
- **Started:** 2026-07-28T16:07:34-04:00 (worktree base commit f2e3d15)
- **Completed:** 2026-07-28T16:15:51-04:00
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Bound the reap guard in `resume_clears_stop_marker_and_advances_past_stop_point` from `workflow::load_state(root, phase).ok()` reloaded from disk — not from the test's stale local `state` binding, which `resume()` never writes back into
- Added `assert!(reloaded.monitor_pid.is_some(), ...)` so a future `resume()` that stops spawning the monitor fails loudly instead of the guard silently reaping nothing
- Closed `.planning/WINDOWS.md` item 5 via `gsd-tools windows fixed 5` — ledger now reads 0 open / 1 waived / 4 fixed / 5 total
- Verified `cargo test --workspace --no-fail-fast` reports 696 passed / 0 failed (baseline preserved — this plan adds an assertion to an existing test, not a new test), and `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` are both clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Bind the guard from reloaded state and prove the spawn happened** - `02cb9ba` (test)
2. **Task 2: Close WINDOWS.md item 5 and gate the workspace** - `dd14c37` (docs)

**Plan metadata:** committed in this same response (final commit below).

## Files Created/Modified
- `crates/devflow-cli/src/pipeline_launch.rs` - Bound `_reap_guard` at line 500 (`reloaded_for_reap.as_ref().map(ReapMonitorOnDrop::after_launch)`), built from `reloaded_for_reap` at line 499 (`workflow::load_state(root, phase).ok()`); first panicking checkpoint it precedes is `result.unwrap()` at line 504. Added final assertion `reloaded.monitor_pid.is_some()` after the three pre-existing assertions, without reordering them.
- `.planning/WINDOWS.md` - Item 5 marked `fixed` via `gsd-tools windows fixed 5`; ledger recomputed to `open_count: 0`, `waived_count: 1`, `fixed_count: 4`, `total_count: 5`.

## Verification evidence

- **Guard binding line:** 500 (`let _reap_guard = reloaded_for_reap.as_ref().map(ReapMonitorOnDrop::after_launch);`), built from `reloaded_for_reap` at line 499. First panicking checkpoint it precedes: `result.unwrap()` at line 504.
- **Observed `reloaded.monitor_pid` proving the spawn is real:** `942461` (observed under `--nocapture` on a representative run: `stage plan → launched Claude Code (monitor pid 942461)`; the test's new assertion also passed, confirming `Some(_)`).
- **Targeted test:** `cargo test -p devflow --bin devflow -- --exact pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point` → `test result: ok. 1 passed; 0 failed`.
- **Workspace test count:** `cargo test --workspace --no-fail-fast` → sum of all `test result: ok` lines = **696 passed; 0 failed** (matches the plan's stated baseline exactly; asserted on the printed `N passed` lines, not exit code, per the plan's trap warning).
- **fmt/clippy:** `cargo fmt --all -- --check` clean (after one `cargo fmt --all` pass reformatted the new `.map(...)` chain onto three lines); `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **`gsd-tools windows status` after closing item 5:**
  ```json
  {
    "ok": true,
    "ledger": {
      "schema_version": 1,
      "open_count": 0,
      "waived_count": 1,
      "fixed_count": 4,
      "total_count": 5,
      "last_updated": "2026-07-28T20:13:59.421Z"
    }
  }
  ```
  (entries 1/3/4 remain `fixed` and entry 2 remains `waived`, all untouched; entry 5 is now `fixed` with `resolved_at: 2026-07-28T20:13:59.421Z`.)

## Decisions Made
- Bound the guard from disk-reloaded state (`reloaded_for_reap`) rather than the test's local `state` binding — the plan's called-out crux, since `resume(root, phase)` loads its own `State` and never writes the spawned pid back into the caller's local variable. The obvious `ReapMonitorOnDrop::after_launch(&state)` form (correct at the five pre-existing sites, which pass `&mut state` that the launch itself populates) would have captured `pid: None` here and silently reaped nothing while looking like a correct fix.
- Ran `cargo fmt --all` once to reformat the new `.map(...)` chain onto three lines per rustfmt's line-length rule; no other formatting changes made.
- Closed the WINDOWS.md ledger entry exclusively through `gsd-tools windows fixed 5`, never by hand-editing the file, per the plan's explicit instruction (the file stores the same data in three synced representations).

## Deviations from Plan

None - plan executed exactly as written. The one auto-applied fix (a `cargo fmt` reformat of the newly added `.map(...)` chain) is not a deviation from the plan's intent — it is exactly the `cargo fmt --all -- --check` gate the plan's own verify step specifies, and running `cargo fmt --all` to satisfy it is the mechanical, expected response to that check's failure.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- The enumeration this plan closes (round 4/5's transitive sweep over all eight launch-reaching entry points) is stated in the plan as CLOSED — this was the last of six monitor-wrapper leak sites in `devflow-cli`'s test suite.
- Residual note carried over from the plan (stated, not hidden): this fix stops the suite from manufacturing NEW orphaned wrappers at this site going forward. It does not clean up wrappers already accumulated on a developer's machine from earlier runs — that cleanup is `devflow gate sweep --reap-strays`, which per 25-15 is safe to run only after its reachability filter landed.
- WINDOWS.md now reads 0 open / 1 waived / 4 fixed / 5 total. No open items remain in this ledger for phase 25's monitor-leak defect class.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*
