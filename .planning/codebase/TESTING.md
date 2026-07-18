# Testing Patterns

**Analysis Date:** 2026-07-17

## Test Framework

**Runner:**
- Built-in `cargo test` (Rust's native test harness, no external framework)
- No Pytest/Jest/Vitest/etc. — pure Rust testing infrastructure
- Config: implicitly built-in; see `Cargo.toml` dev-dependencies

**Assertion Library:**
- Rust's built-in `assert!()` / `assert_eq!()` / `assert_ne!()` macros
- No external assertion crate; plain comparisons and panic on failure

**Run Commands:**
```bash
cargo test                           # Run all tests (unit + integration)
cargo test --lib                     # Run library unit tests only
cargo test --test '*'                # Run integration tests only
cargo test phase7_cli                # Run specific test (e.g., phase7_cli.rs)
cargo test -- --nocapture            # Show stdout/stderr from tests (default: suppressed)
cargo test -- --test-threads=1       # Run serially (default: parallel)
```

**CI:** `.github/workflows/ci.yml` runs:
- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo fmt --check`

All must pass before PR merge.

## Test File Organization

**Location:**
- **Unit tests:** Inline in source files via `#[cfg(test)] mod tests { ... }`
- **Integration tests:** Separate `.rs` files in `crates/*/tests/` directory
- Example: `crates/devflow-core/src/config.rs` has inline unit test; `crates/devflow-core/tests/monitor_e2e.rs` is integration test

**Naming:**
- Integration test file names describe the scenario: `phase7_cli.rs`, `help_snapshot.rs`, `log_format_env.rs`, `monitor_e2e.rs`
- Inline test modules named `tests`: `#[cfg(test)] mod tests { #[test] fn ... }`
- Test function names describe behavior: `devflow_ignores_stray_devflow_yaml()`, `parallel_creates_two_worktrees_and_spawns_two_monitors()`

**Structure:**
```
crates/
├── devflow-core/
│   ├── src/
│   │   ├── config.rs          ← Unit tests inline: #[cfg(test)] mod tests
│   │   ├── agent.rs           ← Unit tests inline: #[cfg(test)] mod tests
│   │   ├── lib.rs             ← No tests
│   └── tests/
│       ├── monitor_e2e.rs     ← Integration test (end-to-end monitor)
└── devflow-cli/
    ├── src/
    │   └── main.rs            ← No inline tests
    └── tests/
        ├── phase7_cli.rs      ← Integration test (CLI workflow)
        ├── help_snapshot.rs   ← Snapshot guard (--help output)
        ├── log_format_env.rs  ← Regression guards (logging behavior)
        ├── gitignore_coverage.rs ← Configuration test
        ├── devcontainer_ci_failfast.rs ← Infrastructure test
        └── snapshots/
            └── devflow-help.txt ← Committed snapshot
```

## Test Structure

**Suite Organization:**
```rust
// Helper functions at top (testable setup)
fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

fn init_repo(root: &Path) {
    // Git initialization, fixture setup
}

fn fake_bin_dir(scripts: &[(&str, &str)]) -> FakeBin {
    // Create mock executables
}

// Test cases below
#[test]
fn devflow_ignores_stray_devflow_yaml() {
    // Arrange
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    
    // Act
    let output = run_devflow(root, &fake_bin.path, &["doctor"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Assert
    assert!(stdout.contains(&format!("devflow v{}", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn parallel_creates_two_worktrees_and_spawns_two_monitors() {
    // Similar structure: Arrange → Act → Assert
}
```

**Patterns:**

1. **Setup via helper functions** (not fixtures framework — pure functions):
   - `init_repo(root)` sets up a minimal git repo with develop branch
   - `fake_bin_dir(scripts)` creates mock CLI commands (e.g., fake `claude`, `codex`)
   - Helpers are reusable across multiple test functions

2. **Temporary directories:**
   - Use `tempfile::tempdir()` for isolation (cleaned up automatically on drop)
   - Example: `let repo = tempfile::tempdir().unwrap(); let root = repo.path();`

3. **Subprocess invocation:**
   - Integration tests spawn actual `devflow` binary via `std::process::Command`
   - Example from `phase7_cli.rs`:
     ```rust
     let output = Command::new(devflow_bin())
         .args(args)
         .arg(root)
         .output()
         .expect("run devflow");
     ```
   - Assert on exit code, stdout, stderr: `assert!(output.status.success(), "...{}", stderr)`

4. **Polling/waiting:**
   - Tests poll for file creation (state files, lock files, agent output)
   - Retry loop with timeout: `wait_for(path)` or `wait_for_pid(path)` helpers
   - Example from `phase7_cli.rs`:
     ```rust
     fn wait_for(path: &Path) {
         for _ in 0..200 {
             if path.exists() { return; }
             std::thread::sleep(Duration::from_millis(25));
         }
         panic!("timed out waiting for {}", path.display());
     }
     ```

5. **Environment isolation:**
   - Tests clean env vars before spawning: `cmd.env_remove("RUST_LOG");`
   - Example from `log_format_env.rs` — ensures test env doesn't leak into subprocess
   - Tests set specific env for behavior verification: `cmd.env("RUST_LOG", "debug");`

## Mocking

**Framework:** Manual mock creation via shell scripts (not a mocking framework)

**Patterns:**
- Fake executables written as shell scripts in temporary directory
- Prepended to PATH when running devflow
- Example from `phase7_cli.rs`:
  ```rust
  let fake_bin = fake_bin_dir(&[
      ("claude", "#!/bin/sh\nprintf 'fake claude\\nDEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n"),
      ("codex", "#!/bin/sh\nprintf 'fake codex\\nDEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n"),
  ]);
  
  let output = Command::new(devflow_bin())
      .args(args)
      .env("PATH", path_with_fake_bin(fake_bin))
      .output()
      .expect("run devflow");
  ```

**What to Mock:**
- External agent CLIs: `claude`, `codex`, `opencode` (replaced with shell scripts that print expected output)
- Git commands: usually NOT mocked — tests run against real git fixtures
- System time: NOT mocked; tests accept timestamps from `SystemTime::now()`

**What NOT to Mock:**
- Git operations: tests invoke real `git` on fixture repos
- File I/O: tests create real temp directories and files
- Process spawning: tests spawn real subprocesses to verify actual behavior

**Design Rationale:**
- Mocking minimal (only agent CLIs replaced) keeps tests closer to real production workflow
- Real git operations validate git-flow correctness against actual git behavior
- Integration tests drive full CLI → core library → git call chains

## Fixtures and Factories

**Test Data:**
- Git fixture repos created fresh per test via `init_repo()`
- Minimal initial state: develop branch + one commit + README.md
- Phase-specific context files pre-baked for phases that need them:
  ```rust
  for phase in ["07", "08", "09"] {
      let dir = root.join(format!(".planning/phases/{phase}-test"));
      fs::create_dir_all(&dir).unwrap();
      fs::write(dir.join(format!("{phase}-CONTEXT.md")), "ctx\n").unwrap();
  }
  ```

**Location:**
- Inline in test files as helper functions (no separate fixture directory)
- Reusable across multiple test functions: e.g., `init_repo()` called by every test

**Factories:**
- `FakeBin` struct wraps `tempfile::TempDir` to ensure cleanup on drop
- State factories via direct construction: `State { stage, phase, agent, mode, ... }`
- No builder pattern; direct struct initialization common in tests

**Example** from `monitor_e2e.rs`:
```rust
fn init_repo(root: &Path, phase: u32) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "devflow@example.com"]);
    git(root, &["config", "user.name", "DevFlow Tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    std::fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "base"]);
}
```

## Coverage

**Requirements:** Not enforced (no coverage gate in CI)

**Observation:**
- All public APIs exercised via integration tests (`phase7_cli.rs`, `monitor_e2e.rs`)
- Core logic (state machine, git operations, result parsing) covered by integration tests spawning full CLI
- Some unit tests inline in source files for pure logic: `config.rs`, `agent.rs`

## Test Types

**Unit Tests:**
- **Scope:** Pure functions, simple state transitions
- **Approach:** Inline in source files via `#[cfg(test)] mod tests`
- **Examples:**
  - `crates/devflow-core/src/config.rs`: `default_uses_hardcoded_constants()` — verifies constant values
  - `crates/devflow-core/src/agent.rs`: `agent_running_detects_self()`, `agent_running_rejects_corrupt_pid_values()` — test PID validation logic

**Integration Tests:**
- **Scope:** End-to-end workflows, CLI invocation, state persistence, git flow
- **Approach:** Separate files in `tests/` directory, spawn actual CLI binary
- **Examples:**
  - `phase7_cli.rs`: Full workflow (phase 7 → phase 9, parallel runs, worktree creation, agent spawning)
  - `monitor_e2e.rs`: Monitor lifecycle (spawns fake agent, captures output, evaluates result markers)
  - `help_snapshot.rs`: Output regression guard (commits snapshot of `--help` output)
  - `log_format_env.rs`: Logging behavior (RUST_LOG, DEVFLOW_LOG_FORMAT env var handling)

**Regression Guards:**
- `help_snapshot.rs`: Verifies `--help` output against committed snapshot
  - Fails if CLI interface changes without updating docs
  - Regenerate via: `cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt`
- `log_format_env.rs`: Regression guards for logging behavior (WR-01, CR-01, CR-02 per CONTRIBUTING.md)
  - Ensures RUST_LOG is honored in JSON format mode
  - Ensures logs go to stderr, not stdout

**E2E Tests:**
- Full workflow runs (not present in this repo's test suite)
- Tests drive the full Define → Plan → Code → Validate → Ship pipeline with real agents
- Instead, integration tests use fake agents (shell scripts) to validate state machine + git flow

## Common Patterns

**Async Testing:**
- Not applicable (Rust's default test harness is synchronous)
- Tests use `std::thread::sleep()` for polling, not async/await

**Error Testing:**
- Verify error paths by checking error return values
- Example from `git.rs`: `delete_branch()` returns `Err` if trying to delete protected branches (main/develop)
- Integration tests verify error output: `assert!(stderr.contains("error message"))`

**Subprocess Testing:**
- Most integration tests spawn subprocesses and examine exit codes + output
- Example:
  ```rust
  let output = Command::new(devflow_bin())
      .args(args)
      .output()
      .expect("spawn devflow");
  assert!(output.status.success(), "... stderr:\n{}", stderr);
  ```

**State Verification:**
- Tests verify on-disk state after operations (not just return values)
- Examples:
  - `phase7_cli.rs`: Verifies `.devflow/state-07.json` exists and has correct phase number
  - `monitor_e2e.rs`: Waits for capture files (stdout, exit code) written by monitor
  - `log_format_env.rs`: Verifies log lines appear on stderr, not stdout

**Timing/Polling:**
- Tests use bounded retries (typically 200 iterations, 25ms sleep) to wait for async I/O
- Fail with clear timeout message if condition not met
- Example: `wait_for_pid()` retries 200 times before panicking with "timed out waiting for a pid"

**Git Fixture Testing:**
- Tests invoke real `git` commands on temporary repositories
- Helpers (`git()`, `git_stdout()`) execute git and assert success
- Tests verify actual git state (branch names, commit counts, merge history)
- Example from `monitor_e2e.rs`:
  ```rust
  fn git(root: &Path, args: &[&str]) {
      let output = Command::new("git")
          .args(args)
          .current_dir(root)
          .output()
          .expect("spawn git");
      assert!(output.status.success(), "git {args:?} failed: ...");
  }
  ```

## Test Dependencies

**Dev-only crates:**
- `tempfile = "3"` — temporary directory/file creation for test isolation
- That's it — no test framework, assertion library, or mocking crate

**Environment:**
- Rust stable toolchain (pinned in `rust-toolchain.toml`)
- Git installed and accessible via PATH (tests invoke real `git` CLI)
- Shell (`/bin/sh`) accessible for fake executables

---

*Testing analysis: 2026-07-17*
