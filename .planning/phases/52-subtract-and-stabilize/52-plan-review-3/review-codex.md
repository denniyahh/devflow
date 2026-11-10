## CONFIRMED

“Confirmed” means the revised plans prescribe a contradictory contract or admit the stated sequence. These are not findings from an implemented round-5 build.

**F1 — High — D-43’s classifier sends repaired gates back into re-check forever.**

- **File:** `52-29-PLAN.md:34,185–192,252`.
- **Quoted text:** “any other read → the opened kind with the fresh status”; “`reader_failure` → the reader-failure variant”; “stale `gaps_found`, reader failure → a re-check”.
- **Concrete sequence:** Open `reader_failure`, repair GSD, approve, read `passed`. The classifier returns `reader_failure + passed`. `answer_basis` ignores that status for this kind, so approve remains re-check and reopens the gate. Similarly, refreshing a `stale_gaps_found` report to either `passed` or matching-digest `gaps_found` preserves the stale-gaps kind and reopens. Repeated approvals count no failure, so the ceiling never intervenes.
- **Evidence checked:** The classifier and router contracts contradict the required successful recoveries at line 252. An in-memory transcription reproduced all three loops; `human_needed → passed → finish` was the opposite-result control.
- **Suggested fix:** Return a fresh routing disposition independently of the original gate kind. Preserve the original kind as provenance; do not feed a successfully repaired result back through its failure-kind override.

**F2 — High — The failure-cap acceptance test still explicitly requires finishing after agent FAIL.**

- **File:** `52-29-PLAN.md:248`; `52-CONTEXT.md:337–341`; `52-22-PLAN.md:30,160`.
- **Quoted text:** “the agent failed three times in a row while GSD reports `passed`”, followed by “approved → `workflow_finished`”.
- **Concrete sequence:** Validate fails repeatedly while GSD reports `passed`; the ceiling produces a `failure_cap` gate recording `passed`; approve finishes. The disagreement exception protects only the two disagreement *kinds*. Relabelling this outcome as `failure_cap` restores the original R3 bypass.
- **Evidence checked:** D-42 says, without a cap exception, “A finish always needs the agent’s pass and GSD’s `passed`.” Plan 22 records the current round’s status on the cap gate. The new test supplies no subsequent agent pass.
- **Suggested fix:** Keep the failed-verdict restriction when a ceiling is reached. If this test intends to exercise a *real pass at the ceiling*, explicitly run a successful Validate after the earlier failures before opening and approving the gate.

**F3 — High — Stopping before collection silently loses an uncovered quick fix.**

- **File:** `52-29-PLAN.md:31`; `52-33-PLAN.md:27,32,304`; `52-28-PLAN.md:87`.
- **Quoted text:** The base is cleared with `code_fix` at “stop”; paths join the set “at a quick pass’s end”; “With an empty set it reads nothing and the run finishes.”
- **Concrete sequence:** Reject a clean pass. Quick commits new uncovered file `src/new.rs`. Stop before its outcome handler collects the diff. Apply the specified stop clearing: `code_fix = None`, `code_fix_base = None`, set still empty. Resume ordinary Code, then obtain a Validate pass over the unchanged old `passed` report. The finish guard sees an empty set and finishes without naming or accepting `src/new.rs`.
- **Evidence checked:** Current resume renders Code from persisted `code_fix` (`pipeline_launch.rs:140–145,3647–3669`). Current stop functions actually preserve it (`:1318–1329,1998–2001`), so the plans prescribe a dangerous change. Their stop-clearing claim also contradicts Plan 28’s stopped re-verify replay test. T-52-60 covers an already-reported failed update; this interruption happens **before any update or warning**.
- **Suggested fix:** Preserve the quick/re-verify intent, note and base across resumable stops. Collect changes before explicitly abandoning that intent.

**F4 — High — A Code post-check can manufacture a finishable human-needed gate without a passing Validate.**

- **File:** `52-28-PLAN.md:25,99–100`; `52-29-PLAN.md:34`; `52-22-PLAN.md:30`.
- **Quoted text:** At the Code post-check, “`human_needed` → `GateKind::HumanNeeded` … not counted”; its later re-check routes “`passed` → finish”.
- **Concrete sequence:** Validate agent says FAIL while GSD reports `stale`. Plan 22 launches re-verify Code. Code succeeds; its post-check reads `human_needed` and opens the ordinary human-needed gate. The operator completes UAT externally, then approves. The fresh read is `passed`; D-43 finishes. **No later Validate agent has passed.**
- **Evidence checked:** Plan 28 explicitly skips Validate on this human-needed branch. Plan 29’s failed-verdict conversion applies only to `reader_failure` and `stale_gaps_found` gates opened over a Validate FAIL; it does not protect this `human_needed` gate.
- **Suggested fix:** Distinguish gates opened after Code from gates backed by a passing Validate. After repairing this Code post-check, a fresh `passed` must launch Validate unless the run has the required passing verdict.

**F5 — Medium — The uncovered-path update leaves a non-replayable crash state.**

