# DevFlow

**Agent-agnostic development workflow automation.**

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange.svg)](https://www.rust-lang.org)

DevFlow automates the mechanical steps of AI-assisted development — branching, monitoring, verifying, documenting, shipping — so you and your coding agents can focus on building.

## The Problem

You use AI coding agents (Claude Code, Codex, OpenCode) to build features. But every phase requires the same tedious mechanical work:

1. Create a feature branch
2. Launch the agent with the right command
3. Wait for it to finish
4. Run tests and lints
5. Update docs
6. Bump the version
7. Create a release branch and PR
8. Merge to main/develop
9. Clean up merged branches

DevFlow handles all of this. You say `devflow start --phase 3 --agent claude --monitor` and walk away.

## Quick Start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/denniyahh/devflow/main/scripts/install.sh | bash

# Or build from source
cargo install devflow

# Initialize a project
cd your-project
devflow init

# Start working on Phase 3 with Claude Code (background monitor)
devflow start --phase 3 --agent claude --monitor

# Check status anytime
devflow status

# When ready to ship (bumps version, creates PR)
devflow ship
```

## State Machine

```
IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING → SHIPPING → CLEANING → IDLE
```

Each step is automated or skippable via `.devflow.yaml` configuration. State is persisted to `.devflow/state.json` and survives restarts.

## Architecture

```
crates/
├── devflow-core/     ← Library: state machine, config, git, versioning, agent adapters
└── devflow-cli/      ← Binary: thin CLI wrapper around core
```

**Key design decisions:**

- **Agent-agnostic** — Claude, Codex, and OpenCode all implement the same `Agent` trait. Adding a new agent takes 3 changes.
- **Worktree isolation** — agents run in isolated git worktrees (`.worktrees/phase-NN/`), preventing cross-phase contamination.
- **Monitor daemon** — optional background process detects agent completion and auto-advances the state machine. No cron, no polling, no tmux.
- **Three-layer evaluation** — agents self-report via `DEVFLOW_RESULT` markers in stdout; fallback layers: exit code, then stdout existence.
- **Shared prompts** — all agents receive the same prompt via `phase_prompt()`. No agent-specific prompt logic.

## Agent Protocol

Agents communicate completion through the `DEVFLOW_RESULT` marker in stdout:

```
DEVFLOW_RESULT: {"status": "success", "commits": 3, "summary": "added tests"}
```

DevFlow evaluates agent output in three layers:

| Layer | Method | Authority |
|---|---|---|
| 1. Marker | Parse `DEVFLOW_RESULT` JSON from stdout | Authoritative |
| 2. Exit code | Exit 0 = success, non-zero = failed | Fallback |
| 3. Existence | stdout exists = success, empty = failed | Last resort |

Rate-limit detection: if an agent's stdout contains rate-limit messages (429), DevFlow writes `.devflow/cron-instructions.json` for rescheduling.

## Commands

### Core Workflow

| Command | Description |
|---|---|
| `devflow start --phase N [--agent X] [--monitor] [--worktree]` | Begin a phase: branch → launch agent → (optional) monitor |
| `devflow check` | Poll state, advance if agent done |
| `devflow status` | Show current step, phase, agent, PID, age |
| `devflow list` | List all feature branches with divergence from develop |
| `devflow cleanup` | Remove phase worktrees and their feature branches |

### Multi-Agent

| Command | Description |
|---|---|
| `devflow parallel --phases 7,8 [--agents claude,codex]` | Run multiple phases concurrently in isolated worktrees |
| `devflow sequentagent --phase 7 --agents claude,codex` | Run two agents sequentially on one phase with worktree isolation |
| `devflow reference [--refresh]` | Create a static reference worktree for multi-agent handoff |

### Shipping

| Command | Description |
|---|---|
| `devflow ship [--phase N]` | Bump version, create release branch, open a PR via `gh` |
| `devflow confirm` | Finalize a shipped phase: check merge, update docs |
| `devflow rejectpr [--reason X] [--redo]` | Reject the last ship; `--redo` unwinds PR/branch/version |

### Quality

| Command | Description |
|---|---|
| `devflow verify` | Run configured verification command (e.g., `cargo test`) |
| `devflow lint` | Run configured lint command (e.g., `cargo clippy`) |
| `devflow docs` | Run configured docs command with optional auto-commit |

### Setup & Recovery

| Command | Description |
|---|---|
| `devflow init` | Bootstrap `.devflow.yaml` and `.devflow/` directory |
| `devflow config` | Show effective configuration |
| `devflow recover` | Inspect or clean up stale/abandoned workflow state |

## Configuration

Projects declare their workflow in `.devflow.yaml` (git-tracked, portable):

```yaml
version:
  scheme: semver
  file: Cargo.toml              # or pyproject.toml, package.json
  field: workspace.package.version
  build_number: git

automation:
  auto_branch: true
  auto_verify: true
  auto_docs: true
  auto_version: patch           # major | minor | patch
  auto_ship: false              # set true for full automation
  auto_cleanup: true
  verify_command: cargo test
  lint_command: cargo clippy -- -D warnings
  docs_command: cargo doc --no-deps

git_flow:
  enabled: true
  main: main
  develop: develop
  feature_prefix: feature/
```

## Supported Agents

| Agent | CLI | Flag |
|---|---|---|
| Claude Code | `claude` | `--agent claude` |
| OpenAI Codex | `codex` | `--agent codex` |
| OpenCode | `opencode` | `--agent opencode` |

Agents must be installed separately. Run `devflow doctor` to verify availability.

## Requirements

- **Rust** 1.91+ (build from source)
- **git** 2.30+
- **gh CLI** 2.0+ (for PR creation, shipping)
- **A POSIX shell** (sh/bash)

Agents (Claude Code, Codex, OpenCode) must be installed separately. See [DEPENDENCIES.md](DEPENDENCIES.md) for the full matrix.

## Installation

```bash
# Option 1: One-command install (recommended)
curl -fsSL https://raw.githubusercontent.com/denniyahh/devflow/main/scripts/install.sh | bash

# Option 2: Cargo install
cargo install devflow

# Option 3: Build from source
git clone https://github.com/denniyahh/devflow.git
cd devflow
cargo build --release
cp target/release/devflow ~/.local/bin/
```

Verify your setup:

```bash
devflow doctor
```

## Documentation

- [DEPENDENCIES.md](DEPENDENCIES.md) — full dependency matrix
- [ARCHITECTURE.md](ARCHITECTURE.md) — design documentation (coming in v1.2.0)
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to contribute
- [CHANGELOG.md](CHANGELOG.md) — version history

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome.

## License

MIT OR Apache-2.0 — see [LICENSE](LICENSE).
