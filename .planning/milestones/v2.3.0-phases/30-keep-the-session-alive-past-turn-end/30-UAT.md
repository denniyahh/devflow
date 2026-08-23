---
status: complete
phase: 30-keep-the-session-alive-past-turn-end
source: [30-01-SUMMARY.md, 30-02-SUMMARY.md, 30-03-SUMMARY.md, 30-04-SUMMARY.md, 30-05-SUMMARY.md]
started: 2026-08-03T09:05:35Z
updated: 2026-08-03T09:20:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Confirm auto-covered deliverables (30-01, 30-03)
expected: |
  **30-01 (6 deliverables):**
  - D1: A real archived Claude stream-json capture yields a Layer-1 AgentResult instead of None — `evaluate_layer1_parses_claude_stream_capture`
  - D2: Last-result-wins with three result events, in both directions — `claude_stream_last_result_event_wins_over_earlier_results`
  - D3: An agent-planted `decided_by_layer:0` is overwritten to `Some(1)` — `claude_stream_overwrites_agent_planted_decided_by_layer`
  - D4: A marker-less final turn defers to Layer 2 rather than silently advancing — `claude_stream_last_result_without_marker_defers`
  - D5: Neither the single-doc envelope nor a Codex stream is consumed by the new parser; plain text untouched — 4 tests
  - D6: The 4 pre-existing `evaluate_layer1_*` tests still pass unedited

  **30-03 (8 deliverables):**
  - D1-D4: Rate-limit classification (allowed event ≠ RateLimited; final-turn denial = RateLimited; a denial before the final turn can't outrank it; an unrecognised status defers)
  - D5-D6: Rate limit outranks the marker path; `is_error:true` on the last result classifies Failed and overrides a stale success marker
  - D7-D8: `session_id` survives a stream capture and is never taken from agent-planted marker text

  All 14 are recorded `result: pass, source: automated` below — proven by the referenced tests, re-verified by me at current HEAD, not merely carried over from when the SUMMARYs were written.
result: pass

### 2. Delivery survives the production launch environment (30c)
expected: |
  Task-notification-origin results (background work completing after the
  orchestrator's turn ends) are delivered through `spawn_monitor`'s real
  process environment — `sh -c`, git-scrubbed env, detached, no TTY, stderr
  separated — not just in a mocked test.

  **Already human-approved during 30-02's execution**, via a blocking
  `checkpoint:human-verify` the operator signed off on after independently
  recounting results from raw JSONL (per `30-02-SUMMARY.md` / `30c-VERDICT.md`:
  `delivery: confirmed`). This checkpoint re-surfaces that finding rather than
  asking you to re-derive it — confirm the prior sign-off stands, or flag if
  anything about it now needs revisiting.

  Evidence: `30c-evidence/raw_output.jsonl` — 3 result events, 2 notification-origin, drain at line 47.
result: pass

### 3. The result is not an artifact of running inside an agent session (30c)
expected: |
  The delivery finding above isn't a false positive caused by the harness
  itself running inside a Claude agent session (which could plant markers in
  its own environment). Verified two ways: a fully-scrubbed run (11 markers
  removed, session markers "(none)") and a plain-fish-shell run outside any
  agent session, both showing the same delivery behavior.

  Already human-approved during 30-02's execution. Confirm the sign-off stands.

  Evidence: `30c-evidence-scrubbed/`, `30c-evidence-operator/` (parent_process: fish).
result: pass

### 4. Delivery is not a one-off (30c)
expected: |
  8 trials across 3 environments (scrubbed, unscrubbed, operator's own shell),
  0 refuted, 0 partial — the delivery finding replicates rather than being a
  single lucky run.

  Already human-approved during 30-02's execution. Confirm the sign-off stands.

  Evidence: `30c-evidence-reliability/trial-1..5` + the other 3 environments.
result: pass

### 5. No committed evidence leaks secrets (30c)
expected: |
  None of the committed capture files leak home paths, OS usernames, session
  identifiers, or credential-shaped tokens. A secret scanner ran over every
  published file with 0 matches, and correctly still flags the staged
  (pre-redaction) captures as a live control — proving the scanner isn't just
  silently passing everything.
result: pass

### 6. Verdict frontmatter matches the raw capture (30c)
expected: |
  `30c-VERDICT.md` and `30c-VERDICT-scrubbed.md`'s frontmatter counts
  (result_events, task_notification_origin_results, etc.) are independently
  recomputed from the raw JSONL and match exactly — the verdict isn't
  hand-typed disconnected from the evidence.
result: pass

### 7. The core crate was read, never written, during 30c
expected: |
  `crates/` stayed byte-identical throughout 30-02's execution — `monitor.rs`
  and `git.rs` were read for their env-scrub list but never modified by that
  plan. `git status --porcelain crates/` was empty at every task boundary.
result: pass

### 8. Post-close exit latency is a real measured distribution (30d)
expected: |
  Exit latency after stdin close is archived as a 5-trial distribution
  (169.5–279.7 ms, all exit code 0) measured through the same production-replica
  launcher as 30c, not estimated. Independently recomputed from the published
  per-trial timings.
result: pass

### 9. Close-with-pending-background-tasks has a recorded, reproducible behavior (30d)
expected: |
  Closing stdin while background tasks are still running was undefined
  behavior before this measurement. It's now recorded across 11 independent
  fields per trial (not collapsed to one label) — 2 trials, both showing
  results delivered after close, both children present, nothing truncated.

  This is a genuine judgment call, not a pass/fail check — it's an
  observation about undefined behavior. Confirm the recorded characterization
  ("exits cleanly, no data lost in 2/2 trials") still reads as accurate framing
  to you, not as overclaiming from 2 trials.

  Evidence: `30d-evidence/mode-b/trial-1..2`.
result: pass

### 10. The observation window couldn't have missed late work (30d)
expected: |
  The measurement window (90s) outlasted the slowest child's deadline plus a
  stated buffer (52.0s floor) in every trial — 63.1s and 62.3s measured past
  the last dispatch — so a "nothing more happened" finding can't be a
  stopwatch artifact. A window shorter than the floor is refused
  (`--window 51.9` aborts).
result: pass

### 11. Every trial's processes were actually cleaned up (30d)
expected: |
  All 7 trials record zero survivor processes after cleanup. One trial was
  deliberately interrupted mid-run to test the reaping path, and it found (and
  reaped) 2 descendants that had left the process group — the exact leak class
  backlog item 999.46 tracks, confirming the check isn't vacuous.
result: pass

### 12. Archived 30d evidence carries no secrets (30d)
expected: |
  Same secret-scan discipline as 30c: 0 matches across 29 in-scope files
  hunting 28 real session UUIDs, with the staged captures matching as a live
  control proving the scanner works.
result: pass

### 13. crates/ was untouched during 30d
expected: |
  `git status --porcelain crates/` was empty at every task boundary during
  30-04's execution — the exit-timing experiment touched only its own harness
  and evidence files.
result: pass

## Auto-Covered Deliverables (30-01, 30-03) — not presented individually

### 14. [30-01 D1] Real archived stream capture yields Layer-1 AgentResult
expected: evaluate_layer1_parses_claude_stream_capture passes
result: pass
source: automated
coverage_id: 30-01-D1

### 15. [30-01 D2] Last-result-wins with three result events
expected: claude_stream_last_result_event_wins_over_earlier_results passes
result: pass
source: automated
coverage_id: 30-01-D2

### 16. [30-01 D3] Agent-planted decided_by_layer:0 is overwritten
expected: claude_stream_overwrites_agent_planted_decided_by_layer passes
result: pass
source: automated
coverage_id: 30-01-D3

### 17. [30-01 D4] Marker-less final turn defers to Layer 2
expected: claude_stream_last_result_without_marker_defers passes
result: pass
source: automated
coverage_id: 30-01-D4

### 18. [30-01 D5] Single-doc envelope and Codex stream not consumed by the new parser
expected: 4 isolation tests pass
result: pass
source: automated
coverage_id: 30-01-D5

### 19. [30-01 D6] Pre-existing evaluate_layer1_* tests unedited and passing
expected: 5 passed, 0 failed
result: pass
source: automated
coverage_id: 30-01-D6

### 20. [30-03 D1] Healthy allowed rate_limit_event is not RateLimited
expected: claude_stream_real_allowed_rate_limit_event_is_not_rate_limited passes
result: pass
source: automated
coverage_id: 30-03-D1

### 21. [30-03 D2] Final-turn denial classifies RateLimited with retry hint
expected: claude_stream_final_turn_denial_rate_limit_event_is_rate_limited passes
result: pass
source: automated
coverage_id: 30-03-D2

### 22. [30-03 D3] A denial before the final turn cannot outrank it
expected: claude_stream_denial_before_final_turn_does_not_outrank_final_result passes
result: pass
source: automated
coverage_id: 30-03-D3

### 23. [30-03 D4] Unrecognised rate_limit_info.status defers rather than fabricating
expected: claude_stream_unrecognised_rate_limit_status_defers passes
result: pass
source: automated
coverage_id: 30-03-D4

### 24. [30-03 D5] Rate limit outranks the marker path
expected: claude_stream_final_turn_denial_outranks_failed_marker passes
result: pass
source: automated
coverage_id: 30-03-D5

### 25. [30-03 D6] is_error:true on last result classifies Failed, overrides success marker
expected: 3 tests pass
result: pass
source: automated
coverage_id: 30-03-D6

### 26. [30-03 D7] session_id survives a stream capture
expected: 3 tests pass
result: pass
source: automated
coverage_id: 30-03-D7

### 27. [30-03 D8] Agent-planted session_id in marker text is never returned
expected: 2 tests pass
result: pass
source: automated
coverage_id: 30-03-D8

## Summary

total: 13
passed: 13
issues: 0
pending: 0
skipped: 0

## Gaps

[none yet]

## Notes

- **30-05 contributes no UAT items.** Its entire deliverable (scoping checkpoint
  gate detection under a stream capture) is internal parser behavior with no
  user-observable surface today — the stream-json path it protects is not
  reachable in production until Phase 31 flips the launch flag. Per this
  workflow's own rule to skip non-observable items.
- **No cold-start smoke test injected.** No SUMMARY references a server/app/
  index/main/database/migrations/docker file — checked directly, not assumed.
- **The extensive post-execution adversarial-review work (8 defects found and
  fixed across 6 review passes, then a root-cause refactor, then a 7th clean
  pass from a different model family) has no separate UAT surface here.** It
  changed internal parsing logic with the same "unreachable in production
  today" property as 30-05. That work is covered by the 136 `agent_result::`
  unit tests (all passing, host + container green at HEAD `099bac6`), not by
  conversational UAT — there is no user-facing behavior for a human to click
  through and confirm.
- **Coverage blocks in 30-01/30-02/30-03/30-04 predate the refactor and 3 fix
  rounds** that followed. I independently re-ran 6 of the 14 auto-passed test
  references against current HEAD before accepting them (all still pass) —
  this UAT session does not blindly trust frontmatter written before the file
  changed three more times.
- A pre-existing, unrelated UAT session for phase 25
  (`25-end-to-end-dogfood-blockers`) is still `status: testing` with 1 test
  pending — untouched by this session, flagged to the operator, not resumed.
