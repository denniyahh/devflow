---
phase: "52"
slug: "subtract-and-stabilize"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-11-09"
---

# Phase 52 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. The requirement → test map below is drawn
> from `52-RESEARCH.md` § Validation Architecture. The planner turns it into per-task rows. Test names are
> proposals until a plan fixes them.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` via cargo-nextest 0.9.151; insta snapshots (`INSTA_UPDATE=no` in `scripts/check.sh`) |
| **Config file** | none for nextest; `scripts/check.sh` |
| **Quick run command** | `cargo nextest run -p devflow-core --lib` / `cargo nextest run -p devflow --bin devflow` |
| **Full suite command** | `scripts/check.sh all` (fmt + clippy `-D warnings` + workspace tests + `doc_check`); parity `scripts/check-in-container.sh` |
| **Per-test gate shape** | `cargo nextest run -p <pkg> {--lib\|--bin devflow\|--test <file>} --no-tests=fail -E 'test(=<full::path>)'`, one test per command, chained with `&&` |
| **Estimated runtime** | quick: minutes; full suite: cold container 2-9 min (pushes are backgrounded) |

---

## Sampling Rate

- **After every task commit:** run the task's named nextest gates. When docs or verbs changed, also run
  `doc_check::doc_referenced_identifiers_exist_in_source` and
  `doc_check::source_devflow_env_vars_and_subcommands_are_documented`.
- **After every plan wave:** `scripts/check.sh all`. Each cut plan must leave the workspace compiling and green: the
  cut wave runs strictly in series (RESEARCH, safe order).
- **Before verification:** `scripts/check.sh all` + `scripts/check-in-container.sh` + `cargo semver-checks
  check-release -p devflow-core` after the 4.0.0 bump.
- **Max feedback latency:** a task's named gates, minutes not the full suite.

---

## Requirement → Test Map (both directions)

| Req | Passing direction | Control (must fail, or must still fire) | Proposed test |
|---|---|---|---|
| CUT-01 | `start --dry-run` lists exactly plan, code, validate | empty output and the 3.0.0 list fail the exact-list assertion; the 3.0.0 dry run is recorded once in the phase notes | `phase7_cli::start_dry_run_lists_exactly_plan_code_validate` |
| CUT-01 | `--until define\|ship` rejected by clap | `--until plan` accepted | `phase7_cli::until_rejects_removed_stages` |
| CUT-02 | passing Validate → `workflow_finished {"reason":"finished","shipped":false}`, worktree and branch kept, state kept as finished (D-35), `notify_fired` once (D-06/D-29) | the `--until plan` halt emits `reason: stopped_at` | `supervisor_e2e::a_run_that_passes_validate_finishes_and_keeps_its_worktree` |
| CUT-02/04 | removed verbs unknown to clap and absent from `--help` | `status` still known | `phase7_cli::removed_subcommands_are_unknown` + existing `help_snapshot` |
| CUT-03 | no-fallback rate limit → gate naming the reset time and resume command; no cron file | one fixture, both directions (file absent, gate present) | `failover_e2e::a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate` |
| CUT-03 | passed reset → exactly one auto relaunch; a second denial leaves the gate open | relaunch count == 1 | `failover_e2e::a_passed_reset_relaunches_once_and_a_second_denial_waits` |
| CUT-03 | the reset outlives a short gate timeout; no reset time → plain gate; a resume does not re-arm the relaunch; sweep spares a rate-limit gate before its reset | an ordinary aged gate is still reaped | `failover_e2e::a_future_reset_outlives_the_gate_timeout`, `…::a_denial_without_a_reset_time_is_a_plain_gate`, `pipeline_launch::tests::resume_does_not_rearm_the_auto_relaunch`, `commands::tests::gate_sweep_skips_a_rate_limit_gate_before_its_reset` |
| CUT-04 | no removed-surface literal under `crates/`, `docs/`, or the scoped docs (comments stripped) | a planted literal of each class is flagged; a planted comment is not; the plant is removed before close | `doc_check::no_removed_surface_literal_remains` |
| GATE-01 | a note containing "abort" keeps the run alive; the explicit field aborts; approve+abort is refused at write and read; a record without the field is not an abort | the old substring rule, kept as a fixture, fails | `gates::tests::a_note_containing_abort_does_not_abort` (+3 siblings) |
| GATE-01 | the note is verbatim in the relaunch prompt for each launch shape (stream, capture) | the pre-fix prompt path fails the verbatim assertion | `gate_wedge_e2e::a_rejection_note_reaches_the_{stream,capture}_prompt_verbatim`, `prompt::tests::fix_prompt_carries_the_note_byte_identical` |
| GATE-02 | gaps → Plan `--gaps`; clean reject → `/gsd-quick` + note; stale/missing → execute re-verify prompt quoting GSD's route (D-32) | each wrong route is asserted absent | `pipeline_outcomes::tests::*_routes_to_*` |
| GATE-02 | each reader failure (no launcher, exit≠0, non-JSON, unknown status, `phase_dir_not_found`, `{"error"}`) fires a gate (D-24) | none reads as a pass | `gsd_reader::tests::*` (stub launcher) |
| GATE-02 | the reader reads the worktree evidence root | a `project_root` read gives `missing` | `pipeline_outcomes::tests::the_reader_reads_the_worktree_evidence_root` |
| GATE-02 | an uncovered new file triggers re-verify (D-33) | a `.planning/`-only fix does not | `pipeline_outcomes::tests::an_uncovered_fix_counts_as_stale`, `…::a_planning_only_fix_does_not` |
| GATE-03 | `gaps_found` driven to `passed` in-run (stub status sequence) | a gate forced to "passed" over `gaps_found` fails the never-lies assertion | `supervisor_e2e::a_gaps_found_validate_runs_plan_gaps_then_execute_gaps_only_then_passes` |
| GATE-03 | three gap rounds committing only `.planning/` force the gate (D-25/D-34) | a round that changes a source file resets the streak | `pipeline_outcomes::tests::three_gap_rounds_without_progress_force_the_gate` |
| GATE-03 | a stale `gaps_found` (digest mismatch) fires the D-11 gate (D-22) | unchanged digest → gap loop; a non-v4 digest → gate | `pipeline_outcomes::tests::a_stale_gaps_found_report_fires_the_no_route_gate` |
| GATE-03 | a Code pass that leaves the report `stale` ends at a gate (D-32) | `passed` after Code → Validate proceeds | `pipeline_outcomes::tests::a_code_pass_that_leaves_the_report_stale_ends_at_a_gate` |
| GATE-03 | approving a gaps gate runs the gap loop (D-36) | it never records `workflow_finished` | `pipeline_outcomes::tests::approving_a_gaps_gate_runs_the_gap_loop` |
| GATE-04 | a >300-char reason reaches `gate show` whole; banner, list and notify stay ≤300 and lead with the way-on sentence | existing `commands::tests::gate_show_renders_full_untruncated_sanitized_context` | `pipeline_outcomes::tests::a_long_failure_reason_is_stored_whole` + e2e via `gate show` |
| OPS-01 | `--version` = `devflow 4.0.0 (<commit>, clean\|dirty)` | an empty commit renders `unknown` | `build_provenance::version_prints_the_embedded_commit_and_dirty_flag` |
| OPS-02 | cleanup names the stop reason and keeps a finished run's worktree (D-35); `--force` removes state; status never shows a gone worktree as active; start announces a stale replace | a run whose worktree is present still reports active | `phase7_cli::cleanup_*`, `commands::tests::status_*` |
| OPS-03 | no tracing lines with `RUST_LOG` unset; doctor reports RUST_LOG as info; no provenance warning off-repo; one missing-remote line | `RUST_LOG=debug` still shows tracing (existing test); provenance still fires on DevFlow's own checkout (existing test) | `log_format_env::rust_log_unset_prints_no_tracing_lines` (inverts the existing `:167` test), staleness off-repo test |
| OPS-04 | status prints the stop reason and the fallback state `armed`/`spent`/`none` | each of the three values | `commands::tests::status_prints_the_stop_reason_and_fallback_state` |

---

## Wave 0 Requirements

- [ ] Stub `gsd_run` helper in `crates/devflow-cli/tests/common/mod.rs` (a scripted JSON sequence per call; logs argv), needed by every GATE-02/03 e2e test.
- [ ] A capture-shape fake-agent helper replacing the `--legacy-claude-launch` usage in the ten e2e files that use it only to get the capture shape.
- [ ] Fake agents that log stdin and argv, for the per-shape note assertion.
- [ ] Recorded gsd-core 1.20.1 JSON fixtures (the research probe outputs) for the `gsd_reader` unit tests.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| The 3.0.0 release binary's dry run lists Define and Ship | CUT-01 control | it records the old binary's output once, not code under test | run the installed 3.0.0 `devflow start --dry-run --phase N` and record the output in the phase notes |
| GSD's gap loop and stale route behave as D-08..D-11 and D-32..D-33 assume | GATE-02/03 | they depend on live agent behaviour of GSD | done in `52-GAP-LOOP-VERIFICATION.md` (checks 1-5); re-probe if gsd-core changes `verification.cjs` |
| The operator's own `.devflow/` holds no 3.x state or pending answers that no longer parse | D-28/D-30 (no migration) | it is the operator's machine, not the repo | at close-out, list `.devflow/state-*.json` and gate answers under the operator's checkouts; clear any that hold `define`/`ship` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency: named gates per task, full suite per wave
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
