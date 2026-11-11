---
phase: 52-subtract-and-stabilize
reviewed: 2026-11-11T17:12:35Z
depth: standard
files_reviewed: 86
files_reviewed_list:
  - ARCHITECTURE.md
  - Cargo.lock
  - Cargo.toml
  - CONTRIBUTING.md
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/config_parse.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/phase_worktree.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
  - crates/devflow-cli/tests/build_provenance.rs
  - crates/devflow-cli/tests/ci_parity_guards.rs
  - crates/devflow-cli/tests/common/mod.rs
  - crates/devflow-cli/tests/cut_release_guard.rs
  - crates/devflow-cli/tests/doctor_json_e2e.rs
  - crates/devflow-cli/tests/failover_e2e.rs
  - crates/devflow-cli/tests/fallback_agent_config.rs
  - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
  - crates/devflow-cli/tests/gate_sweep_e2e.rs
  - crates/devflow-cli/tests/gate_wedge_e2e.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - crates/devflow-cli/tests/help_snapshot.rs
  - crates/devflow-cli/tests/hermetic_spawn_sites.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/pending_supervisor_e2e.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-cli/tests/recover_clean_e2e.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-cli/tests/start_lock_e2e.rs
  - crates/devflow-cli/tests/stop_e2e.rs
  - crates/devflow-cli/tests/supervisor_e2e.rs
  - crates/devflow-cli/tests/test_home_isolation.rs
  - crates/devflow-cli/tests/workspace_version_pin.rs
  - crates/devflow-core/Cargo.toml
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/claude.rs
  - crates/devflow-core/src/agents/codex.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/doc_check.rs
  - crates/devflow-core/src/events.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/gsd_reader.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/outcome_policy.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/recover.rs
  - crates/devflow-core/src/reset_time.rs
  - crates/devflow-core/src/stage.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/supervisor/protocol.rs
  - crates/devflow-core/src/supervisor.rs
  - crates/devflow-core/src/supervisor/runtime.rs
  - crates/devflow-core/src/test_support.rs
  - crates/devflow-core/src/workflow.rs
  - crates/devflow-core/src/worktree.rs
  - crates/devflow-core/tests/devflow_dir_gitignore.rs
  - devflow.toml
  - doc-check-allowlist.toml
  - docs/architecture/agent-model.md
  - docs/architecture/overview.md
  - docs/architecture/state-machine.md
  - docs/diagrams/agent-lifecycle.md
  - docs/diagrams/module-map.md
  - docs/diagrams/system.md
  - docs/guides/configuration.md
  - docs/guides/quickstart.md
  - docs/guides/unattended-mode.md
  - docs/index.md
  - docs/mkdocs.yml
  - .gitignore
  - OPERATIONS.md
  - README.md
  - scripts/cut-release.sh
  - tools/release-preflight/Cargo.toml
  - tools/release-preflight/src/main.rs
  - tools/release-preflight/tests/preflight.rs
findings:
  critical: 2
  warning: 1
  info: 3
  total: 6
  necessity: 0
status: issues_found
---

# Phase 52: Code Review Report

