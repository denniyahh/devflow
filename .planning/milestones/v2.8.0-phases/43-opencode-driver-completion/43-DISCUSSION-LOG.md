# Phase 43: OpenCode Driver Completion - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-08-23
**Phase:** 43-OpenCode Driver Completion
**Areas discussed:** Live-capture strategy, Health check signal, Capability discovery scope

---

## Live-capture strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Capture now | Run a cheap real probe during this discussion so decisions are grounded in the actual schema, not an assumed one | ✓ |
| Defer to research | Let gsd-phase-researcher do the live capture as part of its normal research pass | |

**User's choice:** Capture now.
**Notes:** Ran three real `opencode run ... --auto --format json` invocations in an isolated
scratch directory (never inside the devflow repo): a plain success case, a tool-invoking case, and
a negative-control error case (invalid `--model`). Confirmed the real event schema (`step_start`,
`text`, `tool_use`, `step_finish`, `error`) is flatter than Codex's assumed shape and does NOT have
a `turn.completed`/`turn.failed` terminal event. Raw captures saved under
`.planning/phases/43-opencode-driver-completion/43-evidence/`.

---

## Health check signal

| Option | Description | Selected |
|--------|-------------|----------|
| Credential check (recommended) | Parse `opencode providers list` (or equivalent) to confirm at least one provider/credential resolves, mirroring Pi's `auth check` | ✓ |
| Presence-only | Binary + `--version` only, like the current doctor entry | |

**User's choice:** Credential check.
**Notes:** Verified live that `opencode providers list` (alias `auth list`) reports real configured
credentials on this machine, so a genuine fail-closed signal exists. Also discovered during
follow-up probing (not asked as a separate question, but surfaced as load-bearing evidence):
`providers list` has NO JSON output mode (ANSI-colored box-drawing text, needs stripping), and
`opencode models` — while cleaner text — always lists opencode's own free-tier models regardless of
configured credentials, so it was rejected as a false-positive health signal despite being easier
to parse.

---

## Capability discovery scope

| Option | Description | Selected |
|--------|-------------|----------|
| In scope — probe it | Add an OpenCode capability probe mirroring Pi's subagent-dispatch pattern | ✓ |
| Out of scope — health only | OPCD-03's capability discovery satisfied by defaults; no subagent probing | |

**User's choice:** In scope — probe it.
**Notes:** OpenCode has a real `opencode agent list`/`agent create` subsystem (verified via
`--help`), confirming there's something real to probe, not a speculative feature.

---

## Claude's Discretion

- Exact ANSI-stripping / provider-list parsing implementation
- Exact subagent-detection probe/heuristic for OpenCode's `agent` subsystem
- Whether to fix the stale `cargo install opencode` doctor install hint as a drive-by
- Unit/integration test fixture layout (stub-binary pattern per `pi.rs`)

## Deferred Ideas

- Version floor / pinning `opencode` — not requested, presence-only doctor check stays
- Fixing the stale `cargo install opencode` doctor hint — noted, left to discretion
- Deeper capability probing beyond subagent dispatch (`opencode mcp`, `opencode plugin`) — out of
  scope
