---
phase: 50-addressable-monitor-liveness
plan: 14
subsystem: cli
tags: [rust, stop, supervisor-socket, backstop, unix-socket, teardown]

requires:
  - phase: 50-06
    provides: sweep_in / sweep_for_in / SweepEntry, request_shutdown / ShutdownOutcome, remove_launch_files
  - phase: 50-09
    provides: supervisor::backstop::teardown_stale, BackstopOutcome, BackstopDecision::describe
  - phase: 50-12
    provides: doctor's orphan repair line `devflow stop --phase N --launch <id>`
  - phase: 50-13
    provides: a supervisor whose state will not load still answers `stopped`; handed_off marking
provides:
  - "`devflow stop` addresses the run's supervisor socket: ALIVE shutdown, STALE backstop, GONE lock fallback, probe error refusal"
  - "`devflow stop --phase N --launch <id>` tears down one socket by launch id (sidecar or file-name id), from any directory"
  - "`recover --clean --phase N` refuses while the phase's supervisor is ALIVE"
  - "rewritten `Stop` help text and regenerated help snapshot"
affects: [50-15, 50-16, 50-17, 50-23]

actuals:
  tokens: 26002
  tasks: 3
  commits: 7
plan_head_before: 8336fa8edbdc2078ad0ada3265c4d683f41c797a
plan_head_after: f4d0ad9ae8012cfeb28cb168fbf27cbfcfc8744c

tech-stack:
  added: []
  patterns:
    - "stop / stop_in(cache_dir) split so in-process tests never sweep the operator's real cache"
    - "a shutdown request that errors is probed once more before it counts as a failure"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/stop_e2e.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt

key-decisions:
  - "A shutdown request that errors (reset, broken pipe, refused connect) is probed once more: STALE goes to the backstop, GONE re-locates the run (so a handed-off successor is not missed), and only a still-answering supervisor or a failed probe makes the error stand"
  - "In targeted mode a `handed_off` reply exits 0 with `already handed off to its successor; nothing stopped`; it is reported, never retried"
  - "recover --clean refuses on an ALIVE supervisor only, as the plan says; a probe error does not block it"
  - "A STALE teardown that signals nothing names `devflow resume --phase N` only on the bare stop; a targeted stop of a superseded launch does not call the phase resumable"

patterns-established:
  - "Scripted supervisor sockets in commands::tests (ScriptedSupervisor) for in-process stop tests without mutating the environment"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "stop ends a lock-free live run through its socket (V-17), with a control that advances without stop"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_ends_a_lock_free_live_run_through_the_socket"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#without_stop_a_lock_free_run_completes_and_advances"
        status: pass
    human_judgment: false
  - id: D2
    description: "stop on a corrupt state ends the run through its one sidecar-found socket, exits 0 and writes nothing; several sockets are refused with working --launch lines"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_ends_a_live_run_whose_state_will_not_load"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_refuses_several_sockets_when_the_state_will_not_load"
        status: pass
    human_judgment: false
  - id: D3
    description: "crash row C13: handed_off is retried against the reloaded record, at most 4 requests, then refused recording nothing"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::stop_retries_a_handed_off_reply_against_the_reloaded_record"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::stop_gives_up_after_three_handed_off_retries_and_records_nothing"
        status: pass
    human_judgment: false
  - id: D4
    description: "STALE goes through the 50-09 backstop (incl. R-10 gate kill and a supervisor dying under the shutdown); a probe error is refused; no pid read from state is signalled"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_tears_down_a_stale_supervisor_through_the_backstop"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_after_a_supervisor_killed_at_a_gate_reaps_and_reports_gone"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_never_signals_a_recorded_pid_it_addresses_the_socket"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_reports_a_probe_error_and_never_runs_the_backstop"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::stop_acts_on_a_supervisor_that_dies_under_its_shutdown_request"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::stop_reports_a_shutdown_closed_by_a_live_listener"
        status: pass
    human_judgment: false
  - id: D5
    description: "stop --launch <id> tears down only that socket (sidecar or file-name id, RV-05b), refuses a STALE socket without a usable sidecar, reaches a deleted root's orphan from any directory"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_with_a_launch_id_tears_down_only_that_socket"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#bare_stop_addresses_the_state_record_and_leaves_a_superseded_socket"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_with_a_launch_id_stops_a_live_socket_whose_sidecar_will_not_parse"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_with_a_launch_id_refuses_a_stale_socket_without_a_usable_sidecar"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_with_a_launch_id_reaches_a_deleted_roots_orphan_from_any_directory"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#stop_with_the_recorded_launch_id_runs_the_plain_stop"
        status: pass
    human_judgment: false
  - id: D6
    description: "recover --clean refuses a live run; the Stop help text and snapshot describe the new verb"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/stop_e2e.rs#recover_clean_refuses_while_the_supervisor_is_alive"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false

