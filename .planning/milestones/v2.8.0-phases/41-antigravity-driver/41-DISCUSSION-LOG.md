# Phase 41: Antigravity Driver - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-19
**Phase:** 41-antigravity-driver
**Areas discussed:** Binary resolution, Launch shape, Completion detection, Health/preflight, Prompt rendering

---

## Binary resolution

| Option | Description | Selected |
|--------|-------------|----------|
| A | Target `agycli` (vetted wrapper, auto skip-permissions, stream-json) | |
| B | Target `antigravity` name + probe stream-json capability, fail-closed | |
| C | Target `antigravity-cli` directly (mise path) | |
| D | Target `agy` — operator re-aliased `agycli` → `agy`; uninstalled conflicting binaries | ✓ |

**User's choice:** `agy` — "I just updated the alias from agycli to agy to make things simpler. I also uninstalled the other antigravity applications to avoid any conflicts."
**Notes:** `agy` is a bash wrapper `exec antigravity-cli --dangerously-skip-permissions "$@"`, v1.1.15. Driver argv must not re-add skip-permissions.

## Launch shape

| Option | Description | Selected |
|--------|-------------|----------|
| A | Stream-json day one (mirror ClaudeDriver) | ✓ |
| B | Single-document `-p "<prompt>" --output-format json` first | |

**User's choice:** A — "GA-2 should use streams like Claude."

## Completion / verdict detection

| Option | Description | Selected |
|--------|-------------|----------|
| A | Parse final stream-json `result` for DEVFLOW_RESULT + honest process-exit fallback | ✓ |
| B | Process-exit + prompt-embedded marker only | |

**User's choice:** A.

## Health / preflight

| Option | Description | Selected |
|--------|-------------|----------|
| A | Presence + version floor (≥1.1.14) + capability check, fail-closed | |
| B | Presence-only | ✓ |

**User's choice:** B — "Unless there is a functional reason to floor the version, presence-only should be fine."
**Notes:** Marker-less contract (D-03) is the functional backstop; a version floor would only improve `devflow doctor` accuracy.

## Prompt rendering

| Option | Description | Selected |
|--------|-------------|----------|
| A | Reuse `render_claude_style` | ✓ |
| B | Dedicated `render_antigravity_style` | |

**User's choice:** A — "Reuse render_claude_style."

## Deferred Ideas

- Version floor / capability probe on `agy` (GA-4 option A) — not chosen.
- Update `research/STACK.md` binary-resolution section to the single-`agy` reality — deferred to plan-phase.
