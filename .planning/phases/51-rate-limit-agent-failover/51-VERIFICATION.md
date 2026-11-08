---
phase: 51-rate-limit-agent-failover
verified: 2026-11-08T15:28:35Z
status: gaps_found
score: 114/114 must-have truths verified (4 roadmap success criteria + 110 plan truths); 1 of 9 judgment-tier prohibitions does not hold in full
covered_files: [".planning/phases/51-rate-limit-agent-failover/51-01-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-01-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-02-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-02-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-03-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-03-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-04-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-04-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-05-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-05-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-06-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-06-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-07-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-07-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-09-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-09-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-10-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-10-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-11-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-11-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-12-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-12-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-13-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-13-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-14-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-14-SUMMARY.md", "OPERATIONS.md", "README.md", "crates/devflow-cli/src/commands.rs", "crates/devflow-cli/src/main.rs", "crates/devflow-cli/src/parallel.rs", "crates/devflow-cli/src/pipeline_gate.rs", "crates/devflow-cli/src/pipeline_launch.rs", "crates/devflow-cli/src/pipeline_outcomes.rs", "crates/devflow-cli/src/preflight.rs", "crates/devflow-cli/src/test_support.rs", "crates/devflow-cli/tests/failover_e2e.rs", "crates/devflow-cli/tests/fallback_agent_config.rs", "crates/devflow-cli/tests/gate_notify_persist_e2e.rs", "crates/devflow-core/src/agent_result.rs", "crates/devflow-core/src/config.rs", "crates/devflow-core/src/doc_check.rs", "crates/devflow-core/src/monitor.rs", "crates/devflow-core/src/prompt.rs", "crates/devflow-core/src/state.rs", "doc-check-allowlist.toml", "docs/guides/configuration.md", "docs/guides/unattended-mode.md"]
covered_digest: "v4:sha256:74a30b0ef389a924a7d55a5a0e71909f1b167b0e83a25d312bcd383d94474e31"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: passed
  previous_score: 114/114
  reason: "code-review fix round (9886a95, 7e8ee1a, 982d443, cd35c7a, 0f6c79c, c6e29e0) landed after the 2026-11-08T01:13:18Z report"
  gaps_closed: []
  gaps_remaining: []
  regressions:
    - "The 51-11 docs prohibition no longer holds in full: 9886a95 made `devflow parallel` ignore `devflow.toml`'s `fallback_agent`, and README.md's rate-limit sentence still promises failover whenever the key is set"
gaps:
  - truth: "51-11 prohibition: the operator docs never promise more than the code does (the `devflow.toml` key is described as read by `start` only)"
    status: partial
    reason: "README.md:110 says that when the Code stage is rate-limited and a fallback agent is configured (`--fallback-agent`, or `fallback_agent` in `devflow.toml`), that agent takes over Code. Since 9886a95, `devflow parallel` passes `FallbackSource::NoFallback` and never reads the key, so a parallel run with the key set takes the cron arm. Nothing is printed. The README sentence is false for that case. It was true before the fix round. The prohibition's listed claims still hold: configuration.md says the key is read by `devflow start` only."
    artifacts:
      - path: "README.md"
        issue: "line 110: the fallback sources in parentheses have no `devflow parallel` exception"
      - path: "crates/devflow-cli/src/parallel.rs"
        issue: "line 144: `FallbackSource::NoFallback` drops a configured key silently (review WR-02)"
    missing:
      - "Either name the exception in README.md:110 and in the OPERATIONS.md `devflow parallel` row (parallel arms no fallback and ignores the key), or apply review WR-02's fix: parallel reads the key once, prints a note that it is not armed, and refuses an invalid key as `start` does"