duration: 31min
completed: 2026-10-18
status: complete
---

# Phase 50 Plan 14: `devflow stop` through the supervisor socket Summary

**`devflow stop` now ends a run through its supervisor socket (shutdown when ALIVE, the validated backstop when STALE, the lock holder only when GONE), `--launch <id>` tears down one named socket from any directory, and `recover --clean` refuses a live run.**

## Needs awareness

- **`stop` closes 999.138, but two operator-facing texts outside this plan now describe the old `stop`.** `start`/`resume`'s live-run refusal (`start_live_run_refusal`) still says `devflow stop` "would only mark the state stopped: it signals nothing and does not end this run". For a run with a live supervisor that is now false. 50-15 owns those refusal builders and is planned to rewrite them. `cleanup`'s refusal still names `devflow resume` where `stop` is right (noted by 50-12); this plan does not touch that path either.
- **The Task 2 shell gate checks less than it seems to.** It scans only `stop`'s own body for `monitor_pid`, and `stop` is now a one-line wrapper around `stop_in`. I checked the whole stop path separately: lines 1984-2708 of `commands.rs` (19 functions) contain no `monitor_pid`. The gate exits 0 either way.
- **The `--launch` RED failures came from clap.** Four Task 3 tests failed at RED with `unexpected argument '--launch' found`. The flag was the missing feature, and each test got past its fixture preconditions before it failed. Still, that is a usage error, not an assertion about what `--launch` does. The hand mutants below are the evidence that these tests discriminate.

## Performance

- **Duration:** about 31 min
- **Started:** 2026-10-18T13:10:33Z
- **Completed:** 2026-10-18T13:41Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- **Bare `stop`.** It runs the unchanged gate path first. Then it probes the socket that `state.supervisor` names. If the state is missing or does not load, it uses the one live sidecar-found socket for this root and phase instead. What happens next depends on the probe:
  - ALIVE: `shutdown`.
  - STALE: `teardown_stale`.
  - GONE: the identity-confirmed lock holder.
  - Probe error: a refusal.
- **When `stop` refuses.** Several live sockets with no usable state is a refusal. It lists each one's `devflow stop --phase N --launch <id>` line, using the file-name id for an entry with no usable sidecar. A `handed_off` reply reloads the state and retries, 4 requests in all; when they run out, `stop` exits non-zero and records nothing.
- **Unloadable state.** A stop on a state file that does not load exits 0. It says `stopped` was not recorded and names the file. The state bytes are left untouched.
- **The 6d46008 race.** A shutdown request that errors is probed once more. A supervisor that died under the request reads STALE and goes through the backstop; it is not reported as a failed stop.
- **`--launch <id>`.** It selects the one socket whose sidecar carries the id. For a socket with no usable sidecar, it uses the id in the file name, and only when `sweep_for_in` attributes the socket to this root and phase. It refuses, changing nothing, in two cases:
  - the launch belongs to another phase;
  - the launch belongs to another project root that still exists.

  A deleted root's orphan is reachable from any directory. The state's own launch id runs the plain stop. Any other id acts on that socket only, and writes no state.
