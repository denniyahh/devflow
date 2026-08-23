# Codebase Structure

**Analysis Date:** 2026-07-22

## Directory Layout

```text
devflow/
├── crates/
│   ├── devflow-core/                    # Public library: workflow state, adapters, git, gates, hooks
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs                   # Public module declarations and logging setup
│   │   │   ├── agent.rs                 # Agent process helpers
│   │   │   ├── agent_result.rs          # Layered agent-result evaluation
│   │   │   ├── config.rs                # Git-flow and runtime configuration
│   │   │   ├── events.rs                # Structured event emission
│   │   │   ├── gates.rs                 # Gate request/response/ack protocol
│   │   │   ├── git.rs                   # GitFlow operations
│   │   │   ├── hooks.rs                 # Pipeline hook execution
│   │   │   ├── lock.rs                  # Per-phase and project checkout locks
│   │   │   ├── mode.rs                  # Pipeline modes and retry policy
│   │   │   ├── monitor.rs               # Detached monitor spawning and capture
│   │   │   ├── outcome_policy.rs        # Agent outcome to pipeline action policy
│   │   │   ├── prompt.rs                # Stage prompts and completion contract
│   │   │   ├── recover.rs               # Stale-state inspection and cleanup
│   │   │   ├── ship.rs                  # Rate-limit cron instructions
│   │   │   ├── stage.rs                 # Define/Plan/Code/Validate/Ship model
│   │   │   ├── state.rs                 # Per-phase persisted State
│   │   │   ├── verify.rs                # Validation verdict parsing
│   │   │   ├── version.rs               # Version file I/O
│   │   │   ├── workflow.rs              # State persistence and .devflow setup
│   │   │   ├── worktree.rs              # Git worktree operations
│   │   │   └── agents/                  # AgentAdapter implementations and factory
│   │   └── tests/                    # Core integration tests
│   └── devflow-cli/                     # Binary crate: clap routing and orchestration
│       ├── Cargo.toml
│       ├── build.rs                   # Build provenance environment values
│       ├── src/
│       │   ├── main.rs               # 478 lines: argument types, CliError, dispatch, project_root
│       │   ├── commands.rs           # 2,326 lines: handlers, display, doctor reconciliation
│       │   ├── pipeline_launch.rs    # 585 lines: launch/resume/advance seam
│       │   ├── pipeline_outcomes.rs  # 1,719 lines: outcome handling and checkout hooks
│       │   ├── pipeline_gate.rs      # 789 lines: transitions, gates, finish, abort
│       │   ├── preflight.rs          # 772 lines: pre-launch readiness checks
│       │   ├── staleness.rs          # 1,284 lines: build provenance/staleness enforcement
│       │   ├── parallel.rs           # 530 lines: parallel and sequentagent orchestration
│       │   ├── config_parse.rs       # 75 lines: timeout parsing and escalation threshold
│       │   └── test_support.rs       # 288 lines: shared CLI test fixtures and ENV_MUTEX
│       └── tests/                    # CLI integration tests and snapshots
├── .devflow/                             # Generated, git-ignored runtime state
├── .worktrees/                           # Generated phase worktrees
├── docs/                                 # MkDocs source under guides/, architecture/, diagrams/
├── scripts/                              # Install, deploy, and branch-sync shell entrypoints
├── ARCHITECTURE.md                       # Root architecture overview
├── CONTRIBUTING.md                       # Contributor workflow and review rules
├── DEPENDENCIES.md                       # Dependency matrix
└── OPERATIONS.md                         # Operator reference
```

## Directory Purposes

**`crates/devflow-core/`:**
- Public library for persisted workflow state, policies, adapters, git operations, gates, hooks, monitoring, and worktrees.
- Its API has external crate consumers, so intentional exports use `pub`.

**`crates/devflow-cli/`:**
- Binary crate that parses commands and coordinates `devflow-core`.
- `main.rs` is a thin crate root; operational code belongs in the flat sibling module that owns the behavior.
- Cross-module CLI items use `pub(crate)`, never unrestricted `pub`, because the binary crate has no external API consumers.

**`.devflow/`:**
- Generated project-local state, locks, captures, gate files, cron instructions, and `events.jsonl`.
- The directory writes its own `.gitignore` marker and must never enter repository history.

**`.worktrees/`:**
- Generated linked checkouts for phase and multi-agent isolation.
- Git worktree metadata lives under the repository's common Git directory.

## Key File Locations

**Entry and Commands:**
- `crates/devflow-cli/src/main.rs`: clap argument types, top-level dispatch, `CliError`, and project-root resolution.
- `crates/devflow-cli/src/commands.rs`: command handlers, output rendering, and doctor reconciliation.
- `crates/devflow-cli/src/parallel.rs`: parallel phase startup and the blocking sequentagent handoff.
- `crates/devflow-cli/src/config_parse.rs`: environment-backed timeout parsing.

