---
phase: 27-scrub-redirecting-git-environment-from-production-calls
plan: 02
subsystem: infra
tags: [rust, subprocess, security, git, env-hygiene, git-flow, worktree]

# Dependency graph
requires:
  - phase: 27-01
    provides: "devflow_core::git::{hermetic_command, git_command, REPO_LOCAL_GIT_VARS, ALSO_REDIRECTING_GIT_VARS} — the always-compiled scrubbing constructor"
provides:
  - "crates/devflow-core/src/git.rs — all 9 production git invocations (2 from 27-01, 7 here) construct through git_command, including all 4 GitFlow internal wrappers (git, git_output, git_raw, git_raw_combined) that every mutating GitFlow method (feature_finish, release_finish, tag, push, delete_branch, cleanup_merged, ...) inherits the scrub from"
  - "crates/devflow-core/src/worktree.rs — both production git invocations (run chokepoint and the independently-migrated list) construct through git_command(project_root)"
  - "New regression test worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir proving list()'s independent vulnerability was real (T-27-03) and is now closed"
affects: [27-03, 27-04, 27-05, 27-06]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Wrapper-first migration: migrating the 4 GitFlow internal wrapper methods scrubs all ~19 methods that route through them, without touching each caller individually"
    - "Independent-chokepoint verification: a file with a 'majority' wrapper (worktree.rs's run()) can still hide a second, un-routed direct call site (list()) — proven here by a dedicated regression test rather than assumed from the wrapper's coverage"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/worktree.rs

key-decisions:
  - "D-01/D-03 honored exactly as locked in 27-CONTEXT.md — no escape hatch, mechanism-only proof"
  - "Deviation: worktree::list's regression test could not reuse 27-01's 'chain hostile .env(GIT_DIR) after the constructor and assert immunity' technique verbatim, despite the plan's <behavior> instruction to do so — empirically verified (git 2.55.0) that `worktree list --porcelain` genuinely IS redirected by an explicit GIT_DIR override chained after the scrub (unlike `--show-toplevel`, which falls back to cwd when GIT_WORK_TREE is unset). Applied the sibling pattern from 27-01's own documented deviation for `merge-base --is-ancestor` instead: (a) a structural assertion that the general git_command constructor list depends on marks GIT_DIR for removal, (b) a real call to the unmodified list(real_root) production function, with the hostile GIT_DIR supplied ambiently by this test's own <verify> harness invocation (GIT_DIR=<hostile>/.git cargo test ... list_resolves_caller_root_under_a_hostile_git_dir) rather than chained explicitly in-test. This reaches the same RED-then-GREEN proof the plan asked for, just via the technique 27-01 itself found necessary for genuinely-redirectable git subcommands."

requirements-completed: [D-01, D-03]

coverage:
  - id: D1
    description: "git.rs's 7 remaining production git invocations (4 GitFlow wrappers + 3 free-standing sites) construct via git_command; zero unscrubbed Command::new(\"git\") remains"
    requirement: "D-01"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git:: (38 tests, hostile GIT_DIR)"
        status: pass
      - kind: other
        ref: "rg -v '^\\s*//' crates/devflow-core/src/git.rs | rg -o 'Command::new(\"git\")' | wc -l == 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "19b locale pinning (LC_ALL=C / LANG=C) survives the git_raw/git_raw_combined migration byte-identically"
    requirement: "D-01"
    verification:
      - kind: other
        ref: "rg -o 'env(\"LC_ALL\", \"C\")' crates/devflow-core/src/git.rs | wc -l == 2 (same for LANG); rg -n 'T-19-14|19b' still prints"
        status: pass
    human_judgment: false
  - id: D3
    description: "worktree.rs's run() chokepoint and independently-migrated list() both construct via git_command(project_root); a new regression test proves list() resolves the caller's root under a hostile GIT_DIR where it previously did not"
    requirement: "D-03"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/worktree.rs#tests::list_resolves_caller_root_under_a_hostile_git_dir"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/worktree.rs#worktree:: (9 tests, hostile GIT_DIR)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Both files pass clippy -D warnings and cargo fmt --check clean; full workspace build succeeds"
    verification:
      - kind: other
        ref: "cargo clippy -p devflow-core --all-targets --features test-support -- -D warnings; cargo fmt --check; cargo build --workspace"
        status: pass
    human_judgment: false

# Metrics
duration: 14min
completed: 2026-07-30
status: complete
---

# Phase 27 Plan 02: GitFlow and Worktree Scrub Summary

**All 9 of `git.rs`'s production git invocations (2 from `27-01`, 7 here — including the 4 `GitFlow` internal wrappers that every mutating method routes through) and both of `worktree.rs`'s (the `run` chokepoint plus the independently-migrated `list`) now construct through `git_command`, closing the retarget channel for merge, tag, push, branch-delete, cleanup, and worktree enumeration/creation/removal.**

