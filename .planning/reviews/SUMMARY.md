# Adversarial Review — Phase 36 planning docs

**Targets:** `36-SPEC.md`, `36-CONTEXT.md`, `36-DISCUSSION-LOG.md` (worktree `feature/phase-36` @ `b86615b`)
**Reviewers:** claude (opus, high) · codex (gpt-5.6-sol, high) · antigravity (Gemini 3.1 Pro)
**Date:** 2026-08-15

All three reviewers ran the adversarial lens against the docs, and independently read the actual
tree (`develop` @ `a3a4871`, installed `pi` 0.84.1) to verify claims. Findings with line citations
are in the per-reviewer files. This summary groups them by consensus and severity.

## Consensus findings — settle these before planning

### BLOCKER — 999.67 is already shipped (claude + codex, both code-verified)
`parse_devflow_result` already normalizes *both* arms via `normalise_stream_marker_provenance`
(`agent_result.rs:166-180`), and the mirror regression `generic_marker_cannot_forge_layer0_provenance`
already exists at `agent_result.rs:4343` with the passing counterpart. The ROADMAP entry is stale
("shortlisted for Phase 31", fix described as pending). **999.67 is a no-op** — remove it from the
phase and close the backlog entry as already-fixed.

### BLOCKER — "Pi first, Code-stage only" does not survive the state machine (all three)
Three angles converge on the same collapse:
- **codex:** registering `AgentKind::Pi` exposes it to *all five stages* immediately
  (`pipeline_launch.rs:206`); there is no Code-stage gate. "Widening is purely additive" is false.
- **claude:** the Code-stage prompt still interpolates the literal `/gsd-execute-phase` string
  (`stage.rs:63` → `prompt.rs:274`) — a Claude Code plugin slash command. Pi cannot reach terminal
  completion until the prompt is de-Claude-ified, which the SPEC deferred to Phase 37. AC #1 is
  circular.
- **antigravity:** SPEC's "drive Pi end-to-end" contradicts D-01 (Code-only); the dogfood run that
  aborted would still abort after Phase 36 lands.

**The phase's shape is the thing to re-decide**, not the plan details. Options: (a) pull the
prompt/StageIntent de-Claude work forward into Phase 36, (b) shrink Phase 36 to "adapter plumbing +
health check" and strike AC #1, or (c) re-sequence 999.31 *before* Pi.

### BLOCKER — 999.104 AC #4 is impossible under the one-line probe (claude + codex + antigravity)
`check_signing_viability` (`git.rs:1163`) is capability-only: it answers "can this key sign", not
"is this the maintainer's key". Both keys on this machine are probeable, so a wrong-but-usable key
reports *viable* — the negative control is unobtainable from a repoint. The fingerprint check that
would produce NOT-viable (D-02) is (a) not in the SPEC's 999.104 scope (leaked in via CONTEXT), and
(b) tautological — it sources the expected fingerprint from the same `devflow.releaseSigningKey`
config it validates. Meeting AC #4 needs identity policy (an independently-pinned fingerprint),
which is the work D-03 deferred.

## HIGH — fix the docs

### 999.96's "ready-made positive fixture" no longer exists (claude + codex)
`Cargo.toml:9` and `CHANGELOG.md:3` are both `2.5.0` — the v2.5.0 cut consumed the skew the ROADMAP
entry measured on 2026-08-07. AC #3 as written is unfalsifiable. The check is still worth building;
it needs a synthetic mismatched fixture (plus missing/malformed/duplicate-heading cases), not "the
current tree".

### Pi transport (`-p` vs `--mode json`) is load-bearing, not a discretion (all three)
- `--mode json` breaks `parse_marker_lines` (`agent_result.rs:1590`), which requires `DEVFLOW_RESULT:`
  at line start; Pi JSON mode emits delta-only, JSON-escaped text (`message_update`) with the
  authoritative text inside `message_end`/`agent_end`.
- Non-Claude agents route to `MonitorLaunch::Legacy` (`pipeline_launch.rs:198-208`), so `CloseRule`
  — which CONTEXT points the planner at — never runs for Pi. The "Pi-aware drain-gate arm" as
  written does not exist; a Pi event unwrapper + explicit transport decision are required, and that
  is *not* the XS one-liner CONTEXT D-05 implies.

### SPEC/CONTEXT authority contradiction (codex, echoed by claude)
SPEC frontmatter still reads `status: draft (spec-phase, pre-discuss)` with four live "Open
decisions", while CONTEXT declares "Ready for planning" and "4 requirements locked". A planner
obeying CONTEXT's "MUST read 36-SPEC.md" reads superseded 999.104 scope and settled questions
presented as open.

## Single-reviewer findings

- **Pi provider/model surface (claude):** `pi --help` shows `--provider` defaults to `google`.
  `AgentAdapter::exec_command` has no model/provider surface; with no `--provider`/`--model`,
  DevFlow drives Gemini via `GEMINI_API_KEY` — or fails with no adapter-level diagnosis. SPEC §A
  requirement 3 ("can execute headless") for Pi means "has a working provider credential", which has
  no analogue in the three adapters it says to mirror.
- **Pi trust + sandbox (antigravity, claude):** DevFlow creates a fresh worktree path per phase, so
  `~/.pi/agent/trust.json` never matches and `defaultProjectTrust: "ask"` silently drops project
  resources on every run — `--approve` is load-bearing, not discretionary. Pi has no built-in
  sandbox (`docs/security.md:33`), so `extra_writable_roots` is ignored by construction.

## Recommended decisions before plan-phase

1. **Drop 999.67** (already shipped) and mark its backlog entry resolved.
2. **Re-decide the phase shape** (the blocker above) — this changes the plan *set*, not a plan.
3. **999.104:** either accept capability-only and rewrite AC #4, or pull the identity-policy
   (fingerprint) work into scope and reopen D-03.
4. **999.96:** synthetic fixture, not "current tree".
5. **Lock the Pi transport** (`--mode json` + a Pi event unwrapper + golden fixtures) — remove it
   from "agent's discretion".
6. **Reconcile SPEC ↔ CONTEXT authority** so the planner reads one consistent, settled scope.
