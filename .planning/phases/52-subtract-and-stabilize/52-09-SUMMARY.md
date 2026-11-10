---
phase: 52-subtract-and-stabilize
plan: 09
subsystem: pipeline
tags: [rate-limit, gate, cut-03, infra-ceiling, failover]

requires:
  - phase: 52-07
    provides: retry_after_from_reason moved into pipeline_outcomes.rs
  - phase: 52-08
    provides: Validate is the last stage; finish_run
provides:
  - "handle_rate_limited_outcome opens the never-silent stage gate; no cron-instructions-NN.json, no rate_limit_resume_scheduled event"
  - "A rate limit with no usable fallback no longer increments infra_failures (D-20)"
  - "Gate context leads with `devflow resume --phase N` (plus `--agent <agent>` off claude), then the reset time or 'the denial carried no reset time', then the cause"
  - "answer_stage_failure_gate: the failure-gate answer routing, shared by handle_stage_failure and the rate-limit gate"
affects: [52-10, 52-31, 52-32, 55]

actuals:
  tokens: 14700
  tasks: 2
  commits: 4
plan_head_before: a59657128949bfcb17e9761e854e7d6e253a6070
plan_head_after: dd2179ea52b16537702264266ab58635fe06b59f

tech-stack:
  added: []
  patterns:
    - "A gate context leads with the way on before the cause, because notify and the banner show only its start"
    - "Unit tests of a path that now blocks at a gate use test_support::state_on_disk_at_gate to read the state at the gate and answer it with abort"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/failover_e2e.rs
    - OPERATIONS.md
    - crates/devflow-cli/src/main.rs
    - crates/devflow-core/src/state.rs
    - docs/guides/unattended-mode.md

key-decisions:
  - "The rate-limit gate reuses the failure-gate answer routing: approve or a plain reject relaunches the stage on the same agent, a reject whose note says abort ends the run. D-27.4's distinct approve-to-relaunch action is left to 52-31/52-32."
  - "The reset clause is shown only when parse_reset_time accepts the text after `rate limited until `; otherwise the context says the denial carried no reset time (the 429 fallback `usage limit` is not a time)."
  - "The `phase` parameter of handle_rate_limited_outcome is kept and now feeds the resume command, so the three call sites are unchanged."

patterns-established:
  - "Rate-limit gate context: way on (resume command), then reset time, then cause"

requirements-completed: [CUT-03]

coverage:
  - id: D1
    description: "A rate-limited run with no fallback waits at a blocking Code gate that gate show displays with the reset time and the resume command; no cron file, no rate_limit_resume_scheduled"
    requirement: CUT-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::primary_loop_rate_limited_waits_at_the_code_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_without_a_fallback_waits_at_a_gate_and_never_hands_off"
        status: pass
    human_judgment: false
  - id: D2
    description: "A rate limit with no usable fallback does not count toward the infra ceiling: one below it, the outcome gates instead of aborting and the counter is unchanged"
    requirement: CUT-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_rate_limit_without_a_fallback_does_not_count_as_an_infra_failure"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_unused_fallback_hands_off_one_below_the_infra_ceiling"
        status: pass
    human_judgment: false
  - id: D3
    description: "The fallback handoff path is unchanged"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/failover_e2e.rs#a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code"
        status: pass
    human_judgment: false
  - id: D4
    description: "OPERATIONS.md describes the rate-limit gate (what it shows, approve relaunches, reject with abort ends, resume after the reset)"
    verification:
      - kind: other
        ref: "cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::doc_referenced_identifiers_exist_in_source)'"
        status: pass
    human_judgment: true
    rationale: "doc_check proves the identifiers exist, not that the prose is accurate and readable; a reader should check the Rate-limit gate bullet against the behaviour"

duration: 18min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 09: Rate limit without a fallback waits at a gate Summary

**A rate limit that no fallback can take now waits at the stage's never-silent gate, and no longer writes `cron-instructions-NN.json` or emits `rate_limit_resume_scheduled`. The gate shows in `gate list`/`gate show`, fires notify, and leads with `devflow resume --phase N`, then the reset time, then the cause. It no longer charges `infra_failures`, so waiting for quota cannot trip the infra ceiling.**

## Performance

- **Duration:** about 18 min
- **Started:** 2026-11-10T22:07:23Z
- **Completed:** 2026-11-10T22:25:34Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- `handle_rate_limited_outcome` builds `rate_limit_gate_context` and calls `answer_stage_failure_gate`. The cron write, the infra bump, the ceiling check and the `rate_limit_resume_scheduled` event are gone.
- The failover fixture is now the one-fixture both-directions control for CUT-03: the cron file is absent, one Code `gate_fired`, and `devflow gate show` prints `1785645600` and `devflow resume --phase 5122`.
- New unit test for D-20, with a negative control: putting the bump back fails it with `left: 5, right: 4`.
- OPERATIONS.md's "Cron resume" answer is replaced by "Rate-limit gate". The `--fallback-agent` help, the `State` field docs and the unattended-mode guide no longer say "cron arm".

