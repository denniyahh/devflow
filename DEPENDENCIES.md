# Dependencies

Every binary DevFlow needs, organized by how essential it is.

## Required

These must be on `$PATH` for DevFlow to function.

| Dependency | Min Version | Install |
|---|---|---|
| **git** | 2.30+ | Built-in (Linux/macOS) |
| **sh/bash** | Any POSIX | Built-in |
| **cargo/rust** | stable (edition 2024) | [rustup.rs](https://rustup.rs) |

## Required for Shipping

Needed for `devflow ship` / `devflow confirm` (PR creation and merge).

| Dependency | Min Version | Install |
|---|---|---|
| **gh CLI** | 2.0+ | `brew install gh` / `apt install gh` / [github.com/cli/cli](https://github.com/cli/cli) |

## Optional — Agents

Install the agents you want DevFlow to drive.

| Agent | CLI | Install |
|---|---|---|
| **Claude Code** | `claude` | `npm i -g @anthropic-ai/claude-code` |
| **OpenAI Codex** | `codex` | `npm i -g @openai/codex` |
| **OpenCode** | `opencode` | `cargo install opencode` |

## Optional — Development

For contributing to DevFlow itself.

| Tool | Purpose | Install |
|---|---|---|
| **cargo-clippy** | Linting | `rustup component add clippy` |
| **cargo-fmt** | Formatting | `rustup component add rustfmt` |

## Verification

Run `devflow doctor` to check what's installed and what's missing:

```
$ devflow doctor
  git 2.49.0          ✓
  sh (POSIX shell)    ✓
  cargo 1.91.0        ✓
  gh 2.78.0           ✓
  claude              ✓ (v2.1.181)
  codex               ✗ not found — install: npm i -g @openai/codex
  opencode            ✗ not found — install: cargo install opencode
  devflow v1.0.0      ✓
  .devflow.yaml       ✓ (found)
```
