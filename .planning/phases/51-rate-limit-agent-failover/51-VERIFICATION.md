---
phase: 51-rate-limit-agent-failover
verified: 2026-11-08T18:45:47Z
status: gaps_found
score: 114/114 must-have truths verified (4 roadmap success criteria + 110 plan truths); 1 of 9 judgment-tier prohibitions does not hold in full
covered_files: [".planning/phases/51-rate-limit-agent-failover/51-01-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-01-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-02-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-02-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-03-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-03-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-04-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-04-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-05-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-05-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-06-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-06-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-07-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-07-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-09-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-09-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-10-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-10-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-11-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-11-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-12-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-12-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-13-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-13-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-14-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-14-SUMMARY.md", "OPERATIONS.md", "README.md", "crates/devflow-cli/src/commands.rs", "crates/devflow-cli/src/main.rs", "crates/devflow-cli/src/parallel.rs", "crates/devflow-cli/src/pipeline_gate.rs", "crates/devflow-cli/src/pipeline_launch.rs", "crates/devflow-cli/src/pipeline_outcomes.rs", "crates/devflow-cli/src/preflight.rs", "crates/devflow-cli/src/test_support.rs", "crates/devflow-cli/tests/failover_e2e.rs", "crates/devflow-cli/tests/fallback_agent_config.rs", "crates/devflow-cli/tests/gate_notify_persist_e2e.rs", "crates/devflow-core/src/agent_result.rs", "crates/devflow-core/src/config.rs", "crates/devflow-core/src/doc_check.rs", "crates/devflow-core/src/monitor.rs", "crates/devflow-core/src/prompt.rs", "crates/devflow-core/src/state.rs", "doc-check-allowlist.toml", "docs/guides/configuration.md", "docs/guides/unattended-mode.md"]
covered_digest: "v4:sha256:030d1bfc32adc69f432175052202b53a1f80c7f516aef4857aa95be6b1bc5a1f"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: "114/114 truths; 1 of 9 prohibitions not held in full"
  reason: "fix rounds since the 2026-11-08T15:28:35Z report (fe1971a): 1106d67 (parallel names the key), 17a47cf (torn line outranks a denial), 73303a9/91e1b76, b94d356, doc commits; new code review 4a23acf"
  gaps_closed:
    - "51-11 prohibition, README.md parallel case: README.md:110 now says `devflow parallel` takes no fallback and ignores the key, with a note; parallel.rs:119-128 reads the key once, refuses a broken one, prints the note. Tests `parallel_ignores_the_config_key` and `parallel_refuses_a_config_key_that_names_no_agent` passed in this verifier's own run"
  gaps_remaining: []
  regressions:
    - "51-11 prohibition, new sentence: 17a47cf added OPERATIONS.md:315-319, which promises an idle-timeout record flagged torn for every torn final line after a denial; the stream path (the only path with an idle timeout) writes no record when the torn line is a partial line still in the pipe at the monitor's check"
gaps:
  - truth: "51-11 prohibition: the operator docs never promise more than the code does"
    status: partial
    reason: "OPERATIONS.md:312-319 says that when the capture ends in a torn final line the verdict stays an idle timeout with the torn clause, and that a torn line after a quota denial means the stage is never classified as rate-limited and 'at an idle timeout it is recorded as one, flagged torn, and stops at a gate'. On the stream path a partial line (no newline) is held by the reader's `read_until(b'\\n')` (monitor.rs:1341) and reaches the capture only at EOF, after `fire_idle_timeout` checked it (monitor.rs:2751). If the capture held a complete denial at that check, the monitor returns at the `Denied` arm (monitor.rs:2752-2767): it logs 'classified as rate-limited (resumable)', writes no idle-timeout record and kills the group. With no record, `evaluate_layer1` (agent_result.rs:2760) reaches `parse_claude_event_result`, whose torn guard (agent_result.rs:1902-1906) returns `indeterminate_capture_failure()`: Failed, 'stream capture ends in an unparseable line'. So there is no record, no torn flag and no idle-timeout verdict; the monitor log contradicts 'never classified as rate-limited'. The safety outcome the docs promise still holds: not RateLimited, no failover, no cron, a Code gate (pipeline_launch.rs:4181). Established by reading code, not reproduced at runtime"
    artifacts:
      - path: "OPERATIONS.md"
        issue: "lines 312-319: the record and verdict description is false for the kill-time partial-line order on the stream path"
      - path: "crates/devflow-core/src/monitor.rs"
        issue: "lines 2752-2767: the Denied arm is decided before a pending partial line reaches the capture, and nothing re-checks after EOF (review WR-02)"
    missing:
      - "Either correct OPERATIONS.md:312-319 to name this order (no record; an indeterminate Failed gate; the monitor log says rate-limited), along with the two code comments that overstate it (agent_result.rs:2606 'the two cannot disagree', monitor.rs:4320-4321 'the read side covers that case'), or apply review WR-02's fix: re-classify after the reader reaches EOF, write the flagged record when the answer changed, and add a monitor test whose stub prints the denial and then a partial line with no newline"
