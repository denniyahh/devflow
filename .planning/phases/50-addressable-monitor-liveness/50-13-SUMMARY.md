---
phase: 50-addressable-monitor-liveness
plan: 13
subsystem: cli
tags: [rust, supervisor, unix-socket, handoff, stop, safe-point, gate, liveness]

requires:
  - phase: 50-11
    provides: run_monitor's in-process advance, the supervisor record publish, the chain-flag guard ordering
  - phase: 50-07
    provides: StopToken (safe_point, enter, stop_requested), OwnerPhase, finish's stopped/handed_off final line
  - phase: 50-24
    provides: the DEVFLOW_TEST_HOLD doc-check exception
provides:
  - "StopTokenGuard: a thread-local slot holding the supervisor's stop token and the successor its advance published (record_successor / take_successor)"
  - "run_monitor(invocation, advance): marks the supervisor handed off only after the advance returned and only when a successor was recorded; supervise_monitor folded in"
  - "Advance safe points 1 (after load_state), 2 (before the next spawn), 3 (after the next ping, before publish: shuts the next supervisor down) and 4 (the gate wait); advance_stopped event; ADVANCE_STOP_REASON"
  - "advance_with refuses a stopped state: advance_failed {reason: stopped} and an error naming devflow resume --phase N"
  - "record_supervisor_stop writes only when the state names this launch (lockless check, then under the lock), never fails before finish, lock wait bounded at 2 x TERMINATE_VERIFY_WAIT + 5 s"
  - "Gates::poll_response_cancellable (additive pub fn in devflow-core)"
  - "wait_for_gate_response / GateWaitEnd: GateWait owner phase around a cancellable gate poll inside a supervisor's advance"
  - "cfg(test) test_hold points before_spawn, before_publish, before_record_save, after_publish (inside an advance only; writes <path>.held)"
  - "--until stop clears state.supervisor beside monitor_pid"
affects: [50-14, 50-15, 50-16, 50-22, 50-23]

actuals:
  tokens: 20400   # chars/4 over `git diff ae49ed5..723fe3e` (81599 chars, context lines included)
  tasks: 3
  commits: 6
plan_head_before: ae49ed5c44de4113ecfa9b74bdad0a42b00f3811
plan_head_after: 723fe3e698dee42604366975753dd97f9099c59c

tech-stack:
  added: []
  patterns:
    - "Thread-local RAII guard carries a supervisor's stop token into the advance it runs (no process-global static, no parameter threading)"
    - "A held-advance test fixture: DEVFLOW_TEST_HOLD=<point>:<path> plus a <path>.held marker, so a test lands a stop, signal or kill in a chosen window and knows it is there"
    - "Stops taken over the raw wire (accepted line, then act, then the final line) when the test must act at a known point after the supervisor took the stop"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/test_support.rs

key-decisions:
  - "A stop at the gate wait records stopped and unwinds through an error, not GateAction::Abort: abort() deletes the state file, which contradicts V-15's state.stopped and R-2's 'state marked stopped'"
  - "test_hold fires only inside a supervisor's advance (a stop token engaged), because the hold variable is inherited by the test's own first launch, which would otherwise block on it"
  - "run_monitor takes the supervise-only flag directly so the test harness entry runs run_monitor itself, where the RV-04 scan reads the handed-off marking"
  - "Safe point 3 shuts the unpublished next supervisor down before it records the stop; the next supervisor writes nothing because the state does not name it"
  - "Every safe-point stop emits advance_stopped with the safe point number, so tests assert which point acted"

patterns-established:
  - "A supervisor records its own stop only when state.supervisor names its launch id"

requirements-completed: []  # SUPV-01 is also declared by later 50-xx plans; requirements.ready-ids returned 0/1 ready

