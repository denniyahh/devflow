# Phase 39: Pi End-to-End - Context

**Gathered:** 2026-08-17 (revised after adversarial review — renumbered 38 → 39)
**Status:** Ready for planning (transport gated on Phase 37.1's verdict)

## Phase Boundary

Phase 39 makes `devflow start --agent pi` complete the pipeline, to the degree Pi supports identical
functionality (D-01). The **baseline** transport is `Legacy`/`-p` with structured completion
detection — **no drain-gate claim**. The full-dispatch arm (subagent coverage + `CloseRule`
integration) is gated on Phase 37.1's subagent-extension verdict and lands as a follow-on. Anything
Pi cannot natively support is left out and recorded, not worked around.

## Implementation Decisions

### Scope & success bar
- **D-01 — Full Claude parity, baseline-first.** The success bar is `devflow start --agent pi`
  completing Plan → Code → Validate → Ship (Define is a no-op for every agent, D-14). Parity is
  achieved *incrementally*: the baseline (structured completion) lands in this phase; the
  full-dispatch arm depends on 37.1's verdict. Anything Pi cannot support is excluded + recorded.

### Transport & completion
- **D-02 — Baseline transport: `Legacy`/`-p` + structured completion.** Phase 39 keeps Pi on the
  process-exit `Legacy` path (`-p`) and adds structured completion detection — parse Pi's output
  (`agent_end` / `stopReason` / `willRetry` / an embedded `DEVFLOW_RESULT` marker) into a
  completion/error verdict. **No drain-gate claim** — the review established Pi's `--mode json` is
  single-shot and its event vocabulary has no task-lifecycle events, so `CloseRule` coverage is
  not asserted. — **Reversibility:** reversible — a positive 37.1 verdict upgrades the transport in
  a follow-on without redoing this phase.
- **D-02a — Full dispatch deferred to 37.1.** Whether Pi routes through the pipe-owning arm + drain
  gate (and which subagent extension enables it) is decided by Phase 37.1's spike, not this phase.

### Interactivity
- **D-04 — Define is a no-op (drop `RequiresExistingArtifact` for Define).** The Define stage never
  runs the interactive interview (D-14), so declaring it `RequiresExistingArtifact` would wrongly
  block a fresh `devflow start --agent pi`. Plan's interactivity is TBD — resolved by 37.1 + the
  Codex precedent (Codex's Plan is warn-only today, not a gate). The gate itself is made
  driver-driven in Phase 38, which this phase depends on.

## Deferred (explicitly not here)

- **999.94** — unattended `decision` checkpoint first-option guard (D-03).
- **Full dispatch + `CloseRule` coverage for Pi** — gated on Phase 37.1's verdict.
- **Pi capabilities that cannot match Claude** — left out per D-01, recorded as limitations.

## Canonical References

- `crates/devflow-core/src/agent_result.rs` — Layer 1 completion classification (where Pi's
  completion parser plugs in, alongside `parse_codex_event_result`).
- `crates/devflow-core/src/agents/pi.rs` — the current `PiDriver` (`-p`, `--no-approve`,
  `pi auth check` health).
- `.planning/reviews/phase-38/SUMMARY.md` — the adversarial review that forced this re-scope
  (the `PipeOwning` deadlock, the void D-02 drain claim, the D-01/D-04 Define contradiction).
- `.planning/milestones/v2.6.0-phases/37-modular-agent-driver-architecture-pi-driver-999-31-pi/37-CONTEXT.md`
  — the AgentDriver contract + the Pi deferrals this phase picks up.

## Existing Code Insights

### Reusable Assets
- `parse_codex_event_result` (`agent_result.rs`) — the model for a `parse_pi_result` completion
  parser (Pi output → `AgentResult`).
- `PiDriver` already carries `--no-approve` + `pi auth check` health.

### Integration Points
- `pipeline_launch.rs` `resolve_launch_shape` — where Pi stays on `Legacy` (no change this phase).
- `agent_result.rs` Layer 1 — where Pi's completion parser registers.

## Specific Ideas

No specific requirements — open to standard approaches within the existing completion-parser pattern.
