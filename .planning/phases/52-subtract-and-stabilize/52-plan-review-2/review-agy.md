## CONFIRMED

### F-01: Critical — Fail-open to finish over uncovered changes via gap loop transition (D-33 / D-41 bypass)
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:29-30`, `52-33-PLAN.md:164`
- **Quoted text:**  
  *Line 29-30:* `"The base is cleared only when a remainder computation comes back empty, when the D-41 gate is approved (a finish), on the transition to the gap loop, on any other finish, and on abort; nothing else clears or replaces it."*  
  *Line 164:* `"its digest-match control: equal digests → a stale_after_reverify gate recording gaps_found and naming src/calc_cli.py, on which a hand-written approve sets the gap marker, launches the gaps Plan and clears code_fix_base"`
- **The concrete sequence that breaks it:**
  1. Operator rejects a clean Validate pass with a note (`code_fix = Quick`, `code_fix_base = commit A`).
  2. The quick-fix agent introduces an uncovered source file `src/foo.rs` and commits it (commit B).
  3. Quick pass ends; remainder function determines `src/foo.rs` is not in `covered_files` (remainder is `[src/foo.rs]`).
  4. DevFlow triggers the D-32 re-verify pass (`FixType::StaleReverify`).
  5. The execute-phase agent finds a gap in the test suite or spec; GSD reports `gaps_found` (with matching digest).
  6. Re-verify post-check runs: `src/foo.rs` is still missing from `covered_files`. Per 52-33:29, a non-empty remainder opens `stale_after_reverify` recording `gaps_found` and naming `src/foo.rs`.
  7. The operator approves this gate (`devflow gate approve`). Per D-37 / 52-29 / 52-33:164, approving `gaps_found` starts the gap loop (Plan `--gaps` → Code `--gaps-only` → Validate) **and unconditionally clears `code_fix_base` (`code_fix_base = None`)**.
  8. The gap loop executes and closes the GSD gaps. GSD's verifier at the end of Code `--gaps-only` verifies the gap plans and reports `passed`. It does not inspect and does not list `src/foo.rs` in `covered_files`.
  9. Validate runs. Because `code_fix_base` was wiped out in step 7, Validate behaves as a standard validation pass without any remainder check.
  10. In Auto mode, Validate immediately records `workflow_finished`. In Supervise mode, a standard `clean_pass` gate opens (NOT the D-41 gate naming `src/foo.rs`), and an approval finishes the run.
  11. **Outcome:** An uncovered file (`src/foo.rs`) reaches `finished` without GSD ever verifying it and without the operator ever accepting it via D-41.
- **Evidence checked:** `52-33-PLAN.md:29-30`, `52-33-PLAN.md:164`. Neither 52-27 nor 52-33 preserves or re-checks `code_fix_base` once the gap loop starts.
- **Suggested fix:** Do not clear `code_fix_base` on transition to the gap loop. Retain `code_fix_base` across gap passes and require that the post-gap Validate verify that all paths in `<code_fix_base>..HEAD` are in `covered_files` before allowing a finish or a `clean_pass` gate.

---

### F-02: High — Operator cannot approve `human_needed` after completing manual UAT (Permanent lock / Forced quick-fix loop)
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:33`, `52-29-PLAN.md:240`, `52-33-PLAN.md:33`
- **Quoted text:**  
  *52-29:33:* `"`approve_route(gate: &GateFile) -> ApproveRoute` follows the recorded status on a verification gate: ... `human_needed` and a reader failure (D-24) → refused in `gate_respond` before anything is written, with a message naming `devflow gate reject` and `devflow gate abort`."*  
  *52-33:33:* `"an approve whose route is finish is re-checked when the supervisor picks it up: 52-29's answer screen reads gsd query verification status once more, and only passed finishes. ... No other route re-reads GSD's status"`
