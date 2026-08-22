<!-- refreshed: 2026-07-17 -->
# Architecture

**Analysis Date:** 2026-07-17

## System Overview

```text
┌─────────────────────────────────────────────────────────────┐
│                   CLI Dispatcher                            │
│              `crates/devflow-cli/src/main.rs`               │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│            State Machine (core lib)                          │
│  Define → Plan → Code → Validate → Ship (linear chain)      │
│         `crates/devflow-core/src/stage.rs`                  │
│         `crates/devflow-core/src/state.rs`                  │
└────┬─────────────────────┬───────────────────┬──────────────┘
     │                     │                   │
     ▼                     ▼                   ▼
┌──────────────────┐ ┌────────────────┐ ┌──────────────────┐
│  Agent Stage     │ │  Gate Stage    │ │  Execution Mode  │
│  (Define/Plan/   │ │  (Validate/    │ │  (Auto/Supervise)│
│   Code)          │ │   Ship)        │ │                  │
│                  │ │                │ │  `mode.rs` (0-46)│
│ ┌──────────────┐ │ │  Fires gates   │ └──────────────────┘
│ │ Prompt       │ │ │  to .devflow/  │
│ │ Stage-spec. │ │ │  gates/ for    │
│ │ CLI command  │ │ │  human review  │
│ │ `prompt.rs`  │ │ │  `gates.rs`    │
│ └──────────────┘ │ │  (gate protocol)
│                  │ │
│ ┌──────────────┐ │ │
│ │Agent Adapter │ │ │
│ │(Claude/Codex │ │ │
│ │/OpenCode)    │ │ │
│ │`agents/`     │ │ │
│ │ mod.rs,      │ │ │
│ │ claude.rs    │ │ │
│ └──────────────┘ │ │
└──────────────────┘ └────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│            Monitor Daemon                                    │
│  Spawns detached process that owns the agent, captures      │
│  output, records exit code, then calls devflow advance      │
│  `crates/devflow-core/src/monitor.rs`                       │
└────────────────┬─────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│            Agent Process Tree                                │
│  Agent binary (claude/codex/opencode) runs non-interactive  │
│  Stdout/stderr captured to .devflow/phase-NN-{stdout,stderr}│
│ Exit code written to .devflow/phase-NN-exit                 │
└────────────────┬─────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│            Git Operations                                    │
│  Feature branching: develop → feature/phase-NN → develop    │
│  Release branching: → release/vX.Y.Z → main + develop       │
│  Worktree isolation: .worktrees/phase-NN (linked checkout)  │
│  `crates/devflow-core/src/git.rs` (GitFlow helper)          │
│  `crates/devflow-core/src/worktree.rs` (git worktree cmds)  │
└────────────────┬─────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│            State & Result Persistence                        │
│  .devflow/state-NN.json  (per-phase state)                   │
│  .devflow/phase-NN-exit  (exit code)                         │
│  .devflow/lock-NN        (per-phase lock)                    │
│  .devflow/gates/{NN,stage}*.json (gate protocol)             │
│  `crates/devflow-core/src/workflow.rs` (state I/O)           │
│  `crates/devflow-core/src/agent_result.rs` (result parsing)  │
└─────────────────────────────────────────────────────────────┘
```

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| CLI | Argument parsing, command dispatch, error formatting | `crates/devflow-cli/src/main.rs` |
| State Machine | Stage transitions, advance logic, gate decision | `crates/devflow-core/src/state.rs`, `stage.rs` |
| Agent Adapter | Build CLI command for agent (Claude/Codex/OpenCode) | `crates/devflow-core/src/agents/mod.rs`, `claude.rs`, `codex.rs`, `opencode.rs` |
| Prompt Builder | Stage-specific GSD commands + completion contract | `crates/devflow-core/src/prompt.rs` |
| Monitor Daemon | Spawn agent, capture output, auto-advance state machine | `crates/devflow-core/src/monitor.rs` |
| Git Helper | Feature/release branching, worktree lifecycle | `crates/devflow-core/src/git.rs`, `worktree.rs` |
| Gate Protocol | Request/response/ack file I/O for human pause points | `crates/devflow-core/src/gates.rs` |
| Result Evaluator | Three-layer result parsing (DEVFLOW_RESULT, exit code, heuristic) | `crates/devflow-core/src/agent_result.rs` |
| Locking | Per-phase locks + optional project-wide lock | `crates/devflow-core/src/lock.rs` |
| Recovery | Inspect/cleanup stale workflow state | `crates/devflow-core/src/recover.rs` |

