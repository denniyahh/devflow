# Contributing to DevFlow

Thanks for your interest! DevFlow is an agent-agnostic development workflow automation CLI written in Rust.

## Setup

```bash
git clone https://github.com/denniyahh/devflow.git
cd devflow
cargo build
cargo test
```

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
(`agents::phase_prompt()`) reads `.planning/ROADMAP.md` and
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
4. Ensure `cargo test` passes and `cargo clippy -- -D warnings` is clean
5. `cargo fmt`
6. Submit a PR against `develop`
7. CI runs tests + clippy + format check

Ordinary code contributions need no agent credentials or API keys — the build
and the full test suite run offline. Agent CLIs (Claude, Codex, OpenCode) are
only needed to exercise `devflow start` against a live agent, not to build,
test, or pass CI.

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

DevFlow is agent-agnostic; agent-specific code lives only under
`crates/devflow-core/src/agents/`. Adding a backend is a short checklist — keep
these in sync or tests/builds fail:

1. Add an adapter file in `crates/devflow-core/src/agents/` implementing the `Agent` trait
2. Add a variant to the `AgentKind`/`Agent` enum in `state.rs`
3. Update the `FromStr` parser, `Display`, and `AgentParseError` text in `state.rs`
4. Add a match arm in `agents::adapter_for()`
5. Add the `pub mod` / `pub use` exports in `agents/mod.rs`
6. Extend tests (adapter name, parser aliases, shared-prompt)
7. Update docs (README, this file, ARCHITECTURE.md, DEPENDENCIES.md)

See [ARCHITECTURE.md](ARCHITECTURE.md#extension-points--adding-an-agent) for the
authoritative version of this checklist.

## Questions?

Open an issue or start a discussion.
