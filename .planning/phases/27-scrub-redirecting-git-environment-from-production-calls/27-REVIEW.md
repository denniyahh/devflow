---
phase: 27-scrub-redirecting-git-environment-from-production-calls
reviewed: 2026-07-30T00:00:00Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/test_support.rs
  - crates/devflow-core/src/version.rs
  - crates/devflow-core/src/worktree.rs
findings:
  critical: 0
  warning: 3
  info: 3
  total: 6
status: issues_found
---

# Phase 27: Code Review Report

**Reviewed:** 2026-07-30
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

This phase's core mechanism (`devflow_core::git::{hermetic_command, git_command}` in `git.rs`) is sound and was traced end-to-end: every production `git`/`sh` spawn in the eight reviewed files routes through it, `current_dir` is never dropped, and no production call site re-adds a redirecting variable via a trailing `.env(...)` after the constructor. The `REPO_LOCAL_GIT_VARS` list matches `git rev-parse --local-env-vars` byte-for-byte on this machine's git 2.55.0 (verified directly), `GIT_EXEC_PATH`'s exclusion is correctly reasoned (it only affects helper-binary lookup, never repository resolution), and `GIT_CONFIG_COUNT`'s sufficiency to neutralize `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` is correct per git's own `config.c` behavior. `preflight.rs::fast_forward_base_ref`'s `update-ref <ref> <new> <old>` argument order is correct, and its caller (`ensure_base_ref_current`) re-resolves the CAS `expected_old` immediately before the write rather than reusing a stale earlier read. `version.rs`'s and `agent_result.rs`'s git-derived decisions (baseline tag, bump classification, exit/commit verdict) all pass the correct root through, including the worktree-vs-project_root distinction where it actually matters (`preflight_major_bump_check`'s `execution_root`).

No BLOCKER-tier defects were found in the reviewed files: I could not construct a concrete scenario in which a call site inside these eight files still resolves to the wrong repository under a hostile inherited environment. The findings below are regression-coverage and completeness gaps — real, but not currently exploitable.

## Warnings

### WR-01: Two "hostile GIT_DIR" regression tests inject no hostile environment and pass vacuously under normal `cargo test`

**File:** `crates/devflow-core/src/agent_result.rs:2542` (`branch_evidence_resolves_caller_root_under_a_hostile_git_dir`)
**File:** `crates/devflow-core/src/version.rs:2432` (`tag_reads_resolve_caller_root_under_a_hostile_git_dir`)

**Issue:** Both tests' doc comments claim to prove `evaluate_layer2` / `count_git_tags`+`highest_semver_tag` resolve the caller's own repository "even when the process inherited a hostile `GIT_DIR`." In fact neither test sets `GIT_DIR` anywhere — not via `.env()` chained onto a `Command`, and not via a self-contained spawned child process (the technique later plans in this same phase, e.g. `staleness.rs:1022` `embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir` and `commands.rs:5699` `tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir`, correctly use: re-invoke the same test binary filtered to one test, with `GIT_DIR` set only on that child's `Command::env()`). Both tests' doc comments say the "RED-before/GREEN-after proof this plan's own `<verify>` block relies on" comes from manually running the suite wrapped in `GIT_DIR="$HOSTILE/.git" cargo test ...` — but I confirmed via `rg -rln 'GIT_DIR=' scripts/ .github/` that **no CI script or workflow wraps any test run this way**. That wrapper invocation was a one-time manual acceptance check during this phase's own development, not a standing part of the test suite.

Concrete consequence: if a future refactor in `agent_result.rs::evaluate_layer2`/`evaluate_layer3`, or in `version.rs::count_git_tags`/`highest_semver_tag`, reverted the `git_command(project_root)` call back to a bare `Command::new("git").current_dir(project_root)` (unscrubbed), every test in the standing suite — including these two, and including `git.rs`'s own `git_command_marks_every_redirecting_var_for_removal` (which only tests the constructor itself, not whether these two files actually call it) — would keep passing. `cargo test` on a normal developer machine or in ordinary CI has no ambient `GIT_DIR`, so a test that injects none of its own proves only ordinary-path correctness, which holds identically with or without the scrub.

**Fix:** Rewrite both tests using the self-contained child-process technique already established in this phase (`staleness.rs:1022`, `commands.rs:5699`): spawn `std::env::current_exe()` filtered to the one test name, set `GIT_DIR` only on that child's `Command::env()`, and assert the child's exit status. This makes the hostile-environment claim a standing regression guard instead of a one-off manual verification.

### WR-02: `GIT_CEILING_DIRECTORIES` is omitted from the scrub list without documented rationale

**File:** `crates/devflow-core/src/git.rs:27-48` (`REPO_LOCAL_GIT_VARS`, `ALSO_REDIRECTING_GIT_VARS`)

**Issue:** The phase's own stated threat model explicitly calls out `GIT_CEILING_DIRECTORIES` as a variable to check, but it appears in neither list, and — unlike `GIT_EXEC_PATH`, whose exclusion is explained in `git_command`'s doc comment (`git.rs:54-56`) — there is no comment explaining why it was left out. I verified it is correctly absent from `git rev-parse --local-env-vars` (matches `local_env_vars_match_git`'s own live-diffed test), so its omission from `REPO_LOCAL_GIT_VARS` specifically is not a drift bug. The actual risk is low in this codebase's usage pattern: every call site passes an explicit `current_dir` that is itself a repository root (`project_root`, `execution_root`, or a worktree path), so git's discovery never needs to search upward and `GIT_CEILING_DIRECTORIES` never comes into play — at worst it would cause a fail-loud "not a git repository" error, not a silent redirect to a different repository (unlike `GIT_DIR`/`GIT_WORK_TREE`).

**Fix:** Either add `GIT_CEILING_DIRECTORIES` to `ALSO_REDIRECTING_GIT_VARS` for defense-in-depth completeness, or add one sentence to `hermetic_command`'s doc comment (next to the existing `GIT_EXEC_PATH` justification) explaining explicitly why it's excluded — mirroring the rigor already applied to every other decision in this file.

### WR-03: Five self-disclosed unmitigated `git`-reaching spawn edges remain outside the eight reviewed files, one of them the highest-consequence site in the codebase

**File (out of the assigned 8, cited for completeness):** `crates/devflow-core/src/monitor.rs:148`, `crates/devflow-core/src/hooks.rs:222`, `crates/devflow-core/src/gates.rs:323`, `crates/devflow-core/src/verify.rs:106`, `crates/devflow-cli/src/commands.rs:1998`/`2084` (`cmd_check("git", "git", ...)`, see INFO-01 below)

**Issue:** This phase's own `27-06-SUMMARY.md` transparently records that a broader sweep (beyond the 41 counted literal `Command::new("git")` sites) found 5 genuinely unmitigated spawn edges that still reach `git`, and explicitly leaves "RESEARCH Assumption A2 (the exhaustiveness gap) ... OPEN, not silently closed." I confirmed all five are still present and unscrubbed at the cited lines. The most consequential is `monitor.rs:148`: it is the `sh -c` spawn that launches the AI coding agent itself for a phase — the process that performs the phase's actual git commits/pushes on the operator's behalf. If `devflow start` is invoked from inside a hostile-`GIT_DIR` context (a git hook, `rebase --exec`, `bisect run` — exactly the scenarios named in this phase's own opening threat statement), that `GIT_DIR` rides down through `monitor.rs`'s unscrubbed `sh` spawn into the agent process and every git command the agent itself runs, silently retargeting the phase's actual commits at a foreign repository — the single worst-case outcome this whole phase exists to prevent, on the one call site carrying the most consequence.

This is disclosed, not hidden, and is explicitly out of the file list assigned for this review — I am not scoring it as a phase-27 defect. It is flagged here because "is the mechanism actually complete" is squarely in this review's stated priorities, and a reviewer reading only the 8 assigned files would not otherwise learn that the phase's own audit already found the highest-severity residual gap sitting one call away from every file reviewed here.

**Fix:** Track `monitor.rs:148` (agent spawn) as the top-priority item for a follow-up phase; `hooks.rs:222` reaches the same `sh → cargo → build.rs::run_git` indirect chain this phase already closed once for `commands.rs::test_cmd` (27-04) and should get the identical `hermetic_command("sh", ...)` treatment for consistency.

## Info

### IN-01: `devflow doctor`'s git version check is a literal unscrubbed `git` invocation invisible to a `Command::new("git")` grep

**File:** `crates/devflow-cli/src/commands.rs:1998` (`cmd_check`), invoked at `commands.rs:2084-2089` with `cmd = "git"`

**Issue:** `cmd_check(name, cmd, version_arg, install_hint)` calls `Command::new(cmd).arg(version_arg).output()` with no `current_dir` and no env scrubbing. Because the program name is threaded through a `cmd: &str` parameter rather than spelled `Command::new("git")` literally in source, this is exactly the class of gap this phase's own comment-filtered acceptance grep cannot see (self-identified in `27-06-SUMMARY.md`). It is functionally inert today — `git --version` performs no ref/tree/object resolution, so no hostile `GIT_DIR` can redirect it to a meaningfully different answer — but it is a real gap against `hermetic_command`'s own documented "unconditional... no config lookup" scrub policy (`git.rs:71-75`), and would stop being inert if this helper were ever reused for a `cmd_check` that does resolve repository state.

**Fix:** Route through `devflow_core::git::hermetic_command("git", project_root)` for consistency with the rest of the codebase's policy, even though the current `--version` argument makes it low-priority.

### IN-02: Stale `count` in the fast-forward success message can understate/overstate the actual number of commits advanced

**File:** `crates/devflow-cli/src/preflight.rs:517-541` (`ensure_base_ref_current`'s `Behind { count }` arm), success message at `preflight.rs:536-538`

**Issue:** `count` is captured once, inside `base_ref_currency`, from a `rev-list --count` computed at the START of `ensure_base_ref_current` (before the checked-out-anywhere scan and the CAS re-resolution of `local_sha`/`remote_sha`). If another `git fetch` (from a concurrent process, or the network fetch itself racing something else) advances `refs/remotes/origin/{base}` further between that count and the later `resolve()` calls that supply the actual CAS `new` value, the printed `"advanced `{base}` to `{remote_ref}` ({count} commit(s) fast-forwarded)"` message reports a `count` that no longer matches how far the ref was actually moved. This is cosmetic — the CAS write itself is safe and correctly scoped to whatever `remote_sha` resolves to at write time — but the operator-facing message can be silently wrong.

**Fix:** Recompute the commit count from `local_sha..remote_sha` (the values actually used in the CAS) immediately before printing, instead of reusing the earlier `Behind{count}` value.

### IN-03: `build.rs`'s compile-time `git` calls remain outside this phase's mechanism (documented, accepted risk — restated for completeness)

**File:** `crates/devflow-cli/build.rs:77` (`run_git`)

**Issue:** `run_git` shells out via a bare `Command::new("git")` with no env scrubbing, to embed `DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY` — the exact values `staleness.rs::enforce_build_staleness` later compares against `HEAD` to decide whether to hard-block a stale self-dogfood build. This is a documented, deliberate exclusion (D-02 in `27-CONTEXT.md`/`27-RESEARCH.md`, verified byte-identical across the phase diff in every plan's `<verify>` block) on the stated grounds that build time is a different actor/moment than a shipped `devflow` command's runtime. I re-verify the risk characterization holds: if `cargo build` is ever invoked from a context with a hostile `GIT_DIR` set (e.g., a build triggered from inside a git hook), the binary would embed the wrong `DEVFLOW_BUILD_COMMIT`, and `embedded_commit_is_stale`'s ancestry check could then evaluate a nonsense commit — most outcomes fall through to `Indeterminate`/`Ahead` (warn-only), but a rare case in which the wrong hash happens to be a valid, reachable ancestor could produce a false `Fresh` and silently defeat the D-18 hard-block this phase's other 41 sites exist to protect. Not a new finding — already accepted with rationale — restated here only because it is the one path by which this phase's own headline protection (the self-dogfood staleness block) could still be quietly undermined.

**Fix:** None required for this phase; already an accepted, documented tradeoff. Worth its own backlog entry if `hooks.rs:222`'s `cargo doc` chain (WR-03 above) is ever closed, so the reasoning stays consistent across all `cargo`-triggering spawn edges.

---

_Reviewed: 2026-07-30_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
