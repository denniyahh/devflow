# Review-fix round — verified findings and dispositions

Review base: `4d119fb` (git archive snapshot). Diff under review: `69c3ada..4d119fb -- crates OPERATIONS.md`
(the WR-01..WR-04 fixes from `48-REVIEW.md`). Snapshot integrity: only `graphify-out/cache/last_query_stamp`
changed during the run; the real worktree was unchanged.

## Lanes

| Lane | Model | Result | Citations |
|---|---|---|---|
| codex | gpt-5.6-terra, high | 6 confirmed, 0 suspected | 23 |
| pi | deepseek-v4-pro | 3 confirmed, 1 suspected | 29 |
| agy | gemini-3.8-flash-high | 5 confirmed, 0 suspected | 29 |

All three checked WR-01 (monitor `reaped` guard) across every signal window and found it sound. No lane
reported a critical finding.

## Findings (deduplicated; each verified against source by reading)

| ID | Finding | Lanes | Disposition |
|---|---|---|---|
| R-1 | Sweep deletes an orphan cron record, then prints "no stale workflow state was cleaned" | codex C-4, agy C-1 | Fix in Phase 48 |
| R-2 | Corrupt cron records unreachable by the sweep, silently | codex C-3, pi C-3, agy C-2 | Backlog 999.137 |
| R-3 | Failed stale-lock removal does not set `removal_failed`; exits 0 | codex C-5, pi C-2, agy C-3 | Fix in Phase 48 |
| R-4 | `clear_state` state-temp removal failure is log-only; exits 0 | codex C-2, pi S-1, agy C-4 | Fix in Phase 48 |
| R-5 | Partial gate removal before an error reports "nothing was removed" | pi C-1 | Fix in Phase 48 |
| R-6 | Sweep removes corrupt legacy `state.json`, then prints "no stale workflow state was cleaned" | agy C-5 | Fix in Phase 48 |
| R-7 | A phase with only an orphaned state temp is never swept | codex C-1 | Backlog 999.137 |
| R-8 | `Gates::phases_on_disk` accepts any `NN-*` name; needless lock cycle, no deletion | codex C-6 | Backlog 999.137 |

Disposition is the operator's option 1 (2026-09-22): fix the honest-reporting and exit-code findings in
Phase 48; backlog the reach findings.
