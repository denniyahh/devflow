# Testing Patterns

**Analysis Date:** 2026-07-22

## Test Framework

**Runner:** Rust's built-in `cargo test` harness. The repository does not use a separate test framework or assertion crate.

**Common commands:**

```bash
cargo test
cargo test --workspace
cargo test -p devflow project_root_walks_up_to_nearest_devflow_ancestor
cargo test --workspace -- --list
cargo test -- --nocapture
cargo test -- --test-threads=1
```

`devflow` is the package name. `devflow-cli` is a directory/crate description, not a valid package name for `cargo test -p`. The CLI is binary-only, so `cargo test -p devflow --lib` also does not select a usable target.

**CI:** `.github/workflows/ci.yml` runs these literal commands in separate jobs:

- `cargo test`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --check`

## Test File Organization

**Unit tests:**
- Each source module owns a bottom-of-file `#[cfg(test)] mod tests` block for the production functions it exercises.
- A module imports its own items with `use super::*;`.
- CLI modules import shared fixtures with `use crate::test_support::*;` where needed.
- `crates/devflow-cli/src/main.rs` retains one test for its own `project_root` helper; split behavior tests moved with their production modules.
- `crates/devflow-cli/src/test_support.rs` is declared as `#[cfg(test)] mod test_support;` so the binary's non-test build never sees test-only items.

**CLI integration tests:**
- `crates/devflow-cli/tests/build_provenance.rs`
- `crates/devflow-cli/tests/devcontainer_ci_failfast.rs`
- `crates/devflow-cli/tests/gitignore_coverage.rs`
- `crates/devflow-cli/tests/help_snapshot.rs`
- `crates/devflow-cli/tests/log_format_env.rs`
- `crates/devflow-cli/tests/phase7_cli.rs`
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` is the committed help snapshot.

**Core integration tests:**
- `crates/devflow-core/tests/devflow_dir_gitignore.rs`
- `crates/devflow-core/tests/monitor_e2e.rs`

Test functions use behavior-oriented snake-case names. A verified CLI unit-test example is `project_root_walks_up_to_nearest_devflow_ancestor` in `crates/devflow-cli/src/main.rs`.

## Test Structure

Tests use arrange, act, assert without a fixture framework:

1. Create a temporary directory with `tempfile::tempdir()`.
2. Initialize a real Git fixture when repository behavior matters.
3. Replace external agent CLIs with temporary executable scripts.
4. Invoke the production function or the compiled `devflow` binary.
5. Assert on the return value, exit status, stdout/stderr, emitted events, and on-disk state.

Asynchronous process and file effects use bounded polling helpers. Keep the assertion inside the polling window when the condition can change after an earlier wait; checking several eventually-consistent files only after one combined wait has caused real flakes.

## Mocking

There is no mocking framework. Tests replace only process boundaries that should not call a real external agent:

- Fake `claude`, `codex`, or `opencode` executables are written into a temporary directory.
- The temporary directory is prepended to `PATH` for the scoped test.
- Fake agents print controlled output and `DEVFLOW_RESULT` markers.

Do not mock Git or filesystem behavior when a temporary repository can exercise the real operation. The integration suite intentionally validates actual Git commands, worktrees, branches, commits, and capture files.

## Fixtures and Factories

`crates/devflow-cli/src/test_support.rs` is the shared CLI unit-test fixture module. It owns:

- `ENV_MUTEX`
- `init_repo` and `init_repo_no_version_file`
- `AlwaysFailAdapter` and `FailOnceAdapter`
- `agent_free_git_only_path_dir` and `agent_free_dir_with_agent_stub`
- `stub_agent_binary`
- `prepend_path`
- `stage_launched_count`

Integration-test helpers remain local to their integration-test binary because sibling files under `tests/` compile as separate crates.

## Environment Mutation Rule

Any CLI unit test that changes `PATH`, `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, or `DEVFLOW_GATE_NOTIFY_CMD` must hold `crate::test_support::ENV_MUTEX` for the complete save, mutate, exercise, and restore sequence. Do not declare a second mutex.

**D-04 invariant: every env var is guarded by exactly one mutex, and no var is touched under two.** This is a reviewer-enforced convention; no type or lint checks it mechanically.

`crates/devflow-core/src/gates.rs` and `crates/devflow-core/src/config.rs` each retain their own `ENV_MUTEX`. That is safe only because core and CLI tests compile into different test binaries, so their process-global environments cannot race across the crate boundary. Inside one test binary, a new mutex would violate the invariant.

Prefer a pure parse/read split: test parsing by passing raw values directly, and keep process-global environment access in a small wrapper. The tests in `crates/devflow-cli/src/config_parse.rs` follow this pattern and therefore do not need to mutate environment.

## Coverage

CI does not enforce a line-coverage percentage. Coverage expectations are behavioral:

- Lock the intended contract with a test that fails for the right reason before the implementation change.
- Exercise the real production boundary rather than a copied helper or weaker surrogate.
- Inspect whether the test would still pass if the changed behavior were absent.
- Keep the suite's full target and test-name inventory stable during pure-move refactors.

The project acceptance contract is `.claude/skills/ai-change-acceptance/`, especially `rules/change-acceptance.md` and `rules/test-signal-rejection.md`.

## Test Types

**Unit tests:** Pure parsing, policies, state transitions, path resolution, rendering, and error classification. They live beside the owning production module.

**Integration tests:** Full binary invocation, build provenance, CI source contracts, `.devflow` hygiene, logging formats, monitor lifecycle, and multi-step CLI workflows. They use real Git and temporary repositories while replacing agent CLIs.

**Regression guards:**
- `help_snapshot.rs` protects the CLI surface recorded in `snapshots/devflow-help.txt`.
- `devcontainer_ci_failfast.rs` checks shell/CI command ordering and source-level contract hooks.
- `gitignore_coverage.rs` checks `.devflow` construction coverage.
- `build_provenance.rs` rebuilds nested fixtures to validate embedded provenance.

There is no live-agent end-to-end test in the automated suite. Agent boundaries are exercised with deterministic fake executables.

## False-Green Traps

1. `cargo test --exact` with a bare function name can match zero tests and still exit successfully. Always inspect the harness output and require the intended target to report `1 passed`; exit status alone is insufficient.
2. The package is `devflow`, not `devflow-cli`. A copied `cargo test -p devflow-cli ...` command does not validate this crate.
3. `cargo test -p devflow --lib` is invalid for this binary-only package. Use `cargo test -p devflow <filter>` for its unit-test binary.
4. For a pure-move proof, a green suite alone is insufficient. Diff the committed test-name inventory and compare per-target pass counts so dropped, renamed, or re-nested tests cannot disappear silently.

## Common Patterns

**Temporary resources:** Use RAII types such as `tempfile::TempDir` so repositories, scripts, and captures are removed on drop.

**Process assertions:** Include stderr in failure messages, and assert both exit status and the specific observable contract.

**State assertions:** Inspect persisted state, events, gates, captures, branches, and commit history rather than relying only on printed success text.

**Polling:** Bound every wait and emit the missing path or PID in the timeout message.

**Git fixtures:** Disable commit and tag signing, set a deterministic identity, and use real branches and commits.

## Test Dependencies

- `tempfile = "3"` provides temporary directories and files.
- Rust stable is selected by `rust-toolchain.toml`.
- Git and `/bin/sh` are required by integration fixtures.
- No additional assertion or mocking library is used.

---

*Testing analysis: 2026-07-22*
