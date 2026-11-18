# Phase 53: The Question Channel - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-18
**Phase:** 53-the-question-channel
**Mode:** `--auto` — every selection below is the recommended option taken automatically, not an operator answer.
**Areas discussed:** MCP spike, Server shape and transport, Tool payload, Operator via gate, Prompts and one-shot checkpoint, Event log and history, Closing run

---

## MCP spike

| Option | Description | Selected |
|--------|-------------|----------|
| Pre-planning check recorded in 53-MCP-SPIKE.md | Spike gates transport tasks (52 D-19 pattern) | ✓ |
| Run the live spike inside this auto discuss | Paid live launches of claude and codex | |
| Fold into research | Research reads docs only, no live proof | |

**Choice:** [auto] pre-planning check (D-01). **Notes:** help-text facts only; client timeouts unverified.

---

## Server shape and transport

| Option | Description | Selected |
|--------|-------------|----------|
| Hidden devflow subcommand, per-launch registration, server uses the Gates API | No runner relay | ✓ |
| Server relays to the runner over a socket | Runner owns the gate | |

**Choice:** [auto] D-03..D-06; hand-rolled sync JSON-RPC preferred over an async SDK.

---

## Tool payload

| Option | Description | Selected |
|--------|-------------|----------|
| question + at least one typed option + context + kind; typed error on malformed input | | ✓ |
| Free-text-only questions allowed | | |

**Choice:** [auto] D-07, D-08.

---

## Operator via gate

| Option | Description | Selected |
|--------|-------------|----------|
| New GateKind::Question and verb `gate answer --gate-id --option/--text`; approve/reject refused on it | | ✓ |
| Reuse `gate approve --note` for answers | | |

**Choice:** [auto] D-09..D-11.

---

## Prompts and one-shot checkpoint

| Option | Description | Selected |
|--------|-------------|----------|
| Remove positional section and auto-decide relaunch; tool call everywhere; DEVFLOW_RESULT declaration only as failure fallback | | ✓ |
| Keep the relaunch as a parallel path | | |

**Choice:** [auto] D-12..D-14.

---

## Event log and history

| Option | Description | Selected |
|--------|-------------|----------|
| question_asked / question_answered / question_closed with full text | Phase 54 corpus | ✓ |
| Summary-only events | | |

**Choice:** [auto] D-15, D-16.

---

## Closing run

| Option | Description | Selected |
|--------|-------------|----------|
| `devflow start --until plan` on a throwaway non-DevFlow GSD project, questions via the tool | | ✓ |
| Add a DevFlow discuss stage | Scope creep | |

**Choice:** [auto] D-17; whether it yields real discuss questions is left open for the operator.

---

## Claude's Discretion

Event field names, registration file name and location, hidden subcommand name, `gate show` layout.

## Deferred Ideas

Automatic answering policies (Phase 54); a DevFlow discuss stage (not in scope).
