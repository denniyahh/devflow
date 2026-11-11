---
phase: 52-subtract-and-stabilize
plan: 31
subsystem: rate-limit-gate
tags: [rust, devflow-core, devflow-cli, gates, rate-limit, sweep, tdd, semver-break]

requires:
  - phase: 52-01
    provides: "reset_time::parse_reset_time"
  - phase: 52-22
    provides: "GateKind on every gate, GateLabel on the open path"
  - phase: 52-26
    provides: "State.relaunch_note and run_status_less_step's RelaunchWithNote write"
  - phase: 52-29
    provides: "answer_family, status_less_step, approve_route / reject_route, screen_answer, dispatch_answer, the parity tests and the rule-6 proptest"
  - phase: 52-33
    provides: "way_on_sentence / lead_with_way_on, gate_respond's screen and route print, every_gate_kind"
  - phase: 52-30
    provides: "the stored-whole reason bound and the shared display cut"
provides:
  - "reset_time::RATE_LIMIT_RELAUNCH_MARGIN_SECS (60), relaunch_not_before, auto_relaunch_due, rate_limit_wait_bound"
  - "GateKind::RateLimit; GateFile.not_before, GateLabel.not_before, OpenGate.not_before; GateLabel::rate_limit(reset_at)"
  - "gate sweep leaves a rate-limit gate alone before its not-before time (gate_sweep_may_reap reads it; gate_sweep_at injects the clock)"
  - "Gates::rate_limit_timer_answer (no boolean; refuses any gate whose file is not rate_limit) and RATE_LIMIT_TIMER"
  - "GateAction::Relaunch { note } and GateAction::from_gate_answer (the rate-limit answer mapping)"
  - "GateError::NotARateLimitGate"
  - "State.rate_limit_auto_relaunch_used (#[serde(default)]), set by nothing yet (52-32)"
  - "AnswerFamily::RateLimit, StatusLessStep::RelaunchNow; approve_route → relaunch_now, reject_route → relaunch_with_note"
  - "handle_rate_limited_outcome opens a rate_limit gate with its not-before time, led by the routers' way-on sentence"
  - "gate approve / reject name the agent a printed relaunch runs on"
affects: [52-32, 55]

actuals:
  tokens: 13242
  tasks: 3
  commits: 6
plan_head_before: a1d3b4cfb659f0399cefe21ab74500e9b7670150
plan_head_after: 6175be69104dfa86580689bb6f1d655595a3e9a0

tech-stack:
  added: []
  patterns:
    - "A privileged automatic writer with no boolean parameter that checks the gate file's kind before the shared first-writer-wins publish"
    - "A sweep that takes the clock as a parameter, so a test places it before and after a deadline without waiting"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-31-SUMMARY.md
  modified:
    - crates/devflow-core/src/reset_time.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/test_support.rs
    - OPERATIONS.md
    - .planning/phases/52-subtract-and-stabilize/deferred-items.md

key-decisions:
  - "52-31: the rate-limit gate's approve is a new status-less step, relaunch_now ('relaunches the <stage> stage now on the same agent'); its reject reuses 52-29's relaunch_with_note, which already commits the note to State.relaunch_note. Neither reads the recorded canary (T-52-67)"
  - "52-31: the production rate-limit writer (handle_rate_limited_outcome) now opens a rate_limit gate through GateLabel::rate_limit(parsed reset), so its file carries not_before; its 52-09 answer text ('approving this gate relaunches it now, `devflow gate abort` ends the run') is replaced by lead_with_way_on(GateKind::RateLimit, ..)"
  - "52-31: the not-before time travels on GateLabel (new field not_before, constructor GateLabel::rate_limit taking the reset only), so open_gate / open_registered / run_gate keep one label parameter and no timer or used-flag input exists (D-38)"
  - "52-31: the rate-limit answer mapping is GateAction::from_gate_answer(kind, response); the timer writer is Gates::rate_limit_timer_answer(root, phase, stage, observed), responded_by devflow-rate-limit-timer"
  - "52-31: a GateAction consumer fed only by from_response maps Relaunch to the refusal UNEXPECTED_RELAUNCH: screen_answer refuses it (the reopen names the stage and 'relaunch'), route_answer routes it to a refused approve, which dispatch_answer returns as an error naming the stage"
  - "52-31: gate approve / reject append ', on <agent> (the run's current agent)' to a retry, relaunch-now or relaunch-with-note route line, so the print names the agent the relaunch runs on"

