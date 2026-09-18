---
last_mapped_commit: d581384145e82fd8f7091fee6387d70c6a62fff5
last_mapped_at: 2026-09-18
---
# Coding Conventions

**Analysis Date:** 2026-09-18

Rust 2024 edition workspace (`Cargo.toml`), toolchain pinned exactly to `1.97.1` in `rust-toolchain.toml` (never a floating channel). Two crates: `crates/devflow-core` (lib, package `devflow-core`) and `crates/devflow-cli` (binary-only, package `devflow`).

## Naming Patterns

**Files:**

- `snake_case.rs`, one module per concern, flat under `src/` (e.g. `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/ship_evidence.rs`).
- Sub-module directories use `mod.rs` (`crates/devflow-core/src/agents/mod.rs` plus one file per agent: `claude.rs`, `codex.rs`, `opencode.rs`, `pi.rs`, `hermes.rs`, `antigravity.rs`).
- CLI modules are split by pipeline role with a `pipeline_` prefix: `crates/devflow-cli/src/pipeline_launch.rs`, `pipeline_outcomes.rs`, `pipeline_gate.rs`.
- Integration tests: descriptive snake_case, `_e2e` suffix for real-process end-to-end tests (`crates/devflow-cli/tests/stop_e2e.rs`, `crates/devflow-core/tests/monitor_e2e.rs`).

**Functions:** `snake_case`, verb-first and descriptive (`evaluate_agent_result`, `discover_stray_devflow_processes`, `wait_for_agent_pid`). Path helpers are named `<thing>_path` (`agent_pid_path`, `exit_code_path`, `stdout_path` in `agent_result.rs`).

**Variables:** `snake_case`; constants `SCREAMING_SNAKE_CASE` with a doc comment explaining the value (`EXEC_VISIBILITY_WAIT`, `EXEC_VISIBILITY_POLL` in `crates/devflow-core/src/test_support.rs`). Do not use magic numbers — name the duration/limit.

**Types:** `PascalCase`. Domain newtypes over primitives (`PhaseId` in `crates/devflow-core/src/phase_id.rs`, with `.padded()` for `feature/phase-NN` names). Enums for closed sets: `Stage`, `Mode`, `AgentKind`, `AgentStatus`. Error enums are `<Module>Error` (`GitError`, `LockError`, `WorkflowError`, `RegistryError`, `ShipError`, `HookError`, `GsdConfigError`, `VersionError`, `RecoverError`).

## Code Style

**Formatting:**

- `rustfmt` defaults (no `rustfmt.toml`). Enforced by `cargo fmt --check` in `scripts/check.sh fmt`.

**Linting:**

- `cargo clippy --workspace --all-targets -- -D warnings` (`scripts/check.sh clippy`) — test code is linted too.
- Workspace lints (`Cargo.toml` `[workspace.lints.clippy]`): `dbg_macro`, `todo`, `unimplemented` = warn (so fatal under `-D warnings`). Both crates opt in with `[lints] workspace = true`.
- `clippy.toml` disallows `std::env::set_var` / `std::env::remove_var`. Scope env to the child process with `Command::env` / `env_remove` / `env_clear`. A genuine test-only exception must carry `#[expect(clippy::disallowed_methods, reason = "...")]` (not `allow`) on the smallest enclosing test item — see `crates/devflow-core/src/monitor.rs:3404`.
- Dependency hygiene: `cargo deny check` (`deny.toml`) and `cargo machete`, via `scripts/check.sh deps` (not in `all`; tools installed by `scripts/install-dep-tools.sh`).

**Shell scripts (`scripts/*.sh`, `scripts/hooks/*`):** `#!/usr/bin/env bash`, `set -euo pipefail`, a header comment explaining why the script exists, `usage()` to stderr with exit 2, and missing tools are hard errors, never skips (`scripts/check.sh`).

## Import Organization

**Order (as rustfmt sorts within the block):**

1. External crates (`clap`, `serde`, `thiserror`, `tracing`)
2. Workspace crate (`devflow_core::...`) / `crate::...`
3. `std::...`

Imports are explicit item paths, one `use` per path, grouped with braces when from one module (`use devflow_core::agent_result::{AgentStatus, agent_pid_path, ...}`). No glob imports in production code.

**Path Aliases:** None. The CLI consumes core via `devflow-core.workspace = true`.

## Error Handling

**Patterns:**

