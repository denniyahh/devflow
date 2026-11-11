## CONFIRMED

### C1: Critical — Fail-open finish on `reader_failure` after a failed HEAD read during a quick reject
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:31`, `52-29-PLAN.md:34`, `52-33-PLAN.md:38`
- **Quoted Text**:
  - `52-29-PLAN.md:31`: *"If the HEAD read fails, the reject launches nothing and opens a never-silent `reader_failure` gate naming the read; the answer was valid, so it is claimed and `gate_resolved` records it, no `gate_answer_refused` is emitted, and the note is not carried over (rejecting the new gate gives it again)"*
  - `52-29-PLAN.md:34`: *"D-43 re-check (operator): on a basis whose approve is a re-check (`Status(HumanNeeded)`, the stale-`gaps_found` variant, the reader-failure variant), `gate_respond` writes the approve and prints that the supervisor re-checks GSD's status now. The answer screen, before the claim, re-reads `gsd query verification status` once, launching no agent... It then routes by `approve_route` over that result: `passed` → finish (52-33 adds the uncovered-set guard)..."*
  - `52-33-PLAN.md:38`: *"D-43 over a failed update: when 52-29's re-check runs on a `reader_failure` gate this plan opened because the update itself failed (`code_fix` is still the quick arm and `code_fix_base` is still set), it first re-runs the update, launching no agent..."*
- **Concrete Sequence That Breaks It**:
  1. A supervise run stops at a `clean_pass` Validate gate.
  2. The operator spots a bug and rejects: `devflow gate reject --note "fix bug"`.
  3. In `52-29:142`, the reject handler attempts to read HEAD to establish `code_fix_base`. The git read fails (e.g. transient git index lock, permission error, corrupted HEAD).
  4. Per `52-29:31`, Code launches nothing, `code_fix` and `code_fix_base` are left unset, the note is discarded, and DevFlow opens a `reader_failure` gate at `Stage::Validate`.
  5. The operator releases the lock / repairs git and, seeing the gate context state "approve re-checks GSD's status (D-43)", runs `devflow gate approve`.
  6. In `52-33:38`, the failed-update guard only intercepts if `code_fix_base` is set (`code_fix_base.is_some()`). Because HEAD failed, `code_fix_base` was never set.
  7. DevFlow falls through to `52-29:34`: it re-reads GSD's status, which still holds the pre-reject `passed` report.
  8. `approve_route` routes `Status(Passed)` to `ApproveRoute::Finish`.
  9. The finish guard in `finish_run` checks `State.uncovered_paths`. Because Code never ran, the set is empty.
  10. `finish_run` emits `workflow_finished {"reason":"finished","shipped":false}` and ends the run. The rejection was silently dropped, Code never ran, and the bug was never fixed.
- **Evidence Checked**: Read [crates/devflow-cli/src/pipeline_gate.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/pipeline_gate.rs#L640-L700), `52-29-PLAN.md:31-34, 142`, and `52-33-PLAN.md:38, 246-248`.
- **Suggested Fix**: On a `reader_failure` gate opened by a failed reject/HEAD read, refuse `approve` explicitly (or route `approve` to retry the quick-fix launch with the preserved note, never to GSD status re-check).

---

### C2: High — `ambiguous_verdict` over `passed` finishes the run without the agent ever passing (violating D-08 & D-42)
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:32-33`, `52-29-PLAN.md:188-190`, `52-CONTEXT.md:68` (D-08), `52-CONTEXT.md:337` (D-42)
- **Quoted Text**:
  - `52-29-PLAN.md:32-33`: *"every other verification kind (`clean_pass`, `gaps_found`, `human_needed`, `ambiguous_verdict`, `failure_cap`, `no_gap_closure_plan`, `stale_after_reverify`) routes by its recorded `gsd_status` alone... `passed` → finish, keeping the run's state as stopped with reason `finished` (D-04, D-35), for every verification kind except the two disagreement kinds"*
  - `52-29-PLAN.md:188-190`: *"`disagreement_over_passed` and `disagreement_over_human_needed` → the disagreement variant (their `gsd_status` is recorded, never routed by); `reader_failure`, or any other verification kind whose file holds no `gsd_status`, → the reader-failure variant (fail closed); every other kind → `Status` of its recorded status. Both routers match `AnswerBasis` with no wildcard arm: approve: `Status(Passed)` → finish"*
  - `52-CONTEXT.md:68` (D-08): *"- **D-08:** The Validate verdict is **pass only if** the validate-phase agent says pass **and** GSD's own reader, `query verification status`, returns `passed`."*
  - `52-CONTEXT.md:337` (D-42): *"- **D-42 (operator, 2026-11-09): a disagreement gate never finishes** ... A finish always needs the agent's pass and GSD's `passed`."*
