# Requirements: DevFlow

**Defined:** 2026-08-18
**Core Value:** A developer should be able to run `devflow start --phase N` and walk away —
DevFlow must reliably drive the agent through the full pipeline and never silently corrupt its own
state or lose a human's gate decision, even under a mid-run crash or kill.

## v2.8.0 Requirements

Requirements for the **Remaining Harness Support + Pi Dogfood** milestone. Each maps to a roadmap
phase.

### Antigravity

- [x] **ANTG-01**: Operator can select `--agent antigravity` — the `AgentKind` variant resolves
  through FromStr/Display, `driver_for`, and `agent_program`.

- [x] **ANTG-02**: Antigravity driver launches headless (Claude-style `-p` + stream-json +
  skip-permissions) and passes the shared conformance suite.

- [x] **ANTG-03**: Antigravity completion/verdict is parsed from the stream (or honest process-exit)
  — a marker-less run never advances a stage.

### Dogfood Hygiene (Phase 41)

- [x] **HYG-01**: The Phase-7 integration tests reap their own `devflow start` monitors — a full
  `cargo test` run leaves 0 detached monitor processes (the Phase 40 dogfood leaked 43).

- [x] **HYG-02**: `check-in-container.sh` passes under root (uid 0) in the pinned container — the 3
  git-env tests that fail as root are fixed (unrelated to the code under test).

### Antigravity Dogfood + Cadence (Phase 42)

- [x] **ANTG-04**: Antigravity is dogfooded through a real supervised phase run
  (`devflow start --agent antigravity --phase N --mode supervise`), which unlocks `--mode auto`
  (C2 preflight gate). During the run, event cadence is measured: the real quiet-gap distribution
  is compared against the 120s idle-timeout default (`idle_timeout_setting_for` /
  `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`), and the default is raised if organic thinking gaps
  approach or exceed it (Phase 35.1 precedent: one 120s gap killed a healthy run). Any >5m quiet
  gap that survives confirms the `--print-timeout 60m` override holds end-to-end (closing
  41-UAT.md test 12's deferred negative control).

### Hermes

- [ ] **HRMS-01**: Operator can select `--agent hermes` — full `AgentKind` registration.
- [ ] **HRMS-02**: Hermes driver launches headless (`hermes -z "<prompt>" --yolo`) and passes the
  conformance suite.

- [ ] **HRMS-03**: Hermes completion is honest (process-exit + `DEVFLOW_RESULT` prompt contract); a
  marker-less run never advances a stage.

### OpenCode

- [x] **OPCD-01**: OpenCode driver launches headless with `--auto` + `--format json`.
- [x] **OPCD-02**: OpenCode completion/verdict is parsed from `--format json` events
  (regression-tested), modeled on Codex's `parse_codex_event_result`.

- [x] **OPCD-03**: OpenCode has a fail-closed health check + capability discovery.

### Codex

- [ ] **CODE-01**: `--agent codex` verified end-to-end through a real phase (dogfood); surfaced gaps
  closed.

### Pi Dogfood

- [x] **PIDG-01**: Phase 40 completes a real supervised Define→Validate run through `--agent pi`
  (at least one live gate), proving Pi driver reliability; the deferred isolated-context Pi dispatch
  item is re-filed (not built this phase).

### Unattended Auto-Mode (Phase 45)

- [x] **AUTO-01**: (999.110, HIGH) — worktree creation forks from the branch tracking `.planning/`
  rather than hardcoding `develop`, so `.planning/config.json` is present and
  `preflight_unattended_launch_check` passes out of the box.
  Verified at unit/integration level (fork point with a real negative control + 4 run-scoped
  consumers). The live `devflow start --mode auto` end-to-end check is deferred to a later phase,
  tracked as backlog **999.119** (accepted as a `PASSED (override)` on 45-VERIFICATION.md,
  2026-09-02).

- [x] **AUTO-02**: (999.109, HIGH) — the self-dogfood staleness check (`affects_compiled_binary`)
  inspects only Cargo workspace members (`crates/*`) plus root build files, ignoring
  `.planning/spikes/` and non-workspace crates.

### Opportunistic (capacity-permitting)

- [ ] **DECN-01**: (999.94, HIGH) — an unattended `decision` checkpoint no longer takes the first
  option blindly.

- [x] **MAINT-01**: (999.85, low) — the two stale code comments removed/corrected.

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
| ANTG-01 | Phase 41 | Complete |
| ANTG-02 | Phase 41 | Complete |
| ANTG-03 | Phase 41 | Complete |
| HYG-01 | Phase 41 | Complete |
| HYG-02 | Phase 41 | Complete |
| ANTG-04 | Phase 42 | Complete |
| HRMS-01 | Phase 42 | Pending |
| HRMS-02 | Phase 42 | Pending |
| HRMS-03 | Phase 42 | Pending |
| OPCD-01 | Phase 43 | Complete |
| OPCD-02 | Phase 43 | Complete |
| OPCD-03 | Phase 43 | Complete |
| CODE-01 | Phase 44 | Pending |
| PIDG-01 | Phase 40 | Complete |
| AUTO-01 | Phase 45 | Complete (live e2e deferred → 999.119) |
| AUTO-02 | Phase 45 | Complete |
| DECN-01 | Phase 45 | Pending |
| MAINT-01 | Phase 40 | Complete |

**Coverage:**

- v2.8.0 requirements: 18 total
- Mapped to phases: 18
- Unmapped: 0 ✓

---
*Requirements defined: 2026-08-18 after the v2.8.0 milestone (Remaining Harness Support + Pi
Dogfood) research pass — installed-CLI probes + source read, HIGH confidence.*
