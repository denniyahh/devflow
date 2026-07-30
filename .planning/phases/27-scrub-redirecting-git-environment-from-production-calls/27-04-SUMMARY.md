---
phase: 27-scrub-redirecting-git-environment-from-production-calls
plan: 04
subsystem: infra
tags: [rust, subprocess, security, git, env-hygiene]

# Dependency graph
requires:
  - phase: 27-01
    provides: "devflow_core::git::{hermetic_command, git_command, REPO_LOCAL_GIT_VARS, ALSO_REDIRECTING_GIT_VARS} — the always-compiled scrubbing constructor"
provides:
  - "crates/devflow-cli/src/staleness.rs fully scrubbed — its last two direct git sites (embedded_commit_is_stale) now route through git_command(execution_root), completing the file 27-01 started via run_git_stdout"
  - "crates/devflow-cli/src/commands.rs fully scrubbed — all 3 direct git sites route through git_command(project_root), plus the one indirect sh -> cargo -> build.rs::run_git chain closed via hermetic_command(\"sh\", project_root)"
  - "RESEARCH Open Question #1 (test_cmd's sh -c spawn) formally decided IN SCOPE and closed"
  - "Two new hostile-GIT_DIR regression tests using a 'spawned child test process' injection technique (novel in this codebase) for command classes where chaining .env() directly onto a git_command()-built Command is unsatisfiable (merge-base --is-ancestor, rev-parse --verify)"
affects: [27-06]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Spawned-child-test-process hostile-GIT_DIR injection: for a private function whose git subcommand class (ref-resolving: merge-base --is-ancestor, rev-parse --verify) is empirically vulnerable to a GIT_DIR chained directly onto a git_command()-built Command (27-01-SUMMARY.md Deviation 1), the test re-invokes its own compiled test binary (std::env::current_exe()) as a child process, filtered to just that one test by name, with the hostile GIT_DIR set via Command::env() on that CHILD ONLY — never std::env::set_var on the parent test process (Phase 25 D-14). This is the technique both new tests in this plan use; it did not exist in the codebase before this plan."

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/staleness.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "RESEARCH Open Question #1 — commands.rs::test_cmd's sh -c \"cargo …\" spawn — decided IN SCOPE, exactly as the plan's own recorded decision states: closes the sh -> cargo -> build.rs::run_git -> git indirect chain at DevFlow's own spawn edge via hermetic_command(\"sh\", project_root), without editing build.rs (D-02 boundary preserved, verified via empty git diff --stat)."
  - "Deviation: neither new regression test uses the plan's literally-described '.env(\"GIT_DIR\", foreign) chained directly onto the real function's own Command' shape, because both embedded_commit_is_stale and tag_exists_and_reachable are private functions with no injection point AND their git subcommands (merge-base --is-ancestor, rev-parse --verify) are the SAME vulnerable ref-resolving class 27-01-SUMMARY.md Deviation 1 already found: chaining .env() after git_command() genuinely redirects them (unlike rev-parse --show-toplevel, which falls back to cwd). A literal reproduction against the real function would either be impossible to construct (no injection point) or, if constructed by other means, prove nothing about a real inherited/ambient hostile GIT_DIR. Built a spawned-child-test-process technique instead (see tech-stack.patterns) — genuinely fails before the migration (verified live, RED output below) and genuinely passes after (GREEN output below), all env mutation scoped to a freshly spawned child process, never this test's own process."
  - "Deviation: first draft of both tests' section-header doc comments literally contained the string 'git_command(execution_root)' / would have collided with the acceptance grep — caught by running the plan's own <acceptance_criteria> grep commands before committing, not after; reworded to 'the git_command constructor, called with execution_root' so the comment text no longer collides with the code-only grep pattern."

requirements-completed: [D-01, D-02, D-03]

