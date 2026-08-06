---
phase: 16-pipeline-reliability-hardening
plan: 06
subsystem: cli-ergonomics
tags: [project-root, worktrees, clap, gates, recovery]

requires:
  - phase: 16-01
    provides: trustworthy terminal merge completion
  - phase: 16-03
    provides: shared main.rs completion and capture changes
provides:
  - nearest-ancestor .devflow project resolution for every CLI command
  - positional or flagged gate-stage selection without project-path misbinding
  - actionable corrupt legacy-state warning
affects: [status, gates, logs, recover, all-cli-commands]

tech-stack:
  added: []
  patterns:
    - canonicalize once then walk parents to the nearest state marker
    - reserve gate project selection for explicit --project

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-core/src/workflow.rs

key-decisions:
  - "Gate approve/reject accept STAGE positionally or through --stage; project is now an explicit --project option with a dot default."
  - "When no .devflow ancestor exists, project_root returns the original canonicalized path unchanged."

patterns-established:
  - "All subcommands continue to funnel through one project_root resolver."

requirements-completed: [16f, 16g]

coverage:
  - id: D1
    description: "Nested paths resolve to the nearest .devflow ancestor while idle and missing paths preserve prior behavior."
    requirement: 16f
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#project_root_walks_up_to_nearest_devflow_ancestor"
        status: pass
      - kind: manual_procedural
        ref: "cargo run -q -p devflow -- status from .worktrees/phase-16"
        status: pass
    human_judgment: false
  - id: D2
    description: "Gate approve accepts positional ship, --stage ship, and bare auto-resolution forms without treating ship as a project path."
    requirement: 16g
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#gate_approve_arg_parsing_accepts_positional_stage"
        status: pass
    human_judgment: false
  - id: D3
    description: "Corrupt legacy-state warnings point operators to recover --clean."
    requirement: 16g
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/workflow.rs#migrate_legacy_state_warning_names_recovery_command"
        status: pass
    human_judgment: false

duration: 3min
completed: 2026-07-18
status: complete
---

# Phase 16 Plan 06: Worktree-Aware CLI Resolution Summary

**Every CLI command can now find the nearest DevFlow state root from nested worktree paths, while gate and corrupt-state mistakes produce actionable behavior.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-07-18T01:11:40Z
- **Completed:** 2026-07-18T01:14:29Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Replaced bare path canonicalization with a bounded nearest-ancestor `.devflow` walk-up.
- Accepted `gate approve 15 ship` directly while preserving `--stage ship` and bare auto-resolution.
- Added the sanctioned `devflow recover --clean` command to corrupt legacy-state warnings.

## Task Commits

1. **Task 1 RED: Project-root walk-up contract** - `2bbd546` (test)
2. **Task 1 GREEN: Shared nearest-state-root resolver** - `9b5eb20` (fix)
3. **Task 2 RED: Gate positional-stage regression** - `70c4e15` (test)
4. **Task 2 GREEN: Gate parsing and warning ergonomics** - `c2c8242` (fix)

## Files Created/Modified

- `crates/devflow-cli/src/main.rs` - Walks up project roots and normalizes positional/flagged gate stages.
- `crates/devflow-core/src/workflow.rs` - Includes the recovery command in corrupt legacy-state warnings.

## Decisions Made

- Chosen GateCmd approach: make `STAGE` an optional positional convenience, retain `--stage` as a separate mutually exclusive option, and move project selection to `--project` with default `.`.
- The walk-up stops at the first `.devflow` directory. Reaching filesystem root without a hit returns the starting canonical path, preserving new/idle-project behavior.
- Live smoke from `.worktrees/phase-16`: `devflow status` reported active Phase 15 and open `feature/phase-16`, not `idle`.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 16-07 can expose history from any nested operator working directory through the shared resolver.
- No blockers remain from Plan 16-06.

## Self-Check: PASSED

---
*Phase: 16-pipeline-reliability-hardening*
*Completed: 2026-07-18*