## Task Commits

1. **Task 1 (tracer): the rate-limited run waits at a gate:** `f687c96` (test, RED), `bf8a884` (feat!, GREEN)
2. **Task 2: no infra charge, tests and docs:** `b08ab01` (test), `dd2179e` (docs)

## TDD Gate Compliance

RED evidence came from nextest JUnit reports (a scratch nextest config, `--profile red`). Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `failover_e2e::a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate` | RED_EVIDENCE_OK (`target_test_failed`, exit 100) | Failed on its first planned assertion, "no cron-instructions file is written". The log showed `rate_limit_resume_scheduled` with `infra_failures: 1`, which is the behaviour being removed. It was not a build, load or fixture fault. |
| `pipeline_outcomes::tests::a_rate_limit_without_a_fallback_does_not_count_as_an_infra_failure` | Unexpected green (exit 0), as the plan allowed ("RED if the bump survived anywhere") | Written after Task 1 had already removed the bump. To show the test can fail, the bump was put back temporarily: the test failed on `left: 5, right: 4`, and the file was then restored (`cmp`-checked). |

Gate sequence: `test(52-09)` f687c96, then `feat(52-09)!` bf8a884, then `test(52-09)` b08ab01 (a pin with no feat after it, because Task 1 had already made the behaviour true). No REFACTOR commit.

## Rate-limit tests changed or deleted

`crates/devflow-cli/src/pipeline_outcomes.rs`:
- `primary_loop_rate_limited_writes_single_agent_cron_instructions`: rewritten as `primary_loop_rate_limited_waits_at_the_code_gate`. At the gate the stage is Code and both counters are 0. No cron file is written, `gate_fired` is present and `rate_limit_resume_scheduled` is absent.
- `rate_limited_at_infra_ceiling_stops_resuming_and_aborts`: deleted. D-20 reverses its subject, and the new D-20 test replaces it.
- `rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently`: kept. Its empty-cron-schedule assertion is replaced by two checks: the context says "the denial carried no reset time", and no cron file is written.
- `a_rate_limit_reads_the_infra_ceiling_from_the_committed_counter`: deleted. This path no longer reads the ceiling. The infra and ambiguous variants of that test remain.
- Added: `a_rate_limit_without_a_fallback_does_not_count_as_an_infra_failure`.

`crates/devflow-cli/src/pipeline_launch.rs`:
- `resume_with_a_fallback_agent_rearms_a_consumed_fallback_on_a_cron_paused_run`: renamed to `…_on_a_run_whose_supervisor_exited`. The `handle_rate_limited_outcome` call and its cron and infra assertions are dropped; the fixture already saved the not-stopped run with the exited supervisor record.
- `a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off`: renamed to `…_waits_at_a_gate_and_never_hands_off`. It now polls for `gate_fired`. The "N's supervisor ends" assertion is removed, because N now waits at the gate.
- `a_refused_handoff_takes_the_cron_arm_and_names_the_refusal`: renamed to `a_refused_handoff_waits_at_the_gate_and_names_the_refusal`. It reads the state at the gate, and `infra_failures` is expected to be 0 instead of 1.
- `a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm`: renamed to `…_waits_at_the_plan_gate`.
- `an_unused_fallback_hands_off_one_below_the_infra_ceiling`: its control was "no fallback, so the ceiling aborts". It is now "no fallback, no handoff, a gate, and the counter unchanged".
- `a_rate_limit_after_the_one_hop_takes_the_cron_arm`: renamed to `…_waits_at_the_gate`.
- Helper `assert_cron_resume_written` is replaced by `assert_waited_at_the_gate`.

`crates/devflow-cli/tests/failover_e2e.rs`:
- `a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions`: renamed and rewritten as `a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate`. `Rig::cron_file` is kept, because the handoff and canary tests still use it for their negative controls.

Left for 52-10, which deletes the readers: the `pipeline_launch` tests that write cron instructions directly to test their consumption (`failed_relaunch_preserves_the_phase_cron_instructions_record`, the resume-consume tests, the D-12 `save_rate_limit_paused` fixtures). In the handoff tests, the assertions that `rate_limit_resume_scheduled` is absent still hold, but they can no longer fail. Their polls that list it as an end event are still bounded by `HANDOFF_STEP_WAIT`.

## Files Created/Modified

