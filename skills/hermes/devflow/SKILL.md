---
name: devflow
description: "Automated development workflow via DevFlow CLI — branch management, agent launch, monitoring, version bumping, shipping. One command, end-to-end."
version: "0.1.0"
---

# DevFlow — Hermes Integration

DevFlow is an agent-agnostic CLI that automates the entire development workflow. This skill teaches Hermes when and how to invoke it.

## When to use

- User says "work on Phase N" or "start Phase N" or "develop Phase N"
- User says "ship it" or "create release" or "bump version"
- User asks "what's the dev status?" or "what phase are we on?"
- User says "continue development" or "keep working on this project"

## Prerequisites

- `devflow` binary installed and on PATH
- Project has `.devflow.yaml` (run `devflow init` if not)
- Tmux available (for agent launching)
- Git flow branches configured (`main` and `develop` exist)

## Workflow

### Starting a phase

When the user wants to start work on a phase:

```bash
devflow start --phase N --agent <agent> --monitor
```

- `--phase N`: phase number
- `--agent`: claude, omx, codex, or opencode (default: claude)
- `--monitor`: spawns a background watcher that auto-advances when agent exits

**Agent selection:** Use the agent configured in `.devflow.yaml` if present. Otherwise default to `claude`. If the user explicitly names an agent, use that.

**Example:**
```
User: "work on Phase 3"
→ devflow start --phase 3 --agent claude --monitor
→ Report: "Phase 3 started. Claude launched in tmux:devflow-project-03. Monitor PID 12345 active."
```

### Checking status

When the user asks about progress:

```bash
devflow status
```

Report the step, phase, agent, and whether the agent is still running.

### Shipping

When the user wants to create a release:

```bash
devflow ship
```

This bumps the version and creates a release branch. It does NOT merge or push — that requires manual confirmation.

### Bootstrapping a project

If a project doesn't have `.devflow.yaml`:

```bash
devflow init
```

This creates the config file with sensible defaults. Review and adjust before starting phases.

## What DevFlow handles automatically

| Step | What happens |
|---|---|
| **Branch** | Creates `feature/phase-NN-name` from `develop` |
| **Launch** | Starts agent in detached tmux session |
| **Monitor** | Background process watches tmux session |
| **Advance** | When agent exits: verify → docs → ship → clean |
| **Version** | Bumps semver + git build number |
| **Release** | Creates release branch, merges to main + develop |
| **Cleanup** | Deletes merged feature/release branches |

## Configuration

Projects configure DevFlow via `.devflow.yaml` (git-tracked):

```yaml
agent: claude              # default agent
version:
  scheme: semver
  file: Cargo.toml          # or pyproject.toml, package.json
  field: package.version    # dotted path to version field
  build_number: git
automation:
  auto_branch: true
  auto_verify: true
  auto_docs: true
  auto_version: patch
  auto_ship: false          # false = confirm before release
  auto_cleanup: true
git_flow:
  main: main
  develop: develop
  feature_prefix: feature/
```

## Recovery

If a monitor dies or the machine reboots:

```bash
devflow status              # check current state
devflow check               # manually advance if agent exited
devflow start --phase N --monitor  # re-launch if needed
```

## Pitfalls

- **Agents take time** — don't poll `devflow check` in a tight loop. The monitor handles this.
- **Tmux sessions persist** — `devflow check` cleans up when the session dies.
- **Sandbox limitations** — agents running inside sandboxes (Codex) can't use tmux themselves. This is fine; DevFlow runs on the host.
- **Git state** — ensure `main` and `develop` branches exist before starting. `devflow init` doesn't create them.
