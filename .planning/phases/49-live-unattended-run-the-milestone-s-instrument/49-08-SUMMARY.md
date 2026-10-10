---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 08
subsystem: testing
tags: [bash, python, evidence, git, negative-controls]
requires:
  - phase: 49-03
    provides: Real capture hook and collector
  - phase: 49-07
    provides: Negative-arm checker
provides:
  - Positive-arm checks for base, fork, event order, gate responses, merge history and PR target
  - Arm pairing on binary hash, build commit and differing base branch
  - Evidence-based shipped, cap_reached or not_ready attempt gate
affects: [49-10, 49-11, 49-12]
actuals:
  tasks: 3
  commits: 6
plan_head_before: e8af1f1
tech-stack:
  added: []
  patterns: [Real-capture fixtures, required JSON keys indexed, ledger-relative resolution]
key-files:
  created: []
  modified: [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/check-evidence.sh]
key-decisions:
  - Added controls for each positive check and the paired build commit, beyond the planned nine cases.
  - Manifest paths outside the collection are refused before invoking sha256sum.
requirements-completed: [VERIFY-01]
coverage:
  - id: D1
    description: Positive arm judges snapshots, git and ordered events after the terminal batch
    requirement: VERIFY-01
    verification:
      - kind: integration
        ref: check-evidence.sh --selftest#positive arm passes
        status: pass
      - kind: other
        ref: 49-08 Task 1 automated gate
        status: pass
    human_judgment: false
  - id: D2
    description: Arm pair refuses different binaries, build commits and agreeing base branches
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: check-evidence.sh --selftest#pair passes and pair controls
        status: pass
      - kind: other
        ref: 49-08 Task 2 automated gate
        status: pass
    human_judgment: false
  - id: D3
    description: Attempt gate validates collection, verdict consistency and arm criteria
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: check-evidence.sh --selftest#attempt gate cases
        status: pass
      - kind: other
        ref: 49-08 Task 3 automated gate including independent empty-home refusal
        status: pass
    human_judgment: false
completed: 2026-09-30
status: complete
---

# Phase 49 Plan 08: Evidence Checker Summary

The checker now evaluates the positive arm, pairs it with the negative arm, and decides checkpoint
readiness from recorded evidence rather than the operator's reply.

## Accomplishments

- Added eleven positive checks, fork-point values, a complete gate census and extra-gate deviation notes.
- Added three pair checks. Missing inputs fail by name; snapshot resolution follows each ledger's own root.
- Added the attempt gate: verifying collection manifest, verdict/event agreement and all arm checks for
  shipped; three documented stopped attempts for cap_reached; other states remain not_ready.

## Task Commits

1. `28b03ca` RED positive fixture, `16173cd` positive checks.
2. `da7a4bb` RED pair controls, `e18e088` pair checks.
3. `d1a5cdb` eight RED attempt-gate cases, `a5d428c` evidence gate.

## Verification

All three task gates printed gate_rc=0 on 2026-09-30. Selftest: 34 passed, 0 failed.
Negative-driver selftest remains 8 passed, 0 failed. Bash syntax, ShellCheck and the pinned-container
workspace/all-target Clippy check passed. The gate's independent empty-home case exited nonzero
with verdict=not_ready. Test-case counts were parsed from the terminal selftest line and independently
checked by the named-case task gates.

The positive fixture invokes the real snapshot hook and collector over a planted run. Its final
integration tip is independently asserted to have one parent after a real merge, tag and changelog
commit; the checker uses the collected merge history. Controls reject moved bases, reordered gates,
wrong merge targets, missing snapshots, wrong responders, mismatched binaries/build commits,
manifest mutation and verdict/event disagreement. An extra Validate gate is reported and accepted.

These checks establish checker discrimination on fixtures. They do not establish live agent behavior,
checkpoint auto-decision, actual Ship completion or reliability. VERIFY-01 still needs live evidence.
The cap counts documented stopped verdicts, not provenance-proven attempts, as explicitly deferred
in this plan's review ledger; cap_reached closes the attempt checkpoint as failure, not VERIFY-01 met.

## Deviations from Plan

Eight additional positive/pair mutation cases cover otherwise untested check directions. A manifest
path guard prevents reads outside collected evidence. Planned grep searches were executed as rg
under the operator's shell instruction. Execution was inline under the Codex skill fallback.

## Issues Encountered

None.

## Next Readiness

49-09 remains incomplete in wave 3. No positive live attempt or external repository was created.

## Self-Check

All six task commits, the modified script and three passing task-gate logs were checked before authoring.