## Pattern Overview

**Overall:** Linear state machine driven by a background monitor daemon. No scheduler, no cron, no shared global state across phases.

**Key Characteristics:**
- **Stateless CLI:** Each invocation reads state from disk, never maintains process memory between commands
- **Per-phase isolation:** Every phase has its own state file, lock, and (optional) worktree; `devflow parallel` sibling phases never block each other
- **Monitor ownership:** The background daemon owns the agent process and capture files — the CLI exits but the workflow continues unattended
- **Three-layer result evaluation:** Agents communicate via DEVFLOW_RESULT JSON marker (authoritative); fallback to exit code + commit count (reliable); final fallback to heuristic (last resort)
- **Gate as pause points:** Not exceptions — gates are the normal mechanism for human review (Validate in Supervise, Ship in both modes)
- **Worktree by default:** Agent runs in an isolated `.worktrees/phase-NN/` linked checkout, preventing cross-phase git contamination

## Layers

**Orchestration Layer (CLI):**
- Purpose: Parse arguments, dispatch commands, format output for humans and machines
- Location: `crates/devflow-cli/src/main.rs`
- Contains: Command structs (Start, Advance, Gate, Logs, Parallel, Sequentagent), error handling, output formatting
- Depends on: `devflow_core` library (all core modules)
- Used by: Users via `devflow` binary

**State Machine Layer (core lib):**
- Purpose: Manage workflow stage transitions, gate decisions, and per-phase state persistence
- Location: `crates/devflow-core/src/` (state.rs, stage.rs, mode.rs, workflow.rs)
- Contains: State struct (phase, stage, agent, mode, gate_pending, consecutive_failures), Stage enum (Define/Plan/Code/Validate/Ship), Mode enum (Auto/Supervise)
- Depends on: serde (JSON), tracing (logging)
- Used by: CLI (reads/writes state), Advance command (transitions states), Monitor (spawns with state)

**Agent Execution Layer (core lib):**
- Purpose: Launch coding agents with stage-specific prompts, manage process lifecycle
- Location: `crates/devflow-core/src/agents/`, monitor.rs, prompt.rs, agent.rs
- Contains: AgentAdapter trait (name, exec_command, extra_env, completion_signal_detected), three adapter implementations (Claude/Codex/OpenCode), stage-specific prompts, DEVFLOW_RESULT completion contract
- Depends on: Process control, shell escaping, prompt templates
- Used by: Monitor (to build launch command), Stage Validate (to detect completion)

**Result Evaluation Layer (core lib):**
- Purpose: Parse agent output and determine success/failure
- Location: `crates/devflow-core/src/agent_result.rs`
- Contains: AgentResult struct, three-layer decision logic (DEVFLOW_RESULT marker → exit code + commits → heuristic), rate-limit detection
- Depends on: serde_json (parse JSON markers), file I/O (read capture files)
- Used by: Advance command (evaluates result, decides next stage)

**Git Operations Layer (core lib):**
- Purpose: Git-flow operations (feature branch lifecycle, release branching, worktree management)
- Location: `crates/devflow-core/src/git.rs`, worktree.rs
- Contains: GitFlow struct (feature_start, feature_finish, release_start, release_finish, tag), Worktree helper (add, add_detached, remove, list)
- Depends on: `git` command (spawned via Command), libc (for PID operations)
- Used by: Start (creates feature branch + worktree), Advance (merges + tags on Ship), Cleanup (removes worktree)

