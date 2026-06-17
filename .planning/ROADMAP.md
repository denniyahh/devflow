# DevFlow — Updated Roadmap

> Generated: 2026-06-17 | Replaces stale `ROADMAP.md` as source of truth

## Current State: v0.5.0 (44% of v1.0.0 scope)

**Done:** Core state machine, CLI, git flow, tmux launcher, monitor daemon, error recovery, lock file, basic version bumper (pyproject.toml only).

**Not done:** Tests (5%), CI, Cargo.toml versioner, verify/docs execution, Hermes skill, Agent trait, PR integration.

---

## Phase 1: CI Foundation + Test Coverage (Priority: CRITICAL)

**Why first:** Every subsequent phase needs CI to verify correctness. Building without CI is flying blind.

### 1a — GitHub Actions CI
- [ ] `.github/workflows/ci.yml` — `cargo test`, `cargo clippy`, `cargo fmt --check`
- [ ] Runs on push to develop/main, PRs to develop/main
- [ ] Concurrency: cancel-in-progress on new push
- [ ] Cachix/nix caching for faster builds (if using Nix)
- [ ] **Verify:** All CI jobs pass on current codebase

### 1b — Test Coverage
- [ ] `state.rs` — advance/skip/edge cases
- [ ] `config.rs` — all fields, defaults, missing file, invalid YAML
- [ ] `lock.rs` — acquire/release, concurrent acquire fails, stale lock
- [ ] `version.rs` — edge cases, build number, error paths
- [ ] `workflow.rs` — state persistence, load/save/clear
- [ ] `git.rs` — integration tests (temp git repos)
- [ ] `tmux.rs` — integration tests (skip if tmux unavailable)
- [ ] **Target:** >60% test coverage

### 1c — Fix Critical Issues
- [ ] Replace `unwrap()` in `lock.rs:31` with proper error handling
- [ ] Update AGENTS.md to reflect current state (no more "send-keys" references)
- [ ] Update ROADMAP.md v0.2.0/v0.3.0 sections marking completed items

---

## Phase 2: Version Bumper Expansion (Priority: HIGH)

**Why:** DevFlow can't bump its own version (uses Cargo.toml). Self-dogfooding is blocked.

- [ ] `Cargo.toml` support — read/write `workspace.package.version`
- [ ] `package.json` support — read/write `version` field
- [ ] Auto-detect version file format from project config
- [ ] Update `.devflow.yaml` schema: add `version.file` auto-detection
- [ ] **Verify:** `devflow ship` bumps devflow's own version in `Cargo.toml`

---

## Phase 3: Verify & Docs Execution (Priority: HIGH)

**Why:** State machine advances through VERIFYING/DOCSING but nothing runs. Phases complete with false confidence.

- [ ] `devflow verify` — runs `automation.verify_command` from config
- [ ] `devflow lint` — runs `automation.lint_command` (cargo clippy, ruff check, etc.)
- [ ] `devflow docs` — runs `automation.docs_command`
- [ ] Respect `continue_on_error` config flag
- [ ] Capture and display output on failure
- [ ] **Verify:** DevFlow phase auto-runs `cargo test && cargo clippy` before marking complete

---

## Phase 4: Hermes Skill (Priority: MEDIUM)

**Why:** Hermes currently drives devflow via raw `terminal()` calls. Native skill enables auto-detection and structured reporting.

- [ ] Create `skills/hermes/devflow/SKILL.md`
- [ ] Auto-detect `.devflow.yaml` in project repos
- [ ] Commands: `devflow start`, `devflow check`, `devflow status`
- [ ] Report phase transitions to user
- [ ] Skill testing script

---

## Phase 5: Agent Trait Refactor (Priority: MEDIUM)

**Why:** Adding new agents requires modifying core code. Trait enables extension without touching `state.rs`.

- [ ] `Agent` trait: `launch()`, `is_running()`, `capture_output()`, `name()`
- [ ] Per-agent impls in `agents/`: `claude.rs`, `codex.rs`, `omx.rs`, `opencode.rs`
- [ ] Agent config in `.devflow.yaml`: model, flags, env vars
- [ ] Backward compatible: enum → trait migration with deprecation path
- [ ] **Verify:** All existing agents work identically after refactor

---

## Phase 6: GitHub PR Integration (Priority: LOW, v1.0.0)

- [ ] PR creation via `gh` CLI on ship
- [ ] PR body auto-generated from phase summary
- [ ] Merge detection — auto-advance on PR merge
- [ ] `CODE_OF_CONDUCT.md`, `CHANGELOG.md`
- [ ] Release workflow: build + publish binary artifacts
- [ ] `cargo install devflow` install path

---

## Phase Priority Order

```
Phase 1 (CI + Tests)     ████████████████  CRITICAL — foundation for everything
Phase 2 (Cargo version)  ████████████      HIGH     — unblocks self-dogfooding
Phase 3 (Verify/docs)    ████████████      HIGH     — makes phases meaningful
Phase 4 (Hermes skill)   ████████          MEDIUM   — better Hermes integration
Phase 5 (Agent trait)    ████████          MEDIUM   — cleaner extension
Phase 6 (PR integration) ████              LOW      — v1.0.0 polish
```

## Success Criteria per Phase

| Phase | Success |
|---|---|
| 1 | CI green on every push. >60% test coverage. No `unwrap()` in library. |
| 2 | `devflow ship` bumps `Cargo.toml`. Self-dogfooding works. |
| 3 | `devflow check` runs verify/lint commands. Phase fails if tests fail. |
| 4 | Hermes detects devflow projects. `devflow status` works from Hermes. |
| 5 | New agents added via trait impl. No `state.rs` modifications needed. |
| 6 | PR created on ship. CI runs on PR. Release workflow publishes binary. |
