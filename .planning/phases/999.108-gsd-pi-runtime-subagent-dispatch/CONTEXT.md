---
status: backlog
source: 2026-08-18 — running `$gsd-plan-phase 40` from a Pi session
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

## References

- `~/.codex/gsd-core/references/runtime-aware-dispatch.md` — GSD's current dispatch model.
- `~/.pi/agent/npm/node_modules/@bacnh85/pi-subagent/README.md` — Pi's subagent tool and bundled roles.
- `.planning/UPSTREAM-GSD-ISSUES.md` — if this lands as a GSD-core change, upstream coordination may be needed.
