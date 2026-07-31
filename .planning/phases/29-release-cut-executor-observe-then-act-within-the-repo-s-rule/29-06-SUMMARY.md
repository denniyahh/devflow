---
phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
plan: 06
subsystem: release-automation
tags: [rust, git, gh-cli, worktree, merge-policy, release]

# Dependency graph
requires:
  - phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
    provides: "29-04's release_execute::{StepOutcome, CutReport, ReleaseStep, action_for, unit_for, pr_refs, cut/cut_with} scaffolding; 29-05's PreparedBranch/release_branch_name/prepare_bump_branch, pr_argv/merge_argv/open_and_arm_pr, ScratchWorktreeGuard/branch_exists_on_origin/fetch_ref/add_scratch_worktree, and bump_and_changelog_pr wired into action_for(VersionBumped)/action_for(ChangelogWritten); 29-03's release_policy::{MergeIntent, MergeMethod, resolve_merge_method, discover_allowed_merge_methods, required_method}"
provides:
  - "devflow_core::release_execute::{release_pr_title, release_pr_to_main} — the release pull request from develop into main, wired into action_for(ReleasePrMerged), method resolved via MergeIntent::ReleaseCut"
  - "devflow_core::release_execute::{sync_branch_name, SyncMergeOutcome, merge_main_into_sync_branch_and_push, sync_back_pr} — a port of scripts/sync-main-to-develop.sh (already-an-ancestor short circuit, -X ours merge, tree-identity refusal), wired into action_for(SyncMerged), landed via a pull request whose method (MergeIntent::SyncBack) is unrepresentably wrong"
  - "A tested pin of the unit boundary: with steps 1/2/3/5 all wired, `devflow release cut` walks to the signed-tag step and stops there naming unit 29c (crates/devflow-cli/tests/release_cut.rs#cut_walks_to_the_signed_tag_step_and_stops)"
affects: [29-07-plan-commit-point]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Git-only core / gh-calling wrapper split: merge_main_into_sync_branch_and_push does every git operation (fetch, ancestor check, worktree, merge, tree-identity compare, push) with zero gh dependency, so the port of scripts/sync-main-to-develop.sh is fully unit-testable against real git fixtures with no GitHub repo required; sync_back_pr composes it with open_and_arm_pr(MergeIntent::SyncBack) for the one step that must talk to gh"
    - "Fake-gh test double for CLI-level fixtures: a small bash script placed ahead of PATH answers the exact `gh api <endpoint>` calls release_observe::observe makes (contents@ref, compare/main...develop), letting an integration test drive four of the six release-cut oracles to Present without a real GitHub repository — used once, for the unit-boundary pin, not as a general mocking layer"
    - "Tree-identity refusal via the same reachable helper: tree_object_id(repo) computes `git rev-parse HEAD^{tree}` before and after the -X ours merge; any difference is Err naming both object ids, exactly mirroring scripts/sync-main-to-develop.sh lines 51-62 rather than reimplementing the comparison"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/release_execute.rs
    - crates/devflow-cli/tests/release_cut.rs
    - CONTRIBUTING.md

