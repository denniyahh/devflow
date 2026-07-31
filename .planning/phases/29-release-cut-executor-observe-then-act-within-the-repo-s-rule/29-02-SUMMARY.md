---
phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
plan: 02
subsystem: release-automation
tags: [rust, git, gh-cli, cli, release]

# Dependency graph
requires:
  - phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
    provides: "29-01's Observation/ReleaseStep/TagRefs/TagSignature vocabulary, signed_tag_on_remote, crates_published, release_observation_check, and the release_status CLI handler"
provides:
  - "The remaining four of six release-cut oracles: version_bumped_on_develop, changelog_written_on_develop, release_pr_merged_to_main, sync_merged"
  - "devflow_core::release_observe::observe/observe_all — the single exhaustive dispatcher over all six ReleaseStep variants, no wildcard arm"
  - "devflow release status <version> is now a complete, shippable six-row report, mutating nothing and recording nothing"
affects: [29-03-plan-recoverable-actions, 29-04, 29-05, 29-06, 29-07]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "gh api repos/{owner}/{repo}/contents/<path>?ref=<git_ref> with Accept: application/vnd.github.raw reads a file's raw bytes at any ref without a local fetch/base64 decode step"
    - "The observable is the outcome (does main's Cargo.toml carry version X), never the PR object — querying gh pr list would make the answer depend on a mutable GitHub search index (RD-8)"
    - "sync_merged (remote compare/main...develop via gh) is deliberately a separate function from git::origin_main_ancestor_status (local, already-fetched refs, no network) — collapsing them would silently break release --check's documented no-fetch contract"
    - "observe()'s match has no wildcard arm — a 7th ReleaseStep variant fails to compile rather than silently defaulting; verified live by temporarily adding one and confirming E0004"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/release_observe.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/release_status.rs

key-decisions:
  - "file_at_ref's and sync_merged's Unreachable reasons intentionally name the path/ref/endpoint queried (e.g. 'gh api exited with status 1 fetching Cargo.toml@develop') rather than a generic string — this is what makes observe_dispatches_all_six_variants_to_distinct_unreachable_reasons_with_no_remote's distinctness assertion meaningful, and does not violate T-29-03/T-17-13 since path/ref are non-sensitive metadata, never gh's raw stdout/stderr."
  - "status_leaves_the_repository_untouched needed a real origin remote (not a no-remote fixture) for its FETCH_HEAD-presence mutation-testing check to be meaningful — a fetch against a non-existent remote is a silent no-op that would let the test pass even after a real regression."

patterns-established:
  - "observe(project_root, step, version) -> Observation: the single dispatcher every future call site (29b/29c's own preflight checks) should call instead of invoking an individual oracle function directly."

requirements-completed: [29a]

coverage:
  - id: D1
    description: "devflow release status <version> answers the remaining four release-cut questions (version bumped, changelog written, release PR merged, sync merged) from GitHub's remote copy, three-valued, Unreachable never collapsed into Absent"
    requirement: "29a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_observe.rs#release_observe::tests (18 cases covering classify_manifest_version/classify_changelog_heading/classify_compare_status/observe/observe_all)"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_status.rs#status_reports_unreachable_not_absent_without_a_remote"
        status: pass
    human_judgment: false
  - id: D2
    description: "All six release-cut questions are answered by one exhaustive dispatcher (observe/observe_all) with no wildcard arm, and release_status builds its check list from it alone"
    requirement: "29a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_observe.rs#release_observe::tests::observe_dispatches_all_six_variants_to_distinct_unreachable_reasons_with_no_remote"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/release_observe.rs#release_observe::tests::observe_all_preserves_release_step_all_order"
        status: pass
      - kind: manual_procedural
        ref: "temporarily added a 7th ReleaseStep variant, confirmed cargo check -p devflow-core fails with E0004 (non-exhaustive match), reverted"
        status: pass
    human_judgment: false
  - id: D3
    description: "The observer provably mutates nothing (HEAD, working tree, index, full ref listing, FETCH_HEAD, .devflow/, devflow.toml all byte-identical before/after) and requires no authorization flag"
    requirement: "29a"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_status.rs#status_leaves_the_repository_untouched"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_status.rs#status_rejects_an_authorization_flag"
        status: pass
      - kind: manual_procedural
        ref: "temporarily inserted a git fetch into signed_tag_on_remote, confirmed status_leaves_the_repository_untouched goes red (FETCH_HEAD appears), reverted"
        status: pass
    human_judgment: false

duration: 35min
completed: 2026-07-31
status: complete
---

# Phase 29 Plan 02: Release-Cut Executor — Remaining Four Oracles + Complete Six-Row Report Summary