- **The concrete sequence that breaks it:**
  1. Validate runs and GSD reports `human_needed`.
  2. DevFlow opens a `human_needed` gate recording `gsd_status: human_needed`.
  3. D-37 states: *"a `human_needed` report cannot finish inside a run: the human check happens outside, through GSD's UAT."*
  4. The operator performs the manual UAT outside in GSD, which updates `VERIFICATION.md` to `passed`.
  5. The operator returns to DevFlow and runs `devflow gate approve 52`.
  6. `commands::gate_respond` reads the gate file from disk, evaluates `approve_route(&gate)`, sees `gsd_status: human_needed`, and unconditionally **refuses the approval before writing anything**, displaying an error that only `reject` or `abort` are permitted.
  7. If the operator tries to bypass the CLI by writing a response file manually, 52-29's supervisor screen also checks `approve_route(&gate)` over the recorded `gsd_status: human_needed` and refuses it, reopening the gate.
  8. The 52-33 pickup re-read only triggers for an approve whose route is already `finish` (52-33:33); because `approve_route` on `human_needed` returns `Refused`, the fresh `passed` status on disk is never re-read.
  9. **Outcome:** The operator is trapped. They cannot approve to finish, even after completing UAT. Their only options are `devflow gate reject` (which forces an autonomous AI agent to run an unwanted `/gsd-quick` code fix, potentially dirtying the verified tree) or `devflow gate abort`.
- **Evidence checked:** `52-29-PLAN.md:33, 240`; `52-33-PLAN.md:33`.
- **Suggested fix:** In `gate_respond` and the supervisor pickup screen, permit `gate approve` on a `human_needed` gate to re-read GSD's live status. If live status has transitioned to `passed` (from completed manual UAT), route to finish; if still `human_needed`, refuse.

---

### F-03: High — Stale gaps gate traps operator after manual report refresh (Impossible to approve without code mutation)
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md:146`, `52-29-PLAN.md:33`, `52-29-PLAN.md:188`
- **Quoted text:**  
  *52-28:146:* `"and the stale-gaps gate (what it means, that GSD has no route for it yet, #6078, and that re-running /gsd-execute-phase N by hand refreshes the report) to the Validate section."*  
  *52-29:33:* `"the D-11 stale gaps_found gate ... refused in gate_respond before anything is written"`  
  *52-29:188:* `"stale gaps_found, reader failure → refused (reject or abort)"`
- **The concrete sequence that breaks it:**
  1. Validate finds a `gaps_found` report whose digest mismatches (`stale_gaps_found`).
  2. DevFlow opens the `stale_gaps_found` gate where approve is refused.
  3. The operator reads OPERATIONS.md (mandated by 52-28:146), which instructs them: *"re-running `/gsd-execute-phase N` by hand refreshes the report"*.
  4. The operator executes `/gsd-execute-phase N` outside in an agent session. The report is refreshed and the digest now matches.
  5. The operator returns to DevFlow to run the gap loop with the refreshed report: `devflow gate approve 52`.
  6. `commands::gate_respond` evaluates `approve_route`: `GateKind::StaleGapsFound` statically refuses approval before writing.
  7. The operator cannot approve to run the gap loop. The only way to continue is `devflow gate reject`, which triggers `/gsd-quick` to perform an autonomous code edit, altering the worktree again and potentially reproducing the digest mismatch.
- **Evidence checked:** `52-28-PLAN.md:146-148`; `52-29-PLAN.md:33, 188, 245`.
- **Suggested fix:** Allow `stale_gaps_found` approve to re-check the digest against the live report. If the digest now matches, reclassify the basis to `Status(GapsFound)` and execute the gap loop.

---

