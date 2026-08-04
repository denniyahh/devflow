# 31-05 — Live acceptance run: the two-plan wave that gates the 999.64 arc

**Status:** complete — acceptance **PASSED on attempt 1**, adjudicated by the operator 2026-08-03.
**Tasks:** 3/3. Tasks 1–2 executed by the plan's executor; task 3 was the blocking
`checkpoint:human-verify`, which halted and reported rather than self-adjudicating (D-19).

---

## What this plan was for

Per ROADMAP review constraint H4, the Phase 31 goal is **not substitutable by integration tests**: a
mocked CLI validates plumbing, not the delivery premise. This plan was the live run — a purpose-built
two-plan wave (D-16) on the main checkout (D-17), judged by D-18's criterion, with D-19 making a
failure mean the phase does not close whatever the unit tests say.

## Result

**PASS on all three criteria, first attempt.** Full evidence in `31-ACCEPTANCE.md`.

**D-18, per plan** — both halves, both plans:

| Plan | SUMMARY.md | Merged | Agent branch | Merge commit |
|---|---|---|---|---|
| 97-01 | yes | yes | `worktree-agent-a88358fe70a3a44e7` | `1302e93` |
| 97-02 | yes | yes | `worktree-agent-a572f3ed8b7f202e3` | `5caaa4c` |

**Independently verified by the orchestrator, not taken on report.** Both merge commits have two
parents; `1302e93` merges 97-01's tip into the pre-run commit `626131b`, `5caaa4c` merges 97-02's
tip, and both second parents have `626131b` as their merge-base. Two branches genuinely forked from
the same base and merged back — a shape inline work cannot produce, which is exactly why the
adversarial review replaced the original marker-grep criterion with this one.

**The VOID guard cleared.** Four top-level `background_tasks_changed` events: `[97-01]` →
`[97-01, 97-02]` → `[97-02]` → `[]`. The orchestrator's turn ended at capture line 476 with **both**
executors outstanding — the 999.64 orphan point — and the session survived it and received both
completions.

**Canary:** `claude_delivery_canary_confirmed`, CLI 2.1.220, ~19s. First run against the real CLI.
**Timings:** Code stage 1329s (22m09s). No idle timeout fired.

## What this run does NOT establish

Ten items are enumerated in `31-ACCEPTANCE.md`; the load-bearing ones:

- **One passing run is a weak reliability bound.** It shows the mechanism works, not that it works
  reliably. A third-try pass would be different evidence; this was first-try, and every attempt is
  recorded so a reader can tell.
- **The raw capture was deleted by cleanup and never committed**, so the `background_tasks_changed`
  evidence survives only as transcription. Partial corroboration: the recorded task IDs
  (`a88358fe70a3a44e7`, `a572f3ed8b7f202e3`) match exactly the agent branch names git independently
  proves were merged — two different artifacts agreeing.
- **The 120s idle floor was not exercised.** Only one `tool_progress` keepalive appeared in 932
  lines, so gaps stayed far below it.
- **The drain arm was not isolated as load-bearing.** At both early `result` events the
  `DEVFLOW_RESULT` marker was also absent, so the marker arm alone would have prevented an early
  close. The drain remains defensive, consistent with 30-04.
- **The Code orchestrator read `31-05-PLAN.md` and knew it was being measured.** The mechanism
  evidence stands — CLI delivery behaviour is not something an agent can perform — but its choices
  (declining `handle_branching`, dispatching sequentially) should not be read as representative of
  an unobserved run.
- **This run predates three commits.** See the provenance section of `31-ACCEPTANCE.md`: two later
  fixes are on paths the run never entered, and the third (the hot-path reader rewrite) was
  re-verified separately against the real CLI.

## D-17 compliance

**Not violated.** No `git` command of any kind — not even `git status` — between launch (20:32:24Z)
and the executor's report (20:54:52Z). The `stop_until: "code"` cap made that one unambiguous
interval rather than three sub-second-separated ones, and fired as designed
(`stop_reason: "stopped after code completed (--until code)"`): no Validate, no Ship, no push.

That cap only worked because a stale note claiming `devflow resume` clears `stop_until`
unconditionally was corrected during this phase — the clear is gated on `state.stopped` and pinned
by `resume_preserves_unfired_until_cap`.

## Deviations and residue

**Containment partly failed.** The executor pre-created `feature/phase-97` expecting
`handle_branching` to reuse it, but the Code orchestrator declined branching and stayed on
`feature/phase-31`. Eight acceptance-run commits (`0590537`…`911bf50`), including two worktree
merges and a fabricated phase 97, are therefore permanent in this branch's history. They were
deliberately **not** rewound: they *are* the D-18 merge evidence, re-derivable from git rather than
only from transcription. **They need a disposition before this branch merges to `develop`.**

Cleanup otherwise complete: scratch phase 97 removed, `.devflow/state-97.json` and all `phase-97-*`
captures gone, `feature/phase-97` deleted, worktrees and agent branches gone, `develop` untouched at
`956f3c2`, STATE.md restored to its baseline hash.

**Three tool-honesty defects reproduced**, all known classes: `roadmap.update-plan-progress 97` and
`phase.complete 97` both reported `updated: true` while ROADMAP.md stayed byte-identical (verified by
hash and a zero-line diff — this also answers a reviewer's open question: that verb silently no-ops
on a phase absent from ROADMAP rather than erroring), and `state.begin-phase` again deleted the
hand-maintained STALE-AND-UNVERIFIED block in STATE.md.

## Adjudication

Task 3's checkpoint halted and reported, as designed. The operator adjudicated **closes** on
2026-08-03 after the evidence above, an adversarial review round on plans 31-04/31-05, and a peer
code review round whose CRITICAL findings were fixed (`522e905`) or filed (999.75 / DEN-96).

The 999.64 arc closes with this phase.
