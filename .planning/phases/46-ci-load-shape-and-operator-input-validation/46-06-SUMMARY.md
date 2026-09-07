---
phase: 46-ci-load-shape-and-operator-input-validation
plan: 06
subsystem: testing
tags: [rust, libtest, process-isolation, env-mutex, set_var, test-harness, gap-closure]

requires:
  - phase: 46-ci-load-shape-and-operator-input-validation
    provides: "46-REVIEWS.md finding C-01 and the operator disposition rejecting the `env_lock` sweep in favour of child-processing the PATH-emptying tests; the seven-site inventory (six `NoGitPath::install()` plus the ad-hoc site at `pipeline_launch.rs:2468`)"
provides:
  - "`test_support::run_test_without_git` — re-execs the test binary for exactly one test with `PATH` pointing at an empty directory, set on the `Command` only, with no `std::env::set_var` anywhere"
  - "`test_support::assert_child_ran_exactly_one_passing_test` — a four-part anti-vacuity check (exit status, `test <name> ... ok`, `1 passed`, NON-ZERO `filtered out`) that prints the child's captured stdout/stderr on every failure"
  - "`test_support::CHILD_NO_GIT_ROOT` / `child_no_git_root()` — child-mode detection read before any re-exec, which is what makes recursion impossible"
  - "`an_empty_path_child_cannot_resolve_git_while_the_parent_still_can` — the both-directions proof the six later rewrites inherit"
  - "site 2 of 7 migrated: `resume_re_marks_stopped_when_launch_stage_fails_outright` no longer mutates process-global `PATH`"
affects: [46-07, 46-08, phase-46 verification, any future test needing an unrunnable git]

actuals:
  tokens: 4616
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Remove a process-global hazard by relocating it into a child process, rather than serialising every possible victim around it"
    - "A child-process assertion needs an anti-vacuity check on the child's libtest summary — `status.success()` alone is a false green because a filter matching nothing exits 0"
    - "Both directions or it is not a measurement: the parent asserts the resource IS available, the child asserts it is NOT"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/pipeline_launch.rs

key-decisions:
  - "The window moves into a child process rather than widening the `env_lock` sweep. `d525f9a` closed 47 of at least 78 exposures because it scanned for DIRECT git spawns; every test reaching git through a helper was invisible to it. A child's emptied `PATH` is invisible to the parent, so the hazard stops existing rather than being serialised around."
  - "`PATH` is set to an EMPTY DIRECTORY, not removed. `env_remove` is a different experiment — some resolvers fall back to a built-in default path when `PATH` is absent, which would leave `git` resolvable. Keeping the empty-directory form makes this a relocation of `NoGitPath`'s experiment, not a redefinition of it."
  - "The child check is four assertions, not `status.success()`. A libtest filter matching nothing prints `test result: ok. 0 passed` and exits 0 — observed directly in negative control NC-B, not assumed."
  - "`ErrorKind::NotFound` specifically, not `is_err()`. `Command::output()` returns `Err(NotFound)` only when the program cannot be SPAWNED; a git that runs and exits non-zero returns `Ok(status)`. Accepting any error kind would accept a guard blocking git for the wrong reason."
  - "The plan's `set_var(\"PATH\"` acceptance grep was file-wide and contradicted the plan's own prohibition against touching the `neutral_path_dir` sites. It was replaced with a body-scoped check plus a before/after file-wide delta — see Deviation 1. The unscoped form could only ever have been satisfied by breaking 19 other tests."
  - "The old test name is deliberately not repeated anywhere under `crates/`, including in the new doc comment that explains the rename, so the plan's zero-reference grep stays a true zero. Git history carries it."

patterns-established:
  - "Parent builds the fixture (it needs the resource), child runs the assertion under the resource's absence, parent asserts the child ran exactly one passing test"
  - "Prove a scoped grep discriminates by running it against a sibling that must produce the opposite count, not by observing a single zero"
  - "Hold the `TempDir` naming a child's `PATH` in a local binding across the whole `.output()` call — dropping it earlier silently changes the experiment"

requirements-completed: [INFRA-01]

