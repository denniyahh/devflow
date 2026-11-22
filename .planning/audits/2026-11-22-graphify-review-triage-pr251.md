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

## Re-review of head 0b882e2 (15 possible issues, pasted by the operator from the Graphify page)

Checked by a read-only helper agent against the source; the session re-read the code for 1, 2 and 7.

| # | Finding | Verdict |
|---|---|---|
| 1 | Reopened verification gate says a sign-off is still needed when the fresh status changed (pipeline_outcomes.rs:2095) | FALSE: Approve reopens only on `HumanNeeded` or an unread status (pipeline_outcomes.rs:795-796) |
| 2 | Clock before 1970 makes every reset look far (pipeline_outcomes.rs:117) | REAL, low impact: 999.261 |
| 3 | Refused phase.complete test scripts the wrong reply (pipeline_gate.rs:3037) | FALSE: `script_reply(2, …)` replaces the second call's reply, and the test asserts the refusal reason |
| 4 | Refused handoff on resume leaves the run advanced (pipeline_launch.rs:2568) | INTENDED: the raced case, 53.1-03-PLAN |
| 5 | `commit_files` accepts `committed: false` (gsd_reader.rs:737) | FALSE: the caller records "committed nothing" (pipeline_outcomes.rs:513-526) |
| 6 | Empty `files` passes a bare `--files` (gsd_reader.rs:732) | NIT: the only caller returns early on empty files |
| 7 | Denial without `resetsAt` reads "until rejected" (agent_result.rs:1477) | REAL, wording: 999.261 |
| 8 | Cancel of an id both queued and in flight (ask_server.rs:381) | NIT: needs a reused in-flight request id, which JSON-RPC forbids (spec not re-read) |
| 9 | Gate-reset e2e asserts through `format_utc` (failover_e2e.rs:1072) | NIT: `format_utc` has fixed-value unit tests |
| 10 | Absent-file check passes when git fails (live_payload_fixture.rs:64) | NIT: the positive check on the same repo fails first |
| 11 | Merged-branch sweep test passes with a broken sweep (cleanup_rules_e2e.rs:579) | NIT, true for this test: the next test covers a branch no checkout holds |
| 12 | Notify marker written with literal quotes (ask_e2e.rs:244) | FALSE: the quotes are shell quoting; `printf '%s'` writes the bare value |
| 13 | Publish-whole test can leave the hook running (ask_e2e.rs:1064) | NIT: the child is `sh` with a 1 s sleep and exits on its own |
| 14 | `validate_gate_context` lost its doc (pipeline_outcomes.rs:1557) | REAL, cosmetic: 999.261 |

999.261 (findings 2, 7 and 14) was fixed in this PR on 2026-11-22, each code fix with a test that fails on the old code.
| 15 | Doc says the commit covers GSD's files but paths are fixed (pipeline_gate.rs:2889) | FALSE: the commit takes the files `phase.complete` reported |