**Synchronization Layer (core lib):**
- Purpose: Prevent concurrent state mutations within a phase, serialize project-wide git operations
- Location: `crates/devflow-core/src/lock.rs`
- Contains: LockGuard (per-phase lock), project-wide lock for git operations, stale-holder recovery
- Depends on: File I/O (create_new with O_EXCL for atomicity), libc (PID checking)
- Used by: Advance (acquires per-phase lock across entire stage execution, including gate waits), Ship (acquires project lock during version bump/merge)

**Gate Protocol Layer (core lib):**
- Purpose: File-based handoff between workflow and human (via Hermes or manual intervention)
- Location: `crates/devflow-core/src/gates.rs`
- Contains: GateFile (request), GateResponse (human answer), GateAck (receipt), gate action decision logic
- Depends on: File I/O (atomic writes to .devflow/gates/), serde_json (gate payloads)
- Used by: Advance (fires gates), Gate subcommand (human responds)

**Capture & Persistence Layer (core lib):**
- Purpose: Store workflow state, agent output, exit codes, gate records
- Location: `crates/devflow-core/src/workflow.rs`, agent_result.rs, lock.rs, gates.rs, ship.rs
- Contains: State JSON I/O (atomic writes), capture file paths, cron-instructions persistence
- Depends on: File I/O, serde_json
- Used by: All layers (read state, write results, capture output)

## Data Flow

### Primary Request Path: `devflow start --phase N --agent X --mode auto`

1. **Start command** (`crates/devflow-cli/src/main.rs:Start { ... }`)
   - Parse args: phase, agent kind, mode, worktree flag
   - Call `workflow::state_path()` to locate `.devflow/state-{N:02}.json`

2. **Create state** (`crates/devflow-core/src/state.rs`)
   - New state: stage=Define, phase=N, agent=X, mode=auto, project_root
   - Worktree enabled → set `state.worktree_path = .worktrees/phase-{N:02}`

3. **Create feature branch + worktree** (`crates/devflow-core/src/git.rs`, `worktree.rs`)
   - GitFlow::feature_start(N) → `git checkout develop; git checkout -b feature/phase-NN`
   - Worktree::add(.worktrees/phase-NN, feature/phase-NN, develop) → `git worktree add -b feature/phase-NN .worktrees/phase-NN develop`

4. **Save initial state** (`crates/devflow-core/src/workflow.rs`)
   - `workflow::save_state(&state)` → write to `.devflow/state-{N:02}.json` (atomic via temp file)

5. **Spawn monitor** (`crates/devflow-core/src/monitor.rs`)
   - Build agent command: `agents::adapter_for(X).exec_command(N, prompt, extra_writable_roots)`
   - Spawn detached shell process:
     - Sets up capture files: `.devflow/phase-{N:02}-{stdout,stderr,exit,agent-pid}`
     - Launches agent with prompt (non-interactive: `claude -p`, `codex exec`, `opencode ...`)
     - Waits for agent exit, records exit code to `.devflow/phase-{N:02}-exit`
     - Calls `devflow advance --phase N` (CLI exits here; monitor stays alive)

### Secondary Flow: `devflow advance --phase N` (auto-triggered by monitor)

1. **Acquire per-phase lock** (`crates/devflow-core/src/lock.rs`)
   - Acquire `.devflow/lock-{N:02}` (fails if held by sibling phase)
   - Lock is held through entire stage execution, including gate waits

2. **Load state** (`crates/devflow-core/src/workflow.rs`)
   - Read `.devflow/state-{N:02}.json`

