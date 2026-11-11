---
phase: 52-subtract-and-stabilize
verified: 2026-11-11T19:42:44Z
status: human_needed
score: 4/5 roadmap success criteria verified, 1 uncertain (12/12 requirements satisfied)
covered_files: [".gitignore",".planning/phases/52-subtract-and-stabilize/52-01-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-01-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-02-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-02-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-03-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-03-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-04-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-04-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-05-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-05-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-06-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-06-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-07-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-07-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-08-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-08-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-09-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-09-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-10-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-10-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-11-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-11-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-12-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-12-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-13-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-13-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-14-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-14-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-15-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-15-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-16-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-16-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-17-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-17-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-18-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-18-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-19-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-19-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-20-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-20-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-21-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-21-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-22-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-22-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-23-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-23-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-24-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-24-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-25-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-25-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-26-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-26-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-27-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-27-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-28-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-28-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-29-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-29-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-30-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-30-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-31-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-31-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-32-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-32-SUMMARY.md",".planning/phases/52-subtract-and-stabilize/52-33-PLAN.md",".planning/phases/52-subtract-and-stabilize/52-33-SUMMARY.md",".workspace-divergence","ARCHITECTURE.md","CLAUDE.md","CONTRIBUTING.md","Cargo.lock","Cargo.toml","OPERATIONS.md","README.md","crates/devflow-cli/Cargo.toml","crates/devflow-cli/src/commands.rs","crates/devflow-cli/src/config_parse.rs","crates/devflow-cli/src/main.rs","crates/devflow-cli/src/phase_worktree.rs","crates/devflow-cli/src/pipeline_gate.rs","crates/devflow-cli/src/pipeline_launch.rs","crates/devflow-cli/src/pipeline_outcomes.rs","crates/devflow-cli/src/preflight.rs","crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap","crates/devflow-cli/src/staleness.rs","crates/devflow-cli/src/test_support.rs","crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs","crates/devflow-cli/tests/build_provenance.rs","crates/devflow-cli/tests/ci_parity_guards.rs","crates/devflow-cli/tests/common/mod.rs","crates/devflow-cli/tests/cut_release_guard.rs","crates/devflow-cli/tests/doctor_json_e2e.rs","crates/devflow-cli/tests/failover_e2e.rs","crates/devflow-cli/tests/fallback_agent_config.rs","crates/devflow-cli/tests/gate_notify_persist_e2e.rs","crates/devflow-cli/tests/gate_sweep_e2e.rs","crates/devflow-cli/tests/gate_wedge_e2e.rs","crates/devflow-cli/tests/gitignore_coverage.rs","crates/devflow-cli/tests/help_snapshot.rs","crates/devflow-cli/tests/hermetic_spawn_sites.rs","crates/devflow-cli/tests/log_format_env.rs","crates/devflow-cli/tests/pending_supervisor_e2e.rs","crates/devflow-cli/tests/phase7_cli.rs","crates/devflow-cli/tests/recover_clean_e2e.rs","crates/devflow-cli/tests/snapshots/devflow-help.txt","crates/devflow-cli/tests/start_lock_e2e.rs","crates/devflow-cli/tests/stop_e2e.rs","crates/devflow-cli/tests/supervisor_e2e.rs","crates/devflow-cli/tests/test_home_isolation.rs","crates/devflow-cli/tests/workspace_version_pin.rs","crates/devflow-core/Cargo.toml","crates/devflow-core/src/agent_result.rs","crates/devflow-core/src/agents/claude.rs","crates/devflow-core/src/agents/codex.rs","crates/devflow-core/src/agents/mod.rs","crates/devflow-core/src/config.rs","crates/devflow-core/src/doc_check.rs","crates/devflow-core/src/events.rs","crates/devflow-core/src/gates.rs","crates/devflow-core/src/git.rs","crates/devflow-core/src/gsd_reader.rs","crates/devflow-core/src/history.rs","crates/devflow-core/src/lib.rs","crates/devflow-core/src/mode.rs","crates/devflow-core/src/monitor.rs","crates/devflow-core/src/outcome_policy.rs","crates/devflow-core/src/prompt.rs","crates/devflow-core/src/recover.rs","crates/devflow-core/src/reset_time.rs","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap","crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap","crates/devflow-core/src/stage.rs","crates/devflow-core/src/state.rs","crates/devflow-core/src/supervisor.rs","crates/devflow-core/src/supervisor/protocol.rs","crates/devflow-core/src/supervisor/runtime.rs","crates/devflow-core/src/test_support.rs","crates/devflow-core/src/workflow.rs","crates/devflow-core/src/worktree.rs","crates/devflow-core/tests/devflow_dir_gitignore.rs","devflow.toml","doc-check-allowlist.toml","docs/architecture/agent-model.md","docs/architecture/overview.md","docs/architecture/state-machine.md","docs/diagrams/agent-lifecycle.md","docs/diagrams/module-map.md","docs/diagrams/system.md","docs/guides/configuration.md","docs/guides/quickstart.md","docs/guides/unattended-mode.md","docs/index.md","docs/mkdocs.yml","scripts/cut-release.sh","scripts/hooks/post-commit","skills-lock.json","tools/release-preflight/Cargo.toml","tools/release-preflight/src/main.rs","tools/release-preflight/tests/preflight.rs"]
covered_digest: "v4:sha256:88bc5712d84700824e10da9818b74a9c5a86550bf5932e503ee5c78fc547903c"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 5/5
  gaps_closed:
    - "Backstop truth 52-08: a run started with --no-worktree finishes the same way (now pinned by supervisor_e2e::a_no_worktree_run_that_passes_validate_finishes_the_same_way, passed)"
    - "Backstop truth 52-18: removed-surface findings are reported sorted by path then line (now pinned by doc_check::removed_surface_findings_are_sorted_by_path_then_line, passed)"
    - "Awareness item: open deferred items had no 999.x backlog entry (all 19 entries now resolved or filed as 999.225-229)"
  gaps_remaining: []
  regressions: []
