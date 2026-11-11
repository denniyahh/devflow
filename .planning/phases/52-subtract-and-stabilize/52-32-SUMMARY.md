---
phase: 52-subtract-and-stabilize
plan: 32
subsystem: rate-limit-gate
tags: [rust, devflow-cli, gates, rate-limit, timer, e2e, tdd, semver]

requires:
  - phase: 52-31
    provides: "Gates::rate_limit_timer_answer, GateAction::from_gate_answer, reset_time::rate_limit_wait_bound, State.rate_limit_auto_relaunch_used, the rate_limit gate kind with not_before"
  - phase: 52-26
    provides: "State.relaunch_note and its clear sites"
  - phase: 52-29
    provides: "the routers' RateLimit arms (approve relaunch_now, reject relaunch_with_note) and the single claim"
provides:
  - "pipeline_gate::rate_limit_timer_deadline(state, gate) -> Option<i64>: the one arming and wake function"
  - "pipeline_gate::gate_lifetime_deadline and the per-poll choice pipeline_gate::due_gate_deadline(now, wake, lifetime) -> Option<DueDeadline> (wake first)"
  - "the waiting supervisor's one automatic relaunch: flag committed, then the timer's approve; an operator answering first restores the flag"
  - "event rate_limit_timer_fired { stage, gate_id, outcome }"
  - "gate_resolved names a rate-limit approve or reject as action relaunch (GateAction::from_gate_answer)"
  - "pipeline_launch::LAUNCH_PROBE (test-only launch seam)"
  - "OPERATIONS.md describes the rate-limit gate as built"
affects: [55, close-out]

actuals:
  tokens: 19683
  tasks: 3
  commits: 5
plan_head_before: 33be58d68f61d1b1fec66bb9902c345227043b6e
plan_head_after: 87e45664d4e4dfb9965f8cf3d4b046faa0e06d4c

tech-stack:
  added: []
  patterns:
    - "A gate wait with two deadlines kept apart, chosen per poll by one pure function that checks the armed wake before the lifetime"
    - "A durable at-most-once flag committed before the side effect it guards, restored only on a proven loss (AlreadyResponded)"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-32-SUMMARY.md
  modified:
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - OPERATIONS.md
    - .planning/phases/52-subtract-and-stabilize/deferred-items.md

key-decisions:
  - "52-32: the timer lives in the gate wait (pipeline_gate::wait_with_rate_limit_timer), armed per gate open from the state the supervisor just committed, so a first open, a resume and a takeover all decide arming through rate_limit_timer_deadline and nothing else"
  - "52-32: after the timer writes its answer (or finds an operator's), the wait picks the answer up before it looks at the lifetime again; at equal deadlines the lifetime is due too, and checking it first timed the gate out over its own answer (found by the U8 e2e)"
  - "52-32: a timer write that fails for any reason other than AlreadyResponded keeps the flag spent (the write's effect is unknown) and says so in rate_limit_timer_fired outcome failed; the gate then waits for an operator"
  - "52-32: gate_resolved's action label comes from GateAction::from_gate_answer, so a rate-limit approve or reject reads relaunch; the routing still goes through from_response and the routers' RateLimit arms"
  - "52-32: the rate-limit gate's context is unchanged from 52-31 (sentence, then the resume command, then the reset); the parity row checks both fall inside the shared display cut, which 52-30 set to 450 characters, not 300"

patterns-established:
  - "Rate-limit unit fixtures that open a gate use a future reset (2099), so the timer never relaunches inside a unit test"

requirements-completed: [CUT-03]

