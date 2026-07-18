---
phase: 16-pipeline-reliability-hardening
plan: 01
subsystem: pipeline
tags: [rust, git-flow, hooks, release-versioning, events]

requires:
  - phase: 15-oss-readiness
    provides: Phase 15 dogfood evidence and the manually repaired v1.3.0 release baseline
provides:
  - Terminal Ship hooks merge the phase branch before computing a release version
  - Idempotent merge detection with truthful merge/no-op event telemetry
  - End-to-end regression coverage for post-merge version computation
  - Clean changelog and tag baseline after the Phase 15 release corruption
affects: [16a-external-verification, 16f-project-root-resolution, ship-hooks, release-history]

tech-stack:
  added: []
  patterns: [ordered terminal hooks, idempotent git post-condition guard, effect-specific event telemetry]

key-files:
  created: []
  modified:
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/hooks.rs
    - CHANGELOG.md

key-decisions:
  - "Treat an absent feature branch as already merged so terminal retries are safe after a prior merge deletes the branch."
  - "Emit merge_result with merged=true or merged=false separately from generic hook success."
  - "Verify ancestry using the captured feature-tip SHA because feature_finish intentionally deletes the merged branch."

patterns-established:
  - "Terminal effect ordering: Merge must run before VersionBump, with BranchCleanup last."
  - "Hook success and intended effect are separate signals; merge_result records the effect explicitly."

requirements-completed: [16k]

coverage:
  - id: D1
    description: "Terminal Ship processing merges an unmerged feature branch before VersionBump and reports the real merge effect."
    requirement: "16k"
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/hooks.rs#terminal_hooks_version_post_merge_develop"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow-core hooks::"
        status: pass
    human_judgment: false
  - id: D2
    description: "Absent feature branches are safe terminal retry no-ops with merged=false semantics."
    requirement: "16k"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#merge_is_fail_soft_when_branch_absent"
        status: pass
    human_judgment: false
  - id: D3
    description: "Bogus 1.2.173 through 1.2.176 release entries are removed while the legitimate v1.3.0 baseline remains intact."
    requirement: "16k"
    verification:
      - kind: other
        ref: "rg changelog assertions and git tag -l baseline check"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-07-17
status: complete
---

# Phase 16 Plan 01: Ship Terminal Truth Summary

**Terminal Ship now merges into develop before version computation, reports merge versus no-op truthfully, and is locked by an end-to-end post-merge tag regression test.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-07-17T23:35:06Z
- **Completed:** 2026-07-17T23:39:53Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Added `Hook::Merge` first in the terminal batch and backed it with an idempotent `GitFlow::is_merged_into_develop` post-condition check.
- Added `merge_result` events whose `merged` boolean distinguishes an actual merge from an already-merged or absent-branch no-op.
- Proved the complete terminal batch tags the post-merge develop commit count, then removed the four corrupted changelog entries without altering the clean tag set.

## Task Commits

1. **Task 1 RED: Define terminal merge ordering and no-op contracts** - `af32f1a` (test)
2. **Task 1 GREEN: Wire the idempotent merge hook before versioning** - `a9e698e` (feat)
3. **Task 2: Prove VersionBump computes against post-merge develop** - `9c22eed` (test)
4. **Task 3: Remove bogus wrong-checkout changelog entries** - `e711875` (docs)

## Files Created/Modified

- `crates/devflow-core/src/git.rs` - Adds the absent-or-ancestor merge post-condition guard.
- `crates/devflow-core/src/hooks.rs` - Adds merge dispatch, ordered terminal execution, effect telemetry, and regression tests.
- `CHANGELOG.md` - Removes only the bogus 1.2.173 through 1.2.176 auto-release blocks.

## Decisions Made

- An absent local feature branch means there is nothing to merge and therefore returns a successful `merged=false` no-op.
- Merge conflicts and other `feature_finish` failures remain hard hook errors; only the already-satisfied post-condition is fail-soft.
- The integration test captures the feature-tip SHA before execution, then checks that SHA is an ancestor of develop because successful terminal processing deletes the branch by design.

## Deviations from Plan

None - plan executed exactly as written. The ancestry assertion uses the captured feature-tip SHA rather than the deleted branch ref, preserving the stated behavior while respecting `feature_finish` cleanup semantics.

## Issues Encountered

- Git metadata is read-only in the default sandbox; each required atomic commit was completed through the approved elevated git path.

## User Setup Required

None - no external service configuration required.

## Verification

- `cargo test --workspace` - pass (35 CLI unit tests, 222 core unit tests, all integration/doc tests)
- `cargo clippy --workspace --all-targets -- -D warnings` - pass
- `cargo fmt --check` - pass
- Changelog corruption scan - no matches
- Tag baseline - `v1.0.1 v1.2.0 v1.3.0`

## Next Phase Readiness

- 16a should treat an effect-specific external post-condition as higher-confidence than generic hook/process success, and should run it against the resolved primary project root.
- 16f must resolve the primary root before constructing `HookContext`; terminal correctness still depends on every hook receiving the checkout whose develop ref and release files are authoritative.
- No blockers remain for downstream Phase 16 plans.

## Self-Check: PASSED

- All three modified files exist.
- Task commits `af32f1a`, `a9e698e`, `9c22eed`, and `e711875` exist in repository history.
- No known stubs or unplanned threat surfaces were introduced.

---
*Phase: 16-pipeline-reliability-hardening*
*Completed: 2026-07-17*
