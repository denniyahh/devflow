---
phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
plan: 05
subsystem: release-automation
tags: [rust, git, gh-cli, cargo, worktree, merge-policy, release]

# Dependency graph
requires:
  - phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
    provides: "29-04's release_execute::{StepOutcome, CutReport, ReleaseStep, action_for, unit_for, pr_refs, cut/cut_with} scaffolding (all six action_for arms None); 29-03's release_policy::{MergeIntent, MergeMethod, resolve_merge_method, discover_allowed_merge_methods}; existing version::{write_version, read_version, reachable_semver_baseline, release_range_start, changelog_sections, render_changelog_body} and ship::prepend_changelog"
provides:
  - "devflow_core::release_execute::{PreparedBranch, release_branch_name, prepare_bump_branch} — the version-bump + changelog branch preparation, entirely inside a scratch worktree, idempotent, always cleaned up"
  - "devflow_core::release_execute::{pr_argv, merge_argv, open_and_arm_pr, bump_and_changelog_pr} — the shared pull-request helper every PR-backed release-cut step reuses, plus the composed action wired into action_for(VersionBumped) and action_for(ChangelogWritten)"
  - "version::parse_version_str made pub; hooks::today() made pub(crate) — both reused by prepare_bump_branch rather than reimplemented"
affects: [29-06-plan, 29-07-plan-commit-point]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Drop-guard cleanup (ScratchWorktreeGuard): a scratch worktree is removed unconditionally on every exit path — early `?` return, forced failure, or normal completion — because the guard's `Drop` impl runs regardless of how the function returns, with no way for a future edit to add a return that skips it"
    - "Origin-existence check before worktree creation: `branch_exists_on_origin` is checked before deciding whether `worktree::add` creates a fresh branch off `origin/develop` or checks out the existing one — a re-run after a prior push reuses the same branch and its commits instead of silently discarding them"
    - "Resolve-before-create for pull requests: `open_and_arm_pr` calls `discover_allowed_merge_methods`/`resolve_merge_method` before any `gh pr create`/`gh pr merge` call — a resolution failure short-circuits via `?` before anything is created, so a required-method-unavailable case never leaves an armed PR that can't be merged correctly"
    - "PR body passed by file, never inline (`--body-file`), so no changelog or version text ever crosses a shell argument"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/release_execute.rs
    - crates/devflow-core/src/version.rs
    - crates/devflow-core/src/hooks.rs
    - crates/devflow-cli/tests/release_cut.rs

key-decisions:
  - "Committed Task 1 and Task 2 as a single atomic commit rather than two, after empirically confirming (via a temporary revert + `cargo clippy -p devflow-core --lib -- -D warnings`) that an unwired `prepare_bump_branch` produces 7 `dead_code` errors under this project's `-D warnings` gate — `bump_and_changelog_pr` (Task 2) is the only caller of `prepare_bump_branch` (Task 1), and `action_for`'s wiring (also Task 2) is what makes both reachable from production code. Splitting into two independently-clippy-clean commits would have required artificial scaffolding (a dead_code allow, or a throwaway interim wiring later discarded) not reflecting real design. See Deviations."
  - "`add_scratch_worktree` checks `branch_exists_on_origin` before choosing `worktree::add`'s `create_branch` flag, rather than relying solely on the plan's literal path-exists-only retry — this is what makes `prepare_bump_branch`'s idempotency check origin-based rather than accidentally dependent on whether a prior run's local branch happened to survive in the same checkout."
  - "`version::parse_version_str` made `pub` (was private) and `hooks::today()` made `pub(crate)` (was private) — both are exactly the existing, tested implementations `prepare_bump_branch` needs; widening their visibility avoids a second parser/date-helper that could drift, per this phase's own no-second-implementation rule."
  - "The changelog-already-written idempotency check reuses `release_observe::classify_changelog_heading` directly against the scratch worktree's local `CHANGELOG.md` content — the exact same classifier the observer oracle uses against GitHub's remote copy — rather than a second heading-matching implementation."

