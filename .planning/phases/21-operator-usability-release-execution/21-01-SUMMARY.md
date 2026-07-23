---
phase: 21-operator-usability-release-execution
plan: 01
subsystem: infra
tags: [rust, git, staleness, self-dogfood, build-provenance]

# Dependency graph
requires:
  - phase: 18-dogfood-reliability-hardening
    provides: worktree-aware build staleness enforcement (18c, execution_root)
  - phase: 17-pipeline-dogfood-follow-up
    provides: build provenance + self-dogfood staleness gate (17d), affects_compiled_binary (17-10)
provides:
  - "ancestry_range_affects_build(execution_root, embedded_commit) -> bool — content-aware filter on the strict-ancestor arm"
  - "embedded_commit_is_stale's Ok(Some(0)) strict-ancestor arm now Fresh for docs-only ranges, Stale for build-relevant ranges"
  - "Reworded self-dogfood block message describing build-relevant-change condition instead of misstated non-ancestry"
affects: [21-02, 21-03, 21-04, future dogfood-loop phases]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Content-aware ancestry arm: diff the committed range with git diff --name-only, filter through the existing affects_compiled_binary predicate, never fork a new file-extension matcher"
    - "Fail-toward-Stale on any git subprocess failure — a git error in a safety gate must never resolve to a false Fresh"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/staleness.rs

key-decisions:
  - "Reused affects_compiled_binary verbatim (D-07) rather than forking a second file-extension matcher for the ancestry range"
  - "Left the Ok(Some(1)) reverse-probe arm and Indeterminate fallbacks behaviorally untouched — only the Ok(Some(0)) Some(_) line changed"
  - "Retargeted wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks's follow-up commit from b.txt to src/main.rs, and init_repo_with_diverged_commit's trunk2 commit from trunk2.txt to trunk2.rs, so both fixtures stay build-affecting under the new content-aware semantics"
  - "Added git_error_range_fails_toward_stale by deleting the embedded commit's root tree object (not the commit object) — merge-base --is-ancestor needs only commit objects and keeps succeeding, while git diff --name-only fails, proving the fail-toward-Stale posture end-to-end"

patterns-established:
  - "Content-filtered ancestry check: any future 'diff a committed range' check in this codebase should filter through affects_compiled_binary rather than reinventing file-extension matching"

requirements-completed: [21d, D-07]

coverage:
  - id: D1
    description: "Docs-only strict-ancestor range (e.g. a .planning/*.md-only commit ahead of the embedded build) now classifies Fresh instead of hard-blocking"
    requirement: "21d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::docs_only_range_is_fresh"
        status: pass
    human_judgment: false
  - id: D2
    description: "Mixed range (docs + nested .rs file) and any real source/build-config change still classifies Stale and hard-blocks self-dogfood — Phase 16 false-evidence protection preserved"
    requirement: "21d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::mixed_range_docs_and_source_is_stale"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::embedded_commit_is_stale_maps_ancestry_exit_codes"
        status: pass
    human_judgment: false
  - id: D3
    description: "A git failure while diffing the ancestry range fails toward Stale, never a false Fresh — the fail-toward-Stale safety posture, previously stated but untested"
    requirement: "21d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#staleness::tests::git_error_range_fails_toward_stale"
        status: pass
    human_judgment: false
  - id: D4
    description: "affects_compiled_binary is reused verbatim by the new ancestry helper, not forked or reimplemented"
    requirement: "21d"
    verification:
      - kind: other
        ref: "rg -n \"fn ancestry_range_affects_build\" crates/devflow-cli/src/staleness.rs (exactly one hit); rg -c BUILD_AFFECTING_FILES (exactly 2 — definition + single existing usage site)"
        status: pass
    human_judgment: false

duration: 9min
completed: 2026-07-23
status: complete
---

# Phase 21 Plan 01: Content-Aware Self-Dogfood Staleness Ancestry Summary

**Made `embedded_commit_is_stale`'s strict-ancestor arm content-aware by filtering `git diff --name-only <embedded> HEAD` through the existing `affects_compiled_binary` predicate, so a docs-only commit range no longer re-arms the self-dogfood hard block after every `.planning/` commit.**

