# Ship Flow

Git-flow release process from version bump to merged PR.

## Release Workflow

```mermaid
sequenceDiagram
    participant User
    participant CLI as devflow
    participant Git as Git Repo
    participant GitHub

    User->>CLI: devflow ship

    Note over CLI: Phase complete, verification passed

    CLI->>CLI: Bump version (SemVer)
    CLI->>Git: Commit version bump to feature/phase-N

    CLI->>Git: release_start(v1.2.0)
    Note over Git: git checkout -b release/v1.2.0 HEAD
    Note over Git: Cuts release from current HEAD (not develop)

    CLI->>Git: release_finish(v1.2.0)
    Git->>Git: Merge release → main (--no-ff)
    Git->>Git: Tag v1.2.0
    Git->>Git: Merge release → develop (--no-ff)
    Git->>Git: Delete release/v1.2.0

    CLI->>GitHub: Push main + develop + tags
    CLI->>GitHub: Create PR (release → main)

    CLI-->>User: "Shipped v1.2.0 — PR #42"

    User->>CLI: devflow confirm
    CLI->>GitHub: Merge PR

    Note over CLI: State → Cleaning → Idle
```

## Branch Strategy

```mermaid
gitGraph
    commit id: "main: v1.1.0"
    branch develop
    checkout develop
    commit id: "merge feat-A"
    commit id: "merge feat-B"
    branch feature/phase-3
    checkout feature/phase-3
    commit id: "Phase 3: impl"
    commit id: "Phase 3: tests"
    checkout develop
    merge feature/phase-3
    commit id: "version bump"
    branch release/v1.2.0
    checkout release/v1.2.0
    checkout main
    merge release/v1.2.0 tag: "v1.2.0"
    checkout develop
    merge release/v1.2.0
```

## Version Flow

```mermaid
flowchart LR
    subgraph "Feature Work"
        F1["feature/phase-1"] --> D1["develop"]
        F2["feature/phase-2"] --> D2["develop"]
        F3["feature/phase-3"] --> D3["develop"]
    end

    subgraph "Release"
        D3 -->|"version bump + cut"| R["release/v1.2.0"]
        R -->|"--no-ff merge"| M["main"]
        R -->|"--no-ff merge"| D4["develop"]
        M -->|"tag v1.2.0"| TAG["🏷️ v1.2.0"]
    end
```

## Ship States

| State | Action | Reversible? |
|-------|--------|-------------|
| `Shipped` | Version bumped, release branch created, PR opened | Yes → `devflow rejectpr` |
| `Confirmed` | PR merged to main, tags pushed | No (history is immutable) |
| `Rejected` | PR closed, release branch deleted, version reverted | No (clean slate) |

## Last-Ship Recovery

`devflow ship` writes `.devflow/last-ship.json`:

```json
{
  "version": "1.2.0",
  "pr_number": 42,
  "feature_branch": "feature/phase-3",
  "release_branch": "release/v1.2.0"
}
```

This enables `devflow rejectpr` to unwind a shipped-but-unmerged release.
