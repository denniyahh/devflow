# Architecture Overview

DevFlow is a two-crate Cargo workspace written in Rust. It is a stateful
orchestrator around existing coding agents and GSD workflow commands, not an
agent implementation itself.

## Crate Responsibilities

| Crate | Kind | Responsibility |
|-------|------|----------------|
| `devflow-core` | Library | State, agent adapters/results, Git/worktrees, gates, evidence history, recovery, configuration |
| `devflow-cli` | Binary | Clap command parser, stage transitions, gate commands, terminal output |

The split keeps workflow logic testable and independent of both the CLI surface and any individual coding agent. All terminal formatting lives in the binary; the library returns structured data.

## Key Principles

1. **Three explicit stages** — `Plan → Code → Validate`; no stage can claim success without evidence, and only a `pass` verdict from Validate finishes a run.
2. **Worktree isolation by default** — agents run in `.worktrees/phase-NN/` while state and captures remain in the primary checkout.
3. **Detached monitoring** — a monitor owns the agent process, writes its captures and exit code, then invokes the next transition.
4. **Layered completion evidence** — configured external probes run before agent-controlled markers, exit codes, and Git heuristics.
5. **Human gates are files** — a durable request/response/ack protocol allows a workflow to survive terminal restarts and notify other tools.
6. **The operator integrates** — a passing Validate ends the run and keeps the worktree and the `feature/phase-NN` branch; DevFlow does not merge, tag, or open a pull request.

## See Also

- [State Machine](state-machine.md) — Step transitions and serialization
- [Agent Model](agent-model.md) — Agent trait, adapters, and evaluation
