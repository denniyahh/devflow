---
phase: 17-pipeline-dogfood-followup
plan: 08
subsystem: cli
tags: [rust, gate-protocol, preflight, regression-test, tdd]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup (17-05)
    provides: run_preflight/launch_stage (17c preflight readiness gate)
provides:
  - "run_preflight returns Result<bool, CliError> so its caller can tell 'preflight passed, continue' from 'a resolved gate already relaunched everything'"
  - "launch_stage's call site short-circuits on a resolved preflight gate, closing the CR-01 double-agent-spawn defect"
  - "two RED->GREEN regression tests covering the previously-untested Advance/LoopBack gate-resolution arms"
affects: [18-hermes-support]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Result<bool, T> return contract to disambiguate 'succeeded' from 'already handled by a nested retry' when a function can recursively re-run its own caller's remaining steps"
    - "ENV_MUTEX-guarded PATH stub (harmless no-op executable prepended to PATH) to let a test safely drive launch_stage() to full completion without ever risking a real agent CLI spawn"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - .planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md
    - .planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md

key-decisions:
  - "run_preflight's Advance/LoopBack/Abort match arms now ?-propagate instead of returning the nested call's Result directly, so a single `return Ok(false);` after the match is the only success path out of the failure branch"
  - "Regression tests inject a Cell<bool>-based FailOnceAdapter directly into run_preflight (not through launch_stage's internal adapter_for lookup, which is not test-injectable) and reproduce the call site's blind continuation with an explicit, now-gated, second launch_stage call"
  - "PATH is stubbed with a harmless no-op 'claude' executable under the existing ENV_MUTEX precedent (transition_resets_infra_failures) so the recursive/completing launch_stage call is 100% safe on hosts (like this one) that have real claude/codex/opencode CLIs on PATH"

patterns-established:
  - "When a helper function's failure-recovery path can recursively complete its own caller's remaining work, return an explicit continue/stop signal (bool or enum) rather than overloading Ok(()) — the caller's `?` cannot otherwise distinguish the two cases"

requirements-completed: [17c]

coverage:
  - id: D1
    description: "run_preflight signals its caller (via Ok(true)/Ok(false)) whether the caller should continue launch_stage's remaining steps, closing the double-agent-spawn defect (CR-01) on Advance-resolved preflight gates"
    requirement: "17c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#run_preflight_advance_gate_launches_agent_exactly_once"
        status: pass
    human_judgment: false
  - id: D2
    description: "Same fix verified for the LoopBack-resolved preflight gate arm (GateAction::LoopBack), which shares run_preflight's recursive-relaunch code path with Advance"
    requirement: "17c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#run_preflight_loopback_gate_launches_agent_exactly_once"
        status: pass
    human_judgment: false
  - id: D3
    description: "The pre-existing Abort arm's contract (never continue, never reach spawn_monitor) is still correct and is now explicitly pinned by asserting the returned bool is false"
    requirement: "17c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#run_preflight_failing_check_gates_and_never_reaches_spawn_monitor"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#run_preflight_adapter_hook_override_fires"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-07-19
status: complete
---

# Phase 17 Plan 08: Preflight gate resolution spawns the agent twice Summary

**Fixed CR-01 (run_preflight double-spawns the agent on a resolved Advance/LoopBack preflight gate) with a `Result<bool, CliError>` continue/stop signal, proven by two RED-then-GREEN regression tests, and closed GAP-1 in 17-VALIDATION.md — phase 17 is now `nyquist_compliant: true`.**

## Performance

- **Duration:** ~20 min
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- `run_preflight` now returns `Result<bool, CliError>` — `Ok(true)` = caller continues, `Ok(false)` = a failing check was already resolved via a full retried launch (Advance/LoopBack) or an abort, so `launch_stage`'s call site short-circuits and never spawns the agent a second time
- Two new regression tests (`run_preflight_advance_gate_launches_agent_exactly_once`, `run_preflight_loopback_gate_launches_agent_exactly_once`) cover the two gate-resolution arms that were previously only exercised via the Abort path — both confirmed RED (2 observed `stage_launched` events) against unmodified `main.rs` before the fix, and GREEN (1 event) after
- `17-VALIDATION.md` row 6 flipped from `⚠️ PARTIAL` to `✅ green`, GAP-1 marked RESOLVED with commit refs, `nyquist_compliant: true`, sign-off updated to PASS; GAP-2 (the pre-existing `concurrent_ship_advances_finish_both_phases_independently` race) left escalated and untouched
- `17-REVIEW.md` CR-01 annotated RESOLVED with fix/test commit refs