coverage:
  - id: D1
    description: "A test can run with `git` unresolvable without mutating the parent process's `PATH`, proven in both directions"
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/test_support.rs#an_empty_path_child_cannot_resolve_git_while_the_parent_still_can — `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 362 filtered out`"
        status: pass
      - kind: unit
        ref: "negative control NC-A: child PATH left inherited -> child FAILS, parent prints child's `git version 2.55.0` output"
        status: pass
      - kind: unit
        ref: "negative control NC-B: filter misspelled -> child prints `test result: ok. 0 passed` and exits 0; the three-part assertion catches it"
        status: pass
    human_judgment: false
  - id: D2
    description: "The helper never calls `std::env::set_var`; `PATH` and the child-mode variable are set on the spawned `Command` only"
    requirement: INFRA-01
    verification:
      - kind: other
        ref: "`grep -n set_var crates/devflow-cli/src/test_support.rs` -> 12 hits, all inside `NeutralPath`/`NoGitPath` (4 code, 8 prose); zero inside `run_test_without_git`"
        status: pass
    human_judgment: false
  - id: D3
    description: "Site 2 of 7 migrated: the ad-hoc PATH-emptying window in `resume_re_marks_stopped_when_launch_stage_fails_outright` runs in a child, keeping every assertion it made before"
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#resume_re_marks_stopped_when_launch_stage_fails_outright — `test result: ok. 1 passed; ... 362 filtered out`"
        status: pass
      - kind: other
        ref: "deliberate-failure probe (inverted `reloaded.stopped`) -> child FAILED in 0.00s, parent reported `child test process must exit 0` with the child's panic at pipeline_launch.rs:2466 inlined"
        status: pass
      - kind: other
        ref: "body-scoped grep: migrated test `set_var(\"PATH\"`=0, sibling `resume_with_agent_from_a_rate_limited_state_relaunches`=2 (negative control); `empty_path_dir` file-wide 2 -> 0"
        status: pass
    human_judgment: false
  - id: D4
    description: "Nothing else regressed: `NoGitPath` and its five remaining callers still compile, the whole workspace is green, clippy and fmt are clean, and `d525f9a`'s 47 lines are untouched"
    requirement: INFRA-01
    verification:
      - kind: unit
        ref: "`cargo test -p devflow --bin devflow` -> `test result: ok. 363 passed; 0 failed` (cargo_exit=0)"
        status: pass
      - kind: integration
        ref: "`cargo test --workspace` -> all suites ok including `devflow_core` 764 passed (workspace_test_exit=0)"
        status: pass
      - kind: other
        ref: "`cargo clippy --workspace --all-targets -- -D warnings` clippy_exit=0; `cargo fmt --all -- --check` fmt_exit=0"
        status: pass
      - kind: other
        ref: "`NoGitPath::install` sites in pipeline_outcomes.rs = 5; env_lock holders under crates/ 135 -> 133, both removals proven pre-existing (d525f9a never touched test_support.rs; its one pipeline_launch.rs line went into code_unknown_does_not_transition_to_validate)"
        status: pass
    human_judgment: false

duration: 25 min
completed: 2026-09-06
status: complete
---

# Phase 46 Plan 06: Child-process the PATH-emptying tests (C-01, sites 1-2 of 7) Summary

**The window in which `git` is unresolvable now lives in a child process rather than in this
process's `PATH`, proven in both directions by the test it replaces — the parent resolves `git`
before and after, the child gets `ErrorKind::NotFound` — and the two PATH-emptying sites outside
`pipeline_outcomes.rs` are migrated and have dropped their `env_lock()`.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-09-06T07:10-04:00 (approx; first task commit 07:30:02)
- **Completed:** 2026-09-06T07:34:18-04:00
- **Tasks:** 2 of 2
- **Files modified:** 2 changed (238 insertions, 65 deletions)

**Note on the `actuals.tokens` scale.** Recorded as `4616` = realized-diff chars / 4
(18,464 chars across the two commits). That is the "realized diff" reading of the
`estimateTokens` scale. The *other* reading — chars/4 over the full content of every file
touched — gives **60,355**, because both files are large and pre-existing. 46-05's `15410`
appears to be the second reading over newly-created files, where the two readings nearly
coincide. They do not coincide here, so the number is labelled rather than left to be compared
against a differently-computed sibling. The plan's estimate was 33,000.

## Accomplishments