prohibitions_resolved:
  # Judgment-tier (ADR-550 D4). Decided by the verifier (Claude) on the evidence named.
  - statement: "Failover never resets, stashes, cleans or rebases the worktree and never adds a worktree or branch (51-01)"
    verdict: holds
    basis: "hand_off_agent is unchanged by the fix round; a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage passed in this run"
  - statement: "A rate limit at any stage other than Code never hands off or consumes the fallback (51-01)"
    verdict: holds
    basis: "failover_target unchanged (pipeline_launch.rs:3090); failover_target_selects_the_fallback_only_at_code and a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm passed"
  - statement: "A torn capture line is never classified as a rate limit (51-05)"
    verdict: holds
    basis: "c6e29e0 changed wording only; a_torn_final_line_is_not_read_as_a_clean_silence passed"
  - statement: "The fallback never launches on the primary's canary evidence or session id (51-06)"
    verdict: holds
    basis: "the handoff commit still sets canary = None and session_id = None (pipeline_launch.rs:3208-3209); a_handoff_reruns_the_delivery_canary_for_the_new_agent passed"
  - statement: "A handoff never ends recorded as handed off with nothing running and no gate; a post-commit relaunch failure never schedules a resume (51-06)"
    verdict: holds
    basis: "a_handoff_whose_relaunch_fails_ends_at_a_gate and a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate passed. 7e8ee1a changes only what happens after that gate is answered, and the answer now ends with a stopped run and a gate_relaunch_refused event rather than a silent end"
  - statement: "The never-silent gate never weakens Phase 50 D-03's ownership fence (51-06)"
    verdict: holds
    basis: "one non-test FailoverRelaunchGuard::engage( site (pipeline_launch.rs:3266); relaunch_failed_stage writes through `commit`, which is owned; the_supervisor_record_is_cleared_only_on_launch_failure_paths and a_failover_superseded_mid_relaunch_writes_nothing passed"
  - statement: "The notify command never reaches any agent's environment, fallback included (51-09)"
    verdict: holds
    basis: "untouched by the fix round; agent_via_legacy_monitor_never_receives_the_notify_variable and agent_via_default_launch_canary_never_receives_the_notify_variable passed"
  - statement: "No artifact records criterion 3 from a test whose outcome could not have differed (51-10, 51-13)"
    verdict: holds
    basis: "51-SUPV01-EVIDENCE.md unchanged; the_supv01_evidence_is_complete and the_supv01_evidence_trace_rows_are_complete passed (--run-ignored only)"
  - statement: "The operator docs never promise more than the code does (51-11)"
    verdict: does not hold in full
    basis: "OPERATIONS.md's new remedy text was checked at runtime and is accurate: a plain resume is refused, and `resume --agent <other>` continues (see Behavioral Spot-Checks). README.md:110 over-promises for `devflow parallel` with the key set. Recorded as the one gap"
---

# Phase 51: Rate-Limit Agent Failover Verification Report

**Phase Goal:** An agent that exhausts its token budget mid-stage does not end the night. The run continues under a configured fallback agent, in the same worktree on the same branch, with no manual conflict resolution.
**Verified:** 2026-11-08T15:28:35Z, at HEAD `376b645`
**Status:** gaps_found (one narrow docs gap; the goal itself holds)
**Re-verification:** Yes. The previous report (passed, 01:13Z) went stale when the code-review fix round landed.

## Bottom line

- **The phase goal holds** for `devflow start` and `devflow resume` under the stub-agent standard the operator set (R-5). Every must-have truth's named test passed in this verifier's own full run. The fix round did not touch `failover_target`, `fail_over_on_rate_limit`, `hand_off_agent` or the relaunch guard's engagement.
- **One gap, a regression from the fix round:** after 9886a95, `devflow parallel` ignores `devflow.toml`'s `fallback_agent`. README.md:110 still says the key arms failover, with no exception. That breaks the 51-11 prohibition that the operator docs never promise more than the code does. It is a one-sentence README fix, or review WR-02's fix.
- **The two open review warnings, judged:**
  - **Review WR-01** (`status` points to a plain `resume`, which is refused and overwrites `stop_reason`): **reproduced at runtime, but it breaks no must-have.** The handoff still ends at a gate, which is the must-have. OPERATIONS.md says outright that a plain resume is refused, and that `resume --agent <other>` is the way on. This verifier ran that remedy in a scratch copy, and it works. The defect is a misleading hint in `devflow status` on a recovery path, not a broken guarantee.
  - **Review WR-02** (`parallel` silently ignores the key): **breaks no truth, but it does break the 51-11 docs prohibition through README.md:110.** That is the one gap. The code behaviour matches the configuration guide ("read by `devflow start` only") and 51-04's original design ("parallel takes no `--fallback-agent`, so its runs carry none"). No truth requires `parallel` to arm a fallback.

