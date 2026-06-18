---
name: devflow
description: "Automated development workflow via DevFlow CLI — branch management, agent launch, monitoring, recover, version bumping, shipping. One command, end-to-end."
version: "0.7.0"
---

# DevFlow — Hermes Integration

DevFlow is an agent-agnostic CLI that automates the entire development workflow. This skill teaches Hermes when and how to invoke it.

**Current version:** v0.5.0+ — direct process spawning (no tmux), PID-based monitoring.
**Installed:** `~/.local/bin/devflow` (symlinked from `~/Github/devflow/target/release/devflow`)
**GitHub:** https://github.com/denniyahh/devflow (public)

## When to use

- User says "work on Phase N" or "start Phase N" or "develop Phase N"
- User says "ship it" or "create release" or "bump version"
- User asks "what's the dev status?" or "what phase are we on?"
- User says "continue development" or "keep working on this project"

## Architecture (v0.5.0+)

DevFlow spawns agents as **direct child processes** (no tmux):
- **Claude:** `claude -p "<prompt>" --output-format json --dangerously-skip-permissions --max-turns 50`
- **Codex:** `codex exec "<prompt>" --sandbox workspace-write --json`
- **OMX:** `omx exec "<prompt>" --sandbox workspace-write --json`
- **OpenCode:** `opencode run "<prompt>"`

Agents run headless — no interactive prompts, no trust dialogs. They produce structured output and exit when done.

## Prerequisites

- `devflow` binary installed and on PATH
- Project has `.devflow.yaml` (run `devflow init`)
- Git flow branches configured (`main` and `develop` exist) — or set `auto_branch: false` for GSD-style workflows
- Agent CLI tools available on host PATH: `claude`, `codex`, `omx`, `opencode`

## Workflow

### Starting a phase (AUTOMATED — unreliable, see pitfalls)

```bash
devflow start --phase N --agent <agent> --monitor
```

This creates a feature branch, spawns the agent as a child process, and launches a monitor daemon. The monitor polls `kill -0 <pid>` every 30s and runs `devflow check` when the agent exits.

### Starting a phase (MANUAL — reliable, use this for complex phases)

The automated `devflow start` often produces no work — Claude exits silently with the rich multiline prompt. **For phases with CONTEXT.md files, drive Claude directly:**

```bash
cd ~/Github/devflow
devflow start --phase N --agent claude --monitor  # creates branch + monitor
# Agent exits immediately → devflow check advances state → merge feature branch

# INSTEAD, drive Claude directly:
git checkout -b feature/phase-NN develop
claude -p "Complete phase N. Read .planning/phases/NN-*/CONTEXT.md for specs. Implement, test, commit." \
  --output-format json --dangerously-skip-permissions --max-turns 50
# Wait for Claude to finish (5-15 min), then:
git checkout develop && git merge --no-ff feature/phase-NN -m "merge: phase N — description"
git branch -d feature/phase-NN
```

Use `devflow status` between phases to track state, and `devflow ship` for version bumps.

### Checking status

```bash
devflow status
```

Reports: step, phase, agent, PID, whether agent is running.

### Shipping

```bash
devflow ship
```

Bumps version and creates release branch. Does NOT merge or push.

### Bootstrapping

```bash
devflow init
```

Creates `.devflow.yaml` with sensible defaults.

## What DevFlow handles automatically

| Step | What happens |
|---|---|
| **Branch** | Creates `feature/phase-NN` from `develop` |
| **Launch** | Spawns agent as direct child process (PID-based) |
| **Monitor** | Background daemon polls `kill -0 <pid>`, auto-advances on exit |
| **Advance** | When agent exits: verifying → docsing → shipping → cleaning → idle |
| **Version** | Bumps semver + git build number |
| **Release** | Creates release branch, merges to main + develop |
| **Cleanup** | Deletes merged feature/release branches |

## Rich Prompts (v0.5.0+)

DevFlow now generates structured prompts with:
1. Required Reading section (CLAUDE.md, ROADMAP.md, CONTEXT.md, AGENTS.md)
2. Process steps (read → implement → test → lint → format → commit)
3. Available Commands (`cargo test`, `cargo clippy`, `cargo fmt`, `cargo build --release`)
4. Success criteria

