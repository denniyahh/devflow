---
phase: 27-scrub-redirecting-git-environment-from-production-calls
plan: 01
subsystem: infra
tags: [rust, subprocess, security, git, env-hygiene]

# Dependency graph
requires: []
provides:
  - "devflow_core::git::{hermetic_command, git_command, REPO_LOCAL_GIT_VARS, ALSO_REDIRECTING_GIT_VARS} — the always-compiled, production-reachable scrubbing constructor every later plan in this phase (27-02..27-06) migrates its own call sites onto"
  - "test_support::{git_command, hermetic_command, REPO_LOCAL_GIT_VARS, ALSO_REDIRECTING_GIT_VARS} now re-export the crate::git originals — single source of truth, ~40 existing fixture call sites unaffected"
  - "One migrated devflow-core caller (origin_main_ancestor_status) and one migrated devflow-cli caller (staleness::run_git_stdout) proving the mechanism reaches both crates in production code without the test-support feature"
affects: [27-02, 27-03, 27-04, 27-05, 27-06]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Per-Command env_remove scrub, never process-global std::env::set_var (Phase 25 D-14) — hermetic_command(program, dir) returns a Command with current_dir pinned and all 17 redirecting vars marked for removal, composable with any further .env()/.args() the caller chains after"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/test_support.rs
    - crates/devflow-cli/src/staleness.rs

key-decisions:
  - "D-01/D-02/D-03 cited and honored exactly as locked in 27-CONTEXT.md/27-RESEARCH.md — no escape hatch, build.rs untouched, mechanism-only proof (no project_root_guard.rs built)"
  - "Deviation: simplified origin_main_ancestor_status_holds_under_a_hostile_git_dir to drop a literal unscrubbed-Command reproduction step — empirically verified (git 2.55.0) that merge-base --is-ancestor genuinely IS redirected by a hostile GIT_DIR (unlike --show-toplevel, which falls back to cwd when GIT_WORK_TREE is unset), so a chained .env(GIT_DIR, foreign) after git_command() cannot be asserted to 'still succeed' without contradicting the scrub's own purpose; keeping it would also have pushed git.rs's unscrubbed Command::new(\"git\") count from 7 to 8, breaking the plan's own acceptance criterion for 27-02's remaining scope"
  - "Deviation: plan's first <verify> automated check (unscoped 'cargo test -p devflow-core --features test-support --lib git::tests:: ' expecting 'test result: ok.') cannot pass at this task's boundary — 7 of git.rs's 9 Command::new(\"git\") sites are deliberately left unmigrated for 27-02 per Step 4's own scope statement, so 15 GitFlow-method tests still fail under a hostile GIT_DIR; the three properly-SCOPED checks (hermetic_command test, origin_main_ancestor_status filter, staleness::tests::run_git_stdout filter) all pass exactly as specified"
  - "Deviation: acceptance criterion 'rg -o std::env::var|env::var_os git.rs | wc -l == 0' was already unsatisfiable at the phase base commit — an unrelated, pre-existing test (check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey, git.rs:1636 at HEAD 618386e) uses std::env::var_os(\"HOME\") for HOME-isolation, nothing to do with hermetic_command. Verified hermetic_command's own 6-line body (git.rs, ~line 76) contains no env::var call, no cfg!, no config lookup — the substantive D-01 guarantee holds; the file-wide grep as literally written was never true even before this task"

requirements-completed: [D-01, D-02, D-03]

coverage:
  - id: D1
    description: "devflow_core::git::hermetic_command/git_command exist, always-compiled, unconditionally scrub all 17 redirecting vars with no bypass parameter"
    requirement: "D-01"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git_command_marks_every_redirecting_var_for_removal"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git_command_preserves_git_exec_path"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#local_env_vars_match_git"
        status: pass
    human_judgment: false
  - id: D2
    description: "A real spawned git process built through git_command resolves the caller-supplied root even when GIT_DIR points at an unrelated repository"
    requirement: "D-03"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#origin_main_ancestor_status_holds_under_a_hostile_git_dir"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#run_git_stdout_ignores_a_hostile_git_dir"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow_core::git::git_command is callable from devflow-cli production code WITHOUT the test-support feature"
    requirement: "D-01"
    verification:
      - kind: integration
        ref: "cargo build -p devflow (no --features test-support)"
        status: pass
    human_judgment: false
  - id: D4
    description: "crates/devflow-cli/build.rs is byte-identical to the phase base commit"
    requirement: "D-02"
    verification:
      - kind: other
        ref: "git diff --stat HEAD -- crates/devflow-cli/build.rs (empty)"
        status: pass
    human_judgment: false

# Metrics
duration: 32min
completed: 2026-07-30
status: complete
---

