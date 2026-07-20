---
phase: 17-pipeline-dogfood-followup
plan: 02
subsystem: infra
tags: [build.rs, git, cargo, provenance, rustc-env]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup (plan 01)
    provides: typed-outcome taxonomy + pure outcome->action policy
provides:
  - "devflow-cli/build.rs — the workspace's first build script"
  - "Compile-time env vars DEVFLOW_BUILD_COMMIT, DEVFLOW_BUILD_DIRTY, DEVFLOW_BUILD_TIMESTAMP embedded via cargo:rustc-env"
  - "Absolute cargo:rerun-if-changed watches on the resolved git-common-dir's HEAD/refs/packed-refs"
affects: [17-05 (workflow_started payload + staleness check), 18d (doctor reconciliation)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Hand-rolled build.rs shelling to git via Command::new (argv-array form), zero build-dependencies, matching evaluate_layer2's existing git-subprocess idiom"
    - "git rev-parse --git-common-dir resolution before emitting rerun-if-changed, to avoid a relative .git/HEAD path being wrong for a nested workspace crate"

key-files:
  created:
    - crates/devflow-cli/build.rs
    - crates/devflow-cli/tests/build_provenance.rs
  modified: []

key-decisions:
  - "Resolved git-common-dir via `git rev-parse --git-common-dir` from CARGO_MANIFEST_DIR instead of the PATTERNS.md sketch's relative `.git/HEAD` (superseded per review consensus #7), emitting absolute rerun-if-changed paths for HEAD, refs, AND packed-refs"
  - "Skip rerun-if-changed lines entirely (not a broken relative path) when git-common-dir resolution fails, keeping the no-git build degradation clean"
  - "DEVFLOW_BUILD_TIMESTAMP is documented in-code as the build machine's wall-clock, not the commit timestamp — correct for Plan 05's mtime staleness comparison"

requirements-completed: [17d]

coverage:
  - id: D1
    description: "build.rs emits DEVFLOW_BUILD_COMMIT/DEVFLOW_BUILD_DIRTY/DEVFLOW_BUILD_TIMESTAMP via cargo:rustc-env, resolvable at runtime via env!"
    requirement: "17d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/build_provenance.rs#build_timestamp_is_a_parseable_u64"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/build_provenance.rs#build_dirty_is_exactly_true_or_false"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/build_provenance.rs#build_commit_is_accessible_and_does_not_panic"
        status: pass
    human_judgment: false
  - id: D2
    description: "Build script degrades gracefully (empty commit, false dirty, no rerun-if-changed lines) when git metadata is unavailable, and never fails the build"
    requirement: "17d"
    verification:
      - kind: manual_procedural
        ref: "Reproduced git rev-parse --git-common-dir failure in a scratch non-git directory (exit 128); confirmed run_git returns None on that path, which build.rs's Option handling already routes to empty/false defaults and a skipped rerun-if-changed block"
        status: pass
    human_judgment: false

# Metrics
duration: 2min
completed: 2026-07-18
status: complete
---

# Phase 17 Plan 02: Build Provenance Script Summary

**Hand-rolled `crates/devflow-cli/build.rs` — the workspace's first build script — embeds git commit/dirty/timestamp as compile-time env vars with zero new dependencies.**

## Performance

- **Duration:** 2 min
- **Started:** 2026-07-18T23:33:59Z
- **Completed:** 2026-07-18T23:35:57Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `crates/devflow-cli/build.rs` shells to `git` via `Command::new("git").args([...])` (argv form, never `sh -c`), resolves the repo's git-common-dir from `CARGO_MANIFEST_DIR`, and emits absolute `cargo:rerun-if-changed` paths for `HEAD`, `refs`, and `packed-refs`
- Emits `DEVFLOW_BUILD_COMMIT`, `DEVFLOW_BUILD_DIRTY`, `DEVFLOW_BUILD_TIMESTAMP` via `cargo:rustc-env`, degrading to empty/false/0 when git is unavailable (D-20) — never fails the build
- Integration test `crates/devflow-cli/tests/build_provenance.rs` asserts all three vars resolve via `env!` at compile time and tolerates the degraded no-git case

## Task Commits

Each task was committed atomically:

1. **Task 1: Hand-rolled build.rs emitting git provenance env vars (D-20)** - `1e34c3e` (feat)
2. **Task 2: Integration test asserting provenance env vars resolve** - `215447b` (test)

**Plan metadata:** _pending final docs commit_

## Files Created/Modified
- `crates/devflow-cli/build.rs` - Shells to git, resolves git-common-dir, emits provenance env vars via cargo:rustc-env; new file, workspace's first build script
- `crates/devflow-cli/tests/build_provenance.rs` - Integration test asserting the three env vars resolve and tolerate the no-git degraded case

## Decisions Made
- Followed the plan's explicit supersession of PATTERNS.md's relative `.git/HEAD` sketch: resolved the actual git-common-dir via `git rev-parse --git-common-dir` and emitted absolute rerun-if-changed paths (review consensus #7)
- Included `packed-refs` in the rerun-if-changed set alongside `HEAD`/`refs`, per the plan's explicit requirement, so packed-ref/fetch-only ref movement also triggers a rebuild
- Ran `cargo fmt -p devflow` after writing `build.rs` to match the workspace's existing formatting convention (code-style rule); no logic change, purely the multi-line `Command` builder wrap

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None. Both tasks compiled, linted (`cargo clippy -p devflow -- -D warnings`), and tested clean on the first pass; the only follow-up action was running `cargo fmt` to match the existing wrapping convention, which is standard practice, not a plan deviation.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- `DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY`/`DEVFLOW_BUILD_TIMESTAMP` are live and ready for Plan 05 to read via `env!` for the `workflow_started` payload and staleness check
- No blockers

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-18*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/build.rs
- FOUND: crates/devflow-cli/tests/build_provenance.rs
- FOUND: .planning/phases/17-pipeline-dogfood-followup/17-02-SUMMARY.md
- FOUND: commit 1e34c3e
- FOUND: commit 215447b