- Core: every fallible module defines its own `thiserror` enum; I/O and JSON wrapped with messages like `#[error("registry I/O failed: {0}")]` (`crates/devflow-core/src/registry.rs:37`). Parse errors for CLI values carry the valid options: `#[error("unsupported stage `{0}`; expected define, plan, code, validate, or ship")]` (`stage.rs:101`).
- CLI: a single `pub(crate) enum CliError` in `crates/devflow-cli/src/main.rs:489` composing core errors via `#[error(transparent)] X(#[from] ...)` plus `Message(String)`. `main()` calls `run() -> Result<(), CliError>` and on error prints `error: {err}` to stderr and exits 1.
- No `anyhow`, no `Box<dyn Error>` in signatures. Propagate with `?`; `unwrap()`/`expect()` are for tests (the high counts in `version.rs`, `monitor.rs`, `state.rs` sit in `mod tests`).
- Never fail silently: a skipped or degraded check must print something distinguishable from a pass (principle stated repeatedly, e.g. `scripts/check.sh` `run_deps`, the "never-silent" commit checks in `agent_result.rs`).

## Logging

**Framework:** `tracing` (core) + `tracing-subscriber` with `json` and `env-filter` features (CLI only).

**Patterns:**

- All logs go to **stderr**; stdout is reserved for agent output and machine-readable results (`crates/devflow-core/src/lib.rs` module docs).
- `RUST_LOG` controls level (default `info`); `DEVFLOW_LOG_FORMAT=json` switches to JSON lines (`main.rs` `main`).
- Levels: `error` fatal, `warn` recoverable/forced, `info` state transitions and git ops, `debug` command/file I/O. Import the macros by name: `use tracing::{debug, info, warn};`.
- Structured events carry named fields (`step_entered` / `step_exited` with `phase`).
- User-facing CLI output uses `println!`/`eprintln!` directly in `crates/devflow-cli/src` (library code does not print, except where deliberately doubling a warning to stdout, `monitor.rs:391`).

## Comments

**When to Comment:** Heavily, and about *why*. Comments cite provenance: plan IDs (`48-09`), decision IDs (`D-14`), backlog items (`999.37`), and incidents with dates. Load-bearing config lines get a comment explaining what breaks without them (`serde_json` `preserve_order` in `Cargo.toml`, `INSTA_UPDATE=no` in `scripts/check.sh`). Follow this: when adding a non-obvious guard, state the failure it prevents and its source ID.

**Doc comments:** `//!` module docs at the top of every non-trivial module and test file; `///` on public items and on tests whose intent is not obvious from the name. Reference other code by backticked path/symbol.

## Function Design

**Size:** No enforced limit; several modules are very large (`agent_result.rs` 9134 lines, `commands.rs` 7458, `pipeline_outcomes.rs` 5546; the inline `mod tests` starts at line 3499 and 3984 respectively, so roughly half or more is tests). Prefer small helpers and reuse existing idioms rather than copying them (e.g. `crate::agent::argv_basename` is reused by `test_support`).

**Parameters:** Pass domain types (`PhaseId`, `Stage`, `AgentKind`, `&Path`) not raw strings. Launch configuration bundles into structs (`MonitorLaunch`).

**Return Values:** `Result<T, ModuleError>`; structured types from core, formatting left to the CLI ("core returns structured types; frontends format output", `lib.rs`).

## Module Design

**Exports:** `crates/devflow-core/src/lib.rs` declares `pub mod` for each module; no re-export barrel. Test-only modules are gated: `#[cfg(test)] mod doc_check;` and `test_support` behind `#[cfg(any(test, feature = "test-support"))]`.

**CLI:** binary-only crate; modules declared in `main.rs` with `mod x; use x::{...};`. Test helpers are `#[cfg(test)] mod test_support;` on the `mod` item (not `#![cfg(test)]` inside), to avoid `dead_code` under `-D warnings`.

**Barrel Files:** Not used.

## Commits

Conventional Commits enforced by `scripts/hooks/commit-msg` (types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert, release, merge, sync; `!` for breaking; no trailing period). Hooks are installed via `git config core.hooksPath scripts/hooks`; `pre-commit` chains to any previously active hook; `pre-push` runs `scripts/check-in-container.sh all` (or `scripts/check.sh all` on its non-container path, `scripts/hooks/pre-push:217-220`).

---

*Convention analysis: 2026-09-18*
