# Contributing to DevFlow

Thanks for your interest! DevFlow is an agent-agnostic development workflow automation CLI written in Rust.

## Setup

```bash
git clone https://github.com/denniyahh/devflow.git
cd devflow
cargo build
cargo test
```

### Dev Container

A VS Code / GitHub Codespaces dev container is available (`.devcontainer/devcontainer.json`) with Rust and all dependencies pre-installed.

### Distrobox (optional)

If you use [distrobox](https://github.com/89luca89/distrobox), you can create an isolated environment:

```bash
distrobox create --name devflow-dev --image fedora:41
distrobox enter devflow-dev
# install Rust, build, test as above
```

## Development

```bash
# Build
cargo build

# Run all tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format check
cargo fmt -- --check

# Run a specific command
cargo run -- status
```

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
4. Ensure `cargo test` passes and `cargo clippy` is clean
5. `cargo fmt`
6. Submit a PR against `develop`
7. CI will run tests + clippy + format check

## Commit Conventions

We use conventional commits:

- `feat:` — new feature
- `fix:` — bug fix
- `docs:` — documentation
- `test:` — tests
- `refactor:` — code change that neither fixes a bug nor adds a feature
- `chore:` — maintenance, CI, tooling

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for design documentation.

### Adding a New Agent

DevFlow is agent-agnostic. Adding a new agent requires exactly 3 changes:

1. Create a new file in `crates/devflow-core/src/agents/` implementing the `Agent` trait
2. Add a variant to `AgentKind` enum in `state.rs`
3. Add an entry to `agents::adapter_for()` match arm

That's it. No other files need to change.

## Questions?

Open an issue or start a discussion.
