# External Integrations

**Analysis Date:** 2026-07-17

## APIs & External Services

**AI Coding Agents:**
- **Claude Code** - Launched via `claude` CLI command
  - Launch: `crates/devflow-core/src/agents/claude.rs` wraps prompts with `-p "<prompt>" --output-format json --dangerously-skip-permissions`
  - Output: Parsed from stdout, expects `DEVFLOW_RESULT` JSON marker
  - Auth: Uses Claude's native authentication (installed separately via npm)
  
- **OpenAI Codex** - Launched via `codex` CLI command
  - Launch: `crates/devflow-core/src/agents/codex.rs` uses `codex exec --sandbox workspace-write --json`
  - Sandbox: Extra writable roots granted for git metadata (`.git`, `.git/worktrees/phase-NN`)
  - Output: Parsed from stdout with `DEVFLOW_RESULT` JSON marker
  - Auth: Uses Codex's native authentication (installed separately via npm)
  - Special handling: Disables git commit/tag signing inside sandbox via `GIT_CONFIG_*` env vars
  
- **OpenCode** - Launched via `opencode` CLI command
  - Launch: `crates/devflow-core/src/agents/opencode.rs` wraps prompts (implementation specific)
  - Output: Parsed from stdout with `DEVFLOW_RESULT` JSON marker
  - Auth: Uses OpenCode's native authentication (installed separately via cargo)

**Version Control / PR Management:**
- **GitHub (gh CLI)** - PR creation and merge operations
  - Version: 2.0+
  - Commands: Invoked for `devflow gate approve <phase> --stage ship` to merge branches
  - Auth: Uses GitHub CLI's configured authentication (gh auth)
  - Usage: `crates/devflow-core/src/ship.rs` triggers merge via `gh` binary
  
**Rate-Limit Recovery / Scheduling:**
- **Hermes Scheduler** - External cron scheduling service
  - Integration: DevFlow writes `.devflow/cron-instructions-NN.json` when agent hits 429 rate limit
  - Payload: `CronInstructions` struct includes schedule, command, and arguments
  - Defined in: `crates/devflow-core/src/ship.rs` (rate-limit detection) and `crates/devflow-core/src/agent_result.rs` (429 parsing)

## Gate Protocol & External Notifications

**File-Based Gate Protocol:**
- Request: DevFlow writes `.devflow/gates/NN-{stage}.json` (GateFile struct)
- Response: External approver writes `.devflow/gates/NN-{stage}.response.json` (GateResponse struct)
- Acknowledgment: DevFlow writes `.devflow/gates/NN-{stage}.ack.json` (GateAck struct)
- Implementation: `crates/devflow-core/src/gates.rs`
- Atomic writes: Write-to-temp + rename prevents partial file reads

**Webhook / Notification Hook:**
- Trigger: When a gate is fired (Validate or Ship stage)
- Command: `DEVFLOW_GATE_NOTIFY_CMD` environment variable (optional, any shell command)
- Environment passed to hook:
  - `DEVFLOW_GATE_PHASE` - Phase number
  - `DEVFLOW_GATE_STAGE` - Stage name (e.g., "validate", "ship")
  - `DEVFLOW_GATE_CONTEXT` - Human-readable gate description
  - `DEVFLOW_NON_SILENT_GATE` - Set to `1` if gate fired due to unexpected stage failure
- Example: `curl -d "phase $DEVFLOW_GATE_PHASE" ntfy.sh/my-topic`
- Implementation: `crates/devflow-core/src/gates.rs` spawns hook as subprocess

## Event Log & Observability

**Append-Only Event Log:**
- Location: `.devflow/events.jsonl` (git-ignored, runtime state)
- Format: One JSON object per line (JSONL)
- Schema v1: `{"v":1,"ts":<unix-seconds>,"phase":<N>,"event":"<event-type>",...}`
- Events: `transition` (stage changes), `gate_fired`, `gate_resolved`, `agent_result`, etc.
- Purpose: External tools (web dashboards, TUI, Hermes plugin) tail this file to observe workflow state
- Implementation: `crates/devflow-core/src/events.rs` emits events with fail-soft semantics
- Atomic appends: Single `write_all` on `O_APPEND` handle ensures lines don't tear under concurrent phase monitors

## Process Management

