---
phase: 14-parallel-safety-observability
fixed: 2026-07-16T00:00:00Z
source: 14-REVIEW.md
outcomes:
  fixed: 7
  mitigated: 2
  accepted: 1
validation: cargo test --workspace 260 passed / 0 failed; clippy -D warnings clean; fmt --check clean; live two-phase e2e re-run green (both phases finish, both tags land, watch-live hints shown, recover --clean idle no-op, missing-binary start fails fast with no scaffolding)
---

# Phase 14: Review Fix Record

Each finding from `14-REVIEW.md`, its outcome, and the commit that carries it
(one commit per finding or tightly-coupled pair; each fix has a regression
test).

| Finding | Outcome | Commit | Notes |
|---|---|---|---|
| 14-CR-01 recover --clean wipes live phases | **fixed** | 49859fd | clean() sweeps only stale phases (live agent or fresh state → kept + warning naming the escape hatch); new `recover --clean --phase N` clears one phase unconditionally. Fail→pass verified against the old indiscriminate body. |
| 14-CR-02 fail-soft checkout lock | **fixed** | a4a9f54 | Lock timeout now SKIPS the hook batch (never mutates the checkout unserialized) with a loud warning + `checkout_lock_timeout` and per-hook skipped events. Timeout env-tunable: `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` (default 120s). |
| 14-CR-03 logs --follow stage rollover | **fixed** | 07272ad | Stat-before-read: file shorter than offset ⇒ rollover ⇒ restart from 0 with a visible notice; rollover polls never count as quiescent. |
| 14-CR-04 corrupt legacy state survives clean | **fixed** | 49859fd | `recover --clean` (the operator reset) explicitly deletes an unparsable legacy `state.json` via `workflow::remove_corrupt_legacy_state`. |
| 14-CR-05 missing-binary diagnosis lost | **fixed** | 840064c + closeout commit | `ensure_agent_binary` PATH-preflight — fail-fast "is it installed? (run `devflow doctor`)". Checked at the top of `start` and `sequentagent` (before ANY branch/worktree scaffolding; sequentagent checks BOTH agents so B's absence can't surface after A's full run) and re-checked in `launch_stage`/`run_agent_blocking` for advance-time launches. Covers the start pipeline too (which never had the diagnosis). |
| 14-CR-06 bare-advance silent stall | **mitigated** | e3c90b2 | `advance_failed` event (phase-0 sentinel) recorded before the hard error, so the stall is observable in events.jsonl. Hard error kept — guessing a phase would mis-advance the wrong stage machine. |
| 14-CR-07 partial integration on lock timeout | **mitigated** | e3c90b2 | Lock-timeout error now carries resume guidance (manual fast-forward; warns against `--force`, which re-runs agents on integrated work). Fail-hard kept by design; full rollback out of scope. |
| 14-CR-08 stale cron filename in messages | **fixed** | 840064c | Messages print the per-phase `cron-instructions-NN.json` actually written; the vacuous phase7_cli assertion now checks the real filename and that the legacy file is never created. |
| 14-CR-09 sequentagent silent for the run | **fixed** | 840064c | "watch live: devflow logs -f --phase N [--stderr]" hint printed at launch (start prints one too). |
| 14-CR-10 events.jsonl O(phases × size) scans | **fixed** | e3c90b2 | `events::last_events_by_phase` — one read+parse pass; `status` consumes it once; `last_event_for_phase` shares the pass. Log rotation remains a Phase 16 note. |

## Cleanup notes from the review

- `single_active_phase` now owns the active-phase ambiguity rule
  (advance fallback + logs default) — **fixed**, e3c90b2.
- Dead `agent_label` + its test removed — **fixed**, e3c90b2.
- Dead `delete_all_cron_instructions` removed (lost its only caller to the
  14-CR-01 fix) — **fixed**, 49859fd.
- `.devflow` scanner drift (list_states / list_cron_instructions) and
  `unix_now` triplication — **accepted/deferred**: cross-module consolidation
  not worth the churn now; noted for a future cleanup pass.

## Refuted (no action)

- Legacy cron `unwrap_or(true)` delete — safe dead-file cleanup; see
  14-REVIEW.md for the refutation.

## Incidental hardening found while fixing

- Deflaked the parallel integration test's pid-file reads (2116e35): each
  stage transition briefly deletes the pid file, so one-shot reads raced a
  healthy pipeline.
- Widened the SIGTERM monitor test's kill-detection window 2s → 5s
  (840064c): flaked under a fully parallel workspace run.
