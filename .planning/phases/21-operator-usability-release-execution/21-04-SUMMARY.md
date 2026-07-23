---
phase: 21-operator-usability-release-execution
plan: 04
subsystem: cli
tags: [rust, sequentagent, observability, status, cli, agent-tracking]

# Dependency graph
requires:
  - phase: 21-operator-usability-release-execution (plan 03)
    provides: commands.rs status()/doctor edits this plan lands after (same-wave zero-file-overlap ordering)
provides:
  - "sequentagent slot record: SequentagentSlotKind/SequentagentSlot + write_/read_/clear_sequentagent_slot in agent_result.rs"
  - "SequentagentSlotGuard RAII cleanup guard in parallel.rs, clearing the slot record on every sequentagent exit path"
  - "render_sequentagent_status() in commands.rs, surfaced in `devflow status`"
affects: [devflow-cli-parallel, devflow-cli-status, devflow-core-agent-result]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Typed enum slot param (SequentagentSlotKind) instead of stringly-typed &str for call-site typo-proofing"
    - "RAII Drop guard as the single cleanup point for a multi-exit-path function, replacing a fragile success-only clear"
    - ".devflow/phase-NN-<kind> sibling record convention (mirrors agent_pid_path), created via workflow::ensure_devflow_dir, never routed through State/save_state"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "Slot record is a path-free two-line text file (slot letter + agent kind), not JSON — avoids an infallible-but-still-fallible serde_json::to_string(...).expect() call for a two-field struct of plain strings."
  - "Slot-write failures inside run_agent_blocking are best-effort (println! warning, not propagated as CliError) — observability must never gate or fail sequentagent's actual execution (D-06 narrow scope, threat T-21c-03 disposition)."
  - "doctor integration intentionally out of scope (unchanged from plan) — status is the live-observability command; doctor reconciles persisted State, and a text-only doctor line without a matching --json key would reintroduce the WR-01 human/json split."

requirements-completed: [21c, D-06]

coverage:
  - id: D1
    description: "sequentagent slot record data model + write/clear wiring: write_sequentagent_slot/read_sequentagent_slot/clear_sequentagent_slot round-trip, path-free, creates .devflow/ via ensure_devflow_dir, typed SequentagentSlotKind param on run_agent_blocking, SequentagentSlotGuard clears on every exit path"
    requirement: "21c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#sequentagent_slot_round_trips, sequentagent_slot_is_path_free, sequentagent_slot_write_creates_devflow_dir_and_gitignore, sequentagent_slot_missing_record_reads_as_none, sequentagent_slot_kind_as_str"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/parallel.rs#slot_guard_clears_record_on_early_return, slot_guard_clears_record_on_success_path"
        status: pass
    human_judgment: false
  - id: D2
    description: "devflow status surfaces the live sequentagent slot: render_sequentagent_status distinguishes running/starting/not-running and returns None with no records"
    requirement: "D-06"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#sequentagent_status_renders_running_slot, sequentagent_status_renders_dead_pid_as_not_running, sequentagent_status_renders_starting_when_pid_file_missing, sequentagent_status_none_when_no_records"
        status: pass
    human_judgment: false

# Metrics
duration: ~45min
completed: 2026-07-23
status: complete
---

# Phase 21 Plan 04: sequentagent Second-Agent Tracking Summary

**Typed `SequentagentSlotKind` slot record (`.devflow/phase-NN-sequentagent`) with a `Drop`-guard cleanup, surfaced as a `running`/`starting`/`not running` line in `devflow status`, closing 21c/D-06's second-agent visibility gap.**

## Performance

- **Duration:** ~45 min
- **Completed:** 2026-07-23T21:22:57Z
- **Tasks:** 2/2
- **Files modified:** 3

## Accomplishments
- `sequentagent`'s previously-invisible second agent (and, incidentally, the first) now writes a lightweight, path-free per-phase record naming its slot (A/B) and `AgentKind`, following the existing `xxx_path(project_root, phase)` naming convention (`agent_result.rs`) — never routed through `State`/`save_state` (Phase 19 D-14 preserved).
- `devflow status` now renders a distinct `sequentagent phase N: agent B (codex) running (pid P)`-shaped line, cross-referencing pid liveness from the existing agent-pid file, with an honest `starting` transient for the monitor's async pid-write race and a `not running` fallback for a stale record with a dead pid — never a false-live agent.
- A `SequentagentSlotGuard` RAII cleanup guard, bound once before agent A runs, clears the slot record on ALL five `sequentagent` exit paths (agent-A failed, agent-A rate-limited/zero-commit, rebase-B conflict, agent-B failed/rate-limited, integrate-B failure) plus success — proven live by a test that reaches `drop` via an early `return`, not just fall-through.

