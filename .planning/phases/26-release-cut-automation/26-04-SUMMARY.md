---
phase: 26-release-cut-automation
plan: 04
subsystem: infra
tags: [git, release-automation, rust, cli, tdd]

# Dependency graph
requires:
  - phase: 26-release-cut-automation (26-03)
    provides: "GitFlow::push_ref (orphaned, zero non-test callers) and origin_main_ancestor_status, both consumed here"
provides:
  - "devflow_core::sync module: SyncError, SyncOutcome, SYNC_MERGE_MESSAGE, sync_main_to_develop"
  - "devflow sync CLI subcommand (Command::Sync -> commands::sync_cmd)"
  - "sync::tests::{init_repo, init_bare_remote} pub(crate) fixtures, the shared bare-remote test-fixture home for every post-26-03 core module"
  - "GitFlow::push_ref's first production caller"
affects: [26-06-release-executor, 26-07-gh-call-site-guard]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "One implementation, two entry points (D-07): a library function (sync_main_to_develop) called by both a standalone CLI subcommand and (in 26-06) an internal executor step"
    - "Bounded/neutralized git-stderr-in-error-payload discipline (SyncError::Git, NotOnDevelop.current) via version::sanitize_changelog_subject, mirroring release_tag_state (T-26-08)"
    - "Content-preserving merge with before/after tree-identity verification, refusing before push on mismatch (D-09), no automatic compensating action (D-05)"

key-files:
  created:
    - crates/devflow-core/src/sync.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - OPERATIONS.md

key-decisions:
  - "Test-fixture pushes route through GitFlow::push_ref rather than a raw `git push` argv, so sync.rs contains no bare push/force argv anywhere outside push_ref's own implementation in git.rs (an acceptance-criteria grep guard requires zero '\"push\"' matches in the whole file)."
  - "sync::tests is the new shared pub(crate) fixture home (init_repo/init_bare_remote) for every post-26-03 core module, per the plan's resolved design note — 26-06 imports from here rather than building a third copy."

requirements-completed: ["999.52"]

coverage:
  - id: D1
    description: "devflow sync end-to-end: happy-path content-preserving merge lands on a real (local, bare) remote via push_ref, CLI wired main.rs -> commands.rs -> sync.rs, help snapshot and doc_check invariants green"
    requirement: "999.52"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/sync.rs#sync::tests::pushes_on_success"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
    human_judgment: false
  - id: D2
    description: "The three entry refusals (dirty tree, wrong branch, already-synced no-op) all precede the fetch and mutate nothing"
    requirement: "999.52"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/sync.rs#sync::tests::refuses_on_dirty_tree"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/sync.rs#sync::tests::refuses_off_develop"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/sync.rs#sync::tests::noop_when_already_synced"
        status: pass
    human_judgment: false
  - id: D3
    description: "A content-changing merge is refused before push (D-09) and the remote develop ref is provably unmoved"
    requirement: "999.52"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/sync.rs#sync::tests::aborts_on_tree_mismatch"
        status: pass
    human_judgment: false

# Metrics
duration: 40min
completed: 2026-07-29
status: complete
---

# Phase 26 Plan 04: `devflow sync` — one implementation, two entry points Summary

**Rust port of `scripts/sync-main-to-develop.sh` as `devflow_core::sync::sync_main_to_develop`, wired as both a standalone `devflow sync` CLI subcommand and (per D-07) the future internal sync step for 26-06's release executor, landing `GitFlow::push_ref`'s first production caller.**

## Performance

- **Duration:** ~40 min
- **Started:** 2026-07-29T20:28:00-04:00 (approx.)
- **Completed:** 2026-07-29T20:54:01-04:00
- **Tasks:** 3
- **Files modified:** 6 (1 new, 5 modified)