## Task Commits

Each task was committed atomically:

1. **Task 1: Cover both uncovered gate arms (RED)** - `b570114` (test)
2. **Task 2: Make the return type carry the continue/stop decision (GREEN)** - `c03498d` (fix)
3. **Task 3: Record the closure in the validation contract** - `ba83f28` (docs)

_No separate plan-metadata commit — Task 3's docs commit doubles as the plan-completion commit; this SUMMARY/STATE/ROADMAP update lands in the final metadata commit below._

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` - `run_preflight` signature change to `Result<bool, CliError>`, call site short-circuit in `launch_stage`, two new regression tests plus a `FailOnceAdapter` test double, `stub_agent_binary`/`prepend_path`/`stage_launched_count` test helpers, and updated assertions on the two pre-existing Abort-arm tests
- `.planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md` - row 6 flipped green, GAP-1 resolved, `nyquist_compliant: true`, sign-off/approval updated
- `.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` - CR-01 annotated RESOLVED

## Decisions Made
- **Adapter injection point:** `launch_stage()`'s internal adapter is always `agents::adapter_for(state.agent)` (not test-injectable, and its signature is intentionally unchanged by this plan). The regression tests therefore inject the `FailOnceAdapter` directly into `run_preflight` (matching the two pre-existing tests' pattern) and reproduce "the call site blindly continuing" with an explicit, separately-gated `launch_stage(&mut state, None, None)` call immediately after — this exercises the exact caller contract (`if !run_preflight(...)? { return Ok(()); }`) without needing to modify `launch_stage`'s signature.
- **Safe real-adapter completion via PATH stub:** this host (and potentially others) has real `claude`/`codex`/`opencode` binaries on `PATH`, and CI (`ubuntu-latest`) has none — either way, `launch_stage()` completing via the real, hardcoded adapter must never actually invoke a real agent CLI (existing test comments in this file explicitly call this out as forbidden). Both new tests stub a harmless no-op `claude` executable into a fresh tempdir and prepend it to `PATH` under the existing `ENV_MUTEX` (same precedent as `transition_resets_infra_failures`), so `ensure_agent_binary` and `monitor::spawn_monitor`'s backgrounded exec both resolve to the safe stub instead of a real CLI, deterministically, on any host.

## Deviations from Plan

None - plan executed exactly as written. The RED step (Task 1) failed as predicted (2 observed `stage_launched` events where 1 was expected), confirming the tests exercised the real bug before any fix landed.

## Issues Encountered

Working out a test design that (a) actually reproduces the double-spawn bug (a single direct call to `run_preflight` cannot, by itself, ever produce more than one `launch_stage` completion — the bug only manifests when `run_preflight` is called from *within* a `launch_stage` frame that keeps running after it returns) and (b) never risks spawning a real agent CLI (this codebase's existing tests are explicit that `launch_stage` completing must never happen with a real, unstubbed adapter) took substantial investigation of `launch_stage`, `run_preflight`, `agents::adapter_for`, `monitor::spawn_monitor`, and prior test precedent (`transition_resets_infra_failures`'s `ENV_MUTEX`/PATH-clearing pattern) before landing on the PATH-stub + explicit-second-call design described above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 17 is `nyquist_compliant: true` — all 9 validation rows green, no open Criticals. Ready to ship/merge.
- GAP-2 (`concurrent_ship_advances_finish_both_phases_independently`, a pre-existing race predating Phase 17) remains open and out of this phase's scope — belongs with future ship/version-bump concurrency work, not Phase 17 or 18.

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-19*

## Self-Check: PASSED

All files (`crates/devflow-cli/src/main.rs`, `17-VALIDATION.md`, `17-REVIEW.md`, this SUMMARY)
confirmed present on disk. All three task commits (`b570114`, `c03498d`, `ba83f28`) confirmed
present in `git log --oneline --all`.
