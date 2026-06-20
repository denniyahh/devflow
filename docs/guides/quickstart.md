# Quick Start

Get DevFlow running in your project.

## Installation

```bash
# One-command install
curl -fsSL https://raw.githubusercontent.com/denniyahh/devflow/main/scripts/install.sh | bash

# Build from source
cargo install devflow
```

## Initialize a Project

```bash
cd your-project
devflow init
```

This creates `.devflow/state.json` and `.devflow.yaml` with sensible defaults.

## Start a Phase

```bash
devflow start --phase 3 --agent claude --mode auto
```

## Check Status

```bash
devflow status
```

## Ship a Completed Phase

```bash
devflow ship
```

## Prerequisites

- **Rust** stable (build from source)
- **git** 2.30+
- **gh CLI** 2.0+ (for PR creation)
- An installed AI agent (Claude Code, Codex, or OpenCode)

Verify your setup:

```bash
devflow doctor
```