- **Other changes.** `recover --clean --phase N` refuses while the phase's supervisor is ALIVE. The `Stop` help text is rewritten, and the help snapshot changes on the `stop` line only.

## Task Commits

1. **Task 1: tracer, stop through the socket (V-17, C13, corrupt state)**: `5fc9485` (test, RED), `cc244ef` (feat, GREEN)
2. **Task 2: STALE backstop, R-10, renamed pid test, probe error**: `13b0237` (test, RED), `985f2e4` (feat, GREEN)
3. **Task 3: `--launch`, recover refusal, help text and snapshot**: `180ca2e` (test, RED), `b04612f` (feat, GREEN)
4. **Extra proof tests (deleted root, recorded launch id)**: `f4d0ad9` (test)

## Files Created/Modified

- `crates/devflow-cli/src/commands.rs`: the socket arm of `stop` (`stop_in`, `locate_phase_socket`, `stop_via_socket`, `shutdown_supervisor`, `stop_stale_socket`, `run_backstop`), the targeted `stop_launch` path, `refuse_a_live_supervisor` for recover, and the rewritten `stop` and `stop_via_lock` docs. Also the `commands::tests` C13 and race tests and the `ScriptedSupervisor` helper.
- `crates/devflow-cli/src/main.rs`: `Stop { launch: Option<String> }`, the dispatch and the rewritten help.
- `crates/devflow-cli/tests/stop_e2e.rs`: the `LiveRun` fixture plus 15 new or rewritten tests.
- `crates/devflow-cli/tests/snapshots/devflow-help.txt`: regenerated; one line changed.

## TDD Gate Compliance

RED evidence came from nextest JUnit through `gsd-tools check tdd-red-evidence`. The records are in `target/red-evidence/50-14-*.json` (gitignored).

| Test | RED commit | RED evidence | GREEN |
|------|-----|------|-------|
| `stop_ends_a_lock_free_live_run_through_the_socket` | `5fc9485` | exit 100, `RED_EVIDENCE_OK` | `cc244ef` |
| `stop_ends_a_live_run_whose_state_will_not_load` | `5fc9485` | exit 100, `RED_EVIDENCE_OK` | `cc244ef` |
| `commands::tests::stop_retries_a_handed_off_reply_against_the_reloaded_record` | `5fc9485` | exit 100, `RED_EVIDENCE_OK` | `cc244ef` |
| `commands::tests::stop_gives_up_after_three_handed_off_retries_and_records_nothing` | `5fc9485` | exit 100, `RED_EVIDENCE_OK` | `cc244ef` |
| `commands::tests::stop_acts_on_a_supervisor_that_dies_under_its_shutdown_request` | `5fc9485` | exit 100, `RED_EVIDENCE_OK` | `985f2e4` (needs the STALE arm) |
| `commands::tests::stop_reports_a_shutdown_closed_by_a_live_listener` | `5fc9485` | exit 100, `RED_EVIDENCE_OK` | `cc244ef` |
| `stop_tears_down_a_stale_supervisor_through_the_backstop` | `13b0237` | exit 100, `RED_EVIDENCE_OK` | `985f2e4` |
| `stop_after_a_supervisor_killed_at_a_gate_reaps_and_reports_gone` | `13b0237` | exit 100, `RED_EVIDENCE_OK` | `985f2e4` |
| `stop_with_a_launch_id_tears_down_only_that_socket` | `180ca2e` | exit 100, `RED_EVIDENCE_OK` | `b04612f` |
| `recover_clean_refuses_while_the_supervisor_is_alive` | `180ca2e` | exit 100, `RED_EVIDENCE_OK` | `b04612f` |
| `stop_refuses_several_sockets_when_the_state_will_not_load` | `180ca2e` | exit 100, `RED_EVIDENCE_OK` | `b04612f` |
| `stop_with_a_launch_id_stops_a_live_socket_whose_sidecar_will_not_parse` | `180ca2e` | exit 100, `RED_EVIDENCE_OK` | `b04612f` |
| `stop_with_a_launch_id_refuses_a_stale_socket_without_a_usable_sidecar` | `180ca2e` | exit 100, `RED_EVIDENCE_OK` | `b04612f` |
| controls and rewrites (see below) | in the RED commits | **passed at RED** | — |

