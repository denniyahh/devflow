---
phase: 20-release-correctness-operator-control
plan: 03
subsystem: pipeline
tags: [rust, clap, serde, state-machine, cli]

# Dependency graph
requires:
  - phase: 20-02
    provides: shared main.rs/commands.rs Command/Start clap-enum regions (sequential wave 2 ordering, not a functional dependency)
provides:
  - "devflow start --until <stage> flag halting the pipeline cleanly short of Ship"
  - "State.stop_until/stopped/stop_reason fields (#[serde(default)], backward-compatible)"
  - "pipeline_gate::transition top-of-function stop interception (stop_until == Some(from))"
  - "doctor reconciliation stop-awareness in BOTH check_dead_agent and check_dead_monitor"
  - "devflow resume clears the stop marker so a resumed phase advances past its old stop point"
affects: [20-04, 20-05, future doctor/reconciliation work, future pipeline_gate/pipeline_launch changes]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "New State fields follow the existing #[serde(default)] backward-compat idiom (mirrors monitor_pid/infra_failures/preflight_retries)"
    - "Stop interception placed at the TOP of transition(), keyed on the JUST-COMPLETED stage (stop_until == Some(from)), not the target (to) — avoids an off-by-one that would halt before the target stage ever ran"
    - "Doctor findings gain a facts.stopped early-return guard on both affected checks, not a new parallel check"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/log_format_env.rs
    - OPERATIONS.md

key-decisions:
  - "Stop check reads state.stop_until == Some(from), not Some(to) — the off-by-one fix from cross-AI review (Codex HIGH), verified against Stage::next() and launch_stage's actual role in running a stage"
  - "Both check_dead_agent and check_dead_monitor gain a facts.stopped guard — the doctor gap is bigger than check_dead_agent alone (Codex HIGH)"
  - "resume() clears stopped/stop_reason/stop_until before relaunching, otherwise the phase would immediately re-stop and stay marked stopped forever (Codex MEDIUM)"
  - "--until ship rejected as a semantic no-op (D-07) since Ship never calls transition"

requirements-completed: [20c]

coverage:
  - id: D1
    description: "devflow start --until plan runs Define AND Plan to completion, then halts before advancing to Code, with a persisted stop marker and cleared monitor_pid"
    requirement: "20c"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_until_plan_halts_cleanly"
        status: pass
    human_judgment: false
  - id: D2
    description: "A --until-stopped phase produces zero Severity::Problem findings from devflow doctor (both check_dead_agent and check_dead_monitor recognize the stop marker)"
    requirement: "20c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#reconcile_phase_ignores_dead_agent_when_stopped"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#reconcile_phase_ignores_dead_monitor_when_stopped"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#reconcile_phase_flags_dead_agent_at_agent_stage"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow resume clears stopped/stop_reason/stop_until before relaunching, so a resumed phase advances past its old stop point"
    requirement: "20c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#resume_clears_stop_marker_and_advances_past_stop_point"
        status: pass
    human_judgment: false
  - id: D4
    description: "--until ship is rejected before any stage runs (semantic no-op); --until <bogus> is rejected by the existing Stage parser"
    requirement: "20c"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_until_ship_is_rejected"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_until_unknown_stage_is_rejected_by_clap"
        status: pass
    human_judgment: false
  - id: D5
    description: "New State fields (stop_until/stopped/stop_reason) round-trip through serde and default when absent from older persisted JSON"
    requirement: "20c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#stop_fields_round_trip_through_serde"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#stop_fields_absent_from_json_default"
        status: pass
    human_judgment: false

duration: ~35min
completed: 2026-07-23
status: complete
---

# Phase 20 Plan 03: Plan-Only Pipeline Mode (`--until <stage>`) Summary

**`devflow start --until <stage>` halts cleanly after a named stage — new `State.stop_until`/`stopped`/`stop_reason` fields intercepted at the top of `pipeline_gate::transition`, doctor-aware in both dead-agent and dead-monitor checks, and cleared on `devflow resume`.**

## Performance

- **Duration:** ~35 min
- **Completed:** 2026-07-23
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments

