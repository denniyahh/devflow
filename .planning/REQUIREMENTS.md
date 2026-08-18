# Requirements: DevFlow

**Defined:** 2026-08-18
**Core Value:** A developer should be able to run `devflow start --phase N` and walk away —
DevFlow must reliably drive the agent through the full pipeline and never silently corrupt its own
state or lose a human's gate decision, even under a mid-run crash or kill.

## v2.8.0 Requirements

Requirements for the **Remaining Harness Support + Pi Dogfood** milestone. Each maps to a roadmap
phase.

### Antigravity

- [ ] **ANTG-01**: Operator can select `--agent antigravity` — the `AgentKind` variant resolves
  through FromStr/Display, `driver_for`, and `agent_program`.
- [ ] **ANTG-02**: Antigravity driver launches headless (Claude-style `-p` + stream-json +
  skip-permissions) and passes the shared conformance suite.
- [ ] **ANTG-03**: Antigravity completion/verdict is parsed from the stream (or honest process-exit)
  — a marker-less run never advances a stage.

### Hermes

- [ ] **HRMS-01**: Operator can select `--agent hermes` — full `AgentKind` registration.
- [ ] **HRMS-02**: Hermes driver launches headless (`hermes -z "<prompt>" --yolo`) and passes the
  conformance suite.
- [ ] **HRMS-03**: Hermes completion is honest (process-exit + `DEVFLOW_RESULT` prompt contract); a
  marker-less run never advances a stage.

### OpenCode

- [ ] **OPCD-01**: OpenCode driver launches headless with `--auto` + `--format json`.
- [ ] **OPCD-02**: OpenCode completion/verdict is parsed from `--format json` events
  (regression-tested), modeled on Codex's `parse_codex_event_result`.
- [ ] **OPCD-03**: OpenCode has a fail-closed health check + capability discovery.

### Codex

- [ ] **CODE-01**: `--agent codex` verified end-to-end through a real phase (dogfood); surfaced gaps
  closed.

### Pi Dogfood

- [ ] **PIDG-01**: Phase 40 completes a real Define→Ship run through `--agent pi`, proving Pi driver
  reliability; the deferred isolated-context Pi dispatch item closed if it surfaces.

### Opportunistic (capacity-permitting)

- [ ] **DECN-01**: (999.94, HIGH) — an unattended `decision` checkpoint no longer takes the first
  option blindly.
- [ ] **MAINT-01**: (999.85, low) — the two stale code comments removed/corrected.

## Future Requirements

Deferred to a future milestone. Tracked but not in the current roadmap.

### Remaining backlog

- **999.9** — dependency-update review.
- **999.17 / 999.18** — mutation testing / property+fuzz testing for protocol parsers.
- **999.19 / 999.20** — fast/slow CI lanes / differential coverage enforcement.
- **999.21 / 999.22** — AI change-acceptance review wiring / refactor-equivalence CI guard.
- **999.26 / 999.28** — `devflow parallel` object-store race / explicit `--base` override.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Hermes messaging/gateway surface (chat, gateway, proxy, whatsapp/slack/send, cron, kanban) | v2.8.0 supports only the oneshot agent-run mode (`hermes -z`); the platform-integration surface is a different product scope |
| Antigravity GUI / `agy` GUI wrapper | headless CLI only — the `agy`→GUI alias was explicitly called out as broken in the Phase 19 review |
| Claude stream-json widening beyond Code (999.73) | already closed in v2.4.0; not re-opened |
| Hermes plugin (TUI watcher / status display) | the 2026-06-19 "Hermes Plugin" idea predates the modular driver; superseded by a plain driver |
| New launch family | any harness that needs a third launch shape is out of scope until one is genuinely required — all three targets map onto existing families |

## Traceability

Which phases cover which requirements. Filled during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| ANTG-01 | — | Pending |
| ANTG-02 | — | Pending |
| ANTG-03 | — | Pending |
| HRMS-01 | — | Pending |
| HRMS-02 | — | Pending |
| HRMS-03 | — | Pending |
| OPCD-01 | — | Pending |
| OPCD-02 | — | Pending |
| OPCD-03 | — | Pending |
| CODE-01 | — | Pending |
| PIDG-01 | — | Pending |
| DECN-01 | — | Pending |
| MAINT-01 | — | Pending |

**Coverage:**
- v2.8.0 requirements: 13 total
- Mapped to phases: 0 (pending roadmap)
- Unmapped: 13 ⚠️

---
*Requirements defined: 2026-08-18 after the v2.8.0 milestone (Remaining Harness Support + Pi
Dogfood) research pass — installed-CLI probes + source read, HIGH confidence.*
