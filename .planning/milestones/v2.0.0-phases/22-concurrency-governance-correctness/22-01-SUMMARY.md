---
phase: 22-concurrency-governance-correctness
plan: 01
subsystem: cli
tags: [rust, gates, doctor, cli, dedup]

# Dependency graph
requires: []
provides:
  - "resolve_single_open_gate_stage: shared omitted-stage resolver used by gate_respond and gate_show (commands.rs)"
  - "gate_show single-read flow: one Gates::list_open(project_root) call backs both resolution and selection"
  - "collect_planning_doc_findings now threads devflow_core::config::MAIN into tag_exists_and_reachable instead of a hardcoded \"main\" literal"
affects: [devflow-cli-commands]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Extract-shared-resolver over copy-paste: gate_respond and gate_show's omitted-stage branches now call one function so their no-open/single-open/ambiguous-open wording cannot drift"
    - "Read-once-then-derive: gate_show fetches the open-gate collection a single time and both infers the stage and looks up the selected gate from that same Vec, closing the WR-03 TOCTOU without adding a lock or retry"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "No locking/transactional gate protocol introduced for WR-03 — the fix is reading Gates::list_open once and reusing the result, not synchronizing concurrent access."
  - "collect_planning_doc_findings takes the named devflow_core::config::MAIN constant rather than loading GitFlowConfig — doctor's reconciliation stays read-only with unchanged output for the current MAIN value."

requirements-completed: ["999.30", "DEN-55", "C-01", "C-02", "C-03"]

coverage:
  - id: WR-01
    description: "gate_respond and gate_show share one omitted-stage resolver (resolve_single_open_gate_stage); no-open, single-open, and ambiguous-open wording verified identical across both commands"
    requirement: "999.30/DEN-55 WR-01"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#gate_show, gate_respond tests"
        status: pass
    human_judgment: false
  - id: WR-03
    description: "gate_show resolves and selects from one fetched Gates::list_open(project_root) collection, eliminating the redundant second read"
    requirement: "999.30/DEN-55 WR-03"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#gate_show tests"
        status: pass
    human_judgment: false
  - id: WR-02
    description: "collect_planning_doc_findings passes devflow_core::config::MAIN to tag_exists_and_reachable instead of a hardcoded \"main\" literal"
    requirement: "999.30/DEN-55 WR-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#reconcile_planning_docs / collect_planning_doc_findings tests"
        status: pass
    human_judgment: false

# Metrics
duration: not tracked (executed 2026-07-24; SUMMARY backfilled retroactively 2026-07-25 — see note below)
completed: 2026-07-24
status: complete
---

# Phase 22 Plan 01: Shared Gate Resolution + MAIN Constant Summary

**`resolve_single_open_gate_stage` is now the single source of omitted-stage behavior for both `gate_respond` and `gate_show`; `gate_show` reads the open-gate collection once instead of twice; `collect_planning_doc_findings` reconciles against `devflow_core::config::MAIN` instead of a hardcoded `"main"` literal — closing WR-01, WR-02, and WR-03 from the Phase 21 review (999.30/DEN-55) with no CLI output or gate-semantics change.**

## Retroactive backfill note

This SUMMARY.md was written 2026-07-25, one day after the plan actually executed and shipped. The executor completed both tasks (commit `c442e00`) but did not write this file at the time — a known gsd-executor self-report gap. Content below is reconstructed from `c442e00`'s commit message/diff, the plan's `<source_audit>`, and ROADMAP.md's Phase 22 entry (which records the same-day validation and the 2026-07-24 re-verification: 537 tests across 13 binaries, 0 failed, zero coupling to the in-flight process-management redesign). No task-level timing was captured, so `duration` above is honest about that gap rather than invented.

## Accomplishments
- Extracted `resolve_single_open_gate_stage`, called by both `gate_respond` and `gate_show` for omitted-`--stage` resolution — no-open, single-open, and ambiguous-open outcomes and their exact wording now come from one place instead of two independently-maintained copies.
- `gate_show` now calls `Gates::list_open(project_root)` once and derives both the phase candidates and the selected gate from that single `Vec<OpenGate>`, closing the narrow TOCTOU where a second read could race an in-flight `gate_respond`.
- `collect_planning_doc_findings` now receives `devflow_core::config::MAIN` and passes it into `tag_exists_and_reachable`, replacing the hardcoded `"main"` literal in doctor's planning-doc reconciliation.

## Task Commits

1. **Task 1 (shared resolver + gate_show single-read) + Task 2 (MAIN constant)** — `c442e00` (fix) — both tasks landed in one commit rather than two; see plan `22-01-PLAN.md` for the original two-task breakdown.

## Files Modified
- `crates/devflow-cli/src/commands.rs` — `resolve_single_open_gate_stage` extracted and adopted by `gate_respond`/`gate_show`; `gate_show` reduced to one `Gates::list_open` read; `collect_planning_doc_findings` now imports and uses `devflow_core::config::MAIN`. Net **−6 lines** (124 changed: 63 insertions, 61 deletions per `c442e00`'s diffstat).

## Decisions Made
- No lock, retry loop, or transactional gate protocol was introduced for WR-03 — the TOCTOU is closed by reading once and reusing the result, per the plan's explicit constraint against broadening into a concurrency protocol.
- Doctor reconciliation stays read-only; `MAIN` replaces the literal without loading `GitFlowConfig` or changing reconciliation severity/cutoff rules.

## Deviations from Plan

Both tasks landed in a single commit (`c442e00`) rather than the two separate task commits the plan's structure implies. No behavioral deviation — all three WR-01/WR-02/WR-03 must-haves are present in the diff.

## Issues Encountered

None recorded in the commit message.

## Next Phase Readiness

- 22-02 (IN-01, single-pass `stage_launched` timestamp summary) depended on this plan per its `depends_on: ["22-01"]` and executed after it (`2bbfabd`).
- Validation: per ROADMAP.md Phase 22 entry, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` were run clean at execution time (2026-07-24) and independently re-verified after the 999.37 repository-corruption incident: 537 tests across 13 binaries, 0 failed, zero coupling to `monitor`/`monitor_pid`/pgid/spawn/teardown/`Liveness`/`process_group`/supervisor.

## Self-Check: PASSED (retroactive)

- FOUND: crates/devflow-cli/src/commands.rs
- FOUND commit: c442e00 (Tasks 1+2)
- This SUMMARY.md itself is the backfill artifact — no separate SUMMARY commit was made at original execution time.

---
*Phase: 22-concurrency-governance-correctness*
*Completed: 2026-07-24 (SUMMARY backfilled: 2026-07-25)*
