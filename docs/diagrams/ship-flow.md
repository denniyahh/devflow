# Ship Flow

Ship is a stage with a mandatory human merge gate, not a standalone command.

```mermaid
sequenceDiagram
    participant V as Validate
    participant S as Ship agent
    participant R as REVIEW.md
    participant H as Human gate
    participant G as Git develop

    V->>S: pass verdict
    S->>R: run severity-classified review
    alt Critical finding
        R-->>S: review failure
        S-->>V: return to Code
    else No Critical finding
        S->>H: write Ship approval request
        H-->>S: approve
        S->>G: merge feature branch
        S->>G: write version and tag
        S->>G: best-effort cleanup
    end
```

## Terminal Guarantees

- A failed merge stops terminal bookkeeping and reopens an actionable Ship gate.
- The version is computed after the feature branch has been merged into `develop`.
- A missing feature branch is not treated as proof of a merge.
- Branch cleanup never force-deletes unmerged work.

PR creation belongs to the GSD Ship workflow. DevFlow records the gate and
proves the local terminal Git effects.
