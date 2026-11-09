---
phase: 50-addressable-monitor-liveness
plan: 26
subsystem: monitor
tags: [rust, supervisor, stream-json, waitid, pipes, wr-03, gap-closure]

requires:
  - phase: 50-addressable-monitor-liveness
    provides: "50-20's supervisor runtime (StopToken owner phases, wait_for_exit_without_reaping, safe points) and the 50-08 child-process stage harness"
provides:
  - "drive_stream_io ends at the agent's exit (exit-watch thread, waitid WNOWAIT), not only at stdout EOF"
  - "run_supervised_stage waits at most READER_DRAIN_WAIT (3 s) for its stream threads, closes the capture at the bound and records a stream_wait_bounded event"
  - "test-support hold DEVFLOW_TEST_STREAM_WAIT_HOLD (allowlisted, pinned test-only by source scan G)"
  - "core tests A, B, C, D, F, G and e2e tests E1, E2, E3 on the real __monitor binary"
  - "tools/wr03-revert-check.sh (run / recover)"
affects: [50-27, 50-28, 50-29, criterion-4, live-attempt-2]

actuals:
  tokens: 17416
  tasks: 2
  commits: 5
plan_head_before: 30b2d21fab748e6f84925e5551b161b4c33e311b
plan_head_after: 60b1538e52c47fe1a4dd9e93ae97a95c19cad694

tech-stack:
  added: []
  patterns:
    - "One StreamEvent channel (Line / Eof / AgentExited) instead of a line channel, so the loop can end on process exit"
    - "Bounded thread join: is_finished() polling, then detach by dropping the JoinHandle; shared Mutex<Option<File>> taken with a bounded try_lock"
    - "Holder record written by the fake agent itself (<pid> <start> from /proc/$!/stat) so the test guard signals only a verified process"

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/tools/wr03-revert-check.sh
  modified:
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/supervisor/runtime.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - doc-check-allowlist.toml
    - ARCHITECTURE.md

key-decisions:
  - "RED evidence for A, C, F recorded as nextest JUnit XML through a scratch --config-file profile and classified RED_EVIDENCE_OK by gsd-tools check tdd-red-evidence (not required by the plan; done for the TDD protocol)"
  - "Source scan G also rejects a definition gated by cfg(test) only (an extra planted control beyond the plan's four)"

patterns-established:
  - "Fail-first assertions carry a WR-03: message; fixture failures FIXTURE:; E2's setup failure HOLD-UNREACHABLE:"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "A completed stream run whose agent left a process holding its stdout returns at the agent's exit plus at most 3 s, under the production 120 s idle timeout, with one stream_wait_bounded event"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_completed_stream_run_returns_while_a_leftover_process_holds_the_agents_stdout"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_completed_stream_run_returns_once_its_leftover_process_released_the_agents_stdout"
        status: pass
    human_judgment: false
  - id: D2
    description: "No line reaches the capture after the bound; a leftover that keeps writing neither blocks the return nor gets SIGPIPE while the supervisor lives"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::no_line_reaches_the_capture_after_the_stdout_wait_expires"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_stream_run_returns_while_a_leftover_process_keeps_writing_to_the_agents_stdout"
        status: pass
    human_judgment: false
  - id: D3
    description: "A stop with the agent's stdout held outside the agent's group finishes within the stop's own wait (Stopped 143, advance suppressed, socket gone)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::a_stopped_stream_run_returns_while_a_process_outside_the_agent_group_holds_its_stdout"
        status: pass
    human_judgment: false
  - id: D4
    description: "On the real devflow __monitor binary a completed run with a held stdout reaches its --until halt, and devflow stop sent while advancing ends it at safe point 1"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_completed_stream_run_reaches_its_advance_while_a_leftover_process_holds_stdout"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_completed_stream_run_reaches_its_advance_once_its_leftover_process_released_stdout"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_stop_sent_while_a_completed_run_waits_on_a_held_stdout_ends_it_at_safe_point_1"
        status: pass
    human_judgment: false
  - id: D5
    description: "Every new non-control test discriminates: with the fix commit reverted, A C F D E1 E3 fail at WR-03 and B E2 pass"
    verification:
      - kind: other
        ref: "bash .planning/phases/50-addressable-monitor-liveness/tools/wr03-revert-check.sh run"
        status: pass
    human_judgment: false
  - id: D6
    description: "The stream-wait hold is test-only and read literally; no devflow-core pub change; macOS compiles"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::the_stream_wait_hold_is_test_only_and_read_literally"
        status: pass
      - kind: other
        ref: "semver-gate.sh check --through 50-16 (SEMVER verdict=match); cargo check --target aarch64-apple-darwin for both packages"
        status: pass
    human_judgment: false

