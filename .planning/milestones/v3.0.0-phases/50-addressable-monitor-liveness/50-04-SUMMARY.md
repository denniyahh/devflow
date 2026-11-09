---
phase: 50-addressable-monitor-liveness
plan: 04
subsystem: locking
tags: [rust, lock, pid-recycling, process-start-time, recover, e2e, nextest, tdd]

requires:
  - phase: 50-02
    provides: "tests/common/mod.rs hermetic_devflow + HermeticHome, and the test_home_isolation source scan that covers any suite declaring `mod common;`"
  - phase: 48
    provides: "lock::classify_holder_record / HolderStatus (Live, Recycled, Unconfirmable, NoHolder) and the D-02 one-writer rule"
provides:
  - "lock::acquire_after_existing reclaims NoHolder and Recycled holders through classify_holder_record; Live and Unconfirmable stay Contended"
  - "devflow recover --clean --phase N succeeds at a recycled-pid lock (R-13 repair)"
  - "recover_clean_e2e.rs spawns devflow only through common::hermetic_devflow"
affects: [50-05, stop, doctor, gate sweep, recover]

actuals:
  tokens: 3500
  tasks: 2
  commits: 3
plan_head_before: 1c0d481abd356b718784b051756ee2ecc33c3061
plan_head_after: 99b00c59b62af37c9786cd347b57c031a5f9b6b0

tech-stack:
  added: []
  patterns:
    - "Lock reclaim and read-only holder inspection share one classifier (classify_holder_record), so they always agree about who holds the lock"
    - "A live-holder control writes the lock record by hand (own pid + observed start time, no guard held), so the refusal comes from classification and not from the coordination flock"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/lock.rs
    - crates/devflow-core/src/recover.rs
    - crates/devflow-cli/tests/recover_clean_e2e.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "Reclaim reads liveness before the start time, in the same order holder_status uses. If the pid dies between the two reads the result is NoHolder (reclaim). If the start time is unreadable the result is Unconfirmable (contend). Neither outcome reclaims a confirmed live holder."
  - "The recycled-reclaim warning prints 'unknown' for a missing start time rather than unwrapping. The classifier only returns Recycled when both start times are present, so 'unknown' cannot actually appear."

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "A per-phase lock whose recorded pid is alive with a different start time is reclaimed; the new lock names the caller with its real start time"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/lock.rs#lock::tests::acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time"
        status: pass
    human_judgment: false
  - id: D2
    description: "A live identity-matched holder and a live holder with no recorded start time (Unconfirmable) are never reclaimed"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/lock.rs#lock::tests::acquire_never_reclaims_a_live_holder_whose_start_time_matches"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/lock.rs#lock::tests::acquire_never_reclaims_a_live_holder_with_no_recorded_start_time"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/recover.rs#recover::tests::clean_phase_refuses_a_live_pid_lock_and_cleans_nothing"
        status: pass
    human_judgment: false
  - id: D3
    description: "Reclaim stays serialised through the coordination flock"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/lock.rs#lock::tests::stale_reclaim_is_serialized_before_it_removes_the_lock_path"
        status: pass
    human_judgment: false
  - id: D4
    description: "R-13: `devflow recover --clean --phase N` at a recycled-pid lock exits 0, says it cleaned, and removes the phase's state and gate files"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/recover.rs#recover::tests::clean_phase_reclaims_a_recycled_pid_lock_and_cleans"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/recover_clean_e2e.rs#explicit_clean_at_a_recycled_lock_holder_reclaims_and_cleans"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/recover_clean_e2e.rs#explicit_clean_with_a_held_phase_lock_fails_and_cleans_nothing"
        status: pass
    human_judgment: false
  - id: D5
    description: "recover_cmd's explicit-phase comment describes the real rule; only comment lines of commands.rs changed"
    verification:
      - kind: other
        ref: "git diff -U0 1c0d481..HEAD -- crates/devflow-cli/src/commands.rs: 2 comment lines removed, 4 comment lines added, 0 non-comment lines changed"
        status: pass
    human_judgment: false

duration: 7min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 04: Recycled-pid lock reclaim (999.128 / D-08) Summary

**`acquire_after_existing` now decides reclaim with `classify_holder_record` (pid liveness plus recorded start time). A lock whose pid was reused by another process is reclaimed, so `devflow recover --clean --phase N` now works at a recycled-pid lock. An identity-confirmed live holder, or one whose identity cannot be checked, still keeps its lock.**

## Performance

- **Duration:** about 7 min
- **Started:** 2026-10-17T11:57:02Z
- **Completed:** 2026-10-17T12:04Z
- **Tasks:** 2 of 2
- **Files modified:** 4 source/test files, plus `deferred-items.md`

