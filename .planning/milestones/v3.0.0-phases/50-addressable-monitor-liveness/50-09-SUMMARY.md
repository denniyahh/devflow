---
phase: 50-addressable-monitor-liveness
plan: 09
subsystem: supervisor
tags: [rust, pidfd, process-group, signals, proc, cargo-mutants, macos-cfg]

requires:
  - phase: 50-07
    provides: "supervisor::runtime (the supervisor whose death leaves a STALE socket)"
  - phase: 50-06
    provides: "Sidecar, write_sidecar, remove_launch_files"
  - phase: 50-05
    provides: "SocketPath, publish, probe, Liveness, boot_id, protocol::euid"
  - phase: 50-04
    provides: "agent::process_start_time identity precedent, SupervisorRecord"
provides:
  - "devflow_core::supervisor::backstop: GroupMember, BackstopDecision (8 variants incl. RefuseIdentityUnavailable) + describe(socket, sidecar), SignalPath, BackstopOutcome, BackstopError, assess, teardown_stale"
  - "crate-internal: BackstopFacts (identity_provable), RecordedIdentity, classify, safe_target, group_members, signal_validated_group (Linux)"
  - "devflow_core::agent: crate-internal ProcStat and process_stat_fields (state, pgrp, session, start time from one /proc/<pid>/stat read)"
affects: [50-14, 50-15, 50-17]

actuals:
  tokens: 18566
  tasks: 3
  commits: 5
plan_head_before: 4ee02ac227becbaa75f6dfa37315170ccdd0fe23
plan_head_after: 6fa59a7af42d7773c42efe74f0cfdb42a008a725

tech-stack:
  added: []
  patterns:
    - "Pure decision over gathered facts plus a thin acting shell: classify(&BackstopFacts) has no cfg; only gather() differs by platform (the 999.158 fix shape)"
    - "Signal by pidfd: pidfd_open(leader) -> re-read start time and pgrp -> pidfd_send_signal(PIDFD_SIGNAL_PROCESS_GROUP); EINVAL -> pidfd signal-0 liveness + re-read -> killpg"
    - "Process-group test fixtures: TestGroup guard bound before any assertion; drop killpg's only while the leader is the test's unreaped child, then kills recorded members by (pid, start time)"

key-files:
  created:
    - crates/devflow-core/src/supervisor/backstop.rs
  modified:
    - crates/devflow-core/src/supervisor.rs
    - crates/devflow-core/src/agent.rs

key-decisions:
  - "An unreadable boot id or process table on Linux also answers RefuseIdentityUnavailable (fail closed), never PreviousBoot or EmptyGroup: classify matches (identity_provable, current_boot_id) together"
  - "A leader with the recorded start time that has left its group (pgrp != pgid) reads RefuseLeaderGone with the members, not LeaderPresentSignal: the plan's six steps leave that case unassigned, and signalling it would hit a group the leader no longer leads"
  - "The leader read and the member census both filter by owner uid first; another uid's process at the leader pid reads as absent"
  - "A process-table listing failure is None ('cannot tell'), never an empty group, in both gathering and the post-signal wait"
  - "The signal helper re-checks safe_target itself, so killpg can never reach pgid <= 1 or the caller's group even if a caller skipped classify"
  - "ESRCH mid-grace re-gathers and re-classifies: EmptyGroup is Killed, anything else is Refused(decision) with the files kept. A leader that changed before any signal re-classifies; if it still reads LeaderPresentSignal the call errors instead of looping"
  - "describe(&self, socket, sidecar) is one method on BackstopDecision covering every variant, rather than a Display impl, because the texts need the socket and sidecar paths"

