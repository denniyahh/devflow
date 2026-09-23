---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 20
subsystem: workflow launch safety
tags: [resume, liveness, lock, tdd, e2e]
requires:
  - phase: 48-19
    provides: start live-run refusal and liveness contract
provides:
  - resume refuses a second launch over a recorded live monitor or agent
  - resume preserves phase state and pid files when it refuses
affects: [SURV-01, phase-verification, resume]
actuals:
  tokens: 5500
  tasks: 2
  commits: 3
tech-stack:
  added: []
  patterns: [shared launch liveness guard, RED-GREEN mutation control]
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/start_lock_e2e.rs
    - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
key-decisions:
  - "Keep H1-H10 production hint text unchanged; backlog 999.142 owns that fix shape."
  - "Defer resume recycled-pid identity cost to backlog 999.143; fail closed with explicit repair cost."
requirements-completed: [SURV-01]
coverage:
  - id: D1
    description: Resume refuses a live recorded monitor or agent under the phase lock without writes.
    requirement: SURV-01
    verification:
      - kind: e2e
        ref: crates/devflow-cli/tests/start_lock_e2e.rs#resume_refuses_a_phase_whose_recorded_monitor_is_alive
        status: pass
      - kind: e2e
        ref: crates/devflow-cli/tests/start_lock_e2e.rs#resume_refuses_a_stopped_phase_whose_recorded_agent_is_alive
        status: pass
    human_judgment: false
  - id: D2
    description: Resume retains valid recovery behavior and start text after sharing the guard.
    requirement: SURV-01
    verification:
      - kind: integration
        ref: Task 2 GREEN gate
        status: pass
    human_judgment: false
duration: 16min
completed: 2026-09-23
status: complete
---

# Phase 48 Plan 20: Resume Live-Run Refusal Summary

**`devflow resume` now refuses a live recorded monitor or agent under its phase lock, preserving the existing start refusal byte-for-byte.**

## Performance

- **Duration:** 16 min
- **Started:** 2026-09-23T22:31:00Z
- **Completed:** 2026-09-23T22:46:02Z
- **Tasks:** 2/2
- **Files modified:** 4

## Accomplishments

- Added failing-first end-to-end monitor and stopped-agent refusal arms, dead-process controls, a no-state carve-out, and a start-text golden control.
- Added a shared `LaunchVerb` liveness guard and invoked its resume arm after `lock::acquire` and before `load_state`.
- Made the SIGKILL auto-chain fixture wait until its orphaned sleeper is known before resuming.

## Task Commits

1. **Task 1: RED tests** — `97a1c94` (`test(48-20): cover resume live-run refusal`)
2. **Task 2: GREEN guard** — `fa04bfe` (`feat(48-20): refuse resume over live run`)

## Verification

### Task 1 RED gate

- Both refusal arms exited 101 with exactly one failing test result and their named reasons: `resume must refuse a phase whose recorded monitor is alive` and `resume must refuse a phase whose recorded agent is alive`.
- The dead-leftover control, start golden, no-state carve-out, and auto-chain SIGKILL control each passed with one selected test; clippy, formatting, and the production-untouched control passed.
- The mutation control independently removed only the GREEN call. Both arms then failed again and printed `stage code → launched Claude Code` with a new monitor pid; restoring the exact call made both arms pass.

### Task 2 GREEN gate

- `start_lock_e2e`: 12 passed. Eight regression e2e targets and the bin suite passed.
- Four named resume unit tests passed; `clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` passed.
- Placement probe: resume lock line 7, guard line 16, load line 17; one guard call. Start's existing guard remained lock line 55, guard line 65, cleanup line 73; one call.
- Production-caller control: `production_resume_calls=1 from_main_rs=1`.

## Self-refusal result

`pipeline_launch::resume` has one production caller, the `devflow resume` CLI arm in `main.rs`. No monitor or advance path calls it, so no self- or parent-pid exclusion was added: a live agent that invokes resume for its own phase must be refused to prevent a second writer.

## Operator decisions (2026-09-23)

- H1-H10 production hints that prescribe `devflow resume` remain unchanged here (option b); their follow-up is backlog 999.142.
- The recycled-pid cost on resume is deferred to backlog 999.143, not accepted as a permanent limit. `recover --clean` followed by `start --force` discards the phase work.

## Known limits

- Recycled monitor or agent pids can cause a false refusal; pid identity is deferred with D-03 / Phase 50 (999.143), and the costly repair is described truthfully.
- H1-H10 hint-site behavior is reasoned from source, not exercised. The monitor and agent arms cover their PID shapes, not `status`, doctor, cleanup, gate verbs, stop, or the cron paths.
- IN-05 remains: SIGTERM to the pipe-owning monitor can leave its agent orphaned; the advice is not end-to-end tested.
- 999.139 remains: an unloadable state checks only the agent pid file. Resume subsequently fails its own state load and launches nothing; this branch is untested.
- 999.136 and 999.138 remain unchanged; the stop-marked live-agent arm only proves refusal.
- A Hermes cron firing while its writer monitor remains live would be refused once; this is reasoned, not tested.
- “Nothing was written” excludes the transient lock file and possible legacy-state migration, as in 48-19.
- Pre-existing 999.141 remains for regression files; this plan's new resume helper uses a per-child cache and reaps monitors it launches.

## Deviations from Plan

None. The first RED run found an incomplete hand-rendered golden: it omitted the dynamic two-space `ps` check line. The test-only constant was corrected against the unchanged start output before the RED commit.

## Issues Encountered

One first-pass GREEN shell transcription did not preserve the plan gate's zero-count and escaped-`awk` parsing. The corrected, plan-equivalent gate passed. A one-off `stop_e2e` failure in that malformed run passed directly and in the corrected full gate; no product change followed from it.

## User Setup Required

None.

## Next Phase Readiness

All 20 Phase 48 plans now have summaries. The pinned container gate passed with exit 0 and one normalized `==> check.sh: all OK` marker; the phase verifier remains pending. This plan's tests do not establish the untested known-limit routes.

## Self-Check: PASSED

- `97a1c94` and `fa04bfe` exist in reachable history.
- The four planned source/test files exist and the full Task 2 gate passed.
- The mutation control failed in the required direction and the restored guard passed both refusal arms.

---
*Phase: 48-survivable-state-writes-and-honest-gate-recovery*
*Completed: 2026-09-23*
