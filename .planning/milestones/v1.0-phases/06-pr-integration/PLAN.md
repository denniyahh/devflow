# Phase 6: Agent Completion + Ship Readiness

> Parent: ROADMAP.md | Status: Planned (2026-06-18)
> Replaces: old Phase 6 (PR integration → moved to Phase 7)

## Goal

Make devflow reliably detect whether an agent succeeded or failed, and add safety checks to prevent the branch divergence problems identified in the audit.

Four tasks, all small, all directly addressing real problems we've hit.

## Context

- **Audit:** [AUDIT.md](../../codebase/AUDIT.md) — 18 items identified, 3 critical
- **Agent completion design:** [DESIGN-agent-completion.md](./DESIGN-agent-completion.md)
- **Branch:** `feature/phase-06-ship-readiness` (to be created off develop)

---

## Tasks

### 6A — DEVFLOW_RESULT Agent Completion Protocol 🔴 CRITICAL

The agent self-reports success/failure via a structured marker in its output. Three-layer fallback ensures detection even when agents don't cooperate.

- [ ] **6A-1: Prompt injection** — Add DEVFLOW_RESULT instruction to all 3 agent prompts
  - Claude: natural language protocol in `agents/claude.rs`
  - Codex: shorter directive in `agents/codex.rs`
  - OpenCode: shorter directive in `agents/opencode.rs`
  - Verify: each agent's prompt ends with the DEVFLOW_RESULT instruction

- [ ] **6A-2: Result types** — New `agent_result.rs` module
  - `AgentResult` struct: status, exit_code, reason, commits, summary
  - `AgentStatus` enum: Success, Failed, Unknown
  - Add `agent_result: Option<AgentResult>` to `State`
  - Add `agent_stdout_path: Option<PathBuf>` to `State`
  - Verify: unit tests for marker parsing (success, failure, missing, malformed)

- [ ] **6A-3: Stdout capture for monitor mode** — Fix the Child-drop problem
  - In `start()`, spawn a stdout-capture thread before dropping Child
  - Thread: reads stdout pipe, waits on child, writes stdout + exit code to `.devflow/`
  - Files: `.devflow/phase-NN-stdout`, `.devflow/phase-NN-exit`
  - Verify: monitor mode produces these files; blocking mode already works

- [ ] **6A-4: Three-layer decision in `check()`**
  - Layer 1: Parse DEVFLOW_RESULT from stdout → authoritative
  - Layer 2: Exit code + commit count → reliable fallback
  - Layer 3: Process gone + commits exist → last resort warning
  - Verify: each layer tested independently (see DESIGN doc for test cases)

- [ ] **6A-5: Blocking mode also saves result**
  - `wait_for_agent()` already captures stdout + exit code
  - After wait: save stdout to file, parse DEVFLOW_RESULT, store in state
  - Verify: `devflow start --agent claude` (blocking) → stdout file exists

### 6B — Pre-Start Divergence Check 🟡 IMPORTANT

Before creating a feature branch, warn if develop has advanced significantly. Prevents the "started from stale base → 14-file merge conflict" problem.

- [ ] **6B-1: `GitFlow::divergence_from_develop()`** — new method
  - Returns `(ahead, behind)` commit counts for the current branch vs develop
  - Returns `(0, behind)` when on develop itself
  - File: `crates/devflow-core/src/git.rs`

- [ ] **6B-2: Warning on `devflow start`**
  - After branching, check divergence
  - If behind > 10: warn "develop is {n} commits ahead — consider rebasing first"
  - If behind > 50: error (require `--force` to proceed)
  - Config: `automation.max_divergence` (default: 50)
  - Verify: test with a develop that's ahead of the feature branch

### 6C — Agent Trait Full Integration 🟡 IMPORTANT

Complete the Phase 5 refactor: make `agent.rs` use the trait instead of reaching into `state.rs`.

