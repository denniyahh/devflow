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
the run as a systemd user unit named from the project and the phase, and
persists that phase's progress to `.devflow/state-03.json` (state is
per-phase, not a single shared file). `start` returns once the unit is ready;
the run keeps going after your shell exits. With lingering off it ends when
you log out (`loginctl enable-linger` turns lingering on), and a host without
a systemd user manager cannot launch a run.

## Watch and Stop

From any shell:

```bash
devflow status
devflow logs --phase 3 -f      # follow the agent's output
devflow stop --phase 3         # stop the run's unit
devflow resume --phase 3       # continue a stopped run as a new unit
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
- **gh CLI** 2.0+ (`devflow doctor` checks for it; no DevFlow command runs it)
- An installed AI agent (Claude Code, Codex, or OpenCode)
- A systemd user manager (`systemctl --user`), which runs each run's unit

Verify your setup:

```bash
devflow doctor
```