**Pipeline:**
- `crates/devflow-cli/src/pipeline_launch.rs`: launch, resume, and evaluated-result dispatch.
- `crates/devflow-cli/src/pipeline_outcomes.rs`: typed result handling, checkout hooks, and gate-context rendering.
- `crates/devflow-cli/src/pipeline_gate.rs`: transitions, loop-backs, gates, completion, and abort.
- `crates/devflow-cli/src/preflight.rs`: readiness checks before monitor spawn.
- `crates/devflow-cli/src/staleness.rs`: build provenance and self-dogfood staleness enforcement.

**Core State and Protocols:**
- `crates/devflow-core/src/state.rs`: persisted `State`.
- `crates/devflow-core/src/stage.rs`: `Stage` and stage progression.
- `crates/devflow-core/src/workflow.rs`: per-phase state files and `.devflow` creation.
- `crates/devflow-core/src/gates.rs`: gate protocol.
- `crates/devflow-core/src/agents/mod.rs`: `AgentAdapter` and `adapter_for`.

**Testing:**
- `crates/devflow-cli/src/test_support.rs`: shared CLI unit-test fixtures and the crate-wide `ENV_MUTEX`.
- `crates/devflow-cli/tests/`: binary integration tests and the help snapshot.
- `crates/devflow-core/tests/`: core integration tests.

## Naming Conventions

**Runtime files:**
- State: `.devflow/state-{phase:02}.json`
- Phase lock: `.devflow/lock-{phase:02}`
- Project checkout lock: `.devflow/lock-project`
- Captures: `.devflow/phase-{phase:02}-stdout`, `-stderr.log`, `-exit`, `-agent-pid`
- Gates: `.devflow/gates/{phase:02}-{stage}.json`, `.response.json`, `.ack.json`
- Cron instructions: `.devflow/cron-instructions-{phase:02}.json`
- Worktrees: `.worktrees/phase-{phase:02}/` and `.worktrees/phase-{phase:02}-{agent}/`

**Rust:**
- Paths use `*_path`; operations use verb-noun snake case; predicates use `is_*` or `should_*`; factories use `*_for`.
- Unit tests live at the bottom of the module whose production function they exercise.
- CLI sibling APIs are `pub(crate)`. Core library exports may be `pub` when consumed outside their module or crate.

## Where to Add New Code

**New command:**
- Add clap argument shape and routing in `crates/devflow-cli/src/main.rs`.
- Put the handler and display helpers in `crates/devflow-cli/src/commands.rs`.
- Put reusable state, protocol, or git behavior in the owning `crates/devflow-core/src/` module.

**Pipeline behavior:**
- Start from the owning seam: launch/resume in `pipeline_launch.rs`, result handling in `pipeline_outcomes.rs`, or transitions/gates in `pipeline_gate.rs`.
- The three pipeline modules are mutually cyclic by design. A pipeline change is likely to touch two or three together; the split provides reviewable `pub(crate)` boundaries, not pipeline-internal wave parallelism.
- `preflight.rs` and `pipeline_launch.rs` are also bidirectionally coupled: launch invokes preflight, while an approved preflight advance invokes `launch_stage_inner`. Plan changes to either with both files in view.

**Independent CLI behavior:**
- Commands/display, staleness, parallel orchestration, and config parsing are separate clusters and can usually be planned independently of the pipeline and of each other. This is the main wave-parallelism benefit of the split.

**New agent adapter:**
- Add the implementation under `crates/devflow-core/src/agents/` and register it in `crates/devflow-core/src/agents/mod.rs`.
- Extend CLI parsing only if the adapter needs a new `AgentKind` value.

**New git, gate, or persistence behavior:**
- Git operations belong in `crates/devflow-core/src/git.rs`.
- Gate protocol behavior belongs in `crates/devflow-core/src/gates.rs`; pipeline integration belongs in the appropriate pipeline seam.
- Runtime path construction and state I/O belong in `crates/devflow-core/src/workflow.rs`.

## Special Directories

**`.devflow/`:** Generated runtime evidence. Cleanup is deliberately conservative so captures and events remain available for diagnosis.

**`.worktrees/`:** Generated linked worktrees. Use Git worktree operations for removal so shared metadata remains consistent.

**`docs/`:** MkDocs content. Root operator and contributor documents remain `OPERATIONS.md`, `ARCHITECTURE.md`, `DEPENDENCIES.md`, and `CONTRIBUTING.md`.

**`crates/devflow-cli/tests/` and `crates/devflow-core/tests/`:** Integration-test binaries. Unit tests stay beside their production modules.

---

*Structure analysis: 2026-07-22*