### F-04: High — Inconsistent set/clear lifecycle for `code_fix_base` across 52-29 and 52-33
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:140`, `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:29`, `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:352`
- **Quoted text:**  
  *52-29:140:* `code_fix_base` is `"(the evidence root's HEAD, a new #[serde(default, skip_serializing_if = "Option::is_none")] State field, cleared with code_fix)"`  
  *52-33:352:* `"cleared with code_fix only when code_fix leaves the quick-fix and re-verify arms"`  
  *52-33:29:* `"The base is cleared only when a remainder computation comes back empty, when the D-41 gate is approved (a finish), on the transition to the gap loop, on any other finish, and on abort; nothing else clears or replaces it."`
- **The concrete sequence that breaks it:**
  1. In `crates/devflow-cli/src/pipeline_gate.rs:116`, `transition(from, to)` unconditionally sets `s.code_fix = None;` on every stage transition (including Code → Validate).
  2. If an implementer follows 52-29:140 ("cleared with `code_fix`") or 52-33:352 ("cleared when `code_fix` leaves quick-fix and re-verify"), then as soon as a quick fix completes and transitions to Validate, `code_fix` becomes `None`, which immediately clears `code_fix_base`.
  3. Consequently, if Validate opens a `stale_after_reverify` gate or if a re-verify pass must re-check remainder, `code_fix_base` is already `None`.
  4. At the next check, 52-33:27 trips: *"the remainder function returns an error, never an empty list, when `code_fix_base` is absent at a quick pass's end"*, forcing an unrecoverable `reader_failure` gate.
  5. If instead the implementer follows 52-33:29, `code_fix_base` is decoupled from `code_fix` entirely, directly violating the contract written in 52-29:140.
- **Evidence checked:** `crates/devflow-cli/src/pipeline_gate.rs:116`, `52-29-PLAN.md:140`, `52-33-PLAN.md:29, 352`.
- **Suggested fix:** Harmonize the specifications: explicitly update 52-29:140 to state that `code_fix_base` is NOT cleared with `code_fix`, but is managed independently by the remainder / finish / abort events defined in 52-33:29.

---

### F-05: High — 52-28 re-verify route fails to clear the 52-27 gap marker (`State.gap_closure_pass`)
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md:29`, `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md:7-11`, `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md:88-96`
- **Quoted text:**  
  *52-27:29:* `"The gap-closure marker survives a stop. It is set when Validate routes gaps_found and cleared only when the following Validate reads passed, when the run finishes or is aborted, or when another fix route replaces it (52-28's re-verify, 52-29's quick route, each in the same commit)."`
- **The concrete sequence that breaks it:**
  1. A run enters the gap loop; `State.gap_closure_pass` is set to `true`.
  2. During a gap round or re-verification, GSD status is reported as `stale` or `missing`.
  3. 52-28's re-verify route runs: `code_fix = StaleReverify`.
  4. 52-28 omits clearing `State.gap_closure_pass` (it does not include `state.rs` in `files_modified`).
  5. The run stops or crashes during the re-verify pass.
  6. On resume, 52-27:29 executes: *"at Code the gaps-only intent is derived from the marker, so a stop that clears `code_fix` cannot turn it into a full execute"*.
  7. Because `gap_closure_pass` remains `true`, Code relaunches with `/gsd-execute-phase N --gaps-only` instead of the re-verify command (`/gsd-execute-phase N`).
  8. In addition, `transition_resets_consecutive_failures` continues treating transitions as gap passes, corrupting failure streak tracking.
- **Evidence checked:** `52-27-PLAN.md:29`, `52-28-PLAN.md:7-11` (no `state.rs`), `52-28-PLAN.md:88-96`. 52-29:167 explicitly clears the gap marker, confirming that 52-28 dropped this required clearing point.
- **Suggested fix:** Add `crates/devflow-core/src/state.rs` to 52-28 `files_modified` and specify in Task 1 action that the stale re-verify route clears `State.gap_closure_pass` in the same commit where `code_fix` is set.

---

