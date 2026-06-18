# Changelog

All notable changes to DevFlow.

## [1.0.1] — 2026-06-18

### Added
- `devflow doctor` command — environment audit with version detection, JSON output mode
- `scripts/install.sh` — single-command bootstrap for Linux/macOS
- `DEPENDENCIES.md` — full dependency matrix with install instructions
- Standard OSS files: LICENSE (MIT OR Apache-2.0), CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md

### Changed
- README completely rewritten for v1.0.0 — accurate command listing, state machine diagram, quick start
- Removed all tmux, OMX, and deprecated references from docs
- `.github/ISSUE_TEMPLATE/` and `.github/PULL_REQUEST_TEMPLATE.md` added

### Fixed
- Ship merge direction: now merges from current feature branch, not develop
- Ship guard: requires running from a `feature/*` branch
- Stale binary detection in `devflow doctor`

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
- Removed omx/oh-my-codex agent support (deprecated)
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
