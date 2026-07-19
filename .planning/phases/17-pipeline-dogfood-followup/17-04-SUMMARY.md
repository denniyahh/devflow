---
phase: 17-pipeline-dogfood-followup
plan: 04
subsystem: infra
tags: [rust, outcome-policy, state-machine, cron, events]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "17-01's outcome_policy::decide_action, AgentStatus::as_wire_str(), decided_by_layer, State.infra_failures/MAX_INFRA_FAILURES"
  - phase: 17-pipeline-dogfood-followup
    provides: "17-03's fail-closed evaluate_layer3/evaluate_layer0 split feeding typed outcomes into advance()"
provides:
  - "advance() dispatches exhaustively on outcome_policy::decide_action — Unknown, Failed, RateLimited, ResourceKilled, and AgentUnavailable each route to a gate/resume/abort, never a silent transition"
  - "handle_infra_outcome + gate_or_abort_infra: the dedicated GateInfra path (ResourceKilled/AgentUnavailable) bumping infra_failures on every stage, never consecutive_failures, with an abort ceiling at MAX_INFRA_FAILURES"
  - "devflow resume --phase N: relaunches saved per-phase state via launch_stage without recreating State::new/branch/worktree"
  - "ship::build_single_agent_cron_instructions: single-agent rate-limit resume record invoking devflow resume --phase N"
  - "advance_evaluated events carry decided_by_layer and a serde-pinned wire-format status (AgentStatus::as_wire_str())"