### F-06: Medium — Crash recovery for Rate Limit Gate in 52-32 cannot function as specified on `devflow resume`
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md:28`, `52-32-PLAN.md:147`, `52-32-PLAN.md:162-168`
- **Quoted text:**  
  *52-32:147:* `"`pipeline_launch::tests::a_crash_between_the_flag_and_the_timer_answer_still_relaunches_once`: the state a crash between the two writes leaves (`rate_limit_auto_relaunch_used: true`, `rate_limit_timer_gate` naming the open rate-limit gate, no answer file for it), resumed and, separately, taken over → exactly one timer answer is published for that gate (`responded_by` names the timer) and the stage relaunches once"*  
  *52-32:162:* `"In that same arming function, before waiting: if rate_limit_timer_gate names the open rate-limit gate, call the timer writer for it now"`
- **The concrete sequence that breaks it:**
  1. A supervisor waits on rate-limit gate G1. The reset time passes.
  2. The supervisor sets `rate_limit_auto_relaunch_used = true` and `rate_limit_timer_gate = Some("G1")`.
  3. Before writing the timer answer, the supervisor process crashes.
  4. The operator runs `devflow resume --phase N`.
  5. In `crates/devflow-cli/src/pipeline_launch.rs:3531`, `resume` unconditionally calls `close_gates_for_resume()`, which executes `Gates::close_phase()` and durably removes the gate file for G1.
  6. `resume` does not wait on any gate; it immediately calls `launch_stage` to restart the stage.
  7. The arming function (`rate_limit_timer_deadline`) is never invoked during `resume`, because `resume` is not waiting on a gate.
  8. Even if `resume` attempted to call the timer writer, G1 has already been deleted by `close_gates_for_resume()`, so writing an answer fails.
  9. The test asserting that `resume` publishes a timer answer with `responded_by` naming the timer is unachievable under the existing `resume` architecture. Furthermore, `rate_limit_timer_gate` remains set to `"G1"` in state unless `resume` is specially modified to clean it up.
- **Evidence checked:** `crates/devflow-cli/src/pipeline_launch.rs:3373-3535`, `52-32-PLAN.md:28, 147, 162-168`.
- **Suggested fix:** Distinguish supervisor takeover (which enters `run_gate_with_timeout` and can recover the timer on the open gate) from CLI `resume` (which closes the open gate and launches the stage directly). On CLI `resume`, if `rate_limit_timer_gate` is set, clear `rate_limit_timer_gate` and let `resume`'s own `launch_stage` handle the relaunch, rather than expecting `resume` to publish an answer to a gate it destroys.

---

### F-07: Medium — Post-check failure counting bypasses the `failure_cap` ceiling reset
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:31`, `52-29-PLAN.md:250`, `crates/devflow-cli/src/pipeline_outcomes.rs:802-811`
- **Quoted text:**  
  *52-33:31:* `"Failure count (D-32.2, D-33): the post-check that opens a stale_after_reverify gate ... counts as one Validate failure through the same streak update 52-28's still-stale post-check uses (phase_validate_failures grows by one; consecutive_failures follows D-34's progress rule) ... The cap path is unchanged"`  
  *52-29:250:* `"(a failure-cap gate keeps reset_phase_failures_at_ceiling on an answer that continues the run, D-12)"`
- **The concrete sequence that breaks it:**
  1. A phase accumulates 9 validate failures (`phase_validate_failures = 9`).
  2. A quick fix is rejected; re-verification leaves the report stale or uncovered.
  3. The post-check in 52-28 / 52-33 increments `phase_validate_failures` to 10 (`MAX_PHASE_VALIDATE_FAILURES`).
  4. The post-check opens `GateKind::StaleAfterReverify`, NOT `GateKind::FailureCap`.
  5. The operator answers the gate to continue (e.g. approve over `missing` or `stale`, or reject to run quick fix).
  6. In `pipeline_outcomes.rs:805`, `reset_phase_failures_at_ceiling` is only called when `ceiling_gate` is true (i.e. for `GateKind::FailureCap`).
  7. Because the gate is `StaleAfterReverify`, `phase_validate_failures` is never reset to 0; it continues accumulating beyond 10.
  8. When the run eventually passes re-verification and reaches the normal `handle_validate_outcome`, `phase_validate_failures` is already >= 10, causing Validate to immediately trip the ceiling and open a `failure_cap` gate on what would have been a passing round.
- **Evidence checked:** `crates/devflow-cli/src/pipeline_outcomes.rs:802-824`, `52-29-PLAN.md:250`, `52-33-PLAN.md:31`.
- **Suggested fix:** In the post-check streak update, if `phase_validate_failures >= MAX_PHASE_VALIDATE_FAILURES`, open a `failure_cap` gate (recording the status and uncovered files in context), or ensure `reset_phase_failures_at_ceiling` is evaluated when answering `stale_after_reverify` gates where the ceiling was reached.

---

### F-08: Medium — Tooling/environment failure at `reader_failure` forces an AI code fix or abort with no retry path
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:33, 35`, `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:27`
- **Quoted text:**  
  *52-29:33:* `"reader failure (D-24) → refused in gate_respond before anything is written, with a message naming devflow gate reject and devflow gate abort"`  
  *52-29:35:* `"reject: every verification gate → /gsd-quick with the note, whatever its status or kind"`
