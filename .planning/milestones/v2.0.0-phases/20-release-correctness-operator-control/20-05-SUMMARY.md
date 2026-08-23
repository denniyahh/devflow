---
phase: 20-release-correctness-operator-control
plan: 05
subsystem: pipeline-cli
tags: [rust, cli, clap, gates, state-machine, release-engineering]

# Dependency graph
requires:
  - phase: 20-01
    provides: VersionBump rewrites workspace self-pins — devflow ship's terminal hooks (finish_workflow → hooks_after_ship) inherit the fixed invariant for free since VersionBump runs unchanged
  - phase: 20-04
    provides: shared main.rs Command clap-enum region (sequential wave ordering to avoid a merge conflict with the new Release subcommand, not a functional dependency)
provides:
  - "devflow ship --phase N [--force]: a second, out-of-process consumer of the on-disk Ship gate response, recovering a phase stuck at Stage::Ship after its monitor dies"
  - "pipeline_gate::ship_override — per-phase lock + gate-pair + ack guards, routes Advance/LoopBack/Abort through the exact same finish_workflow/loop_back_to_code/abort the live monitor uses"
  - "D-02 EoP regression coverage: --force provably never bypasses the stage/lock/gate-existence/ack guards"
affects: [operator-recovery-tooling, future-hermes-ship-adjudication]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Second out-of-process consumer of one on-disk gate-response record (D-01) — read Gates::response_path directly, GateAction::from_response, then call the SAME terminal function (finish_workflow) the live poll loop calls, never a reimplemented hook batch"
    - "Per-phase lock acquired BEFORE state load (mirrors pipeline_launch::resume's exact idiom) so a manual recovery command can never race a still-live monitor's poll_response"
    - "Ack-file-as-consumption-marker: presence of Gates::ack_path alongside a response signals prior consumption by a (possibly crashed mid-finish_workflow) process — refuse and direct to devflow doctor rather than re-run terminal hooks"
    - "--force as an explicit, auditable, currently-no-op flag: accepted and echoed in CLI output, but not wired to bypass any correctness guard — prevents both silent-ignore and future unreviewed scope creep"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - OPERATIONS.md

key-decisions:
  - "--force has zero differentiated behavior in this design: no acceptance test or must_have distinguishes force=true from force=false on any path, so it is threaded through and printed for auditability but consulted by no guard — documented explicitly in ship_override's doc comment so a future change to widen its scope is a deliberate, reviewed edit, not a silent drift."
  - "Guard order fixed: lock acquire → load_state → stage==Ship → gate+response exist → ack absent → parse response → dispatch. Lock-before-load is the Codex HIGH fix; ack-after-existence is the Hermes ack-race fix."
  - "LoopBack and Abort route through the exact same loop_back_to_code/abort helpers handle_ship_outcome already calls — no ship-specific branch, verified by a dedicated Abort-routing test (Task 3)."

requirements-completed: [20e]

coverage:
  - id: D1
    description: "devflow ship --phase N advances a Stage::Ship phase with a written, unconsumed Ship response through finish_workflow — same terminal state the live monitor would reach"
    requirement: "20e"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_gate.rs#tests::ship_override_advances_via_written_response"
        status: pass
    human_judgment: false
  - id: D2
    description: "--force never skips an earlier stage, the per-phase lock, the gate-existence check, or the ack check (D-02 EoP regression)"
    requirement: "20e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#tests::ship_override_refuses_when_not_at_ship_stage"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#tests::ship_override_refuses_when_no_response_written"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#tests::ship_override_refuses_when_lock_contended"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#tests::ship_override_refuses_when_response_already_acked"
        status: pass
    human_judgment: false
  - id: D3
    description: "LoopBack/Abort responses route through the exact same shared helpers the live poll loop uses; a LoopBack forking a new detached monitor is announced explicitly"
    requirement: "20e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#tests::ship_override_abort_routes_through_abort"
        status: pass
    human_judgment: false
  - id: D4
    description: "devflow --help / OPERATIONS.md reflect the new Ship subcommand"
    requirement: "20e"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false

duration: ~50min
completed: 2026-07-23
status: complete
---

# Phase 20 Plan 05: Manual Ship Override (`devflow ship --phase N [--force]`) Summary

**A second, out-of-process consumer of the existing Ship gate response record — `devflow ship --phase N` reads an already-written, unconsumed Ship approval and drives the phase through the SAME `finish_workflow` terminal path the live monitor's poll loop would have called, recovering a phase stuck at `Stage::Ship` after its monitor dies, with `--force` provably unable to skip Validate, race the lock, or re-run an already-consumed response.**

## Performance

- **Duration:** ~50 min
- **Completed:** 2026-07-23
- **Tasks:** 3
- **Files modified:** 4 (0 created)

## Accomplishments

