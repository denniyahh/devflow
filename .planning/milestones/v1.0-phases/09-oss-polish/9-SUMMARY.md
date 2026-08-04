# Phase 9 Summary: Open-Source Polish

> Completed: 2026-06-18 | Agent: Claude, Codex (review) | Version: v1.2.0

## Accomplished

### 9a — Remove OMX Agent Support
- [x] Deleted `crates/devflow-core/src/agents/omx.rs`
- [x] Removed `Omx` from `AgentKind` enum, parser, display
- [x] Stripped OMX from `adapter_for()`, module exports, phase prompt
- [x] Removed OMX references from Hermes devflow skill
- [x] Deleted stale `.omx/` directory
- [x] Zero remaining OMX references in source (grep verified)

### 9b — Remove Local-Only Assumptions
- [x] `distrobox.ini` untracked
- [x] `.planning/` gitignore narrowed — tracked convention files, scratch ignored
- [x] GPG-off test setup documented

### 9c — Architecture Documentation
- [x] Root `ARCHITECTURE.md` — crates, state machine, agent model, monitor, worktree, config, extension points
- [x] Corrected "3 changes" agent checklist to actual checklist
- [x] Removed phantom `git_flow.enabled` field from docs

### 9d — Ship Fix
- [x] `devflow ship` cuts release branch from current `HEAD`, not `develop`
- [x] Commits unique to shipped branch kept in release

### 9e — CI Polish
- [x] Status badge in README
- [x] Toolchain pinned to stable
- [x] CI runs `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`

### 9f — Doctor: Stale Binary Detection
- [x] `devflow doctor` detects stale binaries on PATH
- [x] Warns when multiple devflow binaries found at different versions

## Verification
- 173 tests, clippy clean, format clean
- No OMX references anywhere (grep: zero results)
- ARCHITECTURE.md documents actual design
- `devflow ship` release branch contains correct commits
