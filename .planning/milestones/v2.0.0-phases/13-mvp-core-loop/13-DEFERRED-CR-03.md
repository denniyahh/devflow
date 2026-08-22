---
phase: 13-mvp-core-loop
identified: 2026-07-15
source: post-fix code review of the CR-03 lock rescoping (commits 962e931..cbe3478)
severity: critical (design)
status: deferred
deferred_to: 14
---

# Deferred: CR-03 per-phase locks sit on project-global state

## One-line summary

Phase-scoped locks (CR-03) promise that `devflow parallel` sibling phases can
advance independently, but the resources those advances mutate are still
project-global — the single `.devflow/state.json` and the main checkout's git
history — so concurrent phases are unsafe by construction, not by locking.

## The flaw

CR-03 (13-REVIEW.md) rescoped the advance lock from `.devflow/lock` to
`.devflow/lock-{phase:02}` so one phase blocking at a multi-day Ship gate does
not starve its siblings. The rescoping is correct as far as locks go, but it
removed the only mutual exclusion that was (accidentally) protecting two
resources that remained project-global:

1. **`state.json` is a per-project singleton** (`workflow::state_path` =
   `project_root/.devflow/state.json`, no phase component). Every `start`
   overwrites it; every monitor runs `devflow advance {project_root}` and
   loads whatever phase was started *last*. Under `devflow parallel`, phase
   A's monitor can load phase B's state, key the lock on B, and evaluate
   A's exit files against B's stage machine.

2. **`finish_workflow` mutates the shared main checkout** — the version-bump
   commit/tag (`VersionBump`) and `cleanup_merged` (`BranchCleanup`) run
   against the primary checkout. Two phases finishing concurrently race git
   `index.lock`/`HEAD` with no exclusion; under the old project lock the
   second advance failed fast with `Contended` instead.

Verified against the code 2026-07-15 (post-fix review, verifier run):
`workflow.rs` state path, `main.rs` `parallel` → `start` loop,
`monitor.rs` advance invocation, `hooks.rs` after-ship hooks.

## What is already mitigated (shipped in the post-fix pass)

- `advance()` re-loads state **under** the phase lock and bails if the phase
  changed (`fix(cli): re-load state under the phase lock in advance`) —
  closes the same-phase double-advance TOCTOU.
- `recover --clean` no longer deletes live phases' locks
  (`lock::remove_stale_locks` skips live holders).
- These reduce blast radius; they do **not** make cross-phase parallelism safe.

## Failure scenarios still open

- `devflow parallel 13 14`: the second `start` clobbers the first's
  state.json; the first phase's monitor advances the *second* phase's stage
  machine when its agent exits. Wrong-phase evaluation, wrong-phase
  transition, duplicate or lost agent runs.
- Phase A approved at its Ship gate while phase B finishes: both run
  `finish_workflow` git operations on the same checkout concurrently —
  interleaved version-bump commits/tags, racing `cleanup_merged`.
- `clear_state` on either phase's finish erases the other's in-flight state.

## Recommended fix shape (for Phase 14 discussion)

1. **Per-phase state files**: `state-{phase:02}.json` (mirroring the lock
   naming), with `workflow::load_state/save_state/clear_state` taking the
   phase, and a listing helper for `status`/`recover` to enumerate active
   phases. Keep a one-shot migration read of legacy `state.json`.
2. **Thread the phase through the monitor**: `devflow advance {root} --phase N`
   recorded at spawn time, so advance's identity never depends on a shared
   singleton (this also removes the pre-lock state read entirely).
3. **Short project-wide lock for main-checkout mutations only**: wrap
   `finish_workflow`'s git-op section (and any other primary-checkout
   mutation) in a second, coarse lock held for seconds, not gate-days —
   two-level locking instead of choosing one scope for both problems.
4. Re-check `sequentagent` (takes no lock at all today) and
   `cron-instructions.json` (also a project-global single slot) against the
   same per-phase model while in there.

## Acceptance criteria

- Two phases started via `devflow parallel` can each run start→advance→gate
  with no shared-file clobbering (integration test: interleaved fake agents,
  assert both stage machines advance independently).
- Concurrent `finish_workflow`s serialize on the coarse lock (test: second
  finisher blocks or retries; git history contains both bumps, uncorrupted).
- `devflow status`/`recover` enumerate all active phases, not just the last
  one started.
