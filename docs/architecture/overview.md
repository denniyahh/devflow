# Architecture Overview

DevFlow is a two-crate Cargo workspace written in Rust.

## Crate Responsibilities

| Crate | Kind | Responsibility |
|-------|------|----------------|
| `devflow-core` | Library | State machine, config, git/ship orchestration, versioning, agent adapters, monitor daemon, worktrees, recovery |
| `devflow-cli` | Binary | Clap command parser, terminal output, user interaction |

The split keeps workflow logic testable and independent of both the CLI surface and any individual coding agent. All terminal formatting lives in the binary; the library returns structured data.

## Key Principles

1. **Agent-agnostic** — All coding agents implement the same `Agent` trait. Adding a new agent requires no changes to the core workflow.
2. **State machine driven** — The workflow is a deterministic step sequence with clear transitions and serializable state.
3. **Worktree isolation** — Agents run in isolated git worktrees under `.worktrees/phase-NN/`, preventing cross-phase contamination.
4. **Monitor daemon** — A detached child process owns the agent lifecycle, eliminating the need for cron, polling, or tmux.
5. **Three-layer evaluation** — Agent completion is evaluated through a tiered system: `DEVFLOW_RESULT` marker → exit code + commits → commit heuristic.
6. **Shared prompts** — All agents receive identical prompt text; they differ only in CLI flags.

## See Also

- [State Machine](state-machine.md) — Step transitions and serialization
- [Agent Model](agent-model.md) — Agent trait, adapters, and evaluation
- [Git & Ship Model](git-ship.md) — Git-flow orchestration