**Semantic assessment.**
- **V-17 RED.** The preconditions held: the lock was free, the run was ALIVE and the agent group was running. Stop then printed `no lock held for phase 140 — nothing is running` and signalled nothing. That is the 999.138 defect.
- **Corrupt-state RED.** Stop exited 1 with `state JSON failed`. The old path wrote to a state it could not parse.
- **C13 RED.** A received 0 shutdown requests and stop returned `Ok`. The old stop never addressed a socket.
- **Race RED.** The scripted server timed out with `no probe connected`. After Task 1's GREEN, the test reached the re-probe and read STALE. It then failed on Task 1's interim STALE refusal, and went green with Task 2.
- **Task 2 RED.** Both tests failed on stop's STALE refusal, with every precondition met: the supervisor was SIGKILLed, the socket read STALE, and the group was alive or parked at the gate holding the lock. The first draft of R-10 asserted the bare string `devflow resume --phase N`, which the gate path's no-waiter message already prints. I tightened it to the R-8 sentence before committing RED.
- **Task 3 RED.** Four tests failed on clap's `unexpected argument '--launch'` (see Needs awareness). `recover_clean_refuses_while_the_supervisor_is_alive` failed on the real defect: `recover --clean` deleted a live run's state and printed `cleared anyway`.
- **Passed at RED, as expected.** These tests all passed at RED:
  - `without_stop_a_lock_free_run_completes_and_advances` (control). Its own negative control: with the trigger write removed, it fails after 60 s.
  - `bare_stop_addresses_the_state_record_and_leaves_a_superseded_socket` (control).
  - `stop_never_signals_a_recorded_pid_it_addresses_the_socket` and `stop_reports_a_probe_error_and_never_runs_the_backstop`. Task 1 already refused probe errors and fell back to the lock path only on GONE (deviation 2).

  Mutants M3 and M4 show these two tests discriminate.

## Mutation and Discrimination Evidence

I applied 15 hand mutants to `commands.rs` by script. Each ran its named target test or tests under nextest `--no-tests=fail`. After each one, the file was restored and compared byte for byte with `filecmp.cmp(shallow=False)`. `git status` showed `commands.rs` clean afterwards.

| Mutant | Target | Result |
|---|---|---|
| M1 deleted root refused like an existing one | deleted-root e2e | caught |
| M2 no project check for a parsed sidecar | deleted-root e2e (control half) | caught |
| M3 probe error falls through to the lock path | probe-error e2e | caught |
| M4 GONE signals the recorded agent group | recorded-pid e2e | caught |
| M5 handed_off retry reuses the first socket | C13 retry | caught |
| M6 `MAX_SHUTDOWN_REQUESTS` 4 → 5 | C13 cap | caught |
| M7 a shutdown error is never re-probed | race | caught |
| M8 STALE launch without a sidecar removed blind | V-40 | caught |
| M9 `--launch` ignores the file-name id | V-39 | caught |
| M10 recover never refuses | recover e2e | caught |
| M11 unloadable state still written | corrupt-state e2e | caught |
| M12 `--launch <recorded id>` skips the plain stop | recorded-launch e2e | **survived first**, caught after fix |
| M13 bare STALE never reaches the backstop | STALE e2e and R-10 | caught (both) |
| M14 several sockets: act on the first | several-sockets e2e | caught |
| M15 ALIVE skips the shutdown | V-17 | caught |

