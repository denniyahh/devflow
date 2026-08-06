# Phase 27: Scrub Redirecting Git Environment From Production Calls - Research

**Researched:** 2026-07-30
**Domain:** Rust subprocess environment hygiene / git repository-resolution security
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**D-01 — GIT_DIR / redirecting-variable policy.** The scrub is **unconditional, with no escape hatch.** Every production git call gets `GIT_DIR`, `GIT_WORK_TREE`, and the rest of `test_support::REPO_LOCAL_GIT_VARS` + `ALSO_REDIRECTING_GIT_VARS` stripped, always. No flag, env var, or config setting can turn this back on. Reversibility: reversible (a constructor-internal policy choice; loosening later means adding a parameter, not undoing anything structural).

**D-02 — Scope boundary, build.rs excluded.** `crates/devflow-cli/build.rs`'s `run_git` (lines 76-85) is explicitly out of scope. Different actor (whoever compiles the binary), different moment (compile time, not an operator invoking a shipped `devflow` command). Worst case if `GIT_DIR` were set during a build: a wrong staleness signal, not an irreversible action against the wrong repository. Reversibility: reversible.

**D-03 — Acceptance target is the mechanism, not a not-yet-existing guard.** The regression test(s) this phase adds must prove the scrubbing constructor itself holds under a hostile `GIT_DIR` — e.g., with `GIT_DIR` pointed at an unrelated repository, the git-calling functions in `git.rs`/`version.rs` still resolve and act on the caller-supplied `project_root`. Do NOT build a stand-in `mutating_project_root`/`project_root_guard.rs` — neither exists on `develop`; both are unmerged on `feature/phase-26`, tied to 999.25. The existing dirty-`GIT_DIR` unit test failures (999.37 left them red on purpose) are the ready-made secondary signal: once call sites are scrubbed, re-running the suite under a dirty `GIT_DIR` should flip them from red to green. Reversibility: reversible.

### Claude's Discretion

