---
phase: 50-addressable-monitor-liveness
plan: 24
subsystem: supervisor
tags: [rust, monitor, process-group, signals, proc, sidecar, tdd, doc-check, semver]

requires:
  - phase: 50-08
    provides: "run_supervised_stage (spawn, pgid check, pid file, serve), abandon_agent, stage_sidecar, MonitorError::NotReady"
  - phase: 50-09
    provides: "supervisor::backstop::assess and BackstopDecision; agent::process_stat_fields (state) for the SIGSTOP settle"
  - phase: 50-10
    provides: "the in-crate child-test conventions and the semver gate stage 50-10"
provides:
  - "devflow_core::monitor (all private): PUBLISH_WAIT, wait_for_publish, end_unpublished_child, ChildStat, parse_proc_stat_line, read_child_stat, proc_table, direct_children, signal_guarded_group, signal_own_child, exit_within, AgentGroupGuard, end_recorded_agent_group (+ RecordedGroupEnd, decision_name, group_member_count, group_empties_within), #[cfg(test)] monitor_test_hold"
  - "run_supervised_stage: AgentGroupGuard armed after the pgid check, disarmed after serve; sidecar written before serve with the after_sidecar test hold between"
  - "doc-check-allowlist.toml: source_to_docs exception for DEVFLOW_TEST_HOLD (50-13 relies on it)"
affects: [50-11, 50-13, 50-23]

actuals:
  tokens: 12137
  tasks: 2
  commits: 4
plan_head_before: 5e5b18ab74351609aed7120f6fae00f2892c8cf5
plan_head_after: 13f5d8f1829b75235ffae7746cce833d82b0de06

tech-stack:
  added: []
  patterns:
    - "Publish wait: poll try_wait then probe every 20 ms; an early exit is reaped; a timeout is SIGTERM, 2 x TERMINATE_VERIFY_WAIT, SIGSTOP, /proc ppid snapshot, killpg(child), reap, then SIGKILL of each snapshotted child's own group whose leader keeps its start time"
    - "A guard owns an unreaped Child until a hand-off succeeds; its Drop signals the group before it reaps, so the pgid stays reserved for its own signal"
    - "A pgid read from disk (the sidecar) is signalled only on backstop::assess = LeaderPresentSignal plus a fresh is_same_process, through one guarded group-signal fn"
    - "Child entries for process tests are #[ignore]d tests that return at once without their env; the parent spawns current_exe() with `<entry> --exact --include-ignored --test-threads=1` under process_group(0)"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/monitor.rs
    - doc-check-allowlist.toml

key-decisions:
  - "end_recorded_agent_group returns a private RecordedGroupEnd { text, keep_sidecar } instead of the plan's String, because the ending step must know whether to keep the sidecar (Refuse* decisions)"
  - "Refuse* texts give a member count from a /proc census of the recorded pgid (group_member_count), since only RefuseLeaderGone carries its members; the count is for the message only and is not uid-filtered"
  - "A recorded group that still has members 2 s after its SIGKILL keeps its sidecar, so doctor still lists it"
  - "The guarded group-signal fn is private to monitor.rs: 50-07's runtime::signal_group and 50-09's safe_target are private, so nothing crate-visible existed to reuse"
  - "The SIGSTOP is followed by a bounded (100 ms) poll for state T before the /proc snapshot, so a stop still in flight cannot let the child fork after the listing"

