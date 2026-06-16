# DevFlow Roadmap

## v0.1.0 — Implemented (June 16, 2026)

### Core Library (`devflow-core`)

| Module | Lines | Status | Description |
|---|---|---|---|
| `state.rs` | 223 | ✅ | State machine: `Step` enum (Idle→Branching→Executing→Verifying→Docsing→Shipping→Cleaning), `State` struct, `Agent` enum (Claude/Omx/Codex/OpenCode), `advance()` + `advance_skipping()`, shell-safe quoting |
| `config.rs` | 294 | ✅ | `.devflow.yaml` parsing with serde, `Config`/`VersionConfig`/`AutomationConfig`/`GitFlowConfig` structs, sensible defaults, `should_skip()` for automation toggles, `to_yaml()` serialization |
| `git.rs` | 119 | ✅ | `GitFlow` struct: `feature_start()`/`feature_finish()`, `release_start()`/`release_finish()`, `cleanup_merged()` — implemented with raw `git` commands (no `git-flow` CLI dependency) |
| `tmux.rs` | 70 | ✅ | `launch_agent()` — creates detached tmux session + sends agent launch command, `agent_running()` — `tmux has-session` check, `capture_output()` — `tmux capture-pane` |
| `version.rs` | 164 | ✅ | `read_version()` from pyproject.toml, `bump()` semver major/minor/patch, `build_number()` via git commit count or unix timestamp, `write_version()` with in-place field replacement, unit tests |
| `workflow.rs` | 99 | ✅ | State persistence: `save_state()`/`load_state()`/`clear_state()` to `.devflow/state.json`, `advance_state()` orchestrator that steps through the machine respecting config skips |
| `lib.rs` | 15 | ✅ | Module re-exports |

### CLI Binary (`devflow-cli`)

| Command | Status | Description |
|---|---|---|
| `devflow start --phase N [--agent]` | ✅ | Creates feature branch, launches agent in tmux, persists state |
| `devflow check` | ✅ | Polls tmux session, advances state machine when agent exits |
| `devflow status` | ✅ | Shows current step, phase, agent, tmux session, running status |
| `devflow ship` | ✅ | Bumps version, creates release branch, writes back to config file |
| `devflow init [--force]` | ✅ | Bootstraps `.devflow.yaml` + `.devflow/` directory |
| `devflow config` | ✅ | Shows effective config in YAML |

### Tests

| Test | Status |
|---|---|
| `config::parses_devflow_yaml_shape` | ✅ Passes |
| `version::bumps_semver_components` | ✅ Passes (patch, minor, major) |

---

## v0.2.0 — Monitor & Hermes Integration (Next)

### Monitor Daemon
- [ ] `monitor.rs` — `spawn_monitor()` forks a child process
- [ ] Child process: `while tmux has-session -t <name>; do sleep 30; done; devflow check <project>`
- [ ] Parent returns PID, writes to state file
- [ ] `devflow start --monitor` flag to enable
- [ ] Recovery: `devflow recover` re-launches monitor from state file

### Hermes Skill
- [ ] `skills/hermes/devflow/SKILL.md` — skill file in repo
- [ ] Teaches Hermes to invoke `devflow start`/`check`/`status`
- [ ] Auto-detects `.devflow.yaml` in project repos
- [ ] Reports phase transitions to user

### Git Flow CLI Support
- [ ] Optional `git flow` CLI backend (fallback to raw git if not installed)
- [ ] `git flow init` on first use
- [ ] Better error messages when develop branch doesn't exist

---

## v0.3.0 — Robustness & Multi-Project

### Error Recovery
- [ ] `devflow recover` — reads state file, re-launches monitor or resumes from current step
- [ ] Stale state detection — warns if state file is >24h old with no tmux session
- [ ] Lock file to prevent concurrent `devflow check` runs
- [ ] SIGTERM handling in monitor — clean exit, mark state as interrupted

### Multi-Project
- [ ] `devflow list` — show all projects with active workflows
- [ ] Global config `~/.config/devflow/config.yaml` for defaults
- [ ] Project name detection from git remote

### Version Bumper Expansion
- [ ] `Cargo.toml` support (`package.version` field)
- [ ] `package.json` support (`version` field)
- [ ] Calver scheme (`YYYY.MM.DD` or `YYYY.MM.PATCH`)
- [ ] Build metadata suffix (`+build.N`)

---

## v0.4.0 — Agent Trait & Verification

