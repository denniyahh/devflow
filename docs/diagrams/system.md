# System Architecture

C4 Container diagram showing DevFlow's runtime components and their relationships.

```mermaid
C4Context
    title DevFlow System Architecture

    Person(dev, "Developer")

    System_Boundary(df, "DevFlow") {
        Container(cli, "devflow-cli", "Rust", "CLI parser, user I/O")
        Container(core, "devflow-core", "Rust", "State machine, agents, git, ship")
    }

    System_Ext(agents, "AI Agents", "Claude, Codex, OpenCode")
    System_Ext(gh, "GitHub", "PRs, CI, remote repo")

    Rel(dev, cli, "CLI commands")
    Rel(cli, core, "fn calls")
    Rel(core, agents, "launch & monitor")
    Rel(core, gh, "PRs via gh CLI")
```

## Crate Structure

```mermaid
flowchart LR
    CLI["devflow-cli<br/>(Binary)"]

    subgraph CORE["devflow-core (Library)"]
        direction TB
        subgraph state["State"]
            STATE["state.rs"]
            WORKFLOW["workflow.rs"]
        end
        subgraph agent["Agents"]
            AGENT["agent.rs"]
            AGENT_RESULT["agent_result.rs"]
        end
        subgraph git_grp["Git & Ship"]
            GIT["git.rs"]
            SHIP["ship.rs"]
            WORKTREE["worktree.rs"]
        end
        subgraph pipeline["Pipeline"]
            GATES["gates.rs"]
            MONITOR["monitor.rs"]
        end
        subgraph config["Config"]
            CONFIG["config.rs"]
            VERSION["version.rs"]
            PROMPT["prompt.rs"]
        end
        subgraph ops["Operations"]
            RECOVER["recover.rs"]
            HOOKS["hooks.rs"]
            LOCK["lock.rs"]
        end
    end

    CLI --> CORE
```

## Module Groups

| Group | Modules | Purpose |
|-------|---------|---------|
| **State** | `state.rs`, `workflow.rs` | Step transitions + JSON persistence |
| **Agents** | `agent.rs`, `agent_result.rs` | Agent trait + 3-layer evaluation |
| **Git & Ship** | `git.rs`, `ship.rs`, `worktree.rs` | Branch/release ops, version bump, worktree isolation |
| **Pipeline** | `gates.rs`, `monitor.rs` | Auto/manual gates, daemon process |
| **Config** | `config.rs`, `version.rs`, `prompt.rs` | YAML parsing, SemVer, shared prompts |
| **Operations** | `recover.rs`, `hooks.rs`, `lock.rs` | Recovery, lifecycle hooks, concurrency |

## Data Flow

```mermaid
flowchart TB
    subgraph Inputs
        CLI_ARGS["CLI Args<br/>--phase, --agent, --mode"]
        CONFIG_FILE[".devflow.yaml<br/>Project config"]
        STATE_FILE[".devflow/state.json<br/>Runtime state"]
    end

    subgraph "Core Processing"
        PARSE["Config::load()<br/>Parse YAML + CLI"]
        ADVANCE["State::advance()<br/>Step transitions"]
        EXECUTE["Step execution<br/>Branch/Agent/Verify/Docs/Ship"]
    end

    subgraph Outputs
        AGENT_OUTPUT["Agent stdout<br/>Capture files"]
        EXIT_CODE["Exit code<br/>.devflow/phase-NN-exit"]
        PR["GitHub PR<br/>Release branch"]
        DOCS["Documentation<br/>Generated/updated"]
    end

    CLI_ARGS --> PARSE
    CONFIG_FILE --> PARSE
    PARSE --> ADVANCE
    STATE_FILE <--> ADVANCE
    ADVANCE --> EXECUTE
    EXECUTE --> AGENT_OUTPUT
    EXECUTE --> EXIT_CODE
    EXECUTE --> PR
    EXECUTE --> DOCS
```

## Key Design Patterns

| Pattern | Implementation |
|---------|---------------|
| **Agent trait** | `Agent` trait with `name()`, `exec_command()`, `completion_signal_detected()` |
| **State machine** | Deterministic `Step::next()` transitions, serialized to JSON |
| **Strategy pattern** | Agent adapters isolated behind trait, selected by `AgentKind` enum |
| **Observer** | Monitor daemon watches agent process, triggers `devflow check` on exit |
| **Command pattern** | Each CLI subcommand delegates to a core function with clear inputs/outputs |
