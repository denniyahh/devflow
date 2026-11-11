## CONFIRMED

### C1 — **D-43's re-check can never leave two of the three gates it exists for** — Critical
`52-29-PLAN.md:34` (D-43 re-check truth):
> "…any other read → **the opened kind with the fresh status**. It then routes by `approve_route` over that result: `passed` → finish … a result whose approve is still a re-check or refused reopens a gate of that kind and status before the claim"

`52-29-PLAN.md:32` + `:187` (`answer_basis` truth/action): "`stale_gaps_found` → its variant; … `reader_failure` … → the reader-failure variant; **every other kind** → `Status` of its recorded status."

These two rules are mutually incompatible for exactly the two kinds D-43 was written for. The classifier preserves the **opened kind**, and `answer_basis` **ignores `gsd_status` entirely** for `reader_failure` and `stale_gaps_found`. So the re-check's result for those gates is always the refused/re-check basis, whatever GSD now says.

Sequence (reader failure at Validate, agent passed):
1. `gsd query verification status` fails → `reader_failure` gate, no status (`52-22`, D-24).
2. Operator repairs the GSD install (exactly R4's scenario, `52-29-PLAN.md:398`) and runs `devflow gate approve`.
3. Screen re-reads → `passed` → classifier rule "any other read" → `(reader_failure, passed)`.
4. `approve_route` over that file → reader-failure variant → "approve means re-read now" → "a result whose approve is still a re-check … reopens a gate of that kind and status".
5. `reader_failure` gate is back on disk. Goto 2. The fresh read never matters.

Same for `stale_gaps_found`: a fresh `gaps_found` **with a matching digest** falls to rule 6 (rule 2 covers only mismatch/non-`v4:`), so it becomes `(stale_gaps_found, gaps_found)` → stale-gaps variant → re-check → reopen.

The plan's own tests assert the unreachable behaviour: `52-29-PLAN.md:252` — "the `stale_gaps_found` gate with the stub reading `gaps_found` over equal digests → **the gap marker and the gaps Plan launch**; the `reader_failure` gate with the stub reading `stale` → **the re-verify `FixType` and a Code launch**." And `52-33-PLAN.md:38` ("success unions the pass's paths and continues as a quick pass's end does") plus Task 3's `rechecking_a_gate_whose_update_failed_reruns_the_update` depend on the same arm succeeding.

Consequence: the only ways out are `reject` (→ `/gsd-quick`, a **code** fix for an install/reader problem, which then makes the report stale and re-enters the loop) or `abort`. That is the exact R4 defect round 5 claims to have closed, re-introduced by the classifier's fallback arm.

Evidence checked: both plan texts; `crates/devflow-core/src/gates.rs:97-107` (`GateAction::from_response` gives no identity, see C4); no source-level contradiction — this is purely the plan's own routing spec.

Fix: the classifier must return a **basis derived from the fresh read** (e.g. `AnswerBasis::Status(fresh)`) for every non-disagreement, non-reader-failure read, and carry the opened `GateKind` only as display metadata on the reopened gate file; or add a `Desynced { opened_kind, fresh }` basis whose router reads `fresh`.

### C2 — **approve finishes over an agent FAIL and over "no verdict arrived"** — High
`52-29-PLAN.md:33`:
> "`passed` → finish, keeping the run's state as stopped with reason `finished` (D-04, D-35), **for every verification kind except the two disagreement kinds, including a failure-cap gate over a real pass**"

`52-29-PLAN.md:248`:
> "a `failure_cap` gate whose `gsd_status` is `passed` (**the agent failed three times in a row while GSD reports `passed`**), approved → `workflow_finished`"

`52-22-PLAN.md:35` records `gsd_status` on "the ambiguous-verdict gate and the failure-cap gate included", and defines `ambiguous_verdict` as "the existing ambiguous arm" — which in the current code is "the probe passes but **NO agent verdict arrived**" (`crates/devflow-cli/src/pipeline_outcomes.rs:1935`).

Against `52-CONTEXT.md:337-343` (D-42): "**A finish always needs the agent's pass and GSD's `passed`**" and `:68` (D-08). D-42 carved out only the two disagreement kinds, so two gates remain where the run finishes with no agent pass: `failure_cap`+`passed` (agent failed 3×) and `ambiguous_verdict`+`passed` (no verdict). D-37 also says "including a failure-cap gate over a real pass", so **D-37 and D-42 contradict each other** and round 5 silently let D-37 win for these two kinds. No plan text flags it; the tests pin it (`approving_a_failure_cap_gate_over_a_passed_report_finishes`) so a later fix must break a named test.

Sequence: Validate → agent fails 3× while the GSD report says `passed` → `failure_cap` gate → `gate approve` → `Status(Passed)` → `finish_run` (uncovered set empty) → `workflow_finished {"reason":"finished"}`. No agent pass was ever recorded.

This is an operator decision, not mine to make: either D-42's rule wins (add these two kinds to the disagreement carve-out, or add a `no_agent_pass` basis that re-checks/refuses) or D-37 must be amended in writing.

### C3 — **a stop between a quick pass and its D-33 update permanently drops the pass's coverage, silently** — High
`52-33-PLAN.md:26` asserts the invariant: "holds **every path a quick fix changed that no report has covered and no approve has accepted**".
`52-29-PLAN.md:31`: "`code_fix_base` … is cleared with `code_fix`, at every site that clears or replaces `code_fix` (`transition`, the `--until` halt, **stop**, finish, abort, another fix route)".
`52-33-PLAN.md:304` (T-52-60): "a stop, which clears `code_fix` and so the base … a pass whose diff failed … never joins the set."

Two sequences, neither involving an approve:

**(a) update failed.** quick reject → base=HEAD → Code pass commits `src/calc_cli.py` → pass end → update `Err` (agent rewrote history; git failure; non-UTF-8 path) → `reader_failure` gate, set unchanged (empty), base kept → operator `devflow stop` instead of answering → resume. `crates/devflow-cli/src/pipeline_launch.rs:3512-3531` closes **every** gate of the phase (`Gates::close_phase`) before relaunching; `52-33:38`'s update re-run only happens inside the approve re-check, which now has no gate; the base is gone (above) so no update runs at the next pass end. Validate `passed` → finish guard sees an empty set → `workflow_finished`. The `unlisted` event is the only trace; no gate, ever.

**(b) stop before the pass ends.** Same, without even an `unlisted` event: reject → base=HEAD → Code launches the quick fix → agent commits partway → `devflow stop` → resume relaunches Code (base and `code_fix` cleared by the plan's own rule) → quick fix's file sits in the branch, never listed, never in the set → finish. The operator never answered any gate.

Evidence checked: `pipeline_launch.rs:3512-3531` (resume closes all gates), `pipeline_launch.rs:1318-1329` (`record_advance_stop` writes stopped/gate_pending only), `pipeline_gate.rs:86` and `:116` (`code_fix` cleared on `--until` halt and on transition). Nothing in 52-33 re-runs the update on resume; nothing records the base outside `code_fix`'s lifetime.

Fix shape: persist the per-pass base independently of `code_fix`/stop (e.g. `pending_code_fix_base` that survives stop/resume and is consumed only by a listing or a finish), or make the finish guard refuse to finish while an unlisted quick pass exists; at minimum stop must not be able to clear the record before the paths are listed.

### C4 — **the D-41 accept has no carrier, and both degenerate implementations are bad** — High
`52-33-PLAN.md:236`:
> "Add the finish guard at the top of 52-08's `finish_run` … **When the finish answers a `stale_after_reverify` gate recording `passed`, the guard instead accepts the set**: clear it in the finish commit and emit `uncovered_paths_updated` with reason `accepted`, the gate id and the files"

`finish_run` is reached from a gate handler that receives only a `GateAction` (`52-29-PLAN.md:210`: "Route every supervisor approve through `approve_route` … over the gate file read back from disk"), and `GateAction` is a payload-free enum in `crates/devflow-core/src/gates.rs:88-107`. **`52-33-PLAN.md`'s `files_modified` does not include `crates/devflow-core/src/gates.rs`**, so the gate kind/status/id cannot ride on the action; the plan never names a parameter or any other carrier from the answer screen into `finish_run`.

Both fallbacks are defects:
- accept whenever the set is non-empty at an approve-driven finish → `52-33:33`'s D-41 gate becomes a no-op, and an operator reject/approve ordering can finish over uncovered files with no gate at all;
- never accept → approving the D-41 gate runs the guard, finds the set non-empty (the accept didn't happen), re-opens the same gate: the run can only leave it by `reject` or `abort`.

Evidence checked: `gates.rs:88-107` (`GateAction` variants: `Advance`, `LoopBack(Stage)`, `Abort(String)`); `pipeline_gate.rs:661-700` (claim → `GateAction::from_response(&response)` → `Ok(action)`, no gate identity returned); 52-33 frontmatter.

### C5 — **the finish guard trusts a field the agent under verification writes** — Medium-High
`52-33-PLAN.md:291` (Trust Boundaries): "**report on disk → re-verify clearance and finish** | the report's `covered_files`, **not an agent's claim**, decides whether the fix was seen".

But `covered_files` lives in `VERIFICATION.md`, which the Code/Validate agent can edit, and nothing in either plan covers it: D-11's `fingerprint` (`52-28-PLAN.md:36`, `:44`) is "recomputed … over the same phase directory and files" — it hashes the tree, not the report's frontmatter — and the digest check runs **only on `gaps_found`**, not on `passed`.

Sequence: quick fix adds `src/calc_cli.py`; the run goes to a re-verify whose agent "skips the verifier" (#6096, the whole premise of D-32); it nonetheless appends `src/calc_cli.py: …` to the report's `covered_files` → the post-check's subtraction empties the set → Validate → finish guard: empty set → `workflow_finished`, no gate. No threat row names this write (`52-33:296-301` lists T-52-41/56/57/58/60/SC only), while the sibling *assumption* about `.devflow/gates/` got its own accepted row (T-52-59, `52-29:315`). This is the same class, in the file whose tampering the phase exists to catch, and it defeats `T-52-57` ("re-verify agent that skips the verifier").

### C6 — **52-32's "stays open for the operator" / "exactly once" is contradicted by 52-31's sweep floor and by resume's flag handling** — Medium
`52-31-PLAN.md:30`: "…its not-before time (`reset_at + margin`) … `gate sweep` does not reap it before that time, and **reaps it after**".
`52-32-PLAN.md:28`: "A second denial … opens a gate that records its `not_before` but arms no timer, and **it stays open for the operator**" and "the waiting supervisor relaunches the stage by itself **exactly once**".

`not_before` is exactly the timer's deadline (`52-31`'s `auto_relaunch_is_due_exactly_at_reset_plus_margin`), and `crates/devflow-cli/src/commands.rs:2684-2686` reaps any gate with a **live waiter** once aged (`config_parse.rs:142-143`: default 3 days = the gate-timeout default). `Gates::reap_answer` (`crates/devflow-core/src/gates.rs:680-694`) writes `approved: false` with note `"abort: reaped by devflow gate sweep…"`, which D-13 turns into an **abort**. So for a reset farther out than the max age, a routine `devflow gate sweep` run *after* the reset converts the promised relaunch (and the second-denial "stays open") into an aborted run. "Exactly once" becomes "at most once, decided by whichever of the timer and the sweep gets there first".

Second half of the same finding: the flag is committed **before** the answer is written (`52-32:28`), and `devflow resume` deliberately "keeps the flag spent (true)" — so a crash in that window spends the run's one automatic relaunch on a relaunch that never happened, permanently, for that run.

### C7 — **counter semantics: `phase_validate_failures` is incremented by non-Validate passes and can never bound anything** — Medium
`52-28-PLAN.md:32` (route I): one extracted streak update "+1 and D-34's progress rule … that every counted gate calls, 52-27's included", with "an answer that continues the run resets the count through `reset_phase_failures_at_ceiling`". 52-27's `no_gap_closure_plan` gate fires on a **Plan** pass; 52-33's counted gates fire on a **Code** re-verify pass. The counter they increment is the one the operator is shown as `"Validation has failed {} time(s) for this phase"` (`crates/devflow-cli/src/pipeline_outcomes.rs:717`) and the one `mode.should_gate` / `mode::phase_failure_ceiling_reached` read. After this phase, a Plan gap-plan miss is reported to the operator as a failed *validation*, and the ceiling message ("This phase recorded N Validate failure(s)", `pipeline_outcomes.rs:700-704`) counts passes that were never validations.

And the reset rule makes the ceiling unable to bound: it fires only at exactly 10, and **any** continuing answer resets to 0, so the count is `≤10` forever and the ceiling adds a sentence, not a bound. That is only tolerable because every counted gate needs an operator answer — which is worth stating, because it is the only thing keeping `MAX_PHASE_VALIDATE_FAILURES` meaningful; `52-28`'s own rationale for the reset ("the bound would never accumulate at all", `pipeline_outcomes.rs:790-793` region) is preserved only for the *unattended* loop, which these gates cannot create.

---

## SUSPECTED *(speculation marked)*

### S1 — the reopen is a control-flow path the plans do not specify — Medium
`52-29-PLAN.md:39`: "reopens a never-silent gate with the kind and status the supervisor opened it with … before `Gates::claim`, the `gate_pending` write and `gate_resolved`".
`crates/devflow-core/src/gates.rs:539-551` (`open_gate_as`) **sets aside the stage's existing answers** and writes a gate file with a **new `GateId`**; `pipeline_gate.rs:544` binds `gate_id` once for the whole wait, and `registration.finish()` runs on every exit path. So a reopen is not a return — it is a re-entry into the wait with a fresh registration and a new id. Neither 52-29 nor 52-33 says the wait loops. *Speculation:* an implementer reading "reopens … before the claim" as "reopen, then return an error/keep waiting on the old id" produces either a supervisor error on every refusal or a wait on a gate nobody can answer — and both plans' tests only assert events, not that the wait continues.

### S2 — the second-denial gate has no waiter story past the bound — Low-Medium
`52-32:28` says the second-denial gate "stays open for the operator", but the wait is bounded by `rate_limit_wait_bound`, and on `GateWaitEnd::TimedOut` the current code emits `gate_timeout` and returns `Err` (`pipeline_gate.rs:729-737`), i.e. the supervisor exits. Multiple-day resets therefore become a dead waiter, recovered only by `devflow resume` — which 52-32 also says writes no timer answer. The plan's e2e row for this case ("stay 2 over a bounded wait") does not say what timeout it sets, so the test may be measuring the 3-day default. Unverified.

### S3 — `uncovered_paths` may leak into a new run on the same phase — Low
`52-33:34`: "cleared only when a subtraction empties it, when the D-41 gate is approved, and on abort". D-35 keeps the state after a finish and "a new `start` on the phase replaces the finished state" — if 52-25/52-31's `start` mutates the kept file instead of deleting it, a stale set survives into the next run and gates its first finish. Not verifiable from these four plans (52-25 owns `start`).

---

## SIMPLIFY

1. **Drop the `git merge-base --is-ancestor` probe and its exit-code taxonomy** (`52-33:26,29`, plus rows (b)/(c) and the "base itself unusable" decision at `52-33:367`). With the persisted set, the only remaining job is "list what this pass changed"; one `Err` ("the pass's diff could not be listed") covering an absent base, a non-ancestor base and a non-UTF-8 path is sufficient, and it deletes two probes, one `uncovered_paths_updated` reason case, and one test row. R2's original concern is already gone.
2. **Give the D-41 gate its own `GateKind` (`UncoveredFix`).** Today `stale_after_reverify` must mean `stale` (52-28), `passed` (D-41 accept), `missing`, `gaps_found` and `human_needed` (52-33) — five statuses, two owners, and the accept is keyed on a `(kind,status)` pair that `finish_run` cannot see (C4). A dedicated kind makes the accept a kind test, removes the five-row writer matrix in `each_verification_gate_records_the_status_that_chose_it`, and makes the routers exhaustive without a "same kind, opposite route" control.
3. **Collapse `uncovered_paths_updated`'s four reasons to two events per pass** (`added`+`covered` emitted as one `union`/`subtract` pair). The per-change emission is what forces the "stub replies answer in call order" bookkeeping across three plans (`52-33:36`, `:110`).
4. **Replace the flag-before-answer ordering + `responded_by` discrimination in 52-32** with: write the timer answer (already idempotent), then set `rate_limit_auto_relaunch_used` at the single claim, keyed on `responded_by == timer`. That deletes the "restore the flag to false" branch, one of the four recovery states, and the `AlreadyResponded` operator/timer discrimination — and it fixes C6's "flag spent by a crash that never relaunched" for free.
5. **Delete the `code_fix_base` clearing list** in `52-29:31` (`transition`, `--until` halt, stop, finish, abort, another fix route). 52-33's single union commit is the only consumer; enumerating six other clear sites is maintenance surface that 52-33 then contradicts (C3).

---

## COVERAGE

**Read in full:** `52-28-PLAN.md`, `52-29-PLAN.md`, `52-32-PLAN.md`, `52-33-PLAN.md`, `52-plan-review-2/SUMMARY.md`; `52-CONTEXT.md` lines 56-360 (D-06…D-43); `52-22-PLAN.md` lines 25-45, 140-171, 280-285; `52-27-PLAN.md` lines 30-35, 100-145, 155-180, 250-270; `52-08-PLAN.md` grep (finish_run/D-29/`--until`).

**Source read (exact ranges):** `pipeline_gate.rs` 60-140 (`transition`, `--until` halt, `code_fix` clears), 300-400 (3.x finish path), 500-740 (`run_gate_with_timeout`), 729-737 (TimedOut); `pipeline_launch.rs` 1318-1342, 3383-3412, 3512-3560, 3640-3700; `gates.rs` 88-107, 468-560, 680-708, 3840-3870; `pipeline_outcomes.rs` 590-680 (streak), 700-810 (ceiling), 1930-1990 (ambiguous arm); `commands.rs` 2513-2520, 2684-2692, 2720-2760, 4040-4075; `config_parse.rs` 42-60, 127-150.

**Skimmed only:** `52-REVIEWS.md`, `52-RESEARCH.md`, `52-PATTERNS.md`, `52-VALIDATION.md`, `52-GAP-LOOP-VERIFICATION.md`, `52-30/-31` beyond the cited lines (52-31's sweep/semver/`relaunch_note` truths read in full via grep output), `OPERATIONS.md`, `mode.rs` (`MAX_*` constants read via the plans and the ceiling call site, not the file).

**Not verified (state plainly):** whether `devflow stop` clears `code_fix` today (C3 rests on 52-29's *stated* clearing, not current source); whether `devflow gate sweep` is run automatically anywhere (C6 assumes manual, which is what the source shows); 52-25's `start` handling of a kept finished state (S3); real gsd-core `covered_files` behaviour (unchanged from the plans' own n=1 limit).
