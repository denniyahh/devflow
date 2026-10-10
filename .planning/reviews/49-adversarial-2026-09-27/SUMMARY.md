# Phase 49 External Review — Round 3 (2026-09-27)

**Skill:** `external-review` (`~/.agents/skills/external-review`), `--lens adversarial`, on a `git archive` snapshot.
**Reviewed commit:** `1adf4b9` — primary target the round-3 revision (resume-forward) of plans 49-10…49-15, 49-18 and
`49-RESEARCH.md` (`ROUND3.patch` against round-2 tree `9b2718b`).
**Prompt:** `prompt.md` (the skill's adversarial lens template verbatim, plus targets, GSD mechanics and limits).

## Lanes

| Lane | Model | Result | Duration | Citations |
|------|-------|--------|----------|-----------|
| codex | `gpt-6-sol`, reasoning high (probed live; a bogus model id failed, so no silent fallback) | completed — `codex.md` | 7 min, 208k tokens | 18 |
| antigravity | `gemini-3.8-flash-high`, effort high | completed — `antigravity.md` (absolute `file://` prefixes shortened) | 11 min | 65 |

The snapshot's content fingerprint was identical before and after both runs. Every finding below was verified against
the source by the orchestrator before being listed.

## Confirmed findings (deduplicated)

| # | Severity | Finding | Lanes | Verified at |
|---|----------|---------|-------|-------------|
| 1 | Critical | Tasks read "this plan's SUMMARY" before GSD creates it; 49-18 Task 2's gate reads `49-18-SUMMARY.md` mid-plan. GSD writes SUMMARY only in `create_summary` after all tasks, and counts any matching SUMMARY file as completion — so an early SUMMARY would make resume **skip** a stopped plan, breaking resume-forward. Affects the success path too. | codex 1, agy 1 | 49-10:268, 49-11:136, 49-12:163, 49-18:193; `execute-plan.md:225,406-412` |
| 2 | High | Code-stop retry: the operator approves Define for N+1 before any of its evidence (binary, arm pair, Define context) is checked; on attempt 1 those checks precede the approval. | codex 2, agy 5 | 49-10:142-147 vs 49-11 Task 1 catch-up |
| 3 | High | Snapshot-based gates take the lexicographically first `define-*`/`code-*` snapshot, not the one the ledger's `gate_open` line names by path and sha256; codex reproduced a bad snapshot passing. Success path. | codex 3, agy 7 | 49-10:255, 49-11:123 vs 49-03:125-132 |
| 4 | High | 49-10 Task 1 treats an attempt as live when `attempt.json` exists and the archive ref is absent — but `archive-attempt.sh` writes no ref when `feature/phase-49.1` never existed (`ABSENT`), so an archived early failure is misread as a same-N re-presentation. | agy 3 | 49-10:194; 49-08:224 |
| 5 | High | 49-13 Task 2 re-runs `undo-ship-hooks.sh` on a same-N re-presentation; after a successful undo its layout guards refuse, and the plan says to fail rather than improvise — the plan cannot complete. | agy 4 | 49-13:140-152; 49-05 layout guards |
| 6 | Medium | GSD presents checkpoint tasks unconditionally; on a same-N re-presentation 49-11 T2 / 49-12 T2 / 49-13 T1 ask for an approval whose gate is already closed, and `gate.sh approve` then fails with `NoOpenGate`. The checkpoints carry no "already approved" reply. | agy 2 | `gates.rs:216-218`; 49-11:133-148 |
| 7 | Medium | The stop record's `stage` must equal the last ledger `gate_open` stage, which cannot represent a Validate-stage stop (no gate in auto mode) or a stop before any gate opened. | agy 6 | 49-10:127; 49-15:190 |
| 8 | Medium | 49-15 validates only the stop record's attempt and stage; `plan: invented-plan`, `cause: banana`, `disposition: success` passed (codex executed it). | codex 4 | 49-15:190 |
| 9 | Medium | 49-15 accepts C1/C2 `NOT_MET` even when their check files pass, so a false failure drives VERIFY-01 traceability (codex executed it, with a working reverse control). Success path. | codex 5, agy 8 | 49-15:190,210-213,225 |
| 10 | Low | The catch-up parser's `^[ >]*current_attempt=` rejects a Markdown list bullet (`- current_attempt=1`). | agy 9 | 49-11:123 (measured: 2 of 3 forms match) |
| 11 | Low (speculative) | `record-secure` re-run on a same-N re-presentation may append a duplicate record, doubling SHAs handed to `revert`. | agy 10 | 49-12:165; 49-06 |

## Pattern

Findings 3-7 and 11 follow from round 3's same-N re-presentation rule and snapshot reads: making a live plan
re-runnable from Task 1 needs every task — including checkpoint and irreversible ones — to be idempotent against its
own evidence, and that property is not yet complete. Findings 1, 3 and 9 reach the attempt-1 success path.

## Not established

No lane ran DevFlow, GSD execution, systemd, `gh`, containers or network calls; the tools the plans reference do not
exist until waves 1-3 build them. The findings are plan/gate logic, checked against gsd-core and DevFlow source.