human_verification:
  - test: "Re-run `timeout 1800 scripts/check.sh all` on 10e1c5b once memory allows (an AI can do this; the verifier was told not to restart it)"
    expected: "exit 0, all OK"
    why_human: "The verifier's one full run was killed by Claude Code's low-memory reaper during the workspace test step (host had ~2 GiB free, used by other processes). fmt and clippy -D warnings passed on this tree; 491 devflow-bin unit tests reported ok before the kill; the 4 that printed FAILED at the moment of the kill each pass by name. The workspace suite as a whole was not completed on 10e1c5b. The last complete green run is the previous report's, on ce96abb; the diff since then is two added test functions plus planning docs."
  - test: "Policy sign-off, nothing found: judgment-tier prohibition 52-08 (finishing never merges, pushes, tags, deletes or resets the branch or worktree, and runs no after-ship hook)"
    expected: "Confirm the verifier's reading"
    why_human: "ADR-550 D4: a judgment-tier prohibition is never passed silently. finish_run (pipeline_gate.rs) makes no git or worktree call; production code is unchanged since the previous report. Both the worktree finish test and the new --no-worktree finish test passed; the latter asserts no workflow_shipped or hook_run event, the feature branch kept and HEAD still on it. No violation found."
  - test: "Policy sign-off, nothing found: judgment-tier prohibition 52-22 (no gate, finish or event says passed unless GSD's reader says passed; a reader failure is never a pass)"
    expected: "Confirm the verifier's reading"
    why_human: "ADR-550 D4. Production code unchanged since the previous report, where supervisor_e2e::a_gaps_found_report_never_reads_as_passed, the never-silent reader-failure gate test and the can_finish proptest passed. This run re-ran pipeline_outcomes::tests::an_ambiguous_verdict_gates_at_once_in_auto_over_every_status and an_agent_fail_over_a_passed_report_gates_as_a_disagreement by name: both passed. No violation found."
  - test: "Policy sign-off, nothing found: judgment-tier prohibition 52-22 (DevFlow never parses VERIFICATION.md itself)"
    expected: "Confirm the verifier's reading"
    why_human: "ADR-550 D4. The verdict comes from gsd_reader::verification_status. Production code reads the report only for its modified time. agent_result::phase_verification_fingerprint and _mtime_nanos still hash it but have no production caller; they are now filed for removal as backlog 999.226. No violation found."
  - test: "Policy sign-off, nothing found: judgment-tier prohibition 52-23 (turning tracing off by default never hides a never-silent diagnostic)"
    expected: "Confirm the verifier's reading"
    why_human: "ADR-550 D4. Unchanged since the previous report: class-(c) sites have named stderr tests that passed there; class-(a)/(b) dispositions are the 52-23 SUMMARY's site-by-site reading, not repeated site by site by the verifier. No violation found in what was checked."
  - test: "Policy sign-off, nothing found: judgment-tier prohibition 52-26 (the operator's note is never paraphrased, summarized, truncated or reordered)"
    expected: "Confirm the verifier's reading"
    why_human: "ADR-550 D4. Unchanged since the previous report, where the byte-identity unit tests (with the pre-fix-builder control) and the gate_wedge_e2e stream and capture tests passed. No violation found."
  - test: "Policy sign-off, nothing found: judgment-tier prohibition 52-31 (the reset timer answers only rate-limit gates, at most once per gate, and at most one automatic relaunch per run)"
    expected: "Confirm the verifier's reading"
    why_human: "ADR-550 D4. Unchanged since the previous report: rate_limit_timer_deadline refuses non-RateLimit gates and a spent flag; failover_e2e::a_passed_reset_relaunches_once_and_a_second_denial_waits passed there. No violation found."
  - test: "Manual close-out (operator-only): check your own .devflow/ state for 3.x leftovers (cron-instructions-NN.json, last-ship*.json, lock-*) and any leftover hermes cron jobs"
    expected: "None remain, or they are removed by hand"
    why_human: "Operator's machine state outside the repo; the D-20 no-migration rule means the binary will not clean them."