## What this verification does and does not establish

- The limits in the previous report still apply. All failover proof uses stub agents. No live failover run has been made on this build. A real fallback CLI picking up half-done work, the stream path's idle-timeout route into failover, and a run inside a real phase worktree are all unexercised.
- **New in this round, verifier-run, not committed:** a scratch-copy e2e test (`git archive HEAD`, separate target dir, now deleted) drove the WR-02 stop and then ran the documented remedy. It established that a plain `resume` is refused and that `resume --agent claude` hands off, reruns the canary and relaunches Code. It does not establish this for any agent pair other than Antigravity → Claude, or under a live CLI.
- The previous round's mutants (`failover_target` → `None`; guard engagement removed) were **not re-run**. The code they mutate is unchanged since then, and the tests they discriminated passed in this run.
- The fixer's negative controls for the six fix commits were **not re-run** here. They are recorded in 51-REVIEW-FIX.md.

## Goal Achievement

### Roadmap Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | On `RateLimited` with a fallback configured, the run continues under the fallback, in the same worktree on the same branch, and the stage completes (Code only, R-2) | ✓ VERIFIED (stub agents) | `advance_with`'s `RateLimited` arm → `failover_target` → `fail_over_on_rate_limit` (pipeline_launch.rs:4161, 3090, 3253), unchanged by the fix round. `a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage` and binary e2e `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code` passed in this run |
| 2 | Both answers coexist: with a fallback the agents switch, with none the cron instructions are written | ✓ VERIFIED | `a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off`, `a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions` and `a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm` passed |
| 3 | The SUPV-01 dependency is settled by observation | ✓ VERIFIED | `51-SUPV01-EVIDENCE.md` unchanged; both ignored evidence tests passed (`--run-ignored only`, 6/6) |
| 4 | The `n` verb, `rebase_in` and the rebase handoff stay dead | ✓ VERIFIED | `doc_check::the_removed_sequentagent_handoff_stays_dead` passed; the fix round adds none of them |

### Plan Must-Have Truths (110)

A script extracted 108 backticked identifiers from every plan's `must_haves.truths`. 84 of them have a `PASS` line in this verifier's full nextest log. Of the other 24:
- 2 are the ignored SUPV-01 evidence tests, which passed under `--run-ignored only`;
- 15 are production functions, not tests (checked: no `#[test]` above their `fn`);
- 7 are event or field names, or names a plan quotes as examples.

A planted name returned no PASS line, so the match is not vacuous.