**`devflow release status <version>` is now complete: four new oracles (version bump, changelog, release PR, sync-back) plus a single exhaustive dispatcher over all six `ReleaseStep` variants, reproducing the real v2.2.0 incident (5/6 present, signed tag absent) in one live report.**

## Performance

- **Duration:** ~35 min
- **Completed:** 2026-07-31
- **Tasks:** 3 completed (content-at-ref oracles, sync-ancestry oracle + dispatcher, invariant tests)
- **Files modified:** 3

## Accomplishments

- `devflow_core::release_observe` now answers all six release-cut questions: `version_bumped_on_develop`, `changelog_written_on_develop`, `release_pr_merged_to_main` (Task 1, content read at a git ref via `gh api .../contents`), and `sync_merged` (Task 2, `gh api .../compare/main...develop`), joining Plan 01's `signed_tag_on_remote` and `crates_published`.
- `observe(project_root, step, version)` is the single exhaustive dispatcher over `ReleaseStep` — no wildcard arm, verified by temporarily adding a 7th variant and confirming `cargo check` fails to compile (E0004). `observe_all` maps `ReleaseStep::ALL` through it, and `commands.rs`'s `release_status` now builds its entire check list from `observe_all` alone — exactly one place knows the question list.
- Live run against this repo reproduces the real v2.2.0 incident precisely: `devflow release status 2.2.0 .` reports 5/6 present, with only the signed-tag row absent — the exact motivating scenario for this phase.
- Three new invariant tests prove unit 29a is safe to ship standalone: the observer mutates nothing (HEAD, working tree, ref listing, `FETCH_HEAD`, `.devflow/`, `devflow.toml` all byte-identical before/after — mutation-tested by temporarily reintroducing a `git fetch` and confirming the test catches it), accepts no authorization flag, and with no remote at all reports zero absent-shaped rows (only unreachable/failed).

## Task Commits

Each task followed a RED (failing test) → GREEN (implementation) TDD cycle, per its `tdd="true"` attribute:

1. **Task 1: content-at-ref oracles (version/changelog/release-PR)**
   - `c4d352b` `test(29-02): add failing tests for manifest/changelog classifiers` — RED: `classify_manifest_version`/`classify_changelog_heading` stubbed to always return `Unreachable`, 8/29 tests failed for the intended reason
   - `2825fae` `feat(29-02): content-at-ref oracles for version/changelog/release-PR (29a)` — GREEN: `file_at_ref` I/O primitive, real classifiers, three I/O wrappers wired into `release_status`'s check list

2. **Task 2: sync-ancestry oracle and the complete dispatcher**
   - `1d19b3e` `test(29-02): add failing tests for compare-status classifier and dispatcher` — RED: `classify_compare_status`/`observe` stubbed wrong, 6/36 tests failed for the intended reason
   - `9a9bf17` `feat(29-02): sync-ancestry oracle and the complete six-row dispatcher (29a)` — GREEN: real `classify_compare_status`, `sync_merged`, exhaustive `observe`/`observe_all`; `release_status` rewritten to build its check list from `observe_all` alone

3. **Task 3: invariant tests proving 29a is shippable standalone**
   - `3d9952b` `test(29-02): invariants proving the observer mutates and records nothing` — test-only task (no paired implementation change); three new hermetic tests added and both mutation-testing confirmations (fetch-insertion, wildcard-arm addition) run live and reverted before commit

## Files Created/Modified

- `crates/devflow-core/src/release_observe.rs` — added `file_at_ref` (private I/O primitive), `classify_manifest_version`, `classify_changelog_heading`, `classify_compare_status` (pure classifiers), `version_bumped_on_develop`, `changelog_written_on_develop`, `release_pr_merged_to_main`, `sync_merged` (I/O wrappers), `observe`, `observe_all` (dispatcher), plus 18 new unit tests (36 total in the module)
- `crates/devflow-cli/src/commands.rs` — `release_status` rewritten to build its entire `Check` list from `observe_all(project_root, version)` in one pass, replacing the per-question `vec![...]` construction Task 1 temporarily added
- `crates/devflow-cli/tests/release_status.rs` — updated `release_status_summary_line_names_version_and_count` for the new 6-row report (was `0/2`, now `0/6`); added `status_leaves_the_repository_untouched`, `status_rejects_an_authorization_flag`, `status_reports_unreachable_not_absent_without_a_remote`; added `commit`/`rev_parse`/`git_status_porcelain`/`sorted_ref_listing` fixture helpers; added a module-level doc comment recording the live-remote-behavior test boundary

## Decisions Made

