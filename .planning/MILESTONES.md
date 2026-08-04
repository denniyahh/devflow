# Milestones

## v2.3.0 the unattended run (Shipped: 2026-08-04)

**Phases completed:** 2 phases (30-31), 10 plans

**Key accomplishments:**

- A Claude `--output-format stream-json` JSONL capture now produces a real Layer-1 verdict through `evaluate_layer1` — last-result-wins across turns, with `decided_by_layer` derived rather than trusted — while both capture shapes that ship today are proven untouched by runtime assertion.
- `task-notification` delivery survives DevFlow's production launch environment — 8 trials, 3 environments, 0 refutations — so Phase 31 could be planned; but 8 successes bound reliability only above ~69%, so it was planned with a retry/timeout path, not a happy path.
- Rate-limit classification, `is_error` failure attribution and `session_id` now survive a Claude `stream-json` capture with the single-document path's precedence and read discipline intact.
- The 0.38s exit figure was refuted by direct measurement (169.5–279.7 ms, median 242.0 ms), closing stdin with pending tasks turned out to be benign rather than undefined, and the ~12s idle-timeout floor was raised to ≥30s after 2 of 7 trials showed a quiet gap above it.
- A Claude stage now launches with the bidirectional `stream-json` argv and is supervised by a std-only Rust monitor that owns both pipes, releasing stdin only when a `DEVFLOW_RESULT` marker has landed in a top-level `result` AND the background-task list has drained — making Phase 30's stream parser reachable from a production launch path for the first time.
- A nonce-canary guard now distinguishes "the behaviour is gone" from "the guard could not run," and a stream-vs-exit-code arbitration layer resolves disagreements between the two signals, with `--legacy-claude-launch` as a documented, off-by-default escape hatch.
- Live acceptance run **passed on attempt 1** (D-18: both plans produced a SUMMARY.md and both merged), independently verified from git — two merge commits with independent-forking parents — closing the 999.64 arc that has blocked every prior unattended-run attempt in this project's history.

**Review:** one adversarial plan review + one peer code review across the milestone; 3 CRITICAL/HIGH findings fixed with mutation-proven tests (`522e905`), 1 filed to backlog (999.75, itself resolved 2026-08-04 in `2c20ab4`/PR #82).

**Known verification overrides:** 2 (see `STATE.md` § Deferred Items — an unrelated gsd-core tooling debug session, and a Phase 24 UAT audit false-positive, both acknowledged at close).

Full detail: `.planning/milestones/v2.3.0-ROADMAP.md`
