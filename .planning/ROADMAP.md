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

## Phase 6: Ship Readiness (Priority: HIGH — COMPLETED v0.5.1)

**Shipped:** Agent completion protocol (DEVFLOW_RESULT), three-layer result evaluation, monitor daemon owns agent capture, JSON envelope parsing, config-threaded git-flow, clippy clean. 115 tests.

---

## Phase 7: Git Worktrees + PR Integration (Priority: HIGH, v1.0.0)

### 7a — Git Worktree Support
- [ ] `devflow start --phase N --worktree` creates isolated working directory
- [ ] Worktrees live at `.worktrees/phase-NN/` within the repo
- [ ] Agent prompts use absolute worktree path as workdir
- [ ] `devflow cleanup` removes worktree via `git worktree remove`
- [ ] `devflow status` lists active worktrees
- [ ] **Use-case:** Agent isolation — Claude/Codex each get their own sandbox
- [ ] **Use-case:** Parallel phase execution without stash/checkout churn
- [ ] **Use-case:** Reference worktree on develop for diffing/testing while feature branch builds
- [ ] **Verify:** Two agents can run concurrently in separate worktrees without conflict

### 7b — PR Creation via `gh` CLI
- [ ] `devflow ship` creates PR via `gh pr create` with auto-generated body
- [ ] PR body includes: phase summary, changed files, test results, TDD audit trail
- [ ] PR merge detection — auto-advance state when PR merged
- [ ] **Verify:** `devflow ship` on devflow itself creates a valid PR

### 7c — Test Hardening (from Claude's Phase 6 review)
- [ ] Fix weak `spawn_monitor` test — assert on observable output, not `pid > 0`
- [ ] Add Layer 2 failure-path tests (exit=0+commits=0, exit≠0)
- [ ] Cover lowercase-no-space marker variant in agent_result
- [ ] End-to-end monitor integration test (spawn → agent writes DEVFLOW_RESULT → check advances)

### 7d — Prompt Rationalization
- [ ] Extract shared `phase_prompt()` to `agents/mod.rs`
- [ ] Delete `simple_prompt()` — Codex gets the same rich contract as Claude
- [ ] **Verify:** Both agents receive identical instruction text, differing only in CLI flags

---

## Phase Priority Order

```
Phase 1 (CI + Tests)     ████████████████  CRITICAL — foundation for everything
Phase 2 (Cargo version)  ████████████      HIGH     — unblocks self-dogfooding
Phase 3 (Verify/docs)    ████████████      HIGH     — makes phases meaningful
Phase 4 (Hermes skill)   ████████          MEDIUM   — better Hermes integration
Phase 5 (Agent trait)    ████████          MEDIUM   — cleaner extension
Phase 6 (Ship readiness) ████████████      HIGH     — agent completion, monitor capture
Phase 7 (Worktrees + PR) ████████████      HIGH     — v1.0.0: parallel agents, PR shipping
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
