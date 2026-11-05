# Phase 51: Rate-Limit Agent Failover - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-11-05
**Phase:** 51-rate-limit-agent-failover
**Mode:** `--auto` — every choice below was auto-selected as the recommended option; the operator
did not answer any of these questions individually.
**Areas discussed:** Fallback configuration surface, Handoff mechanism, Mid-stage continuation,
Proving both arms and the SUPV-01 dependency, Keeping the removed handoff dead, Folded backlog items

---

## Fallback configuration surface

| Option | Description | Selected |
|--------|-------------|----------|
| `--fallback-agent` flag on start/resume, persisted in state | One agent, same pattern as `legacy_claude_launch` | ✓ |
| `devflow.toml` key + env var | Standing config; issue #180 and 999.41 argue against new knobs | |
| Ordered agent list | Multi-hop; more surface than the stated need | |

**User's choice:** [auto] flag persisted in state (recommended default)
**Notes:** [auto] Fallback validated at launch, not at handoff. Under `--mode auto` the existing
unattended-launch-shape guard admits only claude and antigravity, so other fallbacks are refused at
launch; widening the guard was deferred.

---

## Handoff mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| Reuse `resume_loaded`'s substitution, relaunch same stage now | One shared function, one monitor | ✓ |
| Write cron instructions naming `resume --agent <fallback>` | Defers the switch to the reset window, defeats the purpose | |

**User's choice:** [auto] reuse the substitution (recommended default)
**Notes:** [auto] One hop per run; stays on the fallback; counts toward the infra ceiling; a
runtime-refused handoff falls to the cron arm with an event. Zero-commit case: continue with the
fallback.

---

## Mid-stage continuation

| Option | Description | Selected |
|--------|-------------|----------|
| Normal stage prompt, rely on existing re-run safety | No new prompt; verify per stage | ✓ |
| New "continue this stage" prompt | StageIntent work; larger change | |

**User's choice:** [auto] normal prompt (recommended default)
**Notes:** [auto] Worktree handed over as-is (no reset/stash); tree state recorded in the event.

---

## Proving both arms and the SUPV-01 dependency

| Option | Description | Selected |
|--------|-------------|----------|
| Hermetic e2e with stub agents, each arm with the other as negative control | Reproducible; a real rate limit cannot be produced on demand | ✓ |
| Live run with a real provider rate limit | Not reproducible on demand | |

**User's choice:** [auto] hermetic e2e (recommended default)
**Notes:** [auto] SUPV-01 settled by running the failover test with the liveness answer
unavailable and recording the result.

---

## Keeping the removed handoff dead

| Option | Description | Selected |
|--------|-------------|----------|
| Rust source-scan test, comments stripped | `doc_check.rs` pattern | ✓ |
| Shell grep count | Repo trap: `rg -c` prints nothing on zero | |

**User's choice:** [auto] Rust source-scan (recommended default)
**Notes:** `ship.rs` holds a negative-assertion string literal `"sequentagent"` in test code; the
scan must not trip on it.

---

## Folded backlog items

| Option | Description | Selected |
|--------|-------------|----------|
| Adopt each entry's own fix shape | 999.102 add fields; 999.71 harness + measurements; 999.154 review fix shape | ✓ |

**User's choice:** operator folded all three in (2026-11-05); fix shapes [auto]-selected from the
entries.

## Claude's Discretion

- State field names, event payload shape, file placement of the shared handoff function, plan split.

## Deferred Ideas

- Driver-owned rate-limit detection for Pi, OpenCode, Hermes, Antigravity
- Ordered fallback list
- Widening the auto-mode launch-shape guard
- Automatic switch back to the primary
