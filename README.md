# DevFlow

**Agent-agnostic development workflow automation.**

[![CI](https://github.com/denniyahh/devflow/actions/workflows/ci.yml/badge.svg)](https://github.com/denniyahh/devflow/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

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

DevFlow handles all of this. You say `devflow start --phase 3 --agent claude --mode auto` and walk away.

## Quick Start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/denniyahh/devflow/main/scripts/install.sh | bash

# Or build from source
cargo install devflow

# Start working on Phase 3 with Claude Code in auto mode — no init step required
cd your-project
devflow start --phase 3 --agent claude --mode auto

# Check status anytime
devflow status
```

## Pipeline

```
Define → Plan → Code → Validate → Ship
```

The 5-stage pipeline is driven by GSD-native execution. State is persisted per-phase to `.devflow/state-NN.json` and survives restarts. `--mode auto` advances through Ship unattended (gating only on repeated Validate failure or an unexpected stage crash); `--mode supervise` also gates at Validate for human review.

## Architecture

```
crates/
├── devflow-core/     ← Library: state machine, config, git, versioning, agent adapters
└── devflow-cli/      ← Binary: thin CLI wrapper around core
```

**Key design decisions:**

- **Agent-agnostic** — Claude, Codex, and OpenCode all implement the same `AgentAdapter` trait. Adding a new agent follows a small checklist (see [ARCHITECTURE.md](ARCHITECTURE.md#extension-points--adding-an-agent)).
- **Worktree isolation by default** — agents run in isolated git worktrees (`.worktrees/phase-NN/`) unless `--no-worktree` is passed, preventing cross-phase contamination.
- **Monitor daemon** — optional background process detects agent completion and auto-advances the state machine. No cron, no polling, no tmux.
- **Four-layer evaluation** — operator-declared external post-conditions can fail a stage before agent-controlled signals; ordinary work then uses `DEVFLOW_RESULT`, exit code + commit count, and the final commit heuristic.
- **Per-stage prompts** — `stage_prompt(stage, phase)` builds a dedicated prompt per pipeline stage (not one shared instruction template); the same prompt text is used across agents for a given stage, with adapter-specific logic limited to CLI launch flags, not prompt content.

## Agent Protocol

Agents communicate completion through the `DEVFLOW_RESULT` marker in stdout:

```
DEVFLOW_RESULT: {"status": "success", "commits": 3, "summary": "added tests"}
```

The Validate stage additionally requires a `verdict` field (`"pass"` or `"gaps"`) — a bare `status: success` is not enough to advance to Ship; only `verdict: "pass"` is.

DevFlow evaluates agent output in four layers:

| Layer | Method | Authority |
|---|---|---|
| 0. External verification | Run operator-authored `external_verify` commands declared in PLAN frontmatter | Authoritative failure |
| 1. Marker | Parse `DEVFLOW_RESULT` JSON from stdout | Authoritative for ordinary plans |
| 2. Exit code + commits | Exit 0 **and** commits on the feature branch = success; otherwise failed | Fallback |
| 3. Commit heuristic | Exit code unknown: commits exist = probable success (with warning) | Last resort |

Rate-limit detection: during `devflow sequentagent`, if an agent's stdout contains rate-limit messages (429), DevFlow writes a per-phase `.devflow/cron-instructions-{phase:02}.json` for rescheduling.

## Commands

### Core Workflow

| Command | Description |
|---|---|
| `devflow start --phase N --agent X [--mode auto\|supervise] [--no-worktree]` | Begin a phase: branch/worktree → launch agent → monitor pipeline |
| `devflow status` | Show current stage, phase, agent, PID, age |
| `devflow list` | List all feature branches with divergence from develop |
| `devflow gate list` | List gates awaiting a response |
| `devflow gate approve\|reject <phase> [--stage STAGE] [--note "..."]` | Answer a human gate — the pause points where the workflow waits for approval |
| `devflow logs [--phase N] [--follow] [--stderr]` | Print or follow an agent's captured output for a phase |
| `devflow history [N]` | Correlate a phase's events with retained capture and review evidence |
| `devflow cleanup` | Remove phase worktrees and their feature branches |

### Multi-Agent

| Command | Description |
|---|---|
| `devflow parallel --phases 7,8 [--agents claude,codex]` | Run multiple phases concurrently in isolated worktrees |
| `devflow sequentagent --phase 7 --agents claude,codex` | Run two agents sequentially on one phase with worktree isolation |
| `devflow reference [--refresh]` | Create a static reference worktree for multi-agent handoff |

### Quality

| Command | Description |
|---|---|
| `devflow test` | Run local quality checks: `cargo test`, clippy, and `fmt --check` |

### Setup & Recovery

| Command | Description |
|---|---|
| `devflow recover` | Inspect or clean up stale/abandoned workflow state |
| `devflow doctor` | Check that required tools and agents are installed |

## Configuration

DevFlow stores runtime state per-phase in `.devflow/state-NN.json` (git-ignored). No init step is required. Workflow choices remain CLI flags; an optional minimal `devflow.toml` configures reliability knobs only.

```toml
capture_retention = 5
review_angles = ["doc accuracy", "security", "CI correctness", "external state"]
external_verify_enabled = true
```

PLAN-declared verification shell is agent-writable and requires explicit
operator authorization bound to the reviewed bytes, for example
`DEVFLOW_TRUST_EXTERNAL_VERIFY='["test -f shipped.txt"]'`. Changed commands
fail closed without execution.

Key flags:

| Flag | Description |
|---|---|
| `--phase N` | Phase number to execute |
| `--agent claude\|codex\|opencode` | Agent to launch |
| `--mode auto\|supervise` | `auto` advances through Ship unattended; `supervise` also gates at Validate for human review |
| `--no-worktree` | Run directly in the primary checkout instead of an isolated worktree (worktree is the default) |

Gate responses and unattended-run notifications are file-based: a fired gate writes `.devflow/gates/{phase}-{stage}.json` and (if `DEVFLOW_GATE_NOTIFY_CMD` is set) runs that command with `DEVFLOW_GATE_PHASE`/`DEVFLOW_GATE_STAGE`/`DEVFLOW_GATE_CONTEXT` in its environment. Respond by writing `.devflow/gates/{phase}-{stage}.response.json` with `{"approved": true|false, "note": "..."}`. The poll timeout defaults to 7 days and is configurable via `DEVFLOW_GATE_TIMEOUT_SECS`.

`DEVFLOW_CAPTURE_RETENTION`, `DEVFLOW_REVIEW_ANGLES`, and
`DEVFLOW_EXTERNAL_VERIFY_ENABLED` override the corresponding TOML values.

## Supported Agents

| Agent | CLI | Flag |
|---|---|---|
| Claude Code | `claude` | `--agent claude` |
| OpenAI Codex | `codex` | `--agent codex` |
| OpenCode | `opencode` | `--agent opencode` |

Agents must be installed separately. Run `devflow doctor` to verify availability.

## Requirements

- **Rust** stable, edition 2024 (build from source) — see `rust-toolchain.toml`
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
- [ARCHITECTURE.md](ARCHITECTURE.md) — design documentation
- [OPERATIONS.md](OPERATIONS.md) — operator reference (gate protocol, env vars, `.devflow/` file inventory)
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to contribute
- [CHANGELOG.md](CHANGELOG.md) — version history

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome.

## License

MIT OR Apache-2.0 — see [LICENSE](LICENSE).
