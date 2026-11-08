---
phase: 51-rate-limit-agent-failover
verified: 2026-11-08T20:40:00Z
status: passed
score: 114/114 must-have truths verified (4 roadmap success criteria + 110 plan truths); 9/9 judgment-tier prohibitions hold (one with a recorded advisory)
covered_files: [".planning/phases/51-rate-limit-agent-failover/51-01-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-01-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-02-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-02-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-03-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-03-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-04-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-04-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-05-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-05-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-06-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-06-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-07-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-07-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-09-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-09-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-10-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-10-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-11-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-11-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-12-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-12-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-13-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-13-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-14-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-14-SUMMARY.md", "OPERATIONS.md", "README.md", "crates/devflow-cli/src/commands.rs", "crates/devflow-cli/src/main.rs", "crates/devflow-cli/src/parallel.rs", "crates/devflow-cli/src/pipeline_gate.rs", "crates/devflow-cli/src/pipeline_launch.rs", "crates/devflow-cli/src/pipeline_outcomes.rs", "crates/devflow-cli/src/preflight.rs", "crates/devflow-cli/src/test_support.rs", "crates/devflow-cli/tests/failover_e2e.rs", "crates/devflow-cli/tests/fallback_agent_config.rs", "crates/devflow-cli/tests/gate_notify_persist_e2e.rs", "crates/devflow-core/src/agent_result.rs", "crates/devflow-core/src/config.rs", "crates/devflow-core/src/doc_check.rs", "crates/devflow-core/src/monitor.rs", "crates/devflow-core/src/prompt.rs", "crates/devflow-core/src/state.rs", "doc-check-allowlist.toml", "docs/guides/configuration.md", "docs/guides/unattended-mode.md"]
covered_digest: "v4:sha256:77e76928fb4ff487f44af5efaf10cd8fd57afb58c7d3ed1b34268f7e490df4b4"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: "114/114 truths; 1 of 9 prohibitions not held in full"
  reason: "commits since the 2026-11-08T18:45:47Z report (7095b38): ff649ff (residual documented), cce00cc (way on leads the failure gate), d149da8 (status hint judges the env opt-out as resume does), fc92c6b (gate_show comment), 744739a (clause budget pinned to the display bound), e564570/6443b82/1bdcda0/98d3b3c (.planning only)"
  gaps_closed:
    - "51-11 prohibition, torn-line record: OPERATIONS.md:318-327 now scopes the flagged-record sentence to 'when the torn line reached the capture before the monitor's idle check' and names the kill-time partial-line order as a known residual (999.207): the monitor logs rate-limited, writes no record and kills the agent; the verdict is the indeterminate Failed ('stream capture ends in an unparseable line'); a gate, no failover, no cron. Each clause checked against monitor.rs:1342 (read_until tees whole lines), monitor.rs:2752-2767 (Denied arm: log text, no record, terminate_child_group), agent_result.rs:1901-1906 (torn guard, fires with no result event because torn_json_after_last_matching uses is_none_or), agent_result.rs:2177-2182 (reason text). The two overstating code comments (agent_result.rs capture-denial doc, monitor.rs:4318-4324 test doc) were corrected to the same scope"
  gaps_remaining: []
  regressions: []
advisory:
  - finding: "The residual paragraph's 'The outcome is still a gate, with no failover and no cron resume' assumes the killed agent's partial line reaches the capture. If a process outside the agent's process group still holds the agent's stdout, the reader's read_until waits for a newline or EOF that does not come; finish_stream_threads gives up after READER_DRAIN_WAIT (3 s) and closes the capture (monitor.rs:1130-1195, join_within and close_shared_capture), so the partial line never reaches it. The verdict then reads a clean denial: RateLimited, so failover at Code or cron elsewhere"
    category: other
    reason: "Established by reading code only, not reproduced. Needs a stdout holder that escaped the SIGTERM to the agent's group (monitor.rs:3026) and outlived the 3 s drain bound, which also records a stream_wait_bounded event. The resulting outcome is the pre-Phase-51 behaviour for a capture that, as recorded, is a proven denial; not a goal failure. Would be resolved by adding this sub-case to backlog 999.207 (its post-EOF re-check would not cover it either, since EOF never arrives) or by qualifying the sentence"
    evidence_status: "none provided (code reading only)"
