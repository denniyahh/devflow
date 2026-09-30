---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 09
subsystem: testing
tags: [bash, python, systemd, authentication, fixture]
requires:
  - phase: 49-06
    provides: Complete payload and prelaunch check
  - phase: 49-07
    provides: Negative arm
provides:
  - Attempt preparation, immutable binary copy and explicit build identity
  - Names-only transient-unit Claude authentication probe
  - Private-repository/pushed-base/scope checks before positive unit launch
  - Read-only unit status and three-stop cap controls
affects: [49-10, 49-11, 49-12]
actuals:
  tasks: 2
  commits: 7
plan_head_before: b8a6f95
tech-stack:
  added: []
  patterns: [Shared three-variable unit environment, raw auth output confined to scratch, identity-pinned launcher]
key-files:
  created: [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh]
  modified: []
key-decisions:
  - Operator authorized the existing Claude API key and saved it outside Git in an owner-only file.
  - Claude user apiKeyHelper reads that file; only the previous empty API-key env entry was removed.
  - Fork base/main SHAs are read again after authentication; attempt.json carries build_commit.
requirements-completed: [VERIFY-01]
coverage:
  - id: D1
    description: Prepare pins a real build, writes payload and records the attempt
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: launch-attempt.sh --selftest#prepare
        status: pass
      - kind: integration
        ref: 49-09 Task 1 real prepare gate on e49ec61
        status: pass
    human_judgment: false
  - id: D2
    description: Agent authentication is measured in the unit environment without copying secrets
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: launch-attempt.sh --selftest#probe-auth and failure controls
        status: pass
      - kind: integration
        ref: 49-09 Task 1 real auth probe auth_source=apiKeyHelper
        status: pass
    human_judgment: false
  - id: D3
    description: Start refuses invalid repo/auth/attempt inputs and records the post-auth fork
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: launch-attempt.sh --selftest (17 cases)
        status: pass
      - kind: other
        ref: 49-09 Task 2 gate
        status: pass
    human_judgment: false
completed: 2026-09-30
status: complete
---

# Phase 49 Plan 09: Attempt Launcher Summary

The launcher prepares a pinned disposable payload, probes authentication in the run's systemd
environment, and checks repository identity, privacy, push state and scope before launching.

## Verification

Both task gates passed. Final real Task-1 gate on e49ec61: prepare exit 0, hash match, mode 0555,
payload CHECK passed=20 failed=0, current attempt 1; auth probe exit 0, is_error=false,
reply_ok=yes, auth_source=apiKeyHelper. It recorded 107 variable names, no NAME=value lines,
all three setenv names, scratch_left=0 and session_names=0 with caller_claude_names=1.
CLAUDECODE was deliberately planted only in the caller to make that isolation check non-vacuous.
Task-2 gate passed all required controls with forbidden_lines=0; the full selftest has 17 passes.
Bash syntax, ShellCheck and pinned-container workspace/all-target Clippy passed.

The key file was independently checked nonempty, single-line, owner-only 0600 with directory 0700.
The helper reads the expected bytes; a missing-file control fails without output. The key is never
printed, copied into this repository, placed in command arguments or copied into attempt evidence.
All other Claude user settings were preserved; a private pre-change backup exists outside Git.

This establishes authentication for a bounded probe at that time, not the whole run, account/billing
ownership, or live pipeline reliability. auth_source identifies the mechanism, not which provider
account owns the key. No positive live start or external repository was created by these gates.
VERIFY-01 still requires the operator-owned live attempt and final evidence verification.

## Task Commits

- 91fc3b9 RED tracer; cba140c implementation; c568940 refusal/auth controls.
- 623b280 RED moved-base regression; 59be29f post-auth fork capture fix.
- 65ad326 RED build_commit identity assertion; e49ec61 attempt identity fix.

## Deviations from Plan

The real probe initially failed: configured key names existed but held empty strings. The plan was
left incomplete, and the operator explicitly chose an API key, saved it privately and authorized
its use. Wiring the supported Claude user apiKeyHelper and removing the empty settings env key
resolved the real auth gate; no credential value entered the worktree. User-level helper configuration
also applies to other Claude CLI sessions under this account.

An added mutation control exposed cached pre-auth fork SHAs. A cross-plan check exposed missing
build_commit in attempt.json, which 49-10 uses to verify a fix reaches the pinned binary. Both were
fixed with failing/passing directions. Driver controls already implemented by the tracer did not
receive an artificial failing implementation or unnecessary extra feature commit.

## Issues Encountered

The historical auth failure and tracking-write incident are preserved in 49-09-AUTH-BLOCKER.md.
The STATE blocker insertion was corrected surgically and logged in the required external incident log.
The auth probe's real gate was extended to retain only safe derived/name diagnostics before deleting
its temporary home. Process-local session-bus variables selected the existing UID-1000 manager.

## Next Readiness

49-10 writes the operator runbook. 49-11 remains the human-owned live attempt; no approval is inferred
from this tool check. Historical auth handoff is consumed; the real auth blocker is resolved.
