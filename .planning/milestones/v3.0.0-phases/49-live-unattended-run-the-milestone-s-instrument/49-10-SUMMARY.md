---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 10
subsystem: documentation
tags: [runbook, systemd, evidence, review]
requires:
  - phase: 49-08
    provides: Evidence arm and attempt checker
  - phase: 49-09
    provides: Pinned launcher and successful real authentication probe
provides:
  - Operator-owned prepare/negative/repository/start/gate/collect/checkpoint procedure
  - Confirmed-death, stale-gate, collection retry and scoped cleanup instructions
  - Host setup checklist for the live attempt
  - Source-verified two-lane review and focused recheck
  - Completed producer metadata contract (creation time, tool versions, negative unit)
affects: [49-11, 49-12]
actuals:
  tasks: 2
  commits: 4
plan_head_before: 5041557
tech-stack:
  added: []
  patterns: [One tool per runbook line, names-only auth diagnosis, positive death confirmation]
key-files:
  created: [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md]
  modified:
    - .planning/user/DEV-SETUP-CHECKLIST.md
    - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh
key-decisions:
  - Preserve stale gates as evidence after confirmed death rather than requiring a dead waiter to answer them.
  - Preserve original collection and verdict before retrying a transient lookup on an ended shipped run.
  - Retain the explicit KillMode and raw snapshot-provenance deferrals without overclaiming cleanup/integrity.
requirements-completed: [VERIFY-01]
coverage:
  - id: D1
    description: Runbook commands match implemented tool flags and have safe diagnostic/cleanup paths
    requirement: VERIFY-01
    verification:
      - kind: other
        ref: 49-10 Tasks 1-2 gates and two-tool-line failing mutation
        status: pass
      - kind: other
        ref: exact runbook PID parser NULL/ABSENT/PID and false/zero/string controls
        status: pass
    human_judgment: true
    rationale: Structural/source checks do not prove that an operator can complete a live attempt; 49-11 exercises the procedure.
  - id: D2
    description: Producer supplies the existing evidence contract metadata
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: launch-attempt.sh --selftest (17 cases; RED producer-contract control)
        status: pass
      - kind: integration
        ref: real temporary prepare on 6c935ed with versions, timestamp, commit and negative unit checked
        status: pass
    human_judgment: false
completed: 2026-09-30
status: complete
---

# Phase 49 Plan 10: Reviewed Operator Runbook Summary

The runbook defines the operator's whole attempt, preserves original evidence on failures,
and stops short of substituting a reply or a structural test for live proof.

## Verification

Both task gates pass on the repaired runbook; the required headings were checked in order.
A two-tool-line mutation fails the gate by name. All 25 Bash blocks pass syntax checks without
execution. The exact PID parser accepts distinct NULL/ABSENT/PID cases and rejects false, zero
and string controls. The launcher has 17 passing fake cases, including the historical RED
metadata failure; a real temporary prepare verifies all required metadata fields. Pinned-container
workspace/all-target Clippy passes. Capture hook, collector, watcher and sanitizer selftests pass
6/11/7/12 cases respectively, with independent counts agreeing with their terminal summaries.

Two independent external lanes reviewed 0c51f71 and rechecked repaired 6c935ed. Both rechecks
report no remaining blockers in the six targeted repairs. Reports and source-verified dispositions
are committed in .planning/reviews/49-runbook-2026-09-30. pi failed provider authentication and is
explicitly dropped; the prescribed Hermes/DeepSeek fallback completed, with model verified in its
session record. Antigravity completed independently. Production runbook/launcher paths remain
unchanged from the rechecked revision.

This proves bounded tool/procedure discrimination and static review, not live pipeline completion,
reliability, descendant reaping or authenticity against malicious evidence editing. VERIFY-01 remains
unchecked in REQUIREMENTS.md. The operator-owned live attempt and 49-12's final judgement remain.

## Task Commits

- 3ffe195: runbook main line.
- 0c51f71: deviations, fixes, outward effects, cleanup and host checklist.
- db15a1f: RED missing metadata contract.
- 6c935ed: repaired death/recollection instructions and producer metadata.

## Deviations from Plan

Independent review exposed a stale-gate dead-collection deadlock, null monitor PID mishandling,
missing watcher analysis and producer metadata, negative-unit cleanup omission and an undocumented
shipped-collection retry. These were verified in source and repaired within the documented procedure
and its prerequisite tool; no DevFlow Rust or process-model change was made. Manual runbook corrections
were recorded in the operator-required GSD incident log. The original requested gate sweep is bounded
to this project and explicitly described as potentially a no-op, not registry or stale-file cleanup.

Initial automatic approval review rejected external source export. Explicit standing AGENTS review
authorization, public-repository verification and a bounded archive excluding credentials/unrelated
planning data permitted the retry. No remaining approval rejection blocks this work.

## Next Readiness

49-11 is the next plan, one blocking-human operator task. It permits no files/commits/launches by
this agent before the live attempt is completed and its evidence gate passes. GitHub repository
creation/pushes/deletion and Define/Code responses are the operator's. No live positive attempt or
external repository was created here.

## Self-Check

Task-gate logs, exact review snapshots, real metadata checks, commit scope and pending VERIFY-01
were checked before this summary. The auth key remains outside Git; no key or fingerprint is recorded.
