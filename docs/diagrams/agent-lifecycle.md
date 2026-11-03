# Agent Lifecycle

Each stage of a phase runs under a detached supervisor (the hidden `__monitor`
process) so the agent can finish after the invoking terminal exits. The
supervisor owns the agent, answers liveness and stop requests on its own
socket, and when the agent exits runs the advance in-process: no separate
command is started to advance the phase.

```mermaid
sequenceDiagram
    actor User
    participant CLI as devflow
    participant Sup as Supervisor
    participant Agent
    participant Files as .devflow
    participant Next as Next supervisor

    User->>CLI: start phase 16
    CLI->>Files: write state-16.json
    CLI->>Sup: re-exec as __monitor with phase identity
    Sup->>Agent: launch stage prompt (own process group)
    Sup-->>CLI: socket published, answers ping
    CLI->>Files: record the socket in state-16.json
    Agent-->>Sup: exit
    Sup->>Files: stdout, stderr, exit, PID state
    Sup->>Sup: advance in-process, archive prior evidence
    Sup->>Next: launch the next stage
    Next-->>Sup: socket published
    Sup->>Files: record the next socket, then unlink its own
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