prohibitions_resolved:
  # Judgment-tier (ADR-550 D4). Decided by the verifier (Claude) on the evidence named.
  - statement: "Failover never resets, stashes, cleans or rebases the worktree and never adds a worktree or branch (51-01)"
    verdict: holds
    basis: "the handoff path is untouched since the previous report (pipeline_launch.rs diff since 7095b38 only adds resume_canary_refusal/combine_legacy_launch_opt_out); a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage and a_handoff_inside_a_phase_worktree_keeps_the_tree_branch_and_uncommitted_work passed in this verifier's run"
  - statement: "A rate limit at any stage other than Code never hands off or consumes the fallback (51-01)"
    verdict: holds
    basis: "a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm passed in this verifier's run; failover_target unchanged"
  - statement: "A torn or partially written capture line is never classified as a rate limit and never drives a resume schedule or a handoff (51-05)"
    verdict: holds
    basis: "a_denial_followed_by_a_torn_line_is_torn_for_the_monitor_and_the_verdict and its control a_denial_with_no_torn_tail_is_rate_limited_for_the_monitor_and_the_verdict passed (verifier run). A line that never reaches the capture (see advisory) is not classified at all; the verdict classifies only captured bytes"
  - statement: "The fallback never launches on the primary's canary evidence or session id (51-06)"
    verdict: holds
    basis: "a_handoff_reruns_the_delivery_canary_for_the_new_agent passed in this verifier's run"
  - statement: "A handoff never ends recorded as handed off with nothing running and no gate; a post-commit relaunch failure never schedules a resume (51-06)"
    verdict: holds
    basis: "a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate passed in this verifier's run (its expected way-on text updated by cce00cc to the new lead wording)"
  - statement: "The never-silent gate never weakens Phase 50 D-03's ownership fence (51-06)"
    verdict: holds
    basis: "the guard and ownership code are untouched since the previous report, which recorded the_supervisor_record_is_cleared_only_on_launch_failure_paths and a_failover_superseded_mid_relaunch_writes_nothing as passing; not re-run in this round"
  - statement: "The notify command never reaches any agent's environment, fallback included (51-09)"
    verdict: holds
    basis: "no change to the launch environment since the previous report (diff checked); the two notify-variable tests passed in the orchestrator log the previous report cited; not re-run in this round"
  - statement: "No artifact records criterion 3 from a test whose outcome could not have differed (51-10, 51-13)"
    verdict: holds
    basis: "the_supv01_evidence_is_complete and the_supv01_evidence_trace_rows_are_complete passed with --run-ignored only (verifier run)"
  - statement: "The operator docs never promise more than the code does (51-11)"
    verdict: holds
    basis: "the previous gap is closed (see gaps_closed). The way-on docs (OPERATIONS.md:344-366) still match the code after cce00cc and d149da8: the gate context and notify hook carry the sentence, and the status hint now judges the environment's legacy opt-out as resume does. One sub-case of the residual paragraph is recorded under advisory: established by code reading only, it needs an out-of-group stdout holder, and it is not a statement about captured bytes"
---

# Phase 51: Rate-Limit Agent Failover Verification Report

**Phase Goal:** An agent that exhausts its token budget mid-stage does not end the night. The run continues under a configured fallback agent, in the same worktree on the same branch, with no manual conflict resolution.
**Verified:** 2026-11-08T20:40:00Z, at HEAD `98d3b3c` (crates/ identical to `744739a`)
**Status:** passed
**Re-verification:** Yes. Previous report: gaps_found at `7095b38`.

## Bottom line

- **The previous gap is closed, by documentation, not by a code fix.** OPERATIONS.md now limits the "recorded, flagged torn" promise to a torn line already in the capture, and names the kill-time partial-line order as a known residual (backlog 999.207). Each clause of the new paragraph was checked against the code: the log text, the missing record, the kill, the indeterminate `Failed` reason, and the gate outcome. The two code comments that overstated it now say the same thing. The monitor behaviour itself is unchanged; the code fix is deferred to 999.207 by operator decision.
- **The goal holds** under the stub-agent standard. The failover core is unchanged since the previous report. Re-run here: the binary e2e Claude to Antigravity handoff, the no-fallback cron arm, the handoff inside a phase worktree, the failed-canary gate, and the status-to-resume way on.
- **The other review fixes hold.** The way-on sentence now leads the failure gate context (cce00cc); the new test proves it survives the banner, `gate list` and the all-roots row with a 400+ character reason, with a control. The status hint judges the environment's legacy opt-out as `resume` does (d149da8). The idle-timeout clause budget is pinned to the 300-character display bound with a one-past-budget control (744739a).
- **One advisory, new this round, code reading only.** The residual paragraph's "the outcome is still a gate" assumes the partial line reaches the capture. If a process outside the agent's group still holds its stdout past the 3-second drain bound, the capture is closed without it, and the verdict reads a clean denial (RateLimited). This needs an unusual precondition and is not a goal failure, so it is not a gap. It is not in 999.207's text.

