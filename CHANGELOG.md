# Changelog

## 1.3.69 — 2026-07-18

- Released phase via DevFlow.

## 1.3.16 — 2026-07-17

- Released phase via DevFlow.

## 1.3.0 — 2026-07-17

- Phase 15: OSS readiness — README/ARCHITECTURE/guides rewritten against v2
  reality, CONTRIBUTING + pinned devcontainer with CI parity, dual
  MIT/Apache-2.0 licensing backed by both texts, devflow-core and devflow
  1.2.0 published to crates.io, per-phase SECURITY.md threat verification.

All notable changes to DevFlow.

## [Unreleased]

### Added
- Root `ARCHITECTURE.md` documenting crates, state machine, agent model, three-layer completion evaluation, monitor daemon, worktree model, git/ship model, configuration schema, and the add-an-agent checklist
- Stale binary detection in `devflow doctor`
- Never-silent gates: every stage failure (not just Validate) now writes a gate and fires a pluggable notify hook via `DEVFLOW_GATE_NOTIFY_CMD`, so unattended runs can never halt silently (WR-11). Gate poll timeout is configurable via `DEVFLOW_GATE_TIMEOUT_SECS` (default 7 days)
- Ship stage now runs `/gsd-code-review` before `/gsd-ship` and refuses to ship on any Critical-severity finding, reporting a distinct `review:`-prefixed failure that loops back to Code instead of gating
- Native per-adapter completion parsing: Claude's `is_error`/`num_turns` envelope fields and a Codex `--json` JSONL event-stream parser (previously only a generic marker/exit-code check)
- `verdict` field (`pass`/`gaps`) on the Validate stage's `DEVFLOW_RESULT` contract — `advance()` only proceeds to Ship on `verdict: pass`, closing the gap between "the agent ran validation" and "validation passed"
- `devflow start` runs in an isolated git worktree by default; `--no-worktree` opts out (previously opt-in via `--worktree`)
- Define/Plan stage prompts are now idempotent: if the stage's deliverable (CONTEXT.md/PLAN.md) already exists, the agent reports success without re-running the GSD command or requesting input — fixes headless Codex runs hanging on GSD's "already exists" decision
- `devflow start --agent codex` pre-flights: errors immediately if the phase has no CONTEXT.md on `develop` (headless Codex cannot run an interactive discussion), with a warning if PLAN.md is also missing
- Codex sandbox (`--sandbox workspace-write`) now gets explicit writable-root grants for the linked worktree's git metadata (both the common `.git` and the worktree's admin dir under `.git/worktrees/<name>`), and commit/tag signing is disabled scoped to Codex's own process tree via `GIT_CONFIG_*` env — the sandbox has no route to the operator's ssh/gpg agent

### Changed
- Corrected docs: removed the phantom `git_flow.enabled` field from examples, fixed the completion-evaluation description (Layer 2 = exit code + commit count, Layer 3 = commit heuristic), and replaced the "3 changes" agent claim with the real checklist
- Removed local-only setup assumptions: untracked `distrobox.ini`, narrowed the `.planning/` gitignore to keep only the prompt-required convention files tracked, documented the GPG-off test setup
- Layer 2's commit-count gate is now scoped to Code-like stages only — Define and Validate legitimately produce zero commits and are no longer mis-flagged as failures
- `devflow`'s lock file (`.devflow/lock`) now reclaims itself when the recorded holder process is dead, instead of wedging every later `devflow advance` for the project

### Removed
- OMX (oh-my-codex) agent support — fully removed (adapter, enum/parser/display, module exports, Hermes skill references, and the stale `.omx/` runtime directory). It had been disabled since 1.0.0; `omx`/`oh-my-codex` are no longer accepted agent names.
- Dead v1 `ship.rs` bookkeeping: the `LastShip` record and the PR-body/goal-extraction/test-summary machinery left over from the removed `devflow confirm`/`devflow rejectpr` commands (zero non-test call sites; PR creation and merging happen entirely inside the external `/gsd-ship` slash command, not in DevFlow's Rust code)

### Fixed
- `devflow ship` now cuts the release branch from the current `HEAD` instead of `develop`, so commits unique to the branch being shipped are no longer dropped from the release
- `git tag` (DevFlow's automatic version-bump tags) no longer blocks on `$EDITOR` when the operator's global `tag.gpgsign` is set to `true` — these are internal SemVer bookkeeping tags, not signed release artifacts, so signing is scoped off per-invocation rather than depending on global config
- Codex's self-reported `DEVFLOW_RESULT` (delivered inside an `agent_message` JSONL item, not as a raw stdout line) is now read correctly — previously a self-reported failure with exit code 0 could be misclassified as success
- The rate-limit detection heuristic no longer scans JSONL event lines as plain text, which could false-match ordinary document content echoed into an agent's output and stuff a multi-KB line into a gate's notification; failure reasons surfaced in gate contexts are now capped at 300 characters

### Fixed
- `devflow ship` now cuts the release branch from the current `HEAD` instead of `develop`, so commits unique to the branch being shipped are no longer dropped from the release

## [1.0.1] — 2026-06-18

### Added
- `devflow doctor` command — environment audit with version detection, JSON output mode
- `scripts/install.sh` — single-command bootstrap for Linux/macOS
- `DEPENDENCIES.md` — full dependency matrix with install instructions
- Standard OSS files: LICENSE (MIT OR Apache-2.0), CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md

### Changed
- README completely rewritten for v1.0.0 — accurate command listing, state machine diagram, quick start
- Removed tmux references from docs
- `.github/ISSUE_TEMPLATE/` and `.github/PULL_REQUEST_TEMPLATE.md` added

## [1.0.0] — 2026-06-18

### Added
- Worktree isolation: `--worktree` flag on `start`, dedicated `parallel` and `sequentagent` commands
- Multi-agent support: `parallel` (concurrent phases) and `sequentagent` (sequential handoff)
- Reference worktrees: `reference` command for static snapshots
- PR integration: `ship` creates GitHub PR, `confirm`/`rejectpr` manage lifecycle
- Rate-limit detection: auto-detect agent 429s, write cron instructions for retry
- Monitor daemon: background agent completion detection with auto-advance
- Agent trait system: pluggable agent adapters (Claude, Codex, OpenCode)
- `cleanup` command: remove worktrees and feature branches
- Shared prompt generation: `phase_prompt()` in agent module
- `recover` command: inspect and clean stale workflow state

### Changed
- Removed tmux dependency — agents run directly via CLI
- Deprecated omx/oh-my-codex agent support (disabled; fully removed in a later release)
- CLI reorganized: new command groups for multi-agent and shipping workflows

### Fixed
- Monitor capture thread lifecycle tied to agent process
- Shell-safe quoting in state machine commands
- JSON envelope parsing for agent results

## [0.5.1] — 2026-06-17

### Added
- Ship readiness: version bump, release branch, PR creation via `gh`
- `config` command shows effective configuration
- `init` command bootstraps `.devflow.yaml`

### Changed
- State machine expanded with SHIPPING and CLEANING steps
- Config schema updated with `git_flow` section

## [0.1.0] — 2026-06-16

### Added
- Initial release
- State machine: IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING
- Git flow branch management
- Version bumper (Cargo.toml, pyproject.toml, package.json)
- `.devflow.yaml` configuration
- Basic CLI: `start`, `check`, `status`, `ship`
