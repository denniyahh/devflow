---
status: complete
phase: 37-modular-agent-driver-architecture-pi-driver-999-31-pi
source: 37-01-SUMMARY.md, 37-02-SUMMARY.md, 37-03-SUMMARY.md, 37-04-SUMMARY.md
started: 2026-08-16T19:20:00Z
updated: 2026-08-16T19:25:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Claude zero-regression (D-01 — top priority)
expected: Claude's prompts and launch argv are byte-identical to pre-migration; `devflow start --agent claude` behaves the same
result: pass
evidence: `drivers_reproduce_legacy_adapter_behavior` + `claude_and_opencode_stay_identical_but_codex_renders_native` assert
`render_prompt` byte-equality and the `stream-json` argv; `cargo test -p devflow --bin devflow` 322 passed

### 2. Codex/Pi no longer receive `/gsd-*` slash commands (the dogfood fix)
expected: no stage renders a GSD slash command to Codex or Pi
result: pass
evidence: `codex_and_pi_drivers_reproduce_legacy_behavior` + the precise negative control over the seven slash-command names

### 3. Per-stage contracts preserved for native agents
expected: Validate demands `verdict`, Ship enforces the review gate, Define is a no-op, Plan is idempotent
result: pass
evidence: `workflow_render_preserves_stage_contracts` (pins all four contracts + the Pi workflow root)

### 4. All four agents resolve through the `AgentDriver` contract
expected: `adapter_for` routes Claude/Codex/OpenCode/Pi through their drivers
result: pass
evidence: `adapter_for_returns_correct_names` + `drivers_reproduce_legacy_adapter_behavior` + `codex_and_pi_drivers_reproduce_legacy_behavior`

### 5. Conformance suite is real (not vacuous)
expected: every driver passes `test_contract()` AND a deliberately-broken driver fails it
result: pass
evidence: `every_driver_passes_the_conformance_suite` + `conformance_suite_fails_a_broken_driver`

## Summary

total: 5
passed: 5
issues: 0
pending: 0
skipped: 0

## Gaps

(none)

## Out of scope (deferred, recorded)

- Pi end-to-end `devflow start --agent pi` (JSON unwrapper + `CloseRule`) → 37.1/38 — NOT UAT-able this phase.
- `AgentAdapter` removal + `InteractivityMode` consumption → 999.106.
- Codex parser success-before-failure + writable-root serialization → 999.107 (pre-existing).
