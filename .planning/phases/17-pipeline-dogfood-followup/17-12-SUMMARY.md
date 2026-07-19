---
phase: 17-pipeline-dogfood-followup
plan: 12
subsystem: infra
tags: [hooks, versioning, git, changelog, rust]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "17-10's hook_context_root worktree/project_root routing and affects_compiled_binary predicate; Round 1/Round 2 WR-04 findings in 17-REVIEW.md"
provides:
  - "version::read_version — a git-free version-file reader that reports exactly what a prior write_version call wrote, never a freshly recomputed minor/patch"
  - "GitFlow::commit_path — a scoped single-path commit, mirroring commit_all but never sweeping in unrelated dirty state"
  - "hooks_for_transition(Validate, Ship) = [DocsUpdate] only; hooks_after_ship() = [Merge, VersionBump, ChangelogAppend, BranchCleanup], ChangelogAppend strictly after VersionBump and before BranchCleanup"
  - "changelog_append and version_bump both commit their own writes, scoped to the file each touches"
  - "Regression test proving three-way agreement (changelog heading version == created tag == version file version) plus a clean working tree after the full after-ship batch"
affects: [18-hermes-support]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Scoped git commits (commit_path, staging one path) for hooks whose write must survive independent of what other hooks in the same batch leave dirty — commit_all's 'git add .' is reserved for hooks (docs_update) that legitimately own the whole tree"
    - "A pure file-read version reader (read_version) kept separate from the git-derived version calculator (compute_version), so a hook that must report what was already written never accidentally re-derives a different number from git state that changed in between"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/version.rs
    - crates/devflow-core/src/hooks.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-cli/src/main.rs
    - .planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md

key-decisions:
  - "changelog_append's commit failure now propagates as a HookError (via `?`) rather than fail-soft warning like docs_update — an uncommitted changelog write is exactly the defect this plan closes, so a failed commit must stop the terminal batch's fail-fast before BranchCleanup runs, not silently continue."
  - "version_bump was also fixed to commit its version-file write before tagging (discovered as a blocking pre-existing defect while writing the plan's own required clean-tree regression test — see Deviations). This means VersionBump's tag now includes its own version-file commit for the first time; previously the tag pointed at content that predated the write it claimed to bump."
  - "Accepted tradeoff (plan-directed, not a deviation): with ChangelogAppend ordered after VersionBump, the changelog commit lands after the tag, so the tagged commit does not contain its own changelog entry. Splitting VersionBump into write-then-tag with ChangelogAppend interleaved was deliberately not attempted — out of scope for this plan."
  - "The generated changelog entry body remains the placeholder 'Released phase via DevFlow' — a content-quality defect (also flagged 17-10-SUMMARY.md:104) deliberately left out of scope."

requirements-completed: [17d]

coverage:
  - id: D1
    description: "version::read_version reads MAJOR.MINOR.PATCH from the version file without touching git, round-tripping through every format write_version supports"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#tests::read_version_round_trips_through_write_version_in_plain_cargo_toml"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#tests::read_version_round_trips_through_write_version_in_workspace_cargo_toml"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#tests::read_version_round_trips_through_write_version_in_package_json"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#tests::read_version_does_not_recompute_from_git_tags"
        status: pass
    human_judgment: false
  - id: D2
    description: "ChangelogAppend removed from the Validate->Ship transition and runs strictly after VersionBump in hooks_after_ship(), before BranchCleanup"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#tests::transition_map_finalizes_docs_only_before_ship"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#tests::after_ship_runs_version_changelog_then_cleanup"
        status: pass
    human_judgment: false
  - id: D3
    description: "Full hooks_after_ship() batch produces three-way agreement between the changelog heading version, the created git tag, and the version file's version, with a clean working tree and CHANGELOG.md present in a commit"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#tests::after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#tests::changelog_append_commits_its_own_write"
        status: pass
    human_judgment: false
  - id: D4
    description: "GitFlow::commit_path stages and commits only the given relative path, leaving other dirty files untouched"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#tests::commit_path_stages_only_the_given_path_leaving_other_dirt_uncommitted"
        status: pass
    human_judgment: false
  - id: D5
    description: "Terminal-batch fail-fast is preserved: a failed Merge or VersionBump still stops before ChangelogAppend/BranchCleanup run"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::terminal_hook_failure_stops_before_branch_cleanup"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-07-19