coverage:
  - id: D1
    description: "A denial whose reset has passed relaunches Code once by itself (counted by stage code turns and by Code stage_launched events), the timer's answer names the timer, the flag is spent, and the second denial's gate records not_before and stays open"
    requirement: CUT-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_passed_reset_relaunches_once_and_a_second_denial_waits"
        status: pass
    human_judgment: false
  - id: D2
    description: "The armed wake is not_before whatever the gate timeout; the lifetime is the later of the two; at equal deadlines the wake wins; a spent flag arms nothing"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::the_timer_wakes_at_reset_plus_margin_whatever_the_gate_timeout"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_reset_earlier_than_the_gate_timeout_relaunches_at_the_reset"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_reset_later_than_the_gate_timeout_relaunches_at_the_reset"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_future_reset_outlives_the_gate_timeout"
        status: pass
    human_judgment: false
  - id: D3
    description: "The waiting gate: no cron file, gate show names the reset and the resume command for when the waiter is gone, one Code launch; a denial with no reset is a plain gate that never relaunches"
    requirement: CUT-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_denial_without_a_reset_time_is_a_plain_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::every_gate_names_only_the_routes_its_answers_run"
        status: pass
    human_judgment: false
  - id: D4
    description: "Approve relaunches the same stage on the same agent; reject relaunches it with the note byte for byte on stdin (claude) and argv (codex); neither spends the flag; an approve launches once, the timer then writes nothing on that gate, and a later denial is armed (controls: abort launches nothing, a spent flag arms nothing)"
    requirement: CUT-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#approving_a_rate_limit_gate_relaunches_the_same_stage_on_the_same_agent"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#rejecting_a_rate_limit_gate_relaunches_the_same_stage_with_the_note"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#a_rate_limit_rejection_note_reaches_the_capture_prompt_verbatim"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::the_approve_path_takes_the_route_approve_route_names"
        status: pass
    human_judgment: false
  - id: D5
    description: "Resume (plain and after a stop) and a supervisor over a state whose flag is spent arm nothing; the crash state arms nothing and writes nothing; the same paths over an unspent flag arm the timer"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::resume_does_not_rearm_the_auto_relaunch"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_takeover_does_not_rearm_the_auto_relaunch"
        status: pass
    human_judgment: false
  - id: D6
    description: "OPERATIONS.md describes the gate as built; docs name only existing identifiers and no removed surface; semver --release-type major exits 0; scripts/check.sh all and the container check exit 0"
    requirement: CUT-03
    verification:
      - kind: other
        ref: "cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::doc_referenced_identifiers_exist_in_source)' && ... no_removed_surface_literal_remains && cargo semver-checks check-release -p devflow-core --release-type major && scripts/check.sh all && timeout -k 60 900 scripts/check-in-container.sh (exit 0)"
        status: pass
    human_judgment: false
  - id: D7
    description: "A real agent hitting a real rate limit, waiting for a real reset, and the timer relaunching it"
    verification: []
    human_judgment: true
    rationale: "Every scenario uses a stub denial and a reset computed seconds from now; no live rate limit on a real agent is exercised in this phase (not dogfooded, D-18)"

duration: 60min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 32: The rate-limit gate's one automatic relaunch Summary

**A rate-limit gate now relaunches the stage by itself once, when the reset plus 60 seconds has passed and nobody has answered. The run's flag is written to disk before the timer's answer, so a crash can lose the relaunch but never doubles it. A second denial, a denial with no reset time, or any denial after the relaunch was used waits at the gate for the operator. Approve and reject relaunch the same stage on the same agent (reject with the note) and do not use up the automatic relaunch. Resume and takeover arm the timer only through the same function. OPERATIONS.md describes all of this. Both check environments are green.**

## Performance

