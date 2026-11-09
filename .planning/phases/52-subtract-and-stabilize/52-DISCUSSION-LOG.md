# Phase 52: Subtract and Stabilize - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-08
**Phase:** 52-subtract-and-stabilize
**Areas discussed:** How a run finishes, Validate reject & gaps loop, Abort & rate-limit gate, Confirm the proposed cuts.
The operator did not select the last area at first; it was asked afterwards because CUT-04 requires this step to
confirm the rows.

---

## How a run finishes

| Option | Description | Selected |
|--------|-------------|----------|
| Approve = finished | Gate at every Validate; approve records workflow_finished, reject runs the fix loop | |
| Gate only on trouble | A clean Validate finishes with no gate | |

**User's choice (free text):** "i'm tempted to do away with supervise mode and only have an auto mode that uses jev/laya
to be intelligent enough to only surface issues when needed to the operator. would this be practical or should we still
support a supervise mode that is not too overbearing?"
**Notes:** Claude answered:
- triage does not exist until Phase 54;
- supervise caught 999.189 and 999.193 in Phase 51;
- finishing no longer guards anything irreversible, so supervise's remaining value is the in-run reject-with-note loop.

The follow-up offered: Keep minimal, revisit in 54 ✓ / Remove supervise now / Supervise = gate only on trouble.

| Option | Description | Selected |
|--------|-------------|----------|
| Fire notify on finish | DEVFLOW_GATE_NOTIFY_CMD once on workflow_finished | ✓ |
| Event log only | No push notification | |

| Option | Description | Selected |
|--------|-------------|----------|
| No, keep --until plan | Supervise stays Validate-only | ✓ (via "Keep minimal", which states no Plan gate) |
| Yes, gate Plan→Code | Add a second gate kind | |

**Notes:** The operator first deferred this until the supervise question was settled. The "Keep minimal" option they
then chose states "No Plan gate".

| Option | Description | Selected |
|--------|-------------|----------|
| No --from; Plan is idempotent | Always start at Plan | ✓ |
| Add --from <stage> | Skip Plan when plans exist | |

---

## Validate reject & gaps loop

| Option | Description | Selected |
|--------|-------------|----------|
| Agent AND report | Verdict needs the agent's pass and the report's passed | ✓ |
| Report in gate context only | Keep the agent's verdict | |

| Option | Description | Selected |
|--------|-------------|----------|
| Stage-level: Plan→Code→Validate | plan --gaps, then execute --gaps-only, as separate launches | ✓ |
| One relaunch, three commands | A single session | |

| Option | Description | Selected |
|--------|-------------|----------|
| Code + note via /gsd-quick | Note verbatim as the task | ✓ |
| Code + note via /gsd-audit-fix | Note becomes one input among the audit's findings | |
| Refuse; reject needs gaps | No fix loop for a clean pass | |

| Option | Description | Selected |
|--------|-------------|----------|
| Gate + upstream issue | Stay within GSD | ✓ |
| Prompt the agent to run the verifier | Bends GSD | |

| Option | Description | Selected |
|--------|-------------|----------|
| Reuse existing caps | 3 consecutive / 10 per phase | ✓ |
| Separate, lower gaps cap | New counter | |

**Stale passed report.** The operator asked: "is this a bug or a gap in GSD that allows this to happen? or is this
something else?" Claude read gsd-core 1.20.1 (`verification.cjs` L257-290, L1537; `execute-phase.md` L370-397):
- GSD reports `stale` for this case and routes it to execute-phase, which re-runs the verifier;
- the hole was in Claude's proposal to parse frontmatter;
- the real GSD gap is `gaps_found` taking priority over `stale` (999.208).

The follow-up offered: Use GSD's reader + route ✓ / Parse frontmatter + show staleness.

---

## Abort & rate-limit gate

| Option | Description | Selected |
|--------|-------------|----------|
| New verb: gate abort | Explicit abort field; reject always means fix | ✓ |
| Flag: gate reject --abort | One flag apart from reject | |

| Option | Description | Selected |
|--------|-------------|----------|
| Blocking gate; approve = relaunch | One gate model | |
| Stop and record; resume by hand | No waiter | |
| Auto-relaunch at reset time | Supervisor relaunches after resetsAt | ✓ |

**Notes:** This conflicted with CUT-03 and Phase 55 criterion 1. The follow-ups chose "Phase 52, waiting as a gate" ✓
(over "Phase 52 gate only; auto in 55") and "No time → plain gate; 1 auto try" ✓ (over "Retry up to N with backoff").

---

## Confirm the proposed cuts

| Option | Description | Selected |
|--------|-------------|----------|
| Ship-dependents (9 rows) | Code that serves only Ship or the hook batch | ✓ |
| Legacy paths (6 rows) | Legacy launch, hidden --worktree, positional args, --reap-strays | ✓ |
| reference, list, test | Duplicate git or scripts/check.sh | ✓ |
| release --check/--verify | Used by scripts/cut-release.sh | ✓ |

**Notes:** 19 proposed rows, plus the 9 already decided by the operator, make all 28 `cut` rows. The ROADMAP's "21
proposed" count predates the `parallel` decision.

## Claude's Discretion

- How the note is framed in the prompt.
- The margin after resetsAt, and how the wait is implemented.
- The resume-command shape.
- Whether the 999.192 authorship heuristic survives.
- OPS wording.
- The wave split after the cut wave.

## Deferred Ideas

- Phase 54: fold auto/supervise into a triage policy.
- gsd-core upstream issue: `gaps_found` takes priority over `stale`.