status: complete
---

# Phase 17 Plan 12: WR-04 — the changelog heading is written before the tag that would make it true Summary

**`ChangelogAppend` now runs strictly after `VersionBump` in the terminal hook batch, reads the version `VersionBump` actually wrote via a new `version::read_version` (never recomputing from git), and commits its own write — closing the root cause behind two consecutive false-release-claim Critical findings in `17-REVIEW.md`.**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-07-19T22:16:00Z (approx.)
- **Completed:** 2026-07-19T22:36:01Z
- **Tasks:** 3 (plan) + 1 discovered (main.rs doc/test accuracy fix)
- **Files modified:** 5

## Accomplishments

- New `version::read_version(project_root) -> Result<Version, VersionError>` parses the full
  MAJOR.MINOR.PATCH out of whatever version file `detect_version_file` resolves, mirroring
  `write_version`'s format handling (`[workspace.package]` included). Unlike `compute_version`, it
  never touches git — it reports exactly what a prior `write_version` call wrote.
- `hooks_for_transition(Validate, Ship)` now returns only `[Hook::DocsUpdate]`; `hooks_after_ship()`
  is now `[Merge, VersionBump, ChangelogAppend, BranchCleanup]`, with `ChangelogAppend` strictly
  after `VersionBump` (so the version it reports actually exists as a tag) and before
  `BranchCleanup` (so a changelog failure still stops the branch from being deleted).
- `changelog_append` reads the version via `read_version` instead of `compute_version`, avoiding
  the "off-by-one" trap the plan named: `compute_version` would see the tag `VersionBump` just cut
  and derive a version one higher than the tag actually names.
- `changelog_append` now commits its own write via a new `GitFlow::commit_path` (scoped to
  `CHANGELOG.md`, not `commit_all`, so it never sweeps in unrelated dirty state). A failed commit
  propagates as a `HookError` so the terminal batch's fail-fast stops `BranchCleanup` from running
  against an uncommitted entry.
- `version_bump` had the identical uncommitted-write defect on its own version-file write —
  discovered as a blocking issue while satisfying the plan's own clean-tree regression test (see
  Deviations) — and is now fixed the same way, committing the version file before tagging.
- New regression test `after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean`
  drives the full `hooks_after_ship()` batch and asserts three-way agreement (changelog heading
  version == created tag == version file version), a clean working tree, and `CHANGELOG.md` present
  in a commit. Confirmed by inspection this fails against pre-fix `hooks.rs` (the old
  `hooks_after_ship()` never included `ChangelogAppend`, so `CHANGELOG.md` would not exist).