## Accomplishments
- `crates/devflow-core/src/sync.rs`: `SyncError` (6 variants: `Io`, `Git`, `DirtyWorkingTree`, `NotOnDevelop`, `NoRemote`, `TreeChanged`), `SyncOutcome` (`AlreadyAncestor`, `Merged`), `SYNC_MERGE_MESSAGE`, and `sync_main_to_develop` — the ten-step ported sequence, short-circuiting at the first refusal, every subprocess call an argv array.
- `devflow sync` is a real, documented, `--help`-able CLI subcommand (`Command::Sync` in `main.rs` -> `commands::sync_cmd` -> `devflow_core::sync::sync_main_to_develop`) — proven against the real binary, not just a compile-time claim.
- The `-X ours` merge's before/after tree-identity check refuses to push a content-changing merge (D-09), leaving the local merge commit for the operator and never touching the remote — proven by `aborts_on_tree_mismatch` asserting the bare remote's `refs/heads/develop` is byte-identical before and after the refused call.
- `GitFlow::push_ref` (orphaned since 26-03, zero non-test callers) gets its first production caller here, both in `sync_main_to_develop`'s own success path and — deliberately — in this plan's own test fixtures, so no bare `git push` argv appears anywhere in `sync.rs` outside `push_ref`'s implementation in `git.rs`.
- 5 new tests (B1-B5 from `26-VALIDATION.md` Part B), all pinning behavior at the typed-variant/field level (never a rendered-message substring) and asserting on real remote-ref state, not just the returned `Result` variant.
- `OPERATIONS.md` documents `devflow sync`; `crates/devflow-cli/tests/snapshots/devflow-help.txt` regenerated; `doc_check::source_devflow_env_vars_and_subcommands_are_documented` and all 5 other `doc_check::` tests pass.

## Task Commits

Each task was committed atomically. Task 1 followed a genuine RED->GREEN cycle (tdd="true", tracer):

1. **Task 1: `devflow sync` end to end** — `22f17b1` (test, RED) then `7c9089e` (feat, GREEN)
   - RED: `sync.rs` scaffolding (types + fixtures + `pushes_on_success`) with `sync_main_to_develop` intentionally short-circuiting to `AlreadyAncestor`. Ran the test: failed with `left: AlreadyAncestor, right: Merged { .. }` — the correct failure shape, not a compile error.
   - GREEN: implemented the real ten-step sequence, wired the CLI (`main.rs`, `commands.rs`), regenerated the help snapshot, updated `OPERATIONS.md`. Test passes; task's full `<verify>` block (help_snapshot, doc_check::, `sync --help`, clippy, fmt) all green.
2. **Task 2: The three entry refusals (B1-B3)** — `af70ce0` (test)
   - `refuses_on_dirty_tree`, `refuses_off_develop`, `noop_when_already_synced`. These pin behavior Task 1's implementation already had (guards ordered before the fetch by construction) — see **TDD Gate Compliance** below for why this is not a strict RED/GREEN pair.