patterns-established:
  - "When a kill path re-validates defensively, test the re-validation directly with signal 0 so a wrong 'send' is visible in the return value and harms nothing"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "V-18: a STALE socket over a validated leader-present group is torn down through the leader's pidfd (PidfdGroup on this 7.2 kernel), every member gone, socket and sidecar removed; control: a recorded start time one tick off is LeaderRecycled, all members alive"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::backstop_kills_a_validated_group_whose_leader_is_present"
        status: pass
    human_judgment: false
  - id: D2
    description: "teardown_stale acts only on a STALE socket: Alive, Gone and a ProbeError (regular file) are NotStale with files untouched; control: a STALE socket reaches a decision"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::teardown_stale_acts_only_on_a_stale_socket"
        status: pass
    human_judgment: false
  - id: D3
    description: "V-19/V-20/V-21: leader gone is refused and reported (both members alive, files kept, text lists them with the ps inspection command); a reused pgid is LeaderRecycled; a previous-boot sidecar is PreviousBoot; each with a kill control"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::backstop_refuses_and_reports_a_group_whose_leader_is_gone"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::a_reused_pgid_is_never_signalled"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::a_previous_boot_socket_is_removed_and_nothing_is_signalled"
        status: pass
    human_judgment: false
  - id: D4
    description: "T-50-25: pgid 0, 1, the caller's own group and u32::MAX are RefuseUnsafeTarget with files kept; another uid's group and leader read as empty/absent (group 1 control)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::the_backstop_never_targets_pgid_one_or_its_own_group"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::gathering_skips_processes_of_another_uid"
        status: pass
    human_judgment: false
  - id: D5
    description: "Exhaustive classifier table (every step, boundaries, order checks, MAC-2 identity_provable rows) with zero surviving mutants (gate: 7 caught, 2 unviable; 13 hand mutants caught)"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::classify_covers_every_rule_in_order"
        status: pass
      - kind: other
        ref: "Task 3 cargo-mutants gate, -F classify: mutants_exit=0 caught=7 missed=0 timeouts=0 (2 unviable)"
        status: pass
    human_judgment: false
  - id: D6
    description: "Kernel capability: an independent signal-0 probe of PIDFD_SIGNAL_PROCESS_GROUP; on this kernel the backstop must use PidfdGroup; the helper re-validates the leader after opening its pidfd"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::pidfd_group_signal_is_used_when_the_kernel_supports_it"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::the_group_signal_revalidates_the_leader_after_opening_its_pidfd"
        status: pass
    human_judgment: false
  - id: D7
    description: "MAC-2: the RefuseIdentityUnavailable text names the pgid, launch id, socket and sidecar paths and the manual repair, and no platform command"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/supervisor/backstop.rs#supervisor::backstop::tests::the_identity_unavailable_refusal_names_the_group_and_the_manual_repair"
        status: pass
    human_judgment: false
  - id: D8
    description: "MAC-2 off Linux: no pidfd item, killpg fallback or /proc gathering compiles there; the non-Linux gather and signal bodies refuse"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "cargo check --target aarch64-apple-darwin --all-targets for devflow-core and devflow: rc 0, no warnings (negative control: ungating the Linux helpers gives 4 x E0425)"
        status: pass
    human_judgment: true
    rationale: "Compile only. No macOS host has run the non-Linux gather or the RefuseIdentityUnavailable path (backlog 999.158, 999.160); the decision itself is tested on Linux through classify"

duration: 30min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 09: STALE-Socket Backstop Summary

**`devflow_core::supervisor::backstop` ends an orphaned agent group behind a STALE socket only when the group's leader is present, has the recorded start time and still leads the group. It signals through a pidfd opened on that leader, re-reading the leader after opening, with `PIDFD_SIGNAL_PROCESS_GROUP` (SIGTERM, wait, SIGKILL). A recycled leader pid, an empty group and a previous boot remove the files and signal nothing. A group whose leader is gone is refused: its members are reported and never signalled. Off Linux the backstop refuses before reading anything, and no signalling path exists there.**

## Performance

- **Duration:** 30 min
- **Started:** 2026-10-17T18:31:46Z
- **Completed:** 2026-10-17T19:01:28Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- `classify(&BackstopFacts)` is a pure function with no `cfg`. In order it answers: unsafe target (pgid does not fit `pid_t`, is ≤ 1, or is `getpgrp()`); identity unavailable (`identity_provable` false, or no current boot id); sidecar/state mismatch (launch id, pgid, start time, boot id, naming the field); previous boot; empty group; leader present; leader recycled; leader gone.
- `teardown_stale` probes again first. It returns `NotStale` unless the probe is `Ok(Stale)`. `PreviousBoot`, `EmptyGroup` and `LeaderRecycled` remove the socket and sidecar. `LeaderPresentSignal` sends SIGTERM to the group, polls the census for `TERMINATE_VERIFY_WAIT`, then sends SIGKILL and waits up to 1 s; the files are removed only once the group is empty. Every `Refuse*` keeps the files.
- `assess` returns the same decision without acting, for doctor and the launch guard.
- `BackstopDecision::describe(socket, sidecar)` gives the CLI text for every variant. `RefuseLeaderGone` lists each member's pid, start time, state, session and cmdline, states the F1 limit and prints `ps -o pid,pgid,sid,lstart,args -g <pgid>`.
- `agent::process_stat_fields` reads state, pgrp, session and start time from one `/proc/<pid>/stat` read. `process_start_time` is unchanged, and its existing tests pass.