## What this verification does and does not establish

- The WR-02 order and the advisory sub-case are established by reading code. Neither was reproduced at runtime. The verdict half of WR-02 is tested (a denial plus a torn line with no record is not RateLimited). The monitor half (no record written when the partial line arrives after the check) is untested, as 999.207 says.
- The verdict test asserts only "not RateLimited", not the indeterminate reason text. The text the docs quote (`stream capture ends in an unparseable line`) was confirmed by reading `indeterminate_capture_failure` and the torn guard that returns it.
- Every failover proof uses stub agents. No live failover has been run on this build.
- Two prohibitions (ownership fence; notify variable) rest on tests not re-run in this round. Their code is untouched since the report that recorded them passing.
- The full suite was not re-run by this verifier; the caller states `scripts/check.sh all` passed at `744739a`. Not checked here.

## Goal Achievement

### Roadmap Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | On `RateLimited` with a fallback, the run continues under it in the same worktree and branch, and the stage completes (Code only, R-2) | ✓ VERIFIED (stub agents) | Verifier run: `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code`, `a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage`, `a_handoff_inside_a_phase_worktree_keeps_the_tree_branch_and_uncommitted_work` passed |
| 2 | With a fallback the agents switch; with none, cron instructions are written | ✓ VERIFIED | Verifier run: `a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions`, `a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm` passed |
| 3 | The SUPV-01 dependency is settled by observation | ✓ VERIFIED | both ignored evidence tests passed (`--run-ignored only`, verifier run) |
| 4 | The `n` verb, `rebase_in` and the rebase handoff stay dead | ✓ VERIFIED | `doc_check::the_removed_sequentagent_handoff_stays_dead` passed (verifier run) |

### Plan Must-Have Truths (110)

No truth changed status. Code changes since `7095b38`, checked against the plans that name the code they touch:

| Plan | Status | Impact of the changes |
|------|--------|-----------------------|
| 51-03 (idle-timeout cause) | ✓ VERIFIED | `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` became `pub` (additive) so the CLI can pin it; `the_budgeted_idle_timeout_clause_closes_inside_the_gate_display_bound` passed |
| 51-05 (torn-line classifier) | ✓ VERIFIED | Comments only in agent_result.rs and monitor.rs; both monitor torn-line tests and both verdict tests passed |
| 51-06 (hardened hop, way on) | ✓ VERIFIED | The way on moved from the end of the gate context to its start (`failure_gate_context`, pipeline_outcomes.rs:932-945); `resume_canary_refusal` shared by `resume` and `status`. Failover, guard and gate paths unchanged |
| 51-11 (docs) | ✓ VERIFIED | Residual paragraph added (OPERATIONS.md:319-327); the D-16 truth ("a quota denial cannot be ruled out") still holds |
| all others | ✓ VERIFIED | untouched |

**Score:** 114/114 truths verified, 0 present but behavior-unverified. 9/9 judgment-tier prohibitions hold.

### Deferred Items

None. The 999.207 code fix is not a failed truth: the docs now describe the behaviour accurately. Note that 999.207 says "Phase 52", but no Phase 52 section exists in ROADMAP.md yet.

### Advisory (New Scope, Unevidenced)

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | An agent stdout holder outside the killed group, alive past the 3 s drain bound, keeps the partial line out of the capture; the verdict is then RateLimited, not the gate the residual paragraph promises | other | new-scope, code reading only; needs an unusual precondition; outcome is the pre-Phase-51 behaviour for a proven-denial capture |

### Key Link Verification (hand-checked in the current tree)

