## CONFIRMED

Plan citations below refer to `.planning/phases/52-subtract-and-stabilize/`. These are confirmed defects or contradictions in the specifications, not claims that the proposed implementation was executed.

**C1 — High — 52-27’s control requires routing that 52-29 explicitly forbids.**

- **Location/quote:** `52-27-PLAN.md:143`: a gate whose kind is `human_needed` “does not set the gap marker and does not launch the gaps Plan.” `52-29-PLAN.md:34`: different verification kinds with the same evidence “get the same route.”
- **Breaking sequence:** Approve a `human_needed` gate while the fresh status is `gaps_found`. D-45 requires the gap loop. The retained 52-27 control requires the opposite.
- **Evidence checked:** 52-27’s behavior and automated gate; 52-29’s replacement-router instructions and existing-test dispositions. The dispositions do not name this test.
- **Fix:** Rewrite the control in 52-29. Give it fresh `human_needed` evidence if its purpose is to reject misleading context; test kind independence separately. Otherwise correct D-45 code fails `scripts/check.sh all`.

**C2 — High — Rejecting a failed Code stage overwrites the original fix request.**

- **Location/quote:** `52-29-PLAN.md:197`: a stage-failure reject at Code relaunches “with the note in `code_fix_note`.” `52-31-PLAN.md:34`: “Both stay, because the fix request still applies.”
- **Breaking sequence:** Reject a verification gate with note **Q** → Quick Code pass stores Q in `code_fix_note` → Code fails → reject its stage-failure gate with note **R** → the prescribed carrier replaces Q with R. The original quick-fix task disappears.
- **Evidence checked:** 52-26’s durable fix-note contract; 52-29’s Code rejection row; 52-31’s two-note contract and action at lines 235–237. The latter wires `relaunch_note` for stage failures only at Plan and Validate.
- **Fix:** In 52-31, migrate Code stage-failure rejection to `relaunch_note` too. Add a Q→failure→R test asserting both notes reach the relaunched process. The existing renderer test starts with both fields populated and cannot detect this overwrite.

**C3 — Medium — Uncommitted edits can reuse an earlier Validate PASS.**

- **Location/quote:** `52-29-PLAN.md:369`: “The HEAD comparison sees commits, not uncommitted worktree changes.” Its predicate at line 30 checks fresh `passed` plus the recorded HEAD.
- **Breaking sequence:** Validate passes on H → supervise gate waits → edit an uncovered source file without committing → GSD still reports `passed`, as the accepted uncovered-file limitation permits → approve. HEAD remains H, so the old PASS qualifies and the route finishes without another Validate.
- **Evidence checked:** The prescribed evidence fields, HEAD comparison, approve route, committed-change e2e, and D-45 rule 4.
- **Limit:** This does **not** bypass the literal two-input predicate. It demonstrates that the predicate can authorize finishing a tree the recorded PASS never examined.
- **Fix:** State the clean-tree assumption explicitly and test this sequence. If that assumption is required, invalidate the usable PASS on dirty evidence within the existing evidence boundary. Do not reintroduce uncovered-file tracking.

**C4 — Medium — An obsolete fingerprint test is instructed to survive the machinery’s deletion.**

- **Location/quote:** `52-29-PLAN.md:277–280` names `a_ceiling_loop_back_persists_the_reset_and_the_verification_observation` and says to preserve “the persisted observation,” changing only fix/launch assertions. `52-24-PLAN.md:103–105` instead deletes routing bookkeeping, its writes, and its tests.
- **Breaking sequence:** Execute 52-24’s deletion → reach 52-29 → following its disposition requires preserving or restoring the deleted verification observation.
- **Evidence checked:** The actual test at `crates/devflow-cli/src/pipeline_outcomes.rs:5949`; it computes `phase_verification_fingerprint` at line 5971 and asserts `after.last_verification_fingerprint` at line 6004.
- **Fix:** Retain only the ceiling-reset assertion under a renamed test. Explicitly delete the fingerprint premise and assertion. An obsolete test is not a legitimate non-routing consumer justifying retained state.

**C5 — Medium — The impossible invocation count remains in another rate-limit test.**

- **Location/quote:** `52-32-PLAN.md:141` requires “one claude invocation.” Line 97 correctly warns that the same rig retains canary and Plan invocations.
- **Breaking sequence:** Normal v4 start → Claude delivery canary → Plan → Code denial. That is already at least three non-health invocations before any relaunch.
- **Evidence checked:** `crates/devflow-cli/tests/failover_e2e.rs:66–99` logs those paths; `Rig::invocations` at lines 389–395 removes only `health`. The plan’s C3 repair fixes the automatic-relaunch test, not this waiting test.
- **Fix:** Require exactly one `stage code` entry, corroborated by one Code `stage_launched` event. Correct the remaining behavior text so an executor does not implement an assertion correct code cannot pass.

