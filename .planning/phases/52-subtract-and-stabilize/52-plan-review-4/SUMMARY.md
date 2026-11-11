# Phase 52 plan review, round 4: the D-45 redesign (2026-11-10)

Snapshot: `feature/phase-52` at 9db3e27 (the reviewed snapshot is identical to HEAD). Lanes: codex (gpt-6.1-sol, high), agy (gemini-3.8-flash-high, high), pi (deepseek-flash); all three returned. The prompt is `prompt.txt`. A host verifier checked every finding against the plan text, in wave order 52-22 → 27 → 28 → 29 → 33 → 30 → 31 → 32, and against source at 9db3e27; the host re-checked U1, U2 and U3 against source and plan lines. Severities are the host's own; in v4 a finish ends the run and keeps the branch (D-35), so it merges nothing.

No lane found a `finished` path that bypasses `can_finish`. The `FinishPermit` type and the emission scan hold as written. The one false-finish row (U3) satisfies `can_finish` literally. It is about what "a PASS on HEAD" covers.

Totals: 10 confirmed, 6 refuted, 4 plausible. Confirmed by class: (a) 2, (b) 0, (c) 4, (d) 4, (e) 0.

## Confirmed

Root-cause classes: (a) a gap in the D-45 design; (b) cut machinery still referenced; (c) two plans disagree; (d) a verify gate or test disposition; (e) a pre-existing problem, unrelated to D-45.

