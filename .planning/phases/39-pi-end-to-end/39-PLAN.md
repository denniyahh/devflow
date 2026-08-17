# Phase 39: Pi End-to-End — Plan

**Planned:** 2026-08-17
**Source:** `39-CONTEXT.md` (D-01..D-04) + Phase 37.1 verdict (NOT VIABLE → baseline arm only)
**Package under test:** `devflow` (binary-only) + `devflow-core` (lib)

## Objective

Make `devflow start --agent pi` complete the pipeline (Plan → Code → Validate → Ship) on the
**baseline arm only** — `Legacy`/`-p` transport + a Pi-specific structured-completion parser. No
drain-gate claim, no full-dispatch arm (37.1 verdict: NOT VIABLE). Anything Pi cannot natively
support is recorded as a limitation, not worked around.

## Open questions to resolve at the top of execution (research, not guesses)

These are load-bearing and must be answered against the live `pi` 0.84.1 binary before/while
implementing, per the repo's verify-don't-assume rule:

1. **What does `pi -p --no-approve "<prompt>"` actually emit on stdout?** The baseline transport
   is `-p` (print mode). The CONTEXT names `agent_end`/`stopReason`/`willRetry` — which are
   `--mode json` vocabulary, not `-p` vocabulary. A `DEVFLOW_RESULT` marker emitted by `-p` is
   plain text and *may already be caught* by `parse_devflow_result`'s tail scan. The executor
   must capture a real `pi -p` run's stdout and decide whether `parse_pi_result` adds anything
   over the generic marker path, or whether it is needed only for a future `--mode json` arm.
2. **Provider/credential health:** `pi.rs` hardcodes `pi auth check --provider google`, which
   returns `not_ready` on this machine while `pi` runs fine (operator `litellm`/`deepseek`
   config). Decide: drop `--provider google` (let `pi` probe its configured default), or read the
   provider from Pi's own config. Verify against the live binary.

## Waves

### Wave 1 — Pi completion parser (the core)
- **T-39-01** Add `parse_pi_result(stdout) -> Option<AgentResult>` in `agent_result.rs`, modeled
  on `parse_codex_event_result`: recognize Pi's completion/error signals (the `DEVFLOW_RESULT`
  marker, and — for the structured arm — `agent_end`/`stopReason`/`willRetry`) into an
  `AgentResult`. Resolve open question 1 first; if `-p` output is plain marker text, the parser
  is a thin, Pi-specific wrapper that also handles Pi's error/retry vocabulary. Must be gated by
  an `is_pi_output`/`is_pi_event_stream` predicate so it never misclassifies Claude/Codex captures.
- **T-39-02** Register `parse_pi_result` in `evaluate_layer1`'s `.or_else` chain (after the
  Claude/Codex parsers, before the final rate-limit heuristic), so Layer 1 owns Pi captures.

### Wave 2 — wire the driver
- **T-39-03** `PiDriver::parse_completion` → `parse_pi_result` (the `AgentDriver` hook that
  replaces the deleted `AgentAdapter::completion_signal_detected`).

### Wave 3 — provider/credential health fix
- **T-39-04** Fix `PiDriver::health`'s hardcoded `--provider google` (open question 2). The check
  must reflect the operator's actually-configured provider rather than assuming `google`, so a
  working Pi install is not blocked at preflight.

### Wave 4 — transport lock-in + tests
- **T-39-05** Verify (and lock with a test) that `resolve_launch_shape` keeps Pi on
  `MonitorLaunch::Legacy` — Pi is never Claude, so `stream_launch` is false and the `else` branch
  applies. No production change expected; a regression test pins it.
- **T-39-06** Tests for `parse_pi_result`: success marker, failure marker, error/`willRetry`
  signal, and a negative control (a Claude/Codex capture must NOT be classified by the Pi parser).

## Out of scope (recorded, not built)

- Full-dispatch arm + `CloseRule` drain-gate coverage → 37.1 verdict NOT VIABLE; backlog.
- 999.94 (unattended `decision` checkpoint first-option guard).
- `PipeOwning` transport for Pi — proven to deadlock (review); never routed.

## Acceptance

- `cargo test -p devflow-core --lib` and `cargo test -p devflow --bin devflow` — green, real
  `N passed`.
- `devflow start --agent pi --dry-run` shows the Define→Plan→Code→Validate→Ship pipeline for Pi.
- A live `pi -p` smoke probe confirms the baseline completion signal is detectable (open question
  1 answered with evidence, not assumption).
- No `CloseRule`/`PipeOwning`/drain-gate claim added anywhere.
