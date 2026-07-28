---
phase: 25-end-to-end-dogfood-blockers
plan: 09
subsystem: versioning
tags: [rust, git, semver, conventional-commits, release-anchor]

# Dependency graph
requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: "25-01's compute_version / release_range_start (D-08 range anchor)"
provides:
  - "release_range_start walks the whole ancestry path instead of only its first commit, closing CR-03"
  - "two real-git topology regression fixtures: intervening-trunk-hotfix and post-sync feature-merge"
affects: [preflight.rs::preflight_major_bump_check (25-08 — same anchor function, untouched here)]

# Tech tracking
tech-stack:
  added: []
  patterns: ["ancestry-path walk with early-exit on first non-descendant first-parent, replacing a first-line-only heuristic"]

key-files:
  created: []
  modified:
    - crates/devflow-core/src/version.rs

key-decisions:
  - "Deliberately did NOT implement 25-REVIEW.md/25-VERIFICATION.md's literal 'anchor at the last merge commit' sketch — measured live against this repo's actual v2.0.0..develop history, that sketch returns the wrong commit (819987b instead of c92229e) and would silently drop a commit from classification, which is a worse defect than CR-03 itself."
  - "Fixed Task 1's tripwire fixture construction (an intervening 'chore' commit was inserted between the sync merge and the feature branch's creation) after discovering the literal steps in 25-09-PLAN.md do not reproduce the required pre-fix GREEN state — see Deviations."

requirements-completed:
  - "25c (999.49 / DEN-74) — CR-03: release_range_start silently reverts to the naive tag..HEAD range"
  - "25c (999.49 / DEN-74) / D-08 amendment — the commit-range anchor must hold across realistic release topologies"

coverage:
  - id: D1
    description: "release_range_start no longer collapses to the naive tag..HEAD range when a commit lands directly on trunk between the tag and the sync-merge-back (CR-03 fixed)"
    requirement: "25c (999.49 / DEN-74) — CR-03"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge"
        status: pass
    human_judgment: false
  - id: D2
    description: "A later --no-ff feature-branch merge landing on develop after the sync merge does not move the anchor (tripwire against the review's 'last merge commit' sketch)"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::feature_merge_after_sync_merge_does_not_move_the_anchor"
        status: pass
    human_judgment: false
  - id: D3
    description: "The change is behavior-preserving on this repository's own live history: release_range_start(repo_root, \"v2.0.0\") resolves to the same sync-merge commit (c92229e) before and after the fix"
    requirement: "25c (999.49 / DEN-74) / D-08 amendment"
    verification:
      - kind: unit
        ref: "manual live-history reproduction (git rev-list/merge-base) + throwaway release_range_start(root, \"v2.0.0\") call against this repo's own root, both confirming c92229e — see Live-History Check below"
        status: pass
    human_judgment: false

# Metrics
duration: 45min
completed: 2026-07-28
status: complete
---

# Phase 25 Plan 09: Generalize `release_range_start`'s anchor to the full ancestry path Summary

**Closed CR-03 by walking the entire `git rev-list --ancestry-path` instead of inspecting only its first commit — an intervening trunk hotfix or a later `--no-ff` feature merge can no longer collapse the D-08 classification range back to the naive `tag..HEAD`.**

## Performance

- **Duration:** ~45 min
- **Started:** 2026-07-28T00:15:00Z (approx.)
- **Completed:** 2026-07-28T00:25:00Z (approx.)
- **Tasks:** 2/2
- **Files modified:** 1 (`crates/devflow-core/src/version.rs`)

## Accomplishments

- Added two real-git topology regression fixtures to `version.rs`'s test suite:
  - `trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge` — reproduces CR-03 with a hotfix commit pushed straight to trunk between the baseline tag and the sync-merge-back.
  - `feature_merge_after_sync_merge_does_not_move_the_anchor` — tripwire pinning this plan's deliberate deviation from the review's "anchor at the last merge commit" fix sketch.
