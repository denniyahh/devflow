---
phase: 27-scrub-redirecting-git-environment-from-production-calls
verified: 2026-07-30T21:30:00Z
status: passed
score: 7/7 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification: false
---

# Phase 27: Scrub Redirecting Git Environment From Production Calls Verification Report

**Phase Goal:** Route every production git invocation through a single scrubbing constructor — mirroring what `test_support::git_command` already does for tests — so that `GIT_DIR`, `GIT_WORK_TREE` and the other repository-local variables cannot silently redirect DevFlow onto a repository the operator never named.

**Verified:** 2026-07-30T21:30:00Z
**Status:** passed
**Re-verification:** No — initial verification

**Backlog traceability:** No REQ-IDs exist for this phase (consistent with Phases 21/22/26); tracked by `999.39` / Linear DEN-66. Confirmed empty via `grep -n "Phase 27\|27-scrub" .planning/REQUIREMENTS.md` (no output). Not flagged as a gap, per this phase's own convention.

## Goal Achievement

### Observable Truths

All truths below were independently re-measured against current HEAD (`4d236a5`, which includes commit `936b371` — the review-fix commit that post-dates the numbers recorded in `27-06-SUMMARY.md`). I did not rely on the recorded 411/0 and 188/0 figures as evidence for current-HEAD correctness; I re-ran both commands live in this session.

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Every production `Command::new("git")` site routes through the scrubbing constructor (Sweep A regression) | ✓ VERIFIED | Ran verbatim: `rg --no-heading -n 'Command::new("git")' crates/devflow-core/src crates/devflow-cli/src \| rg -v ':\s*(//\|///\|//!)' \| wc -l` → **0**, at current HEAD. |
| 2 | `hermetic_command`/`git_command` (`crates/devflow-core/src/git.rs:72-94`) actually strip every repository-local variable, and no production call site re-adds one via a trailing `.env(...)` | ✓ VERIFIED | Read `git.rs:72-94`: `env_remove` loop over `REPO_LOCAL_GIT_VARS` (15 entries) + `ALSO_REDIRECTING_GIT_VARS` (3 entries, `GIT_CEILING_DIRECTORIES` added per WR-02). Searched every `.env(...)`/`.envs(...)` call site workspace-wide: all `.env("GIT_DIR", ...)` occurrences are inside test modules (past each file's documented `#[cfg(test)]` boundary line) that deliberately inject a hostile `GIT_DIR` onto a **spawned child test process**, not production code. The only production-code `.env(...)` calls found are `git.rs`'s `LC_ALL`/`LANG` locale pins (added on top of `git_command(&self.root)`, not redirecting vars) and `monitor.rs`'s `.envs(envs...)` (adapter-scoped config applied deliberately *after* the constructor's scrub, by documented design — WR-03). |
| 3 | The two hostile-`GIT_DIR` acceptance commands still pass at current HEAD (post-`936b371`) | ✓ VERIFIED | Re-ran live with a fresh `mktemp -d && git init -q` throwaway repo as `GIT_DIR`: `cargo test -p devflow-core --features test-support` → `test result: ok. 411 passed; 0 failed` (lib target). `cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` → `test result: ok. 188 passed; 0 failed; ... 47 filtered out`. Matches the recorded 27-06 numbers, now re-confirmed to hold post-review-fix. |
| 4 | `crates/devflow-cli/build.rs` is byte-identical across the phase diff (D-02) | ✓ VERIFIED | `git diff --stat $(git merge-base HEAD develop)..HEAD -- crates/devflow-cli/build.rs` → empty, at current HEAD. `git merge-base HEAD develop` = `6350798...`, matching the commit `27-RESEARCH.md` cites as the phase base. |
| 5 | WR-01's rewritten "hostile GIT_DIR" regression tests are genuine (not vacuous) — they actually fail if the scrub is reverted | ✓ VERIFIED | Ran `version::tests::tag_reads_resolve_caller_root_under_a_hostile_git_dir` and `agent_result::tests::branch_evidence_resolves_caller_root_under_a_hostile_git_dir` individually: both pass. **Falsification performed:** temporarily reverted `count_git_tags` in `version.rs` to a bare `Command::new("git").current_dir(...)` (bypassing the scrub) — the test then **failed** (`FAILED. 0 passed; 1 failed`). Restored the file (`git diff --stat` empty afterward) and re-ran: passes again. This proves the test is a real regression guard, not a false green. |
| 6 | RESEARCH Assumption A2 (exhaustiveness beyond the 41 literal sites) is honestly documented as OPEN, not silently closed, and the remaining unmitigated sites are genuinely still unmitigated | ✓ VERIFIED | Confirmed by direct inspection: `hooks.rs:222`, `gates.rs:323`, `verify.rs:106` are still bare `Command::new("sh")`; `commands.rs::cmd_check` (line 1999) is still a bare `Command::new(cmd)` for `cmd="git"` at the `doctor` call site. `monitor.rs:148`'s agent spawn is confirmed migrated to `hermetic_command("sh", ...)` with `.envs(...)` applied after construction (WR-03 closed). This matches `27-SPAWN-CENSUS.md`'s post-correction state exactly — 4 open, 1 closed. |
| 7 | No unresolved debt markers (`TBD`/`FIXME`/`XXX`) were introduced in phase-modified files | ✓ VERIFIED | `grep -n -E "TBD\|FIXME\|XXX"` across all 8 phase-touched files: zero hits. |