Not constrained by CONTEXT.md; the researcher and planner decide:
- Where the scrubbing constructor lives — `devflow-core` is the natural home (both crates have call sites); exact module/function name not fixed.
- Whether it's one function or a small family (git-only vs. a `hermetic_command`-style variant for non-git programs that shell out to git) — match whatever the 41 call sites' actual shapes justify.
- Mechanical migration order/waves across the 7 files — plan around file-overlap serialization (this phase's cluster and the unmerged 999.25 cluster share `git.rs`); exact sequencing is the planner's call.
- Whether `test_support::git_command`/`hermetic_command` themselves change (delegate to the new production constructor) or stay independent — either acceptable; avoiding drift between the two variable lists is the only hard requirement.
- Exact list of variables scrubbed — start from `REPO_LOCAL_GIT_VARS` + `ALSO_REDIRECTING_GIT_VARS` (15 + 2, git 2.55) as the verified baseline; re-verify against `git rev-parse --local-env-vars` on the implementation machine.

### Deferred Ideas (OUT OF SCOPE)

- Any escape hatch for honoring an operator-set `GIT_DIR` (D-01 — no such thing is being built; not filed anywhere).
- `crates/devflow-cli/build.rs`'s compile-time `run_git` helper (D-02 — worth its own backlog entry only if a real incident is ever observed).
- `mutating_project_root` / `project_root_guard.rs` (D-03 — 999.25's territory on `feature/phase-26`; this phase stops at the mechanism layer).
</user_constraints>

<phase_requirements>
## Phase Requirements

This project tracks work by backlog identifier (999.39 / DEN-66), not REQ-IDs — no REQUIREMENTS.md exists. The trackable decision IDs for this phase are D-01, D-02, D-03 from `27-CONTEXT.md`. Plans must cite these explicitly.

| ID | Description | Research Support |
|----|-------------|------------------|
| D-01 | Unconditional scrub, no escape hatch, applied to every production git call | § Standard Stack (constructor API shape), § Code Examples (regression-test pattern proving no bypass parameter exists) |
| D-02 | `build.rs`'s `run_git` explicitly untouched | § Migration Sequencing (confirms `build.rs` is not among the 7 files, not reachable from any production `devflow` command) |
| D-03 | Regression proves the mechanism itself; the acceptance signal is "N failing tests under hostile `GIT_DIR` flip to green" | § Validation Architecture, § The 37-Failing-Test Acceptance Signal — Re-Measured (exact reproducible commands, corrected count) |
</phase_requirements>

## Summary

The phase is mechanical but not trivially so. All 41 production `Command::new("git")` call sites were independently re-enumerated by grep at HEAD `6350798` and match CONTEXT.md's count and per-file/per-line breakdown **exactly** (git.rs 9, version.rs 10, agent_result.rs 3, worktree.rs 2, commands.rs 3, staleness.rs 3, preflight.rs 11 = 41). `test_support::REPO_LOCAL_GIT_VARS` + `ALSO_REDIRECTING_GIT_VARS` (15 + 2 entries) were re-verified against `git rev-parse --local-env-vars` on this machine (git 2.55.0) and match with **zero drift** — the existing `local_env_vars_match_git` test is not lying.

Two structural facts change the plan shape from "wrap 41 lines identically":

1. **`test_support` is feature-gated (`#[cfg(any(test, feature = "test-support"))]`) and devflow-cli's *production* dependency on devflow-core does not enable `test-support`** (only its `[dev-dependencies]` entry does). This means the new constructor **cannot simply live in `test_support.rs` with the gate removed** without also carrying `test_support`'s unrelated exec-visibility-barrier code into every normal build — it needs a new, always-compiled home. `devflow-core::git` (already unconditionally `pub`, already imports `Command`) is the natural fit; `test_support::git_command`/`hermetic_command` should delegate to it (research question #4 — recommended, not required).

2. **`staleness.rs::run_git_stdout` (line 123) is a genuine leverage point.** It has ~15 internal callers in `staleness.rs` plus 2 cross-file callers in `commands.rs`. Migrating its single `Command::new("git")` call (line 124) to the new constructor scrubs all of them without touching each call site individually — this is the single highest-leverage file in the migration.

The "37 failing tests" acceptance figure in CONTEXT.md is **stale and must not be cited verbatim**. Reproduced live at HEAD `6350798` with a hostile `GIT_DIR` pointed at an unrelated empty repository: `cargo test -p devflow-core --features test-support` → **54 failed / 352 passed** (clean run, 4.08s); `cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` → **44 failed / 139 passed** (clean run, 9.49s). Combined: **98 tests currently fail under a hostile `GIT_DIR`**, not 37. Separately, `pipeline_gate::tests::*` and `pipeline_outcomes::tests::*` (both in `commands.rs`, calling into `git.rs`'s `GitFlow` methods) do **not** complete within a bounded run under a hostile `GIT_DIR` — individual tests from these modules pass in isolation in ~1s, but the modules together did not finish inside a 180s bounded run even with reduced parallelism. This is a real behavioral finding (see § Common Pitfalls) that the planner must account for when writing the acceptance command — do not use bare `cargo test --workspace` as the reproduction command.

**Primary recommendation:** Add `pub fn hermetic_command(program: &str, dir: &Path) -> Command` and `pub fn git_command(repo: &Path) -> Command` (thin wrapper calling `hermetic_command("git", repo)`) to `crates/devflow-core/src/git.rs`, using the exact `REPO_LOCAL_GIT_VARS`/`ALSO_REDIRECTING_GIT_VARS` lists (moved or re-exported from `test_support`, single source of truth). Migrate `staleness.rs::run_git_stdout` first (highest leverage), then the remaining 6 files. Use `cargo test -p devflow-core --features test-support` and `cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` under a hostile `GIT_DIR` as the primary bounded acceptance commands; treat `pipeline_gate`/`pipeline_outcomes` as a documented residual requiring separate, unbounded-timeout investigation, not a blocking acceptance criterion for this phase.

## Architectural Responsibility Map

This is a Rust CLI/library monorepo (no browser/API/DB tiers). The tiers below are the project's own architectural layers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Git subprocess construction (scrubbed) | `devflow-core::git` (library) | — | Both `devflow-core` and `devflow-cli` need it; `devflow-core` is the shared dependency both already have unconditionally. |
| Hermetic env-var list (`REPO_LOCAL_GIT_VARS` etc.) | `devflow-core::git` (library, canonical) | `devflow-core::test_support` (delegates) | A single source of truth prevents the two lists (test-hermeticity vs. production-scrub) from drifting apart — the one hard requirement CONTEXT.md states. |
| Call-site migration (41 sites) | `devflow-core` (21 sites: git.rs, version.rs, worktree.rs, agent_result.rs) | `devflow-cli` (20 sites: commands.rs, staleness.rs, preflight.rs) | Split roughly evenly; `devflow-core` compiles first, so its migration is not blocked by `devflow-cli`. |
| Regression test proving the mechanism (D-03) | `devflow-core::git` test module (unit) | `devflow-core/tests/` (integration, cross-crate hostile-env proof) | Unit test proves the constructor's `Command` object has the vars marked for removal (cheap, no subprocess); integration test proves an actual spawned `git` process resolves the right repo under a hostile `GIT_DIR` (expensive, authoritative). |
| OS/subprocess boundary (where `GIT_DIR` actually gets read) | Outside DevFlow's code — the `git` binary itself | — | Not something this phase can change; the scrub controls what env the child process sees, nothing more. |

## Standard Stack

This phase introduces **no new external dependency**. `std::process::Command`, `std::path::Path`, and `std::env` (via `Command::env_remove`) are already used pervasively in every one of the 7 target files — this is a pure internal-API consolidation, not a library adoption.

### Core (existing, reused)
| Item | Version | Purpose | Why Standard |
|------|---------|---------|--------------|
| `std::process::Command` | Rust std (edition 2024, rustc 1.97.1 confirmed on this machine `[VERIFIED: local rustc --version]`) | Subprocess construction | Already the project's only subprocess mechanism; no `git2`/`gix` dependency exists in `Cargo.lock` `[VERIFIED: grep Cargo.lock]` — introducing one now would be a large, out-of-scope rewrite (see Alternatives Considered). |
| `Command::env_remove(&str)` | Rust std | Per-command environment stripping | Already the exact mechanism `test_support::hermetic_command` uses (`git_command_marks_every_redirecting_var_for_removal` test, `test_support.rs:199-214`) `[VERIFIED: read source]`. Scoped to one `Command`, never global — matches the project's own established constraint (Phase 25's D-14: `std::env::set_var` is `unsafe` in Rust 2024 and unsound in a threaded binary). |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Per-`Command` `env_remove` scrub (subprocess shells out to system `git`) | `git2`/`gix` Rust bindings (libgit2/gitoxide) | Eliminates the whole vulnerability class by construction (no environment-variable repo resolution at all) — but the project has zero existing usage of either crate, and every one of the 41 call sites' git operations (`merge-base --is-ancestor`, `rev-list --ancestry-path`, `worktree list --porcelain`, tag/branch operations with `-c tag.gpgSign=false`) would need re-implementing against a different API. This is an order-of-magnitude larger change than "scrub the environment on 41 existing subprocess calls" and is explicitly not what CONTEXT.md's discretion section asked for. **Rejected for this phase** — a legitimate future consideration, not this phase's mission. |
| A single `hermetic_command(program, dir)` | Two independent constructors (`git_command` git-only, plus a separate ad-hoc scrub wherever a non-git call needs it) | `test_support` already committed to the two-function split (`git_command` delegates to `hermetic_command`); mirroring it avoids inventing a second idiom. Confirmed empirically that the general form is not hypothetical — see § Open Questions #1. |

**Installation:** none required — no `Cargo.toml` changes beyond internal module wiring.

## Package Legitimacy Audit

**Not applicable.** This phase adds zero new external packages/crates. No `npm view`/`pip index`/`cargo search` verification is required; the Package Legitimacy Gate is skipped by design (nothing to audit).

## Architecture Patterns

### System Architecture Diagram

```
                     devflow-cli command handlers
                (commands.rs, staleness.rs, preflight.rs)
                                |
                                | calls
                                v
                     devflow-core public API
              (GitFlow methods, free functions in
               git.rs / version.rs / worktree.rs / agent_result.rs)
                                |
                                | NEW: every git-spawning call routes through
                                v
                  devflow_core::git::hermetic_command(program, dir)
                  devflow_core::git::git_command(repo)   [thin wrapper]
                                |
                                | builds Command, then for each of the 17
                                | redirecting vars: .env_remove(var)
                                v
                       std::process::Command
                    (current_dir pinned, env scrubbed)
                                |
                                | .output() / .spawn()
                                v
                        OS fork+exec -> `git` binary
                (reads ONLY the env this scrubbed Command supplies;
                 GIT_DIR / GIT_WORK_TREE / etc. cannot leak in
                 because the parent process's inherited copies were
                 explicitly removed before spawn)
```

Test-side mirror (unchanged data flow, existing code):
```
      test fixtures --> test_support::git_command(repo) --> [delegates to]
                                                              devflow_core::git::hermetic_command
```

### Recommended Project Structure

No new files are strictly required — the constructor fits inside the existing `devflow-core/src/git.rs`, which already `use std::process::{Command, Stdio};` and is unconditionally compiled.

```
crates/devflow-core/src/
├── git.rs              # ADD: hermetic_command(), git_command(), and the
│                        #   REPO_LOCAL_GIT_VARS / ALSO_REDIRECTING_GIT_VARS
│                        #   constants (moved here as canonical source, or
│                        #   kept in test_support and re-exported — planner's
│                        #   call per CONTEXT.md discretion). Migrate this
│                        #   file's own 9 call sites to use it.
├── version.rs           # Migrate 10 call sites to devflow_core::git::hermetic_command
├── worktree.rs          # Migrate 2 call sites (both already funnel through
│                        #   a local `run()` wrapper at line 174 — 1 edit point)
├── agent_result.rs      # Migrate 3 call sites
└── test_support.rs      # MODIFY: git_command/hermetic_command delegate to
                          #   crate::git::hermetic_command instead of
                          #   duplicating the var-clear loop (recommended,
                          #   not required — avoids drift)

crates/devflow-cli/src/
├── commands.rs           # Migrate 3 direct `Command::new("git")` sites
├── staleness.rs          # Migrate run_git_stdout's ONE call site (line 124)
│                        #   — cascades to ~15 internal + 2 cross-file callers
│                        #   for free. Also 2 direct sites (lines 51, 72) in
│                        #   embedded_commit_is_stale, which does NOT route
│                        #   through run_git_stdout.
└── preflight.rs          # Migrate 11 call sites, including 2 that live
                          #   inside closures (`is_ancestor` at line 355-362,
                          #   `resolve` at line 529-537) — both close over
                          #   `project_root`, migrate the same as any other
                          #   site, just note they will not show up via a
                          #   naive "function signature" search, only a
                          #   literal `Command::new("git")` grep.
```

### Pattern 1: Composable scrub-then-augment
**What:** The constructor returns a `Command`, not a finished, immutable configuration — callers that need additional per-call environment (e.g. `git.rs`'s `git_raw`/`git_raw_combined` at lines 387-392 and 415-420, which already chain `.env("LC_ALL", "C").env("LANG", "C")`) can chain further `.env(...)` calls after `hermetic_command()` returns.
**When to use:** Every one of the 41 sites. Confirmed empirically: 2 of the 41 sites (`git.rs:387`, `git.rs:415`) already set `LC_ALL`/`LANG` before `.current_dir()` — these do not conflict with the redirecting-var scrub (different keys; `Command`'s env map is keyed independently per variable name, so `env_remove(GIT_DIR)` and `env(LC_ALL, "C")` are order-independent).
**Example:**
```rust
// Source: proposed, modeled directly on test_support.rs:183-190 (verified existing code)
use std::path::Path;
use std::process::Command;

pub const REPO_LOCAL_GIT_VARS: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_CONFIG", "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT", "GIT_OBJECT_DIRECTORY", "GIT_DIR", "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE", "GIT_GRAFT_FILE", "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS", "GIT_REPLACE_REF_BASE", "GIT_PREFIX",
    "GIT_SHALLOW_FILE", "GIT_COMMON_DIR",
];
pub const ALSO_REDIRECTING_GIT_VARS: &[&str] =
    &["GIT_NAMESPACE", "GIT_DISCOVERY_ACROSS_FILESYSTEM"];

/// Drop-in replacement for `Command::new(program).current_dir(dir)` that
/// additionally strips every variable that could redirect a git-invoking
/// process onto a repository the caller did not name. Unconditional — no
/// bypass parameter exists (D-01).
pub fn hermetic_command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir);
    for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
        cmd.env_remove(var);
    }
    cmd
}

pub fn git_command(repo: &Path) -> Command {
    hermetic_command("git", repo)
}

// Existing call site (git.rs:387-392), migrated:
let output = git_command(&self.root)   // was: Command::new("git")
    .args(args)
    .env("LC_ALL", "C")   // unaffected — different key, order-independent
    .env("LANG", "C")
    .output()?;
```

### Pattern 2: Chokepoint migration before per-site migration
**What:** Where a file already has an internal wrapper function (`staleness.rs::run_git_stdout`, `worktree.rs::run`, `git.rs`'s `git`/`git_output`/`git_raw`/`git_raw_combined` methods), migrate the wrapper's own `Command::new("git")` call first — every caller of that wrapper is scrubbed without a separate edit.
**When to use:** `staleness.rs` (run_git_stdout, ~17 total callers cascade from 1 edit), `worktree.rs` (`run`, 1 of its 2 call sites is the wrapper itself), `git.rs` (4 method-level wrappers already exist; migrating those covers most of git.rs's internal helper usage — only the 5 free-standing sites at lines 100, 175, 488, 497, 708 need direct edits).
**Anti-pattern to avoid:** Editing all 41 line numbers mechanically without first identifying which are wrappers — wastes effort and risks missing that a wrapper's callers were never touched (verified: `staleness.rs:51` and `staleness.rs:72`, inside `embedded_commit_is_stale`, do NOT go through `run_git_stdout` and need their own direct edit).

### Anti-Patterns to Avoid
- **Assuming `test_support`'s feature gate can just be deleted to "promote" it to production:** `test_support.rs` also carries the unrelated 25-11/999.47 exec-visibility barrier (`wait_for_exec_visibility`), which is deliberately test-only ("Never compiled into a normal build" — its own doc comment). Removing the `#[cfg(...)]` gate to expose `git_command` would also compile that barrier into every release binary. Build the production constructor in a new location (`git.rs`) instead.
- **Treating `Command::new("sh").arg("-c").arg("cargo test")` (commands.rs, `test_cmd`, ~line 1956) as out of scope by assumption:** it is not one of the 41 counted sites and not in CONTEXT.md's 7-file list, but it is a genuine indirect git-invoking path (`sh` → `cargo` → `build.rs`'s own `run_git`, which shells to git at compile time). See § Open Questions #1 — this needs an explicit in/out-of-scope decision, not silent omission.
- **Using bare `cargo test --workspace` as the D-03 acceptance command:** confirmed empirically to not complete in a bounded time under a hostile `GIT_DIR` (timed out at both 240s and 480s) due to `pipeline_gate`/`pipeline_outcomes` test modules. Use the two scoped commands in § Validation Architecture instead.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Discovering git's repository-local variable list | A hand-maintained list with no drift check | `git rev-parse --local-env-vars`, asserted against the constant by a test (`local_env_vars_match_git`, already exists in `test_support.rs:233-260`) | A git upgrade that adds a new repository-local variable would silently reopen the hole if the list is static and unchecked. This project already solved this exact problem once for the test side — reuse the pattern, do not reinvent it for production. |
| Global process environment sanitization | `std::env::remove_var`/`set_var` at process start | Per-`Command` `env_remove` | `std::env::set_var` is `unsafe` in Rust 2024 (data race risk in a threaded binary) — already an established constraint in this codebase (Phase 25 D-14, restated in CONTEXT.md's Established Patterns). |

**Key insight:** This problem was already solved once, correctly, in `test_support.rs`. The entire "Don't Hand-Roll" risk for this phase is hand-rolling a *second, divergent* copy of a solution that already exists — the discretion question CONTEXT.md poses (delegate vs. stay independent) exists precisely to prevent that.

## Common Pitfalls

### Pitfall 1: Believing "wraps `Command::new("git")`" means "covers every git invocation"
**What goes wrong:** A migration that greps only for literal `Command::new("git")` and treats that as complete leaves `commands.rs::test_cmd`'s `Command::new("sh").arg("-c").arg("cargo test")` unscrubbed — and `cargo test` triggers `devflow-cli`'s own `build.rs`, which itself calls `Command::new("git")` (D-02's `run_git`, lines 76-85) to embed `DEVFLOW_BUILD_COMMIT`. A hostile `GIT_DIR` inherited by `devflow test`'s child `sh` process reaches that build-time git call too.
**Why it happens:** The 41-site count was deliberately scoped to literal `Command::new("git")` — a reasonable scoping decision for tractability, but it means the count is a floor, not a ceiling, for "places a redirecting variable can reach git."
**How to avoid:** Decide explicitly (see § Open Questions #1) whether `test_cmd` is in scope for this phase, and document the decision either way rather than leaving it undiscovered.
**Warning signs:** Any production code that shells out to `cargo`, `make`, `sh -c "..."`, or another program that is known (or could plausibly) invoke `git` internally, without an explicit note that it was considered and excluded.

### Pitfall 2: The full-workspace test suite does not complete in a bounded time under a hostile `GIT_DIR`
**What goes wrong:** `GIT_DIR=<hostile> cargo test --workspace` was run twice during this research with 240s and then 480s timeouts; both timed out (exit 124) without printing a final `test result:` summary line. Isolating `pipeline_gate::tests::advance_ship_success_runs_finish_workflow` alone completes in ~1s — so no single test is infinite-looping — but `pipeline_gate` + `pipeline_outcomes` together did not finish within 180s even at reduced parallelism (`--test-threads=4`).
**Why it happens:** Not fully diagnosed in this research pass (out of the D-03 mechanism-proof scope to root-cause). Plausible contributors: these modules call `GitFlow` methods (`release_finish`, `tag`, merge operations) that, under a hostile `GIT_DIR`, now operate against a real-but-foreign repository rather than failing fast — producing longer subprocess chains, more retries, or lock contention against `/tmp`'s throwaway repo shared across parallel test threads.
**How to avoid:** Do not write a regression test or CI acceptance step that runs the unscoped full suite under a hostile `GIT_DIR` with a short timeout — it will flake as a timeout, not a clean pass/fail. Use the two scoped commands in § Validation Architecture. Treat `pipeline_gate`/`pipeline_outcomes` as a documented residual for a follow-up investigation, not a blocking claim for this phase's acceptance criterion.
**Warning signs:** A CI job or regression test with a `cargo test --workspace` step and a timeout under ~5 minutes when `GIT_DIR` is set for the whole job.

### Pitfall 3: Assuming the 37 (or 41) figure is still accurate
**What goes wrong:** CONTEXT.md's `37`-failing-test claim (from 999.37, an earlier phase) does not match a live re-measurement at this phase's own HEAD. The actual, reproducible, bounded count is 54 (devflow-core) + 44 (devflow-cli, `pipeline_gate`/`pipeline_outcomes` excluded) = 98.
**Why it happens:** Test suites grow between when a number is recorded and when it's cited three phases later — 999.37's `37` was measured against an earlier HEAD, before subsequent phases (18 through 26) added new tests exercising the same git-calling code paths.
**How to avoid:** Cite the re-measured numbers (54 / 44 / 98) in the plan's acceptance criteria, sourced to this research document, not the CONTEXT.md figure. Re-measure again immediately before writing the phase's final acceptance test, since more commits may land between research and execution.
**Warning signs:** A plan or SUMMARY.md that states "37 tests now pass" without an accompanying fresh command output.

### Pitfall 4: Forgetting `test-support` feature is required to compile devflow-core's own `tests/*.rs` integration tests
**What goes wrong:** `cargo test -p devflow-core` alone (no `--features test-support`) fails to compile with `E0433: cannot find test_support in devflow_core` for `tests/monitor_e2e.rs` and `tests/devflow_dir_gitignore.rs`, because integration test crates are external to the lib target and don't benefit from the `#[cfg(test)]` half of `test_support`'s gate — only `cargo test --workspace` (which unifies features across the whole build graph because `devflow-cli`'s `[dev-dependencies]` entry enables `test-support`) or an explicit `--features test-support` flag makes it compile.
**Why it happens:** Feature unification is a workspace-wide, not package-wide, default.
**How to avoid:** Always pass `--features test-support` when running `-p devflow-core` in isolation; document this in the regression test's run command.
**Warning signs:** `error[E0433]: cannot find test_support in devflow_core` when trying to reproduce a devflow-core-only test run.

## Code Examples

### Existing verified pattern to mirror (test-side, unchanged by this phase)
```rust
// Source: crates/devflow-core/src/test_support.rs:164-190 (read directly, HEAD 6350798)
/// A `git` command pinned to `repo` **and** stripped of every inherited
/// variable that could redirect it somewhere else.
pub fn git_command(repo: &Path) -> Command {
    hermetic_command("git", repo)
}

pub fn hermetic_command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir);
    for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
        cmd.env_remove(var);
    }
    cmd
}
```

### Recommended regression test proving D-03's mechanism claim
```rust
// Source: proposed, modeled on test_support.rs:196-214's existing assertion
// style plus an actual hostile-env subprocess proof (the deeper, authoritative
// half D-03 asks for — not just "the Command object marks vars for removal"
// but "a spawned process under this constructor genuinely resolves the
// caller-supplied root, not GIT_DIR's target").
#[test]
fn hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir() {
    let real_repo = tempfile::tempdir().unwrap();
    // ... `git init` real_repo, one commit ...
    let foreign_repo = tempfile::tempdir().unwrap();
    // ... `git init` foreign_repo, a DIFFERENT commit ...

    let output = crate::git::git_command(real_repo.path())
        .args(["rev-parse", "--show-toplevel"])
        .env("GIT_DIR", foreign_repo.path().join(".git")) // hostile injection
        .output()
        .unwrap();

    let resolved = String::from_utf8_lossy(&output.stdout);
    assert!(
        resolved.trim().ends_with(real_repo.path().file_name().unwrap().to_str().unwrap()),
        "hermetic_command must resolve real_repo even with a foreign GIT_DIR set: got {resolved}"
    );
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `.current_dir(&root)` alone, environment inherited unmodified | `.current_dir(&root)` + per-command `env_remove` of all 17 redirecting vars | Test side: 999.37 (this phase brings production to parity) | `GIT_DIR` outranks `.current_dir()`/`git -C`; only `--git-dir` (a flag, never used at these 41 sites) or clearing the env var closes the gap. |

**Deprecated/outdated:** None — this is the codebase's own established, already-shipped pattern (test side) being extended to production; no external library or approach is being replaced.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | ASVS category mapping in § Security Domain reflects general OWASP guidance, not a codebase-specific ASVS L1 register verified against this project's existing security artifacts (`20-SECURITY.md`, `21-SECURITY.md`, etc.) | Security Domain | Low — the phase's actual security control (env scrub) is independently verified by source reading and live test runs; the ASVS labels are documentary framing, not load-bearing for correctness. |
| A2 | `commands.rs::test_cmd`'s `sh -c "cargo ..."` call is the only non-`Command::new("git")` production site that indirectly reaches git — not exhaustively proven across the whole workspace, only checked for `Command::new("cargo")` (zero hits) and manually inspected the non-git `Command::new` call sites in the 7 target files plus `commands.rs` in full | Common Pitfalls / Open Questions | Medium — if another indirect path exists and is missed, a hostile `GIT_DIR` could still reach git through it after this phase ships, undermining D-01's "no legitimate reason" framing without anyone noticing until an incident. |

## Open Questions

1. **Is `commands.rs::test_cmd`'s `sh -c "cargo test"` (and its `cargo clippy`/`cargo fmt --check` siblings, ~line 1945-1978) in scope for this phase?**
   - What we know: it is NOT among the 41 counted `Command::new("git")` sites, and NOT in CONTEXT.md's "Code this phase changes" file list. It genuinely can reach `git` indirectly, via `cargo`'s invocation of `devflow-cli`'s own `build.rs::run_git` during compilation triggered by `cargo test`/`cargo clippy`/`cargo fmt --check`.
   - What's unclear: whether the operator considers this within D-01's "no legitimate reason a DevFlow-issued command should silently redirect" framing, given it's one level more indirect than the 41 counted sites and touches the same `build.rs` code D-02 explicitly excludes (but for a different reason — D-02 excludes build.rs's *own* compile-time invocation as a different actor/moment; this is a *runtime* `devflow test` command re-triggering that same build.rs call).
   - Recommendation: raise explicitly with the operator before planning locks scope, OR have the planner make an explicit, documented in/out decision citing this research section. If out of scope, route `test_cmd`'s `sh -c` call through `hermetic_command("sh", project_root)` as a one-line "free win" regardless (it costs nothing and closes a real gap), even if not treated as a blocking acceptance criterion.

2. **Root cause of `pipeline_gate`/`pipeline_outcomes` not completing within a bounded run under a hostile `GIT_DIR`.**
   - What we know: individual tests from these modules pass in ~1s in isolation; together, under `GIT_DIR`, they do not finish within 180s even at `--test-threads=4`.
   - What's unclear: whether this is a true hang (infinite retry/poll loop somewhere reachable via `GitFlow::release_finish`/`tag`/merge operations) or simply extreme slowdown from many parallel subprocess spawns each taking longer against a foreign repo.
   - Recommendation: not required for D-03 (which only needs the *mechanism* proven, not every existing test's timing behavior fixed), but flag as a residual risk. If the phase's migration incidentally fixes this (likely, since these tests call through `git.rs`, one of the 7 migrated files), re-measure post-migration and note the result in the phase's SUMMARY.md as a bonus finding rather than a claimed acceptance criterion.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | Every one of the 41 call sites; the `local_env_vars_match_git`-style re-verification test | Yes `[VERIFIED: git --version]` | 2.55.0 | — |
| `cargo`/`rustc` | Build, test, clippy, fmt | Yes `[VERIFIED: cargo --version / rustc --version]` | cargo 1.97.1, rustc 1.97.1, edition 2024 | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` (`#[test]` attributes), no separate test runner or config file |
| Config file | none — feature gating (`test-support`) in `crates/devflow-core/Cargo.toml` is the closest analogue |
| Quick run command (per-file, during migration) | `cargo test -p devflow-core --features test-support --lib <module>::` (e.g. `git::`, `version::`, `worktree::`, `agent_result::`) or `cargo test -p devflow --bin devflow -- <module>::` (e.g. `staleness::`, `preflight::`, `commands::`) |
| Full suite command (normal, non-hostile environment) | `cargo test --workspace` — this already passes cleanly at HEAD per project history (hundreds of tests, 0 failed as of Phase 25); unaffected by this phase's changes in the normal case |
| **Acceptance-specific command (hostile `GIT_DIR`, the D-03 signal)** | See below — do NOT use `cargo test --workspace` for this; use the two scoped commands. |

### Phase Requirement -> Test Map
(Keyed on decision IDs, per this project's no-REQ-ID convention.)

| Decision ID | Behavior | Test Type | Automated Command | File Exists? |
|-------------|----------|-----------|---------------------|-------------|
| D-01 | Every scrubbed `Command` has all 17 redirecting vars marked for `env_remove`; no bypass parameter | unit | `cargo test -p devflow-core --features test-support --lib git::tests::hermetic_command_marks_every_redirecting_var_for_removal` (name TBD by planner, mirrors `test_support.rs:199`) | ❌ Wave 0 — new test to author in `git.rs` |
| D-03 (mechanism proof) | A `git_command`/`hermetic_command`-constructed process resolves the caller-supplied root even with a hostile `GIT_DIR` set | integration | `cargo test -p devflow-core --features test-support --lib git::tests::hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir` (see § Code Examples) | ❌ Wave 0 — new test to author |
| D-03 (acceptance signal — devflow-core) | Previously-failing tests under a hostile `GIT_DIR` now pass | regression, full-file | `GIT_DIR=<throwaway-repo>/.git cargo test -p devflow-core --features test-support` — currently 54 failed / 352 passed; target after migration: 0 failed | ✅ existing tests, migration is the fix |
| D-03 (acceptance signal — devflow-cli) | Same, scoped to avoid the non-terminating modules | regression, full-file | `GIT_DIR=<throwaway-repo>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` — currently 44 failed / 139 passed; target after migration: 0 failed | ✅ existing tests, migration is the fix |
| D-02 | `build.rs` is untouched by this phase | plan-time check, not a runtime test | `git diff --stat <base>..<head> -- crates/devflow-cli/build.rs` must be empty | n/a — verification step, not a test file |

### Sampling Rate
- **Per task commit:** quick-run command scoped to the file just migrated (see Quick run command above).
- **Per wave merge:** `cargo test --workspace` (normal environment) — confirms no regression in the ordinary, non-hostile case.
- **Phase gate:** both hostile-`GIT_DIR` scoped commands above, run to a clean `test result:` line (not a timeout), before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] New unit test in `crates/devflow-core/src/git.rs`'s existing `#[cfg(test)] mod tests` (starts at line 936) — asserts every redirecting var is marked for removal on the new constructor's output, mirroring `test_support.rs:196-214`.
- [ ] New integration-style unit test (can live in the same module, using `tempfile` — already a dependency given `test_support`'s own test patterns) proving a spawned process resolves the caller's root, not a hostile `GIT_DIR`'s target (see § Code Examples).
- [ ] No framework install needed — `cargo test` is already fully functional in this environment.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V5 Input Validation / Command Construction (ASVS 5.0's "Encoding and Sanitization" / OS command injection defense) | yes | Parameterized subprocess construction (`Command::args([...])`, never a shell-interpolated string) — already the project's universal pattern at all 41 sites; this phase does not change argument handling, only environment. `[CITED: OWASP OS Command Injection Defense Cheat Sheet]` |
| V14 Configuration (environment-derived configuration must not silently override explicit configuration) | yes | This IS the phase's core control: an explicit, caller-supplied `project_root`/`repo` argument must not be silently overridden by an inherited environment variable. `env_remove` on a documented, git-verified allowlist, scoped per-`Command`. |
| V6 Cryptography | no | Not touched by this phase (the existing `-c tag.gpgSign=false` scoping in `git.rs:127-133` is unrelated, pre-existing, and untouched). |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Environment-variable repository redirection (`GIT_DIR`/`GIT_WORK_TREE` inherited from a parent process — hooks, `rebase --exec`, `bisect run`, `submodule foreach`, or a maliciously/accidentally polluted shell) causing a git-invoking process to silently act on a different repository than the one it was told to operate on | Tampering | Per-`Command` `env_remove` of the complete, git-verified redirecting-variable list, applied unconditionally with no bypass (D-01). This is exactly the control this phase builds. |
| A guard that compares two paths to confirm "same repository" but each path was resolved through a different, independently-redirectable channel (`--show-toplevel` reads `.git`/cwd; HEAD/refs/objects read `GIT_DIR`) — the CR-01 finding this phase exists to close | Tampering / Elevation of Privilege (an irreversible operation — push/tag/publish — executes against an unintended repository) | Scrub environment BEFORE any comparison or mutating operation runs, so both halves of any future guard (like 999.25's `mutating_project_root`) read from the same, caller-controlled source of truth. This phase is explicitly the prerequisite fix; it does not itself build the guard (D-03). |

## Sources

### Primary (HIGH confidence — verified directly against this repository at HEAD `6350798`)
- `crates/devflow-core/src/test_support.rs` (read in full) — existing hermetic-command pattern, variable lists, and their regression tests.
- `crates/devflow-core/src/git.rs`, `version.rs`, `worktree.rs`, `agent_result.rs`; `crates/devflow-cli/src/commands.rs`, `staleness.rs`, `preflight.rs` — all 41 call sites read with surrounding context via `rg -n -A6 'Command::new\("git"\)'` and targeted `Read` calls.
- `git --version` (2.55.0) and `git rev-parse --local-env-vars` run live on the implementation machine — matches `REPO_LOCAL_GIT_VARS` exactly, zero drift.
- `GIT_DIR=<throwaway>/.git cargo test -p devflow-core --features test-support` and `GIT_DIR=<throwaway>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` — both run live, full output captured (54/352 and 44/139 respectively).
- `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/Cargo.toml`, `crates/devflow-core/src/lib.rs` — confirmed `test-support` feature gate and its production-visibility implications.
- `Cargo.lock` — confirmed no `git2`/`gix` dependency exists.

### Secondary (MEDIUM confidence)
- [OWASP OS Command Injection Defense Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/OS_Command_Injection_Defense_Cheat_Sheet.html) — general ASVS framing for § Security Domain.
- [OWASP Application Security Verification Standard](https://owasp.org/www-project-application-security-verification-standard/) — category naming reference.

### Tertiary (LOW confidence)
- None used — this phase's domain is entirely internal to the codebase; no library-adoption or ecosystem claims were needed beyond the two secondary citations above.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependency; the pattern to mirror is existing, read, and test-verified source in this exact repository.
- Architecture: HIGH — module boundaries, feature gates, and the `run_git_stdout`/`GitFlow`-method chokepoints were all confirmed by direct source reading, not inferred.
- Pitfalls: HIGH for pitfalls 1, 3, 4 (all directly reproduced/verified); MEDIUM for pitfall 2 (the hang/slowdown is confirmed to occur, but its root cause is not fully diagnosed in this research pass — flagged as Open Question #2 rather than asserted).

**Research date:** 2026-07-30
**Valid until:** Re-verify the 41-site count, the variable-list match, and the failing-test counts immediately before planning executes if more than a few days elapse or if `develop`/this feature branch advances — this codebase changes fast (multiple phases per week) and the unmerged `feature/phase-26` cluster shares `git.rs`, a direct source of drift.
