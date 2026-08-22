---
phase: 18-dogfood-reliability-hardening
plan: 03
subsystem: cli
tags: [rust, cli, diagnostics, state-machine, doctor, monitor, liveness]

# Dependency graph
requires:
  - phase: 18-dogfood-reliability-hardening
    provides: "18-01's PhaseFacts/PhaseFinding/reconcile_phase pure core plus collect_phase_facts/render_reconciliation wiring, which this plan extends with a sixth check rather than adding a parallel reconciliation path"
provides:
  - "State.monitor_pid: Option<u32> persisted at spawn time, surviving a monitor/process restart"
  - "Liveness enum (Healthy/BetweenStages/Stuck/Unknown) plus a pure liveness() predicate consumed by both status and doctor"
  - "devflow status renders a monitor_pid row and a liveness line, naming devflow resume --phase N when stuck"
  - "devflow doctor's reconciliation reports a dead monitor as a Problem finding via check_dead_monitor, reusing liveness() rather than re-deriving the matrix"
affects: [18d-code-validate-gate-reachability, 18e-validate-verdict-fix, 18f-preflight-rerun-wedge, status, doctor, monitor]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "single pure predicate (liveness()) shared verbatim by status's rendering and doctor's check_dead_monitor, so the four-row matrix can never drift between the two call sites"
    - "None-first match arm ordering in a pure predicate as the concurrency/precision-edge guard, mirroring 18-01's PhaseFacts pattern: an absent/unrecorded field must be structurally unable to fall through into a problem state"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/log_format_env.rs

key-decisions:
  - "check_dead_monitor placed immediately after check_dead_agent in reconcile_phase's array (not at the end) per the plan's 'extend, not reorder' instruction, since both are liveness checks over the same PhaseFacts"
  - "Added a status-idempotency regression test (status_reading_monitor_liveness_writes_no_state_and_no_event) not explicitly named in any task's acceptance_criteria, to close a gap between the plan's must_haves.truths (idempotency edge) and its per-task verify blocks — see Deviations"

requirements-completed: [18b]

coverage:
  - id: D1
    description: "State.monitor_pid is persisted at spawn time, survives a process restart, and round-trips through serde as an exact u32; a serde-absent value defaults to None, never Some(0)"
    requirement: "18b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#tests::monitor_pid_round_trips_through_serde"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#tests::monitor_pid_absent_from_json_defaults_to_none"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::launch_stage_persists_monitor_pid_for_reload"
        status: pass
    human_judgment: false
  - id: D2
    description: "devflow status probes the monitor PID's liveness alongside the agent PID's and renders stuck — needs devflow resume as a state distinct from a healthy between-stages moment"
    requirement: "18b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::liveness_matrix_covers_all_four_rows"
        status: pass
      - kind: manual_procedural
        ref: "devflow status against a synthetic phase-42 state.json with a dead monitor pid — printed 'liveness: stuck — needs devflow resume' plus a 'devflow resume --phase 42' hint"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow doctor's reconciliation reports a dead monitor as a finding with a devflow resume --phase N repair, staying silent for an unrecorded monitor and for a normal between-stages moment"
    requirement: "18b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_reports_stuck_when_monitor_and_agent_are_both_dead"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_is_silent_when_monitor_pid_is_unrecorded"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::doctor_reconciliation::reconcile_is_silent_when_monitor_alive_and_agent_dead"
        status: pass
      - kind: manual_procedural
        ref: "devflow doctor against the same synthetic phase-42 fixture — printed 'phase 42: monitor pid 999999 recorded but not running at stage code' with 'repair: devflow resume --phase 42'"
        status: pass
    human_judgment: false
  - id: D4
    description: "A monitor_pid of 0, or above i32::MAX, reports not-running via agent::agent_running — never alive (boundary edge)"
    requirement: "18b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::liveness_treats_zero_and_overflow_pids_as_dead"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::status_reading_monitor_liveness_writes_no_state_and_no_event"
        status: pass
    human_judgment: false
  - id: D5
    description: "Running devflow status twice produces identical output and writes no state file and no event (idempotency edge)"
    requirement: "18b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::status_reading_monitor_liveness_writes_no_state_and_no_event"
        status: pass
    human_judgment: false
  - id: D6
    description: "Persisting monitor_pid for one phase leaves a concurrently-active sibling phase's monitor_pid unchanged (concurrency edge)"
    requirement: "18b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::monitor_pid_persisted_for_one_phase_does_not_disturb_a_sibling"
        status: pass
    human_judgment: false

duration: ~30min
completed: 2026-07-21
status: complete
---

# Phase 18 Plan 03: Monitor Liveness (Who Watches the Watcher) Summary

