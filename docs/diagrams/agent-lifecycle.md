# Agent Lifecycle

Each phase uses a detached monitor so the agent can finish after the invoking
terminal exits.

```mermaid
sequenceDiagram
    actor User
    participant CLI as devflow
    participant Monitor
    participant Agent
    participant Files as .devflow

    User->>CLI: start phase 16
    CLI->>Files: write state-16.json
    CLI->>Monitor: spawn with phase identity
    Monitor->>Agent: launch stage prompt
    Agent-->>Monitor: exit
    Monitor->>Files: stdout, stderr, exit, PID state
    Monitor->>CLI: advance --phase 16
    CLI->>Files: archive prior evidence
```

## Completion Evidence

```mermaid
flowchart TD
    A[Captured result] --> X{Reviewed external probe?}
    X -->|fails| F[Failed]
    X -->|passes or absent| M{Native result or marker?}
    M -->|failed| F
    M -->|success| S[Success]
    M -->|absent| E{Exit code and Git evidence}
    E -->|failure| F
    E -->|sufficient| S
    E -->|incomplete| U[Unknown: never silently success]
```

An external verification probe is allowed only when its exact command was
reviewed and supplied by the parent process. Validate additionally requires a
`pass` or `gaps` verdict; Ship reads its review artifact before it can proceed.