## Accomplishments
- Lock reclaim treats `HolderStatus::Recycled` like a dead holder. The change sits inside the coordination flock that `acquire_after_existing` already holds, so the `before_reclaim` seam and the serialisation test are untouched.
- The reclaim warning distinguishes the two cases. A recycled reclaim names the pid, the observed start time and the recorded start time. A dead-holder reclaim keeps its old wording.
- The test that pinned the defect was renamed to `clean_phase_reclaims_a_recycled_pid_lock_and_cleans` and inverted. A live-pid control was added beside it.
- New CLI e2e repair test (R-13). `recover_clean_e2e.rs` was moved to `mod common;` and `common::hermetic_devflow`, and its local `devflow_bin()` was deleted.
- The stale "regardless of staleness" comment in `recover_cmd` was corrected. No code line in `commands.rs` changed.

## Task Commits

1. **Task 1 (tracer): recycled-pid lock reclaimed through the existing classifier**
   - RED `2f74c04`: test(50-04): add failing test for recycled-pid lock reclaim
   - GREEN `2983641`: feat(50-04): reclaim a lock whose live pid was recycled
   - REFACTOR: none needed
2. **Task 2: flip the recover test, add the CLI repair test, fix the recover_cmd comment**
   - `99b00c5`: test(50-04): pin the repair at a recycled-pid lock end to end

## TDD Evidence

- **RED (Task 1).** Ran the three new lock tests together under a scratch nextest JUnit profile (`--tool-config-file gsd-red:<scratch>/nextest-red.toml --profile red`). The run exited 100: 3 run, 2 passed (the two controls), 1 failed. The target failed at its own `expect()` with `a recycled-pid lock must be reclaimed: Contended { pid: "<test pid>", ... }`, which is the pid-only predicate refusing. `gsd-tools check tdd-red-evidence` on the JUnit report returned `RED_EVIDENCE_OK` (`target_test_failed`, format `junit`). Semantic assessment: the target ran and failed on the planned assertion. There was no compile, fixture or load fault, and both controls passed before the change, as the plan requires.
  - Limitation: the `command` field of the evidence record names the filter set in shortened form (the two control names are written as `...`). The report itself is the unedited JUnit file from that run.
- **GREEN (Task 1).** All four named tests pass, each by its own `--no-tests=fail -E 'test(=...)'` command.
- **Task 2 had no separate RED commit, by design.** Its behaviour was implemented by Task 1's GREEN commit. As a substitute, the Task 2 tests were run against the RED-commit `lock.rs` (`git show 2f74c04:...`, then restored with `git checkout -- <file>`). `clean_phase_reclaims_a_recycled_pid_lock_and_cleans` failed (`Lock(Contended {...})`), `explicit_clean_at_a_recycled_lock_holder_reclaims_and_cleans` failed (`phase 65 is live or contended ... nothing was cleaned`), and the live control passed. So both new recycled tests discriminate.

## Negative Controls Run

| Control | Expected | Result |
|---|---|---|
| Mutate `acquire_after_existing` to reclaim Live and Unconfirmable | both lock controls fail | both FAIL (exit 100) with "must contend: LockGuard {...}" |
| Same mutation (Live only) against the recover tests | new `clean_phase_refuses_a_live_pid_lock_and_cleans_nothing` fails; old `..._while_the_per_phase_lock_is_contended` does not catch it | new control FAIL, old test PASS. This confirms the plan's reason for adding the hand-written live control. |
| Pre-fix `lock.rs` against the Task 2 recycled tests | both fail, live control passes | as expected |
| Append a raw `env!("CARGO_BIN_EXE_devflow")` to `recover_clean_e2e.rs` | 50-02's scan test fails on this file | FAIL: "recover_clean_e2e.rs has 1 raw devflow spawn(s); it must have 0" |

Every mutation was reverted. `git diff --quiet HEAD -- <file>` or `cmp` against a saved copy confirmed each file was restored before the next step.

## Verification (commands run, actual exit codes)