---

# Phase 52: Subtract and Stabilize — Verification Report

**Phase goal:** A run is Plan → Code → Validate and ends there as finished, with no Define, no Ship, no after-ship hooks, no cron-resume file, no `devflow ship` and none of the confirmed cut rows, on a tree where `scripts/check.sh all` is green. The supervise gate loop carries the operator's rejection note to the relaunched agent verbatim and can drive a `gaps_found` verification to `passed` from inside the run. `--version`, `cleanup`, `status`, `doctor` and the command output tell the operator what the run did and why it stopped.
**Verified:** 2026-11-11T19:42:44Z, tree 10e1c5b
**Status:** human_needed. Nothing was found against the goal. One item is real unfinished verification: the full `scripts/check.sh all` run on this tree was killed for low host memory before the test step finished. The rest are policy sign-offs and one operator-machine check.
**Re-verification:** Yes. The previous report (ce96abb, human_needed, no gaps) was stale because `crates/devflow-cli/tests/supervisor_e2e.rs` and `crates/devflow-core/src/doc_check.rs` changed.

## What changed since the previous report

`git diff --stat ce96abb..10e1c5b`: four files. Two are code, both test-only additions:
- `doc_check.rs` +45: `removed_surface_findings_are_sorted_by_path_then_line`.
- `supervisor_e2e.rs` +110: `a_no_worktree_run_that_passes_validate_finishes_the_same_way`.
- `.planning/ROADMAP.md` (999.225-229) and `deferred-items.md` (statuses).

No production source changed, so the previous report's evidence for production behaviour still applies. Note: the previous report's `verified:` stamp (22:30Z) is later than this run's wall clock (19:42Z); the earlier stamp appears to be wrong, not this one.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Dry run lists Plan → Code → Validate only; a completed Validate records `workflow_finished` with `shipped: false`, reports finished, keeps worktree and branch; cut surface rejected by clap and absent from `--help`; 3.0.0 control recorded | ✓ VERIFIED | Production code unchanged since the previous report's direct observations (dry run, clap rejections, help snapshot). Re-run by name on 10e1c5b: `supervisor_e2e::a_run_that_passes_validate_finishes_and_keeps_its_worktree` PASS, and the new `a_no_worktree_run_that_passes_validate_finishes_the_same_way` PASS (same terminal payload, branch kept, `worktree_path` none, deregistered). |
| 2 | `scripts/check.sh all` passes on the cut tree; a comment-stripped source scan finds no removed literal and fails on a plant | ? UNCERTAIN | Scan half verified on 10e1c5b: `doc_check::no_removed_surface_literal_remains` and `docs_site_describes_only_the_kept_pipeline` PASS by name. check.sh half not completed: my single run passed `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` (the script is `set -euo pipefail` and reached the test step), then was killed by the low-memory reaper inside the devflow-bin unit tests (491 ok so far). The 4 tests printed FAILED at the kill each pass by name (below). The last full green run is the previous report's, on ce96abb. |
| 3 | Rate limit with no fallback waits at a gate naming the reset time and the exact resume command; relaunches once after reset; no reset or a second denial leaves the gate open; no cron file | ✓ VERIFIED | Production code unchanged; the previous report's failover_e2e tests and code reading stand. Not re-run this round. |
| 4 | `gate reject --note` reaches the relaunched agent verbatim; "abort" in a note does not abort; Validate reject runs only fix work; `gaps_found` driven to `passed` in-run; gate never says passed over another status; `gate show` untruncated, way-on first; three no-progress gap rounds force the gate | ✓ VERIFIED | Production code unchanged; previous report's named tests stand. Re-run this round: `pipeline_outcomes::tests::an_ambiguous_verdict_gates_at_once_in_auto_over_every_status`, `an_agent_fail_over_a_passed_report_gates_as_a_disagreement`, `approving_a_code_stage_failure_over_an_old_passed_report_retries_code`, `approving_a_failure_cap_gate_resets_the_cap_and_follows_the_table`: all PASS. |
| 5 | `--version` shows commit and dirty flag; cleanup, status, start, tracing, doctor, provenance and missing-remote output as specified | ✓ VERIFIED | Production code unchanged; the previous report's direct observations and named tests stand. Not re-run this round. |

