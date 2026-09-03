# Phase 44: Codex End-to-End Verification - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-25
**Phase:** 44-codex-end-to-end-verification
**Areas discussed:** Codex E2E Dogfood, Agent Handoff, Hermes Cron Resume Contract, Cron Record Consumption

---

## Codex E2E Dogfood

| Option | Description | Selected |
|--------|-------------|----------|
| Pure verification pass | Run Codex E2E separately from the hardening issues. | |
| Dogfood the hardening work | Treat #147/#148/#153 as the implementation surface the Codex run proves. | ✓ |
| Re-scope to driver internals | Re-open Codex adapter design independently of the issues. | |

**User's choice:** Scope was pre-locked by operator: CODE-01 plus #147, #148, and #153.
**Notes:** Codex Define/Plan interactivity limits are already encoded in the driver; planning must
create artifacts before expecting headless Codex progress.

---

## Agent Handoff

| Option | Description | Selected |
|--------|-------------|----------|
| `resume --agent <AGENT>` | Extend the existing continuation command; preserve branch/worktree/state. | ✓ |
| `handoff --to <AGENT>` | Add a dedicated command for the same state mutation. | |
| Manual state edit | Keep editing `.devflow/state-NN.json` by hand. | |

**User's choice:** Issue #147 specifically frames `resume --agent` as the desired surface.
**Notes:** Lock decisions: persist agent before relaunch, emit handoff event, preserve all other
state, and refuse unsafe stage/driver combinations.

---

## Hermes Cron Resume Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Keep `--from-devflow` | Wait for Hermes to add the missing flag. | |
| Use supported Hermes flags | Emit a command using existing cron flags such as `--script` / `--no-agent`. | ✓ |
| Manual operator reconstruction | Continue requiring the operator to translate the record by hand. | |

**User's choice:** Issue #148 requires the printed instruction to be executable as emitted.
**Notes:** Lock decisions: remove `--from-devflow`, verify any Hermes contract against installed
help, and fix timezone semantics so UTC reset instants are not emitted as local cron fields.

---

## Cron Record Consumption

| Option | Description | Selected |
|--------|-------------|----------|
| Delete on resume invocation | Remove the file as soon as `devflow resume` starts. | |
| Delete after confirmed relaunch | Remove only once monitor/stage relaunch succeeded. | ✓ |
| Delete only during recover/clean | Keep today's cleanup-only behavior. | |

**User's choice:** Issue #153 requires deletion after genuine consumption.
**Notes:** Lock decisions: failed resume preserves retry record; successful resume and successful
ship both clean idempotently and emit audit events.

---

## Claude's Discretion

- Exact handoff and cron-deletion event field names.
- Whether same-agent `resume --agent` is accepted as idempotent or rejected as a no-op.
- Helper/module layout for local-time conversion and Hermes command rendering.

## Deferred Ideas

- Dedicated `devflow handoff` command unless `resume --agent` proves unsuitable.
- General scheduler abstraction beyond Hermes.
- New agent drivers or broad event-schema redesign.
