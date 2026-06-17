# DevFlow

> Agent-agnostic development workflow automation CLI

## Overview

**Language:** Rust (edition 2024)
**Version:** 0.5.0
**License:** MIT OR Apache-2.0
**Repository:** https://github.com/denniyahh/devflow

DevFlow automates the mechanical workflow steps that AI coding agents need:
branch creation, agent launch, completion monitoring, version bumping, documentation,
and cleanup. Agent-agnostic — works with Claude Code, OpenAI Codex, OMX, and OpenCode.

## Project Status

| Item | Status |
|---|---|
| Core state machine | ✅ |
| CLI (start/check/status/ship/init/config) | ✅ |
| Git flow (feature/release branching) | ✅ |
| Tmux agent launcher | ✅ (fixed monitor deadlock 2026-06-17) |
| Monitor daemon | ✅ |
| Error recovery (recover, lock, SIGTERM) | ✅ |
| Version bumper (pyproject.toml only) | 🟡 Partial |
| CI/CD | ❌ None |
| Tests | ❌ 2 tests (5% coverage) |
| Hermes skill | ❌ |
| GitHub PR integration | ❌ |
| TUI | ❌ |

## Key Files

| File | Purpose |
|---|---|
| `ROADMAP.md` | Original version roadmap (v0.1.0 → v1.1+, partially stale) |
| `AGENTS.md` | AI agent context (partially stale — references old tmux approach) |
| `.planning/codebase/` | Codebase map (7 documents, 2026-06-17) |
| `.planning/CONCERNS.md` | Top findings from codebase audit |

## Tech Stack

- **Rust 2024** — workspace: `devflow-core` (lib) + `devflow-cli` (binary)
- **Dependencies**: serde, clap, thiserror, tracing (zero network deps)
- **System**: tmux, git (required at runtime)
- **Build**: `cargo build --release` → single static binary (~20MB)
