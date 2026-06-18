---
name: devflow
description: Drive DevFlow automated development workflows — start, monitor, and ship phased agent-driven work
category: software-development
triggers:
  - "start phase"
  - "devflow"
  - "feature branch"
  - "ship phase"
---

# DevFlow — Hermes Skill

## What is DevFlow?
DevFlow is an agent-agnostic development workflow automation CLI written in Rust. It manages the mechanical parts of AI-driven development: branching, agent launching, monitoring, verification, documentation, version bumping, and shipping.

## When to Use
Use this skill when:
- User wants to start work on a numbered phase in a DevFlow project
- User asks "what's the status of phase X?"
- User wants to ship/merge a completed phase
- You detect a `.devflow.yaml` in a project root

## Project Detection
Check for `.devflow.yaml` in the project root. If present, the project uses DevFlow. Read the config to understand automation settings.

## Commands

### `devflow start --phase N --agent <agent> [--monitor]`
Start work on phase N with the specified agent. Recommended: always use `--monitor` so the agent runs in background with auto-advancement.

Supported agents: `claude`, `codex`, `omx`, `opencode`

### `devflow status`
Show current workflow state: step, phase, agent, PID, running status.

### `devflow check`
Poll and advance the state machine if the agent has exited.

### `devflow ship`
Create release branch, bump version, and prepare for PR.

### `devflow config`
Show effective DevFlow configuration.

## Workflow
1. When user says "work on phase N" → `devflow start --phase N --agent claude --monitor`
2. Report what was launched: "Phase N started — Claude Code (PID X), monitor PID Y"
3. Monitor auto-advances when agent exits — no need to poll
4. When done, report: "Phase N complete — X commits, Y files changed"
5. Merge to develop: `git checkout develop && git merge --no-ff feature/phase-0N`

## Phase Context
Each phase has a `.planning/phases/0N-*/CONTEXT.md` file with detailed task lists.
Agents read these automatically via the prompt.

## Verification
Phase completion should include:
- `cargo test` passes
- `cargo clippy -- -D warnings` clean
- `cargo fmt -- --check` clean
- Descriptive commits per sub-task
