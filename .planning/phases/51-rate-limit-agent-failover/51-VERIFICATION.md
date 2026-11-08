---
phase: 51-rate-limit-agent-failover
verified: 2026-11-08T01:13:18Z
status: passed
score: 114/114 must-haves verified (4 roadmap success criteria + 110 plan truths)
covered_files: [".planning/phases/51-rate-limit-agent-failover/51-01-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-01-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-02-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-02-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-03-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-03-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-04-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-04-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-05-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-05-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-06-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-06-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-07-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-07-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-09-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-09-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-10-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-10-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-11-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-11-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-12-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-12-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-13-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-13-SUMMARY.md", ".planning/phases/51-rate-limit-agent-failover/51-14-PLAN.md", ".planning/phases/51-rate-limit-agent-failover/51-14-SUMMARY.md", "OPERATIONS.md", "README.md", "crates/devflow-cli/src/commands.rs", "crates/devflow-cli/src/main.rs", "crates/devflow-cli/src/parallel.rs", "crates/devflow-cli/src/pipeline_gate.rs", "crates/devflow-cli/src/pipeline_launch.rs", "crates/devflow-cli/src/pipeline_outcomes.rs", "crates/devflow-cli/src/preflight.rs", "crates/devflow-cli/src/test_support.rs", "crates/devflow-cli/tests/failover_e2e.rs", "crates/devflow-cli/tests/fallback_agent_config.rs", "crates/devflow-cli/tests/gate_notify_persist_e2e.rs", "crates/devflow-core/src/agent_result.rs", "crates/devflow-core/src/config.rs", "crates/devflow-core/src/doc_check.rs", "crates/devflow-core/src/monitor.rs", "crates/devflow-core/src/prompt.rs", "crates/devflow-core/src/state.rs", "doc-check-allowlist.toml", "docs/guides/configuration.md", "docs/guides/unattended-mode.md"]
covered_digest: "v4:sha256:5fb8250e12490d703c4743383bde5a79cd4c56fb7fb649fcc77a2275b8527103"
behavior_unverified: 0
overrides_applied: 0
prohibitions_resolved:
  # Judgment-tier (ADR-550 D4). Decided by the verifier (Claude) on the evidence named;
  # recorded explicitly, not absorbed silently. None was found broken.
  - statement: "Failover never resets, stashes, cleans or rebases the worktree and never adds a worktree or branch (51-01)"
    verdict: holds
    basis: "hand_off_agent runs only `git status --porcelain` (read); a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage asserts HEAD^ == primary commit, branch and worktree lists unchanged; passed"
  - statement: "A rate limit at any stage other than Code never hands off or consumes the fallback (51-01)"
    verdict: holds
    basis: "failover_target requires stage == Stage::Code; failover_target_selects_the_fallback_only_at_code and a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm passed"
  - statement: "A torn capture line is never classified as a rate limit (51-05)"
    verdict: holds
    basis: "capture_shows_rate_limit_denial returns true only for CaptureDenial::Denied; a_torn_final_line_is_not_read_as_a_clean_silence passed"
  - statement: "The fallback never launches on the primary's canary evidence or session id (51-06)"
    verdict: holds
    basis: "the handoff commit sets canary = None and session_id = None; a_handoff_reruns_the_delivery_canary_for_the_new_agent passed"
  - statement: "A handoff never ends recorded as handed off with nothing running and no gate; a post-commit relaunch failure never schedules a resume (51-06)"
    verdict: holds
    basis: "a_handoff_whose_relaunch_fails_ends_at_a_gate passed; verifier re-ran the old-ordering mutant (guard engagement removed) in a scratch copy and the test FAILED with supervisor_superseded recorded:null, as 51-06 records. Adjacent, not covered by this statement: a retry answered at that gate ends ungated (review WR-02, backlog 999.187)"
  - statement: "The never-silent gate never weakens Phase 50 D-03's ownership fence (51-06)"
    verdict: holds
    basis: "one non-test FailoverRelaunchGuard::engage( site (pipeline_launch.rs:3240), pinned by the_supervisor_record_is_cleared_only_on_launch_failure_paths; a_failover_superseded_mid_relaunch_writes_nothing passed"
  - statement: "The notify command never reaches any agent's environment, fallback included (51-09)"
    verdict: holds
    basis: "agent_via_legacy_monitor_never_receives_the_notify_variable and agent_via_default_launch_canary_never_receives_the_notify_variable passed; every launch goes through spawn_monitor"
  - statement: "No artifact records criterion 3 from a test whose outcome could not have differed (51-10, 51-13)"
    verdict: holds
    basis: "51-SUPV01-EVIDENCE.md pairs each experiment with an intervention-removed run and an ordinary-launch control; the_supv01_evidence_is_complete and the_supv01_evidence_trace_rows_are_complete passed (run-ignored); the one untestable read (4) is recorded as a gap and the post-save error arm as established by reading (verifier re-read successor_runs_on: Err returns true)"
  - statement: "The operator docs never promise more than the code does (51-11)"
    verdict: holds, with a caveat
    basis: "configuration.md and OPERATIONS.md describe failover as one hop at Code that can end on the cron arm or at a Code gate, and the key as read by `devflow start` only, which matches start. The docs say nothing about `devflow parallel`, which since 51-14 also arms the key through start; the false statement is a code comment in parallel.rs (review WR-01), not the operator docs"