**`State.monitor_pid` is persisted at spawn and probed by a pure `liveness()` predicate shared by `status` and `doctor`, turning a silently dead monitor into `stuck — needs devflow resume`, distinct from a normal between-stages moment.**

## Performance

- **Duration:** ~30 min
- **Started:** 2026-07-21T03:43:10Z
- **Completed:** 2026-07-21T03:59:00Z
- **Tasks:** 3
- **Files modified:** 3 (`crates/devflow-core/src/state.rs`, `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/tests/log_format_env.rs`; +367/-3 net across all commits)

## Accomplishments

- `State.monitor_pid: Option<u32>` (`#[serde(default)]`), placed after `worktree_path`, initialized `None` in `State::new`; round-trips exactly through serde and defaults to `None` (never `Some(0)`) on serde-absent input — proven by two new `state.rs` tests plus an extended `new_state_starts_at_define` assertion
- `launch_stage` now records `state.monitor_pid = Some(pid)` and re-saves state immediately after `monitor::spawn_monitor` returns, because `transition()` saves state *before* calling `launch_stage` and would otherwise lose the pid — proven by `launch_stage_persists_monitor_pid_for_reload`, which spawns a stubbed agent binary end-to-end and reloads the persisted state file
- `Liveness` enum (`Healthy`/`BetweenStages`/`Stuck`/`Unknown`) plus a pure `liveness(monitor_pid, monitor_alive, agent_alive) -> Liveness` predicate — no I/O, no re-implemented process probe (reuses `agent::agent_running`), `None` matched first so an unrecorded monitor structurally cannot become `Stuck`
- `devflow status` renders `monitor_pid: {pid} (running: {bool})` (or `none`) and a `liveness: {describe}` line, printing `→ devflow resume --phase N` only when `Stuck`; manually verified end-to-end against a synthetic dead-monitor fixture
- `devflow doctor`'s reconciliation gained a sixth check, `check_dead_monitor`, which reuses `liveness()` (not a re-derived matrix) and is spliced into `reconcile_phase`'s fixed array immediately after `check_dead_agent`, extending 18-01's established finding order rather than reordering it; manually verified against the same fixture
- 9 new unit tests total: 2 serde tests in `state.rs`; in `main.rs`, 3 liveness-matrix/edge tests, a sibling-phase non-interference test, a `launch_stage` persistence test, a `status` read-only/idempotency test, and 3 `doctor_reconciliation` tests (stuck-both-dead, silent-when-unrecorded, silent-when-monitor-alive) plus an updated ordering-independence expectation
- No absolute filesystem paths or OS usernames anywhere in the new output — every new line carries phase number, stage name, and/or pid only (WR-02 leak class, T-18-09)

## Task Commits

1. **Task 1: Persist monitor_pid on State** - `9f33b75` (feat)
2. **Task 2: Record the monitor PID at spawn and add the pure liveness predicate** - `05556a2` (feat)
3. **Task 3: Report a dead monitor as a doctor reconciliation finding** - `dbbff40` (feat)
4. **Deviation: status read-only regression test** - `e60271d` (test)

**Plan metadata:** (this commit, once created below)

## Files Created/Modified

- `crates/devflow-core/src/state.rs` - `State.monitor_pid: Option<u32>` field, doc comment, `State::new` initialization, 2 new serde tests, extended `new_state_starts_at_define`
- `crates/devflow-cli/src/main.rs` - `launch_stage`'s post-spawn persist; `Liveness` enum + `liveness()`; `status()`'s new monitor row/liveness line; `PhaseFacts.monitor_pid`/`monitor_alive`; `check_dead_monitor`; `reconcile_phase`'s extended array; 10 new/updated tests
- `crates/devflow-cli/tests/log_format_env.rs` - added `monitor_pid: None` to the manual `State { .. }` struct literal (compile-breaking without it — this is the only other struct-literal `State` construction in the codebase besides `state.rs` itself)

## Decisions Made