coverage:
  - id: D1
    description: "A stage handoff publishes the next supervisor before the old one unlinks; the old one stays Advancing until its advance returns (V-13, RV-04)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_handoff_observer_detects_a_pre_clear_gap"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_handed_off_marking_stays_in_run_monitor_after_the_advance"
        status: pass
    human_judgment: false
  - id: D2
    description: "A stop during an advance is honoured at the next safe point: nothing launched, state stopped, lock released; at safe point 3 the unpublished next supervisor is shut down without a lock deadlock"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stop_during_advance_stops_at_the_next_safe_point_and_launches_nothing"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::without_a_stop_the_held_advance_launches_the_next_stage"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stop_between_the_next_spawn_and_its_publish_shuts_the_next_supervisor_down"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#sigterm_to_a_live_supervisor_ends_the_run_without_advancing"
        status: pass
    human_judgment: false
  - id: D3
    description: "advance_with refuses a stopped state (999.138 residual)"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::advance_refuses_a_stopped_state"
        status: pass
    human_judgment: false
  - id: D4
    description: "A stopped supervisor whose state will not load still answers stopped and writes nothing (T-50-68)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stopped_supervisor_whose_state_will_not_load_still_answers_stopped"
        status: pass
    human_judgment: false
  - id: D5
    description: "SIGTERM during a gate wait ends the wait within about a second: lock released, state stopped, socket gone (V-15); poll_response_cancellable"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::poll_response_cancellable_returns_abort_within_a_second_of_cancel"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::poll_response_cancellable_returns_the_response_when_not_cancelled"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_signal_during_a_gate_wait_aborts_and_releases_the_lock"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::without_a_signal_the_gate_wait_holds_until_answered"
        status: pass
    human_judgment: false
  - id: D6
    description: "A SIGKILL of N between N+1's publish and N's unlink leaves state on the live N+1 and N's socket Stale (R-10)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor"
        status: pass
    human_judgment: false
  - id: D7
    description: "The --until stop clears state.supervisor with monitor_pid"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::until_stop_clears_the_supervisor_record_with_the_monitor_pid"
        status: pass
    human_judgment: false

duration: 36min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 13: Safe Stop and Handoff of the In-Process Advance Summary

**A supervisor now hands its stage to the next one by publishing the successor before it unlinks itself, and a stop or SIGTERM that lands mid-advance or at a gate ends the run at a safe point with the lock released and nothing launched.**

## Needs awareness

- **The gate-wait stop does not use `GateAction::Abort`, as the plan said.** `abort()` deletes the state file. The plan's own V-15 truth needs `state.stopped` to be readable afterwards, so the two could not both hold. The gate wait now clears the gate, records `stopped` (safe point 4) and returns an error. Every caller of `run_gate`/`run_gate_with_timeout` uses `?`, so that error unwinds the advance and drops the per-phase lock. The result: the `__monitor` process exits 1 with "the gate wait for stage X ended: stopped through the supervisor during advance". Production sends the monitor's stderr to `/dev/null`, so nobody sees that line.
- **`record_supervisor_stop`'s "not recorded" warnings are invisible in production.** The plan asks for the reason to be logged to stderr, and it is (`warn!`). The `__monitor` process's stderr is `/dev/null`, though, so an operator only learns of it from the stop caller (50-14 prints its own message).
- **`scripts/check.sh all` passed on the third run, not the first.** The plan's gate is a sentinel-HOME `check.sh all`, and two earlier runs at the same HEAD failed on three tests this plan does not touch. Each of those tests passed when run alone (6/6, 8/8 and 8/8). All three are logged in `deferred-items.md`: `auto_rescan_rejection_parks_for_supervised_repair` (a check-then-act race in the test's writer thread), and two probes of a just-killed socket that read `Connection reset by peer` instead of `Stale`. This plan's own changes are not shown to cause any of them, but the suite is not reliably green under load.
- **The gate poll now checks the response file every second for the whole wait.** Inside a supervisor's advance, the sleeps in `poll_response_cancellable` are capped at 1 s, as the plan specified. The old backoff reached 60 s. A multi-day gate therefore reads one small file once a second. Outside a supervisor (the hidden `devflow advance`, a foreground gate), the unchanged `poll_response` runs.

## Performance

- **Duration:** about 36 min
- **Started:** 2026-10-17T21:10:07Z
- **Completed:** 2026-10-17T21:46Z
- **Tasks:** 3
- **Files modified:** 5 source files (plus `deferred-items.md`)

## Accomplishments

