---
phase: 28-close-the-checkpoint-answer-return-path
plan: 05
subsystem: infra
tags: [rust, cli, pipeline-state, tdd]

# Dependency graph
requires:
  - phase: 28-01
    provides: "phase.execute-phase init context and CONTEXT.md decisions for phase 28, including D-15's problem statement"
provides:
  - "resume()'s stop-marker clear gated on state.stopped — an unfired --until cap now survives devflow resume"
  - "Two new regression tests proving the unfired-cap and no-cap resume paths, alongside the existing fired-cap test"
affects: [pipeline_launch, devflow-cli-resume]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Gate a multi-field state clear on the discriminating boolean (state.stopped) rather than clearing unconditionally — mirrors 18d's transition_resets_consecutive_failures pattern of naming the exact condition instead of a blanket reset."

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs

key-decisions:
  - "Wrapped the three-line clear (stopped/stop_reason/stop_until) in `if state.stopped { ... }`, leaving save_state/launch_stage ordering outside the guard unchanged for both paths — per plan D-15."
  - "Added resume_without_a_cap_is_unchanged as a third regression test (stop_until: None case) since grep confirmed it was not already covered by any existing test."

patterns-established: []

requirements-completed: ["999.60", "D-15"]

coverage:
  - id: D1
    description: "An unfired --until cap (stopped: false, stop_until: Some(stage)) survives devflow resume instead of being silently discarded"
    requirement: "D-15"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_preserves_unfired_until_cap"
        status: pass
    human_judgment: false
  - id: D2
    description: "A phase that IS stopped (fired cap) still resumes exactly as before — stopped/stop_reason/stop_until all cleared, saved stage relaunched"
    requirement: "D-15"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point"
        status: pass
    human_judgment: false
  - id: D3
    description: "An ordinary rate-limit/infra resume with no cap at all (stop_until: None) is unaffected by the gating change"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_without_a_cap_is_unchanged"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-07-31
status: complete
---

# Phase 28 Plan 05: Gate resume()'s stop-marker clear on state.stopped (D-15) Summary

**`resume()` in `pipeline_launch.rs` now clears `stopped`/`stop_reason`/`stop_until` only when `state.stopped` is true, so an unfired `--until` cap survives `devflow resume` instead of being silently discarded.**

## Performance

- **Duration:** 10 min
- **Started:** 2026-07-31T02:21:44Z (worktree base commit)
- **Completed:** 2026-07-31T02:31:03Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments
- Gated `resume()`'s three-field clear (`stopped`, `stop_reason`, `stop_until`) on `state.stopped`, so a rate-limited/infra-paused resume (where the cap has not fired) no longer wipes the operator's `--until` boundary.
- The save/relaunch ordering (`workflow::save_state` before `launch_stage`) is unchanged for both the stopped and not-stopped paths — the guard wraps only the three assignments, not the save or relaunch.
- Doc comment extended (not replaced): the original 20c paragraph stays verbatim as the justification for the stopped branch; a new D-15 paragraph states the discriminator rationale.
- Two new regression tests added test-first, mirroring `resume_clears_stop_marker_and_advances_past_stop_point`'s fixture scaffolding verbatim (`init_repo`, `stub_agent_binary("claude")`, `ENV_MUTEX`, PATH prepend/restore, `ReapMonitorOnDrop::after_launch`):
  - `resume_preserves_unfired_until_cap` — `stopped: false`, `stop_until: Some(Plan)` → after `resume`, `stop_until` is still `Some(Plan)`.
  - `resume_without_a_cap_is_unchanged` — `stopped: false`, `stop_until: None` → after `resume`, nothing appears; confirmed via grep that no prior test covered this ordinary no-cap resume shape before adding it.
- The existing `resume_clears_stop_marker_and_advances_past_stop_point` test's body is unmodified — only referenced in a new doc comment above the added tests.

## RED evidence (change-acceptance rule 3)

`resume_preserves_unfired_until_cap`, run against the pre-fix unconditional clear (commit `392477d`, before `037dc61`):

```
thread 'pipeline_launch::tests::resume_preserves_unfired_until_cap' panicked at crates/devflow-cli/src/pipeline_launch.rs:587:9:
assertion `left == right` failed: resume must NOT discard an unfired --until cap: stopped was false, so the cap has not yet done its job and the operator's boundary must survive
  left: None
 right: Some(Plan)

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 236 filtered out
```

The failure is on the intended assertion (`stop_until` reloaded as `None` instead of the expected `Some(Plan)`) — not a setup error, compile failure, or unrelated panic. Test compiled and ran cleanly; the fixture (stub agent, PATH, monitor spawn) worked, only the behavioral assertion failed.

## GREEN evidence

After applying the fix (`037dc61`):

```
running 3 tests
test pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point ... ok
test pipeline_launch::tests::resume_preserves_unfired_until_cap ... ok
test pipeline_launch::tests::resume_without_a_cap_is_unchanged ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 234 filtered out; finished in 0.23s
```

Full workspace suite (`scripts/check.sh test`): `devflow` unit tests 237 passed / 0 failed; `devflow_core` unit tests 418 passed / 0 failed; all integration test binaries 0 failed. `scripts/check.sh fmt` and `scripts/check.sh clippy` both clean (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`).

## Task Commits

Followed the plan's `tdd="true"` RED → GREEN flow (no REFACTOR needed):

1. **Task 1 RED — add failing regression tests for D-15** - `392477d` (test)
2. **Task 1 GREEN — gate resume()'s stop-marker clear on state.stopped** - `037dc61` (fix)

**Plan metadata:** (this commit, docs: complete plan — see below)

## Files Created/Modified
- `crates/devflow-cli/src/pipeline_launch.rs` - `resume()`'s three-field clear wrapped in `if state.stopped { ... }`; doc comment extended with D-15 rationale; two new tests added to the `#[cfg(test)] mod tests` block.

## Decisions Made
- Confined the diff strictly to `resume()`, its doc comment, and the test module — verified via `git diff <wave-base> HEAD -- crates/devflow-cli/src/pipeline_launch.rs` (146 insertions / 3 deletions, no other function touched), since plan 28-03 edits this same file in the next wave and must not inherit a surprise.
- Added the third behavior case (`resume_without_a_cap_is_unchanged`) per the plan's instruction to check first and not duplicate coverage — `rg`/`grep` for `stop_until: None` and `resume_without` confirmed no prior test covered the ordinary no-cap resume path.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

`resume()` is now the correct discriminator between "cap fired, operator is overriding it" (clear) and "cap still pending" (preserve). Plan 28-03 (next wave, same file) can build on this without inheriting scope creep — the diff touched only `resume()`, its doc comment, and the test module.

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/pipeline_launch.rs (modified, contains `if state.stopped {`)
- FOUND: commit `392477d` (test: add failing regression tests)
- FOUND: commit `037dc61` (fix: gate resume()'s stop-marker clear)

---
*Phase: 28-close-the-checkpoint-answer-return-path*
*Completed: 2026-07-31*
