# Phase 39: Pi End-to-End - Context

**Gathered:** 2026-08-17 (revised after adversarial review — renumbered 38 → 39; re-scoped again after 37.1's verdict flip to VIABLE)
**Status:** Ready for planning — Phase 37.1 verdict **VIABLE** (revised). Both arms in scope.

## Phase Boundary

Phase 39 makes `devflow start --agent pi` complete the pipeline, to the degree Pi supports identical
functionality (D-01), in two stages.

- **Stage 1 (baseline):** `Legacy`/`-p` + completion detection + the `litellm` provider fix.
- **Stage 2 (dispatch):** integrate `@bacnh85/pi-subagent` at user scope — synchronous, in-process,
  so `MonitorLaunch::Legacy` process-exit supervision already observes completion. **No
  `CloseRule`/drain-gate, no `PipeOwning`** — the 37.1 verdict re-scoped the full-dispatch arm to
  drop that machinery (its original premise was the false one).

Anything Pi cannot natively support is left out and recorded, not worked around.

## Implementation Decisions

### Scope & success bar
- **D-01 — Full Claude parity, staged.** The success bar is `devflow start --agent pi` completing
  Plan → Code → Validate → Ship (Define is a no-op for every agent, D-14). Stage 1 lands the
  baseline; Stage 2 lands dispatch — both this phase, now that 37.1 is VIABLE.

### Transport & completion
- **D-02 — Baseline transport: `Legacy`/`-p` + completion detection.** Stage 1 keeps Pi on the
  process-exit `Legacy` path (`-p`). Completion is the `DEVFLOW_RESULT` marker (plain text in `-p`
  mode) — Layer 1's generic `parse_devflow_result` already scans for it. **No drain-gate claim.**
- **D-02a — Dispatch arm (Stage 2): VIABLE via `@bacnh85/pi-subagent`.** 37.1's revised spike found
  `@bacnh85/pi-subagent` works as-is: synchronous (`await`), in-process (SDK sessions, no child
  process), fails closed headless (project agents refused without UI), default scope `user`. Because
  `execute()` awaits its children, the parent stays alive until subagents finish, then emits
  `DEVFLOW_RESULT` and exits — `MonitorLaunch::Legacy` observes it. **No `CloseRule`/drain-gate and
  no `PipeOwning` are needed.** Install (or vendor-pin) at user scope; trust-boundary confirmation
  is the Stage-2 acceptance gate. The process-spawning alternatives (`@dreki-gg`, `@smoose`, bundled
  example) need a `--no-approve` child-argv patch; `@mystilleef` is excluded (passes `--approve`,
  default scope `both`).
- **D-03 — Provider fix (Stage 1):** `PiDriver::health` hardcodes `pi auth check --provider google`,
  but this machine's Pi runs on the `litellm` gateway (`models.json` → `providers.litellm`, models
  `deepseek-v4-pro`/`deepseek-v4-flash`). The health check must probe the actually-configured
  provider, not `google`.

### Interactivity
- **D-04 — Define is a no-op.** Define never runs the interactive interview (D-14); the
  driver-driven gate is **Define-only** (Phase 38 delivered it, then reverted the Plan extension —
  `PLAN.md` is an output, not a precondition). Plan is warn-only/un-gated.

## Deferred (explicitly not here)

- **999.94** — unattended `decision` checkpoint first-option guard.
- **Isolated-context (process-spawning) dispatch** — if ever wanted, it's a follow-on with the
  `--no-approve` child-argv patch + a Pi drain predicate; `@bacnh85`'s in-process model makes it
  unnecessary this phase.
- **Pi capabilities that cannot match Claude** — left out per D-01, recorded as limitations.

## Canonical References

- `crates/devflow-core/src/agent_result.rs` — Layer 1 (`parse_devflow_result`, `evaluate_layer1`).
- `crates/devflow-core/src/agents/pi.rs` — `PiDriver` (`-p --no-approve`, the `--provider google`
  health to fix).
- `.planning/reviews/phase-37.1/research/RESEARCH-SUMMARY.md` + `codex.md`/`antigravity.md` — the
  primary-source investigation that flipped 37.1 to VIABLE (candidates table, trust-boundary
  file:line evidence).
- `.planning/phases/37.1-pi-subagent-extension-spike-research/37.1-DECISION-GATE.md` — the revised
  VIABLE gate.
- `.planning/reviews/phase-38/SUMMARY.md` — the `PipeOwning` deadlock + D-01/D-04 Define
  contradiction (still load-bearing for Stage 1).

## Existing Code Insights

### Reusable Assets
- `parse_devflow_result` / `evaluate_layer1` (`agent_result.rs`) — the generic marker path that
  already covers `@bacnh85`'s plain-text completion; a `parse_pi_result` is likely unnecessary.
- `PiDriver` already carries `--no-approve`; its `health` is where the `litellm` fix lands.

### Integration Points
- `pipeline_launch.rs` `resolve_launch_shape` — Pi stays on `Legacy` (no change; add a regression test).
- `agents/pi.rs` `health` — the `--provider google` → `litellm` fix.