- **Handoff (R-1, V-13).** Stage N saves N+1's record under its lock and stores it as its successor. N stays `Advancing`. Only after `advance_for_stage` returns does `run_monitor` take the successor, enter `OwnerPhase::HandedOff` and call `finish()`, which unlinks N's socket.
- **Safe points 1–3 (R-2).** Each calls `StopToken::safe_point()` through the thread-local guard. On a requested stop it records `stopped` with `ADVANCE_STOP_REASON`, emits `advance_stopped {safe_point}` and returns. Safe point 3 also sends `shutdown` to the unpublished N+1 (bounded at 11 s) and never saves N+1's record.
- **Lock-skip rule (T-50-54).** A stopped supervisor records its stop only when the state names its own launch id. It checks once without the lock and again under it. An unpublished N+1 and a superseded supervisor both write nothing. The lock wait went from 30 s to `2 × TERMINATE_VERIFY_WAIT + 5 s`. No error in this branch returns before `finish()`.
- **Stopped-state refusal (999.138).** `advance_with` emits `advance_failed {reason: "stopped"}` and returns an error naming `devflow resume --phase N`.
- **Cancellable gate wait (V-15).** `Gates::poll_response_cancellable` is new and additive; `poll_response` is byte-unchanged. Inside a supervisor's advance, the gate wait enters `GateWait`, polls with the stop token as the cancel, and returns to `Advancing` afterwards.
- **`--until` clear.** `state.supervisor = None` is now set next to `monitor_pid = None`.

## Task Commits

1. **Task 1: tracer, publish before unlink**: `28e0351` (test, RED), `8cf5a1b` (feat, GREEN)
2. **Task 2: safe points and stopped-state refusal**: `ed26076` (test, RED), `d6e3de4` (feat, GREEN)
3. **Task 3: cancellable gate wait, R-10 fixture, `--until` clear**: `dc8512c` (test, RED), `723fe3e` (feat, GREEN)

**Plan metadata:** the docs commit that adds this SUMMARY.

Tracer gate (Task 1, interactive, `end-of-phase`, automated-only `<verify>`): I re-ran the verify after GREEN and it passed, so the remaining tasks went ahead.

## TDD Gate Compliance

| Test | RED commit | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------|-------|
| `stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked` | `28e0351` | exit 100, 1/1 failed, `RED_EVIDENCE_OK target_test_failed` | `8cf5a1b` |
| `the_handed_off_marking_stays_in_run_monitor_after_the_advance` | `28e0351` | exit 100, `RED_EVIDENCE_OK target_test_failed` | `8cf5a1b` |
| `a_stop_during_advance_stops_at_the_next_safe_point_and_launches_nothing` | `ed26076` | exit 100, `RED_EVIDENCE_OK target_test_failed` | `d6e3de4` |
| `a_stop_between_the_next_spawn_and_its_publish_shuts_the_next_supervisor_down` | `ed26076` | exit 100, `RED_EVIDENCE_OK target_test_failed` | `d6e3de4` |
| `advance_refuses_a_stopped_state` | `ed26076` | exit 100, `RED_EVIDENCE_OK target_test_failed` | `d6e3de4` |
| `a_signal_during_a_gate_wait_aborts_and_releases_the_lock` | `dc8512c` | exit 100, `RED_EVIDENCE_OK target_test_failed` | `723fe3e` |
| `gates::tests::poll_response_cancellable_returns_abort_within_a_second_of_cancel` | `dc8512c` | exit 100, `RED_EVIDENCE_OK target_test_failed` | `723fe3e` |
| controls, `a_stopped_supervisor_whose_state_will_not_load_still_answers_stopped`, the R-10 kill test | in the same RED commits | **passed at RED** (see below) | — |

The evidence records are in `target/red-evidence/50-13-t*.json` (gitignored). The JUnit target names are the full `pipeline_launch::tests::…` path, which the classifier matches as `devflow::bin/devflow#<path>`.

**Semantic assessment.**
- **V-13 RED.** The chain itself worked: the child's events show `transition define→plan` and a `stage_launched` for plan. The test then failed on `the after_publish hold was never reached`, because there was no hold and N never parked. Before GREEN, N was marked handed off nowhere, and nothing kept it alive past the save.
- **Scan RED.** The planted controls passed. The scan then reported the real file's two violations: `spawn_agent_and_record never calls record_successor` and `run_monitor not found` (it was a one-line wrapper).
- **Safe points 2 and 3 RED.** N accepted the stop with owner `advancing`. Its final line was then `{"ok":false,"error":"handed_off"}`, meaning the flagged stop was ignored and the advance launched plan. This is the defect 50-11 reported.
- **Refusal RED.** `advance_with` returned `Ok(())` on a stopped state.
- **V-15 RED.** My first V-15 draft failed on a precondition: owner `gate_wait` was required before the signal. That is the wrong reason, so I moved the owner check into the control and re-ran it. It then failed on `within 5 s of SIGTERM the lock is released and the socket gone`. The committed RED has the revised test.
- **Cancel RED.** The RED scaffold ignored `cancel` and returned 56.8 s after it.
- **Passed at RED, as expected.** The two controls pass by design. `a_stopped_supervisor_whose_state_will_not_load_still_answers_stopped` also passed, because 50-11's `run_monitor` already called `finish()` after the match even when the stop record failed. The R-10 kill test passed too, because Task 1's GREEN built the publish order it checks. Both are discriminating; mutants M7 and M10 below are their evidence.

