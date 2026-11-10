# Phase 52 plan review, round 2 — plans 52-28, 52-29, 52-32, 52-33 (2026-11-09)

Snapshot: the `feature/phase-52` working tree after the round 4 guided revision (uncommitted on top of a44b8d1).
Lanes: agy (gemini-3.8-flash-high, high) and pi (deepseek-flash) returned. codex DROPPED (usage limit, 0 bytes); not
counted as agreement. The host checked each finding below against the plan text and source; "verified" means the
quoted plan text and the code it relies on were read.

## Confirmed

| # | Sev | Lanes | Finding | Evidence |
|---|-----|-------|---------|----------|
| R1 | Critical | agy F-01, pi C1 | Approving the gap loop clears `code_fix_base`, so a quick fix's uncovered file can reach `finished` without any report covering it or any D-41 approve accepting it. | 52-33:186 "on the transition to the gap loop"; 52-33:164 digest-match control clears the base; 52-33:248 the pickup re-read does not recompute the remainder |
| R2 | High | pi C2 + plan checker | A quick reject that finds the base unusable replaces it with HEAD silently, dropping earlier uncovered files. | 52-33:183-184; the "never silent" clause covers only the check-cannot-run branch |
| R3 | High | pi C3 | 52-29 routes `passed` → finish "for every verification kind", which includes `disagreement_over_passed` (agent said FAIL). 52-22:30 says that gate is "never a finish", and D-08 says a pass needs the agent's pass AND GSD's `passed`. | 52-29 truth; 52-22:30; CONTEXT D-08 (L68) |
| R4 | High | agy F-02, F-03, F-08, S-01; pi C4, C5 | Gates whose approve is refused (`human_needed`, `stale_gaps_found`, `reader_failure`) have no answer that re-checks after the operator fixes the cause outside the run (UAT done, report refreshed, tool reinstalled). Reject runs `/gsd-quick`, a code fix for a non-code cause; only abort ends it. `reader_failure` and `human_needed` count no failure, so the loop never reaches the ceiling (operator-paced). | 52-29:33, 188, 191; 52-33:31; 52-28:25; CONTEXT D-24, D-37 (~L281) |
| R5 | Medium | agy F-04, pi C6, plan checker | 52-29 says `code_fix_base` is "cleared with `code_fix`"; `transition()` clears `code_fix` on every stage change (`pipeline_gate.rs` L86, L113-116), which would drop the base at Code→Validate, before 52-33's post-check. 52-33 says nothing else clears it. | 52-29:54/140; 52-33:29, 186-187 |
| R6 | Medium | agy F-05 | 52-27 says the gap marker is cleared when "52-28's re-verify" replaces it "in the same commit", but 52-28 never states that clear. A stop during a re-verify that ran inside a gap pass resumes as `--gaps-only`. | 52-27:29; 52-28 has no gap-marker text |
| R7 | Medium | agy F-06 | 52-32's crash tests expect a `devflow resume` to publish the timer answer, but `resume` closes every gate of the phase first (`close_gates_for_resume` → `Gates::close_phase`, `pipeline_launch.rs` L3512-3535) and relaunches directly. | 52-32:147-148 |
| R8 | Medium | pi C7 | The non-UTF-8 test row adds the path only to the index; `git diff <base>..HEAD` compares commits, so the row cannot reach the error it claims to test. Needs a commit. | 52-33:162 |
| R9 | Medium | pi C8 | 52-29's test asserts a stage-failure reject at Plan/Validate relaunches "with the note", but the carrier (`State.relaunch_note`) arrives in 52-31, and 52-29's own limits section admits there is none before then. | 52-29:34, 166, 315 |
| R10 | Medium | pi C9 | The answer screen's position relative to `Gates::claim`, the `gate_pending` write and `gate_resolved` (`pipeline_gate.rs` ~L641-695) is unspecified; placed after, a refused answer still claims, clears pending and emits `gate_resolved`. | 52-29:39, 205 |

## Plausible (not reproduced)

- agy F-07: counted post-check gates can push `phase_validate_failures` past the ceiling without the failure-cap gate's reset (`reset_phase_failures_at_ceiling` runs only for `ceiling_gate`, `pipeline_outcomes.rs` L743-802).
- pi S1: an approve into the gap loop runs no D-11 digest check at pickup (52-33:248 re-reads only for a finish route).
- pi S2: routing fields are read from a gate file under `.devflow/gates/` that a Code/Validate agent could write; the sandbox boundary is unverified.
- pi S3: `--until validate` now halts nothing; nothing tells the operator.

## Simplification proposals

- pi #1: replace `code_fix_base` + ancestry checks with a persisted set of uncovered paths (union at each quick pass's end, minus each report's `covered_files`, cleared on empty / D-41 approve / abort). Removes R1, R2 and the git exit-code handling.
- agy #2 / pi #3: derive the rate-limit "auto relaunch used" from durable gate/event records instead of `State.rate_limit_timer_gate` (caveat: answers are reaped, so the event log is the record).
- pi #4: route the repeated `query verification status` reads through one helper so the test stubs key by call site.
