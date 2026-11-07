---
phase: 51-rate-limit-agent-failover
plan: 12
subsystem: testing
tags: [rust, e2e, failover, antigravity, delivery-canary, cron-arm, supv-02]
status: complete

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-01 failover at Code (hand_off_agent, fail_over_on_rate_limit), 51-04 start --fallback-agent, 51-06 canary cleared at the hop and the relaunch-failure gate (R2-D1), 51-07 default_stage_prompt render the stub matches on"
provides:
  - "e2e a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code (D-11 criterion 1, D-04, D-10)"
  - "e2e a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate (D-04 never silent, R2-D1)"
  - "e2e a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions (D-11 criterion 2)"
affects: [51-11]

actuals:
  tokens: 5701
  tasks: 2
  commits: 2
plan_head_before: a409d2891122ab3db5ebab19b50cd7ec0ee2bbb6
plan_head_after: 5ac30940616df6d62b64a0f90570d10b7bf3db4d

tech-stack:
  added: []
  patterns:
    - "Stub agents tell stage turns apart by the stage prompt on their first stdin line (/gsd-execute-phase, /gsd-plan-phase, else Define) and log one line per invocation to a directory outside the repository"
    - "A bounded wait on any of the run's possible end events, then assertions on which one, so a wrong end fails at once with the event log instead of at the timeout"

key-files:
  created:
    - crates/devflow-cli/tests/failover_e2e.rs
  modified: []

key-decisions:
  - "The wait in each test returns on any end event (workflow_finished, gate_fired, rate_limit_resume_scheduled) and then asserts the expected one, so a wrong end reports the event log immediately"
  - "The claude stub logs `stage define|plan|code` rather than a bare `stage`, so the handoff test also asserts the run went Define, Plan, Code under Claude; the agy stub logs the plan's literal `canary` / `stage`"

patterns-established:
  - "Fixture for an auto-mode `devflow start` e2e: init_repo plus a committed minimal `.planning/config.json` with a `workflow` object; the stub's Plan turn commits the plan file, so no phase directory is needed on develop"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "Auto-mode Claude run rate-limited at Code hands off to Antigravity, which runs its own delivery canary in the supervisor and completes Code on the same branch over the primary's commit"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code"
        status: pass
    human_judgment: false
  - id: D2
    description: "A handoff whose Antigravity canary withholds the token ends at the Code gate with no cron instructions (R2-D1)"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate"
        status: pass
    human_judgment: false
  - id: D3
    description: "Without a fallback, the same auto-mode rate limit at Code writes cron instructions and never invokes agy"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-11-07
---

# Phase 51 Plan 12: Auto-Mode Claude → Antigravity Failover E2E Summary

**Three tests through the real `devflow` binary show the auto-mode failover at Code. The first runs `devflow start --mode auto --until code --fallback-agent antigravity`: Claude runs Define, Plan and Code, is rate-limited on its Code turn, and Antigravity runs its own delivery canary inside the supervisor and then completes Code on the same branch. In the second, Antigravity's canary withholds the token and the run ends at the Code gate, not on the cron arm. In the third, without a fallback, the same rate limit writes cron instructions and `agy` is never invoked.**

## Performance

- **Duration:** about 5 min
- **Started:** 2026-11-07T22:58:24Z
- **Completed:** 2026-11-07T23:03:09Z
- **Tasks:** 2
- **Files modified:** 1 (created)

## Accomplishments

- `crates/devflow-cli/tests/failover_e2e.rs`. It declares `mod common;`, spawns `devflow` only through `common::hermetic_devflow`, sets PATH on the builder, and reads no `DEVFLOW_*` variable. Each test reaps its run with `LaunchedRunReaper` and bounds every wait at 120 s.
- The `claude` stub answers its canary like `WAITING_CLAUDE`. It tells its stage turns apart by the prompt on its first stdin line. On the Define turn it prints success. On the Plan turn it commits `.planning/phases/<N>-failover/<N>-01-PLAN.md`, a plan with no checkpoint, then prints success. On the Code turn it runs `git commit --allow-empty` and records that commit outside the repo. It then prints the `V3_INIT_EVENT` literal and `v3_rate_limit_event("rejected")` as a literal, and exits.
- The `agy` stub returns or withholds the canary token in the Antigravity shape: `event: "result"`, with the token in `result.response`. On its stage turn it commits once, records that commit, and prints the `ANTG_INIT` and `ANTG_RESULT_MARKER` literals.
- The handoff test asserts all of the following:
  - exactly one `agent_handoff` with `stage: code`, `claude` → `antigravity`, `rate_limited`;
  - the event order `claude_delivery_canary_confirmed` < `agent_handoff` < `antigravity_delivery_canary_confirmed` < `workflow_finished` (`stopped_at`);
  - the claude log is `canary, stage define, stage plan, stage code`, and the agy log is `canary, stage`;
  - the reloaded state has `agent == Antigravity` and `fallback_consumed`;
  - the branch is unchanged, `HEAD` is the agy commit and `HEAD^` is the claude Code-turn commit;
  - there is no `rate_limit_resume_scheduled` and no cron file.