patterns-established:
  - "Bind every process guard (OwnedChild, LeaderGroup keyed by start time) before the first assertion; a LeaderGroup signals only while its leader still has the recorded start time"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "A publish wait that expires with the child running ends the child and the sleeper group it started (SIGTERM ignored, grace, SIGSTOP, snapshot, killpg, reap, snapshot-child group kill) and returns NotReady naming the child and '1 child process group(s) ended'; control: a bare kill of the child's own group leaves the sleeper alive"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_supervisor_that_never_publishes_is_ended_with_its_agent_group"
        status: pass
    human_judgment: false
  - id: D2
    description: "parse_proc_stat_line reads ppid, pgrp and start time (fields 4, 5, 22) after the last ')' with a hostile comm, returns None on a truncated line, and matches parent_id(), process_start_time and getpgrp on /proc/self/stat"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::proc_stat_parse_reads_ppid_pgid_and_start_time_after_the_last_paren"
        status: pass
    human_judgment: false
  - id: D3
    description: "run_supervised_stage that fails before publishing (symlinked sock/) leaves no agent and wait_for_publish reports it exited before publishing; control: a plain cache dir is returned as published with the launch id, the agent alive and identity-confirmed, then request_shutdown stops it within 5 s"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_supervisor_that_fails_to_publish_ends_its_agent_group"
        status: pass
    human_judgment: false
  - id: D4
    description: "RV-03 (V-34): a __monitor SIGKILLed after its sidecar write and before publishing has its agent group ended by the launcher through the sidecar, and the socket and sidecar are gone; controls: sidecar removed -> 'no sidecar' and the agent survives; start time one tick off -> LeaderRecycled, nothing signalled"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_supervisor_killed_before_publishing_has_its_agent_group_ended_through_its_sidecar"
        status: pass
    human_judgment: false
  - id: D5
    description: "monitor_test_hold is defined once under #[cfg(test)] (source scan with six planted controls); DEVFLOW_TEST_HOLD has a reasoned source_to_docs exception and both doc_check tests pass"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::the_test_hold_is_compiled_out_of_non_test_builds"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::allowlist_entries_require_reasons"
        status: pass
    human_judgment: false
  - id: D6
    description: "No pub item added or broken (semver-gate.sh check --through 50-10 prints verdict=match); clippy -D warnings passes on every devflow-core target with the non-test dead-code expectations 50-11 will remove; scripts/check.sh all passes under a sentinel HOME with nothing devflow left in it"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "bash .planning/phases/50-addressable-monitor-liveness/tools/semver-gate.sh check --through 50-10 -> SEMVER verdict=match"
        status: pass
      - kind: other
        ref: "sentinel-HOME scripts/check.sh all -> 'check.sh: all OK', gate exit 0 (devflow-core lib 934 passed, 1 ignored)"
        status: pass
    human_judgment: false
  - id: D7
    description: "RV-03 residual: a SIGKILL of __monitor between the agent's fork and the sidecar's rename leaves no record of the agent, so that agent is not ended (T-50-70, accepted)"
    requirement: SUPV-01
    verification: []
    human_judgment: true
    rationale: "A stated, accepted gap, not a deliverable a test can prove; no fixture can land a SIGKILL in that span"

duration: 18min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 24: Publish Wait and Pre-Publish Agent Teardown Summary

