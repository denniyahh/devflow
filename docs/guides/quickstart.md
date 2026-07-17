# Quick Start

Get DevFlow running in your project.

## Installation

```bash
# One-command install
curl -fsSL https://raw.githubusercontent.com/denniyahh/devflow/main/scripts/install.sh | bash

# Build from source
git clone https://github.com/denniyahh/devflow.git
cd devflow
cargo install --path crates/devflow-cli
```

## Start a Phase

No initialization step is required — DevFlow has no config file and no
`init` command. Run `devflow start` directly from your project root:

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

## Shipping

Shipping is not a separate command you run — Ship is the pipeline's final
stage, driven automatically once Validate passes. It always pauses at a
human gate before merging:

```bash
devflow gate list
devflow gate approve 3 --note "lgtm"
```

See [OPERATIONS.md](../../OPERATIONS.md) for the full gate-answering
reference.

## Prerequisites

- **Rust** stable (build from source)
- **git** 2.30+
- **gh CLI** 2.0+ (for PR creation)
- An installed AI agent (Claude Code, Codex, or OpenCode)

Verify your setup:

```bash
devflow doctor
```
