# Codebase Structure

**Analysis Date:** 2026-07-17

## Directory Layout

```
devflow/                                # Project root
├── crates/
│   ├── devflow-core/                   # Core library: state machine, adapters, git, versioning
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Module re-exports, logging setup docs
│   │       ├── agent.rs                # Agent process helpers (PID checking)
│   │       ├── agent_result.rs         # Three-layer result parsing (DEVFLOW_RESULT, exit code, heuristic)
│   │       ├── config.rs               # Git-flow constants (main, develop, feature/ prefix)
│   │       ├── events.rs               # Event emission (currently empty/unused)
│   │       ├── gates.rs                # Gate request/response/ack file protocol
│   │       ├── git.rs                  # GitFlow helper: feature/release/tag operations
│   │       ├── hooks.rs                # Git hook integration (not yet implemented)
│   │       ├── lock.rs                 # Per-phase + project-wide file-based locks
│   │       ├── mode.rs                 # Mode enum: Auto vs. Supervise
│   │       ├── monitor.rs              # Background daemon spawning (agent owner)
│   │       ├── prompt.rs               # Stage-specific agent prompts + DEVFLOW_RESULT contract
│   │       ├── recover.rs              # Inspect/cleanup stale workflow state
│   │       ├── ship.rs                 # Cron instructions (rate-limit resumption)
│   │       ├── stage.rs                # Stage enum: Define/Plan/Code/Validate/Ship
│   │       ├── state.rs                # State struct (persisted to .devflow/state-NN.json)
│   │       ├── version.rs              # Version file I/O
│   │       ├── workflow.rs             # State persistence (I/O, migration)
│   │       ├── worktree.rs             # Git worktree operations
│   │       ├── agents/
│   │       │   ├── mod.rs              # AgentAdapter trait + adapter factory
│   │       │   ├── claude.rs           # Claude Code CLI adapter
│   │       │   ├── codex.rs            # OpenAI Codex CLI adapter
│   │       │   └── opencode.rs         # OpenCode CLI adapter
│   │       └── tests/
│   │           └── monitor_e2e.rs      # E2E monitor tests
│   │
│   └── devflow-cli/                    # Binary: CLI wrapper around core
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # CLI command dispatcher (Start, Advance, Gate, Logs, Parallel, Sequentagent)
│           └── tests/
│               ├── help_snapshot.rs    # Snapshot test for --help output
│               ├── devcontainer_ci_failfast.rs
│               ├── gitignore_coverage.rs
│               ├── log_format_env.rs    # Tests for DEVFLOW_LOG_FORMAT=json
│               └── phase7_cli.rs        # Integration tests for Phase 7
│
├── .devflow/                            # Runtime state directory (git-ignored)
│   ├── state-NN.json                   # Per-phase workflow state (YAML-like structure)
│   ├── lock-NN                         # Per-phase lock file (contains PID)
│   ├── lock-project                    # Project-wide lock (short-held)
│   ├── phase-NN-stdout                 # Agent stdout capture
│   ├── phase-NN-stderr.log             # Agent stderr capture
│   ├── phase-NN-exit                   # Agent exit code (0 or non-zero)
│   ├── phase-NN-agent-pid              # Agent process ID (for recovery)
│   ├── cron-instructions-NN.json       # Rate-limit resumption manifest
│   ├── gates/
│   │   ├── NN-stage.json               # Gate request (written by workflow)
│   │   ├── NN-stage.response.json      # Gate response (written by human/Hermes)
│   │   └── NN-stage.ack.json           # Gate receipt (written by workflow)
│   └── events.jsonl                    # Structured event log
│
├── .worktrees/                          # Git worktree directory (git-ignored)
│   ├── phase-NN/                       # Worktree checked out at feature/phase-NN
│   └── phase-NN-agent/                 # For multi-agent runs (sequentagent, parallel)
│
├── docs/
│   ├── ARCHITECTURE.md                 # System design
│   ├── OPERATIONS.md                   # Operator reference (.devflow/ file inventory, env vars)
│   ├── DEPENDENCIES.md                 # Tool + agent dependency matrix
│   └── CONTRIBUTING.md
│
└── scripts/
    └── install.sh                      # Installation script (curl -fsSL ... | bash)
```