M12 first survived because the supervisor records `stopped` itself, so `state.stopped` could not tell the two paths apart. The test now asserts the plain stop's own reason, `stopped via \`devflow stop\``. M5's first draft did not compile, so I rewrote it; the table shows the rerun.

## Verification (commands run, actual exit codes)

- **Task 1 `<verify>`.** Run verbatim after GREEN, as the tracer gate: exit 0. Each of the five commands reported `1 test run: 1 passed`.
- **Task 2 `<verify>`.**
  - The four named tests each reported `1 passed`.
  - The shell gate exited 0. It is weaker than it looks; see Needs awareness.
- **Task 3 `<verify>`.**
  - Gate 1, run verbatim: exit 0. Seven commands, each `1 passed`.
  - Gate 2, the sentinel-HOME `scripts/check.sh all`, run verbatim: `check_all_exit=0 leaked_entries=0`, gate exit 0, `==> check.sh: all OK`, passing on the first run.
  - I did not re-test gate 2's failure direction. The plan's own `fails_when` note records that it was tested both ways.
- **Help snapshot diff.** Over this plan's commits, `devflow-help.txt` changes one line: the `stop` entry.
- **Wider regressions.**
  - `cargo nextest run -p devflow --test stop_e2e`: 36/36 by the end.
  - `cargo nextest run -p devflow --bin devflow`: 441 passed, 2 skipped, 2 marked leaky (pre-existing tests).
  - `recover_clean_e2e` and `supervisor_e2e`: 16/16.
  - `cargo clippy -p devflow --all-targets -- -D warnings`: exit 0.
- **Semver.** Not run: `devflow-core` is unchanged. `git diff --stat 8336fa8..HEAD` touches only `crates/devflow-cli/`.

## Decisions Made

See `key-decisions`. All four are within the plan's remit. They fill cases the plan's text leaves open.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] A supervisor dying under the shutdown request (carried-forward item from 6d46008)**
- **Found during:** Task 1
- **Issue:** `request_shutdown` reports a reset, a broken pipe or a refused connect as an error. On a supervisor that died between the probe and the shutdown, `stop` would have reported a failed stop.
- **Fix:** `shutdown_supervisor` probes once more on any shutdown error. STALE runs the backstop. GONE re-locates the run, which is how a handed-off successor gets reached rather than a false stop recorded. Anything else makes the error stand. `stop_one_launch` (targeted mode) does the same.
- **Verification:** `stop_acts_on_a_supervisor_that_dies_under_its_shutdown_request` uses 9481671's technique: the listener is dropped with the shutdown's connect still queued. Its control, `stop_reports_a_shutdown_closed_by_a_live_listener`, checks that a live listener closing the connection is an error. Mutant M7 is caught.
- **Committed in:** `5fc9485` (tests), `cc244ef` / `985f2e4`

**2. [Sequencing] Probe-error refusal and GONE → lock landed in Task 1, not Task 2**
- **Issue:** Task 1's arm needed some answer for every probe result. The plan places ProbeError in Task 2's action.
- **Effect:** Two Task 2 tests passed at RED: the renamed pid test and the probe-error test. Mutants M3 and M4 are their discrimination evidence. Task 1 refused STALE as an interim step, so Task 2's two STALE tests still failed at RED for the right reason.

**3. [Rule 3 - Hermeticity] `stop` split into `stop` / `stop_in(cache_dir)`**
- **Issue:** A bare stop on a missing state now sweeps the cache. Four existing `commands::tests` cases have no state file, so they would have swept and pinged the operator's real `~/.cache/devflow/sock`.
- **Fix:** Those four tests now call `stop_with_an_empty_cache`, which uses a tempdir cache. The C13 tests call `stop(root, phase)` as the plan says; their state loads, so no sweep happens.
- **Committed in:** `cc244ef`