---

# Phase 51: Rate-Limit Agent Failover Verification Report

**Phase Goal:** An agent that exhausts its token budget mid-stage does not end the night — the run continues under a configured fallback agent, in the same worktree on the same branch, with no manual conflict resolution.
**Verified:** 2026-11-08T01:13:18Z, at HEAD `e9edade` (last code commit `1e61089`; everything after it touches `.planning/` only)
**Status:** passed
**Re-verification:** No — initial verification

## What this verification does and does not establish

- **Established by stub-agent tests only.** Every failover proof uses shell stubs for `claude`, `codex` and `agy`: the in-process supervisor rig (51-01, 51-06, 51-07, 51-10, 51-13) and the real `devflow` binary driven end to end (51-12, `tests/failover_e2e.rs`). The stubs replay event shapes copied from DevFlow's own fixtures (the rejected `rate_limit_event` is the archived live event with its status changed), not from a live CLI.
- **No live failover run has been made on this build.** The one natural rate limit (Phase 51's own dogfood run, 2026-11-06, `51-SUPV01-EVIDENCE.md` § D-13) ran on 3.0.0, before the failover code existed, and took the cron arm. It shows that a real Claude limit is classified `RateLimited`, on the pre-51-05 classifier.
- **Not exercised anywhere:**
  - a real fallback CLI picking up a half-done Code stage;
  - the stream path's idle-timeout route into failover (the binary e2e's Claude stub exits after its rate-limit event);
  - a run inside a real phase worktree (the rigs use `--no-worktree` or the project root; `hand_off_agent` does not write `worktree_path`, so the relaunch uses the same root, but no test drives that).
- **Judgment: no must-have is left unverified by this.** Operator ruling R-5 (D-13, `51-CONTEXT.md`) sets stub-agent proof as the standard for criterion 1, and every must-have is phrased as a stub test. The residual risk is outside every must-have's wording, and the operator has accepted it. The failover branch is keyed only on the classified status `RateLimited`, so the live-classification evidence and the stub-driven failover compose. They do not prove that a real fallback agent finishes real interrupted work.

## Goal Achievement

### Roadmap Success Criteria

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | On `RateLimited` with a fallback configured, the run continues under the fallback in the same worktree on the same branch, with no manual conflict resolution, and the resumed stage completes normally (Code only, R-2) | ✓ VERIFIED (stub agents) | `advance_with`'s explicit `RateLimited` arm → `failover_target` (Code, unused, ≠ agent) → `fail_over_on_rate_limit` → `hand_off_agent` → `launch_stage` (pipeline_launch.rs:4135, 3064, 3227, 3141). `a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage` passed: 1 `agent_handoff`, `workflow_finished` after it, fallback commit's parent = primary's commit, branch/branches/worktrees unchanged, no cron. Binary e2e `a_rate_limited_claude_run_in_auto_mode_hands_off_to_antigravity_and_completes_code` passed. **Verifier mutant:** `failover_target` forced to `None` makes the 51-01 test FAIL on "one handoff"; the unmutated scratch copy passes |
| 2 | Both answers coexist: fallback → switch; no fallback → cron instructions | ✓ VERIFIED | `a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off` and `a_rate_limited_claude_run_in_auto_mode_without_a_fallback_writes_cron_instructions` passed alongside the fallback tests; Plan-with-fallback control `a_rate_limit_at_plan_with_a_fallback_takes_the_cron_arm` passed |
| 3 | SUPV-01 dependency settled by observation | ✓ VERIFIED | `51-SUPV01-EVIDENCE.md` has a 7-read trace, six withheld-read experiments each with ordinary-launch control and intervention-removed run, and a per-arm conclusion. `the_supv01_evidence_is_complete` and `the_supv01_evidence_trace_rows_are_complete` passed (`--run-ignored only`). Limits are stated in the file (substituted answers, one machine, read (4) a gap) |
| 4 | The `n` verb, `rebase_in` and the rebase handoff stay dead, counted over comment-stripped source | ✓ VERIFIED | `doc_check::the_removed_sequentagent_handoff_stays_dead` passed (planted positives and decoys). Independent second count: 3 raw `rebase_in` lines under `crates/`, all comments or doc_check plants; the only non-comment `sequentagent` is ship.rs:940's `assert!(!…contains("sequentagent"))` |

