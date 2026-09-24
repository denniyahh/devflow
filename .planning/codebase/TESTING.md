---
last_mapped_commit: d581384145e82fd8f7091fee6387d70c6a62fff5
last_mapped_at: 2026-09-18
---
# Testing Patterns

**Analysis Date:** 2026-09-18

## Test Framework

**Runner:**

- Built-in `cargo test` (libtest), toolchain `1.97.1` (`rust-toolchain.toml`).
- Config: none beyond `Cargo.toml` dev-dependencies and `clippy.toml`.

**Assertion Library:**

- `assert!` / `assert_eq!` with explanatory messages.
- `insta` (workspace dev-dependency) for prompt-text snapshots.
- Dev-dependencies: `tempfile = "3"`, `insta`, and `devflow-core` with `features = ["test-support"]` (both crates). No `proptest`, `mockall`, `rstest`, or `serial_test`.

**Run Commands:**

```bash
cargo test -p devflow-core --lib          # core unit tests
cargo test -p devflow --bin devflow       # CLI unit tests (binary-only; `--lib` fails)
scripts/check.sh test                     # env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test --workspace --no-fail-fast
scripts/check.sh all                      # fmt + clippy (--all-targets, -D warnings) + test
scripts/check-in-container.sh all         # same, inside the pinned CI image
```

Always run through `scripts/check.sh` before declaring green: bare `cargo test` does not pin `INSTA_UPDATE=no`, so an exported `INSTA_UPDATE=always`/`INSTA_FORCE_UPDATE=1` silently re-blesses drifted snapshots.

When filtering with `--exact`, module-qualify the name (`prompt::tests::x`) and check the output says `1 passed` — a filter that matches nothing still exits 0.

**Coverage:** No coverage tool is wired into `scripts/check.sh` or the repo.

## Test File Organization

**Location:**

- Unit tests inline as `#[cfg(test)] mod tests { ... }` at the bottom of each source file (e.g. `crates/devflow-core/src/agent_result.rs:3499`, `crates/devflow-cli/src/commands.rs:3984`).
- Integration tests in `crates/<crate>/tests/*.rs`, one binary per file.
- Fixtures in `crates/<crate>/tests/fixtures/<topic>/`; snapshots in `src/snapshots/` (insta) and `tests/snapshots/` (plain text).

**Naming:**

- Test fns are long descriptive sentences stating the property: `claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it`, `help_output_matches_committed_snapshot`.
- `_e2e.rs` suffix for tests that spawn real processes/binaries.

**Structure:**

```
crates/devflow-core/
  src/*.rs                 # inline mod tests
  src/snapshots/*.snap     # insta baselines (prompt.rs)
  src/test_support.rs      # shared hermetic helpers (feature-gated)
  tests/{monitor_e2e,devflow_dir_gitignore,decimal_phase_paths,agent_kind_antigravity}.rs
  tests/fixtures/opencode/
crates/devflow-cli/
  src/*.rs                 # inline mod tests
  src/snapshots/*.snap     # insta baselines (pipeline_launch.rs)
  src/test_support.rs      # #[cfg(test)] shared fixtures, ENV_MUTEX
  tests/*.rs               # 22 files: e2e, guards on scripts/CI/hooks, help snapshot
  tests/fixtures/{ci-parity,plan-bashisms}/
  tests/snapshots/devflow-help.txt
```

Scale: ~1327 `#[test]` functions across the workspace; heaviest in `agent_result.rs` (200), `commands.rs` (115), `pipeline_outcomes.rs` (73).

## Test Structure

**Suite Organization:**

```rust
/// Doc comment stating the decision/incident the test guards (IDs like D-15, 999.37).
#[test]
fn claude_style_full_execute_fix_prompt_snapshot() {
    let prompt = render_claude_style(&StageIntent::Code {
        phase: PhaseId::new(47),
        fix: Some(FixType::FullExecute),
    });
    insta::assert_snapshot!(prompt);
}
```

(`crates/devflow-core/src/prompt.rs:1360`)

**Patterns:**

- Setup: build a throwaway repo in `tempfile::tempdir()` / `TempDir::new()`; auto-cleanup on drop.
- Exhaustiveness: iterate enum variants through an exhaustive `match` so a new `AgentKind` is a compile error until tested (`policy_carrying_full_execute_fix_prompt_snapshots`, `prompt.rs`).
- Assertions carry context: `assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr))`.
- Guards and scanners have fixture pairs: a legit sample that must pass and bypass samples that must trip (`tests/fixtures/plan-bashisms/legit-PLAN.md` vs `*-bypass-PLAN.md`; `tests/fixtures/ci-parity/valid.yml` vs `decoy-job.yml`, `commented-out.yml`). Keep a known-positive and known-negative for every new guard.

## Mocking

**Framework:** None. Use real processes and real git with fake executables.

**Patterns:**

```rust
// fake agent binary written into a tempdir and put on the child's PATH
let script = format!(
    "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{args}'\nprintf '%s' '{body}'\nexit {exit_code}\n",
);
```

(`crates/devflow-core/src/agents/opencode.rs:381`; similar in `pi.rs:250`, `antigravity.rs:170`, `crates/devflow-cli/src/test_support.rs:482`)

Hermetic git — always go through the helper, never bare `Command::new("git")` in fixtures:

