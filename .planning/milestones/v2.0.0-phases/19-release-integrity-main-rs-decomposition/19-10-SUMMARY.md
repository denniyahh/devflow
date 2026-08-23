---
phase: 19-release-integrity-main-rs-decomposition
plan: 10
subsystem: documentation
tags: [codebase-map, testing, roadmap, main-rs-split]

requires:
  - phase: 19-release-integrity-main-rs-decomposition
    provides: "19-09 final module layout and measured line counts"
provides:
  - "STRUCTURE.md aligned with the split CLI module layout and coupling constraints"
  - "TESTING.md aligned with live tests, shared fixtures, ENV_MUTEX invariant, and false-green traps"
  - "ROADMAP Phase 19 entry reconciled to 10/11 completed plans"
affects: [19-11, phase-20-planning]

tech-stack:
  added: []
  patterns:
    - "Documentation claims verified against HEAD before inclusion"
    - "ROADMAP updates scoped to one phase section"

key-files:
  modified:
    - .planning/codebase/STRUCTURE.md
    - .planning/codebase/TESTING.md
    - .planning/ROADMAP.md

key-decisions:
  - "Document pipeline_launch/pipeline_outcomes/pipeline_gate as a mutually cyclic unit for planning purposes, despite their separate files."
  - "Document commands, staleness, parallel, and config_parse as the split's independent planning lanes."
  - "Use project_root_walks_up_to_nearest_devflow_ancestor as the replacement verified test example because it remains owned by the thin main.rs root."

requirements-completed: [19f]

coverage:
  - id: D1
    description: "Codebase structure map matches HEAD"
    requirement: "19f"
    verification:
      - kind: other
        ref: "Every cited crates/**/*.rs path exists; all ten CLI module line counts match wc -l exactly; coupling and pub(crate) assertions present"
        status: pass
    human_judgment: false
  - id: D2
    description: "Testing guide cites live tests and preserves the D-04 invariant"
    requirement: "19f"
    verification:
      - kind: unit
        ref: "cargo test -p devflow project_root_walks_up_to_nearest_devflow_ancestor: 1 passed, 0 failed"
        status: pass
      - kind: other
        ref: "All eight named integration .rs files and the help snapshot exist; ENV_MUTEX, exactly one mutex, test_support, devflow-cli warning, and ai-change-acceptance assertions present"
        status: pass
    human_judgment: false
  - id: D3
    description: "Phase 19 roadmap reconciled without collateral edits"
    requirement: "19f"
    verification:
      - kind: other
        ref: "11 plan files exactly match 11 Phase 19 ROADMAP entries; Phase heading count remained 24; diff touched only Phase 19"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 10: Codebase Documentation Reconciliation Summary

**Regenerated the two stale codebase maps from live source and reconciled Phase 19's roadmap entry, preserving the split's real coupling constraints and test-safety rules with no source changes.**

## Accomplishments

- Replaced the obsolete single-file CLI map with the measured ten-file layout, including exact line counts and module ownership sourced from each module's doc comment.
- Recorded that the three pipeline modules are cyclic and `preflight.rs` is bidirectionally coupled to `pipeline_launch.rs`; the independent planning benefit applies to commands, staleness, parallel, and configuration parsing.
- Replaced the deleted test example with `project_root_walks_up_to_nearest_devflow_ancestor`, verified by a targeted run reporting exactly one passing test.
- Documented `crate::test_support`, its fixture inventory, the D-04 single-`ENV_MUTEX` invariant, core's separate-test-binary exception, and the `--exact`, package-name, and binary-only false-green traps.
- Updated only the Phase 19 ROADMAP section from 8/11 to 10/11 and marked plans 09 and 10 complete.

## Task Commits

1. `5d8d614` - `docs(19-10): regenerate codebase structure map`
2. `ed75dc9` - `docs(19-10): regenerate testing conventions`
3. `c92a999` - `docs(19-10): reconcile phase 19 roadmap`

## Verification

- `STRUCTURE.md`: every cited Rust path exists; line-count spot checks and the full ten-module check matched `478`, `2326`, `585`, `1719`, `789`, `772`, `1284`, `530`, `75`, and `288` exactly.
- `TESTING.md`: all six CLI and two core integration-test files exist; the stale test name is absent; required invariant and acceptance-contract references are present.
- `ROADMAP.md`: plan-file and roadmap-entry sets diff empty; `### Phase` heading count stayed `24`; the staged diff contained only Phase 19 hunks.
- `git status --porcelain -- crates/` remained empty throughout the plan.

## Deviations From Plan

- The ROADMAP had already been partially reconciled to 8/11 before this plan, rather than retaining the original `0 plans`/`TBD` placeholder described in the plan. The scoped update advanced it from the actual starting state to 10/11.
- `STRUCTURE.md` contained additional verified-stale paths and layout descriptions beyond the single large-`main.rs` claim. Regeneration corrected those within the document's existing section structure.

## Next Phase Readiness

Plan 19-11 is the only incomplete Phase 19 plan. It owns CI-on-branch evidence, final equivalence reconciliation, the downstream scratch-repository reproduction, `ENV_MUTEX` disposition, and the requirement roll-call.
