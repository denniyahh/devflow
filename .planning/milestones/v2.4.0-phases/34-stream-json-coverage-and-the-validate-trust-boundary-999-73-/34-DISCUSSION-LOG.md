# Phase 34: Stream-JSON Coverage and the Validate Trust Boundary (999.73 + 999.74) - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-05
**Phase:** 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-999-74
**Areas discussed:** Rollout granularity, 999.74 re-routing, Drain-gate depth

**Areas offered but not selected:** Capture acquisition — where the four real per-stage captures
come from and what the per-stage pass bar is. Recorded in CONTEXT.md under Claude's Discretion,
constrained by D-02 and D-10.

---

## Rollout granularity

### Q1 — End-state shape of the stage gate

| Option | Description | Selected |
|--------|-------------|----------|
| Keep the explicit list | `STREAM_JSON_STAGES` stays a named const naming all five stages; greppable, evidence story in the doc comment, one-line narrowing | ✓ |
| Invert to a deny-list | `LEGACY_STAGES` (empty), stream-by-default; cost is a new `Stage` variant silently joining with no evidence | |
| Delete the constant | Predicate collapses to `agent == Claude && !legacy_opt_out`; loses per-stage narrowing entirely | |

**User's choice:** Keep the explicit list.

### Q2 — A stage whose capture cannot be obtained in-phase

| Option | Description | Selected |
|--------|-------------|----------|
| Leave that stage narrow | Widen only evidenced stages; reason recorded in the doc comment; phase closes PARTIAL on criterion 1, remainder returns as a numbered backlog entry | ✓ |
| Phase does not close | All four captures or nothing, mirroring 31/D-19; one hard-to-stage capture blocks indefinitely | |
| Widen anyway, flag the gap | Full rollout now; but this is exactly "extend the adapter to four stages on zero evidence" | |

**User's choice:** Leave that stage narrow.

### Q3 — A runtime per-stage dial

| Option | Description | Selected |
|--------|-------------|----------|
| No new dial | 31/D-11's `legacy_opt_out` stays the one predicate; a second per-stage notion would drift, per `claude_stream_launch_enabled`'s own doc comment | ✓ |
| Env-var stage subtraction | `DEVFLOW_LEGACY_STAGES=ship` for surgical recovery; cost is a second stage-shape notion to keep in sync | |
| You decide | Leave to the planner | |

**User's choice:** No new dial.
**Notes:** Accepted cost recorded explicitly in CONTEXT.md D-03 — one bad stage forces the whole
run onto the legacy path.

### Q4 — A capture that reveals a parser or monitor defect

| Option | Description | Selected |
|--------|-------------|----------|
| Leave narrow, file it | Numbered 999.x entry plus Linear issue, capture committed as evidence; phase stays a rollout, not a parser-repair phase | ✓ |
| Fix it in-phase | Real reproducer in hand while context is loaded; cost is unbounded scope | |
| Depends on severity | Split by whether existing `ParsedCapture` / `is_top_level` machinery should have handled it | |

**User's choice:** Leave narrow, file it.

---

## 999.74 re-routing

**Context surfaced before the questions:** a one-pass read of the Validate routing suggesting the
inversion reaches `Stage::Ship` with no gate at all in `auto` mode, and misdescribes a failed run
as *"Validation passed — approve to ship?"* when the mode does gate. Presented explicitly as
preliminary, not as the audit criterion 4 requires. Recorded as F-01 in CONTEXT.md.

### Q1 — Where a status/verdict disagreement routes

| Option | Description | Selected |
|--------|-------------|----------|
| Ambiguous — immediate gate | Two independent signals disagreeing is the case `Ambiguous` exists for; D-18e's reasoning applies verbatim; never touches `consecutive_failures` | ✓ |
| Failed — ordinary auto-loop | Smallest behavioural delta, keeps unattended runs moving; but the delayed gate is the shape D-18e rejected | |
| Split by status | ResourceKilled/IdleTimeout → Failed, Failed/Unknown → Ambiguous; four statuses need four justifications | |

**User's choice:** Ambiguous — immediate gate.
**Notes:** Accepted cost recorded in CONTEXT.md D-05 — an unattended run now stops where it
previously advanced. Rated `costly` for reversibility on behavioural, not code, grounds.

### Q2 — Minimal guard or structural fix

| Option | Description | Selected |
|--------|-------------|----------|
| Exhaustive match on status | No wildcard reaches `Passed`, so a new `AgentStatus` variant is a compile error; matches the repo's structural-over-hand-audited line | ✓ |
| Equality guard on the arm | One line, smallest blast radius; leaves the hand-audit weakness the doc comment already names | |
| You decide | Leave the shape to the planner | |

**User's choice:** Exhaustive match on status.
**Notes:** The function's own doc comment at `pipeline_outcomes.rs:184-188` admits the current
equality test was audited by hand; `IdleTimeout` arriving in Phase 31 is proof the variant set
grows.

