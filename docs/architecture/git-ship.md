# Git & Ship Model

DevFlow uses a Git-flow layout and makes the final merge an explicit human
decision.

## Branch Strategy

- **`develop`** — Integration branch, all feature branches merge here
- **`main`** — Production branch, only release branches merge here (via `--no-ff`)
- **`feature/phase-N`** — Per-phase development branches
- **`release/vX.Y.Z`** — supported Git primitives, but not part of the normal CLI Ship path

## Git Operations

### Feature Lifecycle

```
devflow start → feature/phase-NN from develop + isolated worktree
agent work     → commits and captured evidence
Ship approval  → merge feature/phase-NN into develop (--no-ff)
               → compute version and tag
               → best-effort branch cleanup
```

### Ship Approval

```
Validate → Ship runs documentation and changelog hooks. The Ship agent first
produces a severity-classified review; a Critical finding returns work to Code.
When Ship succeeds, DevFlow writes a durable gate request. A human responds
with `devflow gate approve <phase> --stage ship` or rejects it back to Code.

Approval runs terminal hooks in this order:

1. Merge the feature branch into `develop`, proving ancestry first.
2. Write the computed version and create the version tag against post-merge `develop`.
3. Attempt non-force branch cleanup without discarding work.
```

!!! important "Release branch cut from HEAD"
    PR creation is performed by the GSD Ship workflow, not a separate DevFlow `ship` command. DevFlow's role is to preserve the gate and terminal Git proof.

## See Also

- [Ship Flow diagram](../diagrams/ship-flow.md) — Visual sequence