## Task Commits

1. **Task 1 (tracer): leader-present teardown.** RED `f192f05` test(50-09), GREEN `197e146` feat(50-09)
2. **Task 2: leader gone, reused pgid, previous boot, unsafe targets, refusal text.** RED `aeef149` test(50-09), GREEN `7d9ede8` feat(50-09)
3. **Task 3: classifier table, kernel check, mutation run.** `6fa59a7` test(50-09)

**Tracer feedback gate:** this run was interactive (`auto_advance=false`, `_auto_chain_active=false`), `human_verify_mode` was at its default `end-of-phase`, and Task 1's `<verify>` is automated only, so row 3 applied. Both Task 1 gates were run again on the GREEN tree before `197e146`. V-18 reported `1 test run: 1 passed` (rc 0) and the macOS gate returned rc 0. Expansion followed.

## TDD Gate Compliance

| Task | RED | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------------------------------------------------------------------|-------|
| 1 | `f192f05` | exit 100, 1 run / 1 failed; `RED_EVIDENCE_OK target_test_failed` | `197e146` |
| 2 | `aeef149` | target `the_identity_unavailable_refusal_names_the_group_and_the_manual_repair`: exit 100, 1 run / 1 failed; `RED_EVIDENCE_OK target_test_failed` | `7d9ede8` |
| 3 | none (see below) | substitute: cargo-mutants plus 13 hand mutants | n/a (tests over Task 1-2 code) |

**Semantic assessment.**
- Task 1: V-18 ran and failed on its first planned assertion: `left: Refused(RefuseIdentityUnavailable)`, `right: RemovedNothingSignalled(LeaderRecycled)` (the stubbed `teardown_stale`). There was no compile, fixture or load fault: the group spawned and settled, the STALE socket and the sidecar were written, and the guard reaped everything.
- Task 2: the target failed on `missing "nothing was signalled or removed"` because `describe` was a stub returning an empty string. V-19 also failed, and it failed at its text assertion, after its behavioural assertions (refused, members alive, files kept) had already passed. V-20, V-21, the unsafe-target test and the uid test passed on their first run. The Task 1 classifier already decides those cases (the "feature already exists" case). The hand mutants below are their discrimination evidence.
- Task 3 has no RED commit, for the same reason. Its evidence is the mutation run.

## Mutation and Discrimination Evidence

**Plan gate (cargo-mutants 27.1.0, `-F classify`, run at the final tree):** `9 mutants tested: 7 caught, 2 unviable`, giving `mutants_exit=0 caught=7 missed=0 timeouts=0`, followed by fmt and `clippy --workspace --all-targets -D warnings`, gate rc 0. The two unviable mutants were `classify -> Default::default()` (`BackstopDecision` has no `Default`) and deleting the `(true, Some(boot))` match arm, which does not compile. No mutant timed out.

**Extra run (outside the plan gate)**, `-F 'safe_target|first_disagreement|group_members|is_original_leader|own_process_stat'`: the first run gave 27 caught, **2 missed**, 2 unviable. Both survivors were in `is_original_leader`, the leader check the signal helper repeats after it opens the pidfd (`-> true`, and `&&` → `||`). No test reached that race. I added `the_group_signal_revalidates_the_leader_after_opening_its_pidfd`, which uses signal 0 throughout. The rerun gave **29 caught, 0 missed**, 2 unviable.

**Hand mutants** (each applied, the named test run, the file restored and checked with `cmp`):

