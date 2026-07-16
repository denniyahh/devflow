---
phase: 14-parallel-safety-observability
reviewed: 2026-07-16T00:00:00Z
depth: high
method: 8 finder angles (3 correctness, reuse, simplification, efficiency, altitude, conventions) → dedup → 1-vote verify per candidate
files_reviewed: 12
files_reviewed_list:
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/events.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/lock.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/recover.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/workflow.rs
findings:
  critical: 2
  warning: 7
  info: 1
  cleanup_notes: 4
  refuted: 1
  total: 10
status: issues_found
---

# Phase 14: Code Review Report

**Scope:** `origin/develop...HEAD` — the Phase 14 implementation (per-phase
state, two-level locking, sequentagent-behind-monitor, events.jsonl, logs,
multi-phase status/recover).
**Status:** issues_found

## Summary

The core CR-03 mechanics survived scrutiny across multiple independent
angles: per-phase state/lock keying, lock ordering (phase → project,
never inverted; no deadlock), the sequentagent exit-file handoff (guarded by
`cleanup_phase_files`), legacy-state migration precedence, and
`events::emit` envelope-key protection were all checked and cleared. What
the review caught is that the **operational tooling around the new
multi-phase world didn't fully keep up**: `recover --clean` became a footgun,
the checkout-lock timeout fallback undoes its own guarantee, and several
sequentagent behaviors silently regressed when it moved behind the monitor.

## Findings

### 14-CR-01 [CRITICAL] `recover --clean` wipes live sibling phases — CONFIRMED

`recover.rs::clean()` iterates `list_states` and `clear_state()`s every
phase with no staleness/liveness check and no `--phase` selectivity; worse,
`recover_cmd` prints "run `devflow recover --clean`" whenever ANY phase is
stale. Under `devflow parallel` an operator cleaning a dead phase 7 also
deletes running phase 8's state; phase 8's monitor later hits MissingState
and the live pipeline is silently orphaned. Pre-14 this was moot (one state
slot); the diff made indiscriminate clearing newly dangerous.
**Fix shape:** clean only stale phases; add `recover --phase N`.

### 14-CR-02 [CRITICAL] Fail-soft checkout lock proceeds unserialized — CONFIRMED

`main.rs::run_checkout_hooks` swallows the 120s
`acquire_project_blocking` timeout, prints a warning, and runs the
git-mutating hooks anyway — the exact index.lock/HEAD race the coarse lock
(13-DEFERRED-CR-03 fix shape #3) exists to prevent. Constructible: a
sibling's `integrate_agent_branch` holds the lock across a wedged
`git push`; this phase's hooks then race it, and per-hook fail-softness
turns a lost race into a swallowed warning while the stage still advances.
The other two lock sites (integrate, ensure_branch) correctly fail hard.
**Fix shape:** on timeout, SKIP the hook batch (never run unserialized),
warn loudly, emit skip events; liveness preserved since hooks are already
fail-soft individually.

### 14-CR-03 [WARNING] `logs --follow` misses stage rollover — PLAUSIBLE

The follower's byte offset never resets when `launch_stage` →
`cleanup_phase_files` deletes and the next stage's monitor recreates
`phase-NN-stdout`. Seeking past EOF legally reads 0 bytes, so the next
stage's output below the stale offset is silently skipped, and termination
misfires while the exit file is briefly absent. Window-dependent (the loop
usually terminates at the first agent's exit before cleanup wins the 500ms
race) but unguarded.
**Fix shape:** stat before seek — `len < offset` ⇒ rollover ⇒ reset to 0.

### 14-CR-04 [WARNING] Corrupt legacy `state.json` survives `recover --clean` forever — CONFIRMED

`list_states` only matches the `state-` prefix, `migrate_legacy_state`
deliberately leaves unparsable legacy files, and `clear_state`'s legacy
branch requires a successful parse — so the designated reset tool can never
remove a truncated pre-14a `state.json`, and every future load warns about
it. Regression vs the old unconditional `remove_file(state.json)`.
**Fix shape:** `clean()` explicitly deletes an unparsable legacy state file
(operator-driven reset is the one sanctioned place).

### 14-CR-05 [WARNING] Missing-binary diagnosis lost for sequentagent — CONFIRMED

The deleted sync `launch_agent` mapped `ErrorKind::NotFound` to
"agent binary `X` not found — is it installed?". The monitor path has `sh`
exec the missing program → exit 127 → generic "agent exited with code 127
without reporting a result", after worktrees were already created. (The
`start` pipeline never had the mapping — regression is sequentagent-only.)
**Fix shape:** pre-flight the agent binary on PATH before spawning any
monitor (both `launch_stage` and `run_agent_blocking`).

