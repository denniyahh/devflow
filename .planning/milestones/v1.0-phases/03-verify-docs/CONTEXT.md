# Phase 3: Verify & Docs Execution

## Goal
The state machine advances through VERIFYING and DOCSING steps, but **nothing actually runs**. Commands are configured in `.devflow.yaml` but never executed. Phases complete with false confidence.

## Tasks

### 3a — Verify command execution
- [ ] Add `verify` subcommand to CLI: `devflow verify`
- [ ] Reads `automation.verify_command` from `.devflow.yaml` (e.g., `cargo test`)
- [ ] Runs the command, captures output, returns exit code
- [ ] If exit code != 0 and `continue_on_error` is false, block advancement
- [ ] File: `crates/devflow-cli/src/main.rs` (new command)
- [ ] File: `crates/devflow-core/src/verify.rs` (execution logic)

### 3b — Lint command execution
- [ ] Add `lint` subcommand: `devflow lint`
- [ ] Reads `automation.lint_command` from `.devflow.yaml` (e.g., `cargo clippy -- -D warnings`)
- [ ] Runs the command, captures output
- [ ] File: `crates/devflow-core/src/verify.rs` (shared logic)

### 3c — Docs command execution
- [ ] Add `docs` subcommand: `devflow docs`
- [ ] Reads `automation.docs_command` from `.devflow.yaml`
- [ ] Runs the command
- [ ] If `automation.docs_auto_commit` is true, auto-commit doc changes
- [ ] File: same as above

### 3d — Integration with state machine
- [ ] `devflow check` now calls verify → lint → docs when advancing through VERIFYING/DOCSING
- [ ] Respects `automation.auto_verify` flag (skip if false)
- [ ] Respects `automation.continue_on_error` (advance even on failure if true)
- [ ] File: `crates/devflow-core/src/workflow.rs`

## Verification
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check

# Manual: run devflow verify in devflow project itself
devflow verify  # should run "cargo test"
devflow lint    # should run "cargo clippy -- -D warnings"
```

## Success
When a phase advances through VERIFYING, tests and lints actually run. A failing test blocks advancement (unless continue_on_error is set).