affects: [phase-18-hermes-support (18d doctor reconciliation consumes decided_by_layer + infra_failures)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Exhaustive Action match with no wildcard arm as the compile-time fail-closed guard at the advance() call site (mirrors 17-01's decide_action policy pattern)"
    - "Shared infra-failure counter across two triggers (ResourceKilled/AgentUnavailable and RateLimited) with a single ceiling check factored into a shared gate_or_abort_infra helper, avoiding a double-bump when the RateLimited path itself pre-increments"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-core/src/ship.rs
    - OPERATIONS.md
    - crates/devflow-cli/tests/snapshots/devflow-help.txt

key-decisions:
  - "handle_infra_outcome bumps infra_failures itself and gate_or_abort_infra applies the ceiling check — the AutoResume arm bumps once via its own path and only calls handle_infra_outcome when it decides NOT to resume, so the shared counter is never incremented twice for one outcome"
  - "the AutoResume arm's interim behavior (RateLimited routed through the same GateReview-style handling as Failed/Unknown) was committed first as part of Task 1 to keep the exhaustive match compiling, then replaced by the real single-agent cron auto-resume path in the Task 2 commit"
  - "devflow resume is a non-hidden, user-facing subcommand (unlike Advance) since Hermes cron and an operator manually retrying after a rate limit both invoke it directly"

requirements-completed: [17a, 17b]

coverage:
  - id: D1
    description: "advance() replaces the matches!(Failed | RateLimited) boolean with an exhaustive outcome_policy::decide_action match; a Code-stage Unknown never reaches transition(.., Stage::Validate)"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#code_unknown_does_not_transition_to_validate"
        status: pass
    human_judgment: false
  - id: D2
    description: "ResourceKilled/AgentUnavailable route through handle_infra_outcome on every stage (including Validate), bumping infra_failures and never consecutive_failures — GateInfra never calls handle_validate_outcome"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#resource_killed_on_code_bumps_infra_failures_not_consecutive_failures"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#resource_killed_on_validate_bumps_infra_not_consecutive_failures"
        status: pass
    human_judgment: false
  - id: D3
    description: "reaching MAX_INFRA_FAILURES infra outcomes aborts instead of gating again"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#infra_ceiling_aborts_instead_of_gating"
        status: pass
    human_judgment: false
  - id: D4
    description: "devflow resume --phase N loads saved state and relaunches the saved stage without recreating State::new/branch/worktree; a primary-loop RateLimited outcome writes a single-agent cron-instructions file invoking it and returns without a blocking gate"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#primary_loop_rate_limited_writes_single_agent_cron_instructions"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#single_agent_cron_instructions_resume_command_is_devflow_resume"
        status: pass
    human_judgment: false
  - id: D5
    description: "the RateLimited auto-resume path shares the infra_failures ceiling — at the ceiling it stops resuming and routes to the infra gate/abort path instead of scheduling a resume record"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#rate_limited_at_infra_ceiling_stops_resuming_and_aborts"
        status: pass
    human_judgment: false
  - id: D6
    description: "advance_evaluated emits status via AgentStatus::as_wire_str() (a ResourceKilled result emits \"resource_killed\", not \"resourcekilled\") and carries the decided_by_layer evidence field"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#advance_evaluated_emits_wire_status_and_decided_by_layer_for_resource_killed"
        status: pass
    human_judgment: false
  - id: D7
    description: "AC-1 Phase 16 terminal-contract regression tests still pass unchanged after the dispatch rework"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#terminal_hook_failure_stops_before_branch_cleanup"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-07-19
status: complete
---

# Phase 17 Plan 04: Never-Advance Dispatch + Typed-Outcome Integration Summary

**advance()'s stage-advance dispatch now matches exhaustively on outcome_policy::decide_action (Unknown/Failed/RateLimited/ResourceKilled/AgentUnavailable each route to a gate, auto-resume, or abort — never a silent transition), infrastructure outcomes use a dedicated bounded counter, and a new `devflow resume --phase N` subcommand lets the primary monitor loop auto-resume a rate-limited run through a single-agent cron record instead of the two-agent `sequentagent` handoff.**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-07-18T20:00:00-04:00 (approx.)
- **Completed:** 2026-07-18T20:18:53-04:00
- **Tasks:** 2
- **Files modified:** 4 (2 core, 2 docs/fixtures)

## Accomplishments
- Replaced `advance()`'s `matches!(result.status, Failed | RateLimited)` boolean and the "Success (or Unknown — advance...)" fallthrough with an exhaustive match on `outcome_policy::decide_action(stage, result.status)` — the `Action` enum has no wildcard arm, so a future unhandled `AgentStatus` variant is a compile error at this call site rather than a silent advance (D-01/D-06).
- Added `handle_infra_outcome` + `gate_or_abort_infra`: the dedicated `GateInfra` path for `ResourceKilled`/`AgentUnavailable`. It bumps `state.infra_failures` (saturating), persists, then either aborts at `MAX_INFRA_FAILURES` or fires the never-silent gate via `handle_stage_failure` — on every stage including Validate/Ship, and it never calls `handle_validate_outcome`/`handle_ship_failure` (review consensus #4), so `consecutive_failures` is never touched by an infrastructure fault.
- Added `Command::Resume` + `resume()`: `devflow resume --phase N` acquires the phase lock, loads the saved state via `workflow::load_state`, and relaunches the saved stage via `launch_stage` — it does NOT call `State::new`, `feature_start`, or `ensure_phase_worktree`, so agent/mode/stage are preserved from the saved state and the branch/worktree are never recreated (review consensus #5, resolving the "devflow start resets to Define" hazard).
- Added `ship::build_single_agent_cron_instructions`, a sibling to the existing `build_cron_instructions` whose resume command is `devflow resume --phase N` (agent intentionally omitted — read from saved state) instead of the two-agent `sequentagent` handoff.
- `advance()`'s new `AutoResume` arm (`handle_rate_limited_outcome`) shares the `infra_failures` counter with the infra path (D-08's intentional design): it bumps the counter first; if that reaches `MAX_INFRA_FAILURES` it stops auto-resuming and routes through the same infra gate/abort path instead of looping forever; otherwise it writes the single-agent cron-instructions file via `ship::write_cron_instructions` and returns without firing a blocking gate.
- `advance_evaluated` events now emit `status` via `AgentStatus::as_wire_str()` (review consensus #1 — a `ResourceKilled` result emits `"resource_killed"`, never the Debug-lowercase-collapsed `"resourcekilled"`) and carry the new `decided_by_layer` evidence field (D-10).
- Regenerated the `--help` snapshot and documented `devflow resume` in `OPERATIONS.md`'s Commands table and `.devflow/` file inventory (a workspace-wide `doc_check` test enforces every CLI subcommand appears in scoped operator docs — see Deviations).

## Task Commits

1. **Task 1: Exhaustive decide_action dispatch in advance() — never-advance + infra counter (D-01/D-06/D-08)** - `a7d5d22` (feat)
2. **Task 2: Primary-loop rate-limit auto-resume + structured advance_evaluated evidence (D-09/D-10)** - `134df97` (feat)

_Note: no TDD RED/GREEN split commits — per this plan's per-task (not plan-level) TDD gate scope, tests and implementation were delivered together in each task's commit, matching Plan 01/03's established convention for this phase. Task 1's commit temporarily routed `RateLimited` through the same interim gate handling as `GateReview` (to keep the match exhaustive and compiling) before Task 2's commit replaced that arm with the real auto-resume path — this made the two commits independently buildable and testable rather than one large combined diff._

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` - exhaustive `advance()` dispatch, `handle_infra_outcome`/`gate_or_abort_infra`, `Command::Resume`/`resume()`, `handle_rate_limited_outcome`, `advance_evaluated` evidence fields, 7 new unit tests
- `crates/devflow-core/src/ship.rs` - `build_single_agent_cron_instructions` + unit test
- `OPERATIONS.md` - `devflow resume` documented in the Commands table and `.devflow/` file inventory
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` - regenerated to include the new `resume` subcommand

## Decisions Made
- `handle_infra_outcome` owns the actual `infra_failures` bump; the `AutoResume` arm peeks at what the bump *would* be before deciding whether to resume or gate, and only calls `handle_infra_outcome` (which does the real bump) on the ceiling branch — this satisfies both "the RateLimited path increments infra_failures" and "handle_infra_outcome is the single ceiling/abort authority" without double-counting the same outcome.
- `devflow resume` is a plain (non-`hide = true`) subcommand, unlike the internal `devflow advance` — both a Hermes cron job and a human retrying manually after a rate limit invoke it directly, so it belongs in the public CLI surface and its `--help`/`OPERATIONS.md` documentation.
- Kept the `reason` field name (not renamed to `detail`) in the `advance_evaluated` payload — the plan's action text used "detail" informally while the acceptance criteria and existing schema both use `reason`; renaming would be an unrequested schema break.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Regenerated the `--help` snapshot after adding `Command::Resume`**
- **Found during:** Task 2 (adding the `devflow resume` subcommand)
- **Issue:** `crates/devflow-cli/tests/help_snapshot.rs` pins `devflow --help`'s output against a committed snapshot; adding a new subcommand changed the output and failed `cargo test -p devflow`.
- **Fix:** Regenerated `crates/devflow-cli/tests/snapshots/devflow-help.txt` via `cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt`. Also shortened the `Resume` variant's doc comment (added a blank-line-separated long-form paragraph) so clap's short-help column stays a single readable line rather than the full multi-sentence comment collapsing onto one row.
- **Files modified:** `crates/devflow-cli/tests/snapshots/devflow-help.txt`, `crates/devflow-cli/src/main.rs` (doc comment only)
- **Verification:** `cargo test -p devflow --test help_snapshot` passes.
- **Committed in:** `134df97` (Task 2 commit)

**2. [Rule 3 - Blocking] Documented `devflow resume` in OPERATIONS.md to satisfy the workspace-wide doc/source consistency test**
- **Found during:** Task 2, running `cargo test --workspace` (the plan's own verify commands scope to `-p devflow`/`-p devflow-core ship::`, but the phase-level `<verification>` and `success_criteria` require the full workspace suite green)
- **Issue:** `crates/devflow-core/src/doc_check.rs`'s `source_devflow_env_vars_and_subcommands_are_documented` test asserts every `Command` enum variant appears (case-insensitively) somewhere in README.md/ARCHITECTURE.md/CONTRIBUTING.md/OPERATIONS.md. Adding `Command::Resume` without a docs update failed this test.
- **Fix:** Added a `devflow resume --phase N` row to OPERATIONS.md's Commands table and updated the `cron-instructions-NN.json` file-inventory row to describe both the new single-agent resume command and the pre-existing `sequentagent` handoff command it can also hold.
- **Files modified:** `OPERATIONS.md`
- **Verification:** `cargo test --workspace` (274 devflow-core tests, including `doc_check`, all pass).
- **Committed in:** `134df97` (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 3 — blocking test failures caused directly by this plan's own new CLI surface)
**Impact on plan:** Both fixes are documentation/fixture updates required to keep the full test suite green after adding `devflow resume`; neither touches the plan's `files_modified` production logic (`crates/devflow-cli/src/main.rs`, `crates/devflow-core/src/ship.rs`) beyond the doc-comment wording needed for a readable `--help` row. No scope creep.

## Issues Encountered
None beyond the two deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- The typed-outcome taxonomy from 17-01/17-03 is now fully wired into the primary `advance()` dispatch: no outcome can silently advance, infrastructure faults are bounded and isolated from `consecutive_failures`, rate limits auto-resume safely via saved state (never re-running `State::new`), and every terminal decision emits structured evidence (`decided_by_layer`, wire-format `status`).
- Phase 18's 18d (doctor reconciliation) can now consume `decided_by_layer` + `infra_failures` from `events.jsonl`/`state-NN.json` for forensic tooling, as anticipated by 17-01's SUMMARY.
- Full workspace test suite (48 CLI unit tests + integration suites + 274 devflow-core tests + 2 monitor e2e tests), `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` all green. AC-1 Phase 16 regression tests (`terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished`, `terminal_hook_failure_stops_before_branch_cleanup`) verified passing against final HEAD, unmodified.
- No blockers for Plan 05.

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-19*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/main.rs
- FOUND: crates/devflow-core/src/ship.rs
- FOUND: .planning/phases/17-pipeline-dogfood-followup/17-04-SUMMARY.md
- FOUND commit: a7d5d22 (Task 1)
- FOUND commit: 134df97 (Task 2)
