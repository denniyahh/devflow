---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 06
subsystem: testing
tags: [bash, git, python, unittest, jq, gsd]
requires:
  - phase: 49-02
    provides: Live payload topology and scaffold selftest
provides:
  - Complete disposable retry-budget payload with a blocking-human decision and RED unittest
  - Twenty named pre-launch checks including installed GSD hook resolution
  - Config rendering bridge against DevFlow REAL_SHAPE and its Rust writer test
affects: [49-07, 49-09, 49-10]
actuals:
  tasks: 3
  commits: 6
plan_head_before: 6125665
tech-stack:
  added: []
  patterns: [Named refusal controls, read-only archive test, fail-closed hook JSON validation]
key-files:
  created: []
  modified: [scripts/scratch-dogfood-repo.sh]
key-decisions:
  - Added opposite-result controls for the wave-hook probe, breaking footer and tags alongside the six planned controls.
  - Python reference tests run with -B so timestamp and size cache collisions cannot mask the changed budget.
requirements-completed: [VERIFY-01]
coverage:
  - id: D1
    description: Payload seeds the exact opening-line checkpoint and a discriminating RED test
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: scripts/scratch-dogfood-repo.sh --selftest#st_payload_tests
        status: pass
      - kind: other
        ref: 49-06 Task 1 automated gate
        status: pass
    human_judgment: false
  - id: D2
    description: Pre-launch checker rejects configuration traps and resolves installed GSD hooks
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: scripts/scratch-dogfood-repo.sh --selftest#st_hook_controls and st_check_mutation
        status: pass
      - kind: other
        ref: 49-06 Task 2 automated gate
        status: pass
    human_judgment: false
  - id: D3
    description: Payload config rendering agrees with DevFlow fixture and writer round trip
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: gsd_config::tests::writing_the_flag_leaves_every_other_key_byte_identical
        status: pass
      - kind: other
        ref: 49-06 Task 3 automated gate
        status: pass
    human_judgment: false
completed: 2026-09-30
status: complete
---

# Phase 49 Plan 06: Complete Live Payload Summary

The scaffold now writes the retry-budget project, pre-authored blocking-human decision plan,
RED test and draft validation map; its checker rejects known pre-launch traps.

## Accomplishments

- Generated PROJECT, REQUIREMENTS, STATE, the decision-first plan, test and validation map on main before integration forks.
- Added read-only archive testing, external_verify/history/tag/ancestor-manifest checks and GSD hook resolution with opposite-result controls.
- Extracted REAL_SHAPE from DevFlow source, proved jq reproduces its bytes, and ran the exact Rust writer test.

## Task Commits

1. `61662d3` test: payload cases fail (10 passed, 4 failed); `621108d` feat: payload cases pass.
2. `5681ead` test: five invalid copies accepted by the old checker; `adec247` feat: each refused solely by its named check.
3. `8d637ab` test: bridge fails with REAL_SHAPE ABSENT; `5a71b46` feat: extraction and config-shape check.

## Verification

All three plan task gates printed `gate_rc=0` on 2026-09-30. Selftest: 26 passed, 0 failed;
check: 20 passed, 0 failed. The independent task-1 gate compared the opening line and archived
RED failure. Rust writer test: exactly 1 passed, 851 filtered out. Bash syntax and ShellCheck
(JSON diagnostics: empty array) passed. Host workspace/all-target Clippy and pinned-container
`scripts/check-in-container.sh clippy` both passed.

These checks establish the scaffold and preflight tool behavior. They do not establish a live
DevFlow launch, checkpoint auto-decision, recovery, Ship completion or run reliability. The
rendering bridge does not cover arbitrary special-character string escaping; the payload has none.
VERIFY-01 remains a phase-level live-evidence obligation despite the template's requirements list.

## Deviations from Plan

Three additional controls exercise the wave-hook probe's opposite result, breaking footers and tags.
The plan's two literal grep calls were run as equivalent rg calls under the operator's shell rule.
Execution was inline under the Codex skill fallback; no subagents were dispatched.

## Issues Encountered

ShellCheck's text renderer failed encoding a Unicode table under the host locale; structured JSON
output was used and the literal Markdown backticks received a scoped SC2016 suppression.
Initial sandbox Docker access failed; the pinned check passed with authorized daemon access.

## Next Readiness

49-07 is the next incomplete plan in wave 2. No live attempt or external repository was created.

## Self-Check

The modified script and six task commits exist. All task-gate logs were checked before this summary.