- **Concrete Sequence That Breaks It**:
  1. Validate runs. The validate agent crashes, outputs garbage, omits the verdict marker, or returns conflicting statements (`ValidateOutcome::Ambiguous`). The agent DID NOT say pass.
  2. GSD's report on disk (from an earlier pass or unchanged frontmatter) says `status: passed`.
  3. `handle_validate_outcome` opens an `ambiguous_verdict` gate recording `gsd_status: Some(GsdStatus::Passed)`.
  4. In `52-29:189`, `answer_basis` routes `ambiguous_verdict` solely by its recorded status: `AnswerBasis::Status(GsdStatus::Passed)`.
  5. `approve_route` returns `ApproveRoute::Finish`.
  6. The operator runs `devflow gate approve`.
  7. The run finishes with `workflow_finished {"reason":"finished"}` even though the Validate agent never issued a pass verdict, directly violating D-08 and D-42.
- **Evidence Checked**: Read [crates/devflow-cli/src/pipeline_outcomes.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/pipeline_outcomes.rs#L550-L565), `52-29-PLAN.md:32-33, 188-190`, and `52-CONTEXT.md:68, 337-343`.
- **Suggested Fix**: Map `ambiguous_verdict` to a variant where `approve` is either refused (requiring `reject` with a note or `abort`) or re-executes Validate. It must never route `Status(Passed) -> Finish`.

---

### C3: High — `devflow resume` permanently spends the automatic rate-limit relaunch when an operator manually answered the gate before crash/stop, violating D-38
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md:28`, `52-32-PLAN.md:148`, `52-CONTEXT.md:297-300` (D-38)
- **Quoted Text**:
  - `52-32-PLAN.md:28`: *"A CLI `devflow resume` is different (R7): it closes every gate of the phase first (`close_gates_for_resume` → `Gates::close_phase`, `pipeline_launch.rs` ~L3512-3535 at b8df457) and relaunches the stage itself, so with `rate_limit_timer_gate` set it clears the id, writes no timer answer, and lets its own relaunch run (one launch); the flag stays spent (true), whichever answer, if any, was on the closed gate."*
  - `52-32-PLAN.md:148`: *"a resume row: the same state resumed through `devflow resume` → the gate is closed with its answer set aside (one `resume_found_unapplied_answer` event), no new answer is written, the stage relaunches exactly once, the flag stays true and the id is cleared; control in the same test: the same state with an operator's answer on the gate instead, taken over → the operator's answer is taken and the flag is false again"*
  - `52-CONTEXT.md:297-299` (D-38): *"- **D-38: the automatic-relaunch claim is per gate** (amends D-16, D-27.3). A manual approve of a rate-limit gate relaunches without spending the run's one automatic relaunch. A later denial with a known reset time still gets its one automatic relaunch."*
- **Concrete Sequence That Breaks It**:
  1. A stage hits a rate limit with a reset time. A rate-limit gate opens.
  2. The operator manually answers: `devflow gate reject --note "back off"` or `devflow gate approve`.
  3. Concurrently at the reset time, the timer fires, committing `rate_limit_auto_relaunch_used = true` and `rate_limit_timer_gate = Some(id)`.
  4. The supervisor dies before claiming the answer.
  5. The operator follows instructions and runs `devflow resume --phase N` (per `52-32:200`, `resume` is the sanctioned command when the waiter is gone).
  6. `devflow resume` closes the gate and unconditionally leaves `rate_limit_auto_relaunch_used = true` ("whichever answer, if any, was on the closed gate").
  7. Later in the same run, a second rate limit with a known reset occurs.
  8. Because `rate_limit_auto_relaunch_used` is `true`, no timer is armed.
  9. The run's single automatic relaunch was consumed by a manual operator answer, violating D-38. While supervisor takeover correctly resets the flag to false upon finding an operator's `responded_by` on the gate (`52-32:148`), `devflow resume` blindly burns the quota.
- **Evidence Checked**: Read [crates/devflow-cli/src/pipeline_launch.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L3512-L3536), `52-32-PLAN.md:28, 147-148`, and `52-CONTEXT.md:297-300`.
- **Suggested Fix**: In `close_gates_for_resume`, inspect the unapplied answer on `rate_limit_timer_gate`. If `responded_by` indicates a human answer (or no timer answer won), reset `rate_limit_auto_relaunch_used` to `false`, matching takeover.

---

### C4: Medium — `52-28-PLAN.md` must-have truths and Task 3 specify pre-D-43 rules (refusing approve on `stale_gaps_found` and `human_needed`), directly contradicting D-43 and `52-29-PLAN.md`
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md:25`, `52-28-PLAN.md:28`, `52-28-PLAN.md:157-158`, `52-CONTEXT.md:344-352` (D-43), `52-29-PLAN.md:34`
- **Quoted Text**:
  - `52-28-PLAN.md:25`: *"`human_needed` → 52-22's `human_needed` gate kind with the status in its context, opened through 52-22's open path and taking the routes 52-22 and 52-29 give a `human_needed` Validate (D-37: approve is not offered)"*
  - `52-28-PLAN.md:28`: *"That gate records `GateFile.gate_kind = stale_gaps_found`... (D-37: 52-29 refuses approve on it, and reject runs `/gsd-quick` with the note)"*
  - `52-28-PLAN.md:157-158`: *"The answers to both gates are documented with 52-29's answer table (D-37: approve on the stale-gaps gate is refused; reject runs `/gsd-quick` with the note; abort ends the run), so this text does not describe them."*
  - `52-CONTEXT.md:344-347` (D-43): *"- **D-43 (operator, 2026-11-09): approve on a refused-approve gate re-checks** (amends D-37 for these gates). On a `human_needed` gate, a stale `gaps_found` gate (D-11) and a `reader_failure` gate, `gate approve` means 're-read now'. The supervisor re-reads GSD's status..."*
  - `52-29-PLAN.md:34`: *"D-43 re-check (operator): on a basis whose approve is a re-check (`Status(HumanNeeded)`, the stale-`gaps_found` variant, the reader-failure variant), `gate_respond` writes the approve and prints that the supervisor re-checks GSD's status now."*
- **Concrete Sequence That Breaks It**:
  1. An implementer executing 52-28 must follow truths 25 and 28 and Task 3 line 157.
  2. Task 3 explicitly requires writing to `OPERATIONS.md` that approve on `stale_gaps_found` is refused.
  3. When 52-29 executes in the next wave, it implements D-43, where approve on `stale_gaps_found` and `human_needed` is NOT refused, but accepted and re-checks.
  4. The documentation and verification in 52-28 encode an obsolete, contradictory rule that violates D-43.
- **Evidence Checked**: Read `52-28-PLAN.md:25, 28, 157-158`, `52-29-PLAN.md:34, 192, 245`, and `52-CONTEXT.md:344-352`.
- **Suggested Fix**: Update 52-28 truths and Task 3 text to reflect D-43: `gate approve` triggers a live re-check rather than being refused.

---

### C5: Medium — `52-29-PLAN.md` omits the per-phase ceiling reset when rejecting counted gates at the ceiling
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:206`, `52-28-PLAN.md:27`
- **Quoted Text**:
  - `52-28-PLAN.md:27`: *"A gate opened at the ceiling keeps its kind and status, so its routes stay the ones the evidence calls for, and also acts as the cap gate: its context says the per-phase ceiling is reached, and an answer that continues the run resets the count through `reset_phase_failures_at_ceiling` (D-12), as the failure-cap gate's answers do; abort does not reset."*
  - `52-29-PLAN.md:206`: *"Route the supervisor's reject on every gate through `reject_route`... A failure-cap gate keeps `reset_phase_failures_at_ceiling` on this answer (D-12)."*
  - `52-29-PLAN.md:257-258`: *"(a failure-cap gate keeps `reset_phase_failures_at_ceiling` on an answer that continues the run, D-12, and so does a counted gate 52-28's streak update opened at the per-phase ceiling)"*
- **Concrete Sequence That Breaks It**:
  1. A counted gate opened outside `handle_validate_outcome` (`stale_after_reverify`, `no_gap_closure_plan`, or 52-33's post-check gate) increments `phase_validate_failures` to 10 (`MAX_PHASE_VALIDATE_FAILURES`).
  2. The gate opens with context indicating the ceiling is reached.
  3. The operator rejects with a note to trigger a quick fix.
  4. In `52-29:206`, `reject_route` execution only invokes `reset_phase_failures_at_ceiling` if the gate kind is `failure_cap`. Counted gates opened at the ceiling are omitted.
  5. `phase_validate_failures` is never reset to 0; subsequent failures immediately re-trip the ceiling on cycle 1.
- **Evidence Checked**: Read `52-28-PLAN.md:27`, `52-29-PLAN.md:206, 257-258`, and [crates/devflow-cli/src/pipeline_outcomes.rs](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/pipeline_outcomes.rs#L802-L825).
- **Suggested Fix**: Update `52-29-PLAN.md:206` to specify that counted gates opened at the ceiling also call `reset_phase_failures_at_ceiling` when answered with `reject`.

---

### C6: Low — `52-28-PLAN.md` Task 1 test asserts on non-existent event name `reset_phase_failures_at_ceiling` instead of `phase_failure_budget_reset`
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md:88`, `crates/devflow-cli/src/pipeline_outcomes.rs:812-815`
- **Quoted Text**:
  - `52-28-PLAN.md:88`: *"`pipeline_outcomes::tests::a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered`: ... the count reset to 0 with the reset event reset_phase_failures_at_ceiling emits"*
  - `crates/devflow-cli/src/pipeline_outcomes.rs:812-815`:
    ```rust
    events::emit(
        project_root,
        state.phase,
        "phase_failure_budget_reset",
        serde_json::json!({
            "phase_validate_failures_before": spent,
            "ceiling": mode::MAX_PHASE_VALIDATE_FAILURES,
        }),
    );
    ```
- **Concrete Sequence That Breaks It**:
  1. Task 1 writes unit test `a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered`.
  2. Following line 88 literally, the test asserts an event named `"reset_phase_failures_at_ceiling"` was emitted.
  3. `reset_phase_failures_at_ceiling` in `pipeline_outcomes.rs:815` emits `"phase_failure_budget_reset"`.
  4. The test fails unless the event name is corrected.
- **Evidence Checked**: Read [crates/devflow-cli/src/pipeline_outcomes.rs:812-820](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/pipeline_outcomes.rs#L812-L820) and `52-28-PLAN.md:88`.
- **Suggested Fix**: Change the event name in `52-28-PLAN.md:88` from `reset_phase_failures_at_ceiling` to `phase_failure_budget_reset`.

---

### C7: Low — False premise across 52-27, 52-29, 52-33 that `devflow stop` clears `code_fix`
- **File:Line**: `.planning/phases/52-29-PLAN.md:31`, `52-33-PLAN.md:304`, `52-27-PLAN.md:30`, `crates/devflow-cli/src/commands.rs:4306-4336`, `crates/devflow-cli/src/pipeline_launch.rs:14262`
- **Quoted Text**:
  - `52-29-PLAN.md:31`: *"`code_fix_base`... is cleared with `code_fix`, at every site that clears or replaces `code_fix` (`transition`, the `--until` halt, stop, finish, abort, another fix route)."*
  - `52-33-PLAN.md:304`: *"T-52-60: Repudiation: reject (or a stop, which clears `code_fix` and so the base) over a gate whose pass's files could not be listed"*
  - `crates/devflow-cli/src/pipeline_launch.rs:14262`: `assert_eq!(reloaded.code_fix, fix, "a resume keeps the fix pass");`
- **Concrete Sequence That Breaks It**:
  1. `commands.rs::persist_stopped_state` only commits `state.stopped = true` and `state.stop_reason`. It never clears `code_fix`.
  2. `pipeline_launch.rs:14262` asserts as an invariant that `devflow resume` keeps `code_fix`.
  3. The plans falsely assert in their must-haves and threat models that `devflow stop` clears `code_fix`.
  4. If an implementer follows the plans literally and adds `s.code_fix = None;` to `commands.rs::persist_stopped_state`, they will regress `pipeline_launch::tests::a_resume_keeps_the_fix_pass`.
- **Evidence Checked**: Read [crates/devflow-cli/src/commands.rs:4306-4340](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/commands.rs#L4306-L4340), [crates/devflow-cli/src/pipeline_gate.rs:86, 116](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/pipeline_gate.rs#L86), and [crates/devflow-cli/src/pipeline_launch.rs:14262](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/pipeline_launch.rs#L14262).
- **Suggested Fix**: Strike "stop" from the list of sites that clear `code_fix` in 52-29 and 52-33.

---

## SUSPECTED

### S1: Suspected — `finish_run` finish guard in 52-33 cannot identify which gate it is answering, risking infinite gate loops or premature termination
- **File:Line**: `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:32-33, 232-238`, `52-08-PLAN.md:104`
- **Quoted Text**:
  - `52-33-PLAN.md:32-33`: *"Finish guard (route A, R1): one guard in 52-08's `finish_run` runs before any finish is committed... non-empty opens the D-41 gate... When the finish answers a `stale_after_reverify` gate recording `passed`, the guard instead accepts the set: clear it in the finish commit and emit `uncovered_paths_updated` with reason `accepted`, the gate id and the files, before `workflow_finished`"*
- **Speculation / Concrete Sequence**:
  1. `finish_run(project_root, state, …)` in `52-08:104` was designed as a terminal function taking `(project_root, state)`. It is called both on auto-pass (no gate exists) and on gate approval.
  2. When called on an auto-pass where `uncovered_paths` is non-empty, `finish_run` opens the D-41 gate "instead of finishing" and returns `Ok(())`. `launch_stage` in `pipeline_launch.rs:4238` completes and returns `Ok(())` to `commands.rs:836`. The supervisor process exits because `state.stopped` was not set, leaving an unanswered gate with no supervisor alive to poll it.
  3. Conversely, when the operator approves a D-41 gate (`stale_after_reverify` over `passed`), `approve_route` routes to `Finish` and calls `finish_run`. But `finish_run` does not receive the gate object or gate ID. If `finish_run` simply re-checks `uncovered_paths` without knowing it was called as the response handler for that D-41 gate, it will see `uncovered_paths` is still non-empty and reopen the D-41 gate in an infinite loop.
- **Evidence Checked**: Read `52-08-PLAN.md:104-110`, `52-33-PLAN.md:32-33, 232-238`, and [crates/devflow-cli/src/commands.rs:836](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/5839e9e1-226b-43ee-9122-a2d0c28ed2d9/scratchpad/p52review3/snapshot/crates/devflow-cli/src/commands.rs#L836).
- **Suggested Fix**: Update `finish_run`'s signature to accept `answering_gate: Option<&GateFile>` (or `gate_id`), and specify that when called from auto-mode, opening the D-41 gate must block on `run_gate_with_timeout` rather than returning `Ok(())`.

---

## SIMPLIFY

1. **Collapse the dual opening of the D-41 gate**:
   The D-41 gate (`stale_after_reverify` recording `passed`) is currently opened at two separate lifecycle points:
   - In the re-verify post-check (`pipeline_outcomes.rs`, `52-33:31`);
   - In the finish guard inside `finish_run` (`pipeline_gate.rs`, `52-33:32`).
   Because the post-check prevents any non-empty uncovered set from ever transitioning to Validate, Validate is only reached with an empty set (unless a gap loop was approved). Moving the R1 check directly to the gap loop's transition to Validate would allow `finish_run` to remain a simple terminal cleanup function rather than an intercepting gate-opener.

2. **Remove the custom update re-run in the D-43 answer screen**:
   `52-33:38` introduces special-case logic inside the general `run_gate_with_timeout` answer screen to re-run the D-33 git update if `code_fix` is `Quick` and `code_fix_base` is set. This complicates the answer screen with git operations that duplicate `pipeline_outcomes.rs`. Allowing a standard `gate reject` to re-drive the quick pass cleanly eliminates this bespoke re-run path.

---

## COVERAGE

- **Read in full**:
  - `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md` (all 236 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md` (all 408 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md` (all 389 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md` (all 307 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-plan-review-2/SUMMARY.md` (all 34 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md` (D-01 through D-43 in full)
  - `CLAUDE.md` (all 47 lines)
- **Skimmed / Cross-referenced**:
  - `52-22-PLAN.md`, `52-27-PLAN.md`, `52-30-PLAN.md`, `52-31-PLAN.md`, `52-08-PLAN.md`, `52-REVIEWS.md`
  - Source files under `crates/`: `pipeline_outcomes.rs`, `pipeline_gate.rs`, `pipeline_launch.rs`, `state.rs`, `prompt.rs`, `gates.rs`, `mode.rs`, `commands.rs`.
  - Executed read-only workspace verification: `cargo check --workspace --tests` (exited 0).