## Task Commits

Each task was committed atomically:

1. **Task 1: sequentagent slot record — data model + write/clear wiring** - `66c1983` (feat)
2. **Task 2: surface the live sequentagent slot in `status`** - `6cb4a6b` (feat)

**Plan metadata:** (this SUMMARY commit, made by the worktree orchestrator per parallel-execution protocol)

_Note: Task 1 was typed `tracer` in the plan but executed as a single end-to-end unit (data model + wiring + tests) in one commit, matching the plan's tracer/tdd framing without a separate RED/GREEN split — the plan's `<behavior>` block described the full round-trip/path-free/dir-creation/guard-drop contract as one coherent slice, and TDD-style unit tests were written and verified passing before the commit._

## Files Created/Modified
- `crates/devflow-core/src/agent_result.rs` - `SequentagentSlotKind` enum, `SequentagentSlot` struct, `sequentagent_slot_path`/`write_sequentagent_slot`/`read_sequentagent_slot`/`clear_sequentagent_slot`, plus 5 unit tests
- `crates/devflow-cli/src/parallel.rs` - `run_agent_blocking` gains a typed `slot` param and writes the record after the monitor spawns; `SequentagentSlotGuard` (Drop-based cleanup) bound before agent A runs; both `sequentagent` call sites pass `SequentagentSlotKind::A`/`::B`; 2 unit tests proving the guard clears on both early-return and success paths
- `crates/devflow-cli/src/commands.rs` - `render_sequentagent_status()` (pure, read-only), wired into `status()`; 4 unit tests covering running/dead-pid/starting/no-records

## Decisions Made
- **Path-free two-line text format over JSON** for the slot record — a two-field struct of plain strings has no genuine serialization failure mode, but a JSON approach would have required either an `.expect()` (disfavored per this codebase's "no `.unwrap()`/`.expect()` outside tests" convention) or unnecessary error-plumbing; the plain `slot\nagent\n` text format sidesteps both while staying trivially defensive to parse.
- **Slot-write failures are best-effort, not fatal** — `run_agent_blocking` prints a warning and continues rather than propagating a `CliError` if `write_sequentagent_slot` fails, so an observability write can never block or fail the actual agent run (matches the plan's "MUST NOT change sequentagent's execution/integration/rebase behavior" prohibition and threat T-21c-03's tampering-avoidance disposition).
- **`doctor` integration stays out of scope**, exactly as the plan specified — the function's doc comment records the reasoning (WR-01 human/json split) so a future contributor doesn't reintroduce it without also adding a `--json` key.

## Deviations from Plan

None — plan executed exactly as written. Both tasks' `<action>`, `<behavior>`, and `<acceptance_criteria>` blocks were implemented verbatim; no Rule 1-4 auto-fixes were needed.

## Issues Encountered

None. `cargo fmt` auto-reformatted two multi-field struct-literal expressions and one assertion macro call after the initial edits (standard rustfmt normalization, not a logic change) — re-verified with `cargo fmt --check` (clean) and the full test suite (still green) before committing.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Unit 21c (D-06) is closed: sequentagent's second agent is now observable via `devflow status` without touching the stage machine or `State`/`save_state`.
- Verification commands all pass locally: `cargo test --workspace agent_result::tests::sequentagent_slot` (5/5), `cargo test --workspace parallel::` (9/9, includes 2 new guard tests), `cargo test --workspace commands::tests::sequentagent_status` (4/4), full `cargo test --workspace` (334 devflow-core + 156 devflow-cli + integration binaries, 0 failed), `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.
- No known stubs, no skipped tests, no unrun `<verify>` blocks — all four automated verify commands from the plan's `<verify>`/`<verification>` blocks were run directly against the live worktree, not self-reported.
- Threat register: all four `mitigate`-dispositioned threats (T-21c-01 info disclosure, T-21c-02 stale-pid spoofing, T-21c-03 state-corruption tampering, T-21c-04 parser DoS) have a corresponding implemented mitigation and/or test (path-free assertion test, `agent_running(pid)`-derived liveness, no new `save_state` call, defensive `None`-on-any-error parsing).
- No new threat surface introduced beyond what the plan's `<threat_model>` already declared — no new network endpoints, auth paths, or schema changes.

---
*Phase: 21-operator-usability-release-execution*
*Completed: 2026-07-23*
