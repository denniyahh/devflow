# Contributing to DevFlow

Thanks for your interest! DevFlow is an agent-agnostic development workflow automation CLI written in Rust.

## Setup

```bash
git clone https://github.com/denniyahh/devflow.git
cd devflow
cargo build
cargo test
```

### Pre-push hook (optional, recommended)

A tracked hook at [`scripts/hooks/pre-push`](scripts/hooks/pre-push) runs the
same fmt/clippy/test checks CI requires, before the push leaves your machine:

```bash
git config core.hooksPath scripts/hooks
```

### Distrobox (optional)

If you use [distrobox](https://github.com/89luca89/distrobox), you can create an isolated environment:

```bash
distrobox create --name devflow-dev --image fedora:41
distrobox enter devflow-dev
# install Rust, build, test as above
```

### Dev Container (optional)

The repo includes a [`.devcontainer/devcontainer.json`](.devcontainer/devcontainer.json)
with the `rust-toolchain.toml`-pinned stable toolchain (`clippy` + `rustfmt`) preinstalled and
cargo registry/`target` caches persisted across rebuilds. Open the repo in VS Code and choose
"Reopen in Container", or run `devcontainer up --workspace-folder .` (via the
[Dev Containers CLI](https://github.com/devcontainers/cli)) to get a reproducible build/test
environment without installing Rust locally.

## Development

```bash
# Build
cargo build

# Run all tests
cargo test

# Lint (must include --all-targets, or test code goes unlinted)
cargo clippy --workspace --all-targets -- -D warnings

# Format check
cargo fmt --check

# Run a specific command
cargo run -- status
```

### Testing notes

The git-flow tests create throwaway fixture repositories. If you sign commits
or tags globally, disable signing for these fixtures so the tests don't block
on a GPG prompt. The test harness sets this per-fixture, but if you run any
manual git steps against a fixture, use:

```bash
git config commit.gpgsign false
git config tag.gpgsign false
```

## Phase Plans (`.planning/`)

DevFlow drives agents from per-phase plans under `.planning/`. The launch prompt
(`crate::prompt::stage_prompt(stage, phase)`) reads `.planning/ROADMAP.md` and
`.planning/phases/NN-*/CONTEXT.md`, so these files are tracked in the repo — they
are DevFlow's phase-plan convention, not private scratch. When adding a phase,
commit its `CONTEXT.md` so agents (and reviewers) can read the plan.

## Project Structure

```
crates/
├── devflow-core/     ← Library crate: state machine, config, git, versioning, agents
└── devflow-cli/      ← Binary crate: clap CLI wrapper
```

## Code Style

- Rust edition 2024
- All public items must be documented
- Error handling via `thiserror`
- No `unwrap()` in library code — use `Result`
- Structured output from core, formatting in CLI

## PR Process

1. Fork the repo
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Write code, add tests
4. Ensure `cargo test` passes and `cargo clippy --workspace --all-targets -- -D warnings` is clean
5. `cargo fmt`
6. Submit a PR against `develop`
7. CI runs tests + clippy + format check

**Required checks** — a PR must pass all three CI jobs before it can merge
(mirrors [`.github/workflows/ci.yml`](.github/workflows/ci.yml)):

- `cargo test`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --check`

`devflow test` runs these same three checks locally, and
[`scripts/hooks/pre-push`](scripts/hooks/pre-push) runs them before a push.
The `--all-targets` scope is load-bearing: the narrower `cargo clippy -- -D
warnings` does not compile test targets, so lints inside `#[cfg(test)]`
modules pass it and then fail CI. Regression guards for this live in
`crates/devflow-cli/tests/devcontainer_ci_failfast.rs`.

Ordinary code contributions need no agent credentials or API keys — the build
and the full test suite run offline. Agent CLIs (Claude, Codex, OpenCode) are
only needed to exercise `devflow start` against a live agent, not to build,
test, or pass CI.

## Cutting a Release

1. On `develop`: bump `version` in the root `Cargo.toml`, run `cargo build`
   to sync `Cargo.lock`, add a new top `## X.Y.Z` section to `CHANGELOG.md`.
2. Open a PR from `develop` into `main` titled
   `release: vX.Y.Z — <short description>`.
3. Once CI is green, squash-merge it (this repo's branch settings only
   allow squash merges into `main` — real merge commits are disabled).
4. Tag the resulting commit on `main`: `git tag -a vX.Y.Z <commit> -m "..."`,
   then `git push origin vX.Y.Z`.
5. **Immediately run `scripts/sync-main-to-develop.sh`** from a clean
   `develop` checkout, then `git push origin develop`.

Step 5 is not optional. Because `main` only accepts squash merges, its new
release commit has no parent relationship back to `develop` — skip this
step and the *next* release PR will conflict against a stale merge-base
(this happened going into v1.5.0: main and develop had silently diverged
since v1.4.0, producing conflicts across 11 files including core Rust
source). The script performs a content-preserving `-X ours` merge — it
verifies the resulting tree is byte-identical to develop's before allowing
itself to proceed — so it only ever links history, never changes content.

To publish to crates.io after tagging: `cargo publish -p devflow-core`
first (it must be live on the registry before the next step, since
`devflow`'s manifest depends on it by version, not by path, once
packaged), then `cargo publish -p devflow`.

## Commit Conventions

DevFlow uses [Conventional Commits](https://www.conventionalcommits.org/):
`type(scope): description`, imperative mood, no period at the end.

Common types in this repo: `feat`, `fix`, `docs`, `test`, `ci`, `chore`,
`refactor`. Scope is typically a crate/module (`cli`, `core`) or a phase/plan
identifier (`15-05`, `phase-15`). Phase 11's per-phase branching/merge scheme
(feature branches completed through the gate-driven Ship flow) works alongside
Conventional Commits, not as a replacement for it — every commit in this
project's own history follows the format.

## Logging Conventions

DevFlow uses the [`tracing`](https://docs.rs/tracing) crate for structured
diagnostic logging. All log output goes to **stderr**; stdout is reserved for
agent/system output.

### Writing log events

```rust
use tracing::{info, debug, warn, error};

// State transitions and milestones
info!(before = %old_step, after = %new_step, phase = phase, "step_entered");

// I/O and detail operations
debug!(path = %path, "saved state to disk");

// Recoverable anomalies
warn!("force-pushing branch {branch}");

// Fatal conditions
error!("failed to load config: {err}");
```

### Structured events for state transitions

State transitions in `workflow.rs` should emit paired `step_entered` /
`step_exited` events at `INFO` level with `(before, after, phase)` fields:

```rust
info!(before = %current, after = %next, phase = state.phase, "step_entered");
```

### Controlling log output

| Variable | Purpose |
|---|---|
| `RUST_LOG` | Controls verbosity. Set to `error`, `warn`, `info`, `debug`, or `trace`. Use targeted directives like `devflow_core=debug,devflow=info` to filter by crate. |
| `DEVFLOW_LOG_FORMAT` | Set to `json` for machine-readable JSON output (one JSON object per line on stderr). |

### Do's and Don'ts

- **Do** use `tracing` macros (`info!`, `debug!`, `warn!`, `error!`) — never
  `println!` or `eprintln!` for diagnostic output.
- **Do** log to stderr; reserve stdout for structured results and agent output.
- **Do** use structured fields (`field = value`) instead of string interpolation
  for machine-parseable log entries.
- **Do** add `#[tracing::instrument]` to key state-machine functions so call
  chains appear in log output.
- **Don't** log secrets, tokens, or API keys.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for design documentation.

### Adding a New Agent

DevFlow is agent-agnostic; agent-specific code lives only under
`crates/devflow-core/src/agents/`. Adding a backend is a short checklist — keep
these in sync or tests/builds fail:

1. Add an adapter file in `crates/devflow-core/src/agents/` implementing the `AgentAdapter` trait
2. Add a variant to the `AgentKind` enum in `state.rs`
3. Update the `FromStr` parser, `Display`, and `AgentParseError` text in `state.rs`
4. Add a match arm in `agents::adapter_for()`
5. Add the `pub mod` / `pub use` exports in `agents/mod.rs`
6. Extend tests (adapter name, parser aliases, prompt-sharing)
7. Update docs (README, this file, ARCHITECTURE.md, DEPENDENCIES.md)

See [ARCHITECTURE.md](ARCHITECTURE.md#extension-points--adding-an-agent) for the
authoritative version of this checklist.

## Questions?

Open an issue or start a discussion.