| Mutant | Test | Result |
|---|---|---|
| H1 identity check removed (boot defaults to "") | classify table | caught |
| H2 mismatch check removed | classify table | caught |
| H3 previous-boot check removed | classify table | caught |
| H4 empty-group check removed | classify table | caught |
| H5 empty group checked before previous boot | classify table | caught: row "previous boot wins over an empty group" |
| H6 identity check after the mismatch check | classify table | caught |
| H7 unsafe guard after the identity check | classify table | caught |
| H8 `RefuseLeaderGone` reports no members | classify table | caught |
| H9 boot id not compared in `first_disagreement` | classify table | caught |
| H10 `signed > 0` (pgid 1 allowed) | classify table | caught |
| H11 caller's own group allowed | classify table | caught: row "the caller's own group" |
| H12 start time not compared in `first_disagreement` | classify table | caught |
| H13 force the `killpg` fallback | kernel-capability test | caught: `via` was `ValidatedKillpg` |
| M1 start-time guard off (recycled leader reads present) | V-20 | caught. The signal helper's own check also refused, so nothing was signalled |
| M2 previous-boot check off | V-21 | caught: the group was `Killed` |
| M3 `safe_target` accepts ≥ 0 | unsafe-target test | caught: pgid 0 |
| M4 uid filter off in the census | uid test | caught |
| M5 a leaderless group reads present | V-19 | caught. The helper got ESRCH, classified again and errored, so nothing was signalled |
| Fallback path forced (EINVAL branch) | V-18 | passes with `V-18 signal path: ValidatedKillpg`: the fallback kills the group correctly |

**macOS gate negative control:** I removed the non-Linux signal stub and the Linux `cfg`s from the four Linux helpers. `cargo check --target aarch64-apple-darwin` then failed with 4 × `E0425` (`SYS_pidfd_open`, `SYS_pidfd_send_signal` ×2, `PIDFD_SIGNAL_PROCESS_GROUP`). I restored the file and checked it with `cmp`.

## Verification (commands run, actual exit codes)

| Gate | Result |
|---|---|
| Task 1 `<automated>` 1 (V-18), verbatim | rc 0, `1 test run: 1 passed` |
| Task 1 `<automated>` 2 (macOS, both packages, `--all-targets`), verbatim | rc 0, no warnings (after Task 1 and again after Task 3) |
| Task 2 `<automated>` (5 chained named tests), verbatim | rc 0, each `1 test run: 1 passed` |
| Task 3 `<automated>` 1 (classify table, kernel test), verbatim | rc 0, each `1 test run: 1 passed`; no skip line (this kernel, 7.2.5, has the flag) |
| Task 3 `<automated>` 2 (cargo mutants + fmt + clippy workspace) | rc 0, `mutants_exit=0 caught=7 missed=0 timeouts=0`. Not verbatim: see deviation 1 |
| Task 3 `<automated>` 3 (macOS), verbatim | rc 0, no warnings |
| Core lib suite `cargo nextest run -p devflow-core --lib --no-fail-fast` | rc 0, `928 tests run: 928 passed (1 leaky)` |
| CLI `cargo nextest run -p devflow --bin devflow --no-fail-fast` | rc 0, `415 tests run: 415 passed (2 leaky)` |
| Semver gate `semver-gate.sh check --through 50-08` (before `f192f05`, before `197e146`, and at the final tree) | rc 0, `verdict=match`, no new item; negative control `compare --through baseline` on the final log gives `mismatch` with rc 1 |

**What these checks do not establish:**
- The `EINVAL` fallback has only run under a forced hand mutant, on a kernel that supports the flag. No pre-6.9 kernel has exercised it.
- The macOS branch is compiled only. No macOS host has run the non-Linux gather or the refusal (999.158, 999.160).
- The races that `teardown_stale` defends against are not timed into: a leader recycled between gathering and `pidfd_open`, and a leader dying during the TERM grace while members survive TERM. The re-validation is tested directly, and M1/M5 showed the helper refusing, but the `Refused(RefuseLeaderGone)`-after-SIGTERM branch has never run.
- No test covers a group that survives SIGKILL (the `Io` error path).
- The uid test needs a non-root runner, and it prints a skip line as root.

## Files Created/Modified

- `crates/devflow-core/src/supervisor/backstop.rs`: the module, its 11 tests and their fixtures (`TestGroup` guard, an independent `/proc` scan)
- `crates/devflow-core/src/supervisor.rs`: `pub mod backstop;`
- `crates/devflow-core/src/agent.rs`: `ProcStat`, `process_stat_fields`, `parse_stat_fields` and a parser test

## Decisions Made