## Performance

- **Duration:** 14 min (commit range 15:08 -> 15:22 local)
- **Tasks:** 2 (Task 1 `auto`, Task 2 `auto tdd="true"` RED-then-GREEN pair) — 3 commits
- **Files modified:** 2

## Accomplishments
- `git.rs`'s 4 `GitFlow` internal wrappers (`git`, `git_output`, `git_raw`, `git_raw_combined`) migrated to `git_command(&self.root)` — every mutating `GitFlow` method (`feature_start`, `feature_finish`, `merge_feature_into_develop`, `release_start`, `release_finish`, `tag`, `delete_branch`, `branch_tip`, `ensure_branch`, `checkout`, `delete_remote_branch`, `has_remote`, `push`, `cleanup_merged`, `commit_all`, `commit_path`, `divergence_from_develop`, `list_feature_branches`, `rev_count`) inherits the scrub from these 4 edits alone
- `git.rs`'s 3 free-standing sites (`is_merged_into_develop`, `branch_exists`, `git_config`) migrated to `git_command`
- 19b locale pinning (`.env("LC_ALL", "C")`, `.env("LANG", "C")`) on `git_raw`/`git_raw_combined` preserved verbatim, with its rationale comment intact
- `worktree.rs`'s `run()` chokepoint (covers `add`, `add_detached`, `remove`, `prune`) migrated to `git_command(project_root)`
- `worktree.rs`'s `list()` — proven NOT to route through `run()` — migrated independently, with a new regression test (`list_resolves_caller_root_under_a_hostile_git_dir`) proving the specific T-27-03 threat (a hostile `GIT_DIR` making `list()` enumerate a foreign repository's worktrees) is closed
- `git.rs`'s hostile-`GIT_DIR` unit suite: 38 passed, 0 failed (identical to the normal-environment count)
- `worktree.rs`'s hostile-`GIT_DIR` unit suite: 9 passed, 0 failed (identical to the normal-environment count)
- Full `devflow-core` lib suite in a normal environment: 409 passed, 0 failed
- `cargo build --workspace` succeeds; `cargo clippy -p devflow-core --all-targets --features test-support -- -D warnings` and `cargo fmt --check` both clean

## Task Commits

1. **Task 1: Route git.rs's remaining seven production sites through the constructor** - `00ff5e7` (feat)
2. **Task 2a (RED): add failing hostile-GIT_DIR test for worktree::list** - `d0939ca` (test)
3. **Task 2b (GREEN): route worktree.rs's run and list through git_command** - `0edb190` (feat)

**Plan metadata:** (this commit)

## RED output (verbatim, before `worktree.rs`'s migration)

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir

running 1 test
test worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir ... FAILED

failures:

---- worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir stdout ----

thread 'worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir' panicked at crates/devflow-core/src/worktree.rs:419:9:
expected at least main + added worktree, got: [WorktreeInfo { path: "/tmp/tmp.v90lUI6xOC", branch: Some("main"), head: "0000000000000000000000000000000000000000" }]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 408 filtered out; finished in 0.60s
```

The unmigrated `list()` genuinely enumerated the **foreign** repository's own worktree (`/tmp/tmp.v90lUI6xOC`, the hostile `GIT_DIR` target) instead of the real root's worktrees — exactly the T-27-03 threat.

## GREEN output (verbatim, after `worktree.rs`'s migration)

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir

running 1 test
test worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 408 filtered out; finished in 0.50s

$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib worktree::

running 9 tests
test worktree::tests::add_errors_when_path_exists ... ok
test worktree::tests::add_creates_worktree_on_new_branch ... ok
test worktree::tests::list_includes_main_and_added_worktrees ... ok
test worktree::tests::add_existing_branch_without_creating ... ok
test worktree::tests::parse_porcelain_handles_detached_and_trailing_record ... ok
test worktree::tests::path_helpers_format_phase_numbers ... ok
test worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir ... ok
test worktree::tests::remove_deletes_the_worktree ... ok
test worktree::tests::prune_succeeds_on_clean_repo ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 400 filtered out; finished in 0.89s

$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib git::
... (38 tests)
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 370 filtered out; finished in 2.63s

$ cargo test -p devflow-core --features test-support --lib   # normal (non-hostile) environment
test result: ok. 409 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 15.71s

$ cargo build --workspace
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 48.45s

$ cargo clippy -p devflow-core --all-targets --features test-support -- -D warnings
   Finished `dev` profile [unoptimized + debuginfo] target(s) — 0 warnings

$ cargo fmt --check
   (no output — clean)
```

