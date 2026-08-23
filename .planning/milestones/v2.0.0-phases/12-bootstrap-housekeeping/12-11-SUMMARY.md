---
phase: 12-bootstrap-housekeeping
plan: 11
subsystem: infra
tags: [rust, refactor, naming, state-machine]

# Dependency graph
requires:
  - phase: 12-bootstrap-housekeeping (12-01 through 12-09)
    provides: all other Phase-12 code-review-debt fixes and test additions this rename sweeps
provides:
  - "AgentKind enum (renamed from Agent) with the AgentKind = Agent type alias removed"
  - "AgentAdapter trait (renamed from the adapter trait Agent) implemented by Claude/Codex/OpenCode adapters"
  - "State struct with the dead agent_result/agent_stdout_path fields removed"
affects: [any future phase touching devflow_core::state, devflow_core::agents, or the CLI --agent flag]

# Tech tracking
tech-stack:
  added: []
  patterns: []

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-core/src/agents/claude.rs
    - crates/devflow-core/src/agents/codex.rs
    - crates/devflow-core/src/agents/opencode.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/recover.rs
    - crates/devflow-core/src/workflow.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/agent.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/tests/monitor_e2e.rs
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "Deleted the agent_result/agent_stdout_path State fields outright (both #[serde(skip)] and never populated) rather than adding a tracked TODO, per project convention to delete dead code"
  - "Renamed the enum directly to AgentKind and deleted the type alias, rather than keeping AgentKind as an alias and renaming the trait to something else — matches the plan's explicit IN-03 resolution"

patterns-established: []

requirements-completed: [IN-02, IN-03]

coverage:
  - id: D1
    description: "State no longer carries the never-populated agent_result/agent_stdout_path fields or the unused AgentResult import"
    requirement: "IN-02"
    verification:
      - kind: unit
        ref: "cargo build -p devflow-core (compiles clean) + state::tests::state_serde_round_trips"
        status: pass
    human_judgment: false
  - id: D2
    description: "Enum Agent renamed to AgentKind directly (alias deleted); adapter trait Agent renamed to AgentAdapter; workspace builds/tests/clippy/fmt clean; CLI --agent flag still parses via FromStr"
    requirement: "IN-03"
    verification:
      - kind: unit
        ref: "cargo test --workspace (173 tests across devflow-core lib/e2e + devflow-cli lib/CLI, all pass)"
        status: pass
      - kind: integration
        ref: "cargo run -p devflow -- start --phase 1 --agent claude --mode auto --dry-run"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-07-08
status: complete
---

# Phase 12 Plan 11: Dead-Field Removal + Agent/AgentKind/AgentAdapter Rename Summary

**Removed two never-populated `State` fields and eliminated the `AgentKind = Agent` alias by renaming the enum to `AgentKind` and the adapter trait to `AgentAdapter` across the whole workspace.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-07-08T23:55:00Z (approx, first Read)
- **Completed:** 2026-07-09T00:08:00Z
- **Tasks:** 2
- **Files modified:** 13

## Accomplishments
- Removed `State.agent_result` / `State.agent_stdout_path` (both `#[serde(skip)]`, never populated anywhere) and the now-unused `AgentResult` import from `state.rs`
- Renamed the `Agent` enum to `AgentKind` directly in `state.rs`, deleting the `pub type AgentKind = Agent` alias that previously existed only to dodge the naming collision with the adapter trait
- Renamed the adapter trait `Agent` (in `agents/mod.rs`) to `AgentAdapter`; `adapter_for` now returns `Box<dyn AgentAdapter>`
- Swept every remaining reference to the old enum/trait names across `agents/{claude,codex,opencode}.rs`, `lib.rs`, `agent.rs`, `agent_result.rs`, `recover.rs`, `workflow.rs`, `monitor.rs`, `monitor_e2e.rs`, and `devflow-cli/src/main.rs` — including the tests 12-09 and 12-10 added to `main.rs` in earlier waves of this same phase
- Full workspace build, test suite (173 tests total), clippy (`-D warnings`), and `cargo fmt --check` all pass after the rename; CLI `--agent claude` parsing verified via a live `--dry-run` invocation