coverage:
  - id: D1
    description: "staleness.rs's embedded_commit_is_stale (both merge-base --is-ancestor sites) constructs through git_command(execution_root), and a real hostile-GIT_DIR run proves it resolves the caller's own tree rather than a retargeted one"
    requirement: "D-01"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/staleness.rs#embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir"
        status: pass
      - kind: integration
        ref: "GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- staleness:: (43 passed, 0 failed)"
        status: pass
    human_judgment: false
  - id: D2
    description: "commands.rs's tag_exists_and_reachable (both sites) and phase_artifact_on_develop construct through git_command(project_root); a real hostile-GIT_DIR run proves a foreign repository's tag cannot be reported as belonging to the caller's repository (the T-27-01 false-positive direction)"
    requirement: "D-01"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir"
        status: pass
      - kind: integration
        ref: "GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- commands:: --skip pipeline_gate --skip pipeline_outcomes (102 passed, 0 failed)"
        status: pass
    human_judgment: false
  - id: D3
    description: "test_cmd's sh -c \"cargo …\" child constructs through hermetic_command(\"sh\", project_root), closing the indirect sh -> cargo -> build.rs::run_git -> git chain, without editing build.rs"
    requirement: "D-01"
    verification:
      - kind: other
        ref: "rg -o 'hermetic_command(\"sh\", project_root)' commands.rs == 1; rg -o 'Command::new(\"sh\")' commands.rs == 7 (down from 8); git diff --stat HEAD -- crates/devflow-cli/build.rs (empty)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Zero unscrubbed production git constructions remain in either file (comment-filtered grep), and the full workspace test suite (non-hostile) is unaffected"
    requirement: "D-03"
    verification:
      - kind: other
        ref: "rg -v '^\\s*//' staleness.rs commands.rs | rg -o 'Command::new(\"git\")' | wc -l == 0"
        status: pass
      - kind: integration
        ref: "cargo test --workspace (233 devflow-cli, 408 devflow-core, 0 failed everywhere)"
        status: pass
    human_judgment: false

# Metrics
duration: 23min
completed: 2026-07-30
status: complete
---

# Phase 27 Plan 04: Staleness/Commands Scrubbing Constructor Migration Summary

**`staleness.rs`'s two remaining `merge-base --is-ancestor` sites and `commands.rs`'s three direct git sites plus its one indirect `sh -> cargo -> build.rs` spawn now route through `devflow_core::git::{git_command, hermetic_command}`, proven via a novel spawned-child-test-process hostile-`GIT_DIR` injection technique (no in-codebase precedent) for the ref-resolving command class 27-01 found is genuinely redirected by a chained-on `.env()` call.**

## Performance

- **Duration:** 23 min (commit range 15:07 → 15:34 UTC)
- **Started:** 2026-07-30T19:12:02Z
- **Completed:** 2026-07-30T19:34:48Z
- **Tasks:** 2, both `tdd="true"` — 4 commits total (RED test, GREEN feat, per task)
- **Files modified:** 2