prohibitions_resolved:
  # Judgment-tier (ADR-550 D4). Decided by the verifier (Claude) on the evidence named.
  - statement: "Failover never resets, stashes, cleans or rebases the worktree and never adds a worktree or branch (51-01)"
    verdict: holds
    basis: "hand_off_agent unchanged in the diff since 376b645 (pipeline_launch.rs gained only refuse_recorded_canary_for_resume and one test); a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage and a_handoff_inside_a_phase_worktree_keeps_the_tree_branch_and_uncommitted_work passed in this verifier's run"
  - statement: "A rate limit at any stage other than Code never hands off or consumes the fallback (51-01)"
    verdict: holds
    basis: "failover_target (pipeline_launch.rs:3092) unchanged; a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm passed (verifier run); failover_target_selects_the_fallback_only_at_code PASS in the orchestrator log"
  - statement: "A torn or partially written capture line is never classified as a rate limit and never drives a resume schedule or a handoff (51-05)"
    verdict: holds
    basis: "The verdict is what drives cron and failover, and it is never RateLimited for a denial followed by a torn line: classify_capture_denial puts the torn line first (agent_result.rs:2612-2614), and parse_claude_event_result's torn guard runs before it. a_denial_followed_by_a_torn_line_is_torn_for_the_monitor_and_the_verdict passed (verifier run). It calls evaluate_layer1 with no idle-timeout record, which is the exact post-EOF state of review WR-02's order. Its control a_denial_with_no_torn_tail_is_rate_limited_for_the_monitor_and_the_verdict also passed. In WR-02's order the monitor's 'rate-limited' log is about the complete denial it saw, not the torn line, and it schedules nothing"
  - statement: "The fallback never launches on the primary's canary evidence or session id (51-06)"
    verdict: holds
    basis: "handoff commit still sets canary = None and session_id = None (pipeline_launch.rs:3210-3211); a_handoff_reruns_the_delivery_canary_for_the_new_agent PASS in the orchestrator log"
  - statement: "A handoff never ends recorded as handed off with nothing running and no gate; a post-commit relaunch failure never schedules a resume (51-06)"
    verdict: holds
    basis: "a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate passed (verifier run); a_handoff_whose_relaunch_fails_ends_at_a_gate PASS in the orchestrator log"
  - statement: "The never-silent gate never weakens Phase 50 D-03's ownership fence (51-06)"
    verdict: holds
    basis: "one non-test FailoverRelaunchGuard::engage( site (pipeline_launch.rs:3268); the_supervisor_record_is_cleared_only_on_launch_failure_paths and a_failover_superseded_mid_relaunch_writes_nothing PASS in the orchestrator log"
  - statement: "The notify command never reaches any agent's environment, fallback included (51-09)"
    verdict: holds
    basis: "agent_via_legacy_monitor_never_receives_the_notify_variable and agent_via_default_launch_canary_never_receives_the_notify_variable PASS in the orchestrator log"
  - statement: "No artifact records criterion 3 from a test whose outcome could not have differed (51-10, 51-13)"
    verdict: holds
    basis: "the_supv01_evidence_is_complete and the_supv01_evidence_trace_rows_are_complete passed with --run-ignored only (verifier run)"
  - statement: "The operator docs never promise more than the code does (51-11)"
    verdict: does not hold in full
    basis: "README parallel case closed (see gaps_closed). Its listed item 'the devflow.toml key is described as read by start only' now reads 'armed by devflow start only', with parallel reading it to refuse or note it. The code changed, so the docs changed to match; that is accurate, not a breach. Still open: OPERATIONS.md:312-319 over-promises the torn-line record (the one gap). Review WR-01 does not breach this: OPERATIONS.md claims the way-on sentence only for the gate context and the notify hook, and main.rs:503 says gate list truncates at 300"