- **The concrete sequence that breaks it:**
  1. A reader failure occurs (e.g. missing `gsd-tools`, temporary disk read glitch, non-UTF-8 path emitted by `git diff -z`, or transient frontmatter read failure).
  2. DevFlow opens a `reader_failure` gate.
  3. The operator fixes the external issue (e.g. installs `gsd-tools` or renames the non-UTF-8 path in git).
  4. The operator wants DevFlow to re-read or retry.
  5. `devflow gate approve` is refused by the CLI.
  6. The operator is forced to run `devflow gate reject --note "fixed"`.
  7. DevFlow routes the reject to `/gsd-quick`, launching an autonomous LLM agent with the task "fixed".
  8. The AI agent, having no relevant code task, may modify files, make unnecessary commits, or fail. Only after that full agent cycle finishes does DevFlow attempt to re-read.
  9. There is no clean "retry read" or "retry check" path for human-resolved tooling failures without either terminating the workflow via abort or paying for an unwanted AI agent quick-fix invocation.
- **Evidence checked:** `52-29-PLAN.md:33, 35, 184-194`, `52-33-PLAN.md:27`.
- **Suggested fix:** Either classify `reader_failure` as a status-less gate (like `StageFailure`, where approve retries the check without launching an agent), or permit `approve` on `reader_failure` to re-attempt the reader check.

---

## SUSPECTED

### S-01: Low (Speculation) — Transient GSD reader error during pickup re-read permanently blocks finish
- **Location:** `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md:34`, `52-33-PLAN.md:243`
- **Quoted text:** `"a reader failure reopens as reader_failure with no status"`
- **The concrete sequence that breaks it:**
  1. Supervise Validate completes cleanly and opens `clean_pass`.
  2. Operator runs `devflow gate approve`.
  3. Supervisor picks up the approve and executes the 52-33 pickup re-read of `gsd query verification status`.
  4. A transient I/O glitch or temporary process spawn error occurs during the GSD query, returning a `ReaderFailure`.
  5. The reopen classification function in 52-33 reopens the gate as `GateKind::ReaderFailure` with no status.
  6. Because the gate is now a `reader_failure` gate, subsequent `devflow gate approve` commands are refused by `gate_respond`. Even if the transient read error has resolved, the operator can no longer approve to finish; they are forced to run `/gsd-quick` or `abort`.
- **Evidence checked:** `52-33-PLAN.md:34, 243`.
- **Suggested fix:** Allow a gate reopened due to a pickup-time reader failure to accept `approve`, which simply retries the status read.

---

## SIMPLIFY

