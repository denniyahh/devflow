---
phase: 16-pipeline-reliability-hardening
plan: 04
subsystem: review-pipeline
tags: [prompting, code-review, multi-angle, ship-gate, incremental-review]

requires:
  - phase: 16-02
    provides: env-over-file review_angles resolver
provides:
  - capability-conditional high-depth multi-angle Ship review instructions
  - project-configurable Ship review angle rendering
  - advisory shallow review after each Code plan or wave
affects: [ship, code, review-history, operator-quality-gates]

tech-stack:
  added: []
  patterns:
    - capability-conditional parallel fan-out with sequential fallback
    - project-aware prompt construction with a default-compatible library wrapper

key-files:
  created: []
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "The CLI calls stage_prompt_for_project with the canonical project root; stage_prompt remains the default-only compatibility API."
  - "Incremental review is explicitly advisory and cannot pause execution or request human input."

patterns-established:
  - "Ship reviews fan out by angle when supported, otherwise perform focused sequential passes, then merge and deduplicate into one REVIEW.md."
  - "Code performs a quick shallow drift check; Ship remains the authoritative review."

requirements-completed: [16d, 16e]

coverage:
  - id: D1
    description: "Ship prompts require five high-depth review angles with conditional parallel fan-out, sequential fallback, and one deduplicated REVIEW.md."
    requirement: 16d
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#ship_prompt_includes_multi_angle_conditional_review"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#ship_prompt_defines_critical_gate_and_review_failed_contract"
        status: pass
    human_judgment: false
  - id: D2
    description: "Project review_angles replace built-in Ship angles through canonical-root prompt construction."
    requirement: 16d
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#ship_prompt_uses_project_review_angle_override"
        status: pass
      - kind: other
        ref: "cargo clippy --workspace -- -D warnings"
        status: pass
    human_judgment: false
  - id: D3
    description: "Code prompts include a non-interactive advisory self-review after each plan or wave without Ship review sequencing."
    requirement: 16e
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#code_stage_prompt_is_unchanged_single_command_template"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-07-18
status: complete
---

# Phase 16 Plan 04: Deep and Incremental Review Prompts Summary

**Ship now drives one deduplicated high-depth review across five focused angles, while Code performs lightweight non-blocking drift checks as each plan or wave lands.**

## Performance

- **Duration:** 2 min
- **Started:** 2026-07-18T00:57:50Z
- **Completed:** 2026-07-18T00:59:36Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Added built-in review angles for doc accuracy, leaked data, CI/build correctness, external-state claims, and one generalist deep pass.
- Threaded canonical project-root configuration into CLI prompt construction so `review_angles` overrides replace defaults.
- Added advisory incremental Code self-review without interactive questions or a second blocking gate.

## Task Commits

1. **Task 1 RED: Multi-angle Ship review prompt contract** - `75362f6` (test)
2. **Task 1 GREEN: Multi-angle Ship review rendering** - `3decea6` (feat)
3. **Task 2 RED: Incremental Code review prompt contract** - `1f5bae8` (test)
4. **Task 2 GREEN: Advisory incremental Code review** - `b094f0c` (feat)

## Files Created/Modified

- `crates/devflow-core/src/prompt.rs` - Renders default/overridden angles, fan-out guidance, and incremental review text.
- `crates/devflow-cli/src/main.rs` - Supplies the resolved project root to prompt construction.

## Decisions Made

- Final default angle text:
  - `doc-accuracy cross-reference (do documented claims match source?)`
  - `security / leaked-data (does anything commit secrets, session data, or telemetry?)`
  - `CI/build correctness (can a failing step still report green?)`
  - `external-state claims (does the diff claim merges, tags, or deletions that are not actually true?)`
  - `one generalist deep pass`
- Exact fan-out contract: `If your harness supports parallel finder subagents, dispatch one per angle; otherwise run each angle as a focused sequential pass. Merge and deduplicate every angle's findings into one REVIEW.md.`
- Incremental review key phrase: `Advisory incremental self-review`; it records drift and continues, leaving authority to Ship.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ship review artifacts remain one `REVIEW.md`, ready for Plan 16-07 history correlation.
- No blockers remain from Plan 16-04.

## Self-Check: PASSED

---
*Phase: 16-pipeline-reliability-hardening*
*Completed: 2026-07-18*
