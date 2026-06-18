# Changelog

All notable changes to DevFlow.

## [Unreleased]

### Added
- Root `ARCHITECTURE.md` documenting crates, state machine, agent model, three-layer completion evaluation, monitor daemon, worktree model, git/ship model, configuration schema, and the add-an-agent checklist
- Stale binary detection in `devflow doctor`

### Changed
- Corrected docs: removed the phantom `git_flow.enabled` field from examples, fixed the completion-evaluation description (Layer 2 = exit code + commit count, Layer 3 = commit heuristic), and replaced the "3 changes" agent claim with the real checklist
- Removed local-only setup assumptions: untracked `distrobox.ini`, narrowed the `.planning/` gitignore to keep only the prompt-required convention files tracked, documented the GPG-off test setup

### Removed
- OMX (oh-my-codex) agent support — fully removed (adapter, enum/parser/display, module exports, Hermes skill references, and the stale `.omx/` runtime directory). It had been disabled since 1.0.0; `omx`/`oh-my-codex` are no longer accepted agent names.

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