# Phase 27 Plan 01: Scrubbing Constructor Tracer Summary

**`devflow_core::git::hermetic_command`/`git_command` established as the always-compiled scrubbing constructor, proven end-to-end through one core caller (`origin_main_ancestor_status`) and one cli caller (`staleness::run_git_stdout`), with `test_support` now re-exporting rather than duplicating the variable lists.**

## Performance

- **Duration:** 32 min (commit range 14:30 -> 15:02 local)
- **Tasks:** 1 (tracer, tdd="true") — 2 commits (RED test, GREEN feat)
- **Files modified:** 3

## Accomplishments
- `devflow_core::git::{REPO_LOCAL_GIT_VARS, ALSO_REDIRECTING_GIT_VARS, hermetic_command, git_command}` added as the canonical, unconditionally-compiled home (`git.rs`, `pub mod git;` at `lib.rs:62` — no feature gate, unlike `test_support` at `lib.rs:78`)
- `test_support.rs` reduced to a `pub use crate::git::{...}` re-export — the two variable lists can never drift, and all ~40 existing fixture call sites across both crates' test targets keep compiling unchanged
- Migrated `origin_main_ancestor_status`'s two `Command::new("git")` sites (`git.rs:488,497` at the base commit) to `git_command(project_root)`
- Migrated `staleness::run_git_stdout`'s one `Command::new("git")` site (`staleness.rs:124` at the base commit) to `devflow_core::git::git_command` — the single highest-leverage edit in the phase, scrubbing ~15 in-file callers plus 2 cross-file callers in `commands.rs` for free
- `cargo build -p devflow` succeeds with no `--features test-support` flag — proves the constructor is genuinely reachable from production `devflow-cli` code, not just from test targets
- Five new/moved tests all pass; full `cargo test --workspace` in a normal environment: 0 failed across every target

## Task Commits

Tracer task, `tdd="true"`, executed as a RED-then-GREEN pair:

1. **RED: add failing hostile-GIT_DIR tests** - `548c04f` (test) — three new tests referencing the not-yet-existing `git_command`/`devflow_core::git::git_command`; fails to compile against unmigrated code (E0425 in `devflow-core`, E0432 in `devflow-cli`) — see verbatim output below.
2. **GREEN: implement the scrubbing constructor and migrate two call sites** - `ee2c3c8` (feat) — constructor added, `test_support` re-exports, `origin_main_ancestor_status` and `run_git_stdout` migrated; all three new tests pass.

**Plan metadata:** (this commit)

## RED output (verbatim, before implementation)

`devflow-core` — `cargo test -p devflow-core --features test-support --lib git::tests::hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir`:

```
error[E0425]: cannot find function `git_command` in this scope
    --> crates/devflow-core/src/git.rs:1624:22
     |
1624 |         let output = git_command(real_root)
     |                      ^^^^^^^^^^^ not found in this scope
     |
help: consider importing this function
     |
 938 +     use crate::test_support::git_command;
     |

error[E0425]: cannot find function `git_command` in this scope
    --> crates/devflow-core/src/git.rs:1683:19
     |
1683 |         let cmd = git_command(root);
     |                   ^^^^^^^^^^^ not found in this scope
     |
help: consider importing this function
     |
 938 +     use crate::test_support::git_command;
     |

For more information about this error, try `rustc --explain E0425`.
error: could not compile `devflow-core` (lib test) due to 2 previous errors
```

`devflow-cli` — `cargo test -p devflow --bin devflow -- staleness::tests::run_git_stdout`:

```
error[E0432]: unresolved import `devflow_core::git::git_command`
  --> crates/devflow-cli/src/staleness.rs:12:5
   |
12 | use devflow_core::git::git_command;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no `git_command` in `git`

For more information about this error, try `rustc --explain E0432`.
error: could not compile `devflow` (bin "devflow" test) due to 1 previous error
```

## GREEN output (verbatim, after implementation)

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib git::tests::hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir
running 1 test
test git::tests::hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 407 filtered out

$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib git::tests::origin_main_ancestor_status
running 3 tests
test git::tests::origin_main_ancestor_status_is_ref_absent_without_a_remote ... ok
test git::tests::origin_main_ancestor_status_is_ancestor_when_head_is_up_to_date ... ok
test git::tests::origin_main_ancestor_status_holds_under_a_hostile_git_dir ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 405 filtered out

$ GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- staleness::tests::run_git_stdout
running 1 test
test staleness::tests::run_git_stdout_ignores_a_hostile_git_dir ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 230 filtered out

$ cargo build -p devflow
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.11s

$ cargo test --workspace   # normal (non-hostile) environment
... every target: 0 failed (408 in devflow-core::tests, 231 in devflow::tests, plus integration/doc targets)