## Accomplishments
- `staleness.rs::embedded_commit_is_stale`'s two direct `merge-base --is-ancestor` sites (base-commit lines 51, 72) migrated to `git_command(execution_root)` — the 18c `execution_root` (not `project_root`) distinction preserved exactly, asserted separately by source count so a silent repoint would be caught
- `commands.rs::phase_artifact_on_develop` and both `tag_exists_and_reachable` sites (base-commit lines 91, 2886, 2892) migrated to `git_command(project_root)`
- `commands.rs::test_cmd`'s `sh -c "cargo …"` spawn migrated to `hermetic_command("sh", project_root)` — closes RESEARCH Open Question #1's indirect chain (`sh` → `cargo` → `build.rs::run_git` → `git`) at DevFlow's own spawn edge; `build.rs` itself untouched (D-02, verified via empty `git diff --stat`)
- `Command::new("sh")` count in `commands.rs` drops from 8 to 7 — the seven test-module fixtures below the `#[cfg(test)]` boundary are deliberately untouched
- Two new regression tests, both proving their function's staleness/tag verdict survives a *genuinely* hostile, ambient `GIT_DIR` (not merely a structural assertion) via a new spawned-child-test-process injection technique
- `staleness::` module: 43 passed, 0 failed under a hostile `GIT_DIR` (was 41 passed / 1 collateral failure mid-plan, from `commands.rs`'s not-yet-migrated `tag_exists_and_reachable` sharing the `staleness::` substring filter via its `planning_doc_staleness` submodule name)
- `commands::` module (skipping `pipeline_gate`/`pipeline_outcomes` per the documented hang): 102 passed, 0 failed under a hostile `GIT_DIR`
- `cargo test --workspace` (normal environment): 233 devflow-cli + 408 devflow-core + 2 + 2 integration tests, 0 failed everywhere
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`: both clean after every commit

## RESEARCH Open Question #1 — restated decision and rationale

`27-RESEARCH.md` § Open Questions #1 and § Pitfall 1 flagged `commands.rs::test_cmd`'s `sh -c "cargo test"`/`cargo clippy …`/`cargo fmt --check` spawns (base-commit line 1956) as a genuine indirect git-invoking path not among the 41 counted `Command::new("git")` sites: the child `cargo` process compiles `devflow-cli`, which runs `build.rs::run_git` at compile time to embed `DEVFLOW_BUILD_COMMIT`, so a hostile `GIT_DIR` inherited by `devflow test` reaches `git` two processes down.

**Decision (already recorded in `27-04-PLAN.md`, executed here as written): IN SCOPE.** Scrubbed via `hermetic_command("sh", project_root)`. This plan did not revisit or second-guess that recorded decision — it executed it. Restated rationale, for a reader who reaches this SUMMARY without the plan:
1. D-02's boundary is preserved exactly, not stretched — nothing edits `build.rs`; `git diff --stat -- crates/devflow-cli/build.rs` is empty (verified after every commit in this plan).
2. D-01's "no legitimate reason a DevFlow-issued command should silently redirect" framing covers `devflow test` directly.
3. `27-CONTEXT.md`'s Claude's Discretion #2 named this exact case verbatim ahead of time.
4. None of the three legitimate reasons to defer applied (not a context-cost problem, not missing information, not a dependency conflict).
5. Nothing legitimate breaks — the scrub makes the child behave as if launched from a clean shell.

## Task Commits

Both tasks `tdd="true"`, each executed as a RED-then-GREEN pair:

1. **RED: staleness.rs test** — `48fa2de` (test) — new `embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`; fails against the unmigrated function (see RED output below).
2. **GREEN: staleness.rs migration** — `d8b35b9` (feat) — both `embedded_commit_is_stale` sites migrated to `git_command(execution_root)`; test passes.
3. **RED: commands.rs test** — `1542de6` (test) — new `tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir`; fails against the unmigrated function (see RED output below).
4. **GREEN: commands.rs migration** — `cc0985a` (feat) — all three direct git sites plus the `test_cmd` `sh` spawn migrated; test passes.

**Plan metadata:** (this commit)

## RED output (verbatim, before implementation)

### staleness.rs — `cargo test -p devflow --bin devflow -- staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`

```
running 1 test
test staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir ... FAILED

---- staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir stdout ----

thread '...' panicked at crates/devflow-cli/src/staleness.rs:1033:13:
assertion `left == right` failed: a hostile GIT_DIR pointed at an unrelated repository must not change embedded_commit_is_stale's verdict for execution_root
  left: Indeterminate
 right: Stale

failures:
    staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 231 filtered out; finished in 0.01s

test staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir ... FAILED
thread '...' panicked at crates/devflow-cli/src/staleness.rs:1073:9:
child test process (hostile GIT_DIR pointed at an unrelated foreign repository) must still report embedded_commit_is_stale == Stale for execution_root's own history; child exit status ExitStatus(unix_wait_status(25856))

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 231 filtered out; finished in 0.83s
```

Failed for the intended reason: the unmigrated `merge-base --is-ancestor` call inherited the hostile `GIT_DIR` from its spawning child process (set only on that child, never on the outer test process), resolved `base` against the foreign repo instead of `execution_root`, got an unresolvable ref, and fell through to `Staleness::Indeterminate` instead of the correct `Stale`.

### commands.rs — `cargo test -p devflow --bin devflow -- commands::tests::planning_doc_staleness::tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir`

```
running 1 test
test commands::tests::planning_doc_staleness::tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir ... FAILED

---- ... stdout ----
thread '...' panicked at crates/devflow-cli/src/commands.rs:5715:17:
a hostile GIT_DIR pointed at a foreign repository that DOES carry this tag must not cause tag_exists_and_reachable to report it as belonging to project_root

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 232 filtered out; finished in 0.01s

test commands::tests::planning_doc_staleness::tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir ... FAILED
thread '...' panicked at crates/devflow-cli/src/commands.rs:5765:13:
child test process (hostile GIT_DIR pointed at a foreign repo that DOES carry v1.7.0) must still report tag_exists_and_reachable == false for the real repository; child exit status ExitStatus(unix_wait_status(25856))

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 232 filtered out; finished in 0.27s
```

Failed for the intended reason — the exact security-relevant false-positive direction T-27-01 names: the unmigrated `rev-parse --verify`/`merge-base --is-ancestor` calls inherited the hostile `GIT_DIR` (pointed at a foreign repo crafted to genuinely carry a `v1.7.0` tag reachable from `main`), and reported that foreign tag as existing and reachable in the real (tag-less) repository.

## GREEN output (verbatim, after implementation)

```
$ cargo test -p devflow --bin devflow -- staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir
test staleness::tests::embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 231 filtered out; finished in 0.43s

$ cargo test -p devflow --bin devflow -- commands::tests::planning_doc_staleness::tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir
test commands::tests::planning_doc_staleness::tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 232 filtered out; finished in 0.10s

$ HOSTILE=$(mktemp -d) && git init -q "$HOSTILE" && GIT_DIR="$HOSTILE/.git" cargo test -p devflow --bin devflow -- staleness::
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 190 filtered out; finished in 2.59s

$ HOSTILE=$(mktemp -d) && git init -q "$HOSTILE" && GIT_DIR="$HOSTILE/.git" cargo test -p devflow --bin devflow -- commands:: --skip pipeline_gate --skip pipeline_outcomes
test result: ok. 102 passed; 0 failed; 0 ignored; 0 measured; 131 filtered out; finished in 10.31s

$ cargo test --workspace
devflow-cli bin:  233 passed; 0 failed
devflow-core lib: 408 passed; 0 failed
devflow_dir_gitignore.rs: 2 passed; 0 failed
monitor_e2e.rs: 2 passed; 0 failed

$ cargo clippy --workspace --all-targets -- -D warnings
   Finished — 0 warnings

$ cargo fmt --check
   (no output — clean)

$ git diff --stat HEAD -- crates/devflow-cli/build.rs
   (no output — untouched, D-02)
```

## Files Created/Modified
- `crates/devflow-cli/src/staleness.rs` — migrated `embedded_commit_is_stale`'s two direct sites; added 1 new test
- `crates/devflow-cli/src/commands.rs` — migrated `phase_artifact_on_develop`, both `tag_exists_and_reachable` sites, and `test_cmd`'s `sh` spawn; added 1 new import (`git_command`, `hermetic_command`); added 1 new test

## Decisions Made
- Grep-check final verification: `git_command(execution_root)` == 2, `git_command(project_root)` == 1 (staleness.rs); `git_command(project_root)` == 3, `hermetic_command("sh", project_root)` == 1, `Command::new("sh")` == 7 (commands.rs) — all matching the plan's stated targets.
- See `key-decisions` in frontmatter for the two deviations from the plan's literal test-injection description and the grep-collision fix, both documented in full detail there.

## Deviations from Plan

### Auto-fixed / Adjusted Issues

**1. [Rule 1 — plan defect, adjusted] Neither new test uses the plan's literal ".env(\"GIT_DIR\", foreign) chained directly onto the real function" shape**
- **Found during:** Task 1 and Task 2, writing each `<behavior>`-specified test
- **Issue:** Both `embedded_commit_is_stale` and `tag_exists_and_reachable` are private functions that expose no injection point of their own — a caller cannot chain `.env()` onto a Command those functions build internally. Worse, both functions' git subcommands (`merge-base --is-ancestor`, `rev-parse --verify`) are the SAME ref-resolving class `27-01-SUMMARY.md` Deviation 1 already found is genuinely redirected when `.env("GIT_DIR", foreign)` is chained directly onto a `git_command()`-built Command (unlike `rev-parse --show-toplevel`, which falls back to cwd when `GIT_WORK_TREE` is unset) — so even a manual reproduction of that exact shape against a one-off Command would only demonstrate git's own behavior, not this migration's effect on the real function.
- **Fix:** Built a spawned-child-test-process technique: the outer half of each test builds real + foreign repository fixtures, then re-invokes its own compiled test binary (`std::env::current_exe()`) as a child process filtered to just that one test by name, with the hostile `GIT_DIR` set via `Command::env()` scoped to that CHILD ONLY. The inner half (detected via a sentinel env var also set only on that child) calls the real, unmigrated-or-migrated production function directly and asserts the correct verdict; its own pass/fail becomes the child process's exit status, which the outer half asserts on. `std::env::set_var` is never called on either process — the hostile variable exists only in a freshly spawned child's own environment, satisfying Phase 25 D-14 and the plan's own instruction ("Do NOT mutate the process environment with `std::env::set_var` … reuse 27-01's child-scoped injection technique verbatim") at the level of its actual constraint (never mutate a process you're inside of; only ever set env on a Command you spawn) rather than its most literal reading (which is unsatisfiable for this command class, exactly as 27-01 already found).
- **Files modified:** `crates/devflow-cli/src/staleness.rs`, `crates/devflow-cli/src/commands.rs`
- **Verification:** Both tests demonstrably RED before their respective migration (verbatim output above, failing for the intended reason — the false-negative `Indeterminate` for staleness, the false-positive tag-exists for commands) and GREEN after (verbatim output above).
- **Committed in:** `48fa2de`/`d8b35b9` (staleness.rs), `1542de6`/`cc0985a` (commands.rs)

**2. [Rule 1 — self-caught] Doc-comment text collided with the plan's own acceptance grep**
- **Found during:** Running Task 1's literal `<acceptance_criteria>` grep after writing the test but before migrating the source
- **Issue:** The new test's section-header comment read "...now scrubbed via `git_command(execution_root)`" — this is a comment, not code, but the plan's `git_command\(execution_root\)` count-check grep (`rg -o`) has no comment filter (unlike the separate `Command::new\("git"\)` check, which does), so the comment's own literal text inflated the count to 3 instead of the required 2.
- **Fix:** Reworded to "...now scrubbed via the `git_command` constructor, called with `execution_root`" — same meaning, no longer a literal grep match. Re-ran the check: count == 2, exactly as required.
- **Files modified:** `crates/devflow-cli/src/staleness.rs`
- **Verification:** `rg -o 'git_command\(execution_root\)' staleness.rs | wc -l` == 2.
- **Committed in:** `d8b35b9` (caught before this commit, not a separate fix commit)

---

**Total deviations:** 2 (1 test-technique adjustment with full technical justification, 1 self-caught wording fix before commit)
**Impact on plan:** Neither affects the substantive D-01/D-02/D-03 guarantees — both are documented precisely for the same reason 27-01 documented its own analogous deviation: the plan's literal test description assumed an injection shape that is technically unsatisfiable for this command class, and the actual guarantee is proven more strongly (a genuinely spawned hostile child process, not just a structural assertion) than the literal instruction would have produced even if it had been achievable.

## Issues Encountered
None beyond the two deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `staleness.rs` and `commands.rs` are both fully scrubbed — zero unscrubbed production `Command::new("git")` sites remain in either file (comment-filtered grep confirms), and `commands.rs`'s one indirect `sh -> cargo -> build.rs` chain is closed.
- The spawned-child-test-process hostile-`GIT_DIR` injection technique introduced here is available as precedent for any later plan (27-05, 27-06) whose target functions are private and whose git subcommands fall in the ref-resolving vulnerable class (anything resolving refs/objects, as opposed to `rev-parse --show-toplevel`-style path-only commands).
- No blockers. `crates/devflow-cli/build.rs` remains untouched (D-02, verified via empty `git diff --stat` after every commit in this plan).
- Per this plan's `<scope_boundary>`, no unscrubbed sites were observed in sibling-owned files (`git.rs`, `worktree.rs`, `version.rs`, `agent_result.rs`, `preflight.rs`) during this plan's work — nothing to flag.

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/src/staleness.rs`
- FOUND: `crates/devflow-cli/src/commands.rs`
- FOUND: `.planning/phases/27-scrub-redirecting-git-environment-from-production-calls/27-04-SUMMARY.md`
- FOUND commit: `48fa2de` (test: staleness.rs RED)
- FOUND commit: `d8b35b9` (feat: staleness.rs GREEN)
- FOUND commit: `1542de6` (test: commands.rs RED)
- FOUND commit: `cc0985a` (feat: commands.rs GREEN)

---
*Phase: 27-scrub-redirecting-git-environment-from-production-calls*
*Completed: 2026-07-30*
</content>