**4. [Rule 1 - Accuracy] The no-lock message**
- **Fix:** `stop_via_lock`'s "no lock held for phase N — nothing is running `advance()`" now reads "no live supervisor socket and no lock held for phase N — nothing is running". That is the only state in which the message is reached now.
- **Committed in:** `cc244ef`

**5. [Rule 2 - Test gaps] Two more tests**
- `stop_with_a_launch_id_reaches_a_deleted_roots_orphan_from_any_directory`: the 50-12 hand-off asked for the deleted-root case to be proved end to end. This test runs `devflow doctor`'s printed repair line from an unrelated directory that holds no project state. In the same test, another existing project's orphan is refused and kept (T-50-53).
- `stop_with_the_recorded_launch_id_runs_the_plain_stop`: the plan's "the state's own launch runs the plain stop" rule had no test.
- **Committed in:** `f4d0ad9` (after GREEN, so it has no RED commit of its own; mutants M1, M2 and M12 are its evidence)

**6. [Hygiene] Commit `180ca2e`'s subject is 75 characters**
- The `commit-msg` hook only warned. I did not amend it, as instructed.

**7. [Orchestrator-directed] Tracking**
- STATE.md, ROADMAP.md and the REQUIREMENTS.md checkbox were not touched. `SUPV-01` is also declared by later plans. `pin-guard.sh` passed before the first edit and before every commit. `.planning/user/errors/state-stale.json` was left uncommitted.

---

**Total deviations:** 4 auto-fixed (2 Rule 2, 1 Rule 3, 1 Rule 1), plus 1 sequencing note and 2 process notes.
**Impact on plan:** No public `devflow-core` change. The CLI surface gains the planned `--launch` flag only.

## Issues Encountered

- A pre-existing leaky `devflow-core` unit test left one `sleep 30` (pid 3367759) after `check.sh all`. Its environ showed `CARGO_MANIFEST_DIR` in this worktree. By the time I signalled its recorded pid, it had already exited on its own (`kill` returned 1, no such process).

## Known Stubs

None. Task 1's interim STALE refusal and the `#[expect(dead_code)]` on `LocatedSocket::record` were both replaced in `985f2e4`.

## Cleanup

- **Processes:** none left. I checked with `ps` after every e2e and mutant run for `__monitor`, the fake agent loop (`sleep 0.05`), `sleep 60` and `sleep 30`.
- **Directories:**
  - No `/tmp/dfs*`, `/tmp/dfh*`, `/tmp/dfu*` or `/tmp/p50home.*` directory remains.
  - The operator cache count is unchanged at 1043 entries, and `~/.cache/devflow/sock` is empty.
- **Kept on purpose:**
  - The RED evidence records under `target/red-evidence/50-14-*`, inside the gitignored build directory.
  - The scratch directory `exec-50-14/`, which holds the RED and mutant scripts and the check log. I remove it after this SUMMARY is committed.

## User Setup Required

None.

## Next Phase Readiness

- **50-15 (launch guard).**
  - `start_live_run_refusal`'s description of `stop` is now stale (see Needs awareness).
  - `locate_phase_socket` and `several_sockets_refusal` are crate-private helpers 50-15 can reuse for the guard's sidecar search.
- **50-16** deletes `State::monitor_pid`. Nothing in the stop path reads it.
- **50-17 (OPERATIONS)** should document:
  - the stop arms;
  - `--launch <id>`, which writes no state, answers no gate and signals no lock holder;
  - the unloadable-state message;
  - recover's live refusal.
- **50-23:** T-50-75 is unchanged. A targeted stop that reaches N while N is `Advancing` is forwarded by N.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-18*

## Self-Check: PASSED

- FOUND: the four modified files.
- FOUND: all 7 task commits (`5fc9485`, `cc244ef`, `13b0237`, `985f2e4`, `180ca2e`, `b04612f`, `f4d0ad9`) are ancestors of HEAD.
- Ledger: `git rev-list --count 8336fa8..HEAD` = 7, which matches `commits: 7`.
