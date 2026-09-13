# DevFlow — Project Instructions

Deliberately minimal. Global agent rules live in `~/.config/agents/AGENTS.md`; this file holds only constraints specific to how *this* repository is built, verified, and worked on.

## Crate Topology
- `crates/devflow-core`: Core state machine, pipeline engines, and workflow abstractions (`lib`).
- `crates/devflow-cli`: CLI interface and driver commands (`bin: devflow`).

## Canonical Verification & Test Commands
Always run targeted package tests first, then full workspace validation:
- Test `devflow-core` library: `cargo test -p devflow-core --lib`
- Test `devflow` CLI binary: `cargo test -p devflow --bin devflow`
  *(Note: `devflow` is binary-only; running `cargo test -p devflow --lib` verifies nothing and fails).*
- Full check suite (formatting, clippy, tests): `scripts/check.sh all`
- Container parity check: `scripts/check-in-container.sh`

## Workspace & Worktrees
- Development phases run inside dedicated worktrees under `.worktrees/phase-N`.
- Use `scripts/phase-worktree.sh <phase>` to initialize or attach to a phase worktree.
- Never run concurrent git operations on the main checkout while an executor holds the working tree.

## Development Setup Checklist
- When modifying CI workflows, hooks, devcontainers, or tooling dependencies, keep `.planning/user/DEV-SETUP-CHECKLIST.md` updated in the same commit (warned by `scripts/hooks/post-commit`).