duration: 28min
completed: 2026-10-21
status: complete
---

# Phase 50 Plan 26: WR-03 Stream Wait Fix Summary

**A stream supervisor now ends its stream at the agent's exit (exit-watch thread, `waitid(WNOWAIT)`) and waits at most 3 s for its pipe threads, so a process left holding the agent's stdout no longer keeps a completed or stopped supervisor alive; `devflow stop` sent while it is `advancing` ends the run at safe point 1 on the real binary.**

## Performance

- **Duration:** 28 min
- **Started:** 2026-10-21T19:46:36Z
- **Completed:** 2026-10-21T20:14:49Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- `drive_stream_io` takes one `StreamEvent` channel. An exit-watch thread sends `AgentExited` once the agent is a zombie (never reaps; an error such as ECHILD counts as exited). The reader tees through a shared `Mutex<Option<File>>`, keeps reading after the loop ends, and discards once the capture is taken. Nothing is signalled.
- `StreamThreads::join_within(READER_DRAIN_WAIT)` (= `agent::TERMINATE_VERIFY_WAIT`, 3 s) replaces the unbounded join for both outcomes. At the bound it takes the capture handle with `try_lock`, retried for at most one more wait. It detaches unfinished threads, and `finish_stream_threads` emits `stream_wait_bounded` (`stage`, `waited_ms`, `unfinished`, `capture_closed`).
- The test-support hold `stream_wait_test_hold` parks a stream supervisor after its exit file and before the thread wait. It is released by its release file or by a stop request. G pins it test-only, and `doc-check-allowlist.toml` names its variable.
- E1 to E3 drive the real `devflow __monitor` over a seeded Code-stage state (`--until code`). The revert check proves that A, C, F, D, E1 and E3 fail at `WR-03:` without the fix, and that B and E2 pass.

## Task Commits

1. **Task 1 step 1: test-support hold** - `eabb5f4` (test)
2. **Task 1 step 2: RED (A, B, C, F)** - `07e237b` (test)
3. **Task 1 step 3: the fix** - `493156b` (fix)
4. **Task 2: D, E1-E3, ARCHITECTURE.md** - `5b1c417` (test)
5. **Task 2: revert-check script** - `60b1538` (test)

## RED evidence (Task 1, at `07e237b`, unfixed code)

| Test | nextest exit | Panic message line |
|------|--------------|--------------------|
| A `a_completed_stream_run_returns_while_a_leftover_process_holds_the_agents_stdout` | 100 | `WR-03: the supervisor still runs 20 s after its agent was released to exit; holder ... alive: true; socket: Ok(Alive(PingReply { ... owner: "agent_running", ... }))` |
| B `a_completed_stream_run_returns_once_its_leftover_process_released_the_agents_stdout` | 0 | (control, passes) |
| C `no_line_reaches_the_capture_after_the_stdout_wait_expires` | 100 | `WR-03: the stream supervisor did not return 0 within 10 s while a leftover held its agent's stdout: Ok(Ok(0)) after 20.0058s` |
| F `a_stream_run_returns_while_a_leftover_process_keeps_writing_to_the_agents_stdout` | 100 | `WR-03: the stream supervisor did not return 0 within 10 s while a leftover kept writing to its agent's stdout: Err(Timeout) after 20.0001s` |

Semantic assessment: each failure is the target test's own `WR-03:` assertion, for the intended reason. A saw the supervisor ALIVE in `agent_running` (the 120 s silence wait). C returned only when the holder exited at about 20 s. F never returned while the writer wrote. A JUnit rerun of A, C and F was classified `RED_EVIDENCE_OK` (`target_test_failed`) by `gsd-tools check tdd-red-evidence`.

