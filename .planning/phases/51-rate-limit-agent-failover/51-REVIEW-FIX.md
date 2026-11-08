---
phase: 51-rate-limit-agent-failover
fixed_at: 2026-11-08T17:25:09Z
review_path: .planning/phases/51-rate-limit-agent-failover/51-REVIEW.md
iteration: 1
findings_in_scope: 6
fixed: 6
skipped: 0
status: all_fixed
---

# Phase 51: Code Review Fix Report

**Fixed at:** 2026-11-08T17:25:09Z
**Source review:** .planning/phases/51-rate-limit-agent-failover/51-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 6 (fix_scope `all`)
- Fixed: 6. WR-01 and WR-02 change behavior and need a human to confirm the logic. IN-02 was
  closed by the review's documentation option (recorded against backlog 999.187): the code path
  it describes still ends silently.
- Skipped: 0

**Where verification ran:** in the phase worktree `.worktrees/phase-51` on `feature/phase-51`,
which is the tree these commits are on. `workflow.use_worktrees` is `false`, so no separate
review-fix worktree was created.

**Checks run before each code commit:** `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings` (both exit 0). Each new or changed test
was run by exact name with `cargo nextest run --no-tests=fail -E 'test(=...)'`, and each has a
negative control: the fix was reverted in place, the test failed with the expected message, and
the fix was restored.

**What this does not establish:** `scripts/check.sh all` and the container check were not run.
The full `devflow` and `devflow-core` suites were not run. These ran: the related test files
`failover_e2e`, `fallback_agent_config`, `gate_notify_persist_e2e`, `start_lock_e2e`,
`auto_chain_leak_repair_e2e` and `phase7_cli`, the `doc_check` module, and filtered unit-test
sets (`resume|recovery_hints|canary`, `stage_failure|failover|handoff|never_silent|gate_context`).
All passed. The e2e stubs are DevFlow's own fixtures, not a live Claude or Antigravity CLI.

## Fixed Issues

### WR-01: After the WR-02 stop, `devflow status` sends the operator to the plain `devflow resume`, which is refused and overwrites the only record of the way on

**Files modified:** `crates/devflow-cli/src/commands.rs`, `crates/devflow-cli/src/pipeline_launch.rs`, `crates/devflow-cli/tests/failover_e2e.rs`, `OPERATIONS.md`
**Commit:** 5ced4d8
**Status:** fixed: requires human verification (behavior change)
**Applied fix:**
- `recovery_hints` now gives `devflow resume --phase N --agent <other>` as its continue verb
  when `recorded_canary_refusal(state)` is `Some`, unless a stored Ship answer selects
  `devflow ship`. This covers every hint that uses the continue verb (stale, gone while active,
  stopped), not only the stopped case the review sketched, because a plain resume is refused in
  all of them.
- New `refuse_recorded_canary_for_resume` runs in `resume_loaded` right after
  `refuse_fallback_for_resume`, before the first state write. When no `--agent` handoff to a
  different agent is requested and the run, judged with its legacy opt-out applied, has a
  recorded refusal, `resume` refuses with a message naming `--agent <other>` and writes
  nothing. `stopped` and `stop_reason` are left as they were.
- Unit test `commands::tests::recovery_hints_name_another_agent_when_the_recorded_canary_refuses_the_run`
  checks `Absent` and `Unverified`, with a `Confirmed` control.
- New e2e test `failover_e2e::after_the_refused_relaunch_status_and_resume_lead_to_another_agent`:
  1. After the refused relaunch, `devflow status` shows the `--agent` hint and no plain-resume
     line.
  2. A plain `devflow resume` fails and names the hint. It leaves `stop_reason`, `stopped` and
     the recorded `Absent` unchanged, and launches nothing.
  3. `devflow resume --agent claude` hands off (`agent_handoff`, reason `resume --agent`), and
     Claude reruns its canary (`Confirmed`) and Code.
- Negative controls: with the resume refusal removed, the e2e failed at "the refusal names the
  way on" (the old canary text came back). With the hint change disabled, the unit test failed
  (`["devflow resume --phase 7"]`).
- OPERATIONS.md's "way on" list now names the status hint and says a plain resume changes
  nothing.

### WR-02: `devflow parallel` now silently ignores a configured `fallback_agent` key, including an invalid one

**Files modified:** `crates/devflow-cli/src/parallel.rs`, `crates/devflow-cli/src/commands.rs`, `crates/devflow-core/src/config.rs`, `crates/devflow-cli/tests/fallback_agent_config.rs`, `OPERATIONS.md`, `README.md`, `docs/guides/configuration.md`
**Commit:** 1106d67
**Status:** fixed: requires human verification (behavior change)
**Applied fix:**
- `parallel` reads `config::fallback_agent(project_root)` once, before it launches anything.
  - An `Err` refuses `parallel`, as it refuses `start`.
  - `Ok(Some(agent))` prints a note that the key is not armed by `devflow parallel` and that its
    runs carry no fallback. The note names `devflow start --phase N --fallback-agent <agent>`,
    or `devflow resume --phase N --fallback-agent <agent>` for a stopped or paused run.
  - This differs from the review's suggested wording, which offered only `resume`: `resume` is
    refused over a live run, so it cannot arm a run `parallel` just launched.