---

# Phase 51: Rate-Limit Agent Failover Verification Report

**Phase Goal:** An agent that exhausts its token budget mid-stage does not end the night. The run continues under a configured fallback agent, in the same worktree on the same branch, with no manual conflict resolution.
**Verified:** 2026-11-08T18:45:47Z, at HEAD `d2ec7c3` (code identical to `2213057`; the three later commits touch only `.planning/`)
**Status:** gaps_found (one narrow docs gap; the goal itself holds)
**Re-verification:** Yes. Previous report: gaps_found at `fe1971a`.

## Bottom line

- **The previous gap is closed.** README.md:110 and the OPERATIONS.md `parallel` row now say that `parallel` takes no fallback. `parallel.rs:119-128` reads the key once, refuses a broken one, and prints a note. Both parallel tests passed in this verifier's own run. A planted wrong test name exited 4, so the named-test gates are not vacuous.
- **The goal still holds** for `start` and `resume`, under the stub-agent standard (R-5). The failover core is unchanged since the previous report. Re-run here: the binary e2e Claude→Antigravity handoff, the no-fallback cron arm, the handoff inside a phase worktree, and the failed-canary gate and its way on.
- **One new gap, from review WR-02, which this verifier confirmed by reading code.** 17a47cf's new OPERATIONS.md sentence covers a denial followed by a torn line. It promises an idle-timeout record flagged torn. On the stream path, a torn line that is a partial line still in the pipe reaches the capture only after the monitor's check. In that order the monitor logs "rate-limited", writes no record, and the run ends at an indeterminate Failed Code gate. The outcome is safe: no failover and no cron, as the docs promise. The record and the log are not as the docs describe them, which breaks the 51-11 docs prohibition in the same way the README sentence did last round.
- **Review WR-01 is confirmed and breaks no must-have.** The way-on sentence is appended after a reason capped at 300 characters, so `devflow status` and `gate list` cut it whenever the reason is longer than about 28 characters. That is always the case after a failed handoff relaunch. No plan truth requires it in those views. OPERATIONS.md promises it only in the gate context and the notify hook, which carry it in full. After a retry answer, `status` names `resume --agent <other>` (`after_the_refused_relaunch_status_and_resume_lead_to_another_agent` passed). Correction to the reviewer: the shortest reason (`no details available`) gives a 292-character context, which is under 300 and shown in full. The reviewer's "always cut" holds for the failed-relaunch case, not for every reason.

## What this verification does and does not establish

- WR-02's order is established by reading code: the reader's `read_until`, the `Denied` early return, and the layer-1 cascade order. It was **not reproduced at runtime**, because this run could create no file except this report. The verdict half is tested: `evaluate_layer1` on a denial+torn capture with no record is not RateLimited. The monitor half (no record written in that order) is untested. The existing monitor test prints its torn line with a newline (monitor.rs:4413-4415).
- Every failover proof uses stub agents. No live failover has been run on this build.
- Tests not re-run by this verifier rely on the orchestrator's full-suite log at `2213057` (2120 passed, 0 FAIL lines). That log does not record its commit. Its timestamp (13:25 EST) is after `2213057` (13:13 EST), and no code changed after it. A planted absent name returned 0 PASS lines in it.
- Mutants, fmt, clippy and semver-checks were not re-run in this round.