Doc-check negative control (Task 1 step 1, before the allowlist entry): `doc_check::source_devflow_env_vars_and_subcommands_are_documented` exited 100 with `source-read environment variable \`DEVFLOW_TEST_STREAM_WAIT_HOLD\` is missing from scoped operator docs`. G's own negative control: ungating the real call made G fail (exit 100) with `a call to stream_wait_test_hold( is not directly under #[cfg(any(test, feature = "test-support"))]`. The file was restored afterwards.

## Gate results

- Task 1 gate 1 (A, B, C, F, G): exit 0. A took 3.1 s and F 4.5 s. C took 20.3 s, because the test waits for its holder to exit after the supervisor returned.
- Task 1 gate 2 (six existing stream, stdin and stop tests, both doc checks, `cargo clippy -p devflow-core --all-targets -D warnings`): exit 0. The tracer re-run of the verify passed on the same tree that was committed.
- Task 2 gate 1 (D, E1, E2, E3): exit 0.
- Task 2 gate 2 (`scripts/check.sh all` under a sentinel HOME, with no `devflow` path left there): exit 0, `==> check.sh: all OK`, 39 `test result: ok` lines, 0 `FAILED`.
- Task 2 gate 3: `SEMVER verdict=match`, and both `aarch64-apple-darwin` `cargo check` runs finished. Exit 0.
- Task 2 gate 4 (`bash -n`, `shellcheck`, then the revert check `run`, about 5 min): exit 0.

```
REVERT fix_commits=1
REVERT fail_first=A rc=100 marker=yes
REVERT control=B rc=0
REVERT fail_first=C rc=100 marker=yes
REVERT fail_first=F rc=100 marker=yes
REVERT fail_first=D rc=100 marker=yes
REVERT fail_first=E1 rc=100 marker=yes
REVERT control=E2 rc=0
REVERT fail_first=E3 rc=100 marker=yes
REVERT phase_tree_unchanged=yes
REVERT verdict=pass
```

Afterwards nothing named `wr03-revert` was left under `target/`, `git worktree list` had no entry for it, and `git status --porcelain` was empty. The marker matcher was checked before use. It said yes for the real RED logs of A and C and for an `assert_eq!` message line. It said no for B's passing log and for a log that printed `WR-03:` to stdout and then panicked with `FIXTURE:`.

## Decisions Made

- I recorded the RED evidence as JUnit XML through a scratch nextest `--config-file` profile, so that the GSD classifier could check it. This adds no repo config.
- G has one extra planted control beyond the plan's four: a definition gated by `cfg(test)` only.

## Deviations from Plan

None in substance. The plan executed as written.

- The fix commit's subject (85 chars, the plan's exact text) set off the commit-msg hook's advisory warning about subjects over 72 characters. I kept the subject, because the revert check keys on the `fix(50-26): ` prefix and the plan fixes the text.

## TDD Gate Compliance

The RED commit `07e237b` (`test(50-26):`) comes before the GREEN commit. GREEN is the `fix(50-26):` commit `493156b`, not a `feat(...)` commit: the plan prescribes that prefix, because the revert check reverts exactly the `fix(50-26):` commits. A `feat(` grep for this plan finds nothing, on purpose. There is no REFACTOR commit.

## What these checks do NOT establish

- **No live run exercises the fix.** That is 50-28. The checks ran the debug test build with test-support, not a release binary.
- **Each timing-bounded test ran once or twice per configuration.** Those runs are the gate runs, `check.sh all` and the revert check. A heavily loaded host could still push A (20 s), C (10 s) or F (10 s) past their windows. This is a weak bound, not a flake-rate measurement.
- **Not exercised by any test:** `capture_closed: false` (a capture write blocked past both waits), an unfinished `exit_watch` or `writer` thread, and the idle timeout firing in the same instant as the agent's exit.
- **The 3 s bound is not a measured need.** It rests on reading the code and on these tests.
- **E3 used `devflow stop`'s no-record sweep arm.** The recorded arm is covered by `stop_e2e`.
- The plan's stated limits still hold: the exit file keeps `0` for a safe-point stop (999.166 item 6), in-group leftovers keep running (999.164 item 5), and WR-01's long stretches are open.

## Issues Encountered

None.

## User Setup Required

None. No external service configuration is required.

## Next Phase Readiness

- 50-28 (live attempt 2) can build its pinned binary from a tree that contains the fix.
- STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched. The orchestrator owns those writes after the wave.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-21*

## Self-Check: PASSED
