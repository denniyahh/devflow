---
phase: 47-unattended-decision-policy-consistency
plan: "07"
subsystem: shell-test-hermeticity
tags: [git-config, hooks, commit-signing, regression-test]
requires:
  - phase: 47-06
    provides: completed gap-wave validation boundary
provides:
  - A worktree-guard fixture isolated from inherited hooks, signing, and injected git config
  - A host-independent mutation control for fixture isolation
affects: [phase-worktree-guard, developer-setup]
tech-stack:
  added: []
  patterns: [fixture-local git isolation, hostile-config controls, mutation testing]
key-files:
  created: []
  modified:
    - scripts/test-phase-worktree-guard.sh
    - .planning/user/DEV-SETUP-CHECKLIST.md
key-decisions:
  - "Fixture isolation is repository-local; the guard under test still receives its normal environment."
  - "Both environment carriers that outrank repository config are cleared at the script boundary."
actuals:
  tasks: 2
  commits: 2
commits: 2
plan_head_before: 1691b12
requirements-completed: []
completed: 2026-09-12
status: complete
---

# Phase 47 Plan 07: Worktree-Guard Fixture Isolation Summary

**Made the phase-worktree guard harness reliable under hostile global hooks, SSH signing, and environment-injected git configuration.**

## Accomplishments

- Clears `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_COUNT`, which otherwise outrank repository-local fixture settings.
- Isolates the fixture repository with an empty hooks path and disabled signing before its initial commit.
- Adds hostile hook/signing controls 7a/7b plus host-independent fixture-bootstrap control 7c.
- Updates the setup checklist to describe the 10 named script cases.

## Task Commits

1. `61d85d8` — fixture isolation and injected-config clearing.
2. `b331719` — permanent hostile-config regression case and checklist.

## Verification

- RED gate on the original script returned `gate_rc=1`: hostile, host-without-agent, count-injected, and parameters-injected runs aborted before a summary; only the hermetic run reported `passed=10 failed=0`.
- Task 1 GREEN gate returned `gate_rc=0`: all five runs reported `passed=10 failed=0`, with six direct guard calls and one each of the hooks, signing, and environment-carrier isolation lines.
- Final post-commit gate returned `gate_rc=0`: all five runs reported `passed=13 failed=0`; `case7_ok_lines=3`; `guard_invocations=6`; no untracked script files; and the checklist counters were `10 cases=1`, `7 cases=0`.
- The mutation control removed exactly two repository-local isolation lines and failed in the required direction: `mutant_exit=1`, `mutant_summary=passed=12 failed=1`, and `mutant_fail_7c=1`.
- `bash -n scripts/test-phase-worktree-guard.sh` passed and the index mode remains `100755`.

These checks establish the harness behavior under the five supplied configuration states and prove the two-line mutation is detected. They do not establish behavior under arbitrary git wrappers, `core.fsmonitor`, CI runner configuration, or hosts without the hostile-config controls.

## Deviations from Plan

None.

## Self-Check: PASSED

- Both task commits exist.
- The permanent script runs with all 13 checks passing under every gate-supplied environment.
- The negative mutation produces a named failure instead of an undifferentiated non-zero exit.