patterns-established:
  - "commit_if_changed(git_flow, path, message) -> Result<u32, String>: wraps GitFlow::commit_path and reports whether the branch tip actually moved (1) or the commit was a genuine no-op (0), since commit_path itself folds 'nothing to commit' into Ok(()) without distinguishing the two — used to compute PreparedBranch::commits_created and, transitively, already_prepared."

requirements-completed: [29b]

coverage:
  - id: D1
    description: "prepare_bump_branch creates a scratch worktree off origin/develop (or the existing pushed branch), rewrites both version locations via version::write_version, runs cargo build so Cargo.lock picks up the new version, writes the changelog entry via the existing content chain, commits each artifact, and pushes the branch — never touching the operator's own checkout"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::prepare_bump_branch_rewrites_the_two_place_version_bump (fixture self-pin starts stale; asserts both [workspace.package] and the self-pin equal the requested version), ::prepare_bump_branch_writes_the_changelog_heading, ::prepare_bump_branch_produces_commits_on_the_release_branch, ::prepare_bump_branch_pushes_the_branch_to_origin"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::prepare_bump_branch_leaves_the_operators_checkout_untouched (asserts project root's rev-parse HEAD / abbrev-ref HEAD / status --porcelain identical before and after)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The scratch worktree is removed on every exit path (success and forced failure alike), and re-running after a prior success makes no new commit, reporting already-prepared"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::prepare_bump_branch_removes_the_scratch_worktree_on_success, ::prepare_bump_branch_removes_the_scratch_worktree_on_a_forced_failure (a fixture crate with invalid Rust source forces cargo build to fail after the worktree exists; asserts git worktree list is clean afterward), ::prepare_bump_branch_leaves_the_scratch_worktrees_tree_clean, ::prepare_bump_branch_rerun_after_success_makes_no_new_commit_and_reports_already_prepared"
        status: pass
    human_judgment: false
  - id: D3
    description: "Every merge invocation carries an explicit, resolved method — an argument vector with no method flag is unrepresentable — and a required method absent from the discovered allowed set refuses before any gh pr create/merge call, naming the intent, required method, and discovered set"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::merge_argv_always_carries_a_method_flag (iterates every MergeIntent against every allowed set that resolves; mutation-confirmed: temporarily removing method.flag() from merge_argv made this test and 3 others fail, reverted), ::merge_argv_uses_the_merge_flag_for_merge_method, ::merge_argv_uses_the_squash_flag_for_squash_method, ::merge_argv_contains_auto_and_the_pr_number, ::pr_argv_never_inlines_the_body_and_always_uses_a_body_file_flag, ::open_and_arm_pr_refuses_before_contacting_github_when_resolution_fails"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::action_for_resolves_version_bumped_and_changelog_written_to_the_bump_action, ::action_for_returns_none_for_every_step_without_an_action_in_this_build"
        status: pass
    human_judgment: false
  - id: D4
    description: "The step stops with the real tool's own failure text — never an invented one — when the remote is unreachable or gh is unauthenticated against a real remote"
    requirement: "29b"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_cut.rs#cut_stops_with_a_real_reason_when_the_remote_is_unreachable, #cut_stops_without_crashing_when_gh_is_unauthenticated_against_a_real_remote"
        status: pass
    human_judgment: false

# Metrics
duration: ~2h
completed: 2026-07-31
status: complete
---

# Phase 29 Plan 05: Prepare the Version-Bump/Changelog Branch and Open+Arm Its PR Summary

**`prepare_bump_branch` (scratch-worktree version bump + changelog, always cleaned up, idempotent) and `open_and_arm_pr` (resolve-before-create pull-request helper with an explicit, never-omitted merge method) — wired into `action_for(VersionBumped)`/`action_for(ChangelogWritten)`, giving steps 1 and 2 of the release-cut walker their first real, non-stub actions.**

## Performance

- **Duration:** ~2h (extensive source reading across `version.rs`, `git.rs`, `worktree.rs`, `hooks.rs`, `ship.rs`, `release_policy.rs`, plus design work on cleanup-on-every-exit-path and origin-based idempotency, plus a full local `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` verification loop)
- **Completed:** 2026-07-31
- **Tasks:** 2 planned (`tdd="true"`), implemented and verified together, committed as one atomic commit (see Deviations)
- **Files modified:** 4