- `devflow start --until <stage>` runs the pipeline through the requested stage and halts cleanly before advancing further — no orphaned monitor, no stranded worktree, unblocking cheap/frequent dogfood runs (the project's stated highest-yield bug source).
- The stop interception is the single off-by-one-corrected check `state.stop_until == Some(from)` at the TOP of `pipeline_gate::transition`, verified against `Stage::next()`/`launch_stage` semantics — `--until plan` provably runs Plan to completion (not stops before it) before halting.
- `devflow doctor` recognizes a `--until`-stopped phase as intentional in BOTH `check_dead_agent` and `check_dead_monitor` (the review-identified gap that `check_dead_agent` alone would have left open) — a stopped phase never reports as a crashed agent or dead monitor.
- `devflow resume` clears the stop marker before relaunching, so a resumed phase advances past its old stop point instead of immediately re-stopping or staying marked stopped forever.
- `--until ship` is rejected as a semantic no-op (Ship never calls `transition`); `--until <bogus>` is rejected by the pre-existing `Stage: FromStr` parser with no new parsing surface.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end `--until` slice — flag → state field → transition stop** - `29132b2` (feat, tracer, tdd)
2. **Task 2: Close the doctor gap (both checks) + define resume stop-clearing** - `8f83d79` (fix, tdd)
3. **Task 3: Reject `--until ship`, serde round-trip, regenerate help + OPERATIONS** - `4f9af46` (feat, tdd)

_All three tasks are `tdd="true"`; each new behavior was proven RED (temporarily reverted the fix, confirmed the test failed for the intended reason) then GREEN (restored the fix, confirmed the test passed) before committing — logged inline above._

## Files Created/Modified

- `crates/devflow-core/src/state.rs` — new `stop_until: Option<Stage>` / `stopped: bool` / `stop_reason: Option<String>` fields (`#[serde(default)]`), initialized in `State::new`; round-trip + absent-default serde tests
- `crates/devflow-cli/src/main.rs` — new `--until` flag on `Command::Start`; `--until ship` rejection guard at the dispatch site
- `crates/devflow-cli/src/commands.rs` — `start()` threads `until` into `state.stop_until`; `PhaseFacts.stopped` field + population in `build_phase_facts`; `facts.stopped` guard added to both `check_dead_agent` and `check_dead_monitor`; two new reconcile tests
- `crates/devflow-cli/src/pipeline_gate.rs` — `transition()`'s new stop-interception branch at the top, keyed on `stop_until == Some(from)`
- `crates/devflow-cli/src/pipeline_launch.rs` — `resume()` clears `stopped`/`stop_reason`/`stop_until` before relaunching; new regression test
- `crates/devflow-cli/src/parallel.rs` — updated `start()` call site for the new parameter (`None`)
- `crates/devflow-cli/tests/phase7_cli.rs` — `start_until_plan_halts_cleanly`, `start_until_ship_is_rejected`, `start_until_unknown_stage_is_rejected_by_clap`, plus a `wait_for_stopped` fixture helper
- `crates/devflow-cli/tests/log_format_env.rs` — updated a pre-existing `State {}` struct literal for the new fields (out-of-scope compile fix, Rule 3)
- `OPERATIONS.md` — `--until` documented on the `start` row; resume's stop-marker clearing documented on the `resume` row

## Decisions Made

- Stop check reads `state.stop_until == Some(from)` (the JUST-COMPLETED stage), not `Some(to)` — verified against `pipeline_gate.rs:51-80` and `stage.rs:31` per the plan's incorporated review finding; a `to`-based check would have halted before the target stage ever ran.
- Both `check_dead_agent` and `check_dead_monitor` gained the `facts.stopped` guard (not just the former) — the doctor gap is bigger than the plan's title check alone; without the second guard a stopped phase with a stale `monitor_pid` would still misreport as `Stuck`.
- `resume()` clears the three stop fields and persists that clear BEFORE calling `launch_stage`, so a reload mid-relaunch already reflects "no longer stopped."
- `--until ship` is rejected outright (D-07) rather than silently accepted as a no-op, since `handle_ship_outcome` calls `finish_workflow` directly and never reaches `transition`.

## Deviations from Plan

None — plan executed exactly as written. The plan's own `<review_incorporation>` section had already resolved the off-by-one, doctor-gap, and resume-semantics questions before this execution began; no new architectural or scope decisions were needed during implementation.

One incidental out-of-scope compile fix (Rule 3 — blocking issue): `crates/devflow-cli/tests/log_format_env.rs` constructs a `State { .. }` struct literal directly (not via `State::new`), which failed to compile once the three new fields were added to `State`. Updated the literal to include `stop_until: None, stopped: false, stop_reason: None` — a mechanical fix required for the crate to compile, not a functional change to that test.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- 20-04 and 20-05 depend on this plan sequentially (shared `main.rs`/`commands.rs` `Command`/`Start` clap-enum regions and the CLI help snapshot) per the phase's documented wave ordering — no functional blocker, purely a merge-conflict-avoidance sequencing choice recorded in `20-03-PLAN.md`'s `<review_incorporation>`.
- `cargo test --workspace` passes with 0 failed across every target; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are both clean.
- The help snapshot (`crates/devflow-cli/tests/snapshots/devflow-help.txt`) was regenerated and produced no diff, since `--until` is a `Start`-subcommand-level flag invisible in the top-level `--help` subcommand list.

## Self-Check: PASSED

All files (state.rs, pipeline_gate.rs, pipeline_launch.rs, commands.rs, main.rs, OPERATIONS.md) and all commit hashes (29132b2, 8f83d79, 4f9af46) verified present in the working tree and git log.

---
*Phase: 20-release-correctness-operator-control*
*Completed: 2026-07-23*
