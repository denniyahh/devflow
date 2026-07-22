# Phase 19: Release Integrity + `main.rs` Decomposition - Pattern Map

**Mapped:** 2026-07-21
**Files analyzed:** ~9 new/modified production files + 1 new shared test module + 1 new skill directory
**Analogs found:** 8 / 9 with a strong or partial in-repo match; 1 (shared `#[cfg(test)] pub(crate) mod` test-support module) has **no existing analog anywhere in the workspace** — this phase introduces the first one.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-cli/src/preflight.rs` (new) | module (extracted business logic) | request-response (called from `run()`/pipeline) | `crates/devflow-core/src/agents/claude.rs` (submodule-of-`mod.rs`, thin adapter) | role-match (submodule split pattern) |
| `crates/devflow-cli/src/staleness.rs` (new) | module (extracted business logic) | CRUD (reads/derives from git + fs) | `crates/devflow-core/src/agents/codex.rs` | role-match |
| `crates/devflow-cli/src/pipeline.rs` or `pipeline_{launch,outcomes,gate}.rs` (new) | module (state machine) | event-driven (stage transitions) | `crates/devflow-core/src/agents/mod.rs` (trait/dispatch + `pub mod` sub-files) | role-match |
| `crates/devflow-cli/src/parallel.rs` (new) | module (orchestration) | event-driven / batch | `crates/devflow-core/src/agents/mod.rs` layout | role-match |
| `crates/devflow-cli/src/commands.rs` (new) | module (CLI subcommand handlers) | request-response | `crates/devflow-core/src/agents/mod.rs` layout (dispatch style, `adapter_for`) | partial-match (no existing multi-subcommand dispatch file outside `main.rs` itself) |
| `crates/devflow-cli/src/config_parse.rs` (optional, new) | utility | transform (pure env-parsing) | `crates/devflow-cli/src/main.rs:27-31` `parse_gate_timeout` (in-place, pre-move) | exact (self-analog, just relocated) |
| `crates/devflow-cli/src/test_support.rs` (new) | test-support module | n/a (shared fixtures) | **none in-repo** — closest partial precedents: `crates/devflow-core/src/gates.rs:340-348` (bare `ENV_MUTEX`) and `crates/devflow-core/src/config.rs:169-192` (`ENV_MUTEX` + `EnvOverride` RAII guard) | no analog (first `pub(crate)` shared cross-file test module in workspace) |
| `crates/devflow-core/src/workflow.rs` — new `pub fn ensure_devflow_dir()` | utility (fs side-effecting helper) | file-I/O | `crates/devflow-core/src/workflow.rs:93-101` `write_state_atomic` (same file, sibling function) | exact |
| `crates/devflow-core/src/git.rs` — modify `commit_path` (drop `--allow-empty`) | service method | request-response (shell out to `git`) | `crates/devflow-core/src/git.rs:308-319` `commit_all` (same file, sibling method, the pre-fix idiom to diverge from) | exact |
| `.claude/skills/ai-change-acceptance/SKILL.md` + `rules/*.md` (new, 19g) | config (skill doc) | n/a | **none in-repo** — DevFlow has no `.claude/skills/` directory today (confirmed via research); no in-repo analog, must follow the global `gsd-code-reviewer` project-skill discovery contract instead | no analog |

## Pattern Assignments

### `crates/devflow-cli/src/{preflight,staleness,pipeline*,parallel,commands}.rs` (module split, D-05/D-06)

**Analog:** `crates/devflow-core/src/agents/mod.rs` + `crates/devflow-core/src/agents/claude.rs` — the repo's **only** existing submodule-directory pattern, and D-07's own withdrawal note confirms it as the accepted precedent ("the 'inconsistent with the codebase' objection was withdrawn — `agents/mod.rs` + `claude.rs`/`codex.rs` is exactly this pattern").

**Module declaration + re-export shape** (`crates/devflow-core/src/agents/mod.rs:70-76`):
```rust
pub mod claude;
pub mod codex;
pub mod opencode;

pub use claude::ClaudeAgent;
pub use codex::CodexAgent;
pub use opencode::OpenCodeAgent;
```
Apply this shape in the new thin `main.rs`: `mod preflight; mod staleness; mod pipeline; mod parallel; mod commands;` (all `pub(crate)`-visible internally — devflow-cli is a binary crate with no external consumers, so **do not** use `pub` here; use bare `mod` + `pub(crate) fn`/`pub(crate) struct` on the items each sibling needs, matching Pitfall 6's guidance in RESEARCH.md). Note: `agents/mod.rs` uses `pub mod`/`pub use` because `devflow-core` is a *library* crate with external (intra-workspace) consumers (`devflow-cli` imports `devflow_core::agents`). `devflow-cli`'s own module split has no such external consumer, so `pub(crate)` is the correct, tighter visibility — the analog's *shape* transfers, its *visibility keyword* does not.

**Sibling-file internal structure** (`crates/devflow-core/src/agents/claude.rs:1-11`, read via Grep):
```rust
use super::AgentAdapter;

pub struct ClaudeAgent;

impl AgentAdapter for ClaudeAgent {
```
Pattern: sibling files `use super::X` for shared types defined in the parent `mod.rs`, not `use crate::agents::X`. Apply this for cross-module calls within the split (e.g. `pipeline.rs` calling into `preflight.rs`) — but note RESEARCH.md's Pattern 2 finding: `preflight.rs` and `pipeline*.rs` are siblings under `main.rs`, not parent/child, so the correct import there is `use crate::pipeline::launch_stage_inner;` / `use crate::preflight::run_preflight;` (crate-root-relative, since neither is `super` of the other) — the `agents/` analog's `use super::` only applies to items actually defined in the crate-root `main.rs` (e.g. `CliError`, per Pitfall 6).

**`#[cfg(test)]` placement relative to production code** (`crates/devflow-core/src/agents/mod.rs:78-79`, `crates/devflow-core/src/git.rs` — confirmed pattern across the codebase): test module sits at the **bottom of the same file** as the production code it tests, never in a separate file:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::stage_prompt;
    use crate::stage::Stage;
    ...
}
```
D-10 requires the split preserve this — each new sibling file (`preflight.rs`, `staleness.rs`, etc.) gets its own `#[cfg(test)] mod tests { use super::*; use crate::test_support::*; ... }` block at the bottom, moved atomically with its production code (also required independently by RESEARCH.md Pitfall 5, the binary-crate dead-code-lint hazard).

**Error type co-location** (`crates/devflow-core/src/git.rs:10-17`, `crates/devflow-core/src/workflow.rs:14-25`): error enums live in the same file as the functions that return them, using `thiserror::Error` derive with `#[error("...")]` per variant and `#[from]` for wrapped I/O errors. `main.rs`'s existing `CliError` (defined at `main.rs:312`, confirmed via Grep) is the split's central error type — per RESEARCH.md Pitfall 6, it must become `pub(crate)` and stay in the thin `main.rs` (it is used across every cluster, not owned by one), with every sibling module doing `use crate::CliError;`.

---

### `crates/devflow-cli/src/test_support.rs` (new, D-01/D-03 — shared env-mutation test fixtures)

**No existing analog for a shared cross-file `#[cfg(test)]` module exists in this workspace.** Confirmed by inspection of `crates/devflow-core/src/lib.rs:54-77` (no `mod test_support` or similar declared) and `rg -n "pub(crate) mod tests\|pub(crate) static ENV_MUTEX"` across `crates/` returning no hits. Every existing `ENV_MUTEX` is `mod`-local and non-shared:

**`crates/devflow-core/src/gates.rs:340-348`** — bare mutex, no RAII guard:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-global env vars (`set_var`/
    /// `remove_var` are process-wide and `cargo test` runs in parallel by
    /// default) so they don't race each other.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());
```
Usage site (`gates.rs:608-613`): `let _guard = ENV_MUTEX.lock().unwrap();` with a `// SAFETY: serialized under ENV_MUTEX — no other thread in this...` comment — this per-variable safety-reasoning comment style should be preserved when hoisting.

**`crates/devflow-core/src/config.rs:169-192`** — mutex **plus** an `EnvOverride` RAII guard struct, the strongest analog for the shared module's ergonomics (worth extracting near-verbatim):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvOverride(&'static str);

    impl EnvOverride {
        fn set(key: &'static str, value: &str) -> Self {
            // SAFETY: Tests that mutate this process-global variable are
            // serialized by ENV_MUTEX and the guard removes it on drop.
            unsafe { std::env::set_var(key, value) };
            Self(key)
        }
    }

    impl Drop for EnvOverride {
        fn drop(&mut self) {
            // SAFETY: See EnvOverride::set; the same mutex guard is still held.
            unsafe { std::env::remove_var(self.0) };
        }
    }
```
Usage: `let _lock = ENV_MUTEX.lock().unwrap(); ... let _env = EnvOverride::set("DEVFLOW_CAPTURE_RETENTION", "12");` — lock first, then RAII env override, both dropped at end of scope.

**`crates/devflow-cli/src/main.rs:4034`** (the mutex being hoisted) is itself `mod`-local today, same bare-mutex shape as `gates.rs`, guarding `PATH` (36 uses), `DEVFLOW_GATE_TIMEOUT_SECS` (9), `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` (2), `DEVFLOW_GATE_NOTIFY_CMD` (1) across 18 `.lock()` sites at the line numbers RESEARCH.md enumerates.

**Recommended synthesis for `test_support.rs`:** `#![cfg(test)]` at file top (RESEARCH.md's own skeleton, `main.rs:4026-4034` derivation), `pub(crate) static ENV_MUTEX: Mutex<()>`, and consider adopting `config.rs`'s `EnvOverride` RAII pattern for the hoisted module even though today's `main.rs` tests set/remove env vars manually — it is a strict readability improvement over the bare pattern and D-09's "pure move" constraint applies to *production* logic, not to mechanically wrapping existing test-only env mutation in an existing in-repo idiom (confirm this reading with the planner before committing to it, since it is a judgment call at the boundary of "pure move").

---

### `crates/devflow-core/src/workflow.rs` — new `pub fn ensure_devflow_dir()` (19a / D-14)

**Analog:** `crates/devflow-core/src/workflow.rs:93-101`, `write_state_atomic` — same file, sibling private helper, the closest existing "create a `.devflow/` subdirectory and write a file with error mapping" idiom:
```rust
fn write_state_atomic(path: &Path, contents: &str) -> Result<(), WorkflowError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```
Adjacent `devflow_dir` accessor to build on (`workflow.rs:33-35`):
```rust
pub fn devflow_dir(project_root: &Path) -> PathBuf {
    project_root.join(".devflow")
}
```
`WorkflowError` enum to reuse for the new function's `Result` type (`workflow.rs:14-25`):
```rust
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("state I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("state JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no active DevFlow state at {0}")]
    MissingState(PathBuf),
}
```
`ensure_devflow_dir(project_root: &Path) -> io::Result<PathBuf>` (per RESEARCH.md Pitfall 3's recommended signature) should call `std::fs::create_dir_all(&dir)?` then idempotently `std::fs::write(dir.join(".gitignore"), "*\n")` only if absent (or unconditionally — content is a fixed literal, both are fine per D-14) — mirror `write_state_atomic`'s `create_dir_all` + `?`-propagation style, not its atomic-rename dance (a `.gitignore` write does not need crash-atomicity the way state persistence does).

**Call site to wire (per RESEARCH.md's verified earliest-universal-path finding):** `start()`'s `workflow::save_state(&state)?;` at `main.rs:622`, which runs before `events::emit` and `launch_stage` in every `start()` invocation. **A2 in RESEARCH.md flags this as unverified for `parallel`/`sequentagent` paths — the plan/implementer must grep those command paths' call order before finalizing this as the single insertion point**, since `run_agent_blocking` (the `sequentagent`/`parallel` path, `main.rs:2417`) does not call `save_state` at all (confirmed false premise for the RESEARCH.md-proposed chokepoint per CONTEXT.md D-14's correction) — this may mean the fix needs a call site inside `run_agent_blocking`'s own directory-creation path too, not just `start()`. All 7 existing `create_dir_all` sites (`workflow.rs:95`, `gates.rs:325`, `monitor.rs:98`, `agent_result.rs:964`, `events.rs:58`, `ship.rs:85`, `lock.rs:82`) must be converted to call the new function per D-14's exact wording ("all 7 existing `create_dir_all` sites converted to call it").

---

### `crates/devflow-core/src/git.rs` — modify `commit_path` (19b / D-16)

**Analog:** same file, `commit_all` (`git.rs:308-319`) — the sibling method whose `--allow-empty` + `nothing to commit` idiom `commit_path` currently duplicates and D-16 says to diverge from:
```rust
pub fn commit_all(&self, message: &str) -> Result<(), GitError> {
    debug!("committing all changes: {message}");
    self.git(["add", "."])?;
    // --allow-empty so we don't fail when there are no changes
    match self.git_raw(&["commit", "--allow-empty", "-m", message]) {
        Ok(()) => Ok(()),
        Err(GitError::Command(ref msg)) if msg.contains("nothing to commit") => Ok(()),
        Err(e) => Err(e),
    }
}
```
**Current `commit_path`** (`git.rs:325-346`) to modify — drop `"--allow-empty"` from the `git_raw` args slice, keep the `"nothing to commit"` match arm (D-16: "let the existing `nothing to commit` arm become the genuine no-op"):
```rust
pub fn commit_path(&self, relative_path: &str, message: &str) -> Result<(), GitError> {
    debug!("committing {relative_path}: {message}");
    self.git(["add", relative_path])?;
    // --allow-empty so we don't fail when the path had no changes.
    match self.git_raw(&[
        "commit",
        "--allow-empty",       // <-- D-16: remove this line and its trailing comment
        "-m",
        message,
        "--",
        relative_path,
    ]) {
        Ok(()) => Ok(()),
        Err(GitError::Command(ref msg)) if msg.contains("nothing to commit") => Ok(()),
        Err(e) => Err(e),
    }
}
```
**`git_raw` error-matching idiom to preserve exactly** (`git.rs:415-426`):
```rust
fn git_raw(&self, args: &[&str]) -> Result<(), GitError> {
    debug!("git {}", args.join(" "));
    let output = Command::new("git")
        .args(args)
        .current_dir(&self.root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(GitError::Command(stderr_or_status(&output)))
    }
}
```
The `Err(GitError::Command(ref msg)) if msg.contains("nothing to commit")` match arm in `commit_path` depends on git's own stderr wording surfaced through this exact `stderr_or_status` → `GitError::Command` path — do not change `git_raw` or `GitError` itself, only the call-site args in `commit_path`.

**Also for D-17 (record-not-change):** `commit_all` at `git.rs:308-319` is confirmed the *only* remaining `commit_all` caller per RESEARCH.md, at `crates/devflow-core/src/hooks.rs:184` (`git.commit_all("docs: update generated docs")`). Leave `commit_all` untouched; document in the plan/implementation notes why its empty-commit behavior is retained (D-17 requires recording the reason, not just skipping it).

---

## Shared Patterns

### Env-mutation test serialization (D-01/D-04)
**Sources:** `crates/devflow-core/src/gates.rs:340-348`, `crates/devflow-core/src/config.rs:169-192`
**Apply to:** every split module's `#[cfg(test)] mod tests` block that mutates `PATH`, `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, or `DEVFLOW_GATE_NOTIFY_CMD` — all must import `crate::test_support::ENV_MUTEX` (the new hoisted single instance) instead of declaring a local one. D-04's invariant — "every env var is guarded by exactly one mutex, and no var is touched under two" — has no existing enforcement anywhere in the codebase; this phase is where it must first become true by construction for the `devflow-cli` binary crate.

### `thiserror`-derived error enums co-located with their producing function
**Sources:** `crates/devflow-core/src/git.rs:10-17` (`GitError`), `crates/devflow-core/src/workflow.rs:14-25` (`WorkflowError`)
**Apply to:** `ensure_devflow_dir` (reuse `WorkflowError`); any new error surface introduced in the split modules should reuse `CliError` from the thin `main.rs` rather than mint new module-local error types, per Pitfall 6.

### Module split shape (sibling files under one crate root, `mod`/`pub(crate)`, not a subdirectory)
**Source:** `crates/devflow-core/src/agents/mod.rs` + `claude.rs`/`codex.rs`/`opencode.rs`
**Apply to:** all of `preflight.rs`, `staleness.rs`, `pipeline*.rs`, `parallel.rs`, `commands.rs` — D-07 explicitly cites this file as the precedent that makes flat-sibling-under-`devflow-cli` (not a `commands/` subdirectory) consistent with codebase convention, note again that `devflow-cli` is a binary crate so `pub(crate)` replaces `agents/`'s `pub`/`pub mod`.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `crates/devflow-cli/src/test_support.rs` | test-support module (shared) | n/a | No existing `#[cfg(test)] pub(crate) mod` shared-across-files test-helper pattern exists anywhere in the workspace today. `gates.rs` and `config.rs` each declare their own **file-local, non-shared** `ENV_MUTEX`; this phase introduces the first cross-file shared instance. Planner should treat RESEARCH.md's Code Examples skeleton (`main.rs:4026-4034` derivation) as the primary reference since no in-repo precedent exists, supplemented by `config.rs`'s `EnvOverride` RAII idiom above. |
| `.claude/skills/ai-change-acceptance/` (19g) | config (skill doc) | n/a | DevFlow has no `.claude/skills/` or `.agents/skills/` directory today (confirmed via RESEARCH.md live `ls`). No in-repo `SKILL.md`/`rules/*.md` analog exists to copy from; must follow the global `gsd-code-reviewer` agent's documented project-skill discovery contract (`$HOME/.claude/gsd-core/references/project-skills-discovery.md`) as the format authority instead of an in-repo file. |
| `crates/devflow-cli/src/commands.rs` | module (CLI subcommand dispatch: status/doctor/logs/gate/list/recover) | request-response | No existing file in this repo implements a multi-subcommand dispatch/display module outside `main.rs` itself — `agents/mod.rs`'s `adapter_for` match is a partial shape analog (single dispatch fn) but far smaller in scope than the ~1,280-line `commands.rs` target; treat it as a size/structure precedent only, not a literal-code-to-copy source. |

## Metadata

**Analog search scope:** `crates/devflow-core/src/` (all modules, especially `agents/`, `git.rs`, `workflow.rs`, `gates.rs`, `config.rs`), `crates/devflow-cli/src/main.rs` (self-analog for pre-move code), `crates/devflow-cli/tests/` (integration test layout, not modified this phase), `crates/devflow-core/src/lib.rs` (module declaration list).
**Files scanned:** `agents/mod.rs`, `agents/claude.rs`, `git.rs`, `workflow.rs`, `gates.rs`, `config.rs`, `main.rs` (targeted ranges: imports 1-35, `CliError` def, `ENV_MUTEX` at 4020-4040), `lib.rs`.
**Pattern extraction date:** 2026-07-21

## Notes for the Planner

- **`config_parse.rs` (Claude's Discretion, D-05 open question):** if folded into the thin `main.rs` instead of split out, no separate pattern assignment is needed — `main.rs:27-31`'s existing `parse_gate_timeout` is a self-contained pure-function precedent for the shape either way (doc comment stating "no env access, pure, unit-testable" is the convention to preserve verbatim regardless of file placement).
- **Pipeline sub-split (D-06):** RESEARCH.md Pattern 1/Pitfall 1 (internal cycle across `launch_stage`/`advance`/`transition`/`handle_*_outcome`/`run_gate`) means whichever file layout is chosen, the `pub(crate)` surface between the sub-files will be non-trivial — the `agents/mod.rs` analog's clean one-directional `mod.rs → sub-file` shape does **not** directly transfer to a 3-way pipeline split, since the pipeline sub-files call each other bidirectionally. Plan this as `pub(crate)` cross-imports between siblings, not a parent/child relationship.
- **`preflight.rs` ↔ `pipeline*.rs` coupling (RESEARCH.md Pattern 2):** same caveat — the `agents/` analog has no cross-file production call dependency between adapters; `preflight`/`pipeline` do (`run_preflight` calls `launch_stage_inner` at `main.rs:861`; `launch_stage` calls `run_preflight` at `main.rs:1389`). No in-repo analog exists for this specific bidirectional-sibling-module shape; treat RESEARCH.md's own diagram/explanation as authoritative here rather than searching further for a closer analog — there isn't one in this codebase.