## Goal Achievement

### Roadmap Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | On `RateLimited` with a fallback, the run continues under it in the same worktree and branch, and the stage completes (Code only) | ✓ VERIFIED (stub agents) | `advance_with` RateLimited arm → `failover_target` → `fail_over_on_rate_limit` (pipeline_launch.rs:4201-4202, 3092, 3255). Verifier run: `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code`, `a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage` and `a_handoff_inside_a_phase_worktree_keeps_the_tree_branch_and_uncommitted_work` passed |
| 2 | With a fallback the agents switch; with none, cron instructions are written | ✓ VERIFIED | Verifier run: `a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions` and `a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm` passed |
| 3 | The SUPV-01 dependency is settled by observation | ✓ VERIFIED | both ignored evidence tests passed (`--run-ignored only`, verifier run) |
| 4 | The `n` verb, `rebase_in` and the rebase handoff stay dead | ✓ VERIFIED | `doc_check::the_removed_sequentagent_handoff_stays_dead` passed (verifier run) |

### Plan Must-Have Truths (110)

None changed status since the previous report. Code changes since `376b645` were checked against each plan that names the code they touch:

| Plan | Status | Impact of the changes since the previous report |
|------|--------|--------------------------------------|
| 51-04 / 51-14 (`--fallback-agent`, `devflow.toml` key) | ✓ VERIFIED | `parallel` now reads the key, but its runs still carry no fallback (`parallel_ignores_the_config_key` asserts that no fallback is saved). No truth says `parallel` must not read the key |
| 51-05 (torn-line classifier) | ✓ VERIFIED | Truth 3 is conditional on an idle-timeout record being present. Truth 5 says the monitor writes the flagged record "when it sees the torn answer", and that the denied answer still suppresses. Both hold as written. WR-02's order is one in which the monitor never sees the torn answer: it is a docs gap, not a truth failure |
| 51-06 (hardened hop) | ✓ VERIFIED | `refuse_recorded_canary_for_resume` was added. The handoff, the guard and the gate are unchanged |
| 51-11 (docs) | ✓ VERIFIED (truths) | The D-16 docs truth ("a quota denial cannot be ruled out" for a torn line) still holds at OPERATIONS.md:313-314. The prohibition is the gap |
| all others | ✓ VERIFIED | untouched |

**Score:** 114/114 truths verified, 0 present but behavior-unverified. 9 judgment-tier prohibitions: 8 hold and 1 does not hold in full.

### Key Link Verification (hand-checked in the current tree)

- `RateLimited` arm → `failover_target` → `fail_over_on_rate_limit`: pipeline_launch.rs:4201-4202
- `FailoverRelaunchGuard::engage(` has one non-test site: pipeline_launch.rs:3268
- The handoff clears `canary` and `session_id`: pipeline_launch.rs:3210-3211
- `parallel` → `config::fallback_agent` before the first launch → `start(.., NoFallback)`: parallel.rs:119-128, 159
- `handle_stage_failure` → the way-on sentence appended after `stage_failure_context`: pipeline_outcomes.rs:935-944. This is the WR-01 placement
- `fire_idle_timeout` `Denied` early return: monitor.rs:2752-2767. Nothing re-checks after EOF (the WR-02 gap)

### Behavioral Spot-Checks (this verifier's runs; one `--no-tests=fail -E 'test(=…)'` per test)

