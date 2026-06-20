# Git & Ship Model

DevFlow implements a Git-flow branching strategy with automated release management.

## Branch Strategy

- **`develop`** — Integration branch, all feature branches merge here
- **`main`** — Production branch, only release branches merge here (via `--no-ff`)
- **`feature/phase-N`** — Per-phase development branches
- **`release/vX.Y.Z`** — Release preparation branches

## Git Operations

### Feature Lifecycle

```
devflow start → git checkout -b feature/phase-N develop
agent works   → commits pushed to feature/phase-N
devflow ship  → git merge feature/phase-N into develop (--no-ff)
             → delete feature/phase-N
```

### Release Lifecycle

```
devflow ship  → bump version in working tree
             → git checkout -b release/vX.Y.Z HEAD
             → commit version bump
             → git merge release/vX.Y.Z into main (--no-ff)
             → git tag vX.Y.Z
             → git merge release/vX.Y.Z into develop (--no-ff)
             → delete release/vX.Y.Z
             → push main + develop + tags
             → gh pr create (release → main)
```

!!! important "Release branch cut from HEAD"
    The release branch is cut from the **current `HEAD`** (which includes the version bump commit), not from `develop`. This keeps commits unique to the shipped branch in the release.

## See Also

- [Ship Flow diagram](../diagrams/ship-flow.md) — Visual sequence
