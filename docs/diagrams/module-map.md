# Module Map

Internal dependency graph of `devflow-core` modules.

## Core Module Dependencies

```mermaid
graph TD
    subgraph "Entry Points"
        MAIN["main.rs<br/>(CLI)"]
        LIB["lib.rs<br/>(Public API)"]
    end

    subgraph "State Layer"
        STATE["state.rs<br/>State struct + Step enum"]
        WORKFLOW["workflow.rs<br/>State persistence + transitions"]
    end

    subgraph "Agent Layer"
        AGENT["agent.rs<br/>Agent trait"]
        CLAUDE["agents/claude.rs<br/>Claude adapter"]
        CODEX["agents/codex.rs<br/>Codex adapter"]
        OPENCODE["agents/opencode.rs<br/>OpenCode adapter"]
        AGENT_RESULT["agent_result.rs<br/>3-layer evaluation"]
    end

    subgraph "Git Layer"
        GIT["git.rs<br/>Branch/release ops"]
        SHIP["ship.rs<br/>Version bump + PR"]
        WORKTREE["worktree.rs<br/>Worktree isolation"]
    end

    subgraph "Pipeline Layer"
        GATES["gates.rs<br/>Auto/manual gates"]
        MONITOR["monitor.rs<br/>Daemon process"]
    end

    subgraph "Config Layer"
        CONFIG["config.rs<br/>YAML parsing"]
        VERSION["version.rs<br/>SemVer ops"]
        PROMPT["prompt.rs<br/>Shared prompts"]
    end

    subgraph "Operations Layer"
        RECOVER["recover.rs<br/>State recovery"]
        HOOKS["hooks.rs<br/>Lifecycle hooks"]
        LOCK["lock.rs<br/>Concurrency"]
    end

    MAIN --> LIB
    LIB --> STATE
    LIB --> AGENT
    LIB --> GIT
    LIB --> SHIP
    LIB --> CONFIG
    LIB --> MONITOR
    LIB --> PROMPT

    STATE --> WORKFLOW
    WORKFLOW --> STATE

    AGENT --> CLAUDE
    AGENT --> CODEX
    AGENT --> OPENCODE

    STATE --> AGENT_RESULT
    STATE --> GATES
    STATE --> MONITOR

    GIT --> SHIP
    GIT --> WORKTREE

    CONFIG --> VERSION
    CONFIG --> PROMPT

    RECOVER --> STATE
    RECOVER --> GIT

    HOOKS --> STATE
    LOCK --> STATE
```

## Dependency Rules

```mermaid
flowchart TD
    subgraph "Allowed"
        A1["CLI → Core ✓"]
        A2["Module → Module ✓"]
        A3["Module → std lib ✓"]
    end

    subgraph "Forbidden"
        F1["Core → CLI ✗"]
        F2["Circular deps ✗"]
        F3["Module → external ✗<br/>(except git2, serde, tracing)"]
    end

    style A1 fill:#2e7d32,color:#fff
    style A2 fill:#2e7d32,color:#fff
    style A3 fill:#2e7d32,color:#fff
    style F1 fill:#c62828,color:#fff
    style F2 fill:#c62828,color:#fff
    style F3 fill:#c62828,color:#fff
```

## External Dependencies

| Crate | Purpose | Module |
|-------|---------|--------|
| `clap` | CLI argument parsing | `devflow-cli` |
| `serde` / `serde_json` | State serialization | `workflow.rs` |
| `git2` | Git operations | `git.rs` |
| `tracing` / `tracing-subscriber` | Structured logging | all modules |
| `tempfile` | Test isolation | tests |
| `chrono` | Timestamps | `state.rs` |

## File Sizes

| File | Lines | Purpose |
|------|-------|---------|
| `main.rs` | ~600 | CLI dispatch |
| `git.rs` | ~400 | Git operations |
| `monitor.rs` | ~350 | Daemon |
| `ship.rs` | ~300 | Release workflow |
| `workflow.rs` | ~250 | State persistence |
| `state.rs` | ~200 | State machine |
| `agent.rs` | ~150 | Agent trait |
| `config.rs` | ~200 | YAML config |
| `worktree.rs` | ~150 | Worktree ops |
| `agent_result.rs` | ~100 | 3-layer eval |
| `gates.rs` | ~80 | Stage gates |
| `prompt.rs` | ~80 | Shared prompts |
| `version.rs` | ~80 | SemVer |
| `recover.rs` | ~80 | State recovery |
| `hooks.rs` | ~60 | Lifecycle hooks |
| `lock.rs` | ~60 | Concurrency |