## Files Created/Modified
- `crates/devflow-core/src/git.rs` - migrated the 4 `GitFlow` internal wrappers and 3 free-standing sites to `git_command`; no test additions (the existing `git::` suite serves as the acceptance signal per D-03)
- `crates/devflow-core/src/worktree.rs` - migrated `run()` and `list()` to `git_command(project_root)`; added `use crate::git::git_command;`, removed the now-unused `use std::process::Command;`; added 1 new regression test

## Decisions Made
- Migrated the 4 `GitFlow` wrappers before the 3 free-standing sites, per the plan's chokepoint-first ordering — confirmed each of the 4 line numbers was genuinely inside a wrapper method body before treating it as covered.
- `worktree.rs`'s test fixture setup for the new regression test uses the already-scrubbed general `git_command` constructor directly (not the production `add()` function) so the test proves only `list()`'s own immunity, independent of `run()`'s migration status — avoiding a confound where `run()` being unmigrated during RED could break fixture setup itself rather than cleanly demonstrating `list()`'s specific vulnerability.
- See `key-decisions` in frontmatter for the one deviation from the plan's literal `<behavior>` instruction (all documented with technical justification, no impact on the substantive D-01/D-03 guarantees).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — plan defect, same class as 27-01's documented deviation] `list`'s regression test could not use the plan's literal chained-hostile-env technique**
- **Found during:** Task 2, writing the RED-stage test per the plan's literal `<behavior>` instruction ("Assert `worktree::list(real_repo)` returns worktree entries whose paths are under `real_repo`, with the foreign `GIT_DIR` injected into the spawned child only... Follow whatever injection technique `27-01`'s `hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir` established and reuse it verbatim")
- **Issue:** Empirically verified (this machine, git 2.55.0): `cd <real> && GIT_DIR=<foreign>/.git git worktree list --porcelain` genuinely redirects to enumerate `<foreign>`'s own single worktree entry, not `<real>`'s. Chaining `.env("GIT_DIR", foreign)` AFTER `git_command(real_root)` and asserting the result "still resolves real_root" (the literal `hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir` technique, which works for `--show-toplevel` because that specific subcommand falls back to cwd when `GIT_WORK_TREE` is unset) is unsatisfiable for `worktree list --porcelain` — the exact same class of problem 27-01 already hit and documented for `merge-base --is-ancestor`.
- **Fix:** Applied 27-01's own sibling pattern instead (`origin_main_ancestor_status_holds_under_a_hostile_git_dir`'s technique): (a) a structural assertion that the general `git_command` constructor `list` depends on marks `GIT_DIR` for removal; (b) a real, unmodified call to `list(real_root)`, with the hostile `GIT_DIR` supplied ambiently via this test's own `<verify>`-block harness invocation (`GIT_DIR=<hostile>/.git cargo test ... list_resolves_caller_root_under_a_hostile_git_dir`) rather than chained explicitly inside the test body. This reaches the same RED-then-GREEN proof the plan required — RED output above shows the unmigrated `list()` genuinely enumerating the foreign repo, GREEN shows it resolving the real root — via the technique 27-01 itself found necessary for a git subcommand that a hostile override genuinely redirects.
- **Files modified:** `crates/devflow-core/src/worktree.rs`
- **Verification:** `GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib worktree::tests::list_resolves_caller_root_under_a_hostile_git_dir` → `1 passed; 0 failed` (RED before migration: `0 passed; 1 failed`).
- **Committed in:** `d0939ca` (RED test), `0edb190` (GREEN fix)

---

**Total deviations:** 1 (test-technique adjustment, same class as a deviation 27-01 already documented; no impact on the substantive D-01/D-03 guarantees, which are independently proven by the passing, correctly-scoped tests)
**Impact on plan:** None — the shipped mechanism and its regression coverage match the plan's intent exactly; only the specific in-test injection idiom for one genuinely-redirectable git subcommand differs from the literal instruction, mirroring a gap the plan-authoring process already found and fixed once in `27-01`.

## Issues Encountered
None beyond the one deviation above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `git.rs` and `worktree.rs` are now fully migrated — zero unscrubbed production `Command::new("git")` remains in either file.
- `27-03` (`version.rs`, `agent_result.rs`), `27-04` (`staleness.rs`, `commands.rs`), `27-05` (`preflight.rs`) remain untouched by this plan and are unaffected — no file overlap with this plan's scope.
- Re-measured hostile-`GIT_DIR` baseline for `devflow-core`: was 53 failed / 355 passed after `27-01`; after this plan, `git::` and `worktree::` both reach `0 failed` under a hostile `GIT_DIR` (38 + 9 = 47 tests, all passing identically to the normal-environment run). The crate-wide hostile-`GIT_DIR` failure count for later plans to re-measure against will drop further as `27-03` through `27-06` land on their own files.
- No blockers.

---
*Phase: 27-scrub-redirecting-git-environment-from-production-calls*
*Completed: 2026-07-30*