- **Duration:** about 60 min
- **Started:** 2026-11-11T15:30:18Z
- **Completed:** 2026-11-11T16:30:03Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- **The timer (Task 1).** `pipeline_gate.rs`:
  - `rate_limit_timer_deadline(state, gate) -> Option<i64>` is the one arming function. It returns the gate's `not_before` for a `rate_limit` gate while `rate_limit_auto_relaunch_used` is false, and `None` otherwise.
  - `gate_lifetime_deadline(opened_at, gate, timeout)` is when the gate stops waiting. For a rate-limit gate with a reset it is `rate_limit_wait_bound`; otherwise it is the gate timeout.
  - `due_gate_deadline(now, wake, lifetime) -> Option<DueDeadline>` is the per-poll choice the plan asks to be named. It returns `Wake` whenever an armed wake is due, before it looks at the lifetime.
  - `wait_with_rate_limit_timer` wraps the gate wait in `open_and_await`. A gate that is not a rate-limit gate waits exactly as before. For a rate-limit gate it waits on both deadlines. When the wake is due, it disarms itself and calls `answer_as_rate_limit_timer`, which commits the flag, then calls `Gates::rate_limit_timer_answer`:
    - `AlreadyResponded` (an operator answered first): it commits the flag back to false.
    - Any other error: it keeps the flag spent and prints why.
    - Each attempt emits `rate_limit_timer_fired` with `outcome` `answered`, `operator_answered_first` or `failed`.
  - The answer, the timer's or the operator's, then goes through the gate's existing screen, single claim and the routers' `RateLimit` arms. Nothing re-arms a timer or re-publishes its answer after a crash.