| # | Sev | Lanes | Finding (evidence) | Class | Smallest fix |
|---|-----|-------|--------------------|-------|--------------|
| U1 | High | codex C2, agy C-05, pi C1 | When a Code stage-failure gate is rejected, its note has no working path to the prompt. 52-29:35 puts it in `code_fix_note`. 52-26:157 renders that field only when a `code_fix` pass is replayed, and `default_stage_prompt` (`pipeline_launch.rs` L140-145) builds the Code intent from `code_fix`, which is `None` on an ordinary Code retry. During a quick pass, the note also overwrites the pending fix note Q. 52-31:34 says the stage-failure note travels in `relaunch_note`, but 52-31:36 says Code is the exception. No test checks the Code prompt. | c | Send the Code stage-failure reject note through `relaunch_note`, as Plan and Validate already do. Delete the "at Code … `code_fix_note`" carve-outs at 52-29:35 and 52-31:36. Add a Code row and a Q→failure→R row to 52-31's `a_rejected_stage_failure_at_plan_or_validate_relaunches_with_the_note`. |
| U2 | High | agy C-02 | In auto mode, an agent PASS over GSD `gaps_found` is never counted, so the gap loop has no bound. Today the streak update runs only on `ValidateResult::Failed` (`pipeline_outcomes.rs` L581). 52-28:26/:102 add a count whatever the verdict, but only for `stale`/`missing`. 52-28:256 states "the streak update counts only an agent FAIL". 52-27:39 promises that "each gap round counts" (D-12), but neither 52-27's action (:185) nor any test has an agent-PASS gaps row. 52-29:137's counted row is "agent FAIL, `gaps_found`". | c | At 52-28:26 and :102, change "`stale` or `missing`" to "`gaps_found`, `stale` or `missing`". Add an agent-PASS-over-`gaps_found` row to `a_validate_that_takes_the_reverify_route_counts_one_failure`, and run 52-27's three-round test with agent PASS. |
| U3 | Medium | codex C3, agy C-03, agy C-01, pi S1 | An uncommitted tree reuses a PASS. `read_evidence` compares only HEAD (52-29:146), and 52-29:369 lists that as a limit. D-45 rule 4 accepts only the committed uncovered-file case ("a fresh Validate agent PASS on that commit"), so this limit is the planner's, not an operator acceptance. agy C-01 (finishing from the `no_gap_closure_plan` gate on an earlier Validate's PASS) is a form of this row, not a separate flaw: with a clean tree at a fixed HEAD, GSD's status cannot change from `gaps_found` to `passed`, because the frontmatter is tree content and clean files use commit times (`verification.cjs` L21-25, L952). D-45 does not require the PASS to come from the Validate being routed. It requires a PASS on the current HEAD (rule 1, 52-22:30). | a | In `read_evidence`, make `agent_pass_on_head` true only when the recorded head equals HEAD and `git status --porcelain` is empty, and name the dirty paths `[never-silent]` in the gate. This is one git call, keeps no state and tracks no files, so it fits D-45. Do not exclude `.planning/`: an uncommitted VERIFICATION.md edit is exactly how C-01's status flips. If `.planning/` must be excluded, also clear `validate_pass_head` when the run leaves Validate. Add a dirty row to `can_finish_needs_passed_and_a_pass_on_the_current_head`. Operator decision: close or accept. |
| U4 | Medium | pi C2 | A supervise Validate over `stale`/`missing` has no `GateKind`. D-45 rule 2 and 52-29:32 say supervise gates every row, and 52-28:26 refers to "its gate's approve". But 52-22:37's writer list (nine verification kinds) has no kind for it, 52-22:31 still routes `stale` straight to Code, and 52-22:236 makes the kind mandatory. No supervise-`stale` test exists. | c | Add one verification kind (for example `reverify_needed`) to 52-22's enum and writer test and to 52-29:34's list (ten kinds). Add one supervise-`stale` row (gate, then approve → re-verify). |
| U5 | Medium | codex C1 | 52-27:143's control in `approving_a_gaps_gate_runs_the_gap_loop` pins kind-keyed routing (a `human_needed` kind over a `gaps_found` context must not loop). 52-29 replaces that arm, but its disposition list (:263-280) does not name the test (`rg -n approving_a_gaps_gate_runs_the_gap_loop` matches only 52-27). Under fresh-read routing, either a row fails on correct code or an executor bends the routing to keep the test green. | d | Add the test to 52-29's dispositions. Give the main row a `gaps_found` stub and the control a `human_needed` stub (reopen), or delete the control because `the_approve_path_takes_the_route_approve_route_names` covers kind independence. |
| U6 | Medium | codex C6 | 52-27:32 requires a `gap_closure: true` plan "newer than the report". The only test (:145) is the case where no plan has the flag, so a check that only looks for any such plan passes. From round 2 on, the earlier round's gap plan exists, so a gaps Plan pass that wrote nothing would go on to Code. | d | Add a row with an old gap plan that predates the report (must gate) and a row with a newer one (must proceed). |
| U7 | Medium | pi C4 | Two plans state when `relaunch_note` is cleared ("when the pass it was given to ends", 52-31:236, 52-32:159), but neither names where it is cleared or tests it. Every note test checks presence, or that resume and takeover keep the note. A leak would put the note in every later prompt (52-31:221 renders it for all stages). | d | Name `transition` (beside the `code_fix` clear) as the clear site. Add one assertion: a transition out of the relaunched stage leaves `relaunch_note == None`. |
| U8 | Medium | codex S2 | When the reset is later than `DEVFLOW_GATE_TIMEOUT_SECS`, the timer wake and the gate lifetime fall at the same moment. 52-31:29 sets the lifetime to the later of the timeout and reset plus margin, so it equals `not_before`, and 52-32:146 asserts that "both return the `not_before`". 52-32:112 acts only "when the wake comes first". At equal deadlines the order is unspecified, so a weekly-window reset (the case D-27.2 exists for) can time out with no relaunch. | a | State that the wake is checked before the lifetime when the two are equal, or set the lifetime to `not_before` plus a short grace. Add one dispatcher row with a reset later than the timeout. |
| U9 | Low | codex C5 | 52-32:141 asks for "one claude invocation", but `Rig::invocations` (`failover_e2e.rs` L389-395) keeps the `canary` and `stage plan` lines, so no correct code can produce one. | d | Require exactly one `stage code` entry, plus one Code `stage_launched` event. |
| U10 | Low | pi C5 | The gate's way-on sentence is rendered from the evidence at open time (52-33:29), but 52-29 routes the approve on a fresh read at pickup. 52-29's own tracer (:136) commits during the wait, so the gate text says "approve finishes" while the run goes back to Validate. | c | For the verification family, render approve as the rule (it re-reads GSD's status and finishes only on `passed` with a Validate pass on this commit), not as a predicted row. |

Negative controls (the patterns searched to confirm each row's refuting text is absent):

| Row | Pattern | Searched in | Result |
|-----|---------|-------------|--------|
| U1 | `stage.failure…Code…relaunch_note` and `relaunch_note…at Code` | 52-22..33 | none |
| U2 | `gaps_found…(whatever the agent\|agent pass\|agent PASS)` filtered to count, streak or failure | 52-22..33 | only 52-29:137's agent-FAIL row |
| U3 | `porcelain\|dirty\|uncommitted\|clean tree\|worktree changes` | 52-22..33 | only the 52-29:369 limit and a proptest check at :331 |
| U4 | `stale` near `gate\|kind\|supervise` | 52-22, 28, 29, 33 | only the cut `stale_after_reverify` and `stale_gaps_found` |
| U5 | the test name | all PLANs | 52-27 only |
| U6 | `predat\|older\|newer` | 52-27, 28, 29 | only :32's requirement |
| U7 | `relaunch_note…(None\|cleared)` | 52-22..33 | only the lifetime prose |
| U8 | `equal deadlines\|tie\|same instant\|at equality\|coincid` | 52-31, 52-32 | none |

## Refuted

- agy C-01, as a separate path: the gaps-Plan gate cannot finish on a clean tree, because the status at an unchanged HEAD cannot become `passed` (see U3). Its fix is also wrong: taking `no_gap_closure_plan` out of the Verification family would route a gaps answer without GSD's status.
- agy C-04: 52-03:109 runs the whole `gate_wedge_e2e` binary on purpose, because every caller in that file must pass. With no filter, `--no-tests=fail` fails on zero tests and on any failing test, and :110 names that outcome. The CLAUDE.md trap is about a filter that matches nothing.
- agy C-06: the linter runs only on staged `*PLAN.md` files (`lint-plan-bashisms.sh` L80, pre-commit L107). Running it over every `.md` file is not a gate any hook or `check.sh` runs.
- agy S-02: a crash after the flag commit leaving no automatic relaunch is D-45 rule 5's stated, accepted limit. The proposed fix (commit the flag after the claim) reverses rule 5's ordering.
- codex C4: 52-29:265 says to "`rg -n` each name first, since an earlier plan may already have changed or removed it, and record in the SUMMARY". If 52-24 deletes `last_verification_fingerprint`, the test must change in 52-24 to compile, so 52-29 cannot be made to restore it. (Low wording nit: drop "the persisted observation" from :277-279.)
- pi C7: the scan as specified passes on the tree as it stands. The only non-test literal hits are `pipeline_gate.rs` L91 and L383, `events.rs` L424 sits inside `mod tests` (L200), and `ship_evidence.rs` goes with 52-13. pi's fix (split at the first `#[cfg(test)]`) would blind the scan: `pipeline_launch.rs`'s first one is at L93, ahead of about 4,100 lines of non-test code.

## Plausible

- codex S1: a commit by another writer between the Validate agent's exit and the HEAD read stamps the PASS on a commit the agent did not test. The window is narrow and needs an operator commit at that moment. Worth one line in 52-29's Limits.
- pi C3: 52-27 does not say how the `no_gap_closure_plan` gate is answered in the interim before 52-29. The plan text does not support pi's claim that it inherits Validate's finish arms (the gate opens from Plan outcome handling, not `handle_validate_outcome`). The window is never released, and 52-29's every-kind approve test covers the end state. It would cost one line in 52-27.
- pi C6: 52-29:225 removes the preflight and checkpoint `LoopBack` arms without naming what replaces them in an exhaustive match. The screen refuses those answers before the claim, so an `unreachable!()` would never fire through the screen. Whether a legacy answer can reach it depends on code not yet written.
- pi S2: the D-45 table has no "auto, gate approve" column. 52-29 applies the supervise column to an auto failure-cap approve, which can be defended by "an approve is an approve". Add one OPERATIONS.md line.

## Simplification proposals

- One note field: render `relaunch_note` for every stage relaunch, and keep `code_fix_note` only for the fix request (pi; overlaps with U1's fix). Pi's stronger version (a single field) drops 52-31's P4 dual-introduction test.
- One rate-limit answer interpreter: 52-31:160-163's mapping function duplicates the routers' `RateLimit` arms (:35). Delegate one to the other (codex).
- Document `rate_limit_auto_relaunch_used` as "the automatic budget is spent", not "a relaunch happened" (codex; 52-31:32 against 52-32:112).
- Drop `GateFile.gsd_status`, or keep it only in `context`: its one reader re-renders a refused answer's reopened label (pi S3).
- Collapse `Step`, `StatusLessStep`, `ApproveRoute` and `RejectRoute` into one enum (pi).
- 52-33's forbidden-word scan: assert that the specific removed strings are absent instead of a word list (pi).
- Drop the `until_noop` event and keep the printed line (pi).
- A verification approve's CLI print could skip `read_evidence` and print the rule (agy S-01; pairs with U10).

## What this check does not establish

- Static reading only. No cargo, test or proptest run happened.
- Plans were read in targeted windows (truths, tasks, gates, ledgers), not every line. 52-30 was barely read.
- The clean-tree argument in U3 rests on `verification.cjs` comments (L21-25, L952), not on running GSD's reader.
- Each negative-control search only shows the refuting text is absent under the patterns used.