- The refusal test asserts the event order `agent_handoff` (code) < `agent_handoff_relaunch_failed` (code, `antigravity`) < `gate_fired` (code). It also asserts no cron event or file, no `stopped_at`, an agy log of exactly `canary`, and `agent == Antigravity`, `fallback_consumed`, `canary == Absent`, `gate_pending`.
- The no-fallback test asserts exactly one `rate_limit_resume_scheduled` at `code`. The cron file has resume args `["resume","--phase","5122"]` and a non-empty `hermes_cron.schedule`. It also asserts zero `agent_handoff`, an empty agy log, `agent == Claude` and `fallback_agent == None`.

## Task Commits

1. **Task 1 (tracer): auto-mode Claude → Antigravity failover at Code, and the canary-refusal gate** - `750ab1a` (test)
2. **Task 2: the no-fallback cron arm under the same rig** - `5ac3094` (test)

Each task has a single commit and no GREEN or REFACTOR commit. The plan adds tests only, against behaviour 51-01, 51-04 and 51-06 already shipped. The shared fixture (`Rig`) was written as a helper in Task 1, so Task 2 added only its test, with no refactor.

## Files Created/Modified

- `crates/devflow-cli/tests/failover_e2e.rs`: the three tests, the `Rig` fixture, the two stub scripts and the event-shape literals copied from core tests.

## Fixture additions for auto mode

These were added to this file's fixture, beyond `supervisor_e2e.rs`'s `init_repo`:
- **`.planning/config.json`**, committed on `develop`, holding `{"workflow": {"auto_advance": true, "_auto_chain_active": false}}`. `unattended_config_condition` needs a parseable file at the launch root at Define and Code.
- **A ROADMAP heading** for the phase, as `init_repo` already wrote. No phase directory is created on `develop`. A present ROADMAP heading alone passes `ensure_phase_reachable_on_base`. The Plan turn creates the plan file, and `unattended_planned_checkpoint_condition` reads it at Code.

Nothing else was needed.

## Verification