- `crates/devflow-cli/src/pipeline_outcomes.rs`: the gate, `rate_limit_gate_context`, `answer_stage_failure_gate`, and the unit tests
- `crates/devflow-cli/src/pipeline_launch.rs`: the test dispositions above, and comments that say gate instead of cron arm
- `crates/devflow-cli/tests/failover_e2e.rs`: the rewritten fixture and the module doc
- `OPERATIONS.md`: the Rate-limit gate answer, the `start`/`resume` rows, the torn-line text and the `agent_handoff_refused` row
- `crates/devflow-cli/src/main.rs`, `crates/devflow-core/src/state.rs`, `docs/guides/unattended-mode.md`: "cron arm" wording replaced (doc comments and help text only; no `pub` signature changed)

## Decisions Made

See key-decisions. The gate takes the existing failure-gate answers. A distinct approve-to-relaunch action (D-27.4), the automatic relaunch at the reset (D-15/D-16) and sweep protection (D-27.2) are not built here; 52-31/52-32 own them.

## Deviations from Plan

**1. [Rule 3 - Convention] GREEN commit subject**
- **Found during:** Task 1 commit
- **Issue:** The plan gave the subject `feat(cli)!: a rate limit without a fallback waits at a gate`. GSD's TDD gate looks for `feat(52-09)`, and 52-08 used `feat(52-08)!:`.
- **Fix:** The unpushed commit's message was amended to `feat(52-09)!: …`. The body and the BREAKING CHANGE footer are unchanged.
- **Committed in:** bf8a884

**2. [Rule 2 - Doc accuracy] "cron arm" wording outside OPERATIONS.md**
- **Found during:** Task 2
- **Issue:** The `--fallback-agent` and `resume --fallback-agent` help in `main.rs`, the `State::fallback_agent`/`fallback_consumed` docs and `docs/guides/unattended-mode.md` still said a rate limit "takes the cron arm".
- **Fix:** The wording was changed to say the run waits at the gate. Doc text only, so it is not a semver change.
- **Committed in:** dd2179e

**3. [Rule 1 - Shape] Shared answer routing**
- **Found during:** Task 1
- **Issue:** The rate-limit gate needs `handle_stage_failure`'s answer handling with its own context.
- **Fix:** That `match` moved into `answer_stage_failure_gate`, which both callers use. `handle_stage_failure`'s behaviour is unchanged, and the full suite is green.
- **Committed in:** bf8a884

**Total deviations:** 3 (1 convention, 1 doc accuracy, 1 refactor needed by the task). **Impact:** no scope change.

## Issues Encountered

- `bf8a884` (Task 1 GREEN) is not green on its own for the `--bin devflow` unit tests. The tests that still expected the cron file were updated in Task 2 (`b08ab01`), as the plan split them. Bisecting between those two commits will see those unit tests fail.
- OPERATIONS.md still describes the cron file in three places, left for 52-10 as the plan says: the `cron-instructions-NN.json` file row (~L479), the "scheduled rate-limit resume" sentence (~L427), and "never by cron instructions" after `agent_handoff_relaunch_failed` (~L348). The first of these says the file is written, which is now false.

## Verification

- `cargo nextest run -p devflow --test failover_e2e --no-tests=fail -E 'test(=a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate)'`: exit 0
- `cargo nextest run -p devflow --test failover_e2e --no-tests=fail -E 'test(=a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code)'`: exit 0 (and all 7 `failover_e2e` tests passed)
- `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=pipeline_outcomes::tests::a_rate_limit_without_a_fallback_does_not_count_as_an_infra_failure)'`: exit 0
- `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::doc_referenced_identifiers_exist_in_source)'`: exit 0
- `scripts/check.sh all`: exit 0 on the first run ("all OK"). The known flake `status_and_doctor_name_a_held_state_write_lock` (999.221) did not show.
- What this does not establish: the e2e fixture's events are stub-printed copies of a Claude denial, not a live CLI. The reset text is shown raw (epoch `1785645600`), not as a readable time. The gate's approve and abort answers on this path are exercised only through the shared failure-gate routing tests and the abort answer the unit tests use, not by an approve on a rate-limit gate.

## User Setup Required

None.

## Next Phase Readiness

The only cron-instructions writer is gone. 52-10 can delete the readers (`consume_cron_instructions`, `load_cron_instructions`, the `ship` builders) and the three OPERATIONS.md passages above. 52-31/52-32 add the one automatic relaunch at the reset time on top of this gate.

## Self-Check: PASSED

- Files: every modified file exists. The renamed tests exist by name: nextest ran each one by exact name, 9 of 9 bin tests plus the e2e test.
- Commits f687c96, bf8a884, b08ab01 and dd2179e are ancestors of HEAD; the count from `plan_head_before` is 4.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*
