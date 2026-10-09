---
status: testing
phase: 49-live-unattended-run-the-milestone-s-instrument
source: [49-VERIFICATION.md]
started: 2026-10-09T01:25:32Z
updated: 2026-10-09T01:25:32Z
---

## Current Test

number: 1
name: Decide WR-03 (Ship prompt hard-codes `main` and the fixture branch name `integration`) before merge-back
expected: |
  Operator chooses: fix (interpolate config::MAIN and the resolved base into ship_stage_prompt / workflow_ship_prompt) before the merge-back, or explicitly defer with a backlog entry. 49-REVIEW-DISPOSITION.md records WR-03 as `open`.
awaiting: user response

## Tests

### 1. Decide WR-03 (Ship prompt hard-codes `main` and the fixture branch name `integration`) before merge-back
expected: Operator chooses fix-before-merge-back or explicit deferral with a backlog entry; until fixed, criterion 2's PR-to-main observation measures the agent obeying a literal, not DevFlow resolving a PR base (the local merge into `integration` is a genuine DevFlow observation).
result: [pending]

### 2. Correct or annotate 49-LIVE-RUN.md lines 15-16, 240 and 249-251 (fix commits now on feature/phase-49)
expected: Since bbfe6df both e3f9ddd and 084625c are ancestors of HEAD; only code delta from 084625c is rustfmt commit 5574841. The record says so; because 49-LIVE-RUN.md is a covered file, the edit stales 49-VERIFICATION.md and requires re-verification.
result: [pending]

### 3. Rule on the judgment-tier prohibitions (49-03, 49-04/49-12, 49-09, 49-10, 49-11)
expected: Operator accepts or rejects each verdict in the report's Prohibitions table (no violations found), and rules on the wording conflict: 49-09 "No Phase 49 tool answers a DevFlow gate" vs negctl.sh:249 `gate reject` on the negative arm's own Define gate as 49-07 requires.
result: [pending]

### 4. Rule on attempt 2's quota pause + resume (unattended? counts toward D-11 cap?)
expected: Operator rules or explicitly drops it. Does not gate VERIFY-01 (attempt 3 has 0 rate-limit/relaunch/advance_failed/abort events; the same search finds 3 in attempt 2).
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
