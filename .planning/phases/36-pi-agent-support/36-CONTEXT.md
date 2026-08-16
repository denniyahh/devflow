# Phase 36: Pi Adapter Registration + Release Signing - Context

**Gathered:** 2026-08-15
**Status:** Ready for planning (SPEC locked — post adversarial review)

## Phase Boundary

DevFlow registers **Pi** as a fourth, selectable agent adapter (`AgentKind::Pi` + `PiAgent`) and
gains a preflight health check that distinguishes "Pi installed" from "Pi can execute headless".
This phase does **not** deliver an end-to-end `devflow start --agent pi` run — the Code-stage
prompt is still Claude-specific and non-Claude agents still route through the legacy launch path,
so making Pi *work* is Phase 37. Two release-path items close here: 999.96 (version-bump row) and
999.104 (deterministic signing key).

## Requirements (locked via SPEC.md)

**3 requirements locked** (was 4 — 999.67 was dropped). See `36-SPEC.md` for full requirements,
boundaries, and acceptance criteria; downstream agents MUST read it before planning.

**In scope:** Pi adapter registration + health check; 999.96 (synthetic fixture); 999.104
(deterministic release signing key + remove fragile checks).

**Out of scope:** end-to-end Pi run (Phase 37); `AgentDriver`/`StageIntent` (Phase 37); Pi
JSON-mode unwrapper + monitor integration (Phase 37); 999.67 (already shipped).

## Implementation Decisions

### Pi scope (corrected by review)
- **D-01:** Adapter registration + health check only — **not** an end-to-end run. The prior
  "Code-stage vertical slice" framing was wrong: registering `AgentKind::Pi` exposes it to all
  five stages immediately (`pipeline_launch.rs:206` — no Code-stage gate), and the Code-stage
  prompt still interpolates the literal `/gsd-execute-phase` Claude slash command
  (`stage.rs:63` → `prompt.rs:274`). So "Pi works" is Phase 37, gated on the prompt
  de-Claude-ification.

### Release-signing key (999.104) — resolves the two-key question
- **D-02:** The release/tag path signs **deterministically** with `devflow.releaseSigningKey`
  via an in-code `git -c user.signingkey=` override; the `release --check` signing-viability
  probe and the pre-push fingerprint hook are **removed**. The probe was capability-only
  (`git.rs:1163` — "can this key sign", not "is this the maintainer's key"), and the fingerprint
  hook was tautological (sourced the expected value from the config it validated). A missing
  `devflow.releaseSigningKey` fails loudly at release time. — **Reversibility:** one-way — this
  removes enforcement surface rather than relocating it; undo means re-introducing a check, not a
  config edit.

### Pi interface (from Pi docs v0.84.1)
- **D-03:** `pi -p "<prompt>"` is the **locked transport for Phase 36** (print mode; terminates
  on process exit, needs no unwrapper). The JSON-mode (`--mode json`) transport and its event
  unwrapper are Phase 37, because they couple to the monitor's drain gate. Flags: `--model`
  (Pi defaults provider `google` via `GEMINI_API_KEY`, so `--provider`/`--model` must be wired),
  `--approve` (load-bearing: DevFlow creates a fresh worktree path per phase, so
  `defaultProjectTrust: "ask"` always applies and silently drops project resources). Pi has no
  built-in sandbox, so `extra_writable_roots` is ignored by construction.

### Bundling
- **D-04:** 999.96 stays — but with a **synthetic** fixture, not "the current tree" (the v2.5.0
  cut consumed that skew). 999.67 is **dropped** — `parse_devflow_result` already normalizes both
  arms (`agent_result.rs:166-180`) with the mirror test at `:4343`.

## Canonical References

- `36-SPEC.md` — locked requirements, boundaries, acceptance criteria. MUST read first.
- `.planning/phases/999.31-agent-driver-modularization/CONTEXT.md` — the Phase 37 target.
- `docs/guides/adding-agent.md`, `docs/architecture/agent-model.md` — agent model (the latter
  encodes the shared-prompt assumption Phase 37 removes).
- Pi `docs/usage.md` § CLI Reference / Modes, `docs/json.md`, `docs/security.md` — invocation,
  JSON events, sandbox-absence.
- `.planning/reviews/SUMMARY.md` — the adversarial review that reshaped this phase.

## Existing Code Insights

### Reusable Assets
- `crates/devflow-core/src/agents/{claude,codex,opencode}.rs` — adapters to mirror (a small
  struct: `name`, `exec_command`, `extra_env`, `completion_signal_detected`, `preflight`).
- `crates/devflow-core/src/agent_result.rs:166,1590` — `parse_devflow_result` /
  `parse_marker_lines` (the marker parser; 999.67 already handled here).

### Established Patterns
- Adapters only format the shared stage prompt into CLI flags. For `-p` mode, completion is
  process exit + the `DEVFLOW_RESULT` marker in stdout — no event unwrapper needed.

### Integration Points (corrected by review)
- `crates/devflow-core/src/state.rs:387` — `AgentKind` enum; add `Pi`.
- `crates/devflow-core/src/agents/mod.rs` — `adapter_for(kind)`; add the `Pi` arm + `pub mod pi`.
- `crates/devflow-cli/src/commands.rs` — `release --check` (999.96 row; 999.104 probe removal).
- `scripts/hooks/pre-push` — the fingerprint comparison to remove (999.104).
- `crates/devflow-core/src/git.rs:1099` — `check_signing_viability` to remove (999.104).
- **Not** `monitor.rs` `CloseRule`: non-Claude agents route to `MonitorLaunch::Legacy`
  (`pipeline_launch.rs:198-208`), so `CloseRule` never runs for Pi in this phase. That wiring is
  Phase 37.
