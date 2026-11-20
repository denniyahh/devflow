---
phase: 53-the-question-channel
plan: 07
subsystem: question, ask-server, gates, liveness
tags: [mcp, question, gates, notify, liveness, signals]
status: complete

requires:
  - phase: 53-05
    provides: QuestionSpec::from_arguments and the server's refusal before any gate
  - phase: 53-02
    provides: QUESTION_ASKED / QUESTION_ANSWERED / QUESTION_CLOSED event names and field contract
provides:
  - fire_gate_notify_events, one gate_fired -> bounded hook -> notify_fired helper for the runner and the server
  - NOTIFY_HOOK_TIMEOUT (30 s) and a process-group-killing, cancel-aware hook runner; fire_gate_notify_with_fallback_cancellable
  - Gates::close_if_current, the identity-checked close every server close goes through
  - the D-06 endings (client cancel, stdin EOF, SIGTERM/SIGINT, run stopped, runner gone, timeout, abort), each closing its own gate and recording question_closed
  - the D-03 run binding (state, started_at, stage, live runner lock) checked before any gate
  - liveness::ask_lock_path and liveness::claim_ask_lock (persistent flock lock); liveness::HolderIdentity made pub
  - a removable Mutex<VecDeque> + Condvar call queue, a cancelled-id set keyed by JSON text, and --timeout-secs counted from arrival
affects: [53-08, 53-09, 53-10]

actuals:
  tokens: 17687
  tasks: 3
  commits: 6
plan_head_before: 8dd85d682f49b18b22d2755e096bf080fdce754b
plan_head_after: 56e9a6b736098490957d9f78eaf547dd5d3f9020

tech-stack:
  added: []
  patterns:
    - "Reader thread + worker on the calling thread: the worker drains a Mutex<VecDeque> with a Condvar, and returns at stdin's end or a caught signal so the process can exit while the reader is still blocked on stdin"
    - "A signal handler that only stores to a static AtomicI32 (SA_RESTART); the worker and every held call poll it"
    - "One RunWatch per call: client endings checked on every cancel poll, the state file and runner lock at most once a second, the first ending kept in a Cell"

key-files:
  created:
    - .planning/phases/53-the-question-channel/deferred-items.md
  modified:
    - crates/devflow-cli/src/ask_server.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/liveness.rs
    - crates/devflow-cli/tests/ask_e2e.rs
    - crates/devflow-cli/tests/common/ask_client.rs
    - OPERATIONS.md

key-decisions:
  - "An operator abort records question_closed{reason: aborted_by_operator} with closed_by set to the responder, and replies `devflow_ask closed: aborted_by_operator — ...`, matching the prompt's rule that an abort is a `devflow_ask closed:` result"
  - "A failure that is not a D-06 ending (gate could not open, answer unreadable, claim lost) replies isError but records no question_closed: the reason vocabulary in events.rs has no error reason"
  - "Nothing is written to stdout after stdin's end, so a client_disconnected call gets no reply; on a signal the server replies `devflow_ask closed: terminated`, closes, and exits 128 + the signal"
  - "A call ending while it waits for the ask lock (cancel, EOF, signal, stop, runner gone, timeout) opens no gate and records no question event; only the timed-out case logs its question id to stderr, as the plan asks"
  - "The answer wait gets the time left to the call's deadline rounded to the nearest whole second (Gates::await_answer takes whole seconds); the ask-lock wait checks the deadline exactly"

requirements-completed: [ASK-01, ASK-02, ASK-04]

duration: 72min
completed: 2026-11-19
---

# Phase 53 Plan 07: Question endings, run binding and one question per stage Summary

**A held `devflow_ask` call now ends only by an answer or one of the D-06 endings. Each ending closes the server's own gate through an identity-checked `Gates::close_if_current` and records `question_closed`. A call is refused unless it is bound to a live run. A per-stage flock ask lock plus a removable FIFO allow one question per stage at a time, with the timeout counted from the call's arrival.**

## Performance

- **Duration:** about 72 min, measured from the first task commit (10:13 to 11:25 local). The interrupted run's start time was not recorded.
- **Completed:** 2026-11-19
- **Tasks:** 3 of 3. Task 1 was done before the interruption; this run finished Task 2 GREEN and did all of Task 3.
- **Files modified:** 7

## Accomplishments

- **Notify and events (Task 1, prior run).** `fire_gate_notify_events` is shared by the runner and the server. The hook runner is bounded at `NOTIFY_HOOK_TIMEOUT` (30 s) and killed on a cancel. The server notifies on its own thread and emits `question_asked` / `question_answered`.
- **Every ending closes only its own gate (Task 2).**
  - Endings: client cancel (ids matched by JSON text, so `3` and `"3"` are different ids), stdin EOF, SIGTERM/SIGINT, run stopped, runner gone, timeout and operator abort.
  - The one `close_if_current` call site is `ask_gate` in `crates/devflow-cli/src/ask_server.rs:629`. No `Gates::close(` call is left in ask_server.rs.
  - The D-03 binding check runs before any gate opens.
- **One question per stage (Task 3).**
  - `liveness::claim_ask_lock` is a persistent flock file and is never removed.
  - A cancel removes a call that is still queued.
  - T runs from the call's arrival. A call whose time runs out before its gate opens replies `devflow_ask closed: timed_out — no answer within the gate timeout`, opens no gate and records no question event.

## Task Commits

1. **Task 1: Notify through one bounded helper, and the question events.** RED is `f2cb22f`, GREEN is `cbbba0c`. Both were made in the prior run.
2. **Task 2: Every non-answer ending closes its own gate; calls bind to their run.** RED is `9db7866` (prior run). GREEN is `8ac099c`.
3. **Task 3: One question per stage: removable FIFO, flock ask lock, time from arrival.** RED is `a0a5ae8`, GREEN is `56e9a6b`.