- `Command::Ship { phase: u32, force: bool, project: PathBuf }` added to `main.rs`, dispatched to a new `pipeline_gate::ship_override`.
- `ship_override` implements the full D-01/D-02 guard chain in order: (1) acquires `lock::acquire(project_root, phase)` BEFORE touching state, failing fail-closed on `LockError::Contended` and naming the holder pid — mirrors `pipeline_launch::resume`'s exact idiom (review: Codex HIGH); (2) requires `state.stage == Stage::Ship`, naming the actual stage otherwise (D-02 — `--force` never skips Validate or any earlier stage); (3) requires BOTH `Gates::gate_path` (request) and `Gates::response_path` (response) to exist on disk; (4) refuses if `Gates::ack_path` already exists — that means a (possibly now-dead) monitor already consumed the response, possibly mid-`finish_workflow`, and directs the operator to `devflow doctor` instead of re-running terminal hooks (review: Hermes ack-race).
- On `GateAction::Advance`, calls `finish_workflow(project_root, &mut state)` **verbatim** — no reimplemented Merge/VersionBump/ChangelogAppend/BranchCleanup batch (D-01). On `LoopBack`, routes through the exact same `loop_back_to_code` the live path uses and explicitly prints that a new, detached monitor agent is being launched (review: Antigravity LOW — `devflow ship` must never be silently long-running). On `Abort`, routes through the same shared `abort` helper.
- `--force` is accepted and echoed in the CLI's own output for explicit operator auditability, but is not consulted by any guard — the doc comment on `ship_override` states this explicitly so a future change widening its scope is a deliberate, reviewed edit rather than silent drift (Hermes LOW: explicit `--force` semantics).
- 6 new tests in `pipeline_gate.rs`'s test module: the tracer end-to-end advance, four D-02 refusal regressions (non-Ship stage, no response, contended lock, already-acked response — each covering both `--force` true and false), and an Abort-routing regression proving no ship-specific special case exists.
- Regenerated `crates/devflow-cli/tests/snapshots/devflow-help.txt` (new `ship` row) and added a `devflow ship --phase N [--force]` row to `OPERATIONS.md`'s command table naming its dead-monitor recovery purpose and the LoopBack detached-daemon behavior.

## Task Commits

Each task was committed atomically:

1. **Task 1 (tracer, tdd): End-to-end ship override — second consumer of the Ship response** - `6ec8519` (feat)
2. **Task 2 (tdd): D-02 scope + fail-closed guards — `--force` never skips Validate/lock/ack (EoP guard)** - `cadabe6` (test)
3. **Task 3: Regenerate help snapshot + OPERATIONS.md; LoopBack/Abort symmetry** - `c46aaf5` (docs)

_Task 1 is `type="tracer"`: committed as a real, production-quality end-to-end slice (the full guard chain + `ship_override_advances_via_written_response` GREEN), then re-verified before Task 2 added its refusal-only regression tests, per the tracer feedback gate. Auto mode was active for this run (workflow.auto_advance / `_auto_chain_active`), so the gate re-ran the tracer's own `<verify>` command and confirmed GREEN before proceeding — no checkpoint was surfaced._

## Files Created/Modified

- `crates/devflow-cli/src/main.rs` — `Command::Ship { phase, force, project }` + dispatch to `pipeline_gate::ship_override`.
- `crates/devflow-cli/src/pipeline_gate.rs` — new `pub(crate) fn ship_override`; 6 new tests (`ship_override_advances_via_written_response`, `ship_override_refuses_when_not_at_ship_stage`, `ship_override_refuses_when_no_response_written`, `ship_override_refuses_when_lock_contended`, `ship_override_refuses_when_response_already_acked`, `ship_override_abort_routes_through_abort`).
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated (`ship` row added to the top-level command list).
- `OPERATIONS.md` — new `devflow ship --phase N [--force]` row in the command table.

## Decisions Made

- `--force` has zero differentiated behavior in this design — none of the plan's must-haves, acceptance criteria, or the tests they mandate distinguish `force=true` from `force=false` on any code path (all four refusal tests assert identical refusal for both). Rather than inventing an undocumented, untested "Ship-gate re-verification" step to give the flag something to skip, `ship_override` accepts and prints `force` (so it's never silently ignored — Hermes LOW) but consults it in zero guards, with an explicit doc-comment note that this is the flag's full current scope. This keeps the implementation exactly as narrow as the tests require, per CLAUDE.md's simplicity-first rule (no speculative behavior beyond what was asked).
- Guard order is fixed and load-bearing: lock acquire happens BEFORE `workflow::load_state`, closing the Codex HIGH race window against a live monitor's `poll_response`; the ack check happens AFTER confirming gate+response exist, so the ack-race refusal message is reached only once the request/response pair is otherwise valid.
- LoopBack and Abort deliberately reuse `loop_back_to_code`/`abort` verbatim rather than any ship-specific branch — verified by a dedicated `ship_override_abort_routes_through_abort` test asserting the shared path's own observable effects (state cleared, gate files removed, `workflow_aborted` emitted), not just that the function returns `Ok`.

## Deviations from Plan

None — plan executed as written across all three tasks; no Rule 4 (architectural) decisions were needed. Rule 1/2/3 auto-fixes: none encountered.

## Issues Encountered

None blocking. Commit-splitting for Task 1 vs. Task 2 required a manual two-pass edit (write all guard logic + all 5 initial tests together, then temporarily remove Task 2's four refusal tests, commit Task 1, re-add them, commit Task 2) since both tasks touch the same function in the same file — this is a mechanical consequence of the plan's own task boundaries (Task 1's action already specifies the full guard chain; Task 2 adds the EoP regression tests against it), not a defect.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- 20e is fully resolved: `devflow ship --phase N [--force]` is real, tested, and provably scoped — `--force` cannot skip Validate, race the per-phase lock, or re-run an already-consumed response.
- `cargo test --workspace` → 0 failed (115 passed in the `devflow` binary target including all 6 new tests, 324 in `devflow-core`, every other target green); `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are both clean.
- This was the final plan of Phase 20 (20a–20e all complete) — the phase-level verification/ship steps are the next action, not further plan execution.

---
*Phase: 20-release-correctness-operator-control*
*Completed: 2026-07-23*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/main.rs
- FOUND: crates/devflow-cli/src/pipeline_gate.rs
- FOUND: crates/devflow-cli/tests/snapshots/devflow-help.txt
- FOUND: OPERATIONS.md
- FOUND commit: 6ec8519 (feat: Task 1 — ship_override + tracer test)
- FOUND commit: cadabe6 (test: Task 2 — D-02 EoP regression guards)
- FOUND commit: c46aaf5 (docs: Task 3 — help snapshot + OPERATIONS.md + abort-routing test)
