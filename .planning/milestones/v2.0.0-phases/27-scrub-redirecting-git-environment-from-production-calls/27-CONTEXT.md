# Phase 27: Scrub Redirecting Git Environment From Production Calls - Context

**Gathered:** 2026-07-30
**Status:** Ready for planning

<domain>
## Phase Boundary

Route every production git invocation through a single scrubbing constructor — mirroring what `devflow_core::test_support::git_command` already does for tests — so that `GIT_DIR`, `GIT_WORK_TREE`, and the other repository-local/redirecting environment variables cannot silently retarget DevFlow onto a repository the operator never named. Confirmed by grep on 2026-07-30 at `develop@d1c030e`: **41 production `Command::new("git")` call sites** across 7 files (`git.rs`, `version.rs`, `worktree.rs`, `agent_result.rs`, `commands.rs`, `staleness.rs`, `preflight.rs`), all sharing the same builder shape (`Command::new("git").args([...]).current_dir(...).output()`), and all pinning `current_dir()` but never scrubbing environment. This refines the ROADMAP entry's "~86 call sites" estimate — the actual, grep-confirmed count is 41.

**Why now, not later.** Phase 26's re-review (`26-REVIEW.md` CR-01) found that `mutating_project_root` — the guard written to stop the release executor acting on an unnamed repository — is bypassed by an inherited `GIT_DIR`: `git rev-parse --show-toplevel` reports the cwd's work tree while HEAD/refs/objects come from `GIT_DIR`, so the guard compares two paths, matches, and passes while the executor pushes/tags/publishes against a different repository. This makes the scrub prerequisite #1 for 999.25's re-attempt (DEN-50) and blocks 999.52 (`sync`) from shipping independently.

**Out of scope, decided this session:**
- Any escape hatch for honoring an operator-set `GIT_DIR` (D-01 — no such thing is being built).
- `crates/devflow-cli/build.rs`'s compile-time `run_git` helper (D-02 — different threat model, see below).
- Building any part of `mutating_project_root` / `project_root_guard.rs` ahead of 999.25 (D-03 — neither exists on `develop` today; both remain unmerged on `feature/phase-26`).

</domain>

<decisions>
## Implementation Decisions

### GIT_DIR / redirecting-variable policy

- **D-01:** The scrub is **unconditional, with no escape hatch.** Every production git call gets `GIT_DIR`, `GIT_WORK_TREE`, and the rest of `test_support::REPO_LOCAL_GIT_VARS` + `ALSO_REDIRECTING_GIT_VARS` stripped, always. No flag, env var, or config setting can turn this back on. There is no legitimate operator use case for a DevFlow-issued git command silently redirecting via an inherited variable — an operator who wants DevFlow to act on a different repository passes it a different path.
  — **Reversibility:** reversible — this is a policy choice inside a single constructor; loosening it later means adding a parameter, not undoing anything structural.

### Scope boundary — build.rs excluded

- **D-02:** `crates/devflow-cli/build.rs`'s `run_git` (lines 76-85) is **explicitly out of scope** for this phase. It shells out to git once, at *compile* time, to embed `DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY` via `rustc-env` (consumed later by the runtime staleness check). This is a different actor (whoever is compiling the binary — a developer or CI) and a different moment (build time, not an operator invoking a shipped `devflow` command via a git hook) than CR-01's scenario. Worst case if `GIT_DIR` were set during a build: a wrong staleness signal, not an irreversible action against the wrong repository. Considered and declined, not overlooked — see `<deferred>`.
  — **Reversibility:** reversible — nothing built, easy to fold in later with its own justification if evidence of an actual incident ever surfaces.

### Acceptance target — the mechanism, not a not-yet-existing guard

