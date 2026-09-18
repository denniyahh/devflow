---
last_mapped_commit: d581384145e82fd8f7091fee6387d70c6a62fff5
last_mapped_at: 2026-09-18
---
# Codebase Structure

**Analysis Date:** 2026-09-18

## Directory Layout

```
devflow/
├── crates/
│   ├── devflow-core/          # Library: stage machine, agents, gates, hooks, git
│   │   ├── src/               # One module per concern (+ agents/ subdir)
│   │   │   ├── agents/        # AgentDriver trait (mod.rs) + one driver per agent
│   │   │   └── snapshots/     # insta prompt snapshots
│   │   └── tests/             # Integration tests + fixtures/opencode
│   └── devflow-cli/           # Binary `devflow` (package name `devflow`)
│       ├── src/               # main.rs, commands.rs, pipeline_*.rs, ...
│       │   └── snapshots/     # insta prompt snapshots
│       └── tests/             # E2E / guard tests, fixtures/, snapshots/devflow-help.txt
├── scripts/                   # check.sh, container parity, release, worktree tooling
│   ├── hooks/                 # git hooks (core.hooksPath): commit-msg, post-commit, pre-commit, pre-push
│   └── lib/ci-cpus.sh
├── docs/                      # mkdocs source (architecture/, guides/, diagrams/)
├── site/                      # built mkdocs output (untracked)
├── .github/                   # workflows/ci.yml, devcontainer.yml, templates
├── .devcontainer/             # pinned CI image definition
├── .planning/                 # GSD planning artifacts (GSD-owned)
├── .devflow/                  # runtime state (self-ignored)
├── .worktrees/                # phase worktrees (live WIP)
├── graphify-out/              # knowledge graph output (3 files tracked)
├── Cargo.toml                 # workspace, version, shared deps, clippy lints
├── rust-toolchain.toml, clippy.toml, deny.toml, doc-check-allowlist.toml
└── README.md, ARCHITECTURE.md, OPERATIONS.md, CHANGELOG.md, DEPENDENCIES.md, ...
```

## Directory Purposes

**`crates/devflow-core/src/`:**

- Purpose: all workflow mechanics
- Contains: flat `*.rs` modules declared in `lib.rs`; `agents/` for drivers
- Key files: `stage.rs`, `state.rs`, `workflow.rs`, `monitor.rs`, `agent_result.rs`, `outcome_policy.rs`, `gates.rs`, `hooks.rs`, `git.rs`, `worktree.rs`, `version.rs`

**`crates/devflow-core/src/agents/`:**

- Purpose: per-agent launch/prompt/completion logic
- Key files: `mod.rs` (trait), `claude.rs`, `codex.rs`, `opencode.rs`, `pi.rs`, `hermes.rs`, `antigravity.rs`

**`crates/devflow-cli/src/`:**

- Purpose: CLI and pipeline orchestration
- Key files: `main.rs` (clap + dispatch), `commands.rs`, `pipeline_launch.rs`, `pipeline_outcomes.rs`, `pipeline_gate.rs`, `preflight.rs`, `staleness.rs`, `parallel.rs`, `config_parse.rs`, `test_support.rs` (`#[cfg(test)]`)

**`crates/*/tests/`:**

- Purpose: integration/E2E and repo-policy guard tests (e.g. `ci_parity_guards.rs`, `pre_push_signing_policy.rs`, `workspace_version_pin.rs`, `help_snapshot.rs`)

**`scripts/`:**

- Purpose: verification and release tooling
- Key files: `check.sh`, `check-in-container.sh`, `phase-worktree.sh`, `cut-release.sh`, `cut-pr-branch.sh`, `sync-main-to-develop.sh`, `install.sh`

## Key File Locations

**Entry Points:**

- `crates/devflow-cli/src/main.rs`: binary `main` / `run`
- `crates/devflow-core/src/lib.rs`: library module list + re-exports

**Configuration:**

- `Cargo.toml`: workspace version (2.12.0), edition 2024, shared deps
- `rust-toolchain.toml`, `clippy.toml`, `deny.toml`
- `crates/devflow-core/src/config.rs`: `devflow.toml` schema
- `.github/workflows/ci.yml`: CI

**Core Logic:**

- `crates/devflow-cli/src/pipeline_launch.rs`: `launch_stage`, `advance`, `resume`
- `crates/devflow-core/src/agent_result.rs`: completion detection (largest file, ~9.1k lines)

**Testing:**

- Inline `#[cfg(test)] mod tests` in source; `crates/*/tests/*.rs`; test helpers in `crates/devflow-core/src/test_support.rs` (feature `test-support`) and `crates/devflow-cli/src/test_support.rs`

## Naming Conventions

**Files:**

- snake_case module files: `outcome_policy.rs`, `ship_evidence.rs`
- CLI pipeline seams prefixed `pipeline_`: `pipeline_gate.rs`
- Integration tests named by behaviour, `_e2e` suffix for end-to-end: `stop_e2e.rs`
- insta snapshots: `<crate>__<module>__tests__<name>.snap`

**Directories:**

- Crates `devflow-<role>`; runtime state under dot-dirs (`.devflow/`, `.planning/`, `.worktrees/phase-N`)

## Where to Add New Code

**New Feature (workflow mechanics):**

- Primary code: new module in `crates/devflow-core/src/`, declared in `lib.rs`
- Tests: inline `mod tests`, plus `crates/devflow-core/tests/` for cross-module/process tests

**New CLI subcommand:**

- Variant in `Command` enum in `crates/devflow-cli/src/main.rs`; handler in `crates/devflow-cli/src/commands.rs`
- Tests: `crates/devflow-cli/tests/`; update `tests/snapshots/devflow-help.txt`

**New agent:**

- Driver: `crates/devflow-core/src/agents/<name>.rs` implementing `AgentDriver`; register in `agents/mod.rs` and `AgentKind` (`state.rs`); add a prompt snapshot. See `docs/guides/adding-agent.md`.

**Pipeline behaviour:**

- Launch/advance → `pipeline_launch.rs`; outcome handling → `pipeline_outcomes.rs`; transitions/gates → `pipeline_gate.rs`; pure policy → `crates/devflow-core/src/outcome_policy.rs`

**Utilities:**

- Git helpers: `crates/devflow-core/src/git.rs`; test helpers: `test_support.rs` in each crate

## Special Directories

**`.devflow/`:**

- Purpose: runtime state, events, gates, locks, history
- Generated: Yes
- Committed: No (contains `.gitignore` of `*`)

**`.worktrees/`:**

- Purpose: phase worktrees (`scripts/phase-worktree.sh`)
- Generated: Yes
- Committed: No

**`site/`:**

- Purpose: built docs
- Generated: Yes
- Committed: No (gitignored)

**`target/`:**

- Purpose: cargo build output
- Generated: Yes
- Committed: No

**`graphify-out/`:**

- Purpose: knowledge-graph output
- Generated: Yes
- Committed: Partially (3 files tracked)

---

*Structure analysis: 2026-09-18*