### Plan Must-Have Truths (110)

Every test named in a plan's must_haves was extracted, 97 names plus 4 with digits. Each one is declared as `fn <name>(` in `crates/`; a planted name returned absent. Every test name passed in this verifier's own run. The 5 extracted names with no PASS line are production functions, not tests (`fail_over_on_rate_limit`, `prepare_loop_back_to_code`, `detect_claude_stream_rate_limit`, `build_single_agent_cron_instructions`, `refuse_launch_over_a_live_run`).

| Plan | Truths | Status | Notes |
|------|--------|--------|-------|
| 51-01 failover tracer, `hand_off_agent`, `failover_target`, State fields | 12 | ✓ VERIFIED | code read + tests; mutant A above |
| 51-02 removed-handoff scan | 7 | ✓ VERIFIED | |
| 51-03 idle-timeout cause (999.102) | 9 | ✓ VERIFIED | monitor.rs:2774 reads `idle_timeout_cause` before `write_idle_timeout_record` (2799) |
| 51-04 `--fallback-agent` on start/resume | 8 | ✓ VERIFIED | |
| 51-05 torn-line classifier (999.71) | 7 | ✓ VERIFIED | monitor.rs:2747/2779 three-way answer |
| 51-06 hardened hop | 13 | ✓ VERIFIED | old-ordering mutant reproduced by verifier (guard removed → FAIL, `supervisor_superseded` `recorded:null`) |
| 51-07 audit-fix replay | 6 | ✓ VERIFIED | |
| 51-09 notify unset on resume (999.154) | 7 | ✓ VERIFIED | WR-13 mutant run is SUMMARY-recorded, not re-run |
| 51-10 SUPV-01 part 1 | 8 | ✓ VERIFIED | seam gated `cfg(any(test, feature="test-support"))`; `test-support` enabled only under devflow-cli `[dev-dependencies]` |
| 51-11 docs + phase gate | 9 | ✓ VERIFIED | verifier re-ran full nextest, fmt, clippy `-D warnings`, semver-checks, deny bans/licenses/sources, machete (all exit 0); container parity and the advisory scan are SUMMARY-recorded only |
| 51-12 binary e2e Claude→Antigravity | 5 | ✓ VERIFIED (stub agents) | |
| 51-13 SUPV-01 part 2 | 10 | ✓ VERIFIED | R2-8 error arm re-read: `successor_runs_on` `Err` → `true` |
| 51-14 `devflow.toml` key | 9 | ✓ VERIFIED | persisted-key mutation run is SUMMARY-recorded, not re-run |

**Score:** 114/114 truths verified (0 present, behavior-unverified). 10 judgment-tier prohibitions were resolved on evidence (9 entries: the identical 51-10 and 51-13 statements are merged); see `prohibitions_resolved` in the frontmatter. None was found broken.

### Required Artifacts

`gsd-tools query verify.artifacts`: 40/40 artifacts passed across all 13 plans (exists, contains pattern).

### Key Link Verification

`verify.key-links` could not evaluate any link: every `from` is a symbol, not a file path ("Source file not found"). The verifier checked each link's pattern in the right file, in non-test code. All 24 links are wired, among them:

| From | To | Status | Evidence |
|------|----|--------|----------|
| `advance_with` RateLimited arm | `failover_target` → `fail_over_on_rate_limit` | WIRED | pipeline_launch.rs:4135-4151; `_ =>` wildcard calls `handle_rate_limited_outcome` only |
| `resume_loaded` | `hand_off_agent` (`resume --agent`) | WIRED | pipeline_launch.rs:3548 |
| `hand_off_agent` commit | `canary_gate` (canary cleared) | WIRED | pipeline_launch.rs:3182 |
| `fail_over_on_rate_limit` | `handle_infra_outcome` | WIRED | pipeline_launch.rs:3260 |
| `FailoverRelaunchGuard` | `refuse_launch` / `record_failed_launch` | WIRED | `in_failover_relaunch` at 649, 2074 |
| `prepare_loop_back_to_code` | `State.code_fix` | WIRED | pipeline_gate.rs:205 |
| `commands::start` | `config::fallback_agent` → `validate_fallback_agent` → `state.fallback_agent` | WIRED | commands.rs:410-431 |
| `monitor::fire_idle_timeout` | `capture_rate_limit_denial`, `idle_timeout_cause` | WIRED | monitor.rs:2747, 2774 |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Whole workspace | `cargo nextest run --workspace --no-fail-fast` | 2108 passed, 0 failed, 6 skipped (4 leaky, pre-existing reaper tests) | ✓ PASS |
| Ignored evidence checks | `cargo nextest run --workspace --run-ignored only` | 6/6 passed incl. `the_supv01_evidence_is_complete` | ✓ PASS |
| Failover detected by test | mutant A (`failover_target` → `None`), one named test in scratch copy | FAIL on "one handoff"; unmutated control PASS | ✓ PASS (discriminates) |
| Relaunch-failure gate detected | mutant B (guard engagement removed) | FAIL, `supervisor_superseded` `recorded:null` | ✓ PASS (discriminates) |
| Lint/format | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings` | rc 0 / rc 0 | ✓ PASS |
| Published-crate semver | `cargo semver-checks check-release -p devflow-core` | 229 pass, no semver update required | ✓ PASS |
| Supply chain | `cargo deny check bans licenses sources`; `cargo machete` | ok / rc 0 | ✓ PASS |

### Probe Execution

None: no plan declares a `probe-*.sh`, and the phase is not a migration phase.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|----------|
| SUPV-02 | all 13 plans | ✓ SATISFIED | criteria 1-4 above; REQUIREMENTS.md maps only SUPV-02 to Phase 51, so nothing is orphaned |

Backlog folds named in CLAUDE.md: 999.102 → 51-03, 999.71 (classifier half; measurement stays in backlog by R-4) → 51-05, 999.154 → 51-09. All three are delivered and tested.

### Anti-Patterns Found

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`todo!`/`unimplemented!` is on any line the phase added (`194da83^..HEAD`, 22 files). A planted control line was caught by the same scan.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| crates/devflow-cli/src/parallel.rs | 130-143 | comment says `parallel` runs carry no fallback; `start(.., None)` now resolves `devflow.toml`'s key, and a refusal names `--fallback-agent`, which `parallel` lacks (review WR-01, open) | ⚠️ Warning | `devflow parallel` is refused per phase when the key equals that phase's agent, or in auto mode for a non-claude/antigravity key. Affects only projects that set the new key. No must-have covers `parallel` |
| crates/devflow-cli/src/pipeline_outcomes.rs | 933-946 | the post-failover Code gate offers retry/loop-to-code, which relaunch outside the guard and end ungated (review WR-02 = backlog 999.187) | ⚠️ Warning | the handoff itself ends at a gate (verified); an operator's retry answer can then leave the run not live with no gate. Documented in OPERATIONS.md and filed |
| crates/devflow-core/src/config.rs | 524-527 | a `devflow.toml` read error other than NotFound logs `tracing::warn!` and returns `Ok(None)` | ℹ️ Info | e.g. permission denied silently arms no fallback, unlike the parse-error path, which refuses |

### Human Verification Required

None. Every human-shaped question was settled from evidence (see the prohibitions above). The one question that cannot be tested hermetically, whether a live fallback agent finishes real interrupted work, was ruled out of scope by the operator (R-5).

### Gaps Summary

No gaps. The goal holds under the stub-agent standard the operator set (R-5). Two open review warnings sit adjacent to it. WR-01 is a `devflow parallel` regression from 51-14's key. WR-02 is the post-gate retry dead end, backlog 999.187. Neither falsifies a must-have.

---

_Verified: 2026-11-08T01:13:18Z_
_Verifier: Claude (gsd-verifier)_
