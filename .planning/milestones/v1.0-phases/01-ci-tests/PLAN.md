# Phase 1: CI Foundation + Test Coverage + Critical Fixes

> Parent: [ROADMAP.md](./ROADMAP.md) | Status: Complete (2026-06-17)

## Goal

Establish CI pipeline, raise test coverage from 5% to >60%, and fix the 3 critical issues identified in the codebase audit.

## Context

- **Codebase:** 1,596 lines Rust across 11 source files, 2 crates
- **Current tests:** 2 (`version::bumps_semver_components`, `config::parses_devflow_yaml_shape`)
- **Target coverage:** >60% (unit + integration)
- **No CI exists:** Must build from scratch

## Tasks

### 1a — GitHub Actions CI

- [ ] Create `.github/workflows/ci.yml`
  - Job: `test` — `cargo test` on ubuntu-latest
  - Job: `lint` — `cargo clippy -- -D warnings`
  - Job: `fmt` — `cargo fmt --check`
  - Triggers: push to develop/main, PR to develop/main
  - Concurrency: cancel-in-progress
- [ ] Verify CI passes on current `develop` (commit `4f2b849`)
  - `<automated>` Push to develop, wait for CI run, confirm all green

### 1b — Fix Critical Issue: `unwrap()` in Library Code

- [ ] Replace `unwrap()` in `crates/devflow-core/src/lock.rs:31`
  - Current: `fs::create_dir_all(path.parent().unwrap())?;`
  - Fix: Return `LockError` if path has no parent
  - `<automated>` `cargo clippy` should catch any remaining unwraps

### 1c — Update Stale Documentation

- [ ] Update `AGENTS.md` — "What's Already Done" section
  - Fix: "empty `src/main.rs`" → "318 lines, fully implemented"
  - Fix: "send-keys launch command" → "command as tmux main process"
  - Fix: "No monitor daemon yet" → remove (implemented)
- [ ] Update `ROADMAP.md` — mark completed v0.2.0/v0.3.0 items
  - Mark monitor, recover, lock, SIGTERM as ✅
  - Remove stale Known Limitations entries

### 1d — Unit Tests

- [ ] `state.rs` tests (`tests/unit/state.rs`)
  - Step::advance() through all states
  - State::advance_skipping() with config
  - Agent::launch_command() output format
  - State serialization round-trip (serde)
  - `<automated>` `cargo test test_state`

- [ ] `config.rs` tests (`tests/unit/config.rs`)
  - All fields parse correctly from YAML
  - Default values when fields missing
  - Missing file → error
  - Invalid YAML → error
  - `should_skip()` for each automation toggle
  - `<automated>` `cargo test test_config`

- [ ] `lock.rs` tests (`tests/unit/lock.rs`)
  - Acquire creates lock file
  - Release deletes lock file
  - Concurrent acquire on same project fails
  - Stale lock detection
  - `<automated>` `cargo test test_lock`

- [ ] `version.rs` tests (`tests/unit/version.rs`)
  - bump() major/minor/patch edge cases
  - build_number() git count
  - read_version() missing file
  - write_version() round-trip
  - `<automated>` `cargo test test_version`

- [ ] `workflow.rs` tests (`tests/unit/workflow.rs`)
  - save_state() / load_state() round-trip
  - clear_state() removes file
  - advance_state() respects config skips
  - `<automated>` `cargo test test_workflow`

### 1e — Integration Tests

- [ ] `git.rs` integration tests (`tests/integration/git.rs`)
  - Create temp git repo, init develop branch
  - feature_start(N) creates feature/phase-NN
  - feature_finish(N) merges and deletes
  - release_start/finish with version
  - cleanup_merged() behavior
  - `<automated>` `cargo test test_git -- --ignored` (requires git)

- [x] ~~`tmux.rs` integration tests~~ — **OBSOLETE.** tmux was removed in
  commit `14fa144` (replaced with non-interactive process spawning). Agent
  launch is now covered by `agent.rs` unit tests (`agent_running`,
  `agent_label`).

### 1f — Verify CI is Green

- [ ] Run full suite locally: `cargo test && cargo clippy && cargo fmt --check`
- [ ] Push to develop, confirm CI run passes all jobs
- [ ] Target: >60% coverage (`cargo tarpaulin` or manual count)

## Success Criteria

- [x] `.github/workflows/ci.yml` exists and passes
- [x] `cargo clippy` passes with `-D warnings` (catches remaining unwraps)
- [x] `cargo fmt --check` passes
- [x] `cargo test` passes with >15 new tests (72 tests total)
- [x] No `unwrap()` in library code (`lock.rs` only violation)
- [x] AGENTS.md and ROADMAP.md reflect current state
- [x] Test coverage >60% (**78.66% lines / 80.73% regions** via `cargo llvm-cov`)

## Completion Notes (2026-06-17)

The `14fa144` tmux→process-spawning refactor reintroduced **2 clippy errors**
(`recover.rs`: `map_or`→`is_some_and`, collapsible `if let`) and **fmt drift**
across 5 files, leaving CI red. Fixed both, then added 11 tests for the two
weakest modules:
- `recover.rs`: 0% → 82% (stale-state detection, age formatting, `inspect`)
- `agent.rs`: process-existence + label helpers (`agent_running`, `agent_label`)

Remaining 0%-coverage modules are `main.rs` (CLI entry, integration-level) and
`monitor.rs` (forking daemon) — both impractical for unit tests and out of
scope for the >60% target, which is exceeded library-wide.

## Verification

```bash
# Local verification (pre-push)
cargo test
cargo clippy -- -D warnings
cargo fmt --check

# CI verification
gh run watch  # after push
```