- **The equal-deadline fix (Task 2).** After the timer writes (or finds an operator's answer), the wait takes that answer before it checks the lifetime again (`ANSWER_PICKUP_SECS` = 10).
- **`gate_resolved`.** Its `action` label now comes from `GateAction::from_gate_answer`, so a rate-limit approve or reject reads `relaunch`. This is the first production caller of 52-31's mapping. The routing itself still uses `from_response` and the routers.
- **The handoff-refused path.** It already called `handle_rate_limited_outcome` (`pipeline_launch.rs`, the `HandoffOutcome::Refused` arm), so it gets the timer the same way. Nothing changed there.
- **Docs (Task 3).** OPERATIONS.md now describes:
  - the wake at reset plus 60 s, and that it ignores the gate timeout;
  - that it happens at most once per run, and that the flag is recorded before the answer;
  - an operator answering first, a second denial, a missing reset, and resume, stop-and-resume and takeover;
  - the crash case and the `devflow resume` command for it;
  - a new `rate_limit` row in the status-less gate table (it was listed under `stage_failure`, which went stale when 52-31 gave it its own kind) and the `rate_limit_timer_fired` event.

## Task Commits

1. **Task 1 (tracer):** `ffb0f4f` test (RED), `0755289` feat (GREEN). The plan names the subject `feat(cli): a rate-limit gate relaunches once after the reset`; the GREEN commit uses that text under the `52-32` scope.
2. **Task 2:** `63dc371` test (RED), `a60f033` feat (GREEN: the equal-deadline fix)
3. **Task 3:** `87e4566` docs

**Plan metadata:** the docs commit for this SUMMARY follows.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch `--profile red` config. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `failover_e2e::a_passed_reset_relaunches_once_and_a_second_denial_waits` | `RED_EVIDENCE_OK` | It ran and failed on its own wait for a second `gate_fired`. One Code turn ran, and the first rate-limit gate stayed open with no relaunch. That is the missing timer. |
| `failover_e2e::a_reset_later_than_the_gate_timeout_relaunches_at_the_reset` | `RED_EVIDENCE_OK` | It ran on Task 1's GREEN code and failed on its own wait for a second Code launch. The log shows `rate_limit_timer_fired` (`answered`) and then, at once, `gate_timeout`. At equal deadlines the wait checked the lifetime again before it picked up the timer's answer. This is the U8 defect, and `a60f033` fixes it. |

**Not RED (`unexpected_green`, stated, not hidden).** These tests passed on their RED commit:

- In Task 1: `every_gate_names_only_the_routes_its_answers_run`'s new assertions. 52-31 already wrote the context in that order.
- In Task 2:
  - `a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate`
  - `a_future_reset_outlives_the_gate_timeout`
  - `a_reset_earlier_than_the_gate_timeout_relaunches_at_the_reset`
  - `a_denial_without_a_reset_time_is_a_plain_gate`
  - `approving_…`
  - `rejecting_…`
  - `gate_wedge_e2e::a_rate_limit_rejection_note_reaches_the_capture_prompt_verbatim`
  - `the_timer_wakes_at_reset_plus_margin_whatever_the_gate_timeout`
  - `resume_does_not_rearm_the_auto_relaunch`
  - `a_takeover_does_not_rearm_the_auto_relaunch`
  - the approve parity row.

The plan's tasks overlap: Task 1's GREEN already built the timer, the wait bound and the flag write, and 52-31 had already wired approve and reject. Each of these tests carries a control that can fail:

- the abort row launches nothing;
- a spent flag arms nothing;
- an unspent flag arms the timer and the timer answers;
- the wake-first row has a no-wake control;
- the display-cut row has a pushed-past-the-cut control.

Gate sequence: `test` ffb0f4f → `feat` 0755289; `test` 63dc371 → `feat` a60f033. Each GREEN commit descends from its RED commit.

**Tracer gate.** The run is interactive, in `end-of-phase` mode, and Task 1's verify is automated only. Its two-command chain was re-run before the GREEN commit and exited 0 (0, 0), so expansion went ahead.

## Semver re-check (Task 3)

`cargo semver-checks` 0.51.0 against the published `devflow-core` 3.0.0. The default run exits 100 with 15 failing check kinds (52-18 listed 9). `--release-type major` exits 0. As 52-18 noted, that run executes 0 checks and skips 260, so its exit 0 only shows the comparison could be built; the default run below is the evidence.

The kinds and items not in 52-18's list, each matched to the plan that made it:

| Check | Item | Plan |
|---|---|---|
| `constructible_struct_adds_field` | `GateResponse.abort` | 52-19 |
| | `GateFile.gate_kind` | 52-22 |
| | `GateFile.gsd_status` | 52-29 |
| | `GateFile.not_before`, `OpenGate.not_before` | 52-31 |
| `enum_variant_added` | `GateError::ApprovedAndAborted` | 52-19 |
| | `StageIntent::PlanGaps` | 52-27 |
| | `GateAction::Relaunch`, `GateError::NotARateLimitGate` | 52-31 |
| `enum_struct_variant_field_added` | `StageIntent::{Plan, Code, Validate}.relaunch_note`, `StageIntent::Code.fix_note` | 52-26 |
| | `StageIntent::Code.reverify_report` | 52-28 |
| `enum_tuple_variant_changed_kind` | `GateAction::LoopBack` (now `{ stage, note }`) | 52-26 |
| `function_parameter_count_changed` | `prompt::fix_prompt` (3 parameters) | 52-26 |
| | `mode::transition_resets_consecutive_failures` (3 parameters) | 52-27 |
| `method_parameter_count_changed` | `Gates::open_gate`, `open_registered`, `write_gate` (5 parameters) | 52-22 (the `GateLabel` change in 52-29 falls under the same finding) |
| `struct_pub_field_missing` (new items in an old kind) | `State.last_verification_fingerprint`, `last_verification_mtime_nanos`, `verification_run_nonce` | 52-24 |

**No unmatched break.** This plan changes no `devflow-core` item.

Orchestrator commit `96f90b1` touches `crates/devflow-core/src/state.rs` in a doc comment only (the third reset-to-zero case of `phase_validate_failures`). It changes no `pub` item, and semver-checks names nothing from it.

The additive items the plan's list names do not appear, because none is a break: `GateKind`, `GateLabel`, `FixType`'s new variants, `State.validate_pass_head` (`State` is `#[non_exhaustive]`), `gsd_reader` and `reset_time`.

The version stays 3.0.0. 4.0.0 is for close-out.

## Existing tests changed

- **Unit fixtures moved to a far-future reset.** Unit fixtures that open a rate-limit gate with a reset now use `2099-06-18T15:45:30Z`, not `2026-06-18T15:45:30Z`. That date is now in the past, and the timer would otherwise relaunch inside a unit test. The affected tests:
  - `pipeline_outcomes`: `a_rate_limit_without_a_fallback_does_not_count_as_an_infra_failure`, `primary_loop_rate_limited_waits_at_the_code_gate`, and the rate-limit rows of `every_gate_names_only_the_routes_its_answers_run` and `the_reject_path_takes_the_route_reject_route_names`;
  - `pipeline_launch`: `RATE_LIMIT_ENVELOPE`.

  `retry_after_from_reason_strips_prefix` keeps its date, because it never opens a gate.
- **The `failover_e2e` rig.**
  - The Claude denial's `resetsAt` is now `@RESETS_AT@`. `Rig::new` fills it with now plus one hour, and `Rig::with_reset` takes a value, or `None` for `null`.
  - The stub logs each stdin turn to `claude-stdin.log`.
  - `start_with_env` / `start_with_timeout` pass `DEVFLOW_GATE_TIMEOUT_SECS`.
- **The 52-09 test.** `a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate` now uses a computed future reset, and it asserts one `stage code` turn, one Code `stage_launched` and the waiter wording.
- **The approve parity row.** `the_approve_path_takes_the_route_approve_route_names`'s `RateLimit` arm calls the new helper `rate_limit_approve_launches_once`.

## Decisions Made

See `key-decisions`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] At equal deadlines the gate timed out over the timer's own answer**
- **Found during:** Task 2, by `a_reset_later_than_the_gate_timeout_relaunches_at_the_reset`.
- **Issue:** Task 1's loop wrote the timer's answer and then polled again. The lifetime was due at that same second, so the poll returned `TimedOut` before the answer was read.
- **Fix:** After the timer writes, or finds an operator's answer, the wait reads the answer before it checks the lifetime again.
- **Files:** `crates/devflow-cli/src/pipeline_gate.rs`
- **Verification:** The e2e passed afterwards, along with the rest of the Task 2 chain.
- **Commit:** `a60f033`