## TDD Gate Compliance

- **Task 2.** RED `9db7866` shipped a `close_if_current` stub that closed unconditionally. Negative control, run this session: with the stub `gates.rs` restored, `a_late_server_cleanup_leaves_the_runners_replacement_gate` FAILED, because the replacement gate was removed. With the GREEN code it passes.
- **Task 3.** RED `a0a5ae8` was run before GREEN. Four of the five new e2e tests failed for the intended reasons:
  - "second gate opened early"
  - "a question_asked for the cancelled queued call" (two events)
  - "a queued call still waiting after its deadline: both replies took 4.0s"
  - "the waiting call opened a gate"

  `two_contenders_for_an_abandoned_ask_lock_get_one_owner` failed on the refusing stub. `a_second_call_waits_for_the_first_question_to_close` already passed at RED. Task 2's single FIFO worker already serialises calls on one server, so that test is a regression gate rather than a RED proof.
- **Contender test negative control.** A `claim_ask_lock` that opens the file without a flock fails with "round 0: 2 owners of the ask lock". The real one passes all 50 rounds.

## Verification

- **Task 2 verify chain** (8 named nextest gates with `--no-tests=fail`): all passed.
- **Task 3 verify chain** (6 named gates): all passed.
- **`cargo nextest run -p devflow --test ask_e2e`:** 19 run, 19 passed. The plan said 18; Task 3 adds 5 e2e tests on top of the 14 that already existed.
- **`cargo nextest run -p devflow --bin devflow`:** 509 passed, 3 skipped. Run after Task 2 GREEN.
- **`scripts/check.sh all`:** exit 0 on the run after the Task 3 commit. The run before it failed once on an unrelated flaky monitor test (see Deferred Issues).
- **`cargo clippy -p devflow -p devflow-core --all-targets -- -D warnings`:** clean.
- **semver-checks.** Command: `cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor`. Exit 100, `Checked 202 checks: 198 pass, 4 fail, 0 warn, 58 skip`. None of the four failures names an item this plan added:
  - `enum_variant_added`: `GateKind::Question` (gates.rs:165, from an earlier 53 plan)
  - `function_missing`: `verify::phase_has_blocking_human_checkpoint`, `prompt::checkpoint_auto_decide_prompt`, `agent_result::blocking_human_checkpoint_reported`, `agent_result::checkpoint_reported_in_capture`
  - `inherent_method_missing`: `ClaudeDriver::exec_resume_command`
  - `pub_module_level_const_missing`: `GATE_RESOLUTION_RULE`

  These are earlier removals on the branch toward the 4.0 major. This plan's `close_if_current`, `fire_gate_notify_with_fallback_cancellable`, `NOTIFY_HOOK_TIMEOUT`, `ask_lock_path`, `claim_ask_lock` and the now-pub `HolderIdentity` raise no finding, so they read as additive.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The Condvar queue arrived in Task 2, not Task 3.**
- **Found during:** Task 2
- **Issue:** On EOF or a signal, the worker must wake and return while the reader stays blocked on stdin. The 53-01 `mpsc` worker could not do both, and the reader sat on the main thread.
- **Fix:** Task 2 moved the reader to its own thread, made the calling thread the worker, and replaced the `mpsc` channel with the plan's `Mutex<VecDeque<PendingCall>>` + `Condvar`. Task 3 then added removal of queued calls on cancel, arrival times and the ask lock.
- **Files modified:** crates/devflow-cli/src/ask_server.rs
- **Commit:** 8ac099c

**2. [Rule 1 - Bug] A unit test depended on a reply written after stdin's end.**
- **Found during:** Task 2
- **Issue:** `ping_is_answered_while_a_call_is_held` dropped stdin before reading the released call's reply. Under the new rule that nothing is written after EOF, that ordering races.
- **Fix:** The test now waits for the reply and then drops stdin.
- **Commit:** 8ac099c

**3. [Rule 1 - Bug] Clippy failures in this plan's own code.**
- `collapsible_if` in Task 1's RED test helper (gates.rs) and `needless_borrow` in ask_server.rs were fixed in 8ac099c.
- `type_complexity` on the Task 3 RED stub's signature landed in RED commit a0a5ae8, because that commit was not gated on clippy. It was fixed in 56e9a6b by making the existing `HolderIdentity` alias `pub`, which is additive.

**4. Commit subject over 72 characters.** The Task 2 GREEN subject was shortened by amending the unpushed commit, giving `8ac099c`.

## Deferred Issues

- **Flaky `monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter`.**
  - It failed once inside `scripts/check.sh all` with `SigBlk leader fffffffffffbfeff`.
  - It passed alone twice, the whole lib passed on rerun, and the next `check.sh all` passed.
  - 53-07 does not touch it. Logged in `deferred-items.md`; it should be filed as a 999.x backlog entry.

## Threat Flags

None. Every new surface is in the plan's threat register:
- the `.devflow/.ask-<NN>-<stage>` lock file (T-53-38)
- the SIGTERM/SIGINT handler (T-53-16)

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/ask_server.rs, crates/devflow-core/src/liveness.rs, crates/devflow-core/src/gates.rs, crates/devflow-cli/tests/ask_e2e.rs, .planning/phases/53-the-question-channel/deferred-items.md
- FOUND commits: f2cb22f, cbbba0c, 9db7866, 8ac099c, a0a5ae8, 56e9a6b (each an ancestor of HEAD; `git rev-list --count 8dd85d6..HEAD` = 6)