## Performance

- **Duration:** 9 min
- **Started:** 2026-07-23T16:23:47-04:00
- **Completed:** 2026-07-23T16:32:04-04:00
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- New private helper `ancestry_range_affects_build(execution_root, embedded_commit) -> bool` diffs the committed range and filters through `affects_compiled_binary`, reused verbatim; fails toward `true` (Stale) on any git error
- `embedded_commit_is_stale`'s `Ok(Some(0))` strict-ancestor arm now content-aware: docs-only ranges → `Fresh`, any build-relevant range → `Stale`
- Retired the false-positive hard-block observed live during this phase's own launch (binary at `7163347`, worktree HEAD `3a17381`, delta `.planning/*` only, previously hard-blocked)
- Real-change protection proven preserved: a mixed docs+nested-`.rs` range still hard-blocks; two pre-existing strict-ancestor fixtures (`wr01_…`, `embedded_commit_is_stale_maps_ancestry_exit_codes`) retargeted to build-affecting files and still pass
- New `git_error_range_fails_toward_stale` test closes a previously stated-but-untested safety posture: a git failure diffing the range still yields `Stale`, never a false `Fresh`
- Reworded the self-dogfood block message to describe a build-relevant-change condition instead of misstating non-ancestry for the common ancestor-but-behind case
- `Ok(Some(1))` reverse-probe arm and `Indeterminate` fallbacks left behaviorally untouched (D-07 prohibition)

## Task Commits

Each task was committed atomically:

1. **Task 1: Content-aware strict-ancestor arm (the end-to-end narrowing)** - `24519d7` (feat) — TDD: `docs_only_range_is_fresh` written and confirmed RED (`left: Stale, right: Fresh`) against the unmodified arm, then GREEN after the fix
2. **Task 2: Preserve real-change protection — repair fixtures, add mixed-range + git-error tests, fix block message** - `cf0214a` (test)

**Plan metadata:** SUMMARY commit pending (this file)

## Files Created/Modified
- `crates/devflow-cli/src/staleness.rs` - New `ancestry_range_affects_build` helper; narrowed `embedded_commit_is_stale`'s strict-ancestor arm; retargeted two fixtures; added `mixed_range_docs_and_source_is_stale` and `git_error_range_fails_toward_stale` tests; reworded `enforce_build_staleness`'s block message

## Decisions Made
- Reused `affects_compiled_binary` verbatim rather than forking a second file-extension matcher (D-07 prohibition, verified: `fn ancestry_range_affects_build` has exactly one definition; `BUILD_AFFECTING_FILES` appears exactly twice — definition + its one pre-existing usage site inside `affects_compiled_binary`)
- Forced the git-error test by deleting only the embedded commit's root TREE object (not its COMMIT object): `git merge-base --is-ancestor` is pure commit-graph traversal and keeps succeeding, while `git diff --name-only` needs the tree and fails — this isolates a git failure specifically inside the new ancestry-range diff without also breaking the ancestry check itself
- Left the `Ok(Some(1))` reverse-probe arm and `_ => Indeterminate` fallback byte-identical; `git diff` on the final state confirms only the `Some(_)` line of the `Ok(Some(0))` arm changed

## Deviations from Plan

None — plan executed exactly as written. Both tasks' `<action>` and `<acceptance_criteria>` were followed literally; no Rule 1-4 auto-fixes were needed.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Wave 1 (this plan) is done; 21-02/21-03/21-04 (waves depending on this landing first per D-07 sequencing) are unblocked
- The no-op-rebuild tax for the rest of Phase 21's own Plan/Code/Validate stages after every `.planning/` commit is removed
- `cargo test --workspace` 329/329 (0 failed) in the `devflow` unittests target (18 staleness tests, up from 16), clippy `-p devflow --all-targets -- -D warnings` clean, `cargo fmt --check` clean

---
*Phase: 21-operator-usability-release-execution*
*Completed: 2026-07-23*
