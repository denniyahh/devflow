# DevFlow — Project Instructions

Deliberately minimal. Global agent rules live in `~/.config/agents/AGENTS.md`; this file holds only constraints specific to how *this* repository is built, verified, and worked on.

## Crate Topology
- `crates/devflow-core`: Core state machine, pipeline engines, and workflow abstractions (`lib`).
- `crates/devflow-cli`: CLI interface and driver commands (`bin: devflow`).
- `devflow-core` is published on crates.io, so changing or removing a `pub` item or variant is a semver change. Prefer an additive change, and run `cargo semver-checks check-release -p devflow-core` before committing one.

## Canonical Verification & Test Commands
Always run targeted package tests first, then full workspace validation:
- Test `devflow-core` library: `cargo test -p devflow-core --lib`
- Test `devflow` CLI binary: `cargo test -p devflow --bin devflow`
  *(Note: `devflow` is binary-only; running `cargo test -p devflow --lib` verifies nothing and fails).*
- Full check suite (formatting, clippy, tests): `scripts/check.sh all`
- Container parity check: `scripts/check-in-container.sh`

## Repo Traps (verification gates)
Shapes that have shipped a false green, or a gate that fails correct work, here. `scripts/lint-plan-bashisms.sh` (pre-commit, staged `*PLAN.md`) refuses the first three inside `<automated>` blocks.
- `cargo test <filter>` exits 0 when the filter matches nothing. Gate on one named test: `cargo nextest run -p <pkg> --lib --no-tests=fail -E 'test(=module::path::name)'` (`--bin devflow` for the CLI). One test per command, chained with `&&`: `-E 'test(=a) | test(=b)'` skips a wrong name and exits 0.
- `rg -c` / `--count` print nothing, not `0`, on zero matches. Check "no X left" or "every Y documented" with a Rust source-scan test (like `doc_check.rs`), not a shell count.
- The CLI package is `devflow`, not `devflow-cli`.
- Prefer a named Rust test to any shell pipeline; keep shell to the invocation.
- Every literal `std::env::var("DEVFLOW_…"` / `var_os(` / `env_value(` read under `crates/` (tests included) and every `Command` variant must be named in README/ARCHITECTURE/CONTRIBUTING/OPERATIONS.md or `docs/guides/`, or allowlisted in `doc-check-allowlist.toml`, in the same change; otherwise `doc_check` fails `scripts/check.sh all`.
- `crates/devflow-cli/build.rs` embeds the commit and dirty flag, so a rebuilt binary's sha256 changes after every commit: never compare it with a stored one.
- `core.fileMode=false`: set an executable bit with `git add <f> && git update-index --chmod=+x <f>`; `--chmod` alone exits 128 on an untracked file.

## Workspace & Worktrees
- Development phases run inside dedicated worktrees under `.worktrees/phase-N`.
- Use `scripts/phase-worktree.sh <phase>` to initialize or attach to a phase worktree.
- Never run concurrent git operations on the main checkout while an executor holds the working tree.

## Development Setup Checklist
- When modifying CI workflows, hooks, devcontainers, or tooling dependencies, keep `.planning/user/DEV-SETUP-CHECKLIST.md` updated in the same commit (warned by `scripts/hooks/post-commit`).