**2. [Rule 3 - Blocking] A launch seam for the approve parity row**
- **Found during:** Task 2.
- **Issue:** The plan asks the row to record "exactly one launch … through the launch seam the other parity rows use". No such seam existed. The parity test runs with no agent on `PATH`, so `launch_stage` fails before any `stage_launched` event.
- **Fix:** Added `pipeline_launch::LAUNCH_PROBE`, a `#[cfg(test)]` thread-local that records `(stage, agent)` at the top of `launch_stage`.
- **Commit:** `63dc371`

**3. [Scope] Assertions adjusted to what the code admits (stated, not hidden)**
- **The timer after an approve.** The plan expects the timer writer to return `AlreadyResponded` on the gate after an approve. The relaunch closes the gate first, so the writer returns `NoOpenGate`. The row accepts either error and asserts that the launch count stays 1.
- **The U8 e2e.** It asserts no `gate_timeout` and no `workflow_aborted` *before the relaunch*. The second denial's gate has no timer and a 2 s lifetime, so it times out afterwards, and a whole-log assertion is unreachable.
- **The takeover test.** It does not use the `supersede` helper. A superseded supervisor writes nothing (D-03), so it never opens the gate to be "denied again under that supervisor". The test launches a supervisor over a state whose flag is already spent, since the state on disk is the only thing a takeover inherits. The crash row is a static fixture (flag spent, gate open, no waiter): no code path re-attaches a supervisor to an existing gate.
- **The display cut.** The plan says the cut is 300 characters. It is now 450 (`GATE_CONTEXT_DISPLAY_CHARS`, 52-30). The row checks against that constant.

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking), 1 scope note.
**Impact on plan:** All truths are met as tested. The bug was a real U8 defect in this plan's own Task 1, and the U8 test caught it.