- Runs still carry no fallback (`NoFallback` unchanged).
- Docs updated to match: the comments in `parallel.rs` and `commands.rs`, the doc of
  `config::fallback_agent` (doc-only, no public API change), README, the OPERATIONS.md
  `parallel` row, and the configuration guide.
- Tests:
  - `parallel_ignores_the_config_key` now also asserts the note.
  - New `parallel_refuses_a_config_key_that_names_no_agent`: the key `"nosuch"` refuses
    `parallel`, the refusal names the key and the file, and no state file is written. Control:
    with no `devflow.toml`, `parallel` succeeds and prints no note.
  - Negative control: with the config read replaced by `None`, both tests failed.

### IN-01: `recorded_canary_refusal` is a second copy of `canary_gate`'s refusal mapping, and nothing pins the two together

**Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
**Commit:** 02a2af7
**Applied fix:**
- Took the review's test option instead of extracting a shared helper.
- New `pipeline_launch::tests::recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome`
  enumerates 5 agents x 5 stages x legacy {false, true} x {Confirmed, Absent, Unverified}. For
  each, it asserts that `recorded_canary_refusal(&s).is_some()` equals
  `canary_gate(&mut s.clone(), stream_launch_enabled(..), || unreachable!()).is_err()`.
- With nothing recorded, it asserts `recorded_canary_refusal` is `None` on its own, because
  `canary_gate` runs a fresh canary there by design.
- A control asserts that the enumeration contains both admitted and refused launches.
- The doc comment of `recorded_canary_refusal` names the test.
- Negative control: mapping `Unverified` to `None` in the copy failed the test
  (`claude define legacy=false Unverified`).
- Limit: the test derives the stream flag with the same `stream_launch_enabled` call that
  `launch_stage_inner` makes. If that call site changes how it computes the flag, this test
  does not catch it.
- The commit subject uses `test(51)`, since the change is test and doc only.

### IN-02: A gate retry whose canary has not yet been recorded can still end the run silently (999.187 residual at the same entry point)

**Files modified:** `.planning/ROADMAP.md`
**Commit:** 1c77853
**Applied fix:** took the review's third option and recorded the case against backlog 999.187.
No code change. The 999.187 entry now has a paragraph that covers:
- the canary-specific path: `hand_off_agent` clears the canary, the failover relaunch fails
  before `canary_gate`, a retry passes the pre-check, and a fresh refusing canary clears the
  record;
- why Phase 51 does not fix it: writing the stop after the error needs the supervisor record
  kept on gate-answer relaunches. That needs a second guard site beside
  `FailoverRelaunchGuard`, whose single site a permanent source-scan test pins, and it is
  999.187's own fix shape, which is still marked undecided;
- the check the general fix should include.

The silent end this finding describes still happens. It is read from the code and was not
reproduced at runtime.

### IN-03: The never-silent gate still offers "retry, loop-to-code, or abort" when the recorded canary has already decided that two of the three stop the run

**Files modified:** `crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-cli/tests/failover_e2e.rs`, `OPERATIONS.md`
**Commit:** a722892
**Applied fix:**
- When `recorded_canary_refusal(state)` is `Some`, `handle_stage_failure` appends this to the
  gate context: `; a retry or loop-back will not relaunch under {agent} (the delivery canary
  recorded for this run refuses it): answer the gate, then run `devflow resume --phase {phase}
  --agent <other>``.
- The text is appended after the truncated reason, so the reason's truncation never cuts it.
  The notify hook carries the same context.
- `a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate` now asserts the exact
  sentence in the `gate_fired` context.
- Negative control: with the append disabled, the test failed and showed the old context.
- OPERATIONS.md says the same.

### IN-04: The monitor's torn-line warning now says "a Claude quota denial cannot be ruled out" on every agent's run, including the Antigravity run IN-04 was about

**Files modified:** `crates/devflow-core/src/monitor.rs`
**Commit:** e0683ce
**Applied fix:**
- Took the review's conditional-wording option. The warn line now reads "so if the agent is
  Claude a quota denial cannot be ruled out (only Claude's stream is classified for one)". The
  `[idle-timeout]` monitor-log line now reads "if the agent is Claude a quota denial cannot be
  ruled out".
- No signature change, and no public API change (the function is private).
- `monitor::tests::a_torn_final_line_at_the_idle_timeout_is_flagged_on_the_record` now asserts
  the new log wording.
- Negative control: restoring the old wording failed it.

---

_Fixed: 2026-11-08T17:25:09Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