### Agent Trait
- [ ] `Agent` trait in `agents/mod.rs` with `launch()`, `is_running()`, `capture_output()`
- [ ] Per-agent implementations: `claude.rs`, `omx.rs`, `codex.rs`, `opencode.rs`
- [ ] Agent-specific output parsing (detect completion signals)
- [ ] Agent configuration (model, flags, env vars) in `.devflow.yaml`

### Verification Step
- [ ] `devflow verify` — runs configured verification commands
- [ ] Config: `automation.verify_command: "cargo test"`
- [ ] Config: `automation.lint_command: "cargo clippy"`
- [ ] Fail-fast or continue-on-error modes

### Documentation Step
- [ ] `devflow docs` — placeholder for doc generation
- [ ] Config: `automation.docs_command`
- [ ] Auto-commit docs changes

---

## v1.0.0 — Ship-Ready

### GitHub Integration
- [ ] PR creation via `gh` CLI or GitHub API
- [ ] PR body auto-generated from phase SUMMARY.md
- [ ] Review request automation
- [ ] Merge detection — auto-advance on PR merge

### Open Source Scaffolding
- [ ] `LICENSE` (MIT OR Apache-2.0)
- [ ] `CONTRIBUTING.md`
- [ ] `CODE_OF_CONDUCT.md`
- [ ] `CHANGELOG.md`
- [ ] GitHub repo setup + CI

### CI/CD
- [ ] GitHub Actions: `cargo test`, `cargo clippy`, `cargo fmt --check`
- [ ] Release workflow: build + publish binary artifacts
- [ ] `cargo install devflow` or `curl | sh` install script

---

## v1.1+ — Additional Surfaces

### HTTP API
- [ ] `devflow serve` — lightweight HTTP server on localhost
- [ ] `POST /api/start`, `GET /api/status`, `POST /api/check`, `POST /api/ship`
- [ ] JSON request/response matching CLI semantics

### MCP Server
- [ ] `devflow mcp` — MCP stdio server
- [ ] Tool discovery: `devflow_start`, `devflow_check`, `devflow_status`, `devflow_ship`
- [ ] Compatible with Claude Code, Hermes, and any MCP client

### TUI (`devflow-tui`)
- [ ] Ratatui-based terminal UI
- [ ] Live state machine visualization
- [ ] Tmux pane preview
- [ ] Interactive phase selection + agent launch

### Hermes Plugin
- [ ] Native Hermes tool registration (bypasses `terminal()` calls)
- [ ] Typed output (structured JSON instead of text parsing)
- [ ] Auto-detection of DevFlow-managed projects

### Library Crate
- [ ] `devflow-core` published to crates.io
- [ ] Stable public API for embedding in other Rust tools
- [ ] Semver guarantees for library consumers

---

## Design Decisions (Recorded)

| Decision | Rationale | Date |
|---|---|---|
| Rust | Single binary, cross-platform, no runtime deps, Dennis preference | 2026-06-16 |
| Tmux for all agents | Unified control surface (send-keys, capture-pane works identically) | 2026-06-16 |
| PID-based monitoring | No cron/scheduler dependency, no agent cooperation needed | 2026-06-16 |
| Git flow branch model | feature→develop, release→main+develop, industry standard | 2026-06-16 |
| CLI baseline | Universal contract — every AI agent runs shell commands | 2026-06-16 |
| MCP/HTTP/TUI later | Layer on same core library, no architectural changes needed | 2026-06-16 |
| Raw git commands | No `git-flow` CLI dependency, simpler install | 2026-06-16 |
| Agent-agnostic | `Agent` enum + `launch_command()`, trait coming in v0.4.0 | 2026-06-16 |
| State file in `.devflow/` | Gitignored, survives reboots, enables recovery | 2026-06-16 |
| Config in `.devflow.yaml` | Git-tracked, portable across machines, project-specific | 2026-06-16 |

---

## Known Limitations

| Issue | Impact | Target Fix |
|---|---|---|
| Tmux blocked in Codex sandbox | Tests can't verify tmux launch inside Codex. Production use unaffected (devflow runs on host). | Documented — not a bug |
| Only pyproject.toml version support | Can't read versions from Cargo.toml or package.json | v0.3.0 |
| No monitor daemon yet | `devflow check` must be called manually or via cron | v0.2.0 |
| No Hermes skill yet | Hermes doesn't know how to use devflow automatically | v0.2.0 |
| Verify/docs steps are no-ops | State machine advances through them but no actions run | v0.4.0 |
| No GitHub PR integration | `ship` creates release branch but doesn't create PR | v1.0.0 |
| Agent enum not trait | Adding new agents requires modifying core code | v0.4.0 |