**Score:** 7/7 truths verified (0 present-but-behavior-unverified).

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/git.rs` | Scrubbing constructors `hermetic_command`/`git_command`, `REPO_LOCAL_GIT_VARS`, `ALSO_REDIRECTING_GIT_VARS` | ✓ VERIFIED | Present, substantive (real `env_remove` loop, documented rationale for `GIT_EXEC_PATH`/`GIT_CEILING_DIRECTORIES` exclusions/inclusions), wired (54 call sites workspace-wide via `git_command(`/`hermetic_command(`). |
| `crates/devflow-core/src/test_support.rs` | Mirrors/unifies with production constructor | ✓ VERIFIED | Now `pub use crate::git::{ALSO_REDIRECTING_GIT_VARS, REPO_LOCAL_GIT_VARS, git_command, hermetic_command}` — unified rather than merely mirrored; canonical home moved to `git.rs` so the always-compiled path owns it, test fixtures re-export unchanged. |
| `crates/devflow-core/src/version.rs` (10 sites) | All git invocations via `git_command` | ✓ VERIFIED | 10 `git_command(` calls found above the `#[cfg(test)]` boundary (line 1097); 0 bare `Command::new("git")`. |
| `crates/devflow-core/src/agent_result.rs` (3 sites) | All git invocations via `git_command` | ✓ VERIFIED | 3 `git_command(` calls found above the `#[cfg(test)]` boundary (line 1113). |
| `crates/devflow-core/src/worktree.rs` | Both sites via `git_command` | ✓ VERIFIED | Confirmed via passing `worktree::` test suite (9/0), including `list_resolves_caller_root_under_a_hostile_git_dir`. |
| `crates/devflow-core/src/monitor.rs` | Agent spawn via `hermetic_command` (WR-03 fix) | ✓ VERIFIED | `hermetic_command("sh", workdir_path)` at line 162, `.envs(...)` applied after construction per documented ordering rationale. |
| `crates/devflow-cli/src/staleness.rs`, `commands.rs`, `preflight.rs` | All git sites via `git_command`/`hermetic_command` | ✓ VERIFIED | 0 bare `Command::new("git")` in any of the three files; `git_command`/`hermetic_command` call counts present and consistent with each plan's claimed site counts. |
| `.planning/phases/.../27-SPAWN-CENSUS.md` | Full workspace spawn-edge census, A2 verdict | ✓ VERIFIED | Present, substantive; verdict independently spot-checked against source (see Truth 6) and found accurate as of current HEAD, including the post-936b371 correction note. |
| `.planning/phases/.../27-VALIDATION.md` | Filled Per-Task Verification Map, sign-off | ✓ VERIFIED | Present, no `TBD` remaining, `nyquist_compliant: true`. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `GitFlow`'s internal wrappers (`git`, `git_output`, `git_raw`, `git_raw_combined`) | `git_command` | direct call | ✓ WIRED | Confirmed at `git.rs:461`, `:488` and surrounding wrapper functions — all route through `git_command(&self.root)`. |
| `monitor::spawn_monitor_inner` | `hermetic_command` | direct call, `.envs()` after construction | ✓ WIRED | Line 162-174; ordering verified deliberate and load-bearing per in-code comment and WR-03 closure. |
| `version::count_git_tags`/`highest_semver_tag` | `git_command` | direct call | ✓ WIRED | Confirmed at lines 120, 181; falsification test (Truth 5) proves the wiring is load-bearing, not decorative. |
| `test_support.rs` (all crates' test fixtures) | `crate::git::{git_command, hermetic_command}` | `pub use` re-export | ✓ WIRED | Confirmed at `test_support.rs:138-140` — single source of truth, cannot drift. |

### Anti-Patterns Found

None blocking. No `TBD`/`FIXME`/`XXX` in any of the 8 phase-touched files. No stub returns, no empty handlers, no hardcoded-empty data flowing to production paths.

### Behavioral Spot-Checks / Live Re-Measurement

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Sweep A regression | `rg ... 'Command::new("git")' ... \| rg -v comment \| wc -l` | `0` | ✓ PASS |
| devflow-core hostile-`GIT_DIR` acceptance | `GIT_DIR=<throwaway>/.git cargo test -p devflow-core --features test-support` | `411 passed; 0 failed` | ✓ PASS |
| devflow-cli hostile-`GIT_DIR` acceptance | `GIT_DIR=<throwaway>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` | `188 passed; 0 failed; 47 filtered out` | ✓ PASS |
| WR-01 single named test (core) | `cargo test -p devflow-core --features test-support --lib version::tests::tag_reads_resolve_caller_root_under_a_hostile_git_dir` | `1 passed` | ✓ PASS |
| WR-01 single named test (agent_result) | `cargo test -p devflow-core --features test-support --lib agent_result::tests::branch_evidence_resolves_caller_root_under_a_hostile_git_dir` | `1 passed` | ✓ PASS |
| WR-02 drift test | `cargo test -p devflow-core --features test-support --lib git::tests::local_env_vars_match_git` | `1 passed` | ✓ PASS |
| `git::` module suite | `cargo test -p devflow-core --features test-support --lib git::` | `38 passed; 0 failed` | ✓ PASS |
| `worktree::` module suite | `cargo test -p devflow-core --features test-support --lib worktree::` | `9 passed; 0 failed` | ✓ PASS |
| Falsification: revert `count_git_tags` to bare `Command::new("git")` | (temporary source edit, restored immediately) | Named test **FAILED** as expected, then passed again after restore | ✓ PASS (proves test is non-vacuous) |
| D-02 boundary | `git diff --stat $(git merge-base HEAD develop)..HEAD -- crates/devflow-cli/build.rs` | empty | ✓ PASS |

Full workspace `cargo test --workspace` in a normal (non-hostile) environment was **not** re-run in this verification session — already established by the orchestrator (21 suites, 0 failed) and re-running it provides no new evidence per the "run the full suite at most once" constraint.

### Requirements Coverage

No REQ-IDs map to this phase (confirmed empty grep against `REQUIREMENTS.md`). Tracked instead by backlog `999.39`/DEN-66. Both of that backlog entry's acceptance signals (Sweep A = 0; hostile-`GIT_DIR` tests green) are independently re-measured above and hold at current HEAD.

### Documented Carry-Forward (NOT phase gaps, per explicit task instruction)

These items are known-open, disclosed in `27-SPAWN-CENSUS.md`/`27-REVIEW.md`, and explicitly out of this phase's scope. Re-confirmed still accurately described (not silently worse or better than documented) rather than treated as gaps:

| Item | Status | Evidence |
|------|--------|----------|
| `hooks.rs:222` (`sh -c "cargo doc"`) | Still unmitigated, as documented | `grep -n "Command::new" hooks.rs` → bare `Command::new("sh")` at line 222 |
| `gates.rs:323` (`run_notify_command`, operator-supplied `sh -c`) | Still unmitigated, as documented | Bare `Command::new("sh")` at line 323 |
| `verify.rs:106` (`run_external_verification`) | Still unmitigated, as documented | Bare `Command::new("sh")` at line 106 |
| `commands.rs::cmd_check` (`devflow doctor`'s `git --version`) | Still unmitigated, as documented | Bare `Command::new(cmd)` at line 1999, `cmd="git"` at `doctor` call site |
| `build.rs` compile-time git calls | Accepted, documented exclusion (D-02) | Confirmed byte-identical diff (Truth 4) |
| IN-02 (`preflight.rs` fast-forward count skew) | Open, cosmetic, non-blocking | Not independently re-verified this session — low-risk, disposition unchanged from `27-REVIEW.md` |

### Human Verification Required

None. All must-haves were independently re-measurable via grep, source inspection, and live `cargo test` runs, including one falsification test to rule out a vacuous regression guard.

### Gaps Summary

No gaps found. All four of the task's explicitly-named acceptance signals (Sweep A = 0, scrub constructor correctness + no re-add, both hostile-`GIT_DIR` commands green at current HEAD post-`936b371`, `build.rs` byte-identical) were independently re-measured live in this verification session, not taken from SUMMARY.md claims. The WR-01 fix was additionally falsification-tested (reverted the scrub at one call site, confirmed the named test fails, then restored). The A2 exhaustiveness gap (4 remaining indirect spawn edges) and IN-01/IN-02 are honestly disclosed carry-forward items per the phase's own documentation and this verification's task instructions — not gaps against this phase's stated goal.

---

_Verified: 2026-07-30T21:30:00Z_
_Verifier: Claude (gsd-verifier)_