3. **Evaluate result** (`crates/devflow-core/src/agent_result.rs`)
   - Layer 1: Parse `.devflow/phase-{N:02}-stdout` for `DEVFLOW_RESULT: {...}` marker
     - If found and valid JSON with `status: "success"`, use that result
   - Layer 2: If Layer 1 failed, check exit code (must be 0) AND commit count on feature branch
   - Layer 3: If Layer 2 failed, check heuristic (commits exist + no process = probable success)

4. **Advance stage** (stage-specific logic in `advance()`)
   - **Define → Plan:** Check result was success, advance to Plan, spawn monitor for next stage
   - **Plan → Code:** Check result was success, advance to Code, spawn monitor for next stage
   - **Code → Validate:** Check result was success, advance to Validate, spawn monitor for next stage
   - **Validate → Ship or Code:** 
     - Check result has `verdict: "pass"` (if "gaps", loop back to Code)
     - If consecutive_failures >= MAX_CONSECUTIVE_FAILURES and mode=Auto, fire gate (else advance)
   - **Ship → Done:** Merge feature branch to develop, create release branch, tag, merge to main

5. **Gate decision** (stage-specific)
   - If gate should fire (`mode.should_gate(stage, failures)`):
     - Write gate request to `.devflow/gates/{N:02}-{stage}.json`
     - If DEVFLOW_GATE_NOTIFY_CMD set, invoke it with phase/stage env
     - Wait for human response (polling with backoff up to DEVFLOW_GATE_TIMEOUT_SECS, default 7 days)
     - Read response from `.devflow/gates/{N:02}-{stage}.response.json`
     - Write ACK to `.devflow/gates/{N:02}-{stage}.ack.json`
     - GateAction::Advance → continue, GateAction::LoopBack(Code) → loop, GateAction::Abort → stop

6. **Release the lock** (`crates/devflow-core/src/lock.rs`)
   - Drop LockGuard → delete `.devflow/lock-{N:02}` (RAII cleanup)

7. **If not at Ship, spawn monitor for next stage** (step 5 from Primary Flow)

### Parallel Execution: `devflow parallel --phases 7,8`

- Parse phase list and agent list
- For each phase:
  - Acquire project-wide lock (short-held, just for worktree/branch setup)
  - Create feature branch + worktree
  - Release project lock
  - Spawn monitor (runs in background)
- Return immediately; monitors advance sibling phases concurrently
- Each monitor holds its own per-phase lock (not shared)

**State Management:**
- **Per-phase:** `.devflow/state-{N:02}.json` survives restarts; includes full workflow metadata
- **Global (project-wide):** No singleton state file; only per-phase files exist
- **Gates:** Request/response/ack files persist until cleaned up
- **Locks:** Per-phase and project-wide locks cleaned up via RAII (LockGuard::Drop)

## Key Abstractions

**AgentAdapter Trait:**
- Purpose: Encapsulate agent-specific CLI flags and prompt wrapping
- Examples: `crates/devflow-core/src/agents/claude.rs`, `codex.rs`, `opencode.rs`
- Pattern: Adapter method `exec_command()` returns `(program, args)` from a stage prompt; no prompt modification, only CLI wrapping
- Key methods:
  - `name()` → "Claude Code" | "OpenAI Codex" | "OpenCode"
  - `exec_command(phase, prompt, extra_writable_roots)` → (program, args)
  - `extra_env()` → vec of env var overrides (Codex uses this to disable commit signing)
  - `completion_signal_detected(output)` → bool (agent-specific heuristic for completion)

**Stage Enum:**
- Purpose: Represent the five stages in the linear pipeline
- Pattern: Immutable enum with methods (not a state struct)
  - `next()` → Option<Stage> (Define→Plan→Code→Validate→Ship→None)
  - `is_gate()` → bool (only Validate and Ship)
  - `is_agent_stage()` → bool (only Define, Plan, Code)
  - `gsd_command()` → &'static str ("/gsd-discuss-phase {N}", etc.)