| Plan | Truths | Status | Fix-round impact |
|------|--------|--------|------------------|
| 51-01 tracer, `hand_off_agent`, `failover_target`, State fields | 12 | ✓ VERIFIED | none |
| 51-02 removed-handoff scan | 7 | ✓ VERIFIED | none |
| 51-03 idle-timeout cause | 9 | ✓ VERIFIED | none |
| 51-04 `--fallback-agent` on start/resume | 8 | ✓ VERIFIED | `start` now takes a `FallbackSource`. `main.rs:646` passes `FlagOrConfig(flag)`, so start behaves as before. `parallel` passes `NoFallback`, which matches 51-04's own intent |
| 51-05 torn-line classifier | 7 | ✓ VERIFIED (wording deviation, see below) | c6e29e0 changed the clause text |
| 51-06 hardened hop | 13 | ✓ VERIFIED | 7e8ee1a changes only the answer to the gate; the gate itself is unchanged |
| 51-07 audit-fix replay | 6 | ✓ VERIFIED | none |
| 51-09 notify unset on resume | 7 | ✓ VERIFIED | none |
| 51-10 SUPV-01 part 1 | 8 | ✓ VERIFIED | none |
| 51-11 docs + phase gate | 9 | ✓ VERIFIED | OPERATIONS.md remedy rewritten to the new behaviour and checked at runtime. The README prohibition is the gap |
| 51-12 binary e2e Claude→Antigravity | 5 | ✓ VERIFIED (stub agents) | the existing gate test is unchanged and passes |
| 51-13 SUPV-01 part 2 | 10 | ✓ VERIFIED | none |
| 51-14 `devflow.toml` key | 9 | ✓ VERIFIED | cd35c7a/0f6c79c: the key is parsed with `AgentKind::from_str`; a file that does not parse now gives `devflow.toml: <error>`. The truths still hold: a key naming no agent gives an `Err` naming `fallback_agent` and `devflow.toml`, a file that does not parse gives an `Err`, and `a_valid_flag_does_not_mask_an_invalid_config_key` passed |

**51-05 wording deviation (decided, not a gap).** The truth says the idle-timeout reason states "that the capture ends in a torn line so a quota denial cannot be ruled out". Since c6e29e0 the segment reads `torn final line after the last parsed event`. That still says the line is torn, but drops the quota clause. The truth's operative content holds and is tested: the torn segment leads the capped clause, the verdict is never `RateLimited`, and the controls pass. The dropped clause was false for non-Claude captures, so the change narrows a claim rather than weakening a guarantee. OPERATIONS.md:311-315 still tells the operator that a Claude torn line may have been a quota denial, which satisfies 51-11's D-16 docs truth.

**Score:** 114/114 truths verified, 0 present but behavior-unverified. 9 judgment-tier prohibitions: 8 hold and 1 does not hold in full (the gap).

### Required Artifacts

`verify.artifacts` returned 40/40 passed across all 13 plans, judged from the JSON and not the exit code.

### Key Link Verification

The links were checked by hand in the current tree, because each `from` is a symbol, so `verify.key-links` cannot resolve them. All are still wired:
- `RateLimited` arm → `failover_target` → `fail_over_on_rate_limit`: pipeline_launch.rs:4161, 3253
- `FailoverRelaunchGuard::engage(` has one non-test site: :3266
- the handoff commit clears `canary` and `session_id`: :3208-3209
- `handle_infra_outcome`: :3286
- `in_failover_relaunch` is read by `refuse_launch` (:672) and `record_failed_launch` (:2099)
- `commands::start` → `config::fallback_agent`: commands.rs:425, the only production caller