- Added test helpers `merge_no_ff` (mirrors `GitFlow::merge_feature_into_develop`'s `--no-ff` merge shape) and `head_sha`.
- Replaced `release_range_start`'s `C1`-only heuristic with a full-ancestry-path walk that returns the first commit whose first parent is not a descendant of the baseline tag, falling back to the tag when no candidate qualifies. Extracted a new private helper `first_parent`.
- Rewrote `release_range_start`'s `///` doc comment ("Anchor rule" section) to describe the new rule, name CR-03, and explicitly warn that anchoring at the last merge commit is wrong on this repository (because `GitFlow::merge_feature_into_develop` merges every phase branch with `--no-ff`).
- Confirmed the change is behavior-preserving on this repository's own live history: the anchor for `v2.0.0` resolves to `c92229e` both via manual git reproduction and via a direct (throwaway, uncommitted) call to `release_range_start` against this repo's own root.

## Task Commits

Each task was committed atomically:

1. **Task 1: The two topology fixtures — the RED one for CR-03 and the tripwire for the sketch** - `6e34287` (test)
2. **Task 2: Generalize the anchor rule to the whole ancestry path** - `7dcfa4f` (fix)

**Plan metadata:** (this commit, made by the orchestrator after wave merge — not made by this worktree agent)

## Files Created/Modified

- `crates/devflow-core/src/version.rs` — `release_range_start` algorithm replaced (signature unchanged), new private `first_parent` helper, new test helpers `merge_no_ff`/`head_sha`, two new topology tests.

## RED Output (Task 1, pre-fix)

```
$ cargo test --package devflow-core --lib version::tests::trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge -- --exact
running 1 test
test version::tests::trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge ... FAILED

---- version::tests::trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge stdout ----
thread '...' panicked at crates/devflow-core/src/version.rs:1254:9:
assertion `left == right` failed: anchor must be the sync merge, not the hotfix's tag-ancestor first parent
  left: "v2.0.0"
 right: "035aa7c537523731a81b4f91d312bf7b38acd7ac"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 388 filtered out
```

Tripwire, same pre-fix state:

```
$ cargo test --package devflow-core --lib version::tests::feature_merge_after_sync_merge_does_not_move_the_anchor -- --exact
running 1 test
test version::tests::feature_merge_after_sync_merge_does_not_move_the_anchor ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 388 filtered out
```

Whole `version::` suite pre-fix: `37 passed; 1 failed` — exactly the deliberately-red fixture.

## GREEN Output (Task 2, post-fix)

```
$ cargo test --package devflow-core --lib version:: 2>&1 | tail -5
...
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 351 filtered out; finished in 0.60s
```

Both new fixtures individually:

```
$ cargo test --package devflow-core --lib version::tests::trunk_commit_between_tag_and_sync_merge_still_anchors_at_the_sync_merge -- --exact
test result: ok. 1 passed; 0 failed

$ cargo test --package devflow-core --lib version::tests::feature_merge_after_sync_merge_does_not_move_the_anchor -- --exact
test result: ok. 1 passed; 0 failed
```

Both pre-existing topology tests, unchanged and still green:

```
$ cargo test --package devflow-core --lib version::tests::squash_sync_topology_classifies_only_post_merge_commits -- --exact
test result: ok. 1 passed; 0 failed

$ cargo test --package devflow-core --lib version::tests::two_squash_sync_cycles_anchor_to_the_second_merge_only -- --exact
test result: ok. 1 passed; 0 failed
```

`cargo clippy --workspace --all-targets -- -D warnings`: exit 0, no warnings.
`cargo fmt --check`: exit 0.

## Live-History Check

Per `<verification>` step 4: `git rev-list --ancestry-path --reverse v2.0.0..develop`, then for each commit `C`, `git merge-base --is-ancestor v2.0.0 "$(git rev-parse "$C^1")"`:

| # | commit | subject | first_parent | `is-ancestor` exit |
|---|--------|---------|---------------|---------------------|
| 1 | `c92229e` | `merge: sync main back into develop after release` | `d2831a1` | `1` (NOT ancestor → **qualifies, anchor**) |
| 2 | `c20b198` | `Merge pull request #45 from …/sync/main-to-develop-v2.0.0-retry` | `d2831a1` | `1` (not reached — loop returns at #1) |
| 3 | `7c6fcbf` | `docs: file 999.52 sync gap, re-size 25b …` | `c20b198` | `0` (ancestor) |
| 4 | `819987b` | `Merge pull request #46 from …/fix/sync-gap-and-25b-sizing` | `c20b198` | `0` (ancestor) |

The first commit with a non-zero exit is `c92229e` — matching the required anchor exactly.

Additionally verified directly, not just by manual reproduction: a throwaway (uncommitted, reverted before this commit) unit test calling `release_range_start(&this_repo_root, "v2.0.0")` printed:

```
LIVE-HISTORY ANCHOR: c92229ed39e7ca4d678bd88c2bebe17e3b856271
test version::tests::throwaway_live_history_check_25_09 ... ok
```

confirming the actual function — not just the manual git-command reproduction — resolves to `c92229e`, identical to what the pre-change code produces on this repository's real history. The throwaway test was reverted; it is not part of any committed diff.

## Full Workspace Suite (`<verification>` step 5)

`cargo test --workspace --no-fail-fast`:

| Binary | Passed | Failed |
|---|---|---|
| `devflow` (cli unittests) | 217 | 0 |
| `build_provenance` | 3 | 0 |
| `ci_parity_guards` | 7 | 0 |
| `gate_sweep_e2e` | 4 | 0 |
| `git_env_hermeticity` | 1 | 0 |
| `gitignore_coverage` | 1 | 0 |
| `help_snapshot` | 1 | 0 |
| `log_format_env` | 3 | 0 |
| `phase7_cli` | 17 | 0 |
| `pre_push_signing_policy` | 5 | 0 |
| `reap_strays_e2e` | 2 | 0 |
| `release_check` | 10 | 0 |
| `start_reachability_e2e` | 2 | 0 |
| `stop_e2e` | 9 | 0 |
| `workspace_version_pin` | 1 | 0 |
| `devflow-core` (lib unittests) | 389 | 0 |
| `devflow_dir_gitignore` | 2 | 0 |
| `monitor_e2e` | 2 | 0 |

**Total: 676 passed, 0 failed.** No failure to attribute; nothing deferred to `deferred-items.md`.

## Decisions Made

1. **Did not implement the review's "anchor at the last merge commit" sketch.** `25-REVIEW.md` CR-03's sketch, echoed in `25-VERIFICATION.md`'s `missing:` list, says to walk the ancestry path and anchor at the *last* merge commit. `GitFlow::merge_feature_into_develop` (`git.rs:86`) merges every phase branch into `develop` with `--no-ff`, so ordinary post-release feature work also produces merge commits on the ancestry path. Measured live against `v2.0.0..develop` (2026-07-28): the correct anchor is `c92229e`, but the literal sketch would return `819987b` (a later, unrelated PR merge), silently dropping `7c6fcbf` from classification — today a harmless `docs:` commit, but a `feat!:` in that position would be dropped unnoticed, exactly the false negative D-09 exists to prevent. Implemented instead: a strict generalization of the CURRENT predicate — the same first-parent ancestor test, applied to every commit on the path in order, returning the first one that fails it.
2. **Fixed Task 1's tripwire fixture construction.** As literally specified in `25-09-PLAN.md`, the second test's steps (branch `feature/phase-99` off `develop` immediately after the sync merge, no intervening commit) produce a fixture where `git rev-list --ancestry-path --reverse` places the feature branch's single-parent commit (`ft1`) *before* the sync merge commit itself in its output — a measured, deterministic property of that exact graph shape (confirmed via a standalone bash reproduction outside the Rust test, run twice for stability), not test flakiness. Under that ordering the CURRENT (pre-fix) code's `C1`-only heuristic evaluates `ft1` as `C1`, whose first parent is the sync merge — which the tag IS an ancestor of (directly, via the sync merge's second parent) — so the pre-fix function wrongly returns the bare tag, not the sync merge. This makes the fixture as literally written FAIL pre-fix, contradicting the plan's required "Test 2 must PASS pre-fix" state. Per the plan's own explicit contingency instruction ("If Test 2 fails pre-fix, the fixture is malformed … stop and fix it"), one ordinary intervening `chore:` commit was inserted on `develop` between the sync merge and the feature branch's creation. This is itself realistic (post-release `develop` work commonly precedes the next feature branch) and restores `C1 == sync merge` under the current implementation without changing either of the test's assertions or its intent.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug in test fixture] Fixed Task 1's Test 2 (`feature_merge_after_sync_merge_does_not_move_the_anchor`) construction so it reproduces the required pre-fix GREEN state**
- **Found during:** Task 1, confirming the "expected pre-fix states" per the plan's own instructions
- **Issue:** The fixture as literally specified in `25-09-PLAN.md` (checkout the feature branch directly off `develop` right after the sync merge, with no intervening commit) causes `git rev-list --ancestry-path --reverse` to list the feature branch's tip commit before the sync merge commit, making the pre-fix `C1`-only code return the bare baseline tag instead of the sync merge — the test FAILED pre-fix instead of PASSING, per the plan's own stated contingency
- **Fix:** Inserted one intervening `commit_msg(root, "tail.txt", "chore: continue develop work after sync")` on `develop` between capturing `sync_merge` and branching `feature/phase-99`. This breaks the single-hop parent relationship that caused the reordering, restores `C1 == sync merge` under the current implementation, and does not change either of the test's assertions (a `chore:` commit contributes `Bump::None`, which does not affect the expected `Version { 2, 1, 0 }`)
- **Files modified:** `crates/devflow-core/src/version.rs` (test-only)
- **Verification:** `cargo test --package devflow-core --lib version::tests::feature_merge_after_sync_merge_does_not_move_the_anchor -- --exact` → `1 passed` pre-fix (confirmed before Task 2's implementation change) and `1 passed` post-fix
- **Committed in:** `6e34287` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — bug in a plan-specified test fixture, not production code)
**Impact on plan:** The fix is test-only, preserves both the letter and intent of the plan's tripwire (a later `--no-ff` feature merge must not move the anchor), and was explicitly authorized by the plan's own stated contingency for this exact failure mode. No scope creep; no production-code behavior affected.

## Issues Encountered

None beyond the fixture-construction issue documented above under Deviations.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `25-VERIFICATION.md` GAP 2's two `missing:` items are closed: the full-ancestry-path walk (Task 2) and the intervening-trunk-commit fixture (Task 1).
- `crates/devflow-cli/src/preflight.rs::preflight_major_bump_check` (plan 25-08's exclusive scope this wave) also calls `release_range_start` and was NOT touched — the function's signature and return contract are unchanged, so 25-08's consumer remains compatible without any coordination needed.
- No file outside `crates/devflow-core/src/version.rs` was modified.

## Self-Check

- `crates/devflow-core/src/version.rs` — FOUND (modified, exists)
- Commit `6e34287` — FOUND in `git log --oneline --all`
- Commit `7dcfa4f` — FOUND in `git log --oneline --all`
- `cargo test --package devflow-core --lib version::` — 38 passed, 0 failed (confirmed above)
- `cargo test --workspace --no-fail-fast` — 676 passed, 0 failed (confirmed above)
- Live-history anchor for `v2.0.0` — `c92229e`, confirmed both via manual git reproduction and via a direct (reverted) call to `release_range_start` against this repository's own root

## Self-Check: PASSED

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-28*