**A launcher's `wait_for_publish` now either returns the supervisor's ping reply or ends the supervisor and every agent group it started (by stop-snapshot-kill on timeout, or through the launch's validated sidecar after a SIGKILL), and `run_supervised_stage` ends its own agent group on any error or panic before it publishes.**

## Performance

- **Duration:** about 18 min
- **Started:** 2026-10-17T19:35:18Z
- **Completed:** 2026-10-17T19:53Z
- **Tasks:** 2 (both TDD)
- **Files modified:** 2

## Accomplishments

- `wait_for_publish(child, socket, launch_id, wait)` (private, `PUBLISH_WAIT` = 10 s for 50-11): polls every 20 ms; a matching ping returns the reply and leaves the child running; an early exit is reaped; a timeout runs SIGTERM, a `2 × agent::TERMINATE_VERIFY_WAIT` grace, SIGSTOP, a `/proc` ppid snapshot, `killpg(child)`, reap, and SIGKILL of each snapshotted child's own group whose leader keeps its snapshotted start time. Every path ends in `MonitorError::NotReady` naming the child pid, how it ended, and how many child groups were ended.
- RV-03: once the child is reaped, the ending step reads the launch's sidecar. It SIGKILLs the recorded group only when the sidecar is this launch's, `backstop::assess` returns `LeaderPresentSignal`, and `agent::is_same_process` re-confirms the leader just before the signal. Every other case signals nothing and the `NotReady` text says which case it was. `Refuse*` decisions keep the sidecar so `doctor` still lists the launch.
- `run_supervised_stage`: `AgentGroupGuard` owns the agent's unreaped `Child` from the passed pgid check until `Supervisor::serve` returns `Ok`. On drop it SIGKILLs the group, then reaps. The sidecar is written right after the pid file and before `serve`, with `#[cfg(test)] monitor_test_hold("after_sidecar")` between them.
- One guarded group-signal fn (`signal_guarded_group`) refuses a pgid that does not fit `pid_t`, is ≤ 1, or is this process's own group.

## Task Commits

1. **Task 1 (tracer): a publish wait that expires ends the child and its agent group.** RED `3d30a73` test(50-24), GREEN `08e48e1` feat(50-24).
2. **Task 2: the pre-publish guard, the early sidecar, the RV-03 ending step, the test-hold scan and the allowlist entry.** RED `098c500` test(50-24), GREEN `13f5d8f` feat(50-24).

**Tracer feedback gate:** this was an interactive run (`auto_advance=false`, `_auto_chain_active=false`), `human_verify_mode` was at its default `end-of-phase`, and Task 1's `<verify>` has only automated steps, so row 3 of the gate applied. Both Task 1 gates were run on the GREEN tree before `08e48e1` and passed (exit 0 each). Expansion followed.

## TDD Gate Compliance

| Test | RED commit | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------|-------|
| `a_supervisor_that_never_publishes_is_ended_with_its_agent_group` | `3d30a73` | exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed` | `08e48e1` |
| `proc_stat_parse_reads_ppid_pgid_and_start_time_after_the_last_paren` | `3d30a73` | exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed` | `08e48e1` |
| `a_supervisor_killed_before_publishing_has_its_agent_group_ended_through_its_sidecar` | `098c500` | exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed` | `13f5d8f` |
| `the_test_hold_is_compiled_out_of_non_test_builds` | `098c500` | exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed` | `13f5d8f` |
| `a_supervisor_that_fails_to_publish_ends_its_agent_group` | `098c500` | **passed in RED (exit 0)**: no RED evidence | `13f5d8f` |

**Semantic assessment.**
- **Never-publishes test.** In the RED tree, `wait_for_publish` was a poll-only stub that returned `NotReady` with no teardown. The test failed on its planned assertion, `the teardown must end the sleeper's group <pid>`, after the control half had passed. There was no compile, fixture or load fault.
- **Parse test.** It failed on its first assertion against the stub, which returned `None` where `Some(ChildStat { ppid: 4242, pgid: 777, start_time: 98765 })` was expected.
- **RV-03 test.** It failed on run 1's text assertion. The child published (there was no hold or early sidecar yet), was SIGKILLed, and the wait reported `exited by itself before publishing (signal: 9 (SIGKILL)); 0 child process groups ended` with no sidecar step. Its `LeaderGroup` guard ended the orphaned agent during unwinding: no `sleep 300` was left afterwards.
- **Hold scan.** It failed on the planted `no attribute` control against the stub verdict.
- **Fails-to-publish test (the "feature already exists" case).** It passed in RED because both directions it covers already worked: Task 1's early-exit and success branches, and 50-08's `abandon_agent` on a `serve` error. Mutant M2 below is its discrimination evidence.

## Mutation and Discrimination Evidence

For each mutant: apply it, run the named test, then restore the file from a saved copy and confirm the restore with `cmp`.

| Mutant | Test | Result |
|---|---|---|
| RED stub: `wait_for_publish` with no teardown | never-publishes | caught: sleeper group alive |
| RED stub: parse returns `None` | parse | caught |
| M2: `AgentGroupGuard::drop` forgets the child (no signal, no reap) | fails-to-publish | caught: `no agent survives a publish error: <pid> is alive` |
| M3: the RV-03 ending step skips its SIGKILL | killed-before-publishing | caught: `the recorded agent group <pgid> is ended` |
| M4: `#[cfg(test)]` removed from `monitor_test_hold` | hold scan | caught at compile time (E0425: the cfg(test) `MONITOR_TEST_HOLD_WAIT`) |
| M4b: `#[cfg(test)]` replaced by `#[allow(dead_code)]` and the wait inlined (compiles) | hold scan | caught: `fn monitor_test_hold( is not directly under #[cfg(test)]` |
| `DEVFLOW_TEST_HOLD` allowlist entry removed | `doc_check::source_devflow_env_vars_and_subcommands_are_documented` | caught: `DEVFLOW_TEST_HOLD is missing from scoped operator docs` |

## Verification Run (exit codes recorded)

- Task 1 gate 1 (two named tests): exit 0, each `1 test run: 1 passed`.
- Task 1 gate 2 (presence and privacy scan + `cargo clippy -p devflow-core --all-targets -- -D warnings`): exit 0.
- Task 2 gate 1 (three named tests): exit 0, each `1 passed`.
- Task 2 gate 2 (guard, sidecar write, `assess(` presence; hold scan; `semver-gate.sh check --through 50-10`): exit 0, `SEMVER verdict=match`.
- Task 2 gate 3 (`scripts/check.sh all` under a sentinel HOME, then the `find` for a file or socket with `devflow` in its path): exit 0, `check.sh: all OK`. The devflow-core lib ran 934 passed and 1 ignored (this plan's child entry); the CLI bin ran 418 passed and 2 ignored. The sentinel HOME was removed by the gate.
- Task 2 gate 4 (two doc_check tests by name): exit 0.
- Gates 1 and 2 were re-run on the committed tree `13f5d8f`: exit 0.

**What these runs do not establish:**
- The never-publishes test's ~7.6 s run time is set by the fixed 6 s SIGTERM grace, not by teardown speed.
- No test covers these branches:
  - a supervisor that honours SIGTERM inside the grace;
  - a `Refuse*` decision keeping the sidecar;
  - a recorded group that still has members after its SIGKILL;
  - an unparsable sidecar, or one with another launch id.
- Each test passed in one run. That shows the paths work, not that they are free of timing flakes.

## Files Created/Modified

- `crates/devflow-core/src/monitor.rs`: the publish wait and its teardown, the RV-03 ending step, `AgentGroupGuard` and the early sidecar write in `run_supervised_stage`, the cfg(test) hold, and 5 tests plus 1 `#[ignore]`d child entry.
- `doc-check-allowlist.toml`: the `DEVFLOW_TEST_HOLD` `source_to_docs` exception (the plan's reason text, verbatim).

## Decisions Made

See `key-decisions` in the frontmatter. All were made under Claude's remit, inside the plan's mechanism.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `end_recorded_agent_group` returns `RecordedGroupEnd { text, keep_sidecar }`, not `String`**
- **Found during:** Task 2
- **Issue:** The plan's signature `-> String` cannot tell the ending step to skip `remove_launch_files` for the sidecar on a `Refuse*` decision, which the same plan requires.
- **Fix:** A private two-field struct. The ending step removes only the socket file when `keep_sidecar` is set.
- **Files modified:** `crates/devflow-core/src/monitor.rs`
- **Commit:** `13f5d8f`

**2. [Rule 2 - Missing critical] Member count for every `Refuse*` text, and a kept sidecar when a SIGKILLed recorded group does not empty**
- **Found during:** Task 2
- **Issue:** The plan asks the `Refuse*` texts to give the member count, but only `RefuseLeaderGone` carries its members. A group still populated 2 s after its SIGKILL would otherwise lose its only record.
- **Fix:** `group_member_count` counts processes in the recorded pgid from a `/proc` census (message only, not uid-filtered). A group that does not empty keeps its sidecar and says so.
- **Commit:** `13f5d8f`

**3. [Rule 3 - Blocking] `PUBLISH_WAIT` referenced in the Task 1 test**
- **Found during:** Task 1
- **Issue:** Before Task 2, nothing in the test build used `PUBLISH_WAIT`, and the `cfg_attr(not(test), expect(dead_code))` does not cover the test build. That made the Task 1 gate's `clippy --all-targets -D warnings` fail.
- **Fix:** The never-publishes test asserts that its 1 s wait is shorter than `PUBLISH_WAIT`.
- **Commit:** `3d30a73`

**4. [Rule 2] Tests strengthened beyond `<behavior>`**
- The never-publishes test also asserts `1 child process group(s) ended`.
- The parse test also asserts the pgid against `getpgrp()`.
- The fails-to-publish control also requires the child's log to show its one entry passed. A wrong entry name would otherwise make the failure direction pass vacuously as an "early exit". The failure direction is protected separately: it reads the agent pid file, which is missing if the entry never ran.
- **Commits:** `08e48e1`, `098c500`

**5. Doc-only change on a `pub` item.** The `MonitorError::NotReady` doc comment now also covers "a supervisor that never published". It is not a semver change: the gate still matches.

**6. Commit subject over the 72-character advisory.** The `08e48e1` subject is 73 characters. The `commit-msg` hook only warned. Per the run's instructions it was not amended.

---

**Total deviations:** 4 auto-fixed (2 Rule 2, 2 Rule 3), plus 2 notes (a doc-only change and a 73-character commit subject).
**Impact on plan:** None of these changes the mechanism. 50-01's semver gate still matches.

## Issues Encountered

- During gate 3 a transient `sleep 30` showed up in the process list. It had exited by the time it was inspected (`/proc/<pid>` was gone), so nothing was killed. This plan's tests use `sleep 300`, and none were left.

## RV-03 Residual (stated, not covered)

A SIGKILL of `__monitor` between the agent's fork (inside `Command::spawn`) and the rename of its sidecar leaves no record from which the agent's identity could be proved, so that agent is not ended. The span covers:
- `spawn`'s own fork-to-exec;
- the `getpgid` check (one `/proc` read);
- the pid-file write;
- the sidecar write.

No fixture can hit it. Closing it needs a child subreaper or an agent self-record in `pre_exec`; both were rejected in the plan's Review Dispositions Ledger (T-50-70, accept).

## Cleanup

- Scratch lives under the session scratchpad `exec-50-24/` (logs, RED JUnit copies, mutant backups) and is removed at the end of this run.
- The RED evidence records are in the gitignored `target/red-evidence/`.
- No background process was left, and no `/tmp/dfs*`, `/tmp/dfu*` or `/tmp/p50home.*` directory remained after the runs (checked with `ls`).

## User Setup Required

None.

## Next Phase Readiness

- 50-11 can call `wait_for_publish(&mut child, &socket, &launch_id, PUBLISH_WAIT)` from `spawn_monitor` and remove every `cfg_attr(not(test), expect(dead_code, reason = "spawn_monitor calls it from 50-11"))` this plan added. There are 18 such items (counted two independent ways, `rg -c` and a Python substring count over the non-test text, both 18); an unfulfilled `expect` fails `-D warnings`, so none can be kept by accident.
- 50-13's CLI `test_hold` can rely on the `DEVFLOW_TEST_HOLD` allowlist entry.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- Files: `crates/devflow-core/src/monitor.rs`, `doc-check-allowlist.toml` and this SUMMARY exist.
- Commits `3d30a73`, `08e48e1`, `098c500`, `13f5d8f` and `eac1c55` are ancestors of HEAD (`git merge-base --is-ancestor`). Each RED `test(50-24)` commit comes before its GREEN `feat(50-24)` commit.
- Correction made after the first write: an earlier draft of this SUMMARY said 15 dead-code expectations. The real count is 18, and the text above now says 18.
