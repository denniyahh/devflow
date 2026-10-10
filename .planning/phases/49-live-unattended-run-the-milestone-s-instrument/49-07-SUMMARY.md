---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 07
subsystem: testing
tags: [bash, python, systemd, git, evidence, negative-control]
requires:
  - phase: 49-02
    provides: Default scratch scaffold
  - phase: 49-03
    provides: Gate snapshot hook and evidence layout
provides:
  - Negative-arm driver with binary identity checks, signing isolation and bounded unit cleanup
  - Five named negative-arm evidence checks with absent-input and relocation controls
  - One real-binary proof of Define refusal, notify-hook capture and reject-to-abort
affects: [49-08, 49-09, 49-10]
actuals:
  tasks: 3
  commits: 3
plan_head_before: 7dc8db0
tech-stack:
  added: []
  patterns: [Named text-and-field checks, ledger-relative snapshot resolution, command-local signing overrides]
key-files:
  created:
    - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/negctl.sh
    - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/check-evidence.sh
  modified: []
key-decisions:
  - Refuse an already active named unit before installing the cleanup trap.
  - Checker uses Python standard-library JSON parsing with indexed required fields; shell remains the public entrypoint.
requirements-completed: [VERIFY-01]
coverage:
  - id: D1
    description: Negative arm drives refusal, snapshot and abort through a bounded owned unit
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: tools/negctl.sh --selftest (8 cases)
        status: pass
      - kind: integration
        ref: 49-07 Task 3 real-binary gate on 2026-09-30
        status: pass
    human_judgment: false
  - id: D2
    description: Checker judges negative evidence by exact refusal text, base branch, fork and event order
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: tools/check-evidence.sh --selftest (8 cases)
        status: pass
      - kind: integration
        ref: 49-07 Task 3 real output CHECK passed=5 failed=0
        status: pass
    human_judgment: false
completed: 2026-09-30
status: complete
---

# Phase 49 Plan 07: Negative Arm Summary

A single driver now scaffolds the unconfigured control, captures its Define gate, aborts it,
and checks the recorded refusal, branch, fork and event order.

## Accomplishments

- Added executable-binary/hash validation, command-local scaffold signing overrides, repo-local signing off,
  bounded gate/exit waits and stop/reset of exactly the owned transient unit.
- Added five evidence checks. Snapshot resolution stays under negctl/gates after relocation; required
  JSON keys are indexed and missing files fail with ABSENT.
- Proved the driver and checker once against the real DevFlow binary with the existing user manager.

## Task Commits

- `4bbb89d` test: actual RED `  FAIL negctl: fake arm end to end passes the negative-arm check`; checker 0 passed, 8 failed.
- `53c1888` feat: fake end-to-end driver passes and all eight checker cases discriminate.
- `aa24e73` test: seven driver controls pin signing, identity, fixture-only, environment and cleanup behavior.

## Verification

All three task gates printed `gate_rc=0` on 2026-09-30. Driver selftest: 8 passed, 0 failed.
Checker selftest: 8 passed, 0 failed. Bash syntax and ShellCheck passed (structured JSON diagnostics empty).
The non-comment source scan found zero external repository/remote/push write commands in either tool.

The real gate reported user_manager=degraded (allowed by the stated precondition), negctl_exit=0,
CHECK passed=5 failed=0, a real 001-define snapshot, one Define gate event and one workflow_aborted,
unit_state_after=inactive, registry_entries_naming_run=0. A separate systemctl list-units check
returned no p49-plan07-negctl unit after cleanup.

The checker read start.stdout and required this exact line:

>   [DOES NOT HOLD] GSD config can hold the chain flag — no `.planning/config.json` under the launch root — nowhere for the chain flag to live

The checker independently required the recorded develop SHA to equal the snapshot worktree head,
base_branch to be develop, no base-branch note, and workflow_aborted after the last gate_fired.
The gate's trap removed the temporary evidence after verification; the console proof remains in
/tmp/49-07-real.log for this session and is not a durable evidence archive.

This proves these tools against this binary once. It does not prove a positive live run, agent
behavior, Ship completion or reliability. D-12 requires the negative arm again for every attempt's
pinned binary. VERIFY-01 remains a phase-level live-evidence obligation.

## Deviations from Plan

Checker controls were authored with the initial RED fixture rather than split into a later test commit.
The driver's Task-2 controls passed immediately because Task 1 had already implemented the required
refusals and cleanup; no artificial production mutation or unnecessary feature commit was added.
The planned grep source-count pipeline was translated to rg under the operator rule; rg's no-match
output was normalized to zero only after successful source reads, preserving the intended count.
Systemd commands used process-local XDG_RUNTIME_DIR and DBUS_SESSION_BUS_ADDRESS for the existing
UID-1000 bus because the agent shell omitted them. No host configuration was changed.
Execution was inline under the Codex skill fallback.

## Issues Encountered

The first rg-adapted Task-1 source-count gate failed because no matches produces empty output;
the corrected counter reran and passed. This was a probe issue, not a tool failure.

## Next Readiness

Wave 2 is complete. Next plans: 49-08 (positive evidence/pair/attempt checker) and 49-09
(attempt preparation). The operator-owned positive live attempt remains plan 49-11.

## Self-Check

Both tools and all three task commits exist. Task-gate results and independent unit cleanup were
checked before this summary was authored.