## Task Commits

Each task was committed atomically:

1. **Task 1: IN-02 — remove the never-populated State fields** - `d19c69f` (fix)
2. **Task 2: IN-03 — rename enum Agent → AgentKind and trait Agent → AgentAdapter** - `f7cb521` (refactor)

**Plan metadata:** (this commit) — `docs: complete 12-11 plan`

## Files Created/Modified
- `crates/devflow-core/src/state.rs` - Removed dead fields/import; enum renamed `Agent` → `AgentKind`, alias deleted, `Display`/`FromStr`/tests updated
- `crates/devflow-core/src/agents/mod.rs` - Trait `Agent` renamed to `AgentAdapter`; `adapter_for` return type updated
- `crates/devflow-core/src/agents/claude.rs` - `impl Agent for ClaudeAgent` → `impl AgentAdapter for ClaudeAgent`
- `crates/devflow-core/src/agents/codex.rs` - `impl Agent for CodexAgent` → `impl AgentAdapter for CodexAgent`
- `crates/devflow-core/src/agents/opencode.rs` - `impl Agent for OpenCodeAgent` → `impl AgentAdapter for OpenCodeAgent`
- `crates/devflow-core/src/lib.rs` - Re-export updated to `pub use state::{AgentKind, State};`
- `crates/devflow-core/src/agent.rs` - `launch_agent`/`agent_label` signatures and tests updated to `AgentKind`/`AgentAdapter`
- `crates/devflow-core/src/agent_result.rs` - Test-only `Agent` import/usages updated to `AgentKind`
- `crates/devflow-core/src/recover.rs` - Test-only `Agent` import/usages updated to `AgentKind`
- `crates/devflow-core/src/workflow.rs` - Test-only `Agent` import/usages updated to `AgentKind`
- `crates/devflow-core/src/monitor.rs` - Test-only `Agent` import/usages updated to `AgentKind`
- `crates/devflow-core/tests/monitor_e2e.rs` - Test-only `Agent` import/usages updated to `AgentKind`
- `crates/devflow-cli/src/main.rs` - CLI `--agent` arg type, `start`/`parse_phase_agent_pairs`/`split_two_agents`/`run_agent_blocking` signatures, and all inline tests updated to `AgentKind` (the pre-existing `CliError::Agent(#[from] devflow_core::agent::AgentError)` variant name is unrelated to this enum and was left untouched)

## Decisions Made
- Deleted the two dead `State` fields outright instead of adding a tracked TODO — both were `#[serde(skip)]` so removal is schema-invisible, and the project's dead-code convention favors deletion over placeholder comments.
- Renamed the enum to `AgentKind` directly (not kept as an alias) per the plan's explicit IN-03 resolution — the alias was the root cause of the naming confusion this task existed to fix.

## Deviations from Plan

None - plan executed exactly as written. The one pre-existing `Agent` identifier the plan's `read_first` notes did not explicitly call out — `CliError::Agent(#[from] devflow_core::agent::AgentError)` in `main.rs:167` — is a `thiserror` variant name unrelated to the renamed enum/trait; it was correctly identified and left untouched (verified via `\bAgent\b` grep sweep before and after the rename).

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `IN-02` and `IN-03` (Phase 11 code-review debt) are both closed; Phase 12's 12e section is now fully resolved.
- Workspace is green (build/test/clippy/fmt) with no remaining references to the old `Agent` enum or trait names.
- This was the last plan of Phase 12's wave 3 (sequenced last because the rename touches nearly every source file) — no further plans depend on this one within Phase 12.

---
*Phase: 12-bootstrap-housekeeping*
*Completed: 2026-07-08*

## Self-Check: PASSED

All 13 modified files confirmed present on disk; both task commit hashes (`d19c69f`, `f7cb521`) confirmed in git log.