**Mode Enum + MAX_CONSECUTIVE_FAILURES:**
- Purpose: Determine whether gates fire and whether Code↔Validate auto-loops
- Pattern: `mode.should_gate(stage, consecutive_failures)` → bool
  - Ship always gates (both modes)
  - Validate gates only in Supervise, or in Auto after ≥3 consecutive failures
  - Other stages never gate
  - `mode.should_auto_loop(stage)` → bool (only Validate in Auto mode auto-loops)

**GitFlow Helper:**
- Purpose: Wrap git commands for feature/release/tag operations
- Pattern: Constructor takes project root; methods call `git` command, parse output, handle errors
- Separation: Distinct from Worktree helper — GitFlow is high-level git-flow (branching, merging, tagging); Worktree is low-level `git worktree` management

**Lock Pattern:**
- Purpose: Serialize state transitions and git operations
- Pattern: LockGuard (RAII) — acquire() returns a guard; lock released when guard dropped
- Levels: Per-phase lock (held across stage + gate), project lock (held briefly during git ops)
- Stale-holder recovery: If recorded PID is dead, reclaim lock (prevents wedging after crashes)

## Entry Points

**`devflow start`:**
- Location: `crates/devflow-cli/src/main.rs:Cli::Start { ... }`
- Triggers: User runs `devflow start --phase N --agent X --mode auto`
- Responsibilities:
  1. Parse args
  2. Create state and feature branch + worktree
  3. Save state to disk
  4. Spawn monitor (CLI returns; monitor stays alive)

**`devflow advance`:**
- Location: `crates/devflow-cli/src/main.rs:Cli::Advance { ... }` (hidden command, internal use only)
- Triggers: Monitor calls this after agent exits
- Responsibilities:
  1. Load state from disk
  2. Evaluate agent result (three layers)
  3. Decide next stage (or gate, or loop)
  4. Save updated state
  5. Fire gate or spawn monitor for next stage
  6. Release lock

**`devflow gate`:**
- Location: `crates/devflow-cli/src/main.rs:Cli::Gate { ... }`
- Triggers: Human uses `devflow gate list` or `devflow gate approve/reject <phase> ...`
- Responsibilities:
  1. Read open gate requests from `.devflow/gates/`
  2. Write human response to `.devflow/gates/{phase}-{stage}.response.json`
  3. Advance command wakes up and reads response, resumes workflow

**`devflow logs`:**
- Location: `crates/devflow-cli/src/main.rs:Cli::Logs { ... }`
- Triggers: User runs `devflow logs --phase N [--follow]`
- Responsibilities:
  1. Read capture file (`.devflow/phase-{N:02}-stdout` or `-stderr.log`)
  2. Print or tail with follow (useful for debugging agent runs)

**`devflow parallel`:**
- Location: `crates/devflow-cli/src/main.rs:Cli::Parallel { ... }`
- Triggers: User runs `devflow parallel --phases 7,8 [--agents claude,codex]`
- Responsibilities:
  1. For each phase, run `devflow start` internally (isolated worktrees, feature branches)
  2. Spawn all monitors concurrently
  3. Return immediately; phases advance in parallel

**`devflow sequentagent`:**
- Location: `crates/devflow-cli/src/main.rs:Cli::Sequentagent { ... }`
- Triggers: User runs `devflow sequentagent --phase N --agents claude,codex`
- Responsibilities:
  1. For first agent: spawn monitor (no auto-advance)
  2. Wait for agent to exit
  3. Rebase feature branch onto updated develop (or surface conflicts)
  4. For second agent: spawn monitor with rebased base
  5. Wait for agent to exit
  6. Then call advance to ship the phase

## Architectural Constraints