key-decisions:
  - "Split sync_back_pr into a git-only helper (merge_main_into_sync_branch_and_push) plus a thin gh-calling wrapper, rather than one function performing all 7 steps inline as the plan's action text literally lists them. This is what makes the port's actual logic (ancestor short circuit, -X ours merge, tree-identity refusal, cleanup) testable against real git fixtures with zero gh/GitHub dependency — mirroring 29-05's own prepare_bump_branch/bump_and_changelog_pr split for exactly the same reason."
  - "The CLI-level unit-boundary test (cut_walks_to_the_signed_tag_step_and_stops) uses a small fake `gh` executable placed ahead of PATH, rather than a live GitHub repository, to make steps 1-3 observe as already done. SignedTagPresent's own oracle (git ls-remote --tags, no gh) is left genuinely real against a real (tag-less) local origin, so the one step under test is never faked."
  - "Both tasks committed as a single atomic commit. action_for's exhaustive match is edited by both tasks in the same hunk (the ReleasePrMerged and SyncMerged arms), and the two accompanying test-list edits (action_for_returns_none_for_every_step_without_an_action_in_this_build) also touch both tasks' steps together — splitting cleanly would mean re-deriving those shared edits twice. Matches 29-05's own precedent of bundling functionally-adjacent tasks in this same file/match statement."
  - "Did not run `devflow release cut <version> . --yes-release` live against this worktree's real origin (denniyahh/devflow). Unlike 29-04's live verification (which ran against a build with zero actions implemented — pure observation, no side effects possible), this plan's actions genuinely push branches and open real pull requests. Running it live here would create real, visible artifacts against the operator's actual repository outside of any task's explicit scope. Live pull-request behavior remains, as documented since 29-04/29-05, out of this hermetic suite's scope per 29-VALIDATION.md's Manual-Only Verifications table — a decision for the operator or the orchestrator's own end-of-phase verification, not this plan."

patterns-established:
  - "main_already_synced(project_root): a single wrapper around `git merge-base --is-ancestor origin/main origin/develop`, distinguishing exit 0 (ancestor) from exit 1 (not) from any other status (a real Err) — the load-bearing short circuit that makes a re-run after a successful sync free."

requirements-completed: [29b]