- `check_dead_monitor` is positioned right after `check_dead_agent` in `reconcile_phase`'s array (not appended at the very end), per the plan's explicit "extend after the existing agent-liveness check" instruction — keeps the two liveness checks adjacent and leaves 18-01's other four checks' relative order untouched.
- `agreeing_facts`'s "fully-agreeing" baseline fixture in `doctor_reconciliation` tests was extended with `monitor_pid: Some(4343), monitor_alive: true`, mirroring the existing `agent_pid`/`agent_alive` baseline pattern, so the zero-findings baseline test continues to assert a genuinely healthy state.
- `reconcile_is_silent_when_monitor_alive_and_agent_dead` asserts "no monitor finding" (`!detail.contains("monitor pid")`) rather than "findings is empty," because `check_dead_agent` (18-01, unmodified, out of this plan's scope) independently flags a dead agent pid at an agent stage regardless of monitor state — that pre-existing finding is correct and orthogonal, not a regression introduced here.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `log_format_env.rs`'s manual `State { .. }` struct literal doesn't compile after adding `monitor_pid`**
- **Found during:** Task 1 verification (`cargo build --workspace`)
- **Issue:** `crates/devflow-cli/tests/log_format_env.rs` constructs a `State` via an explicit struct literal (not `State::new`) to write a legacy fixture file. Adding a new non-`Default`-annotated field to `State` makes this a compile error (missing field), independent of any test logic.
- **Fix:** Added `monitor_pid: None` to the literal, matching `State::new`'s own default.
- **Files modified:** `crates/devflow-cli/tests/log_format_env.rs`
- **Verification:** `cargo build --workspace` succeeds; the file's own test (`legacy state migration`) still passes as part of `cargo test --workspace`.
- **Committed in:** `9f33b75` (Task 1 commit)

**2. [Rule 2 - Missing Critical] No regression test locked in the plan's idempotency-edge must_have for `status`**
- **Found during:** Task 2 verification, cross-checking the plan's `must_haves.truths` against its per-task `acceptance_criteria`/`verify` blocks
- **Issue:** The plan's must_haves explicitly require "Running `devflow status` twice produces identical output and writes no state file and no event (idempotency edge)," but no task's acceptance criteria or verify block named a test for it — `status()` never called `workflow::save_state`/`events::emit` before or after this plan's changes (true by code inspection), but that guarantee had no automated regression coverage, unlike `doctor`'s parallel guarantee (`doctor_is_read_only_on_a_mismatched_project`, 18-01).
- **Fix:** Added `status_reading_monitor_liveness_writes_no_state_and_no_event`, mirroring 18-01's doctor read-only test pattern: saves a fixture state with `monitor_pid: Some(u32::MAX)` (also exercising the precision/boundary edge in the same test), runs `status()` twice, and asserts the state file's length/mtime and the event log's line count are unchanged.
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** `cargo test -p devflow status_reading_monitor_liveness_writes_no_state_and_no_event` → `1 passed`; full `cargo test --workspace` → `0 failed`.
- **Committed in:** `e60271d`

---

**Total deviations:** 2 auto-fixed (1 Rule 3 blocking compile fix, 1 Rule 2 missing-critical-coverage fix). No scope creep — both are corrections needed to make the plan's own stated requirements (compile at all; the must_haves' idempotency truth) actually hold.
**Impact on plan:** No functional behavior changed beyond what the plan specified; both deviations are either a required compile fix or a test-coverage gap-closer for a must_have the plan itself declared.

## Issues Encountered

None beyond the two auto-fixed deviations above. The carry-forward correction from 18-01/18-02 (`cargo test -p devflow --lib` hard-errors on this binary-only crate; use `cargo test -p devflow <filter>` without `--lib`) was confirmed to still apply and was used throughout this plan's verification.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 18b (monitor liveness) is complete. `PhaseFacts`, `reconcile_phase`, and `liveness()` are now the shape for any future check that needs to reason about monitor/agent aliveness together — no parallel reconciliation path was introduced.
- `cargo test --workspace` is green at 405 passed / 0 failed (up from 18-02's 394-test baseline: +11 tests — 2 in `devflow-core`, 9 in `devflow-cli`'s `devflow` unittests binary, confirmed via the per-binary `test result:` counts at each task's verification step). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both exit 0.
- Manually verified end-to-end against a synthetic `/tmp` fixture project (not this repo, since this repo has no genuinely stuck phase): a `state-42.json` with `monitor_pid: 999999` (dead) produced `liveness: stuck — needs devflow resume` in `devflow status` and a `phase 42: monitor pid 999999 recorded but not running at stage code` finding with a `devflow resume --phase 42` repair in `devflow doctor`. No absolute paths or usernames appeared in either output.
- No blockers for 18-04/18d (Code↔Validate safety-gate reachability), 18-05/18e (Layer 0/Validate verdict fix), or 18-06/18f (preflight-gate re-run wedge fix) — none depend on this plan's `main.rs` region beyond the shared `PhaseFacts`/`reconcile_phase` core, which was extended additively.

---
*Phase: 18-dogfood-reliability-hardening*
*Completed: 2026-07-21*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/state.rs
- FOUND: crates/devflow-cli/src/main.rs
- FOUND: crates/devflow-cli/tests/log_format_env.rs
- FOUND: .planning/phases/18-dogfood-reliability-hardening/18-03-SUMMARY.md
- FOUND: 9f33b75 (Task 1 commit)
- FOUND: 05556a2 (Task 2 commit)
- FOUND: dbbff40 (Task 3 commit)
- FOUND: e60271d (deviation commit)