- **`file_at_ref`/`sync_merged` error messages name the resource queried, not a generic string.** The plan's own behavior bullet requires `observe`'s six Unreachable reasons to be *distinct* when nothing is reachable. Since three oracles (`VersionBumped`, `ChangelogWritten`, `ReleasePrMerged`) all funnel through the same `file_at_ref` I/O primitive, a bare "gh api exited with status 1" would have collided across all three. Including the queried `path`/`git_ref` (e.g. `Cargo.toml@develop` vs. `Cargo.toml@main`) in the failure text makes each reason unique without violating T-29-03/T-17-13 — path and ref are non-sensitive metadata, never `gh`'s raw stdout/stderr.
- **`status_leaves_the_repository_untouched` needed a real `origin` remote.** An early draft of this test used a fixture with no remote at all, matching the plan's own `<action>` text loosely. Manually verifying the fetch-insertion mutation-test step revealed this was insufficient: `git fetch origin` against a nonexistent remote is a silent no-op, so the test would have passed even with a real regression reintroducing a fetch. Reworked the fixture to include a real local bare-repo origin (same pattern as `release_status_absent_tag_on_reachable_remote_warns`), and added an explicit `.git/FETCH_HEAD` existence check (a fetch always writes this file regardless of whether any ref content changed) as the primary, git-version-independent signal — the `for-each-ref` snapshot alone proved unreliable across different remote-branch topologies during manual verification.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `release_status_summary_line_names_version_and_count` asserted a stale row count**
- **Found during:** Task 2, running `cargo test -p devflow release_status` after wiring the full dispatcher
- **Issue:** This pre-existing test (written in 29-01, already reworked once for Task 1's 2-row world) asserted `"0/2"` in the summary line. Task 2's own scope — "the full six-row report" — necessarily invalidated that count.
- **Fix:** Updated the assertion to `"0/6"` with an updated explanatory message naming all six questions.
- **Files modified:** `crates/devflow-cli/tests/release_status.rs`
- **Verification:** `cargo test -p devflow release_status` — 3/3 passed in that test binary.
- **Committed in:** `9a9bf17` (part of the GREEN commit for Task 2)

---

**Total deviations:** 1 auto-fixed (1 Rule 1 bug — a direct, expected consequence of Task 2's own stated scope, not scope creep).
**Impact on plan:** No design change; the fix was necessary for the pre-existing test to correctly reflect the six-row report this plan's own `<action>` text specifies.

## Issues Encountered

- **`refs/remotes/origin/HEAD` behavior during manual mutation-test verification was git-version-dependent and non-deterministic across fixture shapes** (observed appearing after a fetch in a two-branch-remote fixture but not in a single-branch one). Rather than build the invariant test's core assertion on this git-internal behavior, switched to checking `.git/FETCH_HEAD` existence directly, which `git fetch` always writes regardless of ref-content changes — a more robust, deterministic signal. Resolved before commit; no residual issue.
- **Pre-existing `cargo test -p devflow-core <filter>` (no `--features test-support`) compile failure** (unrelated to this plan, already documented in `29-01-SUMMARY.md`'s Issues Encountered): two pre-existing integration test files reference `devflow_core::test_support`, which needs the `test-support` feature when the package is tested in isolation rather than via `--workspace`. All scoped verification in this plan used `--features test-support`; the plan's own final gate (`cargo test --workspace`) unifies features and is fully green. No code change made.

## User Setup Required

None — no external service configuration required. `gh` and network reachability to `github.com` were already available and used for live verification (not a new requirement introduced by this plan).

## Next Phase Readiness

- Unit 29a is complete and independently shippable: all six release-cut questions answered from remote sources, three-valued throughout, refusing on unreachable, mutating nothing, recording nothing, requiring no authorization flag.
- `29-03-PLAN.md` (recoverable actions — version bump, changelog write, release PR, sync PR) can now call `observe`/`observe_all` as its own pre-action "is this already done" check, per the pattern established here, rather than inventing a parallel check.
- No blockers. `sync_merged`'s doc comment explicitly documents why it must remain distinct from `git::origin_main_ancestor_status` — a future contributor tempted to consolidate them should read that rationale first, since collapsing them would silently break `release --check`'s no-fetch contract (20d).

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_observe.rs`
- FOUND: `crates/devflow-cli/src/commands.rs`
- FOUND: `crates/devflow-cli/tests/release_status.rs`
- FOUND commit `c4d352b` (test: manifest/changelog classifiers, RED)
- FOUND commit `2825fae` (feat: content-at-ref oracles, GREEN)
- FOUND commit `1d19b3e` (test: compare-status classifier and dispatcher, RED)
- FOUND commit `9a9bf17` (feat: sync-ancestry oracle + complete dispatcher, GREEN)
- FOUND commit `3d9952b` (test: invariants proving no mutation/no recording)