3. **Task 3: The tree-identity refusal (B4)** — `9134aa8` (test)
   - `aborts_on_tree_mismatch`. Also pins pre-existing Task 1 behavior (step 9's refusal). Full workspace verification re-run here: 422 passed / 0 failed (`--lib`, 417 baseline + 5), `cargo test -p devflow` 0 failed across every target, clippy/fmt clean.

**Plan metadata:** this commit (docs: complete plan) — pending, added after this SUMMARY.

## Files Created/Modified
- `crates/devflow-core/src/sync.rs` (new) — `SyncError`/`SyncOutcome`/`SYNC_MERGE_MESSAGE`/`sync_main_to_develop`, plus `sync::tests` (`init_repo`, `init_bare_remote` `pub(crate)`, 5 tests)
- `crates/devflow-core/src/lib.rs` — `pub mod sync;` declared alphabetically after `state`
- `crates/devflow-cli/src/main.rs` — `Command::Sync { project }` variant + dispatch arm + `sync_cmd` import
- `crates/devflow-cli/src/commands.rs` — `pub(crate) fn sync_cmd`
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated to include the `sync` row
- `OPERATIONS.md` — new `devflow sync` command-table row

## Decisions Made
- Routed all test-fixture pushes (initial baseline pushes, post-commit pushes, the second-clone push) through `GitFlow::push_ref` instead of a raw `git push` argv. This satisfies the acceptance criterion that `rg -n '"push"' crates/devflow-core/src/sync.rs` returns zero matches anywhere in the file (not just outside `#[cfg(test)]`), and additionally dogfoods `push_ref` in the test fixtures the same way `sync_main_to_develop` itself uses it in production.
- Reworded the module-level doc comment's "never `sh -c`" phrasing to "never a shell string interpolated through an intermediate shell process" — the literal substring `sh -c` inside an explanatory doc comment would have tripped the `rg -n 'sh -c|Command::new\("sh"\)'` acceptance-criteria guard, which does not distinguish comments from code.

## Deviations from Plan

None — plan executed as written. Two small self-corrections during execution (both caught by the plan's own acceptance-criteria greps before commit, not discovered later):
1. Test-fixture push calls initially used raw `git push` argv; rewritten to route through `push_ref` per the exact-file-scope grep guard (see Decisions Made above). No behavior change — same real `git push` invocation, now through the shared primitive.
2. The doc comment's `sh -c` phrase collided with its own no-shell-interpolation guard; reworded with identical meaning.

## TDD Gate Compliance

Task 1 (`tdd="true"`, `type="tracer"`) followed a genuine RED->GREEN cycle: `22f17b1` (test, RED — `pushes_on_success` failed for the correct reason: `AlreadyAncestor` returned instead of `Merged`) then `7c9089e` (feat, GREEN — the real implementation, test passes). No REFACTOR commit was needed.

Tasks 2 and 3 (also `tdd="true"`) each add a `test(...)`-only commit (`af70ce0`, `9134aa8`) pinning behavior that Task 1's implementation *already correctly exhibited* — the plan's own Task 2 action text frames this explicitly ("Confirm, and complete where the sequence written in Task 1 falls short..."). This is not a strict per-task RED/GREEN pair: the guards being tested were built once, in Task 1, as part of the ported ten-step sequence, and Tasks 2/3 are dedicated safety-property pinning tests rather than tests that drove new implementation. Reviewed against D-19's five requirements: requirement 1 ("a test that fails before this change") does not literally apply to these two tasks since no implementation changed — the value is requirement 5 (a test that *could* fail against a wrong implementation), which is met: each assertion checks real state (typed error variant + field, `HEAD` unchanged, remote ref unchanged, byte-identical trees) that a subtly-wrong ordering or a warn-and-push implementation would fail. Framed by the plan's own tracer-bullet structure: Task 1 builds the full working sequence end-to-end first; Tasks 2/3 then pin the individual safety properties (B1-B4) as dedicated, non-vacuous regression tests. No RED/GREEN gate is "missing" — the plan's own task boundaries put the one behavior-introducing RED/GREEN pair entirely in Task 1.

## Issues Encountered
None beyond the two self-corrections listed under Deviations, both resolved before any commit.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- `devflow_core::sync::sync_main_to_develop` is ready for 26-06's release executor to call directly as its own sync step (D-07) — no second implementation to build, no adapter needed.
- `sync::tests::{init_repo, init_bare_remote}` (`pub(crate)`) are the shared bare-remote test-fixture home 26-06 should import from rather than duplicating.
- No blockers. `crates/devflow-core/src/git.rs` was not touched (sibling plan 26-05 owns it in this wave), matching this plan's declared file scope.

---
*Phase: 26-release-cut-automation*
*Completed: 2026-07-29*

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/sync.rs`
- FOUND: `.planning/phases/26-release-cut-automation/26-04-SUMMARY.md`
- FOUND: commit `22f17b1` (test, RED)
- FOUND: commit `7c9089e` (feat, GREEN)
- FOUND: commit `af70ce0` (test, B1-B3)
- FOUND: commit `9134aa8` (test, B4)
