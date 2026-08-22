---
phase: 16-pipeline-reliability-hardening
plan: 03
subsystem: pipeline-reliability
tags: [external-verification, completion-signals, capture-history, forensics]

requires:
  - phase: 16-01
    provides: terminal Ship merge correctness and trustworthy hook telemetry
  - phase: 16-02
    provides: typed config resolvers for capture retention and external verification
provides:
  - PLAN.md-declared external post-condition verification that outranks agent self-report on failure
  - bounded per-phase archives of completed-stage stdout and exit captures
affects: [16-05, 16-06, 16-07, completion-evaluation, operator-forensics]

tech-stack:
  added: []
  patterns:
    - PLAN frontmatter plus exact operator-approved command bytes as the shell-command trust boundary
    - fail-closed Layer 0 before agent-controlled completion signals
    - bounded timestamped capture generations under .devflow/history

key-files:
  created:
    - crates/devflow-core/src/verify.rs
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "external_verify is a quoted scalar in PLAN.md YAML frontmatter; declarations outside frontmatter and strings in agent stdout are ignored."
  - "A failed declared probe returns Failed before Layer 1; successful probes defer to the existing Layer 1/2/3 evidence cascade."
  - "Capture generations use {unix-nanoseconds}-{process-sequence}-{stdout|exit}; the built-in retention default is 5."

patterns-established:
  - "External-only work declares external_verify in PLAN frontmatter; execution requires an exact matching DEVFLOW_TRUST_EXTERNAL_VERIFY JSON command array."
  - "Runtime captures rotate to .devflow/history/phase-NN/ before the next stage starts."

requirements-completed: [16a, 16b]

coverage:
  - id: D1
    description: "Declared external verification failures outrank an agent's DEVFLOW_RESULT success marker, while plans without declarations preserve existing evaluation."
    requirement: 16a
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#failing_external_probe_outranks_success_marker"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#no_external_declaration_preserves_layer1_result"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/verify.rs#reads_external_verify_only_from_plan_frontmatter"
        status: pass
    human_judgment: false
  - id: D2
    description: "Prior-stage stdout and exit captures are archived into bounded per-phase history instead of being deleted."
    requirement: 16b
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#archive_moves_captures_into_history_and_removes_pid_file"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#archive_prunes_history_to_retain_count"
        status: pass
      - kind: other
        ref: "cargo build -p devflow"
        status: pass
    human_judgment: false

duration: 4min
completed: 2026-07-18
status: complete
---

# Phase 16 Plan 03: Trustworthy Completion Signals Summary

**Operator-authored Layer-0 probes now fail external-only work closed, while bounded capture archives preserve the evidence needed to diagnose false-positive self-reports.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-07-18T00:52:59Z
- **Completed:** 2026-07-18T00:56:13Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Added `external_verify: "<shell command>"` PLAN-frontmatter discovery with an explicit PLAN-only trust boundary.
- Prepended external verification failure as Layer 0 ahead of agent self-report, without changing ordinary-plan behavior.
- Post-review hardening binds approval to exact command bytes, runs probes only after Code from the execution worktree, and fails closed on changed or removed declarations.
- Replaced destructive capture cleanup with retention-configured archive rotation under `.devflow/history/phase-NN/`.

## Task Commits

1. **Task 1: Retain per-stage capture history** - `bd4c98d` (feat)
2. **Task 2 RED: External verification contract tests** - `3e2cdce` (test)
3. **Task 2 GREEN: Authoritative external verification layer** - `308fb38` (feat)

## Files Created/Modified

- `crates/devflow-core/src/verify.rs` - Discovers trusted PLAN declarations and executes external probes.
- `crates/devflow-core/src/agent_result.rs` - Adds Layer 0 and bounded capture archival.
- `crates/devflow-core/src/lib.rs` - Exposes the verification module.
- `crates/devflow-cli/src/main.rs` - Rotates captures using configured retention before monitor launches.
- `crates/devflow-cli/tests/phase7_cli.rs` - Keeps rollover documentation aligned with archive semantics.

## Decisions Made

- Probe success defers to normal completion evidence; only probe failure is independently authoritative.
- Double-quoted PLAN scalars decode through JSON string rules; single-quoted and unquoted scalar forms are also accepted.
- Capture archives are paired by a nanosecond timestamp plus process-local sequence, and pruning operates on generation pairs.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Resumed interleaved Task 1 implementation and tests without reconstructing a destructive RED state**
- **Found during:** Task 1 (Retain per-stage capture history)
- **Issue:** The worktree already contained an uncommitted partial implementation with its tests interleaved, so producing a genuine RED-only commit would have required temporarily discarding or rewriting retained work.
- **Fix:** Preserved the partial work, ran every task-level acceptance check, and committed the verified outcome atomically. Task 2 followed the full RED→GREEN sequence.
- **Files modified:** `crates/devflow-core/src/agent_result.rs`, `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/tests/phase7_cli.rs`
- **Verification:** 51 focused agent-result tests and `cargo build -p devflow` passed before commit.
- **Committed in:** `bd4c98d`

---

**Total deviations:** 1 auto-fixed (1 blocking resume-state adaptation).
**Impact on plan:** No behavioral scope changed; all acceptance criteria passed and strict MVP+TDD enforcement was inactive.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 16-05 can enumerate `history_dir()` in the runtime-file gitignore invariant.
- Plan 16-07 can correlate timestamped capture generations with structured events.
- No blockers remain from Plan 16-03.

## Self-Check: PASSED

---
*Phase: 16-pipeline-reliability-hardening*
*Completed: 2026-07-18*