1. **`run_test_without_git(test_name, root)`** in `test_support.rs`: re-execs
   `std::env::current_exe()` with `[test_name, "--exact", "--test-threads=1"]`, `PATH` set to a
   fresh empty `TempDir` and `DEVFLOW_CHILD_NO_GIT_ROOT` set to the fixture root — both `.env()`
   on the `Command`, never `std::env::set_var`. The `TempDir` is held in a local binding across
   the whole `.output()` call and dropped explicitly afterwards. No `--nocapture`.

2. **`assert_child_ran_exactly_one_passing_test(out, test_name)`**: four assertions, each
   documented with the vacuity it alone catches — exit status, `test {name} ... ok`, `1 passed`,
   and a parsed NON-ZERO `filtered out`. Every failure message inlines the child's full stdout
   and stderr, because libtest captured the child's output and the failure is otherwise invisible
   from the parent.

3. **`CHILD_NO_GIT_ROOT` / `child_no_git_root()`**: one shared variable name, read before any
   re-exec, which is what makes recursion structurally impossible. A hand-exported value puts a
   test into child mode with no hostile `PATH`; that is documented and fails loudly rather than
   passing vacuously.

4. **Site 1 migrated and renamed** —
   `an_empty_path_child_cannot_resolve_git_while_the_parent_still_can`. Child asserts
   `ErrorKind::NotFound`; parent asserts `git` resolves before the spawn AND after the child
   returns, and that its own `PATH` is byte-identical across the run. `env_lock()` dropped.

5. **Site 2 migrated** — `resume_re_marks_stopped_when_launch_stage_fails_outright`. Parent builds
   the repo and state file (both shell out to git); child calls `resume()` under the empty `PATH`
   and keeps all four original assertions. The `env_lock()`, the `empty_path_dir` `TempDir`, the
   `unsafe { set_var("PATH", …) }` and its `unsafe` restore block are gone.

6. **`NoGitPath`'s doc comment** now states it is superseded, names the helper, and says the type
   survives only until 46-07 migrates its five remaining callers. It is not deleted — deleting it
   here would break the build.

## Measurements — every number below was produced this session

### The `1 passed` / `filtered out` pair

| Test | Result line |
|---|---|
| `test_support::tests::an_empty_path_child_cannot_resolve_git_while_the_parent_still_can` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 362 filtered out; finished in 0.01s` (cargo_exit=0) |
| `pipeline_launch::tests::resume_re_marks_stopped_when_launch_stage_fails_outright` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 362 filtered out; finished in 0.03s` (cargo_exit=0) |

Both non-zero filter counts. 363 tests exist in the bin target, so `362 filtered out` is the
expected value and not an artifact.

### Negative control NC-A — the emptied `PATH` is what hides `git`

`run_test_without_git` temporarily changed to pass the inherited `PATH` instead of the empty
directory. The child ran and FAILED:

```
thread '…an_empty_path_child_cannot_resolve_git_while_the_parent_still_can' panicked at
  crates/devflow-cli/src/test_support.rs:536:5:
child test process must exit 0; status ExitStatus(unix_wait_status(25856))
--- child stdout ---
running 1 test
test test_support::tests::an_empty_path_child_… ... FAILED
thread '…' panicked at crates/devflow-cli/src/test_support.rs:835:18:
the child's empty PATH must make `git` unresolvable: Output { status: ExitStatus(unix_wait_status(0)),
  stdout: "git version 2.55.0\n", stderr: "" }
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 362 filtered out
--- end child output ---
```

This is the load-bearing observation for the whole plan: the child genuinely runs the assertion,
`git` genuinely resolves without the empty `PATH`, and the parent surfaces the child's captured
output. Reverted immediately after.

### Negative control NC-B — a filter matching nothing exits 0

`run_test_without_git` called with `{NAME}_typo`:

```
child stdout must carry `test test_support::tests::an_empty_path_child_… ... ok` —
  without it the child may have run some other test, or none
--- child stdout ---
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 363 filtered out
--- end child output ---
```

The child exited **0** while running **nothing**. `status.success()` alone would have accepted
this. This is exactly the false green CLAUDE.md records for `cargo test --exact <name>`, and it is
why the check is four assertions rather than one. Reverted immediately after.

### Deliberate-failure probe on Task 2 (required by the plan)

The child's `assert!(reloaded.stopped, …)` inverted to `assert!(!reloaded.stopped, …)`:

```
thread 'pipeline_launch::tests::resume_re_marks_stopped_when_launch_stage_fails_outright'
  panicked at crates/devflow-cli/src/test_support.rs:536:5:
child test process must exit 0; status ExitStatus(unix_wait_status(25856))
--- child stdout ---
running 1 test
test pipeline_launch::tests::resume_re_marks_stopped_… ... FAILED
thread '…' panicked at crates/devflow-cli/src/pipeline_launch.rs:2466:13:
DELIBERATE-FAILURE PROBE: inverted assertion
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 362 filtered out; finished in 0.00s
--- end child output ---
cargo_exit=101
```

It **failed in 0.00s**, it did not hang. That distinction is explicit because this repo has
already had a revert-based proof wedge on a never-answered gate and die to SIGTERM — a hang and a
failure are different observations and only one is evidence. The child's own panic message and
source location are visible from the parent, which is the property that makes a child-process
test debuggable at all. Restored and re-verified green.

### Rename completeness

```
stale_old_name_refs=1   ->  (removed the literal from my own new doc comment)  ->  0
rename_complete_exit=0
```

The `1` is the negative control: the grep was demonstrated to find a reference when one existed,
then to report zero once it did not. A zero observed only once would not discriminate.

### Site accounting — the running count 46-07 and 46-08 must sequence against

| Sites | Count | Where |
|---|---|---|
| Migrated by this plan | **2** | `test_support.rs` (site 1), `pipeline_launch.rs` (site 2) |
| Remaining | **5** | all in `pipeline_outcomes.rs`, all via `NoGitPath::install()` — measured, `grep -rn NoGitPath::install crates/` returns exactly 5 |
| Total | 7 | matches the operator disposition |

`NoGitPath` still exists and still compiles — deliberately. **46-08's revert is not yet safe:**
five PATH-emptying windows remain live, so the 47 sibling `env_lock()` holders are still load-bearing.

### `env_lock` accounting

```
env_lock_holders_at_base=135
env_lock_holders_in_worktree=133
delta=2
```

135 = 88 pre-existing + 47 from `d525f9a`, matching 46-REVIEWS.md. Both removals are **proven** to
be pre-existing holders, not part of `d525f9a`'s 47:

- `d525f9a --stat` does not list `crates/devflow-cli/src/test_support.rs` at all, so site 1's
  guard predates it.
- `d525f9a` added exactly one line to `pipeline_launch.rs` (`@@ -2499,0 +2500 @@`), and a `-U6`
  read shows it landed in `code_unknown_does_not_transition_to_validate`, not in the migrated test.

`d525f9a`'s 47 lines are therefore untouched, as the plan requires.

### Suite, clippy, fmt

```
cargo test -p devflow --bin devflow  -> test result: ok. 363 passed; 0 failed   cargo_exit=0
cargo test --workspace               -> all suites ok (devflow_core 764 passed) exit=0
cargo clippy --workspace --all-targets -- -D warnings                          clippy_exit=0
cargo fmt --all -- --check                                                     fmt_exit=0
```

## Deviations from Plan

### 1. [Rule 1 - Bug] Task 2's `set_var("PATH"` acceptance grep was file-wide and self-contradictory

