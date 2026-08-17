# Phase 39: Pi End-to-End — Plan (dual-stage, revised)

**Planned:** 2026-08-17 (revised — 37.1 verdict VIABLE via `@bacnh85/pi-subagent`)
**Source:** `39-CONTEXT.md` (D-01..D-04) + `.planning/reviews/phase-37.1/research/RESEARCH-SUMMARY.md`
**Package under test:** `devflow` (binary-only) + `devflow-core` (lib)

## Objective

`devflow start --agent pi` completes Plan → Code → Validate → Ship in two stages: **Stage 1**
baseline (`Legacy`/`-p` + `litellm` provider fix + completion detection), **Stage 2** dispatch
(`@bacnh85/pi-subagent` user-scope, no drain gate).

## Open question to settle at execution (research, not guess)

The 37.1 research confirmed the `litellm` provider lives in `~/.pi/agent/models.json`
(`providers.litellm`, models `deepseek-v4-pro`/`deepseek-v4-flash`); `auth.json` is empty `{}`.
`PiDriver::health` hardcodes `--provider google` → `not_ready`. **Fix:** probe the configured
provider rather than `google` (read `models.json` or drop `--provider` and let Pi use its default).
Verify against the live binary before implementing.

## Waves

### Wave 1 — Stage 1 baseline
- **T-39-01** Fix `PiDriver::health`: stop hardcoding `--provider google`. Use the provider from
  Pi's own config (`models.json`) — or drop `--provider` so `pi auth check` probes its configured
  default. Keep the `--no-refresh` flag and the `classify_auth_check` readiness gate.
- **T-39-02** Add a regression test pinning Pi to `MonitorLaunch::Legacy` in `resolve_launch_shape`
  (Pi is never Claude, so `stream_launch` is false → Legacy). No production change expected.
- **T-39-03** Completion: confirm the generic `DEVFLOW_RESULT` marker path (`parse_devflow_result` /
  `evaluate_layer1`) already covers `-p` plain-text completion — a `parse_pi_result` is **not**
  needed for `@bacnh85` (in-process, plain-text marker). If any Pi-specific failure signal
  (rate-limit/`agent_end` error) needs handling, add it here, but do not build a JSON-event parser
  unless a live probe shows `-p` emits non-marker failure text.

### Wave 2 — Stage 2 dispatch arm
- **T-39-04** `@bacnh85/pi-subagent` integration: user-scope install (or vendor-and-pin) — no
  source change to DevFlow required (synchronous + process-exit + generic marker already cover it).
- **T-39-05** Trust-boundary confirmation (no code, an acceptance step): verify the installed
  extension fails closed headless (rejects project agents without UI) and that no child re-trusts
  project extensions. Record the evidence.
- **T-39-06** End-to-end smoke: one `pi -p --no-approve` run with the extension loaded, model
  delegates to a subagent, parent emits `DEVFLOW_RESULT` after it finishes. **Gated on the
  credential question** — must run against a profile with `models.json` (live, or throwaway with
  `models.json` copied), and NOT with `PI_OFFLINE=1`.

## Out of scope (recorded, not built)

- `CloseRule`/drain-gate / `PipeOwning` integration for Pi — unnecessary; synchronous in-process
  dispatch + `Legacy` covers it.
- Isolated-context (process-spawning) dispatch — follow-on (needs `--no-approve` child patch + a Pi
  drain predicate).
- 999.94; provider-agnostic auth beyond the `litellm` fix.

## Acceptance

- `cargo test -p devflow-core --lib` + `cargo test -p devflow --bin devflow` green; `clippy -D warnings` clean.
- `devflow start --agent pi --dry-run` shows the full pipeline for Pi.
- Stage 2 smoke: one live run proves subagent delegation completes under `Legacy` + `DEVFLOW_RESULT`
  (or is recorded as blocked on credentials with the precise reason).
- No `CloseRule`/`PipeOwning`/drain-gate claim added.
