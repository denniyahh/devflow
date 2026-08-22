# Phase 10: Logging + Planning Step

## Summary

Add structured logging infrastructure to devflow-core and introduce a
**Planning** step to the state machine. This is the first of three post-polish
phases that round out DevFlow's production readiness.

## Tasks

### 1. Logging Infrastructure

- [ ] Add `tracing` / `tracing-subscriber` as dependencies in `devflow-core`
- [ ] Configure log levels: `RUST_LOG` env var, default `info`
- [ ] Instrument key modules:
  - `git.rs` — branch create/delete, merge, rebase commands
  - `monitor.rs` — agent spawn/exit/pid polling
  - `workflow.rs` — state transitions and persistence
  - `ship.rs` — version bump, PR creation, confirm/reject
- [ ] Emit structured events for state machine transitions (`step_entered`, `step_exited`)
- [ ] Console output via stderr (stdout belongs to agent/system output)
- [ ] Optional JSON log output for machine consumption (e.g., Hermes watching)
- [ ] Thread-safe global subscriber set up once in `devflow-cli` main
- [ ] `devflow doctor` checks that RUST_LOG is parseable

### 2. Planning Step

The state machine currently has no concept of "plan before execute." Add a
`Planning` step that pauses the workflow so the human (or a planning agent) can
verify/edit phase plans before the coding agent runs.

- [ ] Add `Planning` variant to `Step` enum in `state.rs`
- [ ] Insert into the chain: `Idle → Branching → Planning → Executing → ...`
- [ ] `is_waiting()` returns `true` for `Planning` (blocks on human review)
- [ ] `is_skippable()` returns `true` for `Planning` (auto-advance when disabled)
- [ ] Add `auto_plan: true` config toggle (default `false` — requires explicit `devflow check`)
- [ ] When `auto_plan: false`: monitor pauses at `Planning`, user runs `devflow check` to proceed
- [ ] When `auto_plan: true`: auto-advance through `Planning` (current behavior preserved)
- [ ] `devflow status` shows "awaiting plan review" at this step
- [ ] Update all tests to account for the new step in the chain
- [ ] Update state serialization — old state files with `Planning` must deserialize correctly (new variant, serde aliases)

### 3. Documentation

- [ ] Update ARCHITECTURE.md state machine diagram
- [ ] Update CONTRIBUTING.md with logging conventions
- [ ] Add `.devflow.yaml` example with `auto_plan`

## Verification

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check
RUST_LOG=debug cargo run -- status 2>devflow.log
```

## Deliverables

- Logged state transitions and git operations
- `Planning` step in the workflow chain
- Full test coverage for new step + log configuration
