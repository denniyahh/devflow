# Phase 39: Pi End-to-End - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-17 (revised after adversarial review + renumber)
**Phase:** 39-pi-end-to-end (was 38-pi-end-to-end before the renumber)
**Areas discussed:** success bar, transport, 999.94 scope, interactivity, sequencing/renumber

---

## Success bar

**Original choice:** FULL Claude parity — `devflow start --agent pi` does everything Claude does, to
the degree Pi supports identical functionality; anything Pi can't natively do is left out + recorded.

**Revision (post-review):** re-scoped to "baseline-first" — structured completion now; the
full-dispatch arm is gated on 37.1. Captured as D-01.

## Transport

**Original choice:** commit to `--mode json` + drain-gate coverage (the unwrapper translates Pi's
schema into the monitor vocabulary).

**Revision (post-review):** baseline `Legacy`/`-p` + structured completion, **no drain-gate claim**.
The review proved Pi's `--mode json` is single-shot (not a stream) and its event union has no
task-lifecycle events, so `CloseRule` coverage is not asserted. Captured as D-02 + D-02a.

## 999.94 scope

**Choice:** OUT — defer. Captured as D-03.

## Interactivity

**Original choice:** mirror Codex (Define + Plan → `RequiresExistingArtifact`).

**Revision:** Define dropped — it's a no-op per D-14, so the declaration would wrongly block a fresh
run. Plan TBD by 37.1 + the Codex precedent (Codex's Plan is warn-only). Captured as D-04.

## Sequencing / renumber (Dennis)

- **37.1** = Pi subagent-extension spike (research), gates 39's transport.
- **38** = Driver Contract Completion (`AgentAdapter` removal + `InteractivityMode` consumption + `999.107`).
- **39** = Pi End-to-End (this phase), gated on 37.1 + 38.
- Execution order: **37.1 ∥ 38 in parallel → 39.**

## the agent's Discretion

- The exact Pi completion-parse mapping (`agent_end`/`stopReason`/`willRetry`/`DEVFLOW_RESULT` → `AgentResult`).
- The provider/credential fix (claude finding A) is in this phase's scope but its exact mechanism is a planning detail.

## Deferred Ideas

- 999.94 → later phase.
- Full dispatch + `CloseRule` coverage for Pi → follow-on, gated on 37.1's verdict.
- Pi capabilities that can't match Claude → left out, recorded (D-01).
