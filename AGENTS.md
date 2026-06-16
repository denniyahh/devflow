# DevFlow Project Context

## What We're Building

DevFlow is an **agent-agnostic development workflow automation CLI** written in Rust.

Core problem: AI coding agents (Claude, OMX, Codex, OpenCode) need mechanical workflow steps handled automatically:
- Creating/deleting git branches (git flow model)
- Monitoring agent completion
- Version bumping (semver + build number)
- Documentation updates
- PR/release management
- Post-phase cleanup

## Architecture

```
crates/
├── devflow-core/     ← Library crate: state machine, config, git, versioning, agent adapters
├── devflow-cli/      ← Binary crate: thin CLI wrapper around core
└── devflow-tui/      ← Future: ratatui TUI frontend
```

## Key Design Decisions

1. **Rust** — single binary, cross-platform, no runtime dependencies
2. **Tmux for all agents** — unified control surface (send-keys, capture-pane)
3. **PID-based monitoring** — background process waits for tmux session to die, then advances state
4. **Git flow** — feature → develop, release → main + develop
5. **CLI baseline, MCP/HTTP/TUI later** — core library returns structs, frontends format them
6. **Agent-agnostic** — works with Claude, OMX, Codex, OpenCode via trait-based adapters

## State Machine

```
IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING → SHIPPING → CLEANING → IDLE
```

State persisted to `.devflow/state.json` in project root.

## Config

Projects have a `.devflow.yaml` (git-tracked, portable):
```yaml
version:
  scheme: semver
  file: pyproject.toml
  field: project.version
  build_number: git
automation:
  auto_branch: true
  auto_verify: true
  auto_docs: true
  auto_version: patch
  auto_ship: false
  auto_cleanup: true
git_flow:
  main: main
  develop: develop
  feature_prefix: feature/
```

## What's Already Done

The workspace is scaffolded:
- `Cargo.toml` (workspace root)
- `crates/devflow-core/` — library with `Cargo.toml`, `src/lib.rs`, `src/state.rs`, `src/config.rs`
- `crates/devflow-cli/` — binary with `Cargo.toml`, empty `src/main.rs`

The state machine (`state.rs`) has:
- `Step` enum: Idle, Branching, Executing, Verifying, Docsing, Shipping, Cleaning
- `State` struct: step, phase, agent, tmux_session, monitor_pid, started_at, project_root
- `Agent` enum: Claude, Omx, Codex, OpenCode
- Agent `launch_command()` method
- `State::advance()` → next step

The config (`config.rs`) has:
- Full `Config` struct with defaults
- `Config::load()` from `.devflow.yaml`
- `Config::should_skip()` for automation toggles

## What Needs to Be Built

### 1. CLI (crates/devflow-cli/src/main.rs)
Commands using clap:
- `devflow start --phase N [--agent claude|omx|codex|opencode]` — begin workflow
- `devflow check [project]` — poll state, advance if agent done
- `devflow status [project]` — show current state
- `devflow ship [project]` — create release branch + bump version
- `devflow init [project]` — bootstrap .devflow.yaml
- `devflow config [project]` — show config

### 2. Git Flow module (crates/devflow-core/src/git.rs)
- `feature_start(phase, prefix)` → creates feature branch from develop
- `feature_finish(phase)` → merges to develop, deletes branch
- `release_start(version)` → creates release branch from develop
- `release_finish(version)` → merges to main + develop, tags, deletes branch
- `cleanup_merged()` → deletes merged local branches
- Shell out to `git flow` commands or implement manually

### 3. Version bumper (crates/devflow-core/src/version.rs)
- `read_version(config)` → parse current version from pyproject.toml / Cargo.toml / package.json
- `bump(version, component)` → increment major/minor/patch
- `build_number(config)` → git rev-list --count or timestamp
- `write_version(config, new_version)` → write back to file

### 4. Tmux launcher (crates/devflow-core/src/tmux.rs)
- `launch_agent(state)` → tmux new-session, send-keys launch command
- `agent_running(session_name)` → tmux has-session check
- `capture_output(session_name)` → tmux capture-pane

### 5. Monitor daemon (crates/devflow-core/src/monitor.rs)
- `spawn_monitor(state)` → fork child process
- Child: while tmux session exists, sleep 30s
- On session death: call `devflow check` logic
- Return PID to parent

### 6. Hermes skill (skill file in repo)
- `skills/hermes/devflow/SKILL.md` — how Hermes uses devflow
- When user says "work on Phase N" → `devflow start --phase N --agent claude`
- When done → `devflow status` → report

## Priorities
1. Get it compiling first
2. CLI start/check/status commands
3. Git flow integration
4. Version bumper
5. Monitor daemon
6. Hermes skill
7. Tests
8. TUI (v2)

## Code Standards
- Rust edition 2024
- All public items documented
- Error handling with thiserror
- Structured output from core, formatting in CLI
- No unwrap() in library code — use Result