- `fire_idle_timeout` `Denied` arm: monitor log text, no record, `terminate_child_group`: monitor.rs:2752-2767
- Reader tees whole lines (`read_until(b'\n')`): monitor.rs:1342
- Verdict computed after the stream threads are joined (bounded): monitor.rs:1802-1807, then pipeline_launch.rs:1634 onward in the same process
- Torn guard returns `indeterminate_capture_failure()` before the denial check: agent_result.rs:1901-1911
- `failure_gate_context` leads with the way on when the recorded canary refuses: pipeline_outcomes.rs:932-945
- `status` hint and `resume` refusal share `resume_canary_refusal`: commands.rs:1659, pipeline_launch.rs:3715

### Behavioral Spot-Checks (this verifier's runs; one `cargo nextest run --no-tests=fail -E 'test(=…)'` per test)

| Behavior | Test | Status |
|----------|------|--------|
| Denial + torn tail not RateLimited / control | `agent_result::tests::a_denial_followed_by_a_torn_line_is_torn_for_the_monitor_and_the_verdict` / `…with_no_torn_tail_is_rate_limited…` | ✓ PASS / ✓ PASS |
| Monitor flags the record (torn line already in capture) | `monitor::tests::a_torn_final_line_at_the_idle_timeout_is_flagged_on_the_record`, `monitor::tests::a_denial_followed_by_a_torn_line_at_the_idle_timeout_is_recorded_not_suppressed` | ✓ PASS |
| Failover unit, Plan-stage cron arm, canary re-run | 3 `pipeline_launch::tests` | ✓ PASS |
| Way on survives banner/list/row for a long reason (control: admitting canary) | `commands::tests::the_failure_gate_way_on_survives_the_banner_and_gate_list_for_a_long_reason` | ✓ PASS |
| Clause budget closes inside the display bound (control: budget+24 is cut) | `commands::tests::the_budgeted_idle_timeout_clause_closes_inside_the_gate_display_bound` | ✓ PASS |
| Status hint judges env opt-out as resume | `commands::tests::recovery_hints_judge_the_environment_legacy_opt_out_as_resume_does` | ✓ PASS |
| SUPV-01 evidence | 2 ignored tests, `--run-ignored only` | ✓ PASS |
| e2e: handoff, failed-canary gate, status→resume, no-fallback cron, worktree handoff | 5 `failover_e2e` tests | ✓ PASS |
| `parallel` ignores / refuses the config key | 2 `fallback_agent_config` tests | ✓ PASS |
| Docs pins | `doc_check::operations_md_documents_the_fallback_agent_and_its_events`, `doc_check::the_removed_sequentagent_handoff_stays_dead` | ✓ PASS |
| Negative control | planted name `commands::tests::no_such_test_planted_by_the_verifier` | rc 4 (fails as it must) |

The working tree was clean after the runs.

### Probe Execution

None. No plan declares a `probe-*.sh`.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|----------|
| SUPV-02 | all 13 plans | ✓ SATISFIED | criteria 1-4 above. REQUIREMENTS.md maps only SUPV-02 to Phase 51 (line 269); nothing orphaned |

### Anti-Patterns Found

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`todo!`/`unimplemented!` on any line added under `crates/` or in OPERATIONS.md since `7095b38`. A planted `+// TODO x` line was caught by the same pattern.

| File | Line | Finding | Severity |
|------|------|---------|----------|
| OPERATIONS.md | 326-327 | "The outcome is still a gate" does not cover the out-of-group stdout holder sub-case (advisory 1) | ⚠️ Warning |
| crates/devflow-core/src/monitor.rs | 2752-2767 | `Denied` decided before a pending partial line is teed (WR-02; documented residual, 999.207) | ℹ️ Info (documented, deferred) |
| .planning/phases/51-rate-limit-agent-failover/51-REVIEW-DISPOSITION.md | WR-02 row | records WR-02 as `fixed`; the code fix was deferred to 999.207 and only the docs were fixed | ℹ️ Info |

### Human Verification Required

None. Every judgment-tier prohibition was settled on the evidence above.

### Gaps Summary

No gaps. The one gap from the previous round is closed by accurate documentation of the residual, which this verifier checked clause by clause against the code. The phase goal holds for `start` and `resume` under stub agents. One advisory sub-case (a stdout holder outside the agent's group) is recorded for 999.207's scope.

---

_Verified: 2026-11-08T20:40:00Z_
_Verifier: Claude (gsd-verifier)_