coverage:
  - id: D1
    description: "The release pull request from develop into main is opened with a method resolved for MergeIntent::ReleaseCut against main's discovered allowed set, and refuses loudly (naming intent, required method, and discovered set) rather than substituting when that method is unavailable"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::release_pr_title_starts_with_the_documented_prefix, ::action_for_returns_an_action_for_release_pr_merged, ::release_pr_to_main_refuses_before_creating_anything_when_resolution_fails, ::release_pr_to_main_resolves_squash_against_squash_only_and_against_both_methods"
        status: pass
    human_judgment: false
  - id: D2
    description: "The sync-back reproduces every check scripts/sync-main-to-develop.sh performs — the already-an-ancestor short circuit, the -X ours merge, and the byte-identical tree-identity verification that refuses to push if the merge changed develop's tree — and the operator's own checkout is left untouched"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::merge_main_into_sync_branch_and_push_short_circuits_when_already_an_ancestor, ::merge_main_into_sync_branch_and_push_produces_a_two_parent_merge_commit_when_diverged, ::merge_main_into_sync_branch_and_push_pushes_a_tree_identical_branch_when_diverged, ::merge_main_into_sync_branch_and_push_refuses_when_the_merge_changes_the_tree, ::sync_branch_name_matches_pr_refs_head_for_sync_merged_step"
        status: pass
    human_judgment: false
  - id: D3
    description: "The sync-back pull request requires a real merge commit — MergeIntent::SyncBack resolves to Merge even when squash is also allowed — making a squash on this pull request unrepresentable by construction, not merely discouraged by convention"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::sync_intent_resolves_to_merge_even_when_squash_is_also_allowed, ::merge_argv_always_carries_a_method_flag (release_policy.rs's own resolve_merge_method_sync_back_on_develop_yields_merge already covers the same fact at the policy layer)"
        status: pass
    human_judgment: false
  - id: D4
    description: "With steps 1, 2, 3, and 5 implemented, devflow release cut walks as far as the signed-tag step and stops there with an accurate report naming unit 29c as the unit that supplies it"
    requirement: "29b"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_cut.rs#cut_walks_to_the_signed_tag_step_and_stops"
        status: pass
    human_judgment: false
  - id: D5
    description: "No readiness prediction, no required-check polling loop, and no branch-name conditional exists anywhere in the new code — the walk never predicts whether a merge will be allowed, only attempts it and re-observes"
    requirement: "29b"
    verification:
      - kind: other
        ref: "rg -n 'required_approving_review_count|pr checks|--watch' crates/devflow-core/src/release_execute.rs (zero matches)"
        status: pass
    human_judgment: false
  - id: D6
    description: "Unit 29b is complete and independently shippable: all four recoverable steps (version bump, changelog, release PR, sync-back) have real actions; a stall in 29c (signed tag, crates publish) leaves 29a and 29b delivered"
    requirement: "29b"
    verification: []
    human_judgment: true
    rationale: "Whether the phase's own unit-independence claim holds across the whole three-unit arc is a phase-level judgment made at 29-07/ship time, not something one plan's own tests can establish in isolation — this plan's own contribution (steps 3 and 5 wired, steps 4 and 6 still None) is proven by D1-D4 above."

# Metrics
duration: ~1h 40min
completed: 2026-07-31
status: complete
---

# Phase 29 Plan 06: Release PR and Sync-Back — the Two Remaining Recoverable Steps Summary

**`release_pr_to_main` (the release pull request from `develop` into `main`, method resolved via `MergeIntent::ReleaseCut`) and `sync_back_pr` (a tested port of `scripts/sync-main-to-develop.sh` — ancestor short circuit, `-X ours` merge, byte-identical tree-identity refusal — landed via a pull request whose method is unrepresentably `Merge`), completing unit 29b: `devflow release cut` now walks as far as the signed-tag step and stops there, naming unit `29c`.**

## Performance

- **Duration:** ~1h 40min
- **Completed:** 2026-07-31
- **Tasks:** 2 completed (both `tdd="true"`, tests and implementation verified together)
- **Files modified:** 3

## Accomplishments

- **The release pull request from `develop` into `main` resolves its merge method from intent, never from the fact that `main` happens to permit only squash today.** `release_pr_to_main` calls the existing `open_and_arm_pr` helper with `MergeIntent::ReleaseCut`; resolution happens before anything is created, so a required-method-unavailable case refuses (naming the intent, required method, and discovered set) before any `gh pr create` call is reachable — the same resolve-before-create guarantee 29-05 established.
- **The sync-back is a genuine port, not a redesign.** `merge_main_into_sync_branch_and_push` carries every check `scripts/sync-main-to-develop.sh` performs: fetch `main`/`develop` first, the already-an-ancestor short circuit (`main_already_synced`, mirroring the script's own `git merge-base --is-ancestor` early exit), a scratch-worktree `-X ours --no-edit` merge using the script's own commit message, and a pre/post tree-object-id comparison that refuses — pushing and opening nothing — if the merge changed develop's content. `scripts/sync-main-to-develop.sh` itself is untouched (`git diff --name-only` for this plan's commit does not list it).
- **The sync PR's method is unrepresentably wrong, not merely discouraged.** `sync_back_pr` calls `open_and_arm_pr` with `MergeIntent::SyncBack`, which `release_policy::resolve_merge_method` (29-03) resolves to `MergeMethod::Merge` even when squash is also allowed — `MergeMethod` has no unspecified variant, so the 2026-07-27 incident (an unspecified-method auto-merge silently squashing the sync PR) is closed by the type system, not by a comment.
- **The git-only core is fully unit-tested against real fixture repositories, independent of any `gh`/GitHub dependency.** `merge_main_into_sync_branch_and_push` was factored out of `sync_back_pr` (mirroring 29-05's own `prepare_bump_branch`/`bump_and_changelog_pr` split) precisely so the short circuit, the two-parent merge commit, the byte-identical-tree push, and the tree-changed refusal (with no branch pushed and no scratch worktree surviving) are all provable against real `git` fixtures with zero network dependency.
- **The unit boundary — "steps 1/2/3/5 wired, walk stops at the signed tag naming 29c" — is now a tested fact, not a claim.** `cut_walks_to_the_signed_tag_step_and_stops` uses a small fake `gh` executable (answering only the exact `gh api` calls `release_observe::observe` makes) to make four of the six oracles report already-done, while leaving `SignedTagPresent`'s own real, local `git ls-remote` oracle genuinely observing absent against a real (tag-less) origin — the one step actually under test.
- **`CONTRIBUTING.md` now records the executable equivalent** without touching the manual procedure or `scripts/sync-main-to-develop.sh`, which the plan explicitly required stay unmodified as the reference the port was made from.

## Task Commits

Both tasks were implemented, verified, and committed together as one atomic commit — see "Deviations from Plan" for why.

1. **Tasks 1 & 2: The release pull request from develop into main; the sync-back port with its tree-identity refusal, landed by a real merge commit**
   - `6f1e544` `feat(29-06): release PR and sync-back — the two remaining recoverable steps` — `release_execute.rs` (new: `release_pr_title`, `release_pr_to_main`, `sync_branch_name`, `sync_merge_message`, `main_already_synced`, `SyncMergeOutcome`, `merge_main_into_sync_branch_and_push`, `sync_back_pr`, `tree_object_id`; `action_for` wired for `ReleasePrMerged`/`SyncMerged`; 17 new/updated unit tests), `crates/devflow-cli/tests/release_cut.rs` (new: `cut_walks_to_the_signed_tag_step_and_stops` plus its fake-`gh` fixture helper), `CONTRIBUTING.md` (new note recording the executable equivalent)

## Files Created/Modified

- `crates/devflow-core/src/release_execute.rs` — `release_pr_title` (title prefix `release: v<version>`, matching CONTRIBUTING.md step 3); `release_pr_to_main` (Task 1's action, wired into `action_for(ReleasePrMerged)`); `sync_branch_name` (matches `pr_refs`'s existing `SyncMerged` head, agreement-tested); `sync_merge_message`/`main_already_synced`/`tree_object_id` (pure/thin helpers carried from the script); `SyncMergeOutcome`/`merge_main_into_sync_branch_and_push` (the testable git-only core of the sync-back port); `sync_back_pr` (Task 2's action, composing the above with `open_and_arm_pr(MergeIntent::SyncBack)`, wired into `action_for(SyncMerged)`); 41 total tests in the module (17 new/updated for this plan)
- `crates/devflow-cli/tests/release_cut.rs` — `write_fake_gh_reporting_prs_landed` (fixture double) and `cut_walks_to_the_signed_tag_step_and_stops` (the unit-boundary pin)
- `CONTRIBUTING.md` — a note after the crates.io publish-order paragraph in § "Cutting a Release" recording that `devflow release cut` is now the executable equivalent of the manual procedure, that the manual steps remain authoritative, and that `scripts/sync-main-to-develop.sh` stays in the repository unmodified as the port's reference

## Decisions Made

- **Factored `sync_back_pr` into a git-only core (`merge_main_into_sync_branch_and_push`) plus a thin `gh`-calling wrapper**, rather than one function performing all 7 of the plan's listed steps inline. This is what makes the port's real logic — the checks that matter — testable against real git fixtures with zero `gh`/GitHub dependency, mirroring 29-05's identical split for `prepare_bump_branch`/`bump_and_changelog_pr`.
- **The CLI-level unit-boundary test fakes `gh` via a small script on `PATH`**, rather than requiring a live GitHub repository, so `VersionBumped`/`ChangelogWritten`/`ReleasePrMerged`/`SyncMerged` can be driven to `Present` deterministically. `SignedTagPresent` is left genuinely real (a local `git ls-remote` against a tag-less origin) since that is the one step this test actually verifies.
- **Both tasks committed as one atomic commit.** `action_for`'s exhaustive match is edited by both tasks in the same hunk (`ReleasePrMerged` and `SyncMerged` arms land together), and the accompanying test-list update (`action_for_returns_none_for_every_step_without_an_action_in_this_build`) also touches both tasks' steps in one edit. Splitting would mean re-deriving the same shared hunks twice for no functional benefit — the same reasoning 29-05 recorded for this identical file and match statement.
- **Did not run `devflow release cut <version> . --yes-release` live against this repository's real `origin`.** See Deviations below.

## Deviations from Plan

### Auto-fixed Issues

None — no bugs, missing critical functionality, or blocking issues were discovered during this plan's execution that required an unplanned fix.

### Process Deviations (documented, not auto-fixes)

**1. Both tasks committed as a single atomic commit rather than two.**
- **Found during:** Preparing to commit Task 1 (`release_pr_to_main`) separately from Task 2 (`sync_back_pr`).
- **Reason:** `action_for`'s exhaustive match over `ReleaseStep` has exactly one hunk covering both the `ReleasePrMerged` and `SyncMerged` arms; the existing test `action_for_returns_none_for_every_step_without_an_action_in_this_build` also needed both steps removed from its list in the same edit. Splitting into two independently-committable diffs would require re-deriving these shared hunks twice, with no functional or review benefit.
- **Committed in:** `6f1e544` (both tasks).

**2. The phase-level `<verification>` section's live `devflow release cut <version> . --yes-release` run was not executed against this worktree's real `origin` (denniyahh/devflow).**
- **Found during:** Reviewing the plan's overall `<verification>` list after both tasks' automated tests passed.
- **Reason:** Unlike 29-04's live verification run (a build where every `action_for` arm was still `None` — pure observation, zero side effects possible), this plan's actions genuinely push branches and open real GitHub pull requests when they run. Running the command live from this worktree would create real, visible pull requests against the operator's production repository — a side effect no task in this plan authorizes, and squarely the kind of irreversible, collaborator-visible action 29-VALIDATION.md's Manual-Only Verifications table already reserves for human-supervised or orchestrator-level verification, not autonomous plan execution.
- **Impact:** The hermetic test suite (41 unit tests in `release_execute.rs`, 10 integration tests in `release_cut.rs`, all green) covers every behavior bullet in both tasks, including the unit-boundary pin. The live end-to-end run remains an explicit gap, consistent with 29-04/29-05's own stated scope boundary, and is a matter for the phase's own final verification or the operator, not this plan.

---

**Total deviations:** 0 auto-fixed; 2 documented process/scope decisions (a git-history-granularity accommodation, and a deliberate exclusion of a real-side-effect live run from this plan's own execution). Neither changes the design, scope, or content of either task.
**Impact on plan:** None on functionality or test coverage; both tasks' full scope, all specified tests, and all specified acceptance criteria are present and verified.

## Issues Encountered

None beyond the two documented process deviations above.

## User Setup Required

None — no external service configuration required. `gh` authentication (when present) is read passively by the new PR calls, exactly as 29-04's/29-05's existing call sites already do; no new environment variable or config key was introduced by this plan.

## Next Phase Readiness

- Unit 29b is now complete: all four recoverable release-cut steps (version bump, changelog, release PR, sync-back) have real, tested actions wired into `action_for`. Only `SignedTagPresent` and `CratesPublished` — the two irreversible commit-point operations — remain `None`, and `29-07-PLAN.md` replaces exactly those two arms.
- `devflow release cut <version> [project] --yes-release`, run against a repository where steps 1-3 and 5 are already done, stops cleanly at the signed-tag step and names unit `29c` — proven both by the CLI-level fixture test in this plan and, previously, by 29-04's live run against this very repository at `2.2.0` (before this plan's actions existed).
- Live pull-request behavior for the release PR and the sync-back PR (whether GitHub's auto-merge actually waits for green checks and merges with the requested method) remains deliberately out of this hermetic suite's scope, per `29-VALIDATION.md`'s Manual-Only Verifications table — unchanged from 29-04/29-05, and now explicitly including this plan's own two new PR-opening actions.
- No blockers for `29-07`. `open_and_arm_pr`, `pr_argv`/`merge_argv`, and the resolve-before-create pattern are all reused as-is; `29-07`'s signed-tag and `cargo publish` actions do not touch pull requests at all, so nothing built here constrains their design.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_execute.rs`
- FOUND: `crates/devflow-cli/tests/release_cut.rs`
- FOUND: `CONTRIBUTING.md`
- FOUND commit `6f1e544` (feat: release PR and sync-back, both tasks)
