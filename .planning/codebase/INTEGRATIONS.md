---
last_mapped_commit: d581384145e82fd8f7091fee6387d70c6a62fff5
last_mapped_at: 2026-09-18
---
# External Integrations

**Analysis Date:** 2026-09-18

All integrations are **subprocess CLIs**. The codebase has no HTTP client, SDK or network library. Each external system is a binary called through `std::process::Command`. To add an integration, add a subprocess wrapper; do not add an HTTP crate (`deny.toml` also restricts sources to crates.io).

## APIs & External Services

**AI coding agents** (the `AgentKind` enum is in `crates/devflow-core/src/state.rs`; values are `claude`, `codex`, `opencode`/`open-code`, `pi`, `antigravity`, `hermes`):

- Claude Code (`claude -p`, non-interactive): the default agent. Launch logic is in `crates/devflow-core/src/agent.rs`, and there is a liveness probe in `crates/devflow-core/src/canary.rs`
  - Auth: handled by the CLI itself. DevFlow does not read any credential
  - Idle timeout: `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS`. `DEVFLOW_CLAUDE_LEGACY_LAUNCH` switches the launch path
- Codex (`codex exec`): `crates/devflow-core/src/agents/codex.rs`. Receives a Codex-native instruction instead of a `/gsd-*` slash command (`crates/devflow-core/src/prompt.rs`)
- OpenCode (`opencode`): `crates/devflow-core/src/agents/opencode.rs`, including a credential check
- Pi (`pi`): `crates/devflow-core/src/agents/pi.rs`. Reads `PI_CODING_AGENT_DIR`
- Antigravity (`agy`, stream-json transport): `crates/devflow-core/src/agents/antigravity.rs`, plus a canary probe in `crates/devflow-core/src/canary.rs`. Idle timeout: `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`
- Hermes (`hermes`, headless oneshot): `crates/devflow-core/src/agents/hermes.rs`
- Shared dispatch: `crates/devflow-core/src/agents/mod.rs`. Results are parsed in `crates/devflow-core/src/agent_result.rs`, and the process monitor is `crates/devflow-core/src/monitor.rs`

**GSD workflow (slash commands run by the agent):**

- Stage prompts produce `/gsd-*` commands such as `/gsd-plan-phase N`, `/gsd-execute-phase N`, `/gsd-code-review N`, `/gsd-ship N` and `/gsd-validate-phase N` (`crates/devflow-core/src/prompt.rs`, `crates/devflow-core/src/stage.rs`)
- Reads and writes the GSD config `.planning/config.json` (`crates/devflow-core/src/gsd_config.rs`)

**GitHub:**

- `gh` CLI. Preflight runs `gh auth status` before Ship and keeps only a boolean result (`crates/devflow-cli/src/preflight.rs`, `preflight_gh_auth_check`). If `gh` is missing, this check only warns (fail-soft). PR creation and merge happen through the agent's `/gsd-ship`, not through direct `gh pr` calls in `crates/`
- Repository: `https://github.com/denniyahh/devflow`. The rulesets `develop-merge-or-squash` and `main-squash-only` require the checks `Test`, `Clippy`, `Format` and `Build + test in devcontainer`

**Git:**

- The `git` binary handles branches, `--no-ff` merges, worktrees, tags and history (`crates/devflow-core/src/git.rs`, `crates/devflow-core/src/worktree.rs`, `crates/devflow-core/src/version.rs`, `crates/devflow-core/src/ship.rs`, `crates/devflow-core/src/ship_evidence.rs`)
- There is no `git2` or libgit2 dependency. Tests build hermetic git commands with `crates/devflow-core/src/test_support.rs`, which scrubs the `GIT_*` variables

## Data Storage

**Databases:**

- None. All state is local JSON and JSONL files

**File Storage:**

- Local filesystem only: `.devflow/state-NN.json` (run state, `crates/devflow-core/src/state.rs`), `.devflow/events.jsonl` (`crates/devflow-core/src/events.rs`), `.devflow/gates/` (`crates/devflow-core/src/gates.rs`), `.devflow/history/` (`crates/devflow-core/src/history.rs`), `.devflow/delivery-canary.jsonl` (`crates/devflow-core/src/canary.rs`), and the run registry (`crates/devflow-core/src/registry.rs`)

**Caching:**

- A local cache directory resolved from `DEVFLOW_CACHE_DIR`, then `XDG_CACHE_HOME`, then `HOME`. There is no external cache

## Authentication & Identity

**Auth Provider:**

- None of its own. Credentials belong to each external CLI (`gh`, `claude`, `opencode`, and so on). DevFlow only checks that they are valid and never logs raw auth output

## Monitoring & Observability

**Error Tracking:**

- None

**Logs:**

- `tracing` with `tracing-subscriber` (human or JSON output through `DEVFLOW_LOG_FORMAT`, filtered by `RUST_LOG`), plus the `.devflow/events.jsonl` audit stream

## CI/CD & Deployment

**Hosting:**

- crates.io for the `devflow-core` and `devflow` crates, published by `scripts/cut-release.sh`
- The docs site is deployed to GitHub Pages by `scripts/deploy-docs.sh` (`mkdocs gh-deploy`)

**CI Pipeline:**

- GitHub Actions. `.github/workflows/ci.yml` runs the jobs Test, Clippy, Format, `Dependency checks` (cargo-deny) and an advisory `Sequential 2-CPU check`. `.github/workflows/devcontainer.yml` runs `Build + test in devcontainer`
- Every job runs inside `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`, and `scripts/assert-image-parity.sh` enforces that this matches `.devcontainer/devcontainer.json`

## Environment Configuration

**Required env vars:**

- None are required. All `DEVFLOW_*` variables are optional overrides (see `.planning/codebase/STACK.md`)

**Secrets location:**

- DevFlow keeps no secrets. The external CLIs store their own credentials. `.env` files: none observed at the repo root

## Webhooks & Callbacks

**Incoming:**

- None

**Outgoing:**

- `DEVFLOW_GATE_NOTIFY_CMD`: an operator-supplied shell command run when a gate fires (`crates/devflow-core/src/gates.rs`). It is a no-op when unset

---

*Integration audit: 2026-09-18*
