# Phase 36 — Discussion Log

**Phase:** 36 — Pi Agent Support + Release-Preflight Hardening
**Date:** 2026-08-15
**Mode:** manual discuss-phase (Pi driving; devflow dogfood aborted — devflow hardcodes Claude)

## Session Summary

The SPEC (`36-SPEC.md`) was drafted ahead of discussion, so requirements were locked and the
discussion focused on implementation decisions only. Three gray areas were presented with
recommendations; the operator accepted all three ("your rec").

## Areas Discussed

### 1. Pi stage coverage
- **Options presented:** Code-stage vertical slice first vs. all five stages.
- **Decision:** Code-stage first.
- **Rationale:** proves transport + completion parsing end-to-end before widening; the other four
  stages are the same prompt-wrapping shape and are cheap once Code works.

### 2. 999.104 release-signing key
- **Options presented:** (a) one-line probe only; (b) one-line probe + preflight fingerprint
  check (decision 3); (c) two-key-model rework.
- **Decision:** (b) — one-line probe + surface the fingerprint check at preflight.
- **Rationale:** the probe fixes detection; the preflight fingerprint check turns "silent until
  push" into "fails at preflight." The two-key-model rework is deferred to a follow-up backlog
  entry (it is a workflow redesign, not a bug fix).

### 3. Pi interface sourcing
- **Options presented:** pull Pi's docs now and lock the interface in CONTEXT vs. defer to
  plan-phase research.
- **Decision:** established from Pi docs (v0.84.1) and recorded in CONTEXT (`--mode json`,
  `agent_end` completion, project-trust behavior).
- **Note:** exact flag selection and exit-code semantics left to plan-phase verification against
  the installed `pi` binary.

### 4. 999.67 + 999.96 bundling
- **Decision:** included (operator confirmed earlier; re-confirmed).
- **Rationale:** XS + S, each shares a file with the phase's main deliverables (agent result
  parsing; `release --check`).

## Deferred Ideas
- 999.31 (Modular `AgentDriver` architecture) → Phase 37.
- 999.94 (unattended `decision` checkpoint first-option) → Phase 37, pencilled.
- 999.104 decision 2 (two-key-model rework) → follow-up backlog entry.
- 999.101 (upstream Claude Code task-notification aborts) → observation for Phase 37 driver contract.

## Agent Discretion
- Pi adapter `name()` string.
- `-p` vs `--mode json` transport for the first cut (pending plan-phase exit-code verification).

## Process Notes
- devflow dogfood run (`devflow start --phase 36 --mode supervise --until plan`) was aborted:
  devflow's Define/Plan stages hardcode the Claude launch, and Phase 36 is precisely the phase
  that removes that limitation.
- Worktree: `.worktrees/phase-36` on `feature/phase-36` (branched from `develop` at `a3a4871`).