- [ ] **6C-1: `launch_agent()` accepts trait**
  - Change signature from `launch_agent(state: &State)` to `launch_agent(agent: &dyn agents::Agent, phase: u32, project_root: &Path)`
  - Callers pass `agents::adapter_for(state.agent)` instead of `&state`
  - No behavior change — just pure refactor
  - Verify: all 85 tests pass identically

- [ ] **6C-2: Remove `Agent::exec_command()` from state.rs**
  - `exec_command()` and `name()` already delegate to the trait (done in Phase 5)
  - Clean up: remove the remaining delegation methods, make callers use trait directly
  - Verify: `clippy` clean, no dead code warnings

### 6D — Continuous Integration Hardening

- [ ] Update `.github/workflows/ci.yml` to include new tests
- [ ] Ensure `cargo test --workspace` passes on all platforms
- [ ] Add `cargo fmt --check` to CI (may already exist)

---

## Verification

```bash
# Agent completion
cargo test agent_result              # Marker parsing tests
echo 'DEVFLOW_RESULT: {"status":"success"}' > .devflow/phase-01-stdout && devflow check  # Should advance
echo 'DEVFLOW_RESULT: {"status":"failed"}' > .devflow/phase-01-stdout && devflow check     # Should halt

# Divergence check
devflow start --phase 6 --agent claude   # Should warn if develop is ahead

# Trait integration
cargo test --workspace                   # All 85+ tests pass identically
cargo clippy -- -D warnings              # Clean
```

## Success

1. DevFlow detects agent failure — no more blind advancement
2. Agents self-report via DEVFLOW_RESULT when cooperative
3. Exit code + commit gate catches uncooperative agents
4. Pre-start divergence check prevents stale-branch starts
5. Agent trait fully integrated — `agent.rs` uses `&dyn Agent`
6. All tests pass, clippy clean

## Deferred to Phase 7

- PR creation on `devflow ship`
- Remote push
- Merge detection
- Release workflow + binary upload
- CHANGELOG.md, README polish
- `cargo install` path

## Review Amendments (2026-06-18)

Cross-AI review found 9 concerns. These amendments address the 2 HIGH + key MEDIUM items.

### HIGH #1 fixed: Hardcode divergence thresholds

`max_divergence` as a config field requires ~17 lines of custom parser plumbing. **Decision: hardcode it.** Warn at 10 commits behind, error at 50. No config changes. Config knob deferred to when the custom parser is replaced with serde_yaml.

### HIGH #2 fixed: Shared `capture_agent_output()` function

Both monitor-thread and blocking paths need identical stdout capture + exit code + file write logic. **Extract into a single function** in `agent.rs`:

```rust
pub fn capture_agent_output(
    mut child: Child,
    phase: u32,
    project_root: &Path,
) -> io::Result<AgentCapture> {
    // 1. Read stdout pipe
    // 2. child.wait() for exit code
    // 3. Write stdout to .devflow/phase-NN-stdout
    // 4. Write exit code to .devflow/phase-NN-exit
    // 5. Return AgentCapture { stdout, exit_code }
}
```

Blocking path calls it directly. Monitor path wraps it in `thread::spawn`. The Child is **moved** into this function (ownership transfer documented).

### MEDIUM #3 fixed: Cleanup old stdout files on start

In `start()`, delete `.devflow/phase-NN-stdout` and `.devflow/phase-NN-exit` before launching the agent. Prevents reading stale files from a prior run of the same phase.

### MEDIUM #5 fixed: Branch existence check in Layer 2

Before `git rev-list --count feature/phase-NN`, verify the branch exists with `git rev-parse --verify`. If branch doesn't exist, treat as failure regardless of exit code.

### MEDIUM #6 fixed: Exit code source clarified

Layer 2 reads exit code from `.devflow/phase-NN-exit` **file** (written by capture thread), not from `state.json`. The `State.agent_result` field is populated during `check()` for display only, not as the primary decision input.
