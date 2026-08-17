# Phase 38: Pi End-to-End - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-17
**Phase:** 38-pi-end-to-end
**Areas discussed:** success bar, transport, 999.94 scope, interactivity

---

## Success bar

**User's choice:** FULL Claude parity — `devflow start --agent pi` does everything Claude does, *to
the degree Pi can support identical functionality*. Anything Pi cannot natively support must be
dealt with separately and left out, unless it is already covered elsewhere in Phase 38.

**Notes:** Define is already a no-op for every agent (D-14); the bar is over the agent stages
(Plan → Code → Validate → Ship). Captured as D-01.

## Transport

**User's choice:** Commit to `--mode json` (event stream). The unwrapper translates Pi's event
schema into the monitor vocabulary so the drain gate gets real coverage.

**Notes:** Rejects the alternative (stay on `-p` and parse output post-hoc), which would give no
real `CloseRule` coverage. Captured as D-02.

## 999.94 scope

**User's choice:** OUT — defer. Not part of Phase 38.

**Notes:** Upstream-adjacent (GSD workflow layer), shares no Phase 38 touch points. Captured as D-03.

## Interactivity

**User's choice:** Mirror the Codex treatment — if Pi cannot support interactivity like Codex
(cannot run the interactive interview / plan overwrite decision headlessly), do the same sane thing
we did for Codex. Dennis will revisit interactivity handling in a later phase.

**Notes:** The operator's intent is to make the gate *generic* this phase, not *smarter*. Captured
as D-04.

## the agent's Discretion

- The exact Pi JSON event → monitor-vocabulary mapping (research/planning detail, not a user decision).
- Phase 39's touch-point overlap (999.106/999.107) is out of scope here but noted for sequencing.

## Deferred Ideas

- 999.94 → later phase.
- 999.106 remainder + 999.107 → Phase 39.
- Pi capabilities that cannot match Claude → left out, recorded as limitations (D-01).