- **Found during:** Task 2, running the plan's second `<automated>` block verbatim.
- **Issue:** the block computes `n` over the whole of `pipeline_launch.rs` and asserts
  `test "$n" -eq 0`. It returned `path_setvar_sites=38`, `no_path_mutation_exit=1`. Those 38 are
  the `prepend_path`/stub-dir sites belonging to 19 other tests — the sites the plan's own
  prohibitions say **must not** be touched ("Must NOT touch the `neutral_path_dir` sites. Those
  install a PATH that still contains a real `git`"). The criterion as written could only have been
  satisfied by violating the prohibition. The `empty_path_dir` half of the same block was correctly
  scoped by accident, because that variable name existed only at the migrated site.
- **Fix:** replaced the measurement rather than the code. Two checks, both with negative controls:
  1. **Body-scoped:** extract the migrated test's body (comment lines stripped) and count.
     `resume_re_marks_stopped_when_launch_stage_fails_outright: set_var_PATH=0, empty_path_dir=0`.
     Negative control on a sibling that legitimately still mutates `PATH`:
     `resume_with_agent_from_a_rate_limited_state_relaunches: set_var_PATH=2`. The check
     discriminates.
  2. **File-wide delta:** `path_setvar_before=40`, `path_setvar_after=38`, `delta=2` — exactly the
     two lines the migration removes, nothing else moved.
- **A first attempt at the negative control was itself broken and is recorded rather than hidden:**
  an `awk`-based extractor returned `body_lines=0` for the control test, which reported
  `set_var_PATH=0` — an *absent* extraction reading as a *clean* result. The `body_lines` counter
  is what exposed it. The extractor was rewritten to locate the function by line number and to
  fail loudly on a name it cannot find.
- **Files modified:** none (measurement-only fix).
- **Commit:** the corrected numbers are recorded in `e4fcd0d`'s message.

### 2. [Rule 2 - Missing critical] The rename criterion caught my own doc comment

- **Found during:** Task 1, running the plan's rename grep.
- **Issue:** the doc comment I wrote to explain the rename quoted the old test name, so
  `stale_old_name_refs=1` and the criterion failed. The plan warns about exactly this class — a
  name surviving in prose or a message string rather than as a symbol.
- **Fix:** rephrased the comment to describe the rename without spelling the old name, and said so
  in the comment itself so the next reader does not helpfully re-add it. Re-ran: `0`.
- **Files modified:** `crates/devflow-cli/src/test_support.rs`.
- **Commit:** `8969d90`.

**Total deviations:** 2 auto-fixed (1 plan-measurement bug, 1 self-inflicted criterion failure).
**Impact:** no change to the delivered mechanism. Deviation 1 means Task 2's second `<automated>`
block **as written in the plan will fail for 46-07 as well** if that plan copies it — the same
file-wide shape applied to `pipeline_outcomes.rs` will count that file's own unrelated PATH sites.

## Issues Encountered

None blocking.

## What this does NOT establish

- **The C-01 flake is not fixed.** Five of seven PATH-emptying windows are still process-global.
  Any claim that the hazard is closed must wait for 46-07.
- **No flake was reproduced or measured here.** The argument is mechanical — a child's `PATH` is
  not the parent's `PATH` — not statistical. Nothing in this plan ran under CPU contention, and no
  before/after flake rate was measured. `d525f9a`'s own summary already noted that its 0/8-vs-1/8
  counts were weak evidence; this plan adds no counts at all, deliberately.
- **The helper is proven for `git` specifically.** The child hides *every* binary, but the only
  resolver exercised is `Command::new("git")` via `hermetic_command`, plus `ensure_agent_binary`'s
  `claude` lookup in site 2. A consumer relying on some third resolution path should re-prove it.
- **Nothing was run in the pinned CI container.** All results above are host runs on this worktree.
  The pre-push container gate has not been exercised for these commits, because this plan does not
  push.
- **`git blame` was not used to attribute the two removed `env_lock` lines.** The attribution rests
  on `d525f9a`'s own diff (`--stat` omits `test_support.rs`; the single `pipeline_launch.rs` hunk is
  in a different function), which is direct evidence about that commit but does not rule out some
  *other* commit in the 46 series having added them. Nothing in the plan depends on that distinction —
  what 46-08 must not revert is `d525f9a`'s 47, and those are intact.

## Next Phase Readiness

Ready for **46-07**: migrate the five `NoGitPath::install()` sites in `pipeline_outcomes.rs` using
`run_test_without_git` / `assert_child_ran_exactly_one_passing_test`, drop their `env_lock()`, and
delete `NoGitPath`. Two notes for that plan:

1. Copy the **body-scoped** grep shape from Deviation 1, not the plan-supplied file-wide one.
2. The child in those tests must not need `git` for its fixture — `pipeline_outcomes.rs`'s sites
   build repos, so the same parent-builds/child-asserts split applies.

**46-08 is still blocked.** Its revert of `d525f9a`'s 47 `env_lock` lines is only safe once all
seven windows are gone; five remain.

## Self-Check: PASSED

- `crates/devflow-cli/src/test_support.rs` — FOUND
- `crates/devflow-cli/src/pipeline_launch.rs` — FOUND
- commit `8969d90` — FOUND in `git log`
- commit `e4fcd0d` — FOUND in `git log`
- `cargo test --workspace` — exit 0, re-run after the final restore