## Mutation and Discrimination Evidence

For each mutant I applied it, ran the named test, then restored the file from a saved copy and confirmed the restore with `cmp`. Every restore matched.

| Mutant | Test | Result |
|---|---|---|
| M1: pre-clear `state.supervisor = None` + save before the spawn (WR-04 pitfall) | V-13 | caught: `state read GONE or no record across the handoff: [NoRecord ×4]` |
| M2: `spawn_agent_and_record` enters `HandedOff` after the save | V-13 / RV-04 scan | both caught: `left: "handed_off" right: "advancing"`; `["spawn_agent_and_record enters the handed-off phase"]` |
| M3: the stopped branch always takes the lock and writes (no launch-id check) | safe point 3 test | caught: `N's stop took 11.08s, not under 6s: a lock deadlock between N and N+1` |
| M4: safe point 3 skips the N+1 shutdown | safe point 3 test | caught: `N+1 was shut down through its socket` |
| M5: safe point 2 removed | safe point 2 test | caught: `advance_stopped` reports `safe_point` 3, not 2 |
| M6: a load error returns `Err` before `finish()` | corrupt-state test | caught: `NoReply("no final shutdown line: connection closed before a line arrived")` |
| M7: the `--until` branch does not clear `state.supervisor` | `until_stop_clears_the_supervisor_record_with_the_monitor_pid` | caught |
| M8: the gate wait never enters `GateWait` | V-15 control | caught: `control: N still answers gate_wait` |
| M9: `after_publish` hold moved before the record save | R-10 kill test | caught: `the state names N+1` (left == right, N's id) |
| M10: no `test_hold` (RED tree) | V-13 | caught (the RED run) |

What this does not establish:
- Each new test passed in a handful of runs (V-13 about 5, the others 2–4). That shows the paths work. It does not show they are free of timing flakes. The observer's 5 ms poll, the 6 s safe-point-3 bound and the 5 s V-15 bound are the timing-sensitive parts.
- The V-13 observer can miss a gap shorter than 5 ms. The control shows it sees an 80 ms one. The deterministic proof is the check at the `after_publish` hold.
- I did not run cargo-mutants. The mutants above are hand-picked.

## A6: every `advance_with` / `advance_for_stage` caller

- `run_monitor` (the supervisor's own advance after `AgentOutcome::Completed`). A stopped state here means the run was stopped behind the supervisor (`devflow stop`, a parked repair, an earlier `--until`), and refusing is the point of 999.138.
- `advance()` → `advance_for_stage(.., None)`. Its callers are `pipeline_gate.rs` tests (lines ~780, ~828, ~1209), all on unstopped states.
- `main.rs` `Command::Advance`, the hidden `devflow advance`. It is the subject of 999.138. Its e2e callers (`gate_sweep_e2e`, `gate_wedge_e2e`, `gate_notify_persist_e2e`, `stop_e2e`) all build unstopped states.
- About 30 in-crate tests in `pipeline_launch.rs` call `advance_with` / `advance_for_stage` on unstopped states. All 437 bin tests passed after the refusal landed.
- `resume` clears `stopped` before it launches (`pipeline_launch.rs`, `if state.stopped { state.stopped = false; …`), so no resume path reaches an advance with `stopped` set.

No caller advances a stopped state legitimately.

## Semver Gate

`semver-gate.sh check --through 50-11`: `SEMVER verdict=match`, exit 0. I ran it before the RED commit that added the pub fn (`dc8512c`) and again at HEAD `723fe3e`. The only devflow-core change is the additive `Gates::poll_response_cancellable`. `tools/semver-gate.sh` was not edited. Over this plan's commits, `gates.rs` shows only added lines: `git diff 28e0351^..HEAD -- crates/devflow-core/src/gates.rs` contains no `-` line, and its stat reads `107 insertions(+)` with no deletions.

## Verification (commands run, actual exit codes)

- Task 1 gate 1 (V-13 + observer control) exited 0, and so did Task 1 gate 2 (the RV-04 scan). Each test reported `1 test run: 1 passed`.
- Task 2 gate (five bin tests and the 50-11 SIGTERM e2e) exited 0, with 6 × `1 passed`.
- Task 3 gate 1 (two `devflow-core` gate tests, V-15 pair, R-10) exited 0, with 5 × `1 passed`. All of these ran at HEAD `723fe3e`.
- **Task 3 gate 2 (sentinel-HOME `scripts/check.sh all`).** Claude Code's safety check refused the plan's exact bytes because they contain `rm -rf` inside a `bash -c` script, so they were not run. I ran the same check without the trap removal: `env -u DEVFLOW_CACHE_DIR -u XDG_CACHE_HOME -u XDG_CONFIG_HOME … HOME=<mktemp -d /tmp/p50home.XXXXXX> scripts/check.sh all`, then `find <home> -path "*devflow*" \( -type f -o -type s \)`. Afterwards I removed the two sentinel dirs by their literal paths.
  - Run 1 is void as a leak check. Its sentinel was under my scratchpad, and that path itself contains "devflow", so the find counted 59 entries. It does show the find matches files. Its `check.sh` exited 101: `auto_rescan_rejection_parks_for_supervised_repair` failed.
  - Run 2: `check_all_exit=101 leaked_entries=0`. `status_reports_each_liveness_state_with_its_repair_verb` and `pidfd_group_signal_is_used_when_the_kernel_supports_it` failed.
  - Run 3: `check.sh: all OK`, `check_all_exit=0 find_exit=0 leaked_entries=0`. The libtest summary lines across 39 binaries total 1588 passed, 0 failed, 3 ignored. A second count, of `test … ok` lines, gives 1584. I did not resolve the 4-line difference; captured child output interleaved on a test line is one reading.
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0 before each GREEN commit.
- **Operator cache (read-only count).** `~/.cache/devflow` held 1041 files and sockets before the plan and 1041 after, and `sock/` held 0 both times.

## Files Created/Modified

- `crates/devflow-cli/src/pipeline_launch.rs` changed in these places:
  - added `StopTokenGuard`, `with_stop_token`, `record_successor`/`take_successor`, the safe-point helpers, `shut_down_unpublished`, `wait_for_gate_response`/`GateWaitEnd` and `test_hold`;
  - folded `supervise_monitor` into `run_monitor(invocation, advance)`;
  - added safe point 1 and the stopped-state refusal in `advance_with`, and safe points 2 and 3 plus the four hold points in `spawn_agent_and_record`;
  - rewrote `record_supervisor_stop`;
  - added the 13 plan tests and their fixtures.
- `crates/devflow-cli/src/pipeline_gate.rs`: the gate wait goes through `wait_for_gate_response`, with the `Stopped` arm (safe point 4); the `--until` stop clears `state.supervisor`; and one new test.
- `crates/devflow-core/src/gates.rs`: `Gates::poll_response_cancellable` and two tests.
- `crates/devflow-cli/src/main.rs`: `run_monitor(&…, true)`.
- `crates/devflow-cli/src/test_support.rs`: the harness entry calls `run_monitor(&invocation, advance)`.
- `.planning/phases/50-addressable-monitor-liveness/deferred-items.md`: two out-of-scope flaky-test entries.

## Decisions Made

See `key-decisions` in the frontmatter. All were made under Claude's remit. The `Abort` mapping is a deviation; see below.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug in the plan's mechanism] Gate-wait stop does not map to `GateAction::Abort`**
- **Found during:** Task 3
- **Issue:** Every `GateAction::Abort` arm calls `abort()`, which calls `workflow::clear_state` and deletes the state file. A cancel mapped to Abort would therefore leave nothing to read `stopped` from. That contradicts the plan's V-15 assertion (`state.stopped` true), R-2's "the state is marked stopped" and the reason a stopped run keeps its state (`devflow resume`).
- **Fix:** On `GateWaitEnd::Stopped`, `run_gate_with_timeout` does four things: it cleans up the gate files (as `abort()` does, CR-01), sets `gate_pending = false`, keeps an on-disk `gate_notify_cmd` edit (WR-10) and calls `record_advance_stop(state, 4)`. It then returns `Err`, and every caller's `?` unwinds the advance and releases the lock.
- **Files modified:** `pipeline_gate.rs`, `pipeline_launch.rs`
- **Verification:** V-15 and its control; mutant M8.
- **Committed in:** `723fe3e`

**2. [Rule 3 - Blocking] `run_monitor` takes the supervise-only flag; `main.rs` and `test_support.rs` touched**
- **Found during:** Task 1
- **Issue:** The test harness entry ran `supervise_monitor`, not `run_monitor`, and `run_monitor` was a one-line wrapper. The RV-04 scan requires the handed-off marking and `take_successor(` in `run_monitor`'s own body, with a 10-line floor. In-crate tests must also exercise that body.
- **Fix:** `run_monitor(invocation, advance)` now holds the body. `main.rs` passes `true` and the harness passes its flag. Both files are outside `files_modified`.
- **Committed in:** `8cf5a1b`

**3. [Rule 3 - Blocking] `test_hold` fires only inside an advance, and writes `<path>.held`**
- **Found during:** Task 1
- **Issue:** The child test process launches stage N itself through `spawn_agent_and_record`. It has `DEVFLOW_TEST_HOLD` in its environment, so that N inherits it, which means the test's own first launch would block on the hold. A test also needs to know that N reached the hold before it sends a stop.
- **Fix:** The hold is a no-op unless a stop token is engaged. When reached, it writes `<path>.held`.
- **Committed in:** `8cf5a1b`

**4. [Rule 2 - Observability] `advance_stopped` event**
- Each safe-point stop emits `advance_stopped {stage, safe_point}`. Tests assert the point number (mutant M5).
- **Committed in:** `d6e3de4`

**5. [Scope note] One extra test**
- `pipeline_gate::tests::until_stop_clears_the_supervisor_record_with_the_monitor_pid`. The plan names no test for the `--until` clear (mutant M7).
- **Committed in:** `723fe3e`

**6. [Process] RED scaffolds**
- Task 1 GREEN carried `#[cfg_attr(not(test), expect(dead_code))]` on the token field and `with_stop_token`, because nothing in non-test code read them until Task 2. Task 2 RED added `ADVANCE_STOP_REASON` with the same attribute so its tests compiled. Task 3 RED added `poll_response_cancellable` as a scaffold that ignores `cancel`. Each GREEN commit removed or replaced its scaffold. `expect` (not `allow`) makes a forgotten scaffold a compile error.

**7. [Process] The plan's sentinel-HOME gate bytes were not executed**
- See Verification. Claude Code's safety check refused the `rm -rf` inside `bash -c`. The equivalent run used `mktemp -d /tmp/p50home.XXXXXX` and a literal-path removal afterwards.

---

**Total deviations:** 3 auto-fixed (1 Rule 1, 2 Rule 3), 1 Rule 2 addition, and 3 scope/process notes.
**Impact on plan:** The planned public API is unchanged (one additive pub fn). The gate-cancel mechanism differs from the plan's wording; its outcome matches the plan's truths.

## Issues Encountered

- A RED run of V-15 left its N parked at the gate until the test's 120 s `DEVFLOW_GATE_TIMEOUT_SECS`. I checked its pid until it had exited on its own; nothing was killed by name.
- Pre-existing leaky unit tests left `sleep 30` children during the full runs. I watched them by pid until each exited on its own at 30 s, and killed none.

## Cleanup

- No `__monitor` / `monitor_entry*` process, fake agent (`sleep 0.05` loop) or `sleep 30` remains (checked with `ps` after the last run).
- No `/tmp/dfs*`, `dfh*`, `dfu*` or `p50home*` dir remains. The two sentinel HOME dirs were removed by literal path.
- `~/.cache/devflow/sock` is empty, and the operator cache count is unchanged at 1041.
- The RED evidence JSON stays under `target/red-evidence/50-13-*` (gitignored). The scratch dir `exec-50-13` is removed after this SUMMARY is committed.

## User Setup Required

None.

## Next Phase Readiness

- 50-14 / 50-15 can rely on three behaviours. `request_shutdown` reads `Stopped` on a supervisor whose state does not load. A published supervisor records `stopped` itself. An unpublished or superseded one writes nothing.
- 50-23 builds on several pieces that are already in place. `before_publish`, safe point 3 and the lock-skip rule are as specified. `record_successor` / `take_successor` sit in the thread-local slot. The `HandedOff` marking and `take_successor()` are in `run_monitor`'s own body, right after `advance_for_stage` returns. 50-23 inserts its `before_handoff_mark` hold and post-marking stop check there.
- 50-07's untested cases are now covered: a signal during `Advancing` (safe points 1–3) and during `GateWait` (V-15). A `Supervisor` dropped without `finish()` is still not tested here.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

All seven commits (`28e0351`, `8cf5a1b`, `ed26076`, `d6e3de4`, `dc8512c`, `723fe3e`, `f909aba`) are ancestors of HEAD, and this SUMMARY exists on disk.
