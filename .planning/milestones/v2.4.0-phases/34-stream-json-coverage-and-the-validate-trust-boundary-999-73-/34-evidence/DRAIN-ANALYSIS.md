# Drain analysis — per-stage `BackgroundTaskState` (ROADMAP criterion 2)

One section per attempted stage: the observed `BackgroundTaskState`, its arity, whether it drained
before the marker, and **what a recurrence of that shape costs on the next run** — the last clause
being criterion 2's operative requirement.

All five sections describe **one run** (`devflow start --phase 1 --no-worktree --agent claude
--mode auto`, `claude` 2.1.222, scratch repo, binary `c81d8269…`). n=1 per stage.

## What criterion 2's framing sentence can and cannot ask here

Criterion 2 is framed around what happens when the close rule does **not** fire. For a
`NeverAnnounced` stage that question is **unanswerable by construction**: `should_close()` accepts
`NeverAnnounced` as vacuously drained, so the rule always fires and no capture can show the
non-firing case. What is being graded below is therefore the operative clause — the observed state
and the cost of its recurrence — not the framing sentence.

## The finding that matters: a refutation, not a confirmation

Framed as NC-8 requires. The drain gate keys on exactly one event shape
(`crates/devflow-core/src/monitor.rs:567-582`):

```
type: "system", subtype: "background_tasks_changed", tasks: [ … ]
```

Across the whole run — **1063 top-level events over five stages** — that subtype appeared
**zero times**.

It is not that the run had no concurrency. The run dispatched **8 sub-agents** (`Agent` tool: 3 in
Code, 5 in Ship), each announced as:

```json
{"type":"system","subtype":"task_started","task_type":"local_agent", …}
```

`"task_type":"local_agent"` is **the exact value the drain gate's own synthetic fixture uses** when
it manufactures a `background_tasks_changed` announcement
(`monitor.rs:1164`: `{"task_id":"t0","task_type":"local_agent","description":"child 0"}`). So the
fixture models a `local_agent` task as arriving inside a `background_tasks_changed` `tasks` array,
and production — on CLI 2.1.222, for the sub-agent dispatch path — announced it through
`task_started` / `task_progress` / `task_notification` instead.

**This refutes, for the sub-agent path, the assumption that the drain gate observes concurrent
child work.** It does not confirm it. That is the whole reason the capture campaign was worth
running: D-09 recorded that every gate fixture is labelled SYNTHETIC in-source and the parser's
production correctness was *reasoned, not witnessed*. It has now been witnessed, and the reasoning
did not survive contact for this path.

### What this specifically does NOT establish

Stated because the gap is load-bearing and easy to skate over:

- **The backgrounded-shell path was never exercised.** Every `Bash` tool call in the run carried
  `"run_in_background": false` — 8 occurrences, **zero** `true`. Backgrounded shells are a
  different mechanism from sub-agent dispatch, and this run says nothing about whether they emit
  `background_tasks_changed`. The gate may well work exactly as designed for them.
- **One CLI version, one workload, one run.** `background_tasks_changed` may be emitted by CLI
  versions other than 2.1.222, or under prompts that background differently.
- **It does not show an orphan actually occurred.** No child work was lost in this run. The claim
  is about what the gate *observed*, not about damage.

### Cost of recurrence

If this shape recurs, `CloseRule`'s drain arm is satisfied vacuously on every stage while sub-agents
are live, so stdin closes on the marker alone. That is the 999.64 orphan shape's precondition —
the guard built to prevent it does not see the concurrency it was built to see. Filed as a numbered
backlog entry (D-04), **not fixed here**.

---

## `Stage::Define`

- **Observed state:** `NeverAnnounced` (0 `background_tasks_changed` in 8 events).
- **Arity:** n/a.
- **Drained before the marker?** Vacuously — the drain arm was never constrained.
- **Cost of recurrence:** None. A stage that genuinely backgrounds nothing must be able to close on
  its marker or it hangs for the full idle timeout. This is the designed-for case.
- **Evidentiary weight — low.** 1 turn, 2.3 s, zero tool calls. The scaffold pre-writes the plan, so
  `/gsd-discuss-phase` had nothing to do. This says Define took the stream path and announced
  nothing *on a workload with no work in it*. It does not characterise Define under a real phase.

## `Stage::Plan`

- **Observed state:** `NeverAnnounced` (0 in 11 events).
- **Arity:** n/a. **Drained before the marker?** Vacuously.
- **Cost of recurrence:** None, same as Define.
- **Evidentiary weight — low**, same cause: 2 turns, 11.8 s, agent reported "The deliverable already
  exists … No work performed".

## `Stage::Code`

- **Observed state:** `NeverAnnounced` (0 in 455 events) — **despite 3 concurrent `Agent`
  dispatches.** This is the stage the refutation above rests on.
- **Arity:** n/a — no announcement to carry one.
- **Drained before the marker?** Vacuously, while 3 sub-agents were live.
- **Cost of recurrence:** the serious one. Code is where 999.64 was observed (Phase 29 wave 2
  dispatched two executors and orphaned both). A recurrence means the drain gate contributes
  nothing at the one stage it was built for, and the close decision rests on the marker alone.
- **Context difference from Phase 31 — recorded, not glossed.** Phase 31's raw capture was deleted
  during cleanup and never committed; that stage survives only as **transcription**. This capture is
  a **fresh capture against a scaffolded single-file probe phase**. The two differ in workload shape,
  tool-use volume and backgrounding pressure — precisely the variables this drain question turns on.
  **Phase 31's transcription remains the only production-phase evidence for Code, and this capture
  does not supersede it.**

## `Stage::Validate`

- **Observed state:** `NeverAnnounced` (0 in 126 events).
- **Arity:** n/a. **Drained before the marker?** Vacuously.
- **Cost of recurrence:** None on the drain axis.
- **Separate observation, filed not diagnosed:** the agent self-reported `PHASE 1 IS
  NYQUIST-COMPLIANT` and DevFlow still classified the stage as a `loop_back` to Code — twice, before
  the second pass advanced. That is the validate trust boundary this phase exists to tighten, and
  the classification may well be the *correct* new behaviour rather than a defect. Recorded as an
  observation so a later reader has the capture; deliberately **not** filed as a defect, because
  calling correct behaviour a bug is its own failure.

## `Stage::Ship`

- **Observed state:** `NeverAnnounced` (0 in 463 events), with 5 further `Agent` dispatches.
- **Arity:** n/a. **Drained before the marker?** Vacuously.
- **Cost of recurrence:** same class as Code — 5 concurrent sub-agents, none visible to the gate.
- **Scope limit:** the stage launched, ran 31 turns and emitted a top-level `result` marker, but its
  *work* stopped at preflight because the scratch repo has no git remote. This is evidence about the
  launch path — which is all membership in `STREAM_JSON_STAGES` selects — and **not** evidence that
  a real Ship completes.
