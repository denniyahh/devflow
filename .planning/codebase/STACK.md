# DevFlow — Technology Stack

> Generated: 2026-06-17 | Mapper: gsd-map-codebase (sequential)

## Language & Runtime

| Component | Version | Notes |
|---|---|---|
| **Rust** | edition 2024 | Workspace with 2 crates |
| **rustc** | nightly (1.94.0) | Required for edition 2024 |
| **Binary** | single static binary | `target/release/devflow` (~20MB) |

## Build System

- **Cargo** workspace resolver v2
- Release profile: optimized (`--release`)
- No build.rs scripts, no proc macros
- No feature flags currently in use

## Core Dependencies (`devflow-core`)

| Crate | Version | Purpose |
|---|---|---|
| `serde` | 1.x (derive) | `.devflow.yaml` parsing, state.json serialization |
| `serde_json` | 1.x | JSON state file read/write |
| `thiserror` | 2.x | Error type derivation for all modules |
| `tracing` | 0.1.x | Structured logging (library side) |

**Notable: zero external dependencies beyond the Rust stdlib + serde/thiserror/tracing.** No HTTP clients, no async runtime, no regex, no shell libraries.

## CLI Dependencies (`devflow-cli`)

| Crate | Version | Purpose |
|---|---|---|
| `clap` | 4.x (derive) | CLI argument parsing |
| `devflow-core` | path | Internal library |
| `tracing-subscriber` | 0.3.x | Log output formatting |
| `thiserror` | 2.x | CLI error types |

## System Dependencies (Required at Runtime)

| Tool | Required For | Check |
|---|---|---|
| `tmux` | Agent session management | `tmux -V` |
| `git` | Branch management, versioning | `git --version` |
| Agent CLI (one of) | Actually running work | `claude`, `codex`, `omx`, `opencode` |

## Configuration

| File | Format | Location |
|---|---|---|
| `.devflow.yaml` | YAML | Project root (git-tracked) |
| `.devflow/state.json` | JSON | Project root (gitignored, runtime state) |
| `.devflow/state.lock` | empty file | Mutex for concurrent `devflow check` |

## Shipping

- **Brew**: symlink `~/.linuxbrew/bin/devflow` → `target/release/devflow`
- **Manual install**: `cp target/release/devflow /usr/local/bin/`
- **Future**: `cargo install devflow` or `curl | sh` (v1.0.0)