New in the fix round: `handle_stage_failure`'s retry and loop-back arms → `relaunch_failed_stage` (pipeline_outcomes.rs:937, 945) → `recorded_canary_refusal` (pipeline_launch.rs:636). This verifier read that predicate against `canary_gate` (:570-626): stream path only, `Confirmed` admits, and `Absent` and `Unverified` refuse. They are equivalent today. Nothing pins them together (review IN-01).

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Whole workspace | `cargo nextest run --workspace --no-fail-fast` (once, log saved) | 2111 passed, 0 failed, 6 skipped (4 leaky) | ✓ PASS |
| Ignored evidence tests | `cargo nextest run --workspace --run-ignored only --no-tests=fail` | 6/6 passed | ✓ PASS |
| Fix-round tests | taken from the saved log | `parallel_ignores_the_config_key`, both `*_the_failed_canary_gate_stops_the_run_and_names_the_way_on`, `fallback_agent_key_resolves_from_devflow_toml` PASS | ✓ PASS |
| Remedy after the canary stop (scratch copy only) | one added e2e, `-E 'test(=verifier_scratch_remedy_after_the_canary_stop)'` | a plain `resume` exits 1 (canary refusal); `resume --agent claude` exits 0, `agent_handoff` `reason: resume --agent`, claude log shows a fresh `canary` then `stage code` | ✓ PASS (control: the plain resume is the opposite case and failed) |
| Review WR-01 reproduced (same run) | as above | `devflow status` shows `GONE — … stopped`, `→ devflow resume --phase 5977`, `last action: gate_relaunch_refused`. After the plain resume, `stop_reason` = `resume launch failed: background-task notification delivery is ABSENT … claude_code_version 2.1.220 …`, with no `--agent` | confirmed defect, no must-have broken |
| Lint/format | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings` | rc 0 / rc 0 | ✓ PASS |
| Published-crate semver | `cargo semver-checks check-release -p devflow-core` | no semver update required | ✓ PASS |

### Probe Execution

None. No plan declares a `probe-*.sh`.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|----------|
| SUPV-02 | all 13 plans | ✓ SATISFIED | criteria 1-4 above. REQUIREMENTS.md maps only SUPV-02 to Phase 51 (line 269), so nothing is orphaned |

### Anti-Patterns Found

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`todo!`/`unimplemented!` appears on any added line, either in the fix round (`e9edade..HEAD`) or in the whole phase (`194da83^..HEAD`). A planted `TODO` line was caught by the same pattern.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| README.md | 110 | the fallback sources have no `devflow parallel` exception | 🛑 Gap (docs prohibition) | an operator who sets the key and uses `parallel` expects failover and gets the cron arm |
| crates/devflow-cli/src/parallel.rs | 144 | `NoFallback` drops a configured key, even an invalid one, with no output (review WR-02) | ⚠️ Warning | the cause of the README gap. Also against the project's fail-closed observability rule |
| crates/devflow-cli/src/commands.rs | `recovery_hints` | a run stopped by a canary refusal gets the plain `resume` hint; the refused resume then overwrites `stop_reason` (review WR-01, reproduced) | ⚠️ Warning | misleading recovery hint; the docs give the right way on |
| crates/devflow-cli/src/pipeline_launch.rs | 636 | `recorded_canary_refusal` duplicates `canary_gate`'s mapping, with no test pinning the two (review IN-01) | ℹ️ Info | equivalent today |
| crates/devflow-cli/src/pipeline_outcomes.rs | 962-969 | a retry whose canary was never recorded can still end silently (review IN-02, the residual of 999.187) | ℹ️ Info | reachable only through a transient failure between two identical checks |
| crates/devflow-core/src/monitor.rs | 2780-2794 | the torn-line warning names Claude on every agent's run (review IN-04) | ℹ️ Info | log wording only |

### Human Verification Required

None that the evidence leaves open. The one decision is how to close the gap: fix the docs, make `parallel` loud, or accept the README wording with an override.

### Gaps Summary

One gap, and it is a regression from the fix round. 9886a95 made `devflow parallel` ignore `devflow.toml`'s `fallback_agent`. That restores 51-04's design and matches the configuration guide. But README.md:110 still promises failover whenever the key is set, so the 51-11 "docs never promise more than the code does" prohibition no longer holds in full. Closing it takes either a README clause plus a note in the OPERATIONS.md `parallel` row, or review WR-02's loud-`parallel` fix. If the operator considers the configuration guide's "read by `devflow start` only" sufficient, the alternative is an override:

```yaml
overrides:
  - must_have: "The operator docs never promise more than the code does"
    reason: "README.md:110 is a summary; configuration.md states the key is read by devflow start only"
    accepted_by: "{name}"
    accepted_at: "{ISO timestamp}"
```

Everything else holds. The goal is met for `start` and `resume`, every must-have truth's test passed, and the documented remedy after a failed fallback canary works when run.

---

_Verified: 2026-11-08T15:28:35Z_
_Verifier: Claude (gsd-verifier)_