- Task 1 gate 1 (four named lock tests, chained): exit 0. Each reported `1 test run: 1 passed`. Run once after GREEN and again after Task 2.
- Task 1 gate 2 `cargo nextest run -p devflow-core --lib --no-tests=fail`: **exit 100 right after the GREEN commit**, because the one failure was `recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock`, the defect-pinning test that Task 2 inverts (`--no-fail-fast`: 882 run, 881 passed, 1 failed). After Task 2 it gave **exit 0, 883 passed** (1 leaky, see Issues).
- Task 2 gate 1 (2 core + 3 e2e named tests): exit 0. Each reported `1 passed`.
- Task 2 gate 2 (absence checks, `mod common;` line, 50-02 scan test, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`): exit 0. The scan test reported `1 passed`.
- Plan `<verification>`: `cargo nextest run -p devflow --bin devflow --no-tests=fail`: exit 0, 415 passed (2 leaky). The whole `recover_clean_e2e` target: exit 0, 12 passed.
- `commands.rs` diff over the plan: 0 non-comment lines changed.
- Not run: `scripts/check.sh all`, the container parity check, and the full CLI integration test set beyond `recover_clean_e2e` and `test_home_isolation`. `cargo semver-checks` was not run either, because no `pub` item changed (the edit is inside a private fn).

## Files Created/Modified
- `crates/devflow-core/src/lock.rs`: the reclaim predicate now goes through `classify_holder_record`. Adds the recycled-reclaim warning, three tests and a `write_own_pid_lock` test fixture.
- `crates/devflow-core/src/recover.rs`: the defect-pin test is renamed and inverted, and a live-pid control is added.
- `crates/devflow-cli/tests/recover_clean_e2e.rs`: `mod common;`, hermetic spawn, R-13 repair test (phase 65).
- `crates/devflow-cli/src/commands.rs`: comment-only fix in `recover_cmd`.
- `.planning/phases/50-addressable-monitor-liveness/deferred-items.md`: new entry for leaky tests outside this plan's scope.

## Decisions Made
- The two control tests went into the RED commit along with the target. The plan's behaviour block requires them to stay Contended "before and after", and that can only be shown if they exist before the change.
- `lock::lock_path` is `pub(crate)`, so the e2e test builds `.devflow/lock-{padded}` itself. It follows the existing pattern in `gate_sweep_e2e.rs`, rather than widening a published API.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 1's whole-lib gate could not pass until Task 2 ran**
- **Found during:** Task 1 (tracer feedback gate)
- **Issue:** The second `<automated>` gate of Task 1 runs the whole `devflow-core` lib. Once the GREEN change lands, the existing `recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock` fails, because it asserts the old defect. The plan assigns that test's inversion to Task 2, so the plan's own task order makes Task 1's gate fail in between.
- **Fix:** Before moving on, `--no-fail-fast` was used to confirm this was the only failure (881/882). The tracer's end-to-end slice, its four named tests, passed. Task 2 then inverted the test, and Task 1's whole-lib gate was re-run: exit 0, 883 passed. No code beyond the plan was changed.
- **Files modified:** none beyond the plan
- **Commit:** `2983641` leaves the lib with that one designed failure; `99b00c5` restores it to all green

---

**Total deviations:** 1 (Rule 3, plan-ordering). **Impact:** none on scope. Commit `2983641` alone is not lib-green, so a bisect landing on it will see that one test fail.

## Issues Encountered
- Nextest marked three tests LEAK, all in tests this plan did not touch: `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates`, `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling`, and `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper`. After the `--bin devflow` run, four `sleep 30` orphans from the first of these were alive. All four had already exited on their own when checked by recorded PID, so nothing was killed. Whether these tests also leak at the plan's base commit was not checked. Logged in `deferred-items.md`.
- Every lock fixture writes the lock record with this test process's own pid and start time ±1. That shows the classifier's decision for a live pid. It does not reproduce a real pid wraparound, which the tests cannot force.

## Known Stubs
None.

## Threat Flags
None. The change narrows reclaim to the classifier's NoHolder and Recycled verdicts. Mitigations: T-50-06 is covered by the lock and recover live controls and the mutation results above. T-50-07 is covered because the serialisation test passes unchanged.

## User Setup Required
None.

## Next Phase Readiness
- `stop` and `doctor` can now name `devflow recover --clean --phase N` as the repair at a Recycled gate, and that repair succeeds.
- Scratch files under the session scratchpad `exec-50-04/` are removed after this commit. `target/gsd-red/` and `target/nextest/red/` sit under the gitignored `target/` directory and are removed too.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED
- FOUND: crates/devflow-core/src/lock.rs, crates/devflow-core/src/recover.rs, crates/devflow-cli/tests/recover_clean_e2e.rs, crates/devflow-cli/src/commands.rs
- FOUND commits: 2f74c04, 2983641, 99b00c5 (each `git merge-base --is-ancestor` with HEAD)
- Artifact `contains` checks: `acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time` in lock.rs, and `explicit_clean_at_a_recycled_lock_holder_reclaims_and_cleans` in recover_clean_e2e.rs, each found once
- Key link: `classify_holder_record(` is called at lock.rs:255, inside `acquire_after_existing`