### 14-CR-06 [WARNING] Bare `advance` stalls silently when 2+ phases active — PLAUSIBLE

A monitor spawned by a pre-14a binary (or old automation) calls
`advance <root>` with no `--phase`; `resolve_sole_active_phase` hard-errors
on ambiguity and the monitor's stdout/stderr go to /dev/null — the phase
stalls invisibly. Bounded (one straddling agent run per upgrade) and
erring beats guessing; `recover` does surface the stall after the fact.
**Fix shape:** emit an events.jsonl line on this error path so the stall is
at least observable; keep the hard error.

### 14-CR-07 [WARNING] Partial integration when sequentagent hits lock timeout — PLAUSIBLE

`integrate_agent_branch` hard-fails on checkout-lock timeout; if it fails
for agent B after agent A already fast-forwarded into the base, sequentagent
aborts in a partial state with no rollback, and `--force` re-runs agent A on
top of its own prior output. The fail-hard choice itself is defensible (a
real shared-ref mutation must not run unlocked).
**Fix shape (mitigation):** resume guidance in the error message; full
rollback out of scope.

### 14-CR-08 [WARNING] Messages still print retired `.devflow/cron-instructions.json` — CONFIRMED

Writes moved to per-phase `cron-instructions-NN.json`, but the sequentagent
zero-commit pause message and `write_rate_limit_cron` still print the legacy
path; the phase7_cli assertion on the legacy path now passes vacuously.
**Fix shape:** print `ship::cron_instructions_path(root, phase)`; fix test.

### 14-CR-09 [WARNING] sequentagent runs silent for 10–30 min — CONFIRMED

The sync path's `Stdio::inherit()` streamed agent stderr live; the monitor
captures it to `phase-NN-stderr.log`, so an interactive run shows nothing
until exit. The designed replacement (`devflow logs -f --stderr`, added in
this same diff) is never mentioned to the operator.
**Fix shape:** print a "watch live: devflow logs -f …" hint at launch.

### 14-CR-10 [INFO] events.jsonl scanned whole-file per phase, no rotation — PLAUSIBLE

`last_event_for_phase` reads/parses the entire log per call; `status()`
calls it once per active phase (O(phases × file size)); nothing rotates or
caps the append-only file. Modest today, pure waste structurally, and Phase
16's plugin will tail this file.
**Fix shape:** one-pass `last_events_by_phase`; note rotation for Phase 16.

## Cleanup notes (verified, below severity cap)

- **Duplicated `.devflow` scanners** — `workflow::list_states`,
  `ship::list_cron_instructions`, `ship::delete_all_cron_instructions` each
  hand-roll read_dir+prefix loops; the differing cron prefixes
  ("cron-instructions" vs "cron-instructions-") are each correct in context
  but invite drift. Deferred: shared scanner helper.
- **`default_logs_phase` duplicates `resolve_sole_active_phase`** — same
  list/match/error construction, byte-identical ambiguity message. Fix in
  cleanup batch.
- **Dead `agent_label`** — no production caller after the sync-path
  deletion; only its own unit test references it. Fix in cleanup batch.
- **`unix_now` triplication** — events.rs joins gates.rs and state.rs with a
  third private unix-seconds helper. Deferred: consolidate in one module.

## Refuted candidate (kept for the record)

- **Legacy cron `unwrap_or(true)` cross-phase delete** — mechanically true
  (any phase's `delete_cron_instructions` removes an *unparsable* legacy
  file), but a corrupt record is unloadable by every phase including its
  own (`load_cron_instructions` errors, `list` skips it), so deletion is
  desired dead-file cleanup, not a loss of a recoverable record. REFUTED.
