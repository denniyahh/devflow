# DevFlow — Integrations

> Generated: 2026-06-17 | Mapper: gsd-map-codebase (sequential)

## External Services

**None.** DevFlow has zero network dependencies. It does not call any HTTP APIs, does not connect to databases, and does not use webhooks.

## Shell-Out Integrations

All external interactions are via subprocess (`std::process::Command`):

### Git (`git.rs`)
- `git checkout -b feature/phase-NN` — creates feature branch
- `git checkout develop && git merge --no-ff feature/phase-NN` — finishes feature
- `git branch -d feature/phase-NN` — cleanup
- `git checkout -b release/vX.Y.Z` — release branching
- `git tag vX.Y.Z` — release tagging
- `git rev-list --count HEAD` — build number

### Tmux (`tmux.rs`, `monitor.rs`)
- `tmux new-session -d -s <name> <command>` — launches agent
- `tmux has-session -t <name>` — liveness check
- `tmux capture-pane -p -t <name>` — output capture

### Agent CLIs (`state.rs` — `launch_command()`)
- `claude --dangerously-skip-permissions` — Claude Code
- `codex exec --sandbox workspace-write "Work on phase N..."` — OpenAI Codex
- `omx exec --full-auto --sandbox danger-full-access` — OMX
- `opencode run` — OpenCode

## File System

| Path | Purpose | Access Pattern |
|---|---|---|
| `.devflow.yaml` | Project config | Read (serde YAML) |
| `.devflow/state.json` | Workflow state | Read/Write (serde JSON) |
| `.devflow/state.lock` | Concurrency control | Create/Delete |
| `pyproject.toml` | Version source | Read/Write (text replacement) |
| `Cargo.toml` | Version source (planned v0.3.0) | Not yet implemented |

## Auth / Secrets

**None.** DevFlow does not handle credentials, API keys, or tokens. Agent authentication is handled entirely by the agent CLIs themselves (Claude, Codex, etc. have their own auth mechanisms).

## Platform Support

| Platform | Status | Notes |
|---|---|---|
| **Linux** | ✅ Primary | Fedora KDE (Aurora), any distro with tmux+git |
| **macOS** | ✅ Supported | Tested on Intel macOS 15.7 (SSH target) |
| **Windows** | ❌ Unsupported | No tmux, no plan to support |
