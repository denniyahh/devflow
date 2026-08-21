# Phase 42: Hermes Driver - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-21
**Phase:** 42-hermes-driver
**Areas discussed:** Hermes CLI argv & flags, Prompt rendering style, Subagent dispatch capabilities, Antigravity dogfooding execution

---

## Hermes CLI Argv & Flags

| Option | Description | Selected |
|--------|-------------|----------|
| Bare oneshot | `hermes -z "<prompt>" --yolo` | |
| Headless-safe with hooks auto-accept | `hermes -z "<prompt>" --yolo --accept-hooks` + `HERMES_ACCEPT_HOOKS=1` | ✓ |

**User's choice:** Use `hermes -z "<prompt>" --yolo --accept-hooks` + `HERMES_ACCEPT_HOOKS=1` (headless-safe)
**Notes:** Evaluated pros and cons of bare oneshot vs `--accept-hooks`. `--accept-hooks` prevents interactive TTY prompts on unseen shell hooks in headless automation pipelines.

---

## Prompt Rendering Style

| Option | Description | Selected |
|--------|-------------|----------|
| `render_claude_style` | Standard slash commands (`/gsd-*`) + `DEVFLOW_RESULT` marker | ✓ |
| `render_workflow_style` | Workflow file paths (`$HOME/.hermes/...`) + `DEVFLOW_RESULT` marker | |

**User's choice:** `render_claude_style` — standard slash commands + completion marker (matching Claude/Antigravity/OpenCode).
**Notes:** Consistent with other interactive coding agent drivers.

---

## Subagent Dispatch Capabilities

| Option | Description | Selected |
|--------|-------------|----------|
| Dynamic Probe | Check `hermes tools list` for `enabled.*delegation` | ✓ |
| Static False | Default `subagent_dispatch: false` | |

**User's choice:** Dynamic probe — check `hermes tools list` for enabled delegation toolset.
**Notes:** Hermes CLI includes a built-in `delegation` toolset (`✓ enabled delegation 👥 Task Delegation`). Implemented dynamic probe helper mirroring `pi_subagent_dispatch_available`.

---

## Antigravity Dogfooding Execution (ANTG-04)

| Option | Description | Selected |
|--------|-------------|----------|
| Supervised Antigravity Dogfood | Execute Phase 42 via `devflow start --agent antigravity --phase 42 --mode supervise` | ✓ |

**User's choice:** Drive Phase 42 through DevFlow using Antigravity in supervise mode.
**Notes:** Satisfies ANTG-04, measures event cadence distribution against 120s default floor, verifies `--print-timeout 60m` override, and unlocks `--mode auto` in preflight.

---

## Operator's Discretion

- Test fixture structuring and internal helper naming for the driver implementation.
- Specific version extraction regex for doctor checks.

## Deferred Ideas

- None — discussion stayed within Phase 42 scope.
