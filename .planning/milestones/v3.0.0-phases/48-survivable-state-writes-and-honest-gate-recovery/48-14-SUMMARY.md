---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: "14"
subsystem: recovery-and-doctor
tags: [rust, recovery, phase-lock, gates, doctor, e2e]
requires:
  - phase: 48-13
    provides: read-only HolderStatus classification
  - phase: 48-16
    provides: phase-aware no_waiter_repair CLI text and #200 wedge E2E fixture
provides:
  - lock-held explicit phase recovery that preserves every artifact on contention
  - doctor reconciliation for open gates without a possible waiter
  - lock-free #200 wedge recovery coverage
affects: [SURV-02, recover-clean, doctor, gate-recovery]
tech-stack:
  added: []
  patterns: [acquire-before-cleanup, read-only-holder-facts, verb-doctor-repair-parity]
key-files:
  created: [48-14-SUMMARY.md]
  modified:
    - crates/devflow-core/src/recover.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
key-decisions:
  - "A contended lock, including a recycled-PID record, is live for destructive recovery; only acquisition decides cleanup safety."
  - "Doctor reports the existing phase-aware recovery text without writing state, gates, or lock records."
requirements-completed: []
plan_head_before: 758e16a5c24ce67987c604146a4fe4d0964d5e9c
actuals:
  tokens: 4602
  tasks: 3
  commits: 1
duration: unmeasured
completed: 2026-09-17
status: complete
---

# Phase 48 Plan 14: Lock-held recovery and doctor repair parity Summary

**`recover --clean --phase N` now serializes all destructive cleanup with the phase lock, while doctor names the same recovery command for gates with no possible waiter.**

## Delivered

- `clean_phase` acquires before reading or deleting phase artifacts. A live or recycled-PID lock returns a warning and leaves state, gate request/response/ack files, gate temps, cron records, and the lock byte-identical.
- A successful recovery holds the guard through every stage's gate cleanup, state and cron cleanup, then drops it before stale-lock sweeping.
- `PhaseFacts` carries read-only `HolderStatus`; doctor emits one report-only problem for any open gate whose status is `NoHolder` or `Recycled` and uses `no_waiter_repair`.
- `check_orphan_gate` now yields to that no-waiter finding. A live waiter keeps the orphan-gate behavior.
- The #200 killed-advance wedge arm now runs `recover --clean` and proves state, all phase gate files, `.tmp` files, and the lock are absent.

## Verification

- Cargo RED evidence (no TAP fabricated): all three new exact recovery tests failed on their intended assertions before recovery behavior changed. The lock-free test found a remaining Validate gate; the live-lock test received only a stale-lock warning; the recycled-PID test also failed its contention expectation. Each executed one test and reported 810 filtered out.
- The four new exact doctor controls passed with one test each and 392 filtered out: NoHolder, Recycled, Ship repair, and Live non-finding. The direct empty/corrupt lock control passed with one test and 393 filtered out.
- Task 1 exact recovery controls passed with one test each and 810 filtered out; `gate_wedge_e2e` passed both arms.
- `cargo test -p devflow --bin devflow commands::tests::`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, and `git diff --check` passed.
- Final host `scripts/check.sh all` exited 0: it reported 394 CLI unit tests, 811 core unit tests, E2E suites including the two-arm wedge test, and `==> check.sh: all OK`.
- Final pinned-container command `timeout 45m scripts/check-in-container.sh` exited 0 after mounting the linked-worktree git directories, selecting `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`, pinning CPUs `0,1`, and ending with `==> check.sh: all OK`. The initial sandbox-only attempt exited 1 with `Docker daemon is not reachable`; the final host-Docker run is the container-parity result.
- Independent orchestrator recheck after resume: `scripts/check.sh all` exited 0 with 33 of 33 `test result:` lines successful and `==> check.sh: all OK`; the bounded pinned-container command exited 0 again on `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` with the same final marker.

The passing tests establish the exercised lock-free, live-lock, recycled-PID, no-waiter, and read-only-lock-record cases. They do not establish production timing reliability, a live waiter's lifetime beyond the observed lock state, or external deployment infrastructure behavior.

## Repair Assertion Ledger

All repair-string assertions changed by this plan:

| Test | Old text | New text |
| --- | --- | --- |
| `reconcile_phase_flags_orphan_open_gate` | `devflow gate approve 3 --stage validate` | `run \`devflow resume --phase 3\` or \`devflow recover --clean --phase 3\` to establish recovery` |

The Ship control separately pins that the shared repair text names `devflow ship --phase 14`.

## TDD Notes

Task 1 had behavior-level RED failures before its cleanup implementation. Task 2 required the non-behavioral `PhaseFacts.waiter` data plumbing before a pure reconciliation test could construct the status; its three no-waiter controls then failed on the old `devflow gate approve` repair before the new check was registered. Phase 48's Cargo-output parser exception applies; no TAP or synthetic `tdd-red-evidence` record was created.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Preserve lock and gate cleanup errors in `RecoverError`**
- **Found during:** Task 1 GREEN compile.
- **Issue:** `LockError` and `GateError` could not be propagated by the existing recovery error type.
- **Fix:** Added typed `RecoverError::Lock` and `RecoverError::Gate` variants.
- **Files modified:** `crates/devflow-core/src/recover.rs`.
- **Verification:** Exact recovery controls, workspace clippy, host suite, and pinned-container suite passed.

**2. [Rule 2 - Verification] Prove doctor leaves empty and corrupt lock records intact**
- **Found during:** Task 2 review of the read-only HolderStatus requirement.
- **Issue:** Existing doctor read-only coverage did not exercise the new lock-status path with unreadable records.
- **Fix:** Added a `collect_phase_facts` control for empty and corrupt lock records.
- **Files modified:** `crates/devflow-cli/src/commands.rs`.
- **Verification:** Exact control plus final host and container suites passed.

## Known Stubs

None. The owned source and test files contain no plan-blocking placeholder or empty-data-flow stub.

## Scope and Handoff

Only the three source/test files above and this summary were changed. `scripts/check-in-container.sh` was executed but not modified. `.planning/research/.cache/` and `.planning/user/errors/` remain untouched and untracked.

Per the approved #4799 linked-worktree constraint, the executor attempted no task, metadata, state, roadmap, or requirement commit. After independent verification, the orchestrator made the one scoped normal-hook source commit. `SURV-02` is deliberately not marked complete here; phase-level verification owns requirement completion.

## Self-Check: PASSED

- All three owned source/test files and this summary exist.
- The scoped normal-hook commit contains exactly the three owned source/test files plus this summary; the two explicitly excluded untracked planning directories remain untouched.
- The executor made no task commits; the orchestrator made the verified normal-hook commit under #4799.