**Score:** 4/5 verified, 1 uncertain (0 present but behavior-unverified).

**Limits of this evidence:**
- Truths 3 and 5, and most of 1 and 4, rest on the previous round's runs at ce96abb. That carry-over is sound only because no production file changed (checked with `git diff ce96abb..10e1c5b -- crates/`). It is not a fresh run on 10e1c5b.
- The in-run gap loop is still proven only with a stub agent and stub `gsd_run`; the real GSD side was shown once, on a toy project (52-GAP-LOOP-VERIFICATION.md).
- The rate-limit timer is still proven with computed reset times, not a real quota denial (999.212).

### Backstop truths (the two the previous report left open)

| Plan | Truth | Status | Evidence |
|------|-------|--------|----------|
| 52-08 | A `--no-worktree` run finishes the same way: branch kept, state recorded finished, nothing deleted | ✓ VERIFIED | `supervisor_e2e::a_no_worktree_run_that_passes_validate_finishes_the_same_way` PASS (0.58s). It drives the real binary through a passing Validate with `--no-worktree` and asserts: one `workflow_finished` `{"reason":"finished","shipped":false}`; one notify after it; no `workflow_shipped`/`hook_run`; state kept with `stopped` and `finished` true; `worktree_path` none (the worktree sibling's helper expects it set, so this proves the flag took effect); `feature/phase-95` exists and HEAD is on it; the registry entry is gone, with a control that the registry dir exists. |
| 52-18 | Findings are reported sorted by path then line | ✓ VERIFIED | `doc_check::removed_surface_findings_are_sorted_by_path_then_line` PASS. It calls the real `removed_surface_findings` with input in the order b.rs:5, z.rs:1, b.rs:2 and expects b.rs:2, b.rs:5, z.rs:1, with a count of 3 so an empty result cannot pass. The function's only ordering step is `findings.sort()` (doc_check.rs L2314). Without it, output follows input order, which differs from the expected list. This control is by reading: I did not run a mutant (no repo writes allowed, and the host was short of memory for a scratch build). |

Doctor row order (52-23, the third backstop) was directly observed in the previous round; doctor code is unchanged.

### Plan must-haves (33 plans)

- **Artifacts:** `verify.artifacts` re-run on all 33 plans: 31 all-pass, the same two expected misses as before. 52-11 wants a literal `"ship"` in `phase7_cli.rs`, which 52-18 split with `concat!` on purpose. 52-30 names `notify_context_is_capped_at_300_chars`, renamed after 52-33 raised the cap to 450.
- **Key links:** unchanged since the previous round; no production file moved.

### Behavioral Spot-Checks (this round)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| No-worktree finish | `cargo nextest run -p devflow --test supervisor_e2e --no-tests=fail -E 'test(=a_no_worktree_run_that_passes_validate_finishes_the_same_way)'` | 1 passed | ✓ PASS |
| Sorted findings | `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::removed_surface_findings_are_sorted_by_path_then_line)'` | 1 passed | ✓ PASS |
| Removed-surface scan | `... -E 'test(=doc_check::no_removed_surface_literal_remains)'` and `docs_site_describes_only_the_kept_pipeline` | 1 passed each | ✓ PASS |
| Worktree finish | `... -E 'test(=a_run_that_passes_validate_finishes_and_keeps_its_worktree)'` | 1 passed | ✓ PASS |
| Tests printed FAILED at the kill | 4 `pipeline_outcomes::tests::*` by name | 1 passed each | ✓ PASS |
| Full suite | `timeout 1800 scripts/check.sh all` | fmt OK, clippy OK, test step killed (low memory) | ? SKIP |
| Covered-input digest | `verification.fingerprint` | `v4:sha256:88bc…` vs previous `80eb…`; same 165 files | changed, as expected |

`git status --porcelain` was identical before and after the runs.

### Probe Execution

No probe applies; the plans and SUMMARYs declare no `scripts/*/tests/probe-*.sh`.

### Requirements Coverage

Every Phase 52 ID in REQUIREMENTS.md is claimed by a plan; none is orphaned.

| Requirement | Source plans | Status | Evidence |
|-------------|--------------|--------|----------|
| CUT-01 | 52-02, 16, 17 | ✓ SATISFIED | SC1 |
| CUT-02 | 52-08, 11-15, 18 | ✓ SATISFIED | SC1; both finish tests pass |
| CUT-03 | 52-01, 09, 10, 18, 31, 32 | ✓ SATISFIED | SC3 |
| CUT-04 | 52-02..07, 10-15, 17, 18 | ✓ SATISFIED | scan tests pass on 10e1c5b; sorted-findings test; full check.sh on this tree pending (SC2) |
| GATE-01 | 52-19, 26, 29 | ✓ SATISFIED | SC4 |
| GATE-02 | 52-20, 22, 24, 27-29, 33 | ✓ SATISFIED | SC4 |
| GATE-03 | 52-20, 22, 27-29 | ✓ SATISFIED | SC4 |
| GATE-04 | 52-30, 33 | ✓ SATISFIED | SC4 |
| OPS-01 | 52-23 | ✓ SATISFIED | SC5 |
| OPS-02 | 52-25 | ✓ SATISFIED | SC5 |
| OPS-03 | 52-21, 23 | ✓ SATISFIED | SC5 |
| OPS-04 | 52-25 | ✓ SATISFIED | SC5 |

### Anti-Patterns Found

The two changed code files: no `TBD`/`FIXME`/`XXX`/`TODO`/`HACK` added. The Info items from the previous report remain in code but are now tracked:

| File | Pattern | Severity | Tracked |
|------|---------|----------|---------|
| `crates/devflow-core/src/reset_time.rs` | `pub fn auto_relaunch_due` has no production caller | ℹ Info | 999.226 |
| `crates/devflow-core/src/agent_result.rs` | `phase_verification_fingerprint` / `_mtime_nanos` have no production caller | ℹ Info | 999.226 |
| several `crates/*/src/*.rs` | comments still name removed v3 surface | ℹ Info | 999.227 |

### Closed since the previous report

- All 19 entries in `deferred-items.md` now carry `status: resolved` or `filed as backlog 999.22x`, and none carries any other status. ROADMAP has 999.224-229, each with a fix shape.
- `52-REVIEW-DISPOSITION.md`: 6 findings, 5 fixed, 1 deferred (IN-03 into 999.224), `open: 0`. The named tests for CR-01, CR-02, WR-01 and the finished-run failure reset exist in the files the ledger names. CR-01's test passed by name this round.

### Still open from before (unchanged)

- The workspace version is still `3.0.0`; `cut-release.sh` derives the major bump at release (planner decision).
- The two `#[ignore]` SUPV-01 evidence tests in `pipeline_launch` were not run.

### Human Verification Required

See the `human_verification` frontmatter. In short:
1. **Re-run `scripts/check.sh all` on 10e1c5b.** An AI can do this once memory allows. It is the only thing between SC2 and VERIFIED.
2. **Six judgment-tier prohibitions.** These are policy sign-offs; no violation was found in any of them.
3. **Your own machine state** (`.devflow/` leftovers, `hermes cron` jobs). This is manual close-out, not a phase failure.

### Gaps Summary

None. No truth failed. SC2 is uncertain only because the full suite run on this tree was killed for host memory, not because anything failed: fmt and clippy passed, and every test re-run by name passed.

---

_Verified: 2026-11-11T19:42:44Z_
_Verifier: Claude (gsd-verifier)_