## Deferred items

- **`parse_reset_time` accepts a day past the month's end** (52-01's entry). Not taken, because this plan did not touch the parser. Its only effect on the timer is a later wake, never an earlier one. The entry is updated and stays open.
- **Cron wording in comments** (52-10's entry). Not taken. The entry is updated and stays open.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>` chain (2 named runs), re-run as the tracer gate | 0, 0 |
| `cargo nextest run -p devflow -p devflow-core` after Task 1 GREEN | 0 (2028 passed) |
| Task 2 `<verify>` chain (12 named runs, one at a time) after `a60f033` | all 0 |
| `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` before every code commit | 0 |
| Task 3 chain: both `doc_check` named runs, `cargo semver-checks check-release -p devflow-core --release-type major`, `scripts/check.sh all`, `timeout -k 60 900 scripts/check-in-container.sh` (after the last code change, `a60f033`, and the docs commit) | 0 (host and container each "all OK": 2058 passed, 0 failed) |
| `cargo semver-checks check-release -p devflow-core` (default) | 100, expected; the table above |

None of the known flakes (999.221, 999.222, 999.223) occurred. No container or `devflow __monitor` process was left after the runs (`docker ps` and `ps` were empty).

**What these checks do not establish:**
- Every scenario uses a stub denial. A live rate limit on a real agent is not exercised, and this phase is not dogfooded (D-18).
- The rejection note is proven end to end once per launch shape: on stdin for claude's stream shape, and on argv for codex's capture shape.
- The one automatic relaunch is proven per run, not across separate `start` runs on the same phase.
- Across a crash, it is proven to be at most one: the tests show that a crash leaves none, not that one survives.
- The crash row is a static fixture with no process waiting. Its proof is that nothing in the code re-attaches.
- The e2e reset times are seconds from now. The equal-deadline case is pinned exactly by the pure row, while the U8 e2e reaches it only when the gate opens within about 4 s of the test computing the reset.
- No real `gate sweep` ran.
- **Left for the close-out:** the VALIDATION manual rows. Check the operator's own `.devflow/` for 3.x state or an `abort:` note, and check for hand-made `hermes cron` jobs.

## Threat Flags

None beyond the plan's register. Each mitigated threat is tested:

- T-52-47 (one claim, the flag before the answer, nothing re-published after a crash);
- T-52-48 (at most one automatic relaunch; resume and takeover do not re-arm);
- T-52-49 (the lifetime is the later of the two deadlines, the wake ignores the lifetime, and the wake wins at a tie).

T-52-64 (the relaunch is lost to a crash) is accepted, and OPERATIONS.md names the `devflow resume` way on.

## Known Stubs

None.

## Issues Encountered

- **A misleading failure message.** The first RED run of the tracer test seemed to stall after Plan. The failure message was built before the wait, so it showed a stale snapshot of the event log. The helper now reads the logs when the wait ends.
- **A RED run raced a rebuild.** During that RED run, `cargo build` rebuilt the binary. The waiting supervisor was already running the old binary, and nothing launched after the build. The RED evidence stands.

## User Setup Required

None.

## Next Phase Readiness

This is the phase's last plan. CUT-03 and criterion 3 are complete. The phase is ready for verification and close-out (the 4.0.0 bump).

## Self-Check: PASSED

- Files exist: `crates/devflow-cli/src/pipeline_gate.rs` (`fn rate_limit_timer_deadline`, `fn due_gate_deadline`, `rate_limit_auto_relaunch_used`), `crates/devflow-cli/tests/failover_e2e.rs` (`fn a_passed_reset_relaunches_once_and_a_second_denial_waits`), `OPERATIONS.md` (`rate_limit_timer_fired`).
- Commits `ffb0f4f`, `0755289`, `63dc371`, `a60f033` and `87e4566` are ancestors of HEAD, and `git rev-list --count 33be58d..87e4566` is 5.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