**Agent Process Spawning:**
- Method: Standard `std::process::Command` with libc syscalls
- Isolation: Agents run in git worktrees (`.worktrees/phase-NN/`) by default
- Capture: stdout and stderr captured to `.devflow/phase-NN-stdout` and `.devflow/phase-NN-stderr.log`
- Monitoring: Background monitor daemon (shell script) tracks process via PID file
- Signal handling: SIGCHLD trapped to detect agent completion; SIGTERM/SIGINT propagated
- Rate-limit detection: stdout scanned for `429` HTTP status codes (agent API errors)

**Background Monitor Daemon:**
- Type: Detached shell subprocess (not a daemon binary)
- Lifecycle: Spawned by `devflow start`, runs independently
- Function: Polls agent process, advances state machine on completion
- State tracking: Reads/writes `.devflow/state-NN.json` per phase
- Locking: Per-phase advance lock (`.devflow/lock-NN`) prevents concurrent state mutations
- Shell implementation: See `crates/devflow-core/src/monitor.rs` for generated shell trap script

## Data Storage

**Local File-Based State:**
- Per-phase state: `.devflow/state-NN.json` (persisted `StateData` struct, survives restarts)
- Locks: `.devflow/lock-NN` (per-phase), `.devflow/lock-project` (coarse checkout lock)
- Agent output: `.devflow/phase-NN-stdout`, `.devflow/phase-NN-stderr.log` (captured streams)
- Agent exit: `.devflow/phase-NN-exit` (exit code), `.devflow/phase-NN-agent-pid` (PID)
- Cron instructions: `.devflow/cron-instructions-NN.json` (Hermes integration, rate-limit recovery)
- No database: All state is JSON files under `.devflow/` (git-ignored)

**Version Control:**
- git repository: All code and docs committed
- Branch model: `develop` (main branch), `feature/phase-NN` (per-phase branches)
- Worktrees: Linked worktrees at `.worktrees/phase-NN/` for parallel phase execution
- Tags: Semantic version tags created at Ship stage (via `git tag`)

## Authentication & Secrets

**Agent Authentication:**
- **Claude Code / Codex / OpenCode**: Each has its own CLI-based auth (installed separately)
- DevFlow itself: No embedded API keys; agents manage their own credentials
- git commits: Signed via operator's `gpg-agent` or SSH agent (or unsigned in Codex sandbox)

**GitHub Authentication:**
- Mechanism: `gh` CLI uses configured credentials (typically GitHub token in `~/.config/gh/hosts.yml`)
- Scope: PR creation and merge operations only (no API key embedded in DevFlow)

## CI/CD & Deployment

**Hosting / Distribution:**
- Package: Distributed via crates.io and cargo package registry
- Installation: `cargo install devflow` or build from source
- CI Platform: GitHub Actions (`.github/workflows/ci.yml`)

**CI Pipeline:**
- Trigger: `push` to `main` or `develop`, or `pull_request` against those branches
- Concurrency: Cancels in-progress runs if a new push/PR arrives (group by workflow + ref)
- Jobs (parallel):
  1. **Test** - `cargo test` (runs `crates/devflow-core/tests/` and unit tests)
  2. **Clippy** - `cargo clippy -- -D warnings` (linting, enforced)
  3. **Format** - `cargo fmt --check` (formatting check, enforced)
- Tooling: Uses `dtolnay/rust-toolchain@stable` action with pinned components

## Development Container

**DevContainer Configuration:**
- Base image: `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` (pinned, not floating)
- Mounts:
  - Volume `devflow-cargo-registry` → `/usr/local/cargo/registry` (persistent across rebuilds)
  - Volume `devflow-target-${localWorkspaceFolderBasename}` → `${containerWorkspaceFolder}/target` (build cache)
- Post-create: Installs clippy/rustfmt, runs `cargo build --workspace`
- VS Code extensions: rust-analyzer, vscode-lldb (debugger), even-better-toml

## MCP Integrations

**Model Context Protocol Servers:**
- `gsd-workflow` - GSD workflow execution integration
  - Binary: `/var/home/linuxbrew/.linuxbrew/lib/node_modules/@opengsd/gsd-pi/packages/mcp-server/dist/cli.js`
  - Runtime: Node.js 26.4.0
  - Config: `GSD_CLI_PATH`, `GSD_BIN_PATH`, `GSD_WORKFLOW_EXECUTORS_MODULE`, `GSD_WORKFLOW_WRITE_GATE_MODULE`
  - Purpose: Bridges GSD planning/execution to DevFlow phases
  
- `gsd-browser` - GSD browser/session integration
  - Binary: `gsd-browser` CLI
  - Args: Session ID, identity scope, project identity
  - Purpose: External workflow observation and control

---

*Integration audit: 2026-07-17*