**Per-phase context files** live in `.planning/phases/NN-name/CONTEXT.md` with:
- Goal statement
- Task checklist (`- [ ]` items)
- Verification commands
- Success criteria

Create these BEFORE launching phases — agents rely on them.

## Configuration

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
  auto_version: patch
  auto_ship: false
  auto_cleanup: true
  verify_command: cargo test
  lint_command: cargo clippy -- -D warnings
  docs_command: echo "Phase docs manually updated"
  docs_auto_commit: false
  continue_on_error: true
git_flow:
  enabled: false            # set false for GSD-style workflow
  main: main
  develop: develop
  feature_prefix: feature/
```

## Recovery

```bash
devflow recover              # inspect abandoned/stale state
devflow recover --clean      # clear abandoned state
```

## Pitfalls

### Automated launch is unreliable
**The primary pitfall of v0.5.0+:** `devflow start` with the rich multiline prompt often fails — Claude exits immediately without producing any work. The monitor advances state to idle, and the feature branch has zero new commits beyond develop.

**Workaround:** Drive Claude directly with `claude -p "...read CONTEXT.md..."` on the feature branch, then manually merge. Use `devflow` only for: branch creation (`devflow start` then `git checkout develop && git merge`), status checks (`devflow status`), and version bumping (`devflow ship`).

**Root cause hypothesis:** The multiline prompt with `\n` sequences may not survive the `Command::new("claude").arg(prompt)` path correctly. Direct `claude -p "$PROMPT"` from shell works fine.

### `--bare` flag breaks Claude v2.1.181
Claude v2.1.181 returns "Not logged in" when `--bare` is passed alongside other flags. DevFlow v0.5.0+ does NOT use `--bare`. If you see this error, you're running an old binary — rebuild.

### Codex stdin hangs with Stdio::null()
Codex CLI reads stdin in a loop and `/dev/null` never produces EOF for it. Fix: use `Stdio::piped()` and immediately `drop(child.stdin.take())` after spawn. DevFlow v0.5.0+ handles this.

### Codex rate limits
OpenAI Codex CLI has usage caps. If you see "You've hit your usage limit. Try again at XX:XX PM", switch to Claude: `devflow start --phase N --agent claude --monitor`.

Note: Codex and OMX share the same OpenAI API rate limit bucket. If Codex is rate-limited, OMX will also fail.

### Multi-agent rotation
When rate limits hit, chain through available providers: Claude (Anthropic) → Codex (OpenAI) → OMX via Fish (OpenAI, shared bucket with Codex). Run phases in parallel when multiple providers are available. OMX requires Fish shell for PATH: `fish -lc 'echo "" | omx exec --sandbox workspace-write --json "prompt"'`.

### `?` operator on Option returns silently fails
When writing Rust parsing logic inside `Option`-returning functions, never use `?` on methods like `split_once()` — the `?` propagates `None` as the function's return value even when it just means "skip this line." Use `if let` instead. See `references/agent-launch-debugging.md` § Issue 6 for reproduction and fix.

### Manual state advancement
If monitor died and state is stuck at "executing" but agent completed: `devflow check` repeatedly until idle.

### Binary deployment
`~/.local/bin/devflow` is a static copy of `~/Github/devflow/target/release/devflow`. After `cargo build --release`, copy it: `cp target/release/devflow ~/.local/bin/devflow`.

### Git state requirements
Ensure `main` and `develop` branches exist. `devflow init` doesn't create them.

### State cleanup
After pipeline completes, `.devflow/state.json` is removed. `devflow status` says "no active state."

### Distrobox missing git
Fresh Fedora distrobox containers lack git. Install: `sudo dnf install -y git`.

## Project

DevFlow is itself developed with DevFlow:
- **Repo:** `~/Github/devflow` — https://github.com/denniyahh/devflow
- **ROADMAP.md:** 6-phase v1.0.0 plan
- **Architecture:** Rust workspace — `devflow-core` (library) + `devflow-cli` (binary)
- **Agent Launch Debugging:** see `references/agent-launch-debugging.md` — v0.5.0 launch issues and fixes