```rust
let output = devflow_core::test_support::git_command(root)
    .args(args)
    .output()
    .expect("spawn git");
```

(`crates/devflow-core/tests/monitor_e2e.rs`). `git_command` scrubs `GIT_DIR` and related repo-local vars; fixture repos set `user.email`, `user.name`, `commit.gpgsign false`, `core.hooksPath /dev/null`.

Seam injection: where a process call must be faked in-process, pass a closure (`type OutputFn = Box<dyn FnOnce() -> std::io::Result<std::process::Output>>`, `opencode.rs:789`).

**What to Mock:** External agent CLIs (claude, codex, opencode, pi, hermes, antigravity) via `#!/bin/sh` fakes; timeouts via env knobs (`DEVFLOW_GATE_TIMEOUT_SECS` etc.).

**What NOT to Mock:** git, the filesystem, process spawning, `/proc` inspection. Integration tests invoke the real binary via `env!("CARGO_BIN_EXE_devflow")` (`crates/devflow-cli/tests/help_snapshot.rs`).

## Environment Isolation

- `std::env::set_var` / `remove_var` are disallowed by `clippy.toml`. Configure the spawned child with `Command::env` / `env_remove` / `env_clear`.
- `PATH` is supplied only on a child `Command` through `devflow_core::test_support::run_test_in_child`; it is never installed in the parent test process, and its replacement is always an existing directory, not a removed value. Every child result is checked with `assert_child_ran_exactly_one_passing_test` using the module-qualified test name, so an exact filter that runs zero tests cannot look green (Phase 48, 48-03..48-06, recorded by 48-17).
- The remaining process-global test mutations are deliberately deferred: `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, `DEVFLOW_GATE_NOTIFY_CMD` and `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` (the CLI `ENV_MUTEX` doc lists them). Each holds the mutex for the complete save, mutate, exercise and restore sequence.
- Remaining exceptions hold an `ENV_MUTEX` and carry `#[expect(clippy::disallowed_methods, reason = "...")]` (`crates/devflow-core/src/monitor.rs:3404`, `crates/devflow-core/src/gates.rs:381`). The CLI's shared `ENV_MUTEX` is in `crates/devflow-cli/src/test_support.rs`; its doc lists which vars it guards — add a new var there, not in a new mutex (D-04: every remaining process-global env var is guarded by exactly one mutex, and no var is touched under two).
- `/proc` race: after spawning a child, call `test_support::wait_for_exec_visibility` before asserting on a `/proc/<pid>/cmdline` census; `agent_running` (kill 0) is not a barrier (`crates/devflow-core/src/test_support.rs`).
- `crates/devflow-cli/tests/git_env_hermeticity.rs` fails fast if the suite runs with `GIT_DIR`-family vars set.

## Fixtures and Factories

**Test Data:**

```rust
fn init_repo(root: &Path, phase: PhaseId) {
    git(root, &["init", "-q"]);
    // ...identity, gpgsign off, hooksPath /dev/null...
    git(root, &["checkout", "-q", "-b", "develop"]);
    // base commit, then feature/phase-NN with one commit
    let branch = format!("feature/phase-{padded}", padded = phase.padded());
}
```

(`crates/devflow-core/tests/monitor_e2e.rs`)

**Location:** `crates/devflow-core/src/test_support.rs` (cross-crate via `test-support` feature), `crates/devflow-cli/src/test_support.rs` (CLI-internal), file fixtures under `tests/fixtures/`.

## Snapshot Tests

- `insta` baselines: `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__*.snap` (one per agent adapter, deliberately duplicated — do not merge) and `crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__*.snap`. A mismatch is a wording change to review; never re-bless to make a failure go away.
- CLI `--help` is a plain-text snapshot (`crates/devflow-cli/tests/snapshots/devflow-help.txt`); on change, update docs then regenerate with `cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt`.

## Test Types

**Unit Tests:** inline `mod tests`, pure logic plus tempdir-backed state/git.

**Integration Tests:** `crates/*/tests/*.rs` — real `devflow` binary, real monitor with a fake agent (`monitor_e2e.rs`), and repo-policy guards over scripts/CI/hooks (`ci_parity_guards.rs`, `pre_push_signing_policy.rs`, `pre_commit_branch_guard.rs`, `workspace_version_pin.rs`, `gitignore_coverage.rs`, `plan_bashism_scanner.rs`, `worktree_guard_harness.rs`).

**E2E Tests:** `*_e2e.rs` (`gate_sweep_e2e.rs`, `stop_e2e.rs`, `reap_strays_e2e.rs`, `start_reachability_e2e.rs`, `auto_chain_*_e2e.rs`). No live-agent tests in the suite.

**Ignored tests:** No `#[ignore]` attributes on tests in the tree (only mentioned in comments).

## Common Patterns

**Async Testing:** None — no async runtime. Waits are bounded polls with named constants (`EXEC_VISIBILITY_WAIT` 10s / `EXEC_VISIBILITY_POLL` 2ms) that fail loudly rather than hang.

**Error Testing:**

```rust
assert!(matches!(
    load_state(root, PhaseId::new(7)),
    Err(WorkflowError::MissingState(_))
));
```

(`crates/devflow-core/tests/monitor_e2e.rs:121`)
Match the specific error variant or assert on the intended failure text — not merely on a non-zero exit code.

---

*Testing analysis: 2026-09-18*
