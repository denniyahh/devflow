# System Architecture

DevFlow separates operator interaction from reusable workflow mechanics.

```mermaid
flowchart LR
    U[Developer] --> CLI[devflow CLI]
    CLI --> CORE[devflow-core]
    CORE --> WT[Git worktree]
    CORE --> MON[Detached monitor]
    MON --> AG[Claude, Codex, or OpenCode]
    AG --> CAP[Phase captures]
    MON --> ADV[Advance phase]
    ADV --> GATE[Durable gate files]
    U --> GATE
    CORE --> GIT[Git branches and tags]
```

## Components

| Component | Responsibility |
|---|---|
| `devflow-cli` | Commands, stage transitions, status, logs, and gate responses |
| `devflow-core` | State, prompts, adapters, result evaluation, Git operations, hooks, history, and recovery |
| Detached monitor | Owns the agent process, captures output, records exit status, and advances its phase |
| `.devflow/` | Per-phase state, gate request/response files, event log, captures, and archived evidence |
| `.worktrees/` | Isolated execution directories for phase agents |

The primary checkout owns workflow state and terminal Git mutations. The
worktree is the execution location for an agent, not a second source of truth.

## Data Flow

```mermaid
flowchart TB
    START[Start phase] --> STATE[state-NN.json]
    START --> WORKTREE[Create worktree]
    WORKTREE --> PROMPT[Stage prompt]
    PROMPT --> AGENT[Agent process]
    AGENT --> CAPTURE[stdout, stderr, exit]
    CAPTURE --> EVAL[Evidence evaluation]
    EVAL -->|pass| NEXT[Next stage]
    EVAL -->|fail or gaps| GATE[Gate or Code loop]
    GATE -->|approval| NEXT
```