- **D-03:** The regression test(s) this phase adds must prove **the scrubbing constructor itself** holds under a hostile `GIT_DIR` — e.g., with `GIT_DIR` pointed at an unrelated repository, the git-calling functions in `git.rs`/`version.rs` still resolve and act on the caller-supplied `project_root`. **Do not** build a stand-in version of `mutating_project_root` or `project_root_guard.rs` to have something to test against CR-01 directly — confirmed 2026-07-30 that **neither exists on `develop`**; both are still unmerged on `feature/phase-26`, tied to 999.25. Building a placeholder guard now would mean building part of the executor ahead of its own phase, the same coupling problem that kept the resume-ledger (D-06a) off the 999.5 split.

  **The existing 37 dirty-environment unit test failures are the ready-made secondary signal.** 999.37 deliberately left these red under a `GIT_DIR`-polluted test run rather than papering over them (fixture containment was fixed; production was left honest so the failures would name a real exposure). Once production call sites are scrubbed, re-running the suite under a dirty `GIT_DIR` environment should flip these from red to green — a natural, already-instrumented acceptance check requiring no new fixture design.

  When 999.25 eventually rebuilds the executor (including `mutating_project_root`) on top of `develop`, it inherits an already-scrubbed codebase — the guard is correct by construction rather than needing a separate fix.
  — **Reversibility:** reversible.

### Claude's Discretion

Not constrained here; the researcher and planner decide:

