# Agent Lifecycle

How DevFlow launches, monitors, and evaluates AI coding agents.

## Lifecycle Sequence

```mermaid
sequenceDiagram
    actor User
    participant CLI as devflow-cli
    participant Core as devflow-core
    participant Monitor as Monitor Daemon
    participant Agent as AI Agent
    participant Git as Git Repo
    participant FS as Filesystem

    User->>CLI: devflow start --phase 3 --agent claude
    CLI->>Core: start(phase=3, agent=Claude)
    Core->>Git: feature_start("feature/phase-3")
    Git-->>Core: branch created

    alt Monitor Mode (--monitor)
        Core->>Monitor: spawn detached process
        Monitor->>Agent: launch claude (headless)
        Monitor-->>CLI: "Agent launched, monitor PID: 12345"
        CLI-->>User: "Agent running in background"
        Note over Monitor,Agent: CLI exits, monitor owns agent lifecycle

        loop Poll
            Monitor->>Agent: check process alive
        end

        Agent->>Git: commits changes
        Agent->>FS: writes stdout capture
        Agent-->>Monitor: process exits (PID gone)
        Monitor->>FS: record exit code to .devflow/phase-3-exit
        Monitor->>Core: devflow check (advance state)
    else Direct Mode
        Core->>Agent: launch claude (headless)
        Core->>Agent: poll for completion
        Agent->>Git: commits changes
        Agent-->>Core: process exits
        Core->>FS: record exit code
    end

    Core->>Core: evaluate completion (3-layer)
    Core->>Core: advance state → Verifying → Docsing → Shipping
```

## Three-Layer Evaluation

```mermaid
flowchart TD
    AGENT_EXIT["Agent Process Exits"]

    L1{"DEVFLOW_RESULT<br/>marker in stdout?"}
    L1_SUCCESS["✓ SUCCESS<br/>Marker says 'success'"]
    L1_FAIL["✗ FAILED<br/>Marker says 'failed'"]

    L2{"Exit code<br/>known?"}
    L2_SUCCESS{"exit=0 AND<br/>commits>0?"}
    L2_FAIL{"exit=0 AND<br/>commits=0?"}
    L2_HALT["⚠ HALT<br/>'no work done'"]
    L2_CRASH["✗ FAILED<br/>'agent failed'"]

    L3{"Commits on<br/>feature branch?"}
    L3_WARN["⚠ PROBABLE SUCCESS<br/>Warning surfaced"]
    L3_FAIL["✗ FAILED<br/>No commits, no evidence"]

    AGENT_EXIT --> L1
    L1 -->|"Yes: parse JSON"| L1_SUCCESS
    L1 -->|"Yes: parse JSON"| L1_FAIL
    L1 -->|"No"| L2
    L2 -->|"Yes"| L2_SUCCESS
    L2 -->|"Yes"| L2_FAIL
    L2 -->|"Unknown"| L3
    L2_SUCCESS -->|"Yes"| L1_SUCCESS
    L2_FAIL -->|"Yes"| L2_HALT
    L2_FAIL -->|"exit≠0"| L2_CRASH
    L3 -->|"Yes"| L3_WARN
    L3 -->|"No"| L3_FAIL

    style L1_SUCCESS fill:#2e7d32,color:#fff
    style L1_FAIL fill:#c62828,color:#fff
    style L2_HALT fill:#f57f17,color:#000
    style L2_CRASH fill:#c62828,color:#fff
    style L3_WARN fill:#f57f17,color:#000
    style L3_FAIL fill:#c62828,color:#fff
```

## Agent Trait

All agents implement a shared interface:

```rust
pub trait Agent {
    fn name(&self) -> &str;
    fn kind(&self) -> AgentKind;
    fn exec_command(&self, phase: u32) -> (String, Vec<String>);
    fn completion_signal_detected(&self, output: &str) -> bool;
}
```

| Method | Purpose |
|--------|---------|
| `name()` | Human-readable name ("Claude Code") |
| `kind()` | Enum variant for matching/dispatch |
| `exec_command()` | Returns (program, args) for headless launch |
| `completion_signal_detected()` | Scans agent output for completion markers |

## DEVFLOW_RESULT Protocol

Agents signal completion by emitting a JSON marker to stdout:

```json
DEVFLOW_RESULT: {"status": "success", "commits": 3, "summary": "added tests for monitor module"}
```