1. **Collapse unused `GateKind` variants in routing logic:**
   - `GateKind` has 15 variants: `CleanPass`, `GapsFound`, `HumanNeeded`, `DisagreementOverPassed`, `DisagreementOverHumanNeeded`, `AmbiguousVerdict`, `FailureCap`, `NoGapClosurePlan`, `StaleAfterReverify`, `StaleGapsFound`, `ReaderFailure`, `StageFailure`, `Preflight`, `CheckpointDeclaration`, `RateLimit`.
   - In `52-29-PLAN.md:32`, `answer_basis` routes all verification gates by `gate.gsd_status` alone, with only `StaleGapsFound` and `ReaderFailure` using `gate_kind`.
   - Nine verification variants (`CleanPass`, `GapsFound`, `HumanNeeded`, `DisagreementOverPassed`, `DisagreementOverHumanNeeded`, `AmbiguousVerdict`, `FailureCap`, `NoGapClosurePlan`, `StaleAfterReverify`) have identical routing behavior based solely on their `gsd_status`. Retaining distinct variants creates redundant match arms and test matrices across CLI and core modules without altering supervisor behavior.
2. **Remove `State.rate_limit_timer_gate` in 52-32:**
   - A rate-limit gate file on disk already contains `gate_kind: rate_limit` and its `not_before` / `resets_at` timestamp.
   - Storing the open gate ID as a separate persistent field in `State` (`State.rate_limit_timer_gate`), committing it before the timer write, and clearing it after the timer write introduces 4 distinct crash edge-cases requiring complex handling across takeover and resume.
   - If `rate_limit_auto_relaunch_used` is maintained, any supervisor resumption can directly inspect whether an open `RateLimit` gate exists whose reset time has expired, eliminating the need to synchronize an ephemeral gate ID in `State`.
3. **Consolidate `code_fix_base` git checks in 52-33:**
   - 52-33 runs `git rev-parse` and `git merge-base --is-ancestor` on `code_fix_base` during the reject handling, and then runs `git merge-base --is-ancestor` again at the end of the quick pass before computing `git diff`.
   - Running `git merge-base --is-ancestor` before `git diff` at the pass boundary is sufficient to guarantee history validity; the redundant check at reject write time adds two synchronous process spawns per gate reject with separate fail-closed handling that can be collapsed into a single check.

---

## COVERAGE

- **Read in full:**
  - `.planning/phases/52-subtract-and-stabilize/52-28-PLAN.md` (all 218 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md` (all 380 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-33-PLAN.md` (all 371 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-32-PLAN.md` (all 296 lines)
  - `.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md` (all implementation decisions D-01 through D-41)
  - `CLAUDE.md` (Repo traps, canonical test commands, dogfooding rules)
  - `.planning/phases/52-subtract-and-stabilize/52-27-PLAN.md` (gap loop, state fields, failure counters)
  - `.planning/phases/52-subtract-and-stabilize/52-22-PLAN.md` (gate kinds, Validate routing, status in hand)
  - `.planning/phases/52-subtract-and-stabilize/52-31-PLAN.md` (rate limit timer writer, relaunch note)
  - `crates/devflow-cli/src/pipeline_gate.rs` (transition logic, code_fix clearing, failure counters)
  - `crates/devflow-cli/src/pipeline_outcomes.rs` (handle_validate_outcome, streak updates, failure budget resets, relaunch_failed_stage)
  - `crates/devflow-cli/src/pipeline_launch.rs` (launch_stage, resume, close_gates_for_resume)
  - `crates/devflow-core/src/mode.rs` (failure ceilings, consecutive_failures_made_progress)
  - `crates/devflow-core/src/gates.rs` (close_phase, close_stage_locked)
  - `crates/devflow-core/src/doc_check.rs` (doc and identifier checks)
- **Skimmed:**
  - `.planning/phases/52-subtract-and-stabilize/52-REVIEWS.md` (earlier review rounds and ledger dispositions)
  - `.planning/phases/52-subtract-and-stabilize/52-VALIDATION.md` (traceability matrix)
  - `.planning/phases/52-subtract-and-stabilize/52-30-PLAN.md` (stored-whole gate context)