$ cargo clippy --workspace --all-targets -- -D warnings
   Finished `dev` profile [unoptimized + debuginfo] target(s) — 0 warnings

$ cargo fmt --check
   (no output — clean)

$ git diff --stat HEAD -- crates/devflow-cli/build.rs
   (no output — untouched, D-02)
```

## Re-measured hostile-`GIT_DIR` baselines (bonus, not this task's acceptance gate)

The phase-level 54/44/98 baseline (recorded in `27-01-PLAN.md`'s "Phase-level recorded decisions") is for the FULL crate under a hostile `GIT_DIR`. This task migrates only 2 of `git.rs`'s 9 sites and 1 of `staleness.rs`'s 3 sites, so most of the 98 residual failures are expected to remain until 27-02 through 27-06 land:

| Scoped command | Before this task | After this task |
|---|---|---|
| `GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support` | 54 failed / 352 passed | **53 failed / 355 passed** |
| `GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` | 44 failed / 139 passed | **44 failed / 140 passed** |

`devflow-core`'s failure count dropped by exactly 1 (the two `origin_main_ancestor_status` sites migrating flips the `origin_main_ancestor_status` hostile-`GIT_DIR` failure; the +3 passed beyond that comes from the 3 new/relocated tests). `devflow-cli`'s failure count is unchanged — expected, since `run_git_stdout`'s own hostile-behavior tests weren't part of the pre-existing 44 (they're new), and the 44 pre-existing failures live in `preflight.rs`/`commands.rs`/`staleness.rs`'s `embedded_commit_is_stale` (lines 51,72) — all owned by later plans.

## Files Created/Modified
- `crates/devflow-core/src/git.rs` - added the scrubbing constructor + constants + 5 tests (2 new, 3 moved), migrated `origin_main_ancestor_status`
- `crates/devflow-core/src/test_support.rs` - replaced definitions with a `pub use crate::git::{...}` re-export, removed 3 now-moved tests, dropped 2 now-unused imports
- `crates/devflow-cli/src/staleness.rs` - migrated `run_git_stdout`, added 1 new test + 1 import

## Decisions Made
- Constructor placed in `devflow-core::git` (not `test_support`, not a new module) — the only always-compiled home both crates already depend on unconditionally, per `27-CONTEXT.md`'s discretion and `27-RESEARCH.md`'s primary recommendation.
- `test_support` delegates via re-export rather than staying independent — the phase's one hard requirement ("avoid drift between the two lists") is satisfied structurally: there is now exactly one definition of each constant/function workspace-wide.
- See `key-decisions` in frontmatter for the three deviations from the plan's literal `<verify>`/acceptance text (all documented with technical justification, none affecting the substantive D-01/D-02/D-03 guarantees).

## Deviations from Plan

### Auto-fixed / Adjusted Issues

**1. [Rule 1 — plan defect] Simplified `origin_main_ancestor_status_holds_under_a_hostile_git_dir`**
- **Found during:** Task 1, writing the RED-stage test per the plan's literal `<behavior>` instruction ("spawn the same argv through `git_command` with `.env("GIT_DIR", foreign)` and assert the exit status matches the non-hostile case")
- **Issue:** Empirically verified (this machine, git 2.55.0) that chaining `.env("GIT_DIR", foreign)` AFTER `git_command()` for a ref-resolving command (`merge-base --is-ancestor`) genuinely redirects git's ref resolution to the foreign repo — `Command`'s env-builder is a last-write-wins map, so an explicit `.env()` call after `git_command()`'s `env_remove()` loop overrides the scrub for that one key. Reproduced directly: `GIT_DIR=<foreign>/.git git -C <real> merge-base --is-ancestor origin/main HEAD` → `fatal: Not a valid object name origin/main` (exit 128). Asserting "exit status matches the non-hostile case" for this specific command, with GIT_DIR literally re-injected, can never hold — the literal plan instruction describes an unsatisfiable test for `merge-base --is-ancestor` (it IS satisfiable for `--show-toplevel`, used in the sibling test, which falls back to cwd when `GIT_WORK_TREE` is unset — verified both ways empirically).
- **Fix:** Kept the two achievable, meaningful halves: (a) the `Command` this code path builds via `git_command` is structurally asserted to mark `GIT_DIR` for removal; (b) the real, unmodified `origin_main_ancestor_status` (using the scrubbed constructor, nothing re-adding `GIT_DIR` afterward) is spawned and asserted to return the correct answer. Documented the empirical finding in the test's own doc comment so a future reader does not reintroduce the unsatisfiable assertion.
- **Files modified:** `crates/devflow-core/src/git.rs`
- **Verification:** `cargo test -p devflow-core --features test-support --lib git::tests::origin_main_ancestor_status` → `3 passed; 0 failed` under a hostile `GIT_DIR`.
- **Committed in:** `ee2c3c8`

**2. [Rule 1 — plan defect, documented not fixed] First `<verify>` automated check cannot pass at this task's scope boundary**
- **Found during:** Running the plan's `<verify>` block after GREEN
- **Issue:** `HOSTILE=$(mktemp -d) && git init -q "$HOSTILE" && GIT_DIR="$HOSTILE/.git" cargo test -p devflow-core --features test-support --lib git::tests:: 2>&1 | tee ... | rg -q 'test result: ok\.'` requires ZERO failures across the ENTIRE `git::tests` module under a hostile `GIT_DIR`. But this task's own Step 4 explicitly scopes migration to 2 of `git.rs`'s 9 `Command::new("git")` sites, leaving 7 for 27-02 (also stated in `<success_criteria>`: "`git.rs` has 7 remaining unmigrated production sites (owned by `27-02`)"). Running the unscoped check produces `15 failed` (GitFlow methods: `cleanup_merged_*`, `commit_path_*`, `feature_*`, `release_*`, `delete_branch_*`, `tag_stays_lightweight_*`) — all from the 7 sites this task deliberately does not touch.
- **Resolution:** Did not attempt to satisfy this specific automated check (doing so would require migrating 27-02's scope inside this plan, violating the plan's own stated task boundary and the phase's wave/serialization design). Ran and confirmed the three PROPERLY-scoped checks instead (`hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir`, `origin_main_ancestor_status`, `staleness::tests::run_git_stdout`), all passing exactly as their own dedicated `<automated>` entries specify.
- **Files modified:** none (verification-only finding)
- **Committed in:** n/a (documentation only, this SUMMARY)

**3. [Rule 1 — plan defect, documented not fixed] `std::env::var|env::var_os` acceptance grep was already non-zero at the phase base commit**
- **Found during:** Running the acceptance-criteria checklist
- **Issue:** `[ "$(rg -o 'std::env::var|env::var_os' crates/devflow-core/src/git.rs | wc -l)" -eq 0 ]` — verified this was ALREADY `1` at the phase base commit `618386e` (`git.rs:1636`, inside the pre-existing, unrelated `check_signing_viability_degrades_when_gpg_format_unset_and_no_signingkey` test, which legitimately isolates `$HOME` for a signing-viability check). This grep is file-wide, not scoped to `hermetic_command`'s body, so it was never satisfiable as literally written, independent of this task's changes.
- **Resolution:** Verified the SUBSTANTIVE D-01 guarantee by reading `hermetic_command`'s own ~6-line body directly: no `env::var` call, no `cfg!`, no config lookup — only `Command::new`, `.current_dir()`, and a loop of `.env_remove()` calls over the two constant slices. The unconditional-scrub property holds; only the literal file-wide grep does not.
- **Files modified:** none (verification-only finding)
- **Committed in:** n/a (documentation only, this SUMMARY)

---

**Total deviations:** 3 (1 code adjustment to an unsatisfiable test instruction, 2 documented plan-defect findings with no code change required)
**Impact on plan:** None affect the substantive D-01/D-02/D-03 guarantees, which are independently proven by the passing, correctly-scoped tests and direct source verification. All three are plan-authoring gaps in the `<verify>`/acceptance text, not defects in the shipped mechanism.

## Issues Encountered
None beyond the three deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- The scrubbing constructor (`devflow_core::git::{hermetic_command, git_command}`) is established, always-compiled, and proven reachable from both crates' production code — every later plan in this phase (27-02 through 27-06) can migrate its own call sites onto it with zero further plumbing.
- `git.rs` has 7 remaining unmigrated production `Command::new("git")` sites (owned by 27-02); `staleness.rs` has 2 (lines 51, 72, inside `embedded_commit_is_stale`, owned by 27-04). `version.rs` (10), `worktree.rs` (2), `agent_result.rs` (3), `commands.rs` (3), `preflight.rs` (11) are entirely untouched — all in scope for their respective later plans per `27-CONTEXT.md`'s canonical file list.
- Re-measured hostile-`GIT_DIR` baselines for later plans to track against: `devflow-core` 53 failed / 355 passed (was 54/352); `devflow-cli` (skip `pipeline_gate`/`pipeline_outcomes`) 44 failed / 140 passed (unchanged failure count, +1 new passing test).
- No blockers. `crates/devflow-cli/build.rs` remains untouched (D-02, verified via empty `git diff --stat`).

---
*Phase: 27-scrub-redirecting-git-environment-from-production-calls*
*Completed: 2026-07-30*