- **Where the scrubbing constructor lives** — `devflow-core` (alongside or adjacent to where `test_support::git_command` already lives, minus its `#[cfg(test)]`/feature gating) is the natural home since both `devflow-core` and `devflow-cli` have call sites, but the exact module and function name are not fixed here.
- **Whether it's one function or a small family** (e.g., a `Command`-returning builder plus a `hermetic_command`-style variant for non-git programs that shell out to git, mirroring `test_support`'s two-function split) — match whatever the researcher finds cleanest given the 41 call sites' actual shapes.
- **Mechanical migration order/waves across the 7 files** — plan around the file-overlap serialization tax already flagged in the ROADMAP entry (this phase's cluster and the unmerged 999.25 cluster share `git.rs`), but the exact sequencing is the planner's call.
- **Whether `test_support::git_command`/`hermetic_command` themselves change** (e.g., delegate to the new production constructor to avoid two copies of the same variable list) or stay independent. Either is acceptable; avoiding drift between the two lists is the only hard requirement.
- **Exact list of variables scrubbed** — start from `test_support::REPO_LOCAL_GIT_VARS` + `ALSO_REDIRECTING_GIT_VARS` (15 + 2 entries, git 2.55) as the verified baseline; the researcher should re-verify against `git rev-parse --local-env-vars` on the implementation machine per the existing `local_env_vars_match_git` test pattern, rather than assume the list is still current.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Backlog source and escalation rationale
- `.planning/ROADMAP.md` § "Phase 999.39" (now `PROMOTED — Phase 27`) — the full original evidence, the 37-failing-test signal, and the CR-01 escalation rationale in detail. This CONTEXT.md does not restate it.
- `.planning/ROADMAP.md` § "Phase 27" — the phase entry itself, written when promoting from backlog; goal, depends-on, scope caution.
- `.planning/superseded/26-release-cut-automation/26-REVIEW.md` § "CR-01" — the exact bypass mechanism (`--show-toplevel` vs. `GIT_DIR`-sourced HEAD/refs/objects), reproduced end-to-end against `mutating_project_root` (`main.rs:718-779` on `feature/phase-26`, **not present on `develop`**).

### Code this phase changes
- `crates/devflow-core/src/git.rs` — 9 call sites (lines 100, 175, 387, 415, 438, 450, 488, 497, 708).
- `crates/devflow-core/src/version.rs` — 10 call sites (lines 120, 147, 160, 184, 216, 240, 305, 338, 399, 563).
- `crates/devflow-core/src/agent_result.rs` — 3 call sites (lines 574, 583, 664).
- `crates/devflow-core/src/worktree.rs` — 2 call sites (lines 121, 175).
- `crates/devflow-cli/src/commands.rs` — 3 call sites (lines 91, 2886, 2892).
- `crates/devflow-cli/src/staleness.rs` — 3 call sites (lines 51, 72, 124) — note `staleness.rs:123` already has a `run_git_stdout` wrapper; check whether its callers can route through the new constructor without touching every internal call.
- `crates/devflow-cli/src/preflight.rs` — 11 call sites (lines 146, 160, 183, 332, 345, 356, 371, 426, 457, 530, 778) — the largest single file.

### Code this phase mirrors, does not change
- `crates/devflow-core/src/test_support.rs` (lines ~75-175) — `git_command`, `hermetic_command`, `REPO_LOCAL_GIT_VARS` (15 entries), `ALSO_REDIRECTING_GIT_VARS` (2 entries), and the `local_env_vars_match_git` test that keeps the list honest against the installed git. This is the pattern to reproduce for production, per D-01/discretion above.

### Explicitly out of scope
- `crates/devflow-cli/build.rs` (lines 33-85, `run_git`) — D-02.
- `crates/devflow-core/src/main.rs`'s `mutating_project_root` and `crates/devflow-cli/tests/project_root_guard.rs` — do not exist on `develop`; both remain on `feature/phase-26`, in 999.25's territory. D-03.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`test_support::git_command(repo: &Path) -> Command`** and **`test_support::hermetic_command(program: &str, dir: &Path) -> Command`** — the exact pattern to mirror. `hermetic_command` is the more general form (any program that shells out to git, e.g. `cargo`, not just `git` itself) — worth considering for the production side too, since a scrub that only wraps `Command::new("git")` calls would miss a production call that shells out to `cargo` in a way that itself invokes git.
- **`REPO_LOCAL_GIT_VARS`** (15 entries) + **`ALSO_REDIRECTING_GIT_VARS`** (2 entries) — the verified variable list, already kept honest by a test asserting it matches `git rev-parse --local-env-vars`.

### Established Patterns
- **Uniform builder shape.** All 41 production call sites follow `Command::new("git").args([...]).current_dir(&root).output()...` — no site does anything structurally different (no piped stdin, no long-running process, no shell wrapping). This is what makes the phase mechanical rather than requiring per-site judgment calls.
- **Fail-soft where established** (Phase 24's `check_ssh_signing_viability` precedent) — most existing call sites already degrade gracefully (`.ok()`, `Option`-returning) rather than panicking; the new constructor should not change that posture, only what environment the spawned process sees.
- **Per-`Command` `env_remove`, never process-global `set_var`** — established by 999.37/999.38 (D-14 in Phase 25's context): `std::env::set_var` is `unsafe` in Rust 2024 and unsound in a threaded test/production binary. The scrub must be scoped to each individual `Command`, matching how `test_support` already does it.

### Integration Points
- `crates/devflow-cli/src/staleness.rs:123` (`run_git_stdout`) is itself a small wrapper already — one of the few production indirections over a bare `Command::new("git")`. Migrating it to build on the new constructor (rather than being a fourth parallel implementation) is likely the cleanest integration point in that file.
- The unmerged `feature/phase-26` branch's `mutating_project_root`/`git.rs` executor code will eventually need to build on whatever this phase lands — the researcher/planner should keep the constructor's signature simple and stable (a drop-in replacement for `Command::new("git")` plus `.current_dir()`) so 999.25's later rebase-equivalent work isn't complicated by an awkward API.

</code_context>

<specifics>
## Specific Ideas

- **Operator's framing, worth preserving in commit messages / docs:** "no legitimate reason an operator would want a DevFlow-issued command to silently redirect via an inherited variable" — this is the reasoning behind D-01's no-escape-hatch stance and should carry into any doc-comment explaining why the constructor has no bypass parameter.
- **The 37-failing-test signal (D-03) is the single most useful acceptance artifact this phase has** — it already exists, is already red for the right reason, and needs no new fixture design. The planner should treat "these 37 now pass under a dirty `GIT_DIR` environment" as a first-class success criterion, not an incidental side effect.

</specifics>

<deferred>
## Deferred Ideas

- **`build.rs`'s compile-time git calls** (D-02) — considered and explicitly excluded, not overlooked. Worth its own backlog entry only if a real incident (a build machine with `GIT_DIR` set producing a wrong staleness signal) is ever actually observed; no evidence of one exists today.
- **An operator-facing escape hatch for honoring `GIT_DIR`** (D-01) — considered and explicitly rejected as a standing feature. Not filed anywhere; re-derive from scratch if a genuine use case ever surfaces.
- **`mutating_project_root` / `project_root_guard.rs`** — not built here; both remain 999.25's territory on `feature/phase-26`. This phase deliberately stops at the mechanism layer (D-03).

### Reviewed Todos (not folded)
None — no todo-cross-reference matches were found for this phase.

</deferred>

---

*Phase: 27-scrub-redirecting-git-environment-from-production-calls*
*Context gathered: 2026-07-30*