### Q3 — How criterion 4's open question is established and recorded

| Option | Description | Selected |
|--------|-------------|----------|
| Executable demo + written finding | Test pins the pre-fix manufactured pass; short written trace with file:line alongside | ✓ |
| Written audit only | Cheapest, matches how 999.67's analogous question was answered; nothing prevents drift | |
| Executable demo only | Test as the record; the *why* lives nowhere a future reader finds it | |

**User's choice:** Executable demo + written finding.

### Q4 — Test shape for criterion 3

| Option | Description | Selected |
|--------|-------------|----------|
| Full matrix sweep + named controls | Table-driven over every `(AgentStatus × Option<Verdict>)` pair, plus named positive and negative controls; Phase 30's constraint-9 sweep is precedent | ✓ |
| Four named mirror tests | As the roadmap entry proposes; covers only the four pairs someone thought to write | |
| Both | Coverage plus legibility; overlapping assertions to keep in sync | |

**User's choice:** Full matrix sweep + named controls.

---

## Drain-gate depth

**Context surfaced before the questions:** 31/D-10's premise that Code "is the only stage that
actually backgrounds" is contradicted by the Phase 26 dogfood run, where the **Plan** stage's
orchestrator backgrounded its `gsd-phase-researcher` Agent() call and ended its turn, losing the
work under one-shot `claude -p` while still exiting 0. Recorded as F-02 in CONTEXT.md, with the
note that this does not make the close rule wrong — `NeverAnnounced` is vacuously drained by design
and `Pending(n>0)` correctly blocks.

### Q1 — How criterion 2's per-stage re-derivation is done

| Option | Description | Selected |
|--------|-------------|----------|
| Empirical, from each capture | Inspect each capture for whether `background_tasks_changed` appears, arity, and drain-before-marker; nearly free, and functions as a negative control | ✓ |
| Reasoned argument only | Rest on the existing `NeverAnnounced`/`Pending`/`Unreadable` design; but that is the "reasoned, not witnessed" standard 31/D-09 declined | |
| Empirical plus a stated limit | Same work, with the single-capture bound written down | |

**User's choice:** Empirical, from each capture.

### Q2 — Number of captures per stage

| Option | Description | Selected |
|--------|-------------|----------|
| n=1 per stage | Matches criterion 1's literal wording, keeps the phase in its size cap; the limit must be stated in the summary | ✓ |
| n=1, plus repeats where it matters | Second run only where the first shows a `background_tasks_changed` event; unbounded until captures return | |
| n≥2 for every stage | Mirrors 30c reliability trials; eight real agent runs, likely past the size cap alone | |

**User's choice:** n=1 per stage.
**Notes:** The "what n=1 does not establish" statement is carried into CONTEXT.md D-10 as a
required summary item, not an optional caveat.

### Q3 — A capture showing a list that never drains

| Option | Description | Selected |
|--------|-------------|----------|
| Widen it — that's the rule working | A non-draining list means the close rule correctly held stdin open — the 999.64 orphan prevented; record it as a stage where the drain arm is load-bearing rather than defensive | ✓ |
| Leave narrow pending investigation | Consistent with the earlier missing-evidence answers; but declines to widen where the mechanism demonstrably worked | |
| Depends which state | `Pending(n>0)` that resolves → widen; `Unreadable` or still-pending at run end → file | |

**User's choice:** Widen it — that's the rule working.
**Notes:** Consequence to check in the capture, recorded in D-11 — stdin held open means the run
leans on the idle timeout, so the capture must show the timeout behaved.

### Q4 — D-14 per-child declared tokens

| Option | Description | Selected |
|--------|-------------|----------|
| Out — stays deferred | Re-filed as its own numbered backlog entry so the 999.73 pairing note does not expire silently | ✓ |
| In — pair them | Strictly better evidence for the criterion this phase is trying to satisfy; pushes well past the size cap | |
| Decide after the captures | Hold until coalescing is shown to obscure something; an open decision carried mid-phase | |

**User's choice:** Out — stays deferred.

---

## Claude's Discretion

- **Capture acquisition** — offered as a gray area, not selected. Where the four real per-stage
  captures come from and what the per-stage pass bar is. Constrained by D-02 and D-10.
- **Plan sequencing within the phase** — 999.73 and 999.74 have no structural dependency.
- **Where the exhaustive-match rewrite physically lands**, and how D-07's pre-fix demonstration is
  scaffolded.

## Deferred Ideas

- **D-14 per-child declared tokens** — deferred on size for the second time (31/D-14 was the
  first). Action item: file as its own numbered `999.x` ROADMAP entry plus a Linear issue.
- **Parser or monitor defects revealed by a per-stage capture** — filed with the capture as
  evidence, not fixed in-phase (D-04).
- **Un-widened stages** — if a capture cannot be obtained, the remaining widening returns as a
  numbered `999.x` entry and the phase closes PARTIAL on criterion 1 (D-02).
