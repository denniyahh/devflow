# Technology Stack

**Analysis Date:** 2026-07-17

## Languages

**Primary:**
- Rust stable (edition 2024) - Core library and CLI binary. Pinned via `rust-toolchain.toml` with components: clippy, rustfmt

**Secondary:**
- Shell (POSIX sh/bash) - Agent process execution, monitor daemon, git operations

## Runtime

**Environment:**
- Rust stable toolchain via `rustup`
- Requires: git 2.30+, POSIX shell (sh/bash), cargo

**Package Manager:**
- Cargo 1.91.0+ (Rust package manager)
- Lockfile: `Cargo.lock` present and committed
- Workspace structure: `Cargo.toml` (root) with members `crates/devflow-core` and `crates/devflow-cli`

## Frameworks

**Core:**
- Standard library (no web framework; this is a CLI tool)

**CLI:**
- `clap` 4.x - CLI argument parsing with derive macros (flag/option definitions in `crates/devflow-cli/src/main.rs`)

**Serialization:**
- `serde` 1.x with derive - Structure serialization
- `serde_json` 1.x - JSON parsing and emission (used for state, gates, agent results)

**Error Handling:**
- `thiserror` 2.x - Custom error types with `#[derive(Error)]` (all modules export error enums)

**Logging:**
- `tracing` 0.1 - Structured logging instrumentation
- `tracing-subscriber` 0.3 - Log output formatting and filtering (`json` and `env-filter` features enabled in CLI)

**Testing:**
- Built-in Rust test framework via `#[cfg(test)]` and `#[test]`
- `tempfile` 3.x - Temporary directory fixtures for tests

**Build/Dev:**
- `cargo-clippy` - Linting (enforced: `-D warnings` in CI)
- `cargo-fmt` - Code formatting (enforced in CI)

## Key Dependencies

**Critical:**
- `serde`/`serde_json` 1.x - State machine state, gate protocol, agent result parsing, event serialization
- `clap` 4.x - CLI interface (all command definitions, subcommands, options)
- `thiserror` 2.x - Error propagation across modules
- `tracing` 0.1 / `tracing-subscriber` 0.3 - Observability (all major operations logged)

**Infrastructure:**
- `libc` 0.2 - POSIX syscalls (process spawning, signal handling, terminal control)

**Development:**
- `tempfile` 3.x - Test fixtures (phase state isolation, worktree simulation)

## Configuration

**Environment:**
Configured entirely via CLI flags and environment variables (no config file required).

**Key environment variables:**
- `DEVFLOW_GATE_NOTIFY_CMD` - Custom shell command fired when a gate is written (optional; used for external notifications)
- `DEVFLOW_GATE_TIMEOUT_SECS` - Gate polling timeout, default 7 days (604800 seconds)
- `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` - Shared-checkout lock timeout, default 120s
- `RUST_LOG` - Log verbosity level (default: `info`), compatible with `tracing-subscriber` env-filter
- `DEVFLOW_LOG_FORMAT` - Output format: `plain` (default) or `json`
- `GIT_CONFIG_*` - Scoped git config overrides (used by Codex agent to disable signing inside sandbox)

**Build:**
- `Cargo.toml` - Workspace manifest with shared package metadata and dependency versions
- `Cargo.lock` - Lockfile for reproducible builds
- `rust-toolchain.toml` - Pinned Rust channel and components
- GitHub Actions CI: `.github/workflows/ci.yml` - Test, clippy, fmt checks on push/PR

## Platform Requirements

**Development:**
- Rust stable (latest or via `rust-toolchain.toml`)
- git 2.30+
- POSIX shell (sh/bash)
- cargo 1.70+ (workspace support)

**Production/Runtime:**
- Linux or macOS (tested on ubuntu-latest in CI)
- git 2.30+
- POSIX shell
- Agents: Claude Code, OpenAI Codex, or OpenCode must be installed separately on $PATH

## Build Artifacts

**Binaries:**
- `crates/devflow-cli/src/main.rs` → `devflow` (distributed via `cargo install devflow`)
- Library: `crates/devflow-core/src/lib.rs` → `libdevflow.rlib` (internal dependency of CLI)

**Distribution:**
- Distributed via: `cargo install devflow` (fetches from crates.io)
- Or built from source: `cargo build --release` → `target/release/devflow`
- Installation script: `scripts/install.sh` (curl | bash)

---

*Stack analysis: 2026-07-17*