patterns-established:
  - "A rate-limit gate is recognised by gate_kind rate_limit, never by its context; its not-before time depends on the reset alone"

requirements-completed: [CUT-03]

coverage:
  - id: D1
    description: "The automatic relaunch is due exactly at the reset plus a 60 s margin: false one second before, true at and after"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::auto_relaunch_is_due_exactly_at_reset_plus_margin"
        status: pass
    human_judgment: false
  - id: D2
    description: "A rate-limit gate opened with a reset records gate_kind rate_limit and not_before = reset + margin; with no reset it records the kind and no not_before; an ordinary gate writes no not_before key"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::a_rate_limit_gate_records_its_not_before_time"
        status: pass
    human_judgment: false
  - id: D3
    description: "gate sweep does not reap an aged rate-limit gate before its not-before time, reaps it at that time, and still reaps an ordinary aged gate"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_sweep_skips_a_rate_limit_gate_before_its_reset"
        status: pass
    human_judgment: false
  - id: D4
    description: "The timer writer refuses a non-rate-limit gate (and a kindless file) writing nothing, approves a rate-limit gate once as the timer, and loses to a human answer written first"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::the_relaunch_timer_writer_refuses_a_gate_that_is_not_a_rate_limit_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::the_relaunch_timer_writer_approves_a_rate_limit_gate_once"
        status: pass
    human_judgment: false
  - id: D5
    description: "Answers to a rate-limit gate map to Relaunch { None }, Relaunch { Some(note) } byte for byte, or from_response's Abort; none advances, and other kinds keep from_response's actions"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::rate_limit_gate_answers_map_to_relaunch_or_abort"
        status: pass
    human_judgment: false
  - id: D6
    description: "The rate-limit wait bound is the later of the gate timeout and the reset plus margin"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/reset_time.rs#reset_time::tests::the_wait_bound_is_the_later_of_the_gate_timeout_and_the_reset"
        status: pass
    human_judgment: false
  - id: D7
    description: "State.rate_limit_auto_relaunch_used persists under its own key and defaults to false on an older state"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::rate_limit_auto_relaunch_used_defaults_to_false"
        status: pass
    human_judgment: false
  - id: D8
    description: "The routers route a rate_limit gate's approve to relaunch_now and its reject to relaunch_with_note; a clean_pass and a preflight gate with the same context keep their own routes; gate reject / approve print those routes and the agent, and refuse a whitespace-only note"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_rate_limit_gate_routes_approve_and_reject_to_a_relaunch_of_its_stage"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::gate_reject_on_a_rate_limit_gate_prints_the_relaunch_with_the_note"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::the_reject_path_takes_the_route_reject_route_names"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::the_approve_path_takes_the_route_approve_route_names"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::workflow_finished_is_reached_only_when_can_finish_holds"
        status: pass
    human_judgment: false
  - id: D9
    description: "The live rate-limit writer opens a rate_limit gate led by the routers' way-on sentence, naming no answer after it"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::every_gate_names_only_the_routes_its_answers_run"
        status: pass
    human_judgment: false
  - id: D10
    description: "A real rate-limited agent run waiting at the gate, the timer's automatic relaunch, and a real sweep over that gate"
    verification: []
    human_judgment: true
    rationale: "Nothing here waits on a real reset or runs a real agent: the timer is not wired (52-32), and every gate test uses a tempdir fixture and an injected clock"

duration: 34min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 31: The rate-limit gate's timing, kind and timer writer Summary

**A rate-limit gate is now its own kind (`gate_kind: rate_limit`). When the denial named a reset, its file records `not_before` (the reset plus 60 seconds), and `devflow gate sweep` leaves it alone until then. An approve relaunches the stage now on the same agent; a reject relaunches it with the note. The live writer opens this gate, led by the routers' way-on sentence. The pieces 52-32 wires are also in place, pinned by tests: the timing rules, a timer writer that can only approve a rate-limit gate and only once, `GateAction::Relaunch`, and the run's used flag. Nothing calls the timer yet.**

## Performance

- **Duration:** about 34 min
- **Started:** 2026-11-11T14:44:57Z
- **Completed:** 2026-11-11T15:19:34Z
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments

- **Timing (Tasks 1, 2).** `reset_time` gains:
  - `RATE_LIMIT_RELAUNCH_MARGIN_SECS = 60`, a constant with no env var or config key;
  - `relaunch_not_before(reset)`, which saturates instead of overflowing;
  - `auto_relaunch_due(now, reset)`, true exactly at the reset plus the margin;
  - `rate_limit_wait_bound(now, reset, timeout)`, the later of the timeout and the time to the not-before time.
- **The gate's kind and not-before time (Task 1).**
  - `GateKind::RateLimit` is a new kind.
  - `GateFile`, `GateLabel` and `OpenGate` gain `not_before: Option<i64>`. In `GateFile` it is `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  - `GateLabel::rate_limit(reset_at)` takes the reset time only, so the not-before time cannot depend on a timer or the used flag (D-38).
- **Sweep (Task 1).**
  - `gate_sweep_may_reap(stage, waiter_live, not_before, now)` refuses a rate-limit gate before its not-before time. The sweep prints `left … alone — a rate-limit gate waiting for its reset (not before N); not reaped`.
  - `gate_sweep` now delegates to `gate_sweep_at(.., now)`, so the test injects the time instead of sleeping.
- **The routers (Task 1).**
  - `answer_family` maps `RateLimit` to its own `AnswerFamily::RateLimit`. This family has no status and no finish route.
  - `status_less_step` gives:
    - approve → `StatusLessStep::RelaunchNow` (`relaunch_now`);
    - reject → `RelaunchWithNote(note)`;
    - an empty note → refused.
  - The canary is not read.
  - `approve_route`, `reject_route`, `way_on_sentence`, `screen_answer` and `run_status_less_step` list the family. `RelaunchNow` launches the stage the way `RetryStage` does.
- **The live writer (Task 1).**
  - `handle_rate_limited_outcome` parses the reset with `parse_reset_time` and opens the gate with `GateLabel::rate_limit(reset)`, then dispatches the answer through `dispatch_answer`.
  - Its context is now `lead_with_way_on(GateKind::RateLimit, ..)` followed by the resume command, the reset and the cause. The 52-09 answer text is gone.
  - OPERATIONS.md's rate-limit paragraph now names the kind, the routes, the empty-note refusal and the sweep's not-before rule.
- **Timer writer (Task 2).**
  - `Gates::rate_limit_timer_answer(root, phase, stage, observed)` takes no boolean.
  - It reads the gate file and refuses before writing anything when the gate id differs (`GateIdMismatch`), the file is missing (`NoOpenGate`), or the file does not record `rate_limit` (new `GateError::NotARateLimitGate`).
  - Otherwise it publishes an approve with `responded_by: devflow-rate-limit-timer` (`RATE_LIMIT_TIMER`), through the same `answer_with_hook` first-writer-wins path as every answer.
- **Relaunch action (Task 2).** `GateAction::Relaunch { note: Option<String> }` and `GateAction::from_gate_answer(kind, response)`:
  - on a rate-limit gate, approve → `Relaunch { None }`, reject → `Relaunch { Some(note) }` with the note unchanged, abort → `from_response`'s `Abort(reason)`;
  - on every other kind, `from_response`.
- **Used flag (Task 2).** `State.rate_limit_auto_relaunch_used` is `#[serde(default)]`. Its doc says what D-38 sets. Nothing sets it yet: the timer writer does not touch it, and 52-32 orders the flag and the write.
- **The print (Task 3).** `gate reject` on a rate-limit gate writes the reject and prints `route: relaunch_with_note — relaunches the <stage> stage with the note, on <agent> (the run's current agent)` and the note. `gate approve` prints `relaunch_now … on <agent>`. The route comes from the routers, with no separate marker check. The empty-note refusal is 52-29's screen, unchanged.

## `GateAction::Relaunch` consumers (Task 2 acceptance)

The compiler (`cargo check -p devflow --all-targets`) listed four exhaustive `GateAction` matches. Each gained an explicit `Relaunch` arm, with no wildcard:

| Consumer | `Relaunch` arm |
|---|---|
| `commands.rs` stop path (`stop_via_gate`'s answered-gate read) | Counted as a non-abort answer, beside `Advance` and `LoopBack`: "already has a non-abort response; stop did not write it" |
| `pipeline_gate.rs` `open_and_await`, the `gate_resolved` `action` label | `"relaunch"` |
| `pipeline_outcomes.rs` `screen_answer` | `Err(UNEXPECTED_RELAUNCH)`. The supervisor's refusal line and reopened gate name the stage and `relaunch` |
| `pipeline_outcomes.rs` `route_answer` | `Answer::Approve(ApproveRoute::Refused(UNEXPECTED_RELAUNCH))`. `dispatch_answer` returns it as `phase N: a refused relaunch on the <stage> gate reached the dispatch: …` |

Both `pipeline_outcomes.rs` consumers are fed by `GateAction::from_response`, which never returns `Relaunch`, so neither arm is reachable today. They fail loudly rather than panic or do nothing.

The plan's read_first also named sites in `pipeline_launch.rs` (~L4096-4153) and `preflight.rs` (~L1489-1535), anchored at 1dac4f1. After 52-29, those files hold no `GateAction` match: `rg 'GateAction::' crates/devflow-cli/src/pipeline_launch.rs crates/devflow-cli/src/preflight.rs` finds doc comments only. Neither file changed. The new `from_gate_answer` matches over `GateAction` with explicit arms too.

## Task Commits

1. **Task 1 (tracer):** `ea883e8` test (RED, with compiling skeletons), `93cdd41` feat (GREEN)
2. **Task 2:** `799ebea` test (RED, with skeletons and the CLI `Relaunch` arms), `435b138` feat (GREEN)
3. **Task 3:** `680fb8d` test (RED), `6175be6` feat (GREEN)

The commit scope is `52-31` so the TDD gate's grep finds each pair. The plan names the subject `feat(core): the rate-limit gate's timing and timer writer`; Task 2's GREEN commit uses that text under the `52-31` scope. Every subject is 72 characters or fewer.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch `--profile red` config. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `reset_time::tests::auto_relaunch_is_due_exactly_at_reset_plus_margin` | `RED_EVIDENCE_OK` | `relaunch_not_before` returned the bare reset (left 1785628800, right 1785628860): no margin. |
| `gates::tests::a_rate_limit_gate_records_its_not_before_time` | `RED_EVIDENCE_OK` | Failed on `raw["not_before"] == expected`: the skeleton label wrote no not-before time. |
| `commands::tests::gate_sweep_skips_a_rate_limit_gate_before_its_reset` | `RED_EVIDENCE_OK` | The sweep reaped the rate-limit gate one second before its not-before time. |
| `gates::tests::the_relaunch_timer_writer_refuses_a_gate_that_is_not_a_rate_limit_gate` | `RED_EVIDENCE_OK` | The skeleton writer answered the ordinary gate (`Ok(..response.json)`). |
| `gates::tests::the_relaunch_timer_writer_approves_a_rate_limit_gate_once` | `RED_EVIDENCE_OK` | `responded_by` was `devflow`, not `devflow-rate-limit-timer`. |
| `gates::tests::rate_limit_gate_answers_map_to_relaunch_or_abort` | `RED_EVIDENCE_OK` | An approve mapped to `Advance`, not `Relaunch { note: None }`. |
| `reset_time::tests::the_wait_bound_is_the_later_of_the_gate_timeout_and_the_reset` | `RED_EVIDENCE_OK` | The bound was the 3-day timeout (259200), not 5 days plus the margin (432060). |
| `state::tests::rate_limit_auto_relaunch_used_defaults_to_false` | `INVALID_RED` (`unexpected_green`) | Not RED. Leaving out `#[serde(default)]` to make it fail would also have broken every test that loads a hand-written older state, inside the RED commit. The test's control is its predating-state row. |
| `commands::tests::gate_reject_on_a_rate_limit_gate_prints_the_relaunch_with_the_note` | `RED_EVIDENCE_OK` | The print named the route and the note but not the agent (`codex`). |
| `pipeline_outcomes::tests::a_rate_limit_gate_routes_approve_and_reject_to_a_relaunch_of_its_stage` | `INVALID_RED` (`unexpected_green`) | Not RED. The plan's Task 1 adds the routers' `RateLimit` arms, because the new kind does not build without them. This test pins those arms with its clean_pass and preflight controls. |

Gate sequence: `test` ea883e8 → `feat` 93cdd41; `test` 799ebea → `feat` 435b138; `test` 680fb8d → `feat` 6175be6. Each GREEN commit descends from its RED commit. There is no REFACTOR commit.

Tracer gate: the run was interactive, in `end-of-phase` mode, and Task 1's verify is automated only. Its three-command chain was re-run after `93cdd41` and exited 0, so expansion went ahead.

## Existing tests changed

- `commands::tests::gate_sweep_preview_uses_the_no_waiter_matrix`: the call gained the `not_before` and `now` arguments. Its assertions are unchanged.
- Four `sweep_one_gate(..)` test calls gained `now_secs()`.
- Struct literals of `GateFile`, `GateLabel` and `OpenGate` in tests gained `not_before: None`.
- 52-33's `commands::tests::gate_reject_prints_the_route_it_will_run` and `gate_approve_prints_the_route_it_will_run` list `AnswerFamily::RateLimit`: reject prints `relaunch_with_note`, approve prints `relaunch_now`.
- 52-29's parity tests (`the_reject_path_takes_the_route_reject_route_names`, `the_approve_path_takes_the_route_approve_route_names`) gained `ParityCase::RateLimit`:
  - the reject row drives `handle_rate_limited_outcome` at Plan, Code and Validate, and asserts the same stage, the same agent, `relaunch_note` and `gate_resolved` `relaunch_with_note`;
  - the approve row runs under an admitting and a refusing canary, and asserts no GSD read, no stop and `relaunch_now`.
- 52-29's proptest `workflow_finished_is_reached_only_when_can_finish_holds` picks the kind from `every_gate_kind`, which now lists `RateLimit`. Its independent `status_less_row` spells out the rate-limit routes. The real routing passes and the mutant control still fails.
- 52-33's `every_gate_names_only_the_routes_its_answers_run` gained a rate-limit writer row at Plan and Code, under a refusing canary. Its planted control appends the old 52-09 answer text, and the check fails on it.
- No test named in a plan verify line was renamed.

## Decisions Made

See `key-decisions`. Two need a note here:

- **The live writer was switched in this plan.** The plan says 52-32 "wires the supervisor". The orchestrator's note and 52-33's hand-off asked this plan to give the existing writer its own kind and row through the routers, so `handle_rate_limited_outcome` now opens a `rate_limit` gate. Its reject commits the note through 52-29's `RelaunchWithNote` arm, which already writes `relaunch_note`, so a reject's note reaches the relaunched stage now, not only after 52-32. What 52-32 still owns:
  - the timer;
  - the wait bound in `run_gate_with_timeout`;
  - the used flag's write;
  - any wiring of `GateAction::from_gate_answer`.
- **The agent in the print.** The plan's test asks the reject print to name the agent. The route phrases are shared with the way-on sentence, which has no state, so the agent is appended in `answer_route_lines` only, for the three steps that relaunch on the run's agent. The stage-failure prints gain the same suffix, and their tests use `contains`, so they still pass.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The not-before time needed a carrier on the open path and in the listing**
- **Found during:** Task 1
- **Issue:** The plan asks for "a way to open a rate-limit gate … through the open path's kind parameter" and for the sweep to read the not-before time. `GateLabel` had no slot for it, and `OpenGate`, which the sweep iterates, did not carry it.
- **Fix:** Added `GateLabel.not_before` with `GateLabel::rate_limit`, and `OpenGate.not_before`, which `list_open` fills from the file.
- **Commits:** `ea883e8`, `93cdd41`

**2. [Rule 2 - Missing] The timer writer needed its own refusal error**
- **Found during:** Task 2
- **Fix:** Added `GateError::NotARateLimitGate { phase, stage }`. Its message says nothing was written.
- **Commits:** `799ebea`, `435b138`

**3. [Scope] Files beyond `files_modified`:** `crates/devflow-cli/src/test_support.rs` (`every_gate_kind` lists `RateLimit`), `OPERATIONS.md` (the rate-limit paragraph), and `deferred-items.md` (dispositions below). `pipeline_launch.rs` and `preflight.rs` are listed but needed no change; see the consumers section.

**Total deviations:** 2 auto-fixed (1 blocking, 1 missing piece), 1 scope note.
**Impact on plan:** The plan's goal is met. Two of its ten named tests were not RED; see TDD Gate Compliance.

## Deferred items

- **`parse_reset_time` accepts a day past the month's end (52-01's entry).** Not taken: this plan did not change the parser. The entry now says so and defers it to 52-32. The only effect on this plan's work is a later `not_before`, never an earlier one.
- **Cron wording in comments (52-10's entry).** Not taken; noted in the entry, still open for 52-32.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>` chain (3 named runs), and re-run as the tracer gate | 0, 0 |
| Task 2 `<verify>` chain (5 named runs and `cargo check -p devflow --all-targets`) | 0 |
| Task 3 `<verify>` named runs (2) | 0 |
| `cargo nextest run -p devflow -p devflow-core` after Task 1 GREEN | 100: 1 failure, `pending_supervisor_e2e::stop_ends_a_launch_its_running_launcher_promotes_during_the_wait` (known flake 999.223); passed on its one rerun |
| `cargo nextest run -p devflow-core --lib` after Task 2 GREEN | 0 (1084 passed) |
| `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` before every commit | 0 |
| `scripts/check.sh all` after the last code change (`6175be6`) | 0 ("all OK"; 2047 passed, 0 failed across its `test result` lines) |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

semver-checks lists 15 major findings for the phase. This plan's are:

- `constructible_struct_adds_field` for `GateFile.not_before` and `OpenGate.not_before`;
- `enum_variant_added` for `GateAction::Relaunch` and `GateError::NotARateLimitGate`.

`GateKind` and `GateLabel` are new since the last release, and `State` is `#[non_exhaustive]`, so neither is flagged. 4.0.0 covers all of these, and the version was not bumped.

**What these checks do not establish:**
- Nothing calls `rate_limit_timer_answer`, `from_gate_answer` or `rate_limit_wait_bound`, and nothing sets `rate_limit_auto_relaunch_used`. They are unit contracts for 52-32 to wire. The supervisor's real wait, the timer's relaunch and the one-relaunch rule are not exercised.
- The timer writer's kind check reads the gate file before the publish. That a gate replaced in between is refused rests on the gate id check under the lock inside `answer_with_hook`. The tests do not race a replacement.
- The sweep test uses a tempdir fixture and an injected clock. No real `devflow gate sweep` ran over a real rate-limit gate.
- The print's agent is the state's agent when `gate reject` runs. The supervisor relaunches on `state.agent` when it picks the answer up, and a handoff in between is not considered.
- No real agent hit a real rate limit. The writer was driven with a stub reason string.

## Threat Flags

None beyond the plan's register. T-52-44 (timer writer: no boolean, kind check, first-writer-wins) and T-52-46 (sweep spares the gate before its not-before time) are mitigated and tested. T-52-45's single-claim half is 52-32's.

## Known Stubs

- `State.rate_limit_auto_relaunch_used` has no writer, and `Gates::rate_limit_timer_answer`, `GateAction::from_gate_answer` and `reset_time::rate_limit_wait_bound` have no production caller. This is intentional and planned: 52-32 wires them, and nothing in this plan's goal depends on a caller.

## Issues Encountered

- The first full run hit the known flake 999.223 once. Its rerun passed, and the later `scripts/check.sh all` run was clean.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-32. It needs to:

- call `Gates::rate_limit_timer_answer` at `reset_time::relaunch_not_before(reset)`, armed only while `state.rate_limit_auto_relaunch_used` is false;
- set the flag for the gate whose single claim takes the timer's answer (`responded_by == RATE_LIMIT_TIMER`);
- bound the wait with `rate_limit_wait_bound`, checking the wake before the lifetime (U8);
- decide whether the supervisor's routing should move from `from_response` plus the routers to `GateAction::from_gate_answer`. Today the routers' `RateLimit` arms already relaunch on approve and on reject with the note.

## Self-Check: PASSED

- Files exist with the planned items:
  - `crates/devflow-core/src/reset_time.rs`: `pub fn auto_relaunch_due`, `pub fn rate_limit_wait_bound`, `pub const RATE_LIMIT_RELAUNCH_MARGIN_SECS`;
  - `crates/devflow-core/src/gates.rs`: `RateLimit`, `Relaunch {`, `pub not_before`, `pub fn rate_limit_timer_answer`;
  - `crates/devflow-cli/src/pipeline_outcomes.rs`: `RateLimit`.
- Commits `ea883e8`, `93cdd41`, `799ebea`, `435b138`, `680fb8d` and `6175be6` are ancestors of HEAD, and `git rev-list --count a1d3b4c..6175be6` is 6.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
