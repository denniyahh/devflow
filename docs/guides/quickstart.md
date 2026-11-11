# Quick Start

Get DevFlow running in your project.

## Installation

```bash
# Build and install from source
git clone https://github.com/denniyahh/devflow.git
cd devflow
cargo install --path crates/devflow-cli
```

## Start a Phase

No initialization step or `init` command is required. The optional
`devflow.toml` only tunes reliability behavior; run `devflow start` directly
from your project root:

```bash
cd your-project
devflow start --phase 3 --agent claude --mode auto
```

This creates a feature branch (in its own git worktree by default), launches
the agent, and persists that phase's progress to
`.devflow/state-03.json` (state is per-phase, not a single shared file).

## Check Status

```bash
devflow status
```

## Finishing

A passing Validate ends the run. DevFlow keeps the worktree and the
`feature/phase-NN` branch and does not merge, tag or open a pull request:
integrating the branch is your step. In `--mode supervise` the run pauses at
a human gate before it finishes:

```bash
devflow gate list
devflow gate approve 3 --note "lgtm"
```

See the [operator reference](https://github.com/denniyahh/devflow/blob/develop/OPERATIONS.md)
for the full gate-answering reference.

## Prerequisites

- **Rust** stable (build from source)
- **git** 2.30+
- **gh CLI** 2.0+ (`devflow doctor` checks for it; no DevFlow command runs it
  since the Ship stage was removed)
- An installed AI agent (Claude Code, Codex, or OpenCode)

Verify your setup:

```bash
devflow doctor
```
