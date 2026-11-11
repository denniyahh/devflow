# Phase 52 plan review, round 3 — plans 52-28, 52-29, 52-32, 52-33 after round 5 (2026-11-09)

Snapshot: the `feature/phase-52` working tree after round 5 (uncommitted on top of a44b8d1). All three lanes returned:
codex (gpt-6.1-sol, high), agy (gemini-3.8-flash-high, high), pi (deepseek-flash). The round-5 plan check ran alongside
it (1 blocker, 3 warnings). The host checked each row below against plan text and source.

## Confirmed

| # | Sev | Lanes | Finding |
|---|-----|-------|---------|
| T1 | Critical | codex F1, pi C1 | The D-43 re-check never leaves `reader_failure` or `stale_gaps_found`. The classifier keeps the opened kind, and `answer_basis` ignores the status for those kinds, so a fresh `passed` (or a matching-digest `gaps_found`) reopens the same gate forever. 52-29:34 vs :32/:187; 52-29:252 asserts the unreachable route. |
| T2 | High | checker blocker, codex F4, agy C1 | A fresh `passed` finishes on gates opened before Validate ran on the current code (52-28 Code post-check `human_needed`/`reader_failure`, 52-33 quick-pass-end `reader_failure`, 52-27 gaps-Plan `reader_failure`, a reject whose HEAD read failed). Resolved in CONTEXT by the D-43 clarification. |
| T3 | High | codex F2, agy C2, pi C2, checker W1 | `failure_cap` over `passed` and `ambiguous_verdict` over `passed` finish with no agent pass. Failure cap resolved by D-44. The ambiguous verdict falls under the same D-42 rule (no agent pass). |
| T4 | High | codex F3, agy C7, pi C3 | The plans say a stop clears `code_fix` (and so the per-pass base). Current `persist_stopped_state` does not touch `code_fix` (verified), and `a_resume_keeps_the_fix_pass` pins that. Following the plans would regress it and lose a quick pass's files on stop/resume before they are listed. |
| T5 | Medium | codex F5 | A crash between the union commit (which clears the base) and leaving Quick leaves Quick with no base; the replay then fails closed with no recovery. Make the union idempotent and keep the base until Quick is left. |
| T6 | High | pi C4, agy S1 | `finish_run` cannot tell it is answering the D-41 gate (`GateAction` carries no gate identity, and `gates.rs` is not in 52-33's files), so the D-41 accept is either a no-op or a reopen loop. |
| T7 | High | codex F6, agy C3, pi C6, codex S1 | 52-32 timer: a crash between claim and launch loses the relaunch; `devflow resume` keeps the flag spent even when the operator answered; `gate sweep` can reap the rate-limit gate after `not_before` (an abort); a "later of timeout and reset" wait bound can fire days late. |
| T8 | Medium | pi C5 | The finish guard trusts `covered_files` in VERIFICATION.md, which the agent under verification can edit; no threat row names it. |
| T9 | Medium | checker W2, agy C4 | 52-28 (L25, L28, L156-158) and 52-29 (T-52-54) still say approve is refused where D-43 makes it a re-check. |
| T10 | Medium | agy C5, agy C6, pi C7 | Ceiling reset on a reject of a counted gate is stated only for `failure_cap`; 52-28's test names a non-existent event (`reset_phase_failures_at_ceiling` emits `phase_failure_budget_reset`); Plan/Code-pass gates are reported to the operator as "Validation has failed N time(s)". |
| T11 | Low | codex F7, checker W3 | Row (a) (absent base) is told to assert the base is "still set"; key the update re-run on the gate's recorded cause, not on base presence. |

## Suspected

- pi S1: a reopen means a fresh gate id and re-entering the wait; the plans do not say the wait loops.
- pi S3: `uncovered_paths` could leak into a new `start` on the same phase if start keeps the old state file.
- pi S2: the second-denial rate-limit gate has no waiter after the wait bound.
- checker: the agent verdict a disagreement rewrite needs lives only in supervisor memory; a takeover would lose it.

## Simplification proposals

- Route the fresh read independently of the opened kind; keep the kind as provenance only (codex, pi).
- A dedicated `GateKind` for the D-41 gate (pi), so the accept is a kind test.
- Drop the ancestry probe; one "pass's diff could not be listed" error (pi).
- 52-32: write the timer answer first and set `rate_limit_auto_relaunch_used` at the single claim, keyed on `responded_by == timer` (pi); or extend the durable launch reservation (codex).
