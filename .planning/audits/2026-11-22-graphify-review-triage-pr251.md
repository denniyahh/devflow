# Graphify review triage — PR #251 (Phase 53.1), head 4c5fb89

Triaged 2026-11-22. The review lists 1 confirmed bug and 15 possible issues. Only 4 of the 15 are named in the PR
comment and the Graphify MCP `review_status` reply; the other 11 are on the Graphify web page only and were not
triaged here. Findings 2-5 were checked by a read-only helper agent; the session checked finding 1 and the
helper's evidence for 1 and 5.

| # | Finding | Verdict | Disposition |
|---|---|---|---|
| 1 | A huge reset epoch shows a wrapped year (P3 bug, reset_time.rs:45) | REAL, minor: `civil_from_days` cast the year to i32; `format_utc(i64::MAX)` printed `219250468-12-04` | fixed in this PR: the year stays i64; test `format_utc_keeps_the_year_of_the_largest_epoch` fails on the old code |
| 2 | A rate-limit gate under a refusing canary promises a relaunch that is refused | FALSE: a stage under a recorded canary refusal never launches (pipeline_launch.rs:495-516), so it cannot be rate-limited | dismissed |
| 3 | A refused handoff on resume leaves the stage advanced | INTENDED: 53.1-03-PLAN orders commit, transition, handoff; the handoff checks run on the judged copy first (pipeline_launch.rs:2532-2536); only an environment change in between reaches it | dismissed |
| 4 | Resume after `--until plan` always launches Code, skipping a Plan->Validate move | INTENDED: documented in `resume_loaded` and scoped out in 53.1-03-PLAN; filed as 999.260 | 999.260 |
| 5 | A commit reply with `committed: false` is accepted as committed | FALSE: the caller reads `committed == true` and records "committed nothing" on false (pipeline_outcomes.rs:515-527) | dismissed |

CI on the same head failed `the_live_payload_commits_a_context_file_on_integration` (dubious ownership in the CI
checkout); fixed in this PR and logged in `.planning/user/CI-FAILURES.md`.