**Reviewed:** 2026-11-11T17:12:35Z
**Depth:** standard (configured deep, downgraded by GSD's depth resolver for the 86-file scope)
**Files Reviewed:** 86
**Status:** issues_found

## Summary

Reviewed the change `4ad32db..HEAD` (+27,525 / -17,061 lines). Coverage was weighted, not uniform. The production
parts of `pipeline_outcomes.rs` (lines 1-2347, read whole), `pipeline_gate.rs` (1-1103, read whole), `gsd_reader.rs`,
`reset_time.rs`, `stage.rs`, the `gates.rs` types and `GateAction` mapping, and the `state.rs`, `prompt.rs` (note
block and fix prompts), `commands.rs` (start, gate respond/route lines, sweep, stop, resume refusal, cleanup),
`preflight.rs`, `agent_result.rs`, `config.rs`, `recover.rs`, `workflow.rs`, `supervisor*` and `monitor.rs` diffs,
the advance routing in `pipeline_launch.rs`, `phase_worktree.rs`, `scripts/cut-release.sh` and
`tools/release-preflight/src/main.rs` were reviewed. The test files and docs were sampled. They were searched for
leftovers of the removed Define/Ship stages, `yes_ship`, `devflow parallel`, `devflow release` and `--reap-strays`,
and none were found; a positive control confirmed the search matches. Test files were not read line by line.

The real GSD reader was probed once (`gsd_run query verification status` on a phase directory with no report). It
returned `status: missing` with a `route` field, which matches what `gsd_reader::classify` requires. No other
finding was confirmed by running code. Each one was traced through the current source. The known items 999.220-223
and the day-past-month-end parse were not re-reported.

The main concerns:

1. In auto mode an ambiguous Validate verdict follows the D-45 table into the gap loop or the re-verify pass, and it
   counts no failure. That loop has no ceiling.
2. A finished run now keeps its unmerged branch, and the way DevFlow tells the operator to start again force-deletes
   that branch.

The necessity rubric (`necessity-review.md`) was read. No necessity finding was made.

## Critical Issues

### CR-01: An ambiguous Validate verdict loops forever in auto mode over `gaps_found`, `stale` or `missing`

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:1528-1532`, `:1564-1571`, `:515-525`
**Issue:** `handle_validate_outcome` counts a failure only for `AgentVerdict::Fail`, or for a `Pass` over a non-`passed`
status. `AgentVerdict::Ambiguous` is never counted (line 1531). The only override ahead of the D-45 table is
`kind == GateKind::FailureCap` (line 1564). With no counted failure, `cap_reached` stays false, so in `Mode::Auto` the
step comes from `next_step(Trigger::AfterValidate { mode: Auto, .. })`, which routes on GSD's status alone:

- `GapsFound` goes to `Step::GapLoop`;
- `Stale` and `Missing` go to `Step::Reverify`.

Neither route counts a failure. A gaps Plan pass that writes a new gap plan counts nothing either (`handle_gaps_plan_outcome`,
`Ok(true)` arm, line 1974). `loop_back_to_code` touches no counter, and `transition` resets `infra_failures`.

Failure scenario: a run with `external_verify` declared, whose probe passes while the agent reports `verdict: gaps`.
`classify_validate_outcome` returns `Ambiguous` (line 243). GSD reports `gaps_found`, so the run goes Validate, then
Plan `--gaps`, then Code `--gaps-only`, then Validate, and again. In auto mode it never reaches
`MAX_CONSECUTIVE_FAILURES` or `MAX_PHASE_VALIDATE_FAILURES` and never opens a gate. The same loop runs through the
re-verify pass when the report is `missing` or `stale`. That is an unattended loop burning agent runs.

This also contradicts the code's own contract. The comment at lines 1526-1527 says "An ambiguous verdict ... gate at once
and touch no counter (T-18-19)". `ValidateOutcome::Ambiguous` (lines 177-181) promises an immediate gate. The gate
kind is still labelled `AmbiguousVerdict` (lines 434-438), but no gate opens. The tests cover Ambiguous only over
`passed` in auto (rows at lines ~6350 and ~8131) and over `gaps_found` in supervise (~9580), so this cell is untested.
D-45's table lists the agent column as "any" for these statuses, so the routing may be what the table says. The
missing bound is still a defect either way, and someone must choose which fix to make.
**Fix:** Either keep T-18-19 and gate an ambiguous verdict in every mode, ahead of the table:
```rust
let step = if kind == GateKind::FailureCap || verdict == AgentVerdict::Ambiguous {
    Step::Gate
} else {
    next_step(Trigger::AfterValidate { mode: state.mode, evidence: &evidence })
};
```
or, if D-45's "any" must route an ambiguous verdict too, count it as a failure whenever the step is not a gate
(`(AgentVerdict::Ambiguous, Ok(r)) => r.status != GsdStatus::Passed` in `counted_failure`), so the ceilings bound it.
Add the auto rows `Ambiguous × {gaps_found, stale, missing}` to the routing tests.

### CR-02: Starting a finished phase again force-deletes its unmerged branch, the only copy of the finished work

**File:** `crates/devflow-cli/src/phase_worktree.rs:31-38`, `crates/devflow-cli/src/commands.rs:617`, `:1268-1277`, `:1321-1325`, `:4628-4634`
**Issue:** Phase 52 removed Ship, so a finished run no longer merges anything. `finish_run` records "the worktree and
branch are kept" (`pipeline_gate.rs:159-160`), and `feature/phase-NN` holds the only copy of the work until the operator
runs `scripts/cut-pr-branch.sh`. The paths DevFlow itself offers from a finished run all destroy that branch:

- `devflow resume` refuses a finished run and says "Run `devflow start --phase N` to start a new run"
  (`commands.rs:4628-4634`).
- `start` in worktree mode (the default) then fails with "worktree already exists ... use --force to recreate it"
  (`phase_worktree.rs:42-46`), because the finished run's worktree is still there.
- `start --force` runs `worktree::remove(.., true)` and `delete_branch(&branch, true)`, which is `git branch -D`
  (`phase_worktree.rs:31-38`, `git.rs:275`). The unmerged commits are deleted with no check and no warning.
  `replaced_state_notice` (`commands.rs:255-277`) says only "this start replaces the state of a finished run".
- `cleanup` keeps a finished run's worktree with the hint "pass --force to remove the worktree and the run's state"
  (`commands.rs:1321-1325`; also `OPERATIONS.md:624`). `cleanup --force` also runs `git.delete_branch(branch, true)`
  (`commands.rs:1273`), which the hint does not mention.

Before Phase 52 the branch had been merged by Ship before either path ran, so nothing was lost. Now, an operator who
follows DevFlow's own instructions loses the finished phase's commits. They stay recoverable only through
`git fsck` until the next gc.
**Fix:** Before force-deleting `feature/phase-NN`, check whether it is merged into the run's base
(`git branch --merged <base>`, or `merge-base --is-ancestor`). Refuse an unmerged branch of a finished run unless the
operator names it explicitly (for example `--discard-unmerged`), or keep the branch and remove only the worktree.
At the least, `start --force`, `cleanup --force`, the kept-worktree hint and the resume refusal must say that the
branch will be force-deleted, and point to `scripts/cut-pr-branch.sh` first.

## Warnings

### WR-01: A reader whose child left a process holding stdout is reported as timed out, but that process is never killed

**File:** `crates/devflow-core/src/gsd_reader.rs:288-310`, `:330-349`
**Issue:** `run_bounded`'s doc says "On expiry the child's process group is killed and the child reaped before
`TimedOut` returns". That holds only for the `try_wait` loop. When the launcher exits but a process it started
(for example `node` under the `gsd_run` wrapper) keeps the pipe open, `collect` times out at the deadline and returns
`ReaderFailure::TimedOut` without calling `end`. Nothing signals the process group, so the leftover process runs on.
The drain thread stays blocked in `read_to_end` for the rest of the process's life. Validate outcomes and every
verification approve call this reader, and both run in the long-lived supervisor, so each such read leaks one
process and one thread.
**Fix:** On a `collect` timeout, signal the reader's process group, which still exists while a member lives:
remember `pid` before `try_wait` reaps the child, and `libc::kill(-pid, SIGKILL)` before returning `TimedOut`. Or keep
the group id from `process_group(0)` and kill it on every timeout path. Add a stub-launcher test whose launcher exits
while a backgrounded child holds stdout, and assert that the child is gone afterwards.

## Info

### IN-01: Doc links point to `validate_route`, which does not exist

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:339`, `:1391`
**Issue:** Both doc comments say the route is picked "through [`validate_route`]", but no function has that name.
The route is `validate_gate_kind` plus `next_step`. This is a broken intra-doc link, and it misleads a reader looking
for the router.
**Fix:** Change both to [`validate_gate_kind`] / [`next_step`].

### IN-02: The doc of `handle_gaps_plan_outcome` describes a reject route the code no longer takes

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:1961-1964`
**Issue:** The doc says "Interim until 52-29's `approve_route` ... reject loops back to Code for a gaps-only pass with the
note". 52-29 is implemented. A reject on these verification-family gates goes through `reject_route_by_kind` to
`Step::Quick`, a `/gsd-quick` pass that clears `gap_closure_pass` (`pipeline_gate.rs:339-341`), not a gaps-only pass.
**Fix:** Drop the interim paragraph, and state that both gates are routed by `approve_route`/`reject_route` like every
verification gate.

### IN-03: A hand-run `devflow advance` over a finished run names `devflow resume`, which refuses it

**File:** `crates/devflow-cli/src/pipeline_launch.rs:3646-3660`
**Issue:** `advance_with` refuses any `stopped` state with "run `devflow resume --phase N` to continue it". A finished
run is also `stopped`, and `resume` refuses it (`commands.rs:4628`), so the operator is sent to a command that fails.
**Fix:** Branch on `state.finished` and print the finished-run line (`devflow start --phase N` for a new run), as the
resume refusal does.

---

_Reviewed: 2026-11-11T17:12:35Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