| Check | Command | Result |
|-------|---------|--------|
| Task 1 gate (also the tracer gate's re-run before Task 2) | `cargo nextest run -p devflow --test failover_e2e --no-tests=fail -E 'test(=a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code)' && cargo nextest run -p devflow --test failover_e2e --no-tests=fail -E 'test(=a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate)'` | exit 0 |
| Task 2 gate | the plan's chained command: the no-fallback test, the handoff test, `test_home_isolation::suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment`, `hermetic_spawn_sites::only_common_names_the_devflow_binary`, `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Plan-level | `cargo test -p devflow --test failover_e2e` | 3 passed, 0 failed, exit 0 |
| doc_check | `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::source_devflow_env_vars_and_subcommands_are_documented)'` | pass |
| Scans cover the new file | read `test_home_isolation.rs` and `hermetic_spawn_sites.rs` | both enumerate `tests/` with `read_dir`, not a fixed list |
| Production tree untouched | `git diff --name-only a409d28..HEAD` | only `crates/devflow-cli/tests/failover_e2e.rs` |

**Observed timeline (one run, from a temporary dump, since reverted).** The events were:
1. `workflow_started`
2. `claude_delivery_canary_confirmed` (define)
3. `stage_launched`, `advance_evaluated` success, and `transition` for define, then the same for plan
4. `stage_launched` code (claude)
5. `advance_evaluated` code `ratelimited`, `decided_by_layer: 1`, `reason: "rate limited until 1785645600"`
6. `agent_handoff` (`tree_dirty: false`, `commit_count: 2`)
7. `antigravity_delivery_canary_confirmed` (code)
8. `stage_launched` code (antigravity)
9. `advance_evaluated` code success
10. `workflow_finished` `stopped_at`

The `health` lines in the stub logs come from `emit_canary_outcome`'s `claude --version` and `agy --version` provenance probes. Each runs once, right after that agent's canary.

**Negative control: the per-agent canary.** I mutated production code for this check only. I removed `s.canary = None;` from `hand_off_agent`'s commit (`pipeline_launch.rs` L3142), so Claude's `Confirmed` crossed the hop. Both tests then failed on the intended assertion, with exit 100:
- the handoff test: `no antigravity_delivery_canary_confirmed event`;
- the refusal test: `no agent_handoff_relaunch_failed event`.

I restored the file from a saved copy and confirmed it with `cmp`. `git status --porcelain -- crates/devflow-core/src crates/devflow-cli/src` printed nothing. So the D-04 canary-order assertion and the gate assertion both depend on the in-supervisor canary actually running for Antigravity.

**Paired control between the tests.** The refusal test uses the same rig with one difference: the agy canary withholds the token. That run ends at the gate with an agy log of `canary` alone. The handoff run completes Code with `canary, stage`. The two opposite outcomes come from that one changed input.

**No probe without a fallback.** In the no-fallback run, `agy.log` was never created. A temporary check, since reverted, printed `AGYLOG-EXISTS=false`. So `start` does not probe an agent it will not launch, not even with `--version`. The test's `health` filter therefore hides nothing there.

**What these tests do not establish.**
- The stubs' event shapes are copied from DevFlow's own core tests (`V3_INIT_EVENT`, `v3_rate_limit_event`, `ANTG_INIT`, `ANTG_RESULT_MARKER`, the canary result shape). The tests therefore do not show that a real Claude or Antigravity CLI still emits those shapes. The `rejected` rate-limit event is synthetic in the core tests too: no archived capture holds a blocked stream.
- The Claude stub exits after its rate-limit event. The stream path's idle-timeout route (RESEARCH Pitfall 8, A3) is therefore not exercised: stream rate limit, then idle timeout, then denial suppression, then `RateLimited`, then failover.
- The Define and Plan turns only print success, and Plan commits a stub plan file. The tests say nothing about those stages' real work.
- The agy stage turn is a single `--allow-empty` commit plus a marker. Whether Antigravity really executes the plan is not shown.
- Each test passed on every unmutated run it got: the handoff test 5 times, the refusal test 3 times and the no-fallback test 3 times. The counts include the two dump runs and the plan-level `cargo test`. That is evidence the path works. It is not a reliability bound and does not size flakiness.

## TDD Gate Compliance

There is no RED commit, by design. The plan states that these tests exercise code that 51-01, 51-04 and 51-06 already shipped, and that a first-run pass is expected. All three tests passed on their first run. That is an `unexpected_green` against the strict RED rule, and it is recorded as such, not dressed up as a RED. The RED-equivalent evidence that the tests can fail is the production mutation above, which killed both Task 1 tests on their intended assertions. The no-fallback test's negative control is its own zero-handoff and empty-agy-log assertions. Those discriminate against the handoff test's outcome on the same rig.

## Decisions Made

- Each test waits on any of the run's end events, then asserts which one it got. A wrong end, such as a gate where completion was expected, fails at once with the event log instead of after 120 s.
- The claude stub logs `stage define`, `stage plan` and `stage code`, not a bare `stage`. The handoff test can then assert that the run went Define, Plan, Code under Claude before the hop. The agy stub keeps the plan's literal `canary` and `stage` lines.

## Deviations from Plan

None. The plan was executed as written. The one change made during execution was to a commit subject, not to the plan: Task 1's first commit subject was 82 characters, over the operator's 72-character rule. I amended my own unpushed commit to `750ab1a` before starting Task 2. The content is unchanged.

## Issues Encountered

None.

## Known Stubs

None. The stub agents are test fixtures by design and do not stand in for production behaviour.

## User Setup Required

None.

## Next Phase Readiness

The Claude → Antigravity pair that auto mode admits is now shown end to end at Code, together with its two alternative arms. 51-11, which depends on this plan, can proceed.

---
*Phase: 51-rate-limit-agent-failover*
*Completed: 2026-11-07*

## Self-Check: PASSED

- Files: `crates/devflow-cli/tests/failover_e2e.rs` and this SUMMARY exist.
- Commits: `750ab1a` and `5ac3094` are ancestors of HEAD.
- The three planned test names are present, once each.
- The cited mutation site `pipeline_launch.rs` L3142 (`s.canary = None;`) matches the current file.