## Accomplishments

- **`prepare_bump_branch` never touches the operator's checkout.** Every write and commit happens inside a scratch worktree `worktree::add` creates under `.worktrees/release-bump-<version>`; a `ScratchWorktreeGuard` (a `Drop` impl) removes it on every exit path — proven live by a forced-failure test (an invalid-Rust-source fixture crate makes `cargo build` fail deterministically after the worktree already exists) asserting `git worktree list` is clean afterward, and by a test asserting the project root's `rev-parse HEAD` / `abbrev-ref HEAD` / `status --porcelain` are byte-identical before and after.
- **The two-place version bump is never re-derived.** `prepare_bump_branch` parses the operator's version positional with `version::parse_version_str` (made `pub` for this reuse) and calls `version::write_version` — the single function that rewrites both `[workspace.package] version` and every local-path `[workspace.dependencies]` self-pin. A fixture whose self-pin starts stale proves both were rewritten together.
- **The changelog entry reuses the exact chain Phase 25 already shipped** (`reachable_semver_baseline` → `release_range_start` → `changelog_sections` → `render_changelog_body` → `ship::prepend_changelog`), with no new formatting logic — the idempotency check for "changelog already written" reuses `release_observe::classify_changelog_heading` directly, the same classifier the remote observer oracle uses.
- **Idempotent by construction.** `branch_exists_on_origin` is checked before deciding whether to create the branch fresh off `origin/develop` or check out the existing one — a re-run after a prior success finds both artifacts already correct, makes no new commit, and reports `already_prepared: true`; a re-run after a partial failure completes only what's missing.
- **Every merge invocation carries an explicit method — never a bare `--auto`.** `merge_argv` always includes `MergeMethod::flag()`; because `MergeMethod` has no unspecified variant, an argument vector with no method flag is unrepresentable. Mutation-confirmed live: temporarily removing `method.flag()` from `merge_argv` made 4 tests fail (`merge_argv_always_carries_a_method_flag` and three others), reverted before commit.
- **`open_and_arm_pr` resolves before creating anything.** `discover_allowed_merge_methods` → `resolve_merge_method` runs first; a resolution failure short-circuits via `?` before any `gh pr create`/`gh pr merge` call is reachable — a required method discovered unavailable can never leave an armed, unmergeable PR behind. The PR body is written to a temporary file and passed via `--body-file`, never inline.
- **Steps 1 and 2 of the release-cut walk are no longer stubs.** `action_for(ReleaseStep::VersionBumped)` and `action_for(ReleaseStep::ChangelogWritten)` both now resolve to `bump_and_changelog_pr`, which composes `prepare_bump_branch` + `open_and_arm_pr(MergeIntent::VersionBump)`.
- **Real, non-invented failure text on a broken remote.** Two new CLI integration tests (`cut_stops_with_a_real_reason_when_the_remote_is_unreachable`, `cut_stops_without_crashing_when_gh_is_unauthenticated_against_a_real_remote`) prove the walk stops with a real `git`/`gh` failure message rather than a fabricated one.

## Task Commits

Both tasks were implemented, verified, and committed together as one atomic commit — see "Deviations from Plan" for why.

1. **Tasks 1 & 2: Prepare the release branch in a scratch worktree; open and arm its pull request with an explicitly resolved merge method**
   - `842e5b4` `feat(29-05): prepare version-bump/changelog branch and open+arm its PR` — `release_execute.rs` (new: `PreparedBranch`, `release_branch_name`, `ScratchWorktreeGuard`, `branch_exists_on_origin`, `fetch_ref`, `add_scratch_worktree`, `prepare_bump_branch`, `commit_if_changed`, `pr_argv`, `merge_argv`, `write_temp_body_file`, `open_and_arm_pr`, `bump_and_changelog_pr`; `action_for` wired for `VersionBumped`/`ChangelogWritten`; 17 new unit tests plus 2 tests updated for the new wiring), `version.rs` (`parse_version_str` made `pub`), `hooks.rs` (`today()` made `pub(crate)`), `release_cut.rs` (2 new integration tests)