- **File:** `52-33-PLAN.md:26–29,138–146,246–248`.
- **Quoted text:** “union the rest into `uncovered_paths` and clear `code_fix_base`”; an absent base is an error; update recovery requires “`code_fix` is the quick arm and `code_fix_base` is set”.
- **Concrete sequence:** Quick succeeds. Commit the union and clear its base. Crash before replacing `code_fix` or transitioning. Durable state is now `stage = Code`, `code_fix = Quick`, nonempty set, **no base**. Resume replays Quick. Its next completion fails the mandatory absent-base check. Approve cannot take the specified update recovery because its predicate excludes the missing base.
- **Evidence checked:** The union commit does not clear Quick or record its completed continuation. Resume relaunches the saved stage and fix. The plan’s tests cover crash-free collection, not this cut between state commits.
- **Suggested fix:** Keep the base until leaving Quick, making repeated union operations idempotent, or persist the completed continuation atomically with collection. Add crash controls immediately before and after collection.

**F6 — High — Timer recovery still loses the automatic relaunch after the durable claim.**

- **File:** `52-32-PLAN.md:110–114,154–172`; `crates/devflow-cli/src/pipeline_gate.rs:640–701`; `crates/devflow-core/src/gates.rs:1188–1203`.
- **Quoted text:** “clear `rate_limit_timer_gate` once the write returns”; “then let the single claim decide”; recovery allegedly covers “every state the two writes can leave”.
- **Concrete sequence:** Commit flag/id, publish timer approve, clear timer id, durably claim the answer, **crash before launching the stage**. The flag remains spent and the recovery id is gone. Reclaiming that answer returns `AlreadyConsumed`; the existing handler exits without launching. The automatic relaunch happened zero times.
- **Evidence checked:** `Gates::claim` durably publishes an acknowledgement separately from launching. The handler has an existing `after_claim` hold point before its remaining state changes and returned action. Both new crash tests stop before the id clear; neither exercises claim-to-launch recovery. A no-crash control launches once.
- **Suggested fix:** Recover a pending relaunch through a durable launch reservation keyed by gate identity. Reconcile the claim with that reservation before declaring the action completed. Test crashes after claim and around reservation publication; answer publication alone establishes neither launch completion nor exactly-once execution.

**F7 — Medium — The absent-base negative test cannot pass a correct implementation.**

- **File:** `52-33-PLAN.md:171`.
- **Quoted text:** Row “(a) `code_fix_base` absent”, followed by “Rows (a), (b), (c) and (f) also assert … `code_fix_base` still set”.
- **Concrete sequence:** Supply row (a)’s absent base. Correct fail-closed handling preserves its absence and opens `reader_failure`. The required “still set” assertion fails. Inventing a base to satisfy it violates the unchanged-input/fail-closed contract.
- **Evidence checked:** The same plan requires retaining the base on update failure, rather than creating one. The absent and present cases discriminate in opposite directions.
- **Suggested fix:** Assert that the base is **unchanged**: absent in (a), still equal to its supplied value in the other rows.

## SUSPECTED

**S1 — Medium — Speculation: the reset timer may wait for the gate timeout instead of the reset deadline.**

- **File:** `52-32-PLAN.md:107–110`; `52-31-PLAN.md:29,151`; `crates/devflow-cli/src/pipeline_launch.rs:1423–1452`.
- **Quoted text:** “wait with `rate_limit_wait_bound`”; that bound is “the later of the gate timeout and … reset”; timer publication happens when the wait “ends `TimedOut`”.
- **Concrete sequence:** Reset is one hour away; gate timeout is the default three days. Passing the prescribed maximum bound to the existing wait produces no timeout at one hour, so the automatic relaunch occurs approximately three days later.
- **Evidence checked:** The existing waiter accepts one timeout and has no separate reset wakeup. The plan also says “first wait to reset + margin” and introduces a deadline helper, so an implementer could resolve this correctly; the actual split is unspecified.
- **Suggested fix:** Specify separate reset and expiry deadlines. The armed timer wakes at reset-plus-margin; the overall gate lifetime uses the maximum bound. Test a **future reset earlier than the timeout**, asserting relaunch at that reset.

## SIMPLIFY

- Separate **fresh evidence routing** from **gate provenance**. Reconstructing a synthetic gate and routing it again causes F1 and makes verdict restrictions easy to lose.
- Keep quick-pass collection idempotent across replay. Consuming the base before leaving Quick creates the special failed-update recovery and crash ambiguity.
- Extend the existing durable launch reservation protocol for timer recovery. More flag/answer crash branches cannot close the claim-to-launch gap.

## COVERAGE

- **Read in full:** all four target plans, `52-CONTEXT.md`, review-2 `SUMMARY.md`, and `CLAUDE.md`.
- **Targeted reading:** Plans 22, 26, 27, 31 and 08; earlier `52-REVIEWS.md`; source for gate waiting/claiming, stop/resume, prompt selection, stage outcomes and failure counters.
- Traced R1–R10 against the revised contracts. Checked target exact-test references against current source names and preceding/in-plan declarations; found no additional missing **basename**. This does not establish compiled module paths.
- Ran only read-only inspections and in-memory contract probes with opposite-result controls. No Rust suite or future implementation was executed.
- No files edited, scratch artifacts created, or persistent processes started.