## Directory Purposes

**`crates/devflow-core/`:**
- Purpose: Core library encapsulating the workflow state machine, agent adapters, and git operations
- Contains: Public API types (State, Stage, Mode, AgentAdapter), error types, helper functions
- Key files: `lib.rs` (module re-exports), `stage.rs` (5-stage pipeline), `state.rs` (persisted state)

**`crates/devflow-cli/`:**
- Purpose: Thin CLI wrapper; translates user commands to core library calls
- Contains: Command-line argument parsing (clap), error formatting, output rendering
- Key files: `main.rs` (single 1000+ line file with all commands)

**`.devflow/`:**
- Purpose: Persistent workflow state directory (always `.devflow` at project root, never configurable)
- Contains: State files, locks, capture files, gates, cron instructions
- Git-ignored: Yes (added to `.gitignore`)
- Committed: No

**`.worktrees/`:**
- Purpose: Isolated git worktrees for each phase (linked checkouts sharing the main `.git`)
- Contains: One subdirectory per active phase (`phase-NN/`)
- Git-ignored: No (worktrees are git-native; `.git/worktrees/` metadata lives in the main repo)
- Committed: No

## Key File Locations

**Entry Points:**
- `crates/devflow-cli/src/main.rs`: Single entry point; parse CLI args, dispatch to command handlers

**Configuration:**
- No config file required (all options are CLI flags to `devflow start`)
- `crates/devflow-core/src/config.rs`: Hardcoded git-flow constants (main=main, develop=develop, feature_prefix=feature/)

**Core Logic:**
- `crates/devflow-core/src/stage.rs`: Stage enum and pipeline logic (Define → Plan → Code → Validate → Ship)
- `crates/devflow-core/src/state.rs`: State struct (agent, phase, stage, mode, gate_pending, consecutive_failures, worktree_path)
- `crates/devflow-core/src/workflow.rs`: State persistence (read/write `.devflow/state-{N:02}.json`)
- `crates/devflow-core/src/monitor.rs`: Background daemon spawning and agent ownership
- `crates/devflow-core/src/agents/mod.rs`: AgentAdapter trait; factory function `adapter_for()`

**Testing:**
- `crates/devflow-core/tests/monitor_e2e.rs`: E2E monitor tests
- `crates/devflow-cli/tests/`: Integration tests (help snapshot, log format, phase 7 CLI)

**Runtime Files:**
- `.devflow/state-{NN:02}.json`: Per-phase workflow state (read/write by `workflow.rs`)
- `.devflow/lock-{NN:02}`: Per-phase lock (created/released by `lock.rs`)
- `.devflow/phase-{NN:02}-stdout`: Agent stdout capture (written by monitor shell script)
- `.devflow/phase-{NN:02}-exit`: Agent exit code (written by monitor shell script)
- `.devflow/gates/{NN:02}-{stage}.json`: Gate request (written by `gates.rs`)
- `.devflow/gates/{NN:02}-{stage}.response.json`: Human response (written by user/Hermes)

## Naming Conventions

**Files:**
- State: `.devflow/state-{phase:02d}.json` (zero-padded 2-digit phase number)
- Locks: `.devflow/lock-{phase:02d}` (same padding)
- Capture: `.devflow/phase-{phase:02d}-stdout` (stdout), `-stderr.log` (stderr), `-exit` (exit code), `-agent-pid` (PID)
- Gates: `.devflow/gates/{phase:02d}-{stage}.json` (request), `.response.json` (response), `.ack.json` (receipt)
- Worktrees: `.worktrees/phase-{phase:02d}/` (basic), `.worktrees/phase-{phase:02d}-{agent}/` (multi-agent)
- Cron instructions: `.devflow/cron-instructions-{phase:02d}.json`

**Directories:**
- Phases use lowercase: `phase-NN`, `feature/phase-NN`
- Stages use lowercase: `define`, `plan`, `code`, `validate`, `ship`
- Agents use lowercase: `claude`, `codex`, `opencode`

**Functions:**
- Paths: `*_path()` (returns PathBuf)
- Operations: verb + noun, snake_case (e.g., `feature_start()`, `gate_approve()`)
- Predicates: `is_*()` or `should_*()` (e.g., `is_gate()`, `should_gate()`)
- Factories: `*_for()` (e.g., `adapter_for()`)

