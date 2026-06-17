# Contributing to DevFlow

Thanks for your interest! DevFlow is an open-source project and contributions are welcome.

## Getting Started

```bash
git clone https://github.com/denniyahh/devflow.git
cd devflow
cargo build
cargo test
```

## Development Workflow

This project uses DevFlow itself for development (dogfooding). To work on a phase:

```bash
cargo run -- start --phase N --agent claude
```

Or just write code directly — DevFlow doesn't force a specific workflow.

## Code Standards

- **Rust edition 2024**
- All public items **must** be documented
- Error handling via `thiserror` — no bare `unwrap()` in library code
- Structured output from core library; formatting in CLI
- Run `cargo clippy -- -D warnings` before committing
- Run `cargo test` before pushing

## Architecture

```
crates/
├── devflow-core/     ← Library: state machine, config, git, version, agent adapters
└── devflow-cli/      ← Binary: thin CLI wrapper (clap)
```

The core library returns structured types. The CLI formats them for humans. This separation enables future TUI, HTTP, and MCP frontends.

## Pull Requests

1. Fork the repo
2. Create a feature branch
3. Make your changes
4. Run `cargo test && cargo clippy -- -D warnings`
5. Submit a PR to `develop`

## Reporting Issues

Use GitHub Issues. Include:
- DevFlow version (`devflow --version`)
- Rust version (`rustc --version`)
- Steps to reproduce
- Expected vs actual behavior

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
