---
last_mapped_commit: d581384145e82fd8f7091fee6387d70c6a62fff5
last_mapped_at: 2026-09-18
---
# Technology Stack

**Analysis Date:** 2026-09-18

## Languages

**Primary:**

- Rust, edition 2024: all product code in `crates/devflow-core/src/` and `crates/devflow-cli/src/`

**Secondary:**

- POSIX sh / Bash: tooling in `scripts/*.sh`, `scripts/lib/`, and git hooks in `scripts/hooks/` (`pre-commit`, `commit-msg`, `post-commit`, `pre-push`)
- Markdown + MkDocs config: documentation site in `docs/` (`docs/mkdocs.yml`), built output committed in `site/`
- YAML: CI in `.github/workflows/ci.yml` and `.github/workflows/devcontainer.yml`

## Runtime

**Environment:**

- Native binary `devflow` (Linux; the code uses `libc` and shells out to `sh`, `kill`, `date`, `sleep`)
- Toolchain pinned to exact `1.97.1` with `clippy` and `rustfmt` in `rust-toolchain.toml`. Do not switch it to a floating channel.

**Package Manager:**

- Cargo workspace, resolver 2 (`Cargo.toml`)
- Lockfile: `Cargo.lock` is present and committed

## Frameworks

**Core:**

- No application framework. The product is a hand-rolled CLI plus a state-machine library
- `clap` 4 (lock 4.6.1, `derive` feature): CLI argument parsing in `crates/devflow-cli/src/main.rs` and `crates/devflow-cli/src/commands.rs`

**Testing:**

- Built-in `cargo test` harness
- `insta` 1 (lock 1.48.0): snapshot drift guard for prompt text. Snapshots live in `crates/devflow-core/src/snapshots/` and `crates/devflow-cli/src/snapshots/`. `scripts/check.sh` must run tests under `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`; the reason is written in `Cargo.toml`
- `tempfile` 3 (lock 3.27.0): hermetic temporary directories (dev-dependency in both crates)
- `devflow-core` feature `test-support` exposes `crates/devflow-core/src/test_support.rs`, which builds hermetic git commands. It is enabled only as a dev-dependency

**Build/Dev:**

- `crates/devflow-cli/build.rs`: runs `git` itself to embed the build commit and the dirty flag with `cargo:rustc-env`. It has no build-dependencies, it always reruns, and it must not fail when `.git` is absent
- `scripts/check.sh`: the canonical gate (`all`, `test`, `clippy`, `fmt`, and `deps`, which runs `cargo deny check` and `cargo machete` but is not part of `all`)
- `scripts/check-in-container.sh`: runs `check.sh` inside the pinned CI image
- `scripts/install-dep-tools.sh`: installs cargo-deny and cargo-machete
- `clippy.toml`: bans `std::env::set_var` and `std::env::remove_var` through `disallowed-methods`. Scope environment changes per child with `Command::env`, `Command::env_remove` or `Command::env_clear`. A test-only exception needs `#[expect(clippy::disallowed_methods, reason = ...)]`
- `[workspace.lints.clippy]` in `Cargo.toml` warns on `dbg_macro`, `todo` and `unimplemented`
- `deny.toml`: cargo-deny policy for licenses, bans and sources. The only allowed registry is crates.io and `allow-git = []`

## Key Dependencies

**Critical (`crates/devflow-core/Cargo.toml`):**

- `serde` 1 (`derive`) and `serde_json` 1 (lock 1.0.150): state and event serialization. The `preserve_order` feature is required. Without it, rewriting `.planning/config.json` in `crates/devflow-core/src/gsd_config.rs` would re-sort every key
- `toml` 1 (lock 1.1.2): parses `devflow.toml` in `crates/devflow-core/src/config.rs`
- `thiserror` 2 (lock 2.0.18): error enums
- `semver` 1 (lock 1.0.28): version bumping in `crates/devflow-core/src/version.rs`
- `git-conventional` 1 (lock 1.1.0): Conventional Commit parsing for version and changelog logic
- `libc` 0.2 (lock 0.2.186): process-level operations (signals and pids) in the monitor and lock code

**Infrastructure:**

- `tracing` 0.1 plus `tracing-subscriber` 0.3 (lock 0.3.23, features `json` and `env-filter`, CLI only). `RUST_LOG` and `DEVFLOW_LOG_FORMAT` control the output

## Configuration

**Environment (read in `crates/`):**

- `DEVFLOW_BASE_BRANCH` sets the base branch and overrides `devflow.toml`
- Gate timing: `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`, `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS`
- `DEVFLOW_GATE_NOTIFY_CMD` is an optional shell command run when a gate fires (`crates/devflow-core/src/gates.rs`)
- Agent idle timeouts: `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS`, `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`
- `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`
- `DEVFLOW_CLAUDE_LEGACY_LAUNCH` is a toggle for the launch path
- Cache location comes from `DEVFLOW_CACHE_DIR`, then `XDG_CACHE_HOME`, then `HOME`
- Logging: `RUST_LOG`, `DEVFLOW_LOG_FORMAT`
- `PI_CODING_AGENT_DIR` is used for pi agent credential and config discovery
- Also read: `PATH`, `HOME`, `USER`, `GIT_CONFIG_GLOBAL`

**Files:**

- `devflow.toml` at the project root holds a minimal set of knobs, for example `base_branch` (`crates/devflow-core/src/config.rs`)
- `.planning/config.json` is the GSD config. DevFlow reads it and sets keys in it (`crates/devflow-core/src/gsd_config.rs`)
- `.devflow/` is runtime state: `state-NN.json`, `events.jsonl`, `gates/`, `history/`, `delivery-canary.jsonl`

**Build:**

- `Cargo.toml`, `crates/*/Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `deny.toml`, `doc-check-allowlist.toml` (the last is used by `crates/devflow-core/src/doc_check.rs`)

## Platform Requirements

**Development:**

- Rust 1.97.1 through rustup
- `git` and `gh` on `PATH`
- Optional: Docker or Podman for `scripts/check-in-container.sh`, cargo-deny, cargo-machete, and `mkdocs` for `scripts/deploy-docs.sh`
- Devcontainer: `.devcontainer/devcontainer.json` uses image `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` with the github-cli feature. Its header comment still says `channel = "stable"`, which no longer matches `rust-toolchain.toml`
- Hooks are enabled with `core.hooksPath=scripts/hooks`. `pre-push` runs the container gate

**Production:**

- Published to crates.io as `devflow-core` and `devflow` (workspace version `2.12.0`). Publish order is core first, then the CLI (`scripts/cut-release.sh publish`)
- Installed with `cargo install` or `scripts/install.sh`
- Dual license: MIT OR Apache-2.0

---

*Stack analysis: 2026-09-18*