## Files Created/Modified

- `crates/devflow-core/src/release_execute.rs` — `PreparedBranch` struct; `release_branch_name` (extracted from `pr_refs`'s inline literal, with an agreement test); `ScratchWorktreeGuard` (Drop-based unconditional cleanup); `branch_exists_on_origin`/`fetch_ref`/`add_scratch_worktree` (origin-aware worktree creation); `prepare_bump_branch` (the branch-preparation half); `commit_if_changed` (commit-landed-or-no-op detection); `pr_argv`/`merge_argv` (pure argument-vector builders); `write_temp_body_file`; `open_and_arm_pr` (the shared PR helper); `bump_and_changelog_pr` (the composed action); `action_for` now returns `Some(bump_and_changelog_pr)` for `VersionBumped`/`ChangelogWritten`; 30 unit tests total in the module (17 new/updated)
- `crates/devflow-core/src/version.rs` — `parse_version_str` visibility widened to `pub` (doc comment explains the reuse rationale); no behavior change
- `crates/devflow-core/src/hooks.rs` — `today()` visibility widened to `pub(crate)`; no behavior change
- `crates/devflow-cli/tests/release_cut.rs` — 2 new integration tests: `cut_stops_with_a_real_reason_when_the_remote_is_unreachable`, `cut_stops_without_crashing_when_gh_is_unauthenticated_against_a_real_remote`

## Decisions Made

- **Committed both tasks as one atomic commit.** See Deviations below — a clippy-enforced `dead_code` constraint made an independently-clean two-commit split impossible without artificial scaffolding.
- **Origin-based idempotency, not local-branch-survival idempotency.** `add_scratch_worktree` is given `create_branch = !branch_exists_on_origin(...)` rather than only handling the plan's literal path-already-exists retry — this makes "the branch already exists on origin and already carries both changes" (the plan's own stated idempotency behavior) true regardless of whether a prior run's local branch happened to survive in the same checkout between calls.
- **`version::parse_version_str` made `pub`, `hooks::today()` made `pub(crate)`.** Both are the exact existing, already-tested implementations `prepare_bump_branch` needs (version parsing and the changelog entry's date); widening visibility avoids a second implementation of either that could drift, consistent with this phase's own no-second-implementation discipline (already applied to `write_version`/`changelog_sections`/`render_changelog_body`/`prepend_changelog`).
- **The changelog-already-written check reuses `release_observe::classify_changelog_heading`** against the scratch worktree's local `CHANGELOG.md`, rather than a second heading-matching implementation — the same classifier the remote observer oracle (`changelog_written_on_develop`) already uses.
- **A small hand-rolled temp-file helper (`write_temp_body_file`) instead of the `tempfile` crate** for the PR body file: `tempfile` is a `devflow-core` dev-dependency only, not available in production builds, and this phase's research explicitly recommends against adding new production dependencies where an existing-tool equivalent suffices.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — a plan-implied constraint conflicting with the codebase's own clippy gate] Task 1 and Task 2 committed together, not as two separate task commits**
- **Found during:** Preparing to commit Task 1 (`prepare_bump_branch`) separately from Task 2 (`open_and_arm_pr` + `action_for` wiring), per the standard one-commit-per-task protocol.
- **Issue:** `bump_and_changelog_pr` (Task 2's composed action) is the only caller of `prepare_bump_branch` (Task 1); `action_for`'s wiring (also Task 2) is what makes both reachable from non-test production code. Empirically confirmed via a temporary revert (`action_for` arms set back to `None`) plus `cargo clippy -p devflow-core --lib -- -D warnings`: with Task 2's wiring absent, `ScratchWorktreeGuard`, `branch_exists_on_origin`, `fetch_ref`, `add_scratch_worktree`, `prepare_bump_branch`, `commit_if_changed`, and `bump_and_changelog_pr` all produce `dead_code` errors under this project's `-D warnings` gate — a Task-1-only commit would fail the plan's own required `cargo clippy --workspace --all-targets -- -D warnings` verification step.
- **Fix:** Committed the fully-wired, fully-tested implementation of both tasks as one atomic commit. No design change to either task's scope or content — both tasks' code is exactly as each task's `<action>` specifies; only the git-history granularity differs from the default one-commit-per-task convention.
- **Files modified:** None beyond what both tasks already specified.
- **Verification:** Reverted the temporary experiment before committing; re-ran `cargo test -p devflow-core --features test-support --lib release_execute` (30 passed, 0 failed) and `cargo clippy --workspace --all-targets -- -D warnings` (clean) against the fully-wired state before the real commit.
- **Committed in:** `842e5b4`

---

**Total deviations:** 1 (a git-history-granularity accommodation forced by the tasks' intentional functional coupling under a strict `-D warnings` lint gate — no design, scope, or content change to either task).
**Impact on plan:** None on functionality or test coverage; both tasks' full scope, all specified tests, and all specified acceptance criteria are present and verified.

## Issues Encountered

- **The plan's literal acceptance-criteria command shape (`cargo test -p devflow-core release_execute -- prepare`) does not narrow to only "prepare"-named tests.** Empirically confirmed: Rust's default test harness treats multiple positional filter arguments as an OR (union), not an AND (intersection) — since `release_execute` (the first filter) already matches every test in that module, providing `prepare`/`merge_argv` afterward adds no further narrowing, and the command simply reports the module's full test count (30). This does not affect correctness of the underlying "at least N passed" acceptance thresholds (30 ≥ 8, 30 ≥ 20, 30 ≥ 28, 30 ≥ 4 all hold), but the literal command does not test what its own wording implies. No code change; verified interactively by invoking the compiled test binary directly with both filter arguments and observing the harness run all 30 rather than a narrower subset.
- **The acceptance criterion `rg -n 'workspace\.package|workspace\.dependencies' crates/devflow-core/src/release_execute.rs` returns no matches** is satisfied only by the file's **production** code — the same task's own `<action>` text mandates building test fixtures whose `Cargo.toml` content literally contains `[workspace.package]`/`[workspace.dependencies]` (to prove the two-place bump against a real fixture, per `29-PATTERNS.md`), so the full-file `rg` command necessarily matches those fixture-construction lines and doc comments. Verified the underlying intent by re-running the same `rg` command against only the file's content preceding `#[cfg(test)]` (the production code): zero matches, confirming the two-place bump is genuinely delegated to `version::write_version` and never re-derived in production logic.

## User Setup Required

None — no external service configuration required. `gh` authentication (when present) is read passively by the new PR calls, exactly as `29-04`'s existing `open_pr`/`observe` call sites already do; no new environment variable or config key was introduced by this plan.

## Next Phase Readiness

- `devflow_core::release_execute`'s `PreparedBranch`, `release_branch_name`, `prepare_bump_branch`, `pr_argv`, `merge_argv`, `open_and_arm_pr` are all in place and stable for `29-06-PLAN.md` (steps 3 and 5's actions — `ReleasePrMerged`'s pull request and `SyncMerged`'s sync-back pull request) to reuse `open_and_arm_pr` directly with `MergeIntent::ReleaseCut`/`MergeIntent::SyncBack` respectively, rather than reimplementing the resolve-before-create pull-request flow.
- `29-07-PLAN.md` (the commit-point unit, steps 4 and 6 — signed tag and `cargo publish`) can rely on the same `action_for`/`unit_for` shape; no type or CLI-surface redesign is needed.
- No blockers. Live pull-request behavior (whether GitHub's auto-merge genuinely waits for green checks and merges with the requested method) remains deliberately out of this hermetic suite's scope, per `29-VALIDATION.md`'s Manual-Only Verifications table — unchanged from `29-04`.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_execute.rs`
- FOUND: `crates/devflow-core/src/version.rs`
- FOUND: `crates/devflow-core/src/hooks.rs`
- FOUND: `crates/devflow-cli/tests/release_cut.rs`
- FOUND commit `842e5b4` (feat: prepare version-bump/changelog branch and open+arm its PR)