- **Threading:** Single-threaded event loop per process (Rust standard library async not used; blocking I/O with monitor spawning as a detached child). No shared mutable state across threads within a single process. Per-phase locks prevent concurrent mutations on the same phase's state/branch.
- **Global state:** No module-level singletons. State is always read from disk (`.devflow/state-NN.json`), never cached in process memory. This allows CLI to be invoked multiple times without state conflicts.
- **Circular imports:** Minimal module coupling. Agent adapters depend on prompt; prompt is independent; state machine independent; gate protocol independent. No circular dependencies in the module graph.
- **Worktree isolation:** When worktree mode is enabled (default), the agent's cwd is `.worktrees/phase-NN`, but all state/lock/capture files live under the project root. This prevents git confusion while keeping metadata centralized.
- **Git operations are synchronous:** No async git operations. Every git command is spawned as a child process, waits for completion, and checks exit code. This is safe because git operations are fast relative to agent execution (seconds vs. minutes).
- **Rate-limit resumption:** Detected via agent-specific heuristics in stdout (Claude JSON envelope, Codex plain-text "Try again at"). If detected, a `CronInstructions` manifest is written to `.devflow/cron-instructions-{N:02}.json` for Hermes to reschedule.

## Anti-Patterns

### Over-reliance on Global Config File

**What happens:** Early designs used a `.devflow.toml` or `devflow.json` config file for workflow settings (mode, agent, phase). This led to ambiguity: was state in the file or on disk?

**Why it's wrong:** Config files become stale (e.g., a phase completes but the config still says "phase 3"). The source of truth splits between file and state. Also, `devflow parallel` requires different agents per-phase but only one config file exists.

**Do this instead:** All workflow options are supplied as CLI flags to `devflow start`. The state machine reads/writes `.devflow/state-{N:02}.json` as the single source of truth per phase. This is followed in `crates/devflow-core/src/workflow.rs` and enforced by the CLI.

### Shared Project-Wide State Lock Held Across Gates

**What happens:** An early design held a single project-wide lock during the entire `advance()` call, including gate waits (which can last days). This would block any sibling phase under `devflow parallel`.

**Why it's wrong:** One phase waiting for human approval starves all other phases. A 3-day gate on phase 7 blocks phase 8's feature branch creation.

**Do this instead:** Per-phase locks (held across stage + gate). Project-wide lock only for the brief critical section (version bump, branch merge). Implemented in `crates/devflow-core/src/lock.rs` with two separate `acquire()` and `acquire_project()` functions. Lock is held per-phase; released via LockGuard drop (RAII).

### Synchronous Agent Capture + CLI Blocking

**What happens:** Early design had the CLI spawn the agent process directly and capture its output in a thread. If the CLI crashed or was killed, the capture thread died with it, and the agent continued running unsupervised.

**Why it's wrong:** No way to know if the agent succeeded (its exit code is lost). Future `devflow advance` calls have no output to parse.

**Do this instead:** Spawn a detached monitor process (shell script) that owns the agent, captures output to `.devflow/phase-{N:02}-{stdout,stderr,exit}`, records the PID to `.devflow/phase-{N:02}-agent-pid`, and calls `devflow advance` when the agent exits. The CLI returns immediately; the monitor persists. Implemented in `crates/devflow-core/src/monitor.rs`.

### Single Global State File for All Phases

**What happens:** Early design used `.devflow/state.json` for all phases. Under `devflow parallel`, phase 7's monitor and phase 8's monitor could both try to read/write it, causing race conditions or state clobbering.

**Why it's wrong:** `devflow parallel` runs phases concurrently in separate worktrees. A global state file is a single point of failure. Also, each phase's monitor should advance only its own state machine, not depend on reading a "current phase" from a singleton.

**Do this instead:** Per-phase state files (`.devflow/state-{N:02}.json`). Each phase's monitor reads/writes only its own state. Migration from legacy single-slot file is one-shot on first read (see `crates/devflow-core/src/workflow.rs`). This ensures phase 7's monitor never clobbers phase 8's state.

---

*Architecture analysis: 2026-07-17*
