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

## Revisions after adversarial review (2026-11-05)

Reviewers: codex (gpt-6.1-sol, high) and DeepSeek Flash via `pi`. agy was dropped: quota
exhausted (HTTP 429, about 36 h to reset). Each finding below was checked against source before it
changed a decision.

| Decision | Finding | Change |
|----------|---------|--------|
| D-01 | `resume` refuses over a live run, so a fallback cannot be armed mid-flight | Scoped to `start` and resume of a non-live run; re-arming clears the consumed mark |
| D-03 / D-07 | Preflight is stage-dependent, so launch validation cannot cover every handoff stage | Launch validation made best-effort; full check at handoff; accepted limit recorded |
| D-04 | Recorded canary is reused across agents; relaunch failure after commit had no recovery | Invalidate canary on substitution; recovery to a gate or cron arm |
| D-06 | Infra ceiling shared with `Ambiguous` re-drives; failover ordering unstated | Failover before the ceiling, not charged to it; boundary test |
| D-09 | Plan's idempotent prompt accepts a partial PLAN.md; Code relaunch drops the audit-fix intent | Persist and replay the interrupted intent; falsifiable tests for both |
| D-12 | No "liveness unavailable" switch; failover relaunch may not read liveness at all | Trace first, then controlled interventions; a trace showing no read is the result |
| D-14 | No mechanism for the `n` verb check | Scan clap variants, names and aliases; negative-control fixture |
| D-15 | `IdleTimeoutRecord` has no free-text field | Optional serde-default fields written by the monitor; pre-51 record test |
| D-16 | Rate-limit classifier never sees `line_shapes` | Classifier receives the torn-line signal; measurement is not a gate |
| D-17 | Phase 50.1 superseded part of WR-12/IN-10 | Reproduce first; WR-13 test kept |

Not adopted: pi's claim that D-10 reverses the idle-timeout rule. Today's rate-limit cron arm
already restarts on the left-over tree, and a rate limit is a reported stop, unlike an idle timeout.
The distinction is now stated in D-10.