**Modules:**
- Trait + implementations in same file (e.g., `agent.rs` for `agent_running()`)
- Adapter implementations in submodule (e.g., `agents/claude.rs` for `ClaudeAgent`)
- Error types co-located with responsibility (e.g., `GitError` in `git.rs`)

## Where to Add New Code

**New Stage or Pipeline Logic:**
- Primary: `crates/devflow-core/src/stage.rs` (update Stage enum, next() method, is_gate(), is_agent_stage())
- Secondary: `crates/devflow-core/src/prompt.rs` (add stage-specific prompt function)
- Tertiary: `crates/devflow-cli/src/main.rs` (add Advance command arm for new stage)

**New Agent Adapter:**
- Implementation: `crates/devflow-core/src/agents/{agent_name}.rs` (implement AgentAdapter trait)
- Registration: `crates/devflow-core/src/agents/mod.rs` (add to `adapter_for()` match arm)
- CLI: `crates/devflow-cli/src/main.rs` (add variant to `--agent` arg value parsing)
- Tests: Add snapshot/integration tests in `crates/devflow-cli/tests/`

**New Git Operation:**
- Implementation: `crates/devflow-core/src/git.rs` (add method to GitFlow impl block)
- Usage: Call from `crates/devflow-cli/src/main.rs` (Start, Advance, or Cleanup command)
- Tests: Add unit tests at bottom of git.rs file

**New Gate or Gate Decision Logic:**
- Gate protocol: `crates/devflow-core/src/gates.rs` (gate request/response/ack structures, decision logic)
- Advance integration: `crates/devflow-cli/src/main.rs` (Advance command arm for the stage)
- Tests: Add tests in gates.rs; integration tests in CLI tests

**New Command:**
- CLI enum: Add variant to `Command` enum in `crates/devflow-cli/src/main.rs`
- Handler: Implement command handler in main.rs (or split to a submodule if complex)
- Core logic: Implement in `crates/devflow-core/` (new module or extend existing)
- Tests: Add integration tests in `crates/devflow-cli/tests/`

**New Logging or Observability:**
- Tracing integration: Already set up in `crates/devflow-core/src/lib.rs` (RUST_LOG, DEVFLOW_LOG_FORMAT)
- Events: Add structured event emission via `tracing::info!()` in relevant modules
- Tests: Add test in `crates/devflow-cli/tests/log_format_env.rs`

**Error Handling:**
- Error types: Define per-module (e.g., `GitError`, `WorkflowError`, `LockError`)
- Pattern: Use `#[from]` attributes for automatic conversion in `Result<T, E>` contexts
- No panics: Use `.map_err()` and `?` operator; never `.unwrap()` in production code

## Special Directories

**`.devflow/` (Runtime State):**
- Purpose: Persistent per-phase workflow state and gate protocol files
- Generated: Yes (created by CLI on first use)
- Committed: No (git-ignored)
- Cleanup: `devflow cleanup` removes phase worktrees and branches; leaves state/capture files for audit

**`.worktrees/` (Git Worktrees):**
- Purpose: Isolated git worktrees per phase, each with its own working directory but shared `.git` object database
- Generated: Yes (created by `git worktree add`)
- Committed: No (not git-tracked; worktree metadata lives in main `.git/worktrees/`)
- Cleanup: `devflow cleanup` removes worktree directories; `git worktree remove` cleans git metadata

**`docs/` (Documentation):**
- ARCHITECTURE.md — system design (you're reading this codebase map, which corresponds to ARCHITECTURE.md at the project level)
- OPERATIONS.md — operator reference (gate protocol, env vars, .devflow/ file inventory)
- DEPENDENCIES.md — tool and agent dependency matrix
- CONTRIBUTING.md — how to contribute

**`crates/devflow-cli/tests/` (Integration Tests):**
- Not unit tests (which live in their respective modules)
- Integration tests that spawn the full CLI and check behavior
- Examples: help snapshot, log format (JSON vs. text), phase 7 CLI walkthrough

---

*Structure analysis: 2026-07-17*
