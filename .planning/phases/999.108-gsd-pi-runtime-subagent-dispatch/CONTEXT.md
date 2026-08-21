---
status: backlog
source: 2026-08-18 — running `$gsd-plan-phase 40` from a Pi session
linear: https://linear.app/denniskim/issue/DEN-114
---

# Backlog: GSD Subagent Dispatch From the Pi Runtime

> Filed after `$gsd-plan-phase 40` could not spawn its subagents from a Pi session, forcing inline
> execution.

## Goal

Let GSD's subagent-dispatching workflows run natively from a Pi session, rather than requiring a
Codex or Claude Code session.

## The gap (observed)

- Pi has a subagent mechanism (a `subagent` tool, provided by `@bacnh85/pi-subagent`), but GSD's
  workflows do not use it.
- GSD does not recognize `pi` as a runtime; its runtime-name policy omits it, and detection falls
  back to another runtime's behavior.
- GSD's subagent dispatch currently supports only two shapes, neither of which matches Pi's
  mechanism.

## Live reproduction (2026-08-19)

The Phase 40 Pi dogfood (`devflow start --agent pi --phase 40 --mode supervise`) reproduced this gap
end-to-end:

- **The `subagent` tool is available headlessly.** A fresh `pi -p --no-approve` lists `subagent` in
  its tool schema (alongside read/bash/edit/write), so the extension loads correctly in `-p` mode —
  "fails closed headlessly" is about project-scoped agents, not the tool itself.
- **The Code stage still dispatched nothing.** Over a full Code stage (5 commits, incl. a correct
  999.85 rewrite), the Pi agent made **0 `subagent` tool calls**. Its session log shows it read the
  plan text (49 "subagent" mentions) but never invoked the tool.
- **Root cause — runtime mismatch, not tool absence.** DevFlow's Code-stage prompt is
  "follow `execute-phase.md`", which dispatches `gsd-executor` via Claude/Codex `Agent(...)` syntax.
  A Pi agent has no `Agent`/`Task` tool, so it hit the workflow's own "Agent unavailable → sequential
  inline execution" fallback and did the work itself. The workflow never references the `subagent`
  tool, so the agent never reached for it.

Net: the capability is detected and the tool is present, but the GSD workflow cannot use it because
it only knows the Claude/Codex `Agent()` dispatch model. This is the concrete reproduction the item
lacked.

## References

- `~/.codex/gsd-core/references/runtime-aware-dispatch.md` — GSD's current dispatch model.
- `~/.pi/agent/npm/node_modules/@bacnh85/pi-subagent/README.md` — Pi's subagent tool and bundled roles.
- `.planning/UPSTREAM-GSD-ISSUES.md` — if this lands as a GSD-core change, upstream coordination may be needed.
