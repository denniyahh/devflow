---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 10
subsystem: workflow-state-and-gates
tags: [rust, filesystem, atomic-write, hard-link, concurrency]
requires:
  - phase: 48-09
    provides: phase-local execution context and test isolation groundwork
provides:
  - collision-retrying, collectable temporary files for state and gate writes
  - first-answer-wins gate response publication via hard links
  - phase- and stage-specific orphan temporary-file cleanup
affects: [48-11, 48-12, state-persistence, gate-recovery]
actuals:
  tokens: 4757
  tasks: 3
  commits: 1
plan_head_before: 01f2d440ec4d8af7b7b1416cd898ed61b2c32033
tech-stack:
  added: []
  patterns:
    - File::create_new with a process-id and monotonic-sequence sibling name
    - hard_link for exclusive response publication
key-files:
  created: []
  modified:
    - crates/devflow-core/src/workflow.rs
    - crates/devflow-core/src/gates.rs
key-decisions:
  - "Unique temporary names use .{target}.{pid}.{seq}.tmp and retry only AlreadyExists collisions."
  - "Gate responses hard-link a completed temporary file and return AlreadyResponded on the losing publisher."
requirements-completed: [SURV-01, SURV-02]
coverage:
  - id: D1
    description: State writes use collision-retrying unique temps and clear only their phase's orphan temps.
    requirement: SURV-01
    verification:
      - kind: unit
        ref: crates/devflow-core/src/workflow.rs#state_write_leaves_no_temp_and_list_states_ignores_a_planted_temp
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/workflow.rs#clear_state_sweeps_orphan_temps_for_its_phase_only
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/workflow.rs#unique_temp_creation_retries_after_a_same_pid_orphan
        status: pass
    human_judgment: false
  - id: D2
    description: Gate responses publish exclusively while request and acknowledgement writes keep overwrite semantics.
    requirement: SURV-02
    verification:
      - kind: unit
        ref: crates/devflow-core/src/gates.rs#second_publisher_past_the_existence_check_gets_already_responded
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/gates.rs#single_responder_still_publishes
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/gates.rs#write_gate_still_overwrites_a_leftover_request
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/gates.rs#cleanup_removes_orphan_temps_for_its_gate_only
        status: pass
    human_judgment: false
duration: unmeasured
completed: 2026-09-16
status: complete
---

# Phase 48 Plan 10: Collision-Proof State and Gate Writes Summary

**State and gate writes now use collision-retrying sibling temps, while gate responses publish first-answer-wins through a hard link and cleanup collects only matching orphan files.**

## Performance

- **Duration:** Unmeasured; this executor instance did not persist its start timestamp.
- **Completed:** 2026-09-16T23:42:49Z
- **Tasks:** 3/3
- **Files modified:** 2

## Accomplishments

- Added `.{target}.{pid}.{seq}.tmp` creation with `File::create_new` and retry-on-`AlreadyExists`; state rename failures remove the selected temp.
- Made gate response publication exclusive: the completed temp is hard-linked to the response path, preserving the first response and reporting `AlreadyResponded` to a second publisher.
- Swept matching state and gate orphan temps without touching a sibling phase or stage, while preserving request/ack rename-overwrite behavior.

## TDD Gate Compliance

- **Task 1 RED:** Cargo exited 101 before implementation because the new direct helper test referenced the absent `create_unique_temp`; the same initial compile also exposed a test-only invalid `State` equality assertion, corrected before GREEN. This is truthful Cargo RED evidence but not TAP-shaped `tdd-red-evidence` output; the operator-approved Phase 48 exception applies. No TAP output was fabricated.
- **Task 2 RED:** Cargo exited 101 before implementation because the direct first-writer test referenced the absent `publish_response_exclusive` helper. This is truthful Cargo RED evidence, not an assertion-form TAP record; the approved exception applies.
- **Task 3 RED:** `cleanup_removes_orphan_temps_for_its_gate_only` failed its own `code_orphans.iter().all(|path| !path.exists())` assertion with exit 101 before cleanup implementation.
- **GREEN:** All named behavior tests passed with exactly one executed test each and nonzero filtered-test controls; the final core library run passed 800 tests and workspace Clippy passed with `-D warnings`.
- **Commits:** The user-approved Phase-wide upstream GSD #4799 workaround records this verified source scope and summary through one orchestrator-managed normal-hook commit. The executor therefore did not create per-task RED/GREEN commits.

## Files Modified

- `crates/devflow-core/src/workflow.rs` — unique-temp generation, retrying exclusive creation, failed-rename cleanup, state-temp sweeping, and state tests.
- `crates/devflow-core/src/gates.rs` — exclusive response publisher, shared retrying gate writes, gate-temp sweeping, and gate protocol tests.

## Decisions Made

- Kept distinct per-module atomic counters while sharing one path shape and the same retrying creation helper; this avoids a cross-module global dependency while preserving collision safety.
- Kept `write_gate` and `ack` rename-overwrite semantics, reserving hard-link exclusivity for the only competing-writer path: responses.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Preserved the state target path after deletion before scanning its orphan prefix.**
- **Found during:** Task 1 verification.
- **Issue:** Passing the `PathBuf` by value to `remove_file` moved it before the required orphan-prefix scan.
- **Fix:** Removed by reference.
- **Files modified:** `crates/devflow-core/src/workflow.rs`
- **Verification:** Core library tests and workspace Clippy pass.

**2. [Rule 1 - Bug] Collapsed the new state cleanup branch required by Clippy.**
- **Found during:** Task 3 verification.
- **Issue:** `clippy::collapsible_if` failed the required `-D warnings` gate.
- **Fix:** Used the idiomatic let-chain form without changing cleanup behavior.
- **Files modified:** `crates/devflow-core/src/workflow.rs`
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` exits 0.

**Total deviations:** 2 Rule-1 fixes. Both were directly caused by this plan's changes; no scope expansion occurred.

## Verification

- Task 1 gate: three named workflow tests each reported `1 passed` with filtered-test controls; source negative control reported `write_state_fixed_tmp=0 write_state_retry_helper=1`.
- Task 2 gate: five named gate tests each reported `1 passed` with filtered-test controls; source negative control reported `gates_fixed_tmp=0 gates_hard_link=1 gates_retry_helper=2`.
- Task 3 gate: `cleanup_one_passed=1`; `cargo test -p devflow-core --lib` reported `800 passed`; workspace Clippy exited 0; all gates reported `gate_rc=0`.

These tests prove the named state/gate file behaviors on the local filesystem. They do not establish behavior on a filesystem that lacks hard-link support; that deliberately returns `GateError::Io` rather than falling back to overwrite.

## Known Stubs

None. The changed lines' stub-pattern scan found no placeholders, TODO/FIXME markers, or empty UI-facing values.

## Next Phase Readiness

Plans 48-11 and 48-12 can rely on collision-safe state and gate file primitives. The verified changes are recorded through the approved orchestrator normal-hook commit.

## Self-Check: PASSED

- Both modified Rust files exist in the worktree.
- The summary exists at the required plan path.
- The independently verified source scope and this summary are recorded through the approved #4799 orchestrator normal-hook commit.