See `key-decisions` in the frontmatter. Each is inside the plan's remit: a case the plan's six steps leave unassigned, fail-closed handling of unreadable facts, and the shape of the text API.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The mutants gate was run through a scratch script, not verbatim**
- **Found during:** Task 3
- **Issue:** the plan's command is `L=$(mktemp -d); trap "rm -rf \"$L\"" EXIT`. The harness refuses `rm -rf` on a variable (the same as 50-08's deviation 1), and `mktemp -d` puts the output on the RAM-backed `/tmp`.
- **Fix:** I ran the same `cargo mutants -v --no-shuffle -f … -F "classify" -p devflow-core -o <fixed dir> -- --lib supervisor::backstop::` with the same count parsing and the same pass condition (`mutants_exit` 0 or 3, caught + timeouts ≥ 1, missed 0, then fmt and workspace clippy). `TMPDIR` pointed at an on-disk directory under `target/`. I removed both directories afterwards.

**2. [Rule 2 - Missing test] The signal helper's post-open re-validation was untested**
- **Found during:** Task 3, extra mutants run
- **Issue:** two mutants of `is_original_leader` survived. That check is the T-50-24 mitigation.
- **Fix:** added `the_group_signal_revalidates_the_leader_after_opening_its_pidfd` (wrong start time, and a process that does not lead its pid's group, both `LeaderGone`; control: the real leader is `Sent`). It uses signal 0 throughout. The rerun reported 0 missed. Commit `6fa59a7`.

**3. [Rule 2 - Missing test] Owner-uid filtering had no test**
- **Found during:** Task 2
- **Fix:** added `gathering_skips_processes_of_another_uid`. Group 1 is visible to an independent scan but reads empty through `group_members`, and `own_process_stat(1)` is `None`. Commit `aeef149`.

**4. [Rule 1 - Unassigned case] A present, original leader that left its group**
- The plan's steps 4-6 assume that a leader with the recorded start time also leads the group. When it does not, the case reads `RefuseLeaderGone(members)`: the group is reported and never signalled. It is pinned by the table row "original leader left the group".

**5. [Scope note] `agent.rs` went into the Task 1 RED commit**
- The RED test's fixtures read `/proc` through `process_stat_fields`, so the parser and its unit test had to be in the RED commit. The RED commit carried only the backstop's public types and stubs; `classify` and every private helper came in GREEN.

**6. [Orchestrator-directed] Tracking and commit guard**
- STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched. `pin-guard.sh` passed before the first edit and before every commit. The `agent-*` branch allow-list does not apply to this sequential executor on `feature/phase-50`. `.planning/user/errors/state-stale.json` was left uncommitted.

---

**Total deviations:** 4 auto-fixed (1 blocking, 2 missing-test, 1 unassigned case), plus 2 scope/process notes
**Impact on plan:** none on the planned public API. All additions are additive, and the semver gate matched.

## Issues Encountered

- `agents::opencode::tests::probe_rejects_a_setpgid_pipe_holder_before_parent_exit` failed once in the first full core run. It is the known intermittent failure in backlog 999.151, in code this plan does not touch. Run alone it passed 5 of 5 times, and a second full run passed 928/928.
- Pre-existing leaky tests left `sleep 30` children in 3 core runs and 1 CLI run: `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper`, `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling` and `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates`. I confirmed each by `/proc/<pid>/environ` (`CARGO_MANIFEST_DIR` in this worktree, `NEXTEST_TEST_NAME`) and killed it by its recorded pid (2358586, 2490513, 2493103, 2500071, 2500073, 2500074; 2499678 had already exited). No backstop test leaked a process in any run, including the mutant runs.

## Known Stubs

None.

## User Setup Required

None.

## Next Phase Readiness

- 50-14 (`devflow stop` on a STALE socket) calls `teardown_stale(socket, sidecar, state_record)` and prints `decision.describe(socket, sidecar)` for `Refused`/`RemovedNothingSignalled`. It must treat `RefuseIdentityUnavailable` like every other `Refuse*`: the run counts as possibly live.
- 50-15 (the launch guard) and doctor call `assess` and must not act on its result.
- 50-17 (OPERATIONS) should document the residual F1 limit (leader gone, members alive, report only), the inspection command, and the macOS manual repair.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- Files: `crates/devflow-core/src/supervisor/backstop.rs`, `crates/devflow-core/src/supervisor.rs`, `crates/devflow-core/src/agent.rs` and this SUMMARY exist.
- Commits `f192f05`, `197e146`, `aeef149`, `7d9ede8` and `6fa59a7` are ancestors of HEAD. Each RED `test(50-09)` precedes its GREEN `feat(50-09)`: `f192f05` before `197e146`, `aeef149` before `7d9ede8`.
