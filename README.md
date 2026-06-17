# DevFlow

**Agent-agnostic development workflow automation.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange.svg)](https://www.rust-lang.org)

DevFlow automates the mechanical steps of AI-assisted development — branching, monitoring, verifying, documenting, shipping — so you and your coding agents can focus on building.

## The Problem

You use AI coding agents (Claude Code, Codex, OpenCode, oh-my-codex) to build features. But every phase requires the same tedious mechanical work:

1. Create a feature branch
2. Launch the agent (in tmux, with the right command)
3. Wait for it to finish (polling? cron? manual?)
4. Run tests and lints
5. Update docs
6. Bump the version
7. Create a release branch
8. Merge to main/develop
9. Clean up merged branches

DevFlow handles all of this. You say `devflow start --phase 3 --agent claude` and walk away.

## Quick Start

```bash
# Install
cargo install devflow

# Initialize a project
cd your-project
devflow init

# Start working on Phase 3 with Claude Code
devflow start --phase 3 --agent claude --monitor

# Check status anytime
devflow status

# When ready to ship
devflow ship
```

## State Machine

```
IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING → SHIPPING → CLEANING → IDLE
```

Each step is automated or skippable via `.devflow.yaml` configuration.

## Configuration

Projects declare their workflow in `.devflow.yaml` (git-tracked, portable):

```yaml
version:
  scheme: semver
  file: Cargo.toml          # or pyproject.toml, package.json
  field: workspace.package.version
  build_number: git

automation:
  auto_branch: true
  auto_verify: true
  auto_docs: true
  auto_version: patch       # major | minor | patch
  auto_ship: false          # set true for full automation
  auto_cleanup: true
  verify_command: cargo test
  lint_command: cargo clippy -- -D warnings

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
| oh-my-codex | `omx` | `--agent omx` |
| OpenCode | `opencode` | `--agent opencode` |

## Commands

| Command | Description |
|---|---|
| `devflow start --phase N [--agent X] [--monitor]` | Begin a phase: branch → launch agent → (optional) monitor |
| `devflow check` | Poll state, advance if agent done. Safe for cron. |
| `devflow status` | Show current step, agent, session, age |
| `devflow recover` | Inspect abandoned/stale state |
| `devflow recover --clean` | Clean up stale state |
| `devflow ship` | Bump version, create release branch |
| `devflow finish` | Merge release → main + develop, tag, delete branch |
| `devflow init` | Bootstrap `.devflow.yaml` |
| `devflow config` | Show effective config |

## Recovering from Crashes

If your agent crashes or you lose your tmux session:

```bash
devflow recover           # inspect state — is it stale?
devflow recover --clean   # clear abandoned state
devflow start --phase 3 --agent claude  # restart
```

DevFlow's lock file prevents concurrent `devflow check` invocations from racing.

## Requirements

- **Rust** 1.91+ (build from source)
- **tmux** (for agent session management)
- **git** 2.30+

Agents (Claude Code, Codex, etc.) must be installed separately.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full plan. Current focus:

- **v0.6.0** — Error recovery, multi-project support, version bumper expansion
- **v0.7.0** — Configurable agents, language-agnostic defaults, pluggable formats

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). PRs welcome.

## License

MIT — see [LICENSE](LICENSE).