| Behavior | Test | Status |
|----------|------|--------|
| `parallel` notes the key and arms nothing | `fallback_agent_config::parallel_ignores_the_config_key` | ✓ PASS |
| `parallel` refuses an invalid key before launching (control: no file, no note) | `fallback_agent_config::parallel_refuses_a_config_key_that_names_no_agent` | ✓ PASS |
| Denial + torn tail is not RateLimited (no record) / control: no torn tail is RateLimited | `agent_result::tests::a_denial_followed_by_a_torn_line_is_torn_for_the_monitor_and_the_verdict` / `..._with_no_torn_tail_is_rate_limited_...` | ✓ PASS / ✓ PASS |
| Monitor writes the flagged record when the torn line is already in the capture | `monitor::tests::a_denial_followed_by_a_torn_line_at_the_idle_timeout_is_recorded_not_suppressed` | ✓ PASS (newline-terminated torn line only) |
| Failover e2e, cron e2e, worktree handoff, failed canary gate, status→resume way on | 5 `failover_e2e` tests | ✓ PASS |
| Unit failover / Plan-stage cron arm | 2 `pipeline_launch::tests` | ✓ PASS |
| Docs pins | `doc_check::operations_md_documents_the_fallback_agent_and_its_events`, `doc_check::the_removed_sequentagent_handoff_stays_dead` | ✓ PASS |
| SUPV-01 evidence | 2 ignored tests, `--run-ignored only` | ✓ PASS |
| Negative control | planted wrong test name | rc 4 (fails as it must) |

### Probe Execution

None. No plan declares a `probe-*.sh`.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|----------|
| SUPV-02 | all 13 plans | ✓ SATISFIED | criteria 1-4 above. REQUIREMENTS.md maps only SUPV-02 to Phase 51 (line 269), so nothing is orphaned |

### Anti-Patterns Found

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`todo!`/`unimplemented!` appears on any line added since `376b645`. A planted `TODO` was caught by the same pattern.

| File | Line | Finding | Severity |
|------|------|---------|----------|
| OPERATIONS.md | 312-319 | the torn-line record and verdict are over-promised for the kill-time partial-line order | 🛑 Gap (51-11 prohibition) |
| crates/devflow-core/src/monitor.rs | 2752-2767 | `Denied` is decided before a pending partial line is teed; the log says "classified as rate-limited" in that order (review WR-02) | ⚠️ Warning (safe outcome) |
| crates/devflow-core/src/agent_result.rs | 2606 | doc comment "the two cannot disagree" is true only for a torn line already in the capture | ⚠️ Warning |
| crates/devflow-core/src/monitor.rs | 4320-4321 | test doc "the read side covers that case" is true only when a record exists | ⚠️ Warning |
| crates/devflow-cli/src/pipeline_outcomes.rs | 935-944 | the way-on sentence follows a reason capped at 300, so `status` and `gate list` cut it (review WR-01) | ⚠️ Warning (no must-have) |
| crates/devflow-cli/src/commands.rs | 1654 | status hint ignores the environment's legacy opt-out (review IN-01) | ℹ️ Info |

### Human Verification Required

None that the evidence leaves open. The one decision is how to close the gap.

### Gaps Summary

There is one gap, the same kind as last round's: an operator-doc sentence promises more than the code does. 17a47cf made a torn line win over a parsed denial. OPERATIONS.md:312-319 then says such a run is always recorded as an idle timeout, flagged torn. On the stream path, the realistic torn line is the partial line a killed or wedged CLI leaves. That line arrives after the monitor's check, so the monitor suppresses the record and logs "rate-limited", and the stage ends at an indeterminate Failed gate. The safety guarantee holds (no failover, no cron, a gate). The described record and log do not.

Two ways to close it:
- **Docs only:** name this order in OPERATIONS.md and fix the two code comments. That is about three sentences.
- **Code:** apply WR-02's fix. Re-classify after EOF, write the flagged record, and add a monitor test with a partial line that has no newline. That also completes the operator's option-B ruling ("the torn line wins everywhere") for the monitor side.

If the operator accepts the sentence as it stands, record an override:

```yaml
overrides:
  - must_have: "The operator docs never promise more than the code does"
    reason: "OPERATIONS.md's torn-line sentence states the safety outcome correctly (no failover, no cron, a gate); the record shape differs only in the kill-time partial-line order"
    accepted_by: "{name}"
    accepted_at: "{ISO timestamp}"
```

---

_Verified: 2026-11-08T18:45:47Z_
_Verifier: Claude (gsd-verifier)_
