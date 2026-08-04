---
phase: 12-bootstrap-housekeeping
plan: 07
subsystem: regression-testing
tags: [rust, cli, config, parser, completion-marker]
requires:
  - phase: 11-refactor-gsd-native
    provides: Config-free pipeline and DEVFLOW_RESULT parsing
provides:
  - Non-regression coverage proving .devflow.yaml is ignored
  - Explicit completion-marker tail-scan semantics and long-output coverage
affects: [configuration, agent-result, phase-13]
tech-stack:
  added: []
  patterns: [last-valid-marker wins within bounded output tail]
key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-core/src/agent_result.rs
key-decisions:
  - "Retain character-boundary-safe tail extraction and document its ASCII marker assumption instead of introducing a byte-index rewrite."
patterns-established:
  - "Legacy configuration files require an explicit ignored-input regression guard."
requirements-completed: [WR-10, WR-09]
coverage:
  - id: D1
    description: "A malformed stray .devflow.yaml does not influence CLI behavior."
    requirement: WR-10
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/phase7_cli.rs#devflow_ignores_stray_devflow_yaml"
        status: pass
    human_judgment: false
  - id: D2
    description: "The last valid DEVFLOW_RESULT marker wins in long agent output."
    requirement: WR-09
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::parse_marker_lines_returns_last_marker_in_long_output"
        status: pass
    human_judgment: false
duration: 3min
completed: 2026-07-08
status: complete
---

# Phase 12 Plan 07: Legacy Config and Marker Parser Guards Summary

**Legacy config scaffolding is removed, stray `.devflow.yaml` files are explicitly ignored, and final completion markers win reliably in long output.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-07-08T22:03:00Z
- **Completed:** 2026-07-08T22:06:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Deleted dead v1 `write_config` and `write_last_ship` test helpers.
- Added an e2e guard proving malformed `.devflow.yaml` content is not read.
- Documented bounded ASCII marker scanning and added long-output last-marker-wins coverage.

## Task Commits

1. **Task 1: Guard ignored legacy config** - `8efc4a1` (test)
2. **Task 2: Guard final completion marker wins** - `f87432c` (test)

## Files Created/Modified

- `crates/devflow-cli/tests/phase7_cli.rs` - Removes stale helpers and adds the config-ignore e2e test.
- `crates/devflow-core/src/agent_result.rs` - Documents marker assumptions and tests final-marker precedence.

## Decisions Made

- Preserved the existing Unicode-safe character tail extraction because the review found no correctness defect; clarified the ASCII marker and last-wins contract instead.

## Deviations from Plan

None - plan executed as specified.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

WR-09 and WR-10 are complete. Workspace-root `.devflow.yaml` removal remains correctly assigned to Phase 13.

## Self-Check: PASSED

- `cargo test -p devflow --test phase7_cli`: 6 passed.
- `cargo test -p devflow-core agent_result::`: 27 passed.
- `cargo clippy --workspace --tests -- -D warnings`: passed.
- `cargo fmt --check`: passed.
- No references to the deleted helper functions remain.

---
*Phase: 12-bootstrap-housekeeping*
*Completed: 2026-07-08*