- `17-REVIEW.md` updated: header status line flips `1 Critical OPEN` → `0 Critical OPEN`
  (`ship_gate` deliberately left `BLOCKED` — eight Warnings remain open and unblocking is an
  operator call, not this plan's scope); both the live CR-01 entry and the Round 1 appendix's WR-04
  entry get a `RESOLVED — 17-12` note.

## Task Commits

1. **Task 1: Add version::read_version (RED → GREEN)** - `a3a1067` (feat)
2. **Task 2 + Task 2b: Reorder hooks, re-source version, commit the entries** - `31757ef` (fix)
3. **Deviation: update main.rs docs/tests for the new hook batch composition** - `b81ec7d` (fix)
4. **Task 3: Record the WR-04 disposition** - `d9701d7` (docs)

Tasks 2 and 2b were committed together — see Deviations for why.

## Files Created/Modified

- `crates/devflow-core/src/version.rs` - added `read_version`/`parse_version_str`; 6 new tests
  (round-trip across 3 formats, error-without-file, and proof it never recomputes from git tags)
- `crates/devflow-core/src/hooks.rs` - reordered `hooks_for_transition`/`hooks_after_ship`;
  `changelog_append` uses `read_version` and commits via `commit_path`; `version_bump` also commits
  its write before tagging; rewrote/added tests for the new order, commit behavior, and the
  three-way-agreement regression
- `crates/devflow-core/src/git.rs` - new `GitFlow::commit_path` (scoped single-path commit) plus a
  direct unit test proving it doesn't sweep in unrelated dirt
- `crates/devflow-cli/src/main.rs` - `hook_context_root`'s and one test's doc comments corrected to
  describe the new batch composition; `checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout`
  switched from `hooks_for_transition(Validate, Ship)` to `hooks_after_ship()` (see Deviations)
- `.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` - WR-04 marked resolved (live CR-01
  entry, Round 1 appendix WR-04 entry, header status line)

## Decisions Made

- **`changelog_append`'s commit failure propagates as an error, not a fail-soft warning** (unlike
  `docs_update`'s pattern): an uncommitted changelog write is precisely the WR-04 defect being
  closed, so a failed commit must stop the terminal batch before `BranchCleanup` runs.
- **`version_bump` also fixed to commit its version-file write before tagging.** This is a
  behavior change beyond the plan's literal `files_modified` scope statement about
  `changelog_append`, but it lives in the same file (`hooks.rs`) already listed, and was required
  to satisfy the plan's own must-have truth (a clean working tree after the full after-ship batch).
  See Deviations for the full discovery trail.
- **Accepted tradeoff, as directed by the plan:** the changelog commit lands after the tag, so the
  tagged commit doesn't contain its own changelog entry. Splitting `VersionBump` into
  write-then-tag with `ChangelogAppend` interleaved was deliberately not attempted.
- **`ship_gate` left `BLOCKED` in `17-REVIEW.md`,** even though the Critical it was gating on is now
  resolved: eight Warnings remain open (WR-01, WR-02, WR-06 through WR-11), and per the file's own
  established convention, flipping `ship_gate`/`status` is reserved for an operator decision, not
  an autonomous fix pass — mirrored from how the Round 1 appendix's closing note left `status`
  unchanged after CR-02 resolved while Warnings remained.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `version_bump` also left its version-file write uncommitted**
- **Found during:** Task 2's own required verification — the plan's must-have truth 6 ("a
  regression test asserts the working tree is clean after the full after-ship batch") failed with
  `M Cargo.toml` still dirty after `ChangelogAppend`'s own fix, even though `ChangelogAppend` was
  correctly committing `CHANGELOG.md`.
- **Issue:** `version_bump` calls `version::write_version(...)` (a raw `fs::write`, never
  committed) and then immediately tags the *unchanged* HEAD — the same "write without commit"
  defect WR-04 named for `ChangelogAppend`, just never called out for `version_bump` because no
  test had previously driven the full terminal batch and checked tree cleanliness. As a corollary,
  the tag never actually included the version-file bump it claimed to make.
- **Fix:** `version_bump` now commits the version-file write via `GitFlow::commit_path` (scoped to
  the detected version file's name) before tagging, so the tag now correctly includes its own
  version bump.
- **Files modified:** `crates/devflow-core/src/hooks.rs`
- **Verification:** `after_ship_batch_changelog_tag_and_version_file_agree_and_tree_is_clean` and
  `terminal_hooks_version_post_merge_develop` both pass; `cargo test --workspace` green (377
  passed, 0 failed).
- **Committed in:** `31757ef` (part of the Task 2/2b commit)

**2. [Rule 1 - Bug] A pre-existing test's assertion strategy was silently invalidated by the reorder**
- **Found during:** running `cargo test --workspace` after the hook reorder (Task 2)
- **Issue:** `checkout_hooks_skip_instead_of_running_unserialized_on_lock_timeout`
  (`crates/devflow-cli/src/main.rs`) drove `hooks::hooks_for_transition(Stage::Validate,
  Stage::Ship)` and asserted `CHANGELOG.md` did not exist as proof the batch was skipped on lock
  contention. Since `ChangelogAppend` no longer lives in that batch, the assertion became
  vacuously true whether or not the batch actually ran — a silent test-effectiveness regression
  (still green, but no longer testing what its own doc comment claimed).
- **Fix:** Switched the test to drive `hooks::hooks_after_ship()` instead (which still contains
  `ChangelogAppend`); none of its hooks execute in this test regardless, since the lock-timeout
  check short-circuits before the first hook runs, so no real merge/version state was needed.
  Updated the doc comment accordingly.
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** Test passes; still exercises the skip-on-lock-timeout path meaningfully.
- **Committed in:** `b81ec7d`

**3. [Rule 1 - Bug] Stale doc comments describing the old hook batch composition**
- **Found during:** post-Task-2 review of `main.rs` for other WR-04-adjacent references (the
  original WR-04 finding cited `main.rs:1627`/`hook_context_root`)
- **Issue:** `hook_context_root`'s doc comment described `ChangelogAppend` as a Validate→Ship
  content hook needing worktree targeting and the terminal batch as
  `Merge`/`VersionBump`/`BranchCleanup` — both now false. `gh_auth_check_applies`'s doc comment
  and `content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root`'s doc comment
  had the same staleness.
- **Fix:** Rewrote all three doc comments to describe the current batch composition and explicitly
  warn a future reader not to restore 17-10's worktree targeting to `ChangelogAppend`, since it now
  correctly targets `project_root`.
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`
  both clean; no test asserts comment text, so this was a read-through correctness check.
- **Committed in:** `b81ec7d`

**4. [Rule 2 - Missing Critical] Two existing hooks.rs tests would have broken or gone misleading without updates**
- **Found during:** Task 2, running `cargo test -p devflow-core --lib hooks::` after the reorder
- **Issue:** `transition_map_finalizes_docs_and_changelog_before_ship` and
  `validate_to_ship_hooks_append_changelog` asserted the old `[DocsUpdate, ChangelogAppend]`
  composition and CHANGELOG.md presence after the Validate→Ship transition — both now false.
  `terminal_hooks_version_post_merge_develop` computed an expected tag version from
  `git rev-list --count develop` measured *after* the full batch ran, which now also counts
  `ChangelogAppend`'s and `VersionBump`'s new commits, an assumption invalidated by fixes #1 above.
- **Fix:** Renamed/rewrote the two Validate→Ship tests to assert the new `[DocsUpdate]`-only
  composition and CHANGELOG.md absence; rewrote `terminal_hooks_version_post_merge_develop` to
  derive its expected tag from `version::read_version` instead of a rev-list count that no longer
  matched the tag's actual commit position.
- **Files modified:** `crates/devflow-core/src/hooks.rs`
- **Verification:** `cargo test -p devflow-core --lib hooks::` — 12 passed, 0 failed.
- **Committed in:** `31757ef`

---

**Total deviations:** 4 auto-fixed (2 bugs surfaced by the reorder + regression test, 1 stale-doc
accuracy fix, 1 missing/invalidated test-coverage fix).
**Impact on plan:** All four trace directly to the plan's own required regression test (a clean
tree after the full after-ship batch) or to the WR-04 root-cause fix itself. No scope creep beyond
`hooks.rs`, `git.rs` (already declared), and `main.rs` (comment/test accuracy only, no behavior
change to CLI logic).

## Issues Encountered

None beyond the deviations above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- WR-04 (both Round 1's root cause and Round 2's uncommitted-write facet) is resolved; no open
  Critical or WR-04 entry remains in `17-REVIEW.md`.
- `ship_gate` in `17-REVIEW.md` is deliberately left `BLOCKED` — eight Warnings (WR-01, WR-02,
  WR-06 through WR-11) remain open and unrelated to this plan; unblocking is an explicit operator
  call for a future pass, not something this plan changes.
- The generated changelog entry body is still the placeholder "Released phase via DevFlow" — a
  content-quality defect, already flagged in `17-10-SUMMARY.md:104`, deliberately out of scope here.
- `VersionBump`'s tag now includes its own version-file bump commit for the first time (a
  correctness improvement beyond WR-04's literal scope) — any future work reasoning about "the
  commit a release tag points at" should account for this change in behavior.

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-19*

## Self-Check: PASSED

All modified files confirmed present on disk; all five commit hashes (`a3a1067`, `31757ef`,
`b81ec7d`, `d9701d7`, `e89a25b`) confirmed present in `git log --oneline --all`.
