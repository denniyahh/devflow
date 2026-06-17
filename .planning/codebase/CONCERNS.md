# DevFlow — Concerns

> Generated: 2026-06-17 | Mapper: gsd-map-codebase (sequential)

## Critical

### 1. No Test Coverage (5% — 2 tests for ~30 functions)
- State machine transitions are untested — a regression in `advance()` or `advance_skipping()` would silently corrupt workflow state
- `git.rs` branch operations are untested — could delete wrong branches or fail silently
- `monitor.rs` spawn logic untested — the recent deadlock bug (persistent `sh` shell) went unnoticed because no test covers agent completion detection
- **Risk**: Every change is a blind change. Dogfooding catches some issues but not edge cases.

### 2. No CI Pipeline
- No `.github/workflows/` directory exists
- No automated `cargo test`, `cargo clippy`, or `cargo fmt --check`
- All verification is manual
- **Risk**: Regressions merged without detection. Formatting drifts.

### 3. Single `unwrap()` in Library Code
- `crates/devflow-core/src/lock.rs:31`: `fs::create_dir_all(path.parent().unwrap())?`
- If path has no parent, this panics in library code (not CLI)
- **Risk**: Low probability (state paths always have parents), but violates "no unwrap in library" standard

## High

### 4. Version Bumper: pyproject.toml Only
- `version.rs` only supports `pyproject.toml` format
- DevFlow itself uses `Cargo.toml` for versioning — can't dogfood its own version bumper
- `ROADMAP.md` targets v0.3.0 for Cargo.toml support
- **Risk**: DevFlow can't self-bump. Manual version updates needed.

### 5. Agent Enum Not a Trait
- Adding a new agent requires modifying `state.rs` (`Agent` enum + `launch_command` match)
- Agent-specific configuration (model, flags) not supported
- `ROADMAP.md` targets v0.4.0 for `Agent` trait
- **Risk**: Brittle extension point. Every agent addition touches core.

### 6. Verify/Docs Steps Are No-Ops
- State machine advances through `VERIFYING` and `DOCSING` but no commands run
- Config fields `verify_command`, `lint_command`, `docs_command` exist but aren't executed
- **Risk**: Phases complete without verification. User gets false sense of completion.

## Medium

### 7. Stale ROADMAP
- `ROADMAP.md` lists v0.2.0 (monitor, Hermes skill) and v0.3.0 (recover, lock) as TODO
- Monitor, recover, lock, and SIGTERM are already implemented (phase-01)
- Known Limitations section says "No monitor daemon yet" — false
- **Risk**: Contributors get wrong picture of project state.

### 8. No Hermes Skill
- `ROADMAP.md` v0.2.0 target: `skills/hermes/devflow/SKILL.md`
- Hermes currently drives devflow via raw shell commands (`terminal()` tool)
- **Risk**: Hermes can't detect devflow-managed projects or report phase transitions natively.

### 9. AGENTS.md References Old Tmux Approach
- `AGENTS.md` still says "send-keys launch command" — was fixed to pass command directly to `tmux new-session`
- "What's Already Done" section shows `CLI: empty src/main.rs` — `main.rs` is 318 lines, fully implemented
- **Risk**: New AI agents get stale context, may suggest wrong approaches.

## Low

### 10. No `cargo clippy` Config
- No `.clippy.toml` — using rustc defaults
- No deny-list for common mistakes (unwrap, expect in library, missing docs)
- **Risk**: Code quality drifts without lint enforcement.

### 11. Hardcoded `sleep 30` in Monitor
- `monitor.rs` shells out a script with `sleep 30` between tmux polls
- Not configurable — large projects with slow agents may waste 30s
- **Risk**: Minor latency in state advancement. Cosmetic.

### 12. No Windows Support
- Tmux is Unix-only — Windows users can't use devflow
- `ROADMAP.md` doesn't mention this limitation
- **Risk**: Low (target audience is Linux/macOS developers). Documented implicitly.

## Summary

| Severity | Count | Top Items |
|---|---|---|
| Critical | 3 | No tests, no CI, library unwrap |
| High | 3 | No Cargo.toml versioner, no Agent trait, verify/docs no-ops |
| Medium | 3 | Stale ROADMAP, no Hermes skill, stale AGENTS.md |
| Low | 3 | No clippy config, hardcoded sleep, no Windows |