**C6 — Medium — The gap-plan gate does not discriminate the required “newer than report” property.**

- **Location/quote:** `52-27-PLAN.md:32` requires a `gap_closure: true` plan “newer than the report.” Its test at line 145 exercises the case where “no plan reads `gap_closure: true`.”
- **Breaking sequence:** An old gap-closure plan exists → a newer report still has gaps → the new Plan pass writes no replacement → an implementation checking only `any(gap_closure == true)` wrongly advances.
- **Evidence checked:** The post-check truth, Task 2 behavior, action, automated gate and failure description. None explicitly requires the old-true-plan negative case.
- **Fix:** Add an old true plan predating the report and require the gate. Pair it with a genuinely newer true plan that permits Code. The specified absent-flag case cannot distinguish the defective existence-only implementation.

## SUSPECTED

**S1 — High — Speculation: the PASS can be stamped onto a commit it did not validate, or its permit can outlive HEAD.**

- **Location/quote:** `52-22-PLAN.md:166–168` reads HEAD “when the outcome is classified.” `52-29-PLAN.md:153–155` builds evidence from that sample and mandates “no second HEAD read.”
- **Candidate sequence:** Validate reports PASS on H0 → another writer commits H1 before outcome classification → the recorder stamps H1. Alternatively, evidence samples H0 successfully, then H1 lands before `finish_run` commits the finish.
- **Evidence checked:** The proposed sampling contract; existing `commit` at `pipeline_launch.rs:1256` serializes DevFlow state, not repository HEAD; the planned e2e changes HEAD before approve, not inside either decision window.
- **Unverified:** The future implementation’s exact ordering and any enforced repository-write exclusion.
- **Fix:** Specify the evidence linearization boundary. Add controlled commits between verdict/recording and evidence/finish. Bind the PASS to the commit actually evaluated; a boolean derived from a later sampled HEAD cannot establish that binding.

**S2 — Medium — Speculation: timer wake and gate expiry coincide for resets beyond the timeout.**

- **Location/quote:** `52-31-PLAN.md:29` sets lifetime to the later of timeout and reset-plus-margin. `52-32-PLAN.md:112` executes the timer “When the wake comes first.”
- **Candidate sequence:** Gate timeout is three days; reset-plus-margin is five days away. Both deadlines become day five. If the implementation handles lifetime expiry first, it returns timeout without the automatic relaunch.
- **Evidence checked:** Both timing contracts and 52-32’s tests. The future-reset test checks survival past the short timeout; the actual relaunch test covers a wake earlier than lifetime, not equality.
- **Unverified:** Tie-breaking in the unwritten wait implementation.
- **Fix:** Define timer priority at equality and test a reset beyond the configured timeout through the actual wait dispatcher.

## SIMPLIFY

- **Remove the second rate-limit answer interpreter.** `52-31-PLAN.md:159–163` adds a mapping from answers to `GateAction::Relaunch` beside the routers’ existing RateLimit arms. Delegate to one interpretation; otherwise the advertised single source has two implementations.
- **Correct the flag’s meaning instead of adding recovery.** `52-31-PLAN.md:32,167–169` calls it a completed/winning relaunch, while `52-32-PLAN.md:113` spends it before publishing an answer. Document it as *automatic budget consumed*. A crash with no launch is already accepted by D-45; no additional state is needed.
- **Delete the obsolete observation assertion identified in C4.** Preserve the counter property without preserving its former verification machinery.

## COVERAGE

- **Read completely:** D-45 including its clarification; CLAUDE.md; 52-VALIDATION.md; 52-REVIEWS.md’s final operator-direction section.
- **Read all operative truths, tasks and verification gates:** targets 52-03, 22, 27, 28, 29, 31, 32 and 33; also 52-30. Historical disposition ledgers were selectively read.
- **Targeted supporting reads:** plans 52-08, 09, 20, 24 and 26; source sections covering outcome classification, counters, transitions, gate answer/claim handling, ownership fencing, resume, supervisor publication, prompt construction, Git isolation and the cited tests.
- **Verification limits:** Static inspection and read-only cross-checks only. No proposed Rust tests or cargo checks ran. The wave-chain/name check found no target gate whose named test is first introduced later in that chain; this does not establish that the future tests compile.
- No file-writing commands, temporary artifacts or background processes were created.
