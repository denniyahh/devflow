---
phase: 17-pipeline-dogfood-followup
reviewed: 2026-07-19T00:00:00Z
depth: standard
files_reviewed: 12
files_reviewed_list:
  - crates/devflow-cli/build.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/build_provenance.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/outcome_policy.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
findings:
  critical: 1
  warning: 3
  info: 1
  total: 5
status: issues_found
---

# Phase 17: Code Review Report

**Reviewed:** 2026-07-19T00:00:00Z
**Depth:** standard
**Files Reviewed:** 12
**Status:** issues_found

## Summary

This is a fresh review of the current state of Phase 17 (`pipeline-dogfood-followup`), superseding the prior review captured at commit `e526d91`. That prior review's CR-01 (infra_failures never resetting across a phase's lifetime) is confirmed fixed: `transition()` in `crates/devflow-cli/src/main.rs` now resets `state.infra_failures = 0` alongside `state.consecutive_failures = 0` on every successful stage transition (commit `cb9ddab`), and both the in-memory and persisted-JSON reset are exercised by `transition_resets_infra_failures`. The companion WR-01 gap (strict-ancestor staleness misclassified as Fresh) is also confirmed fixed: `embedded_commit_is_stale()` now only returns `Staleness::Fresh` on an EXACT match to current `HEAD`, treating any other ancestor as `Stale` (commit `f73a968`), with a dedicated regression test (`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`) reproducing the exact clean-tree, two-commit incident narrative from 17-VERIFICATION.md.

However, this pass found a new, previously-untested defect introduced by the 17c preflight-readiness-gate feature: `run_preflight()` recursively calls `launch_stage()` on a resolved gate (Advance/LoopBack), but is itself called from the *middle* of `launch_stage()`'s own body — so a resolved preflight-failure gate causes the agent for that stage to be launched **twice** (see CR-01 below). This is a genuine functional/resource bug that the existing tests do not catch, because both `run_preflight` tests only exercise the `Abort` gate response, never `Advance`/`LoopBack`.

Three further Warnings were found in the newly added 17d build-provenance/self-dogfood staleness logic and in a stale/misleading doc comment in the 17-04 infra-outcome routing code; one Info-level note on `gh auth status` invocation context.

## Critical Issues

### CR-01: `run_preflight`'s recursive `launch_stage` call causes a duplicate agent spawn on gate resolution

**File:** `crates/devflow-cli/src/main.rs:788-816` (`run_preflight`) and `crates/devflow-cli/src/main.rs:1042-1123` (`launch_stage`)

**Issue:**

`run_preflight` is called from the *middle* of `launch_stage`'s body (line 1067), with more launch work (`enforce_build_staleness`, `agent_result::archive_phase_files`, `monitor::spawn_monitor`) still to run afterward:

```rust
// launch_stage (abridged)
let project_root = state.project_root.clone();
run_preflight(&project_root, state, adapter.as_ref())?;   // <-- line 1067

enforce_build_staleness(&project_root, state, ...)?;       // <-- runs again below
if let Some(stamp) = agent_result::archive_phase_files(...)? { ... }
let pid = monitor::spawn_monitor(state, program, &args, &adapter.extra_env())...;
```

But when the generic/adapter preflight check fails, `run_preflight`'s failure branch resolves the gate and then recurses into a **full** `launch_stage(state, None, None)` call to retry:

```rust
return match run_gate(project_root, state, stage, &context)? {
    GateAction::Advance => {
        let _ = Gates::cleanup(project_root, state.phase, stage);
        state.gate_pending = false;
        launch_stage(state, None, None)   // <-- runs enforce_build_staleness,
    }                                       //     archive_phase_files, AND
    GateAction::LoopBack(_) => {             //     spawn_monitor to completion
        let _ = Gates::cleanup(project_root, state.phase, stage);
        launch_stage(state, None, None)
    }
    GateAction::Abort(reason) => abort(project_root, state, &reason),
};
```

That nested `launch_stage` call runs the *entire* function to completion — including its own `enforce_build_staleness`, `archive_phase_files`, and `monitor::spawn_monitor` — and returns `Ok(())` once the agent is actually running. That `Ok(())` becomes `run_preflight`'s return value.

Back in the **outer** `launch_stage` frame that originally called `run_preflight(...)?` at line 1067, the `?` sees `Ok(())` and — because `run_preflight`'s success case is indistinguishable from "the retry already fully launched the agent" — **execution simply continues** into `enforce_build_staleness`, `archive_phase_files`, and `monitor::spawn_monitor` a **second time**, spawning a second competing agent process for the same stage, writing a second `stage_launched` event, and racing the second `archive_phase_files`/`spawn_monitor` against the first agent's just-started, still-live stdout capture and worktree.

This affects *every* call site of `launch_stage` (`start`, `resume`, `transition`, `loop_back_to_code`, and the retry arms of `handle_stage_failure`) whenever a preflight check fails and the gate resolves to `Advance` or `LoopBack` — most plausibly the realistic Ship-stage `gh auth status` check: an operator sees the preflight gate, runs `gh auth login`, approves the gate, and DevFlow launches Ship's agent twice concurrently in the same worktree.

This defect is untested: `run_preflight_failing_check_gates_and_never_reaches_spawn_monitor` and `run_preflight_adapter_hook_override_fires` (main.rs:4476, 4510) both only exercise the `Abort` gate response; no test exercises `Advance`/`LoopBack` through `run_preflight`, which is exactly the path where the double-spawn occurs.

Note that this pattern does *not* affect `handle_stage_failure`'s existing analogous recursive `launch_stage` calls, because `handle_stage_failure` is only ever invoked from the top-level `advance()` dispatcher (never from inside `launch_stage` itself) — there is no "more work to do" in the caller after it returns. `run_preflight` is new in this phase and is the only recursive-into-`launch_stage` helper that is itself called *from within* `launch_stage`.

**Fix:** Make `run_preflight` report whether the caller should continue, instead of overloading `Ok(())` to mean two different things:

```rust
/// Returns `Ok(true)` when the caller should continue the rest of
/// `launch_stage` (preflight passed). Returns `Ok(false)` when a failing
/// check was resolved via a gate that ALREADY completed a full retried
/// launch (Advance/LoopBack) or aborted (Abort) — the caller must not run
/// any more launch steps for this invocation.
fn run_preflight(
    project_root: &Path,
    state: &mut State,
    adapter: &dyn agents::AgentAdapter,
) -> Result<bool, CliError> {
    let stage = state.stage;
    if let Err(reason) =
        generic_preflight_checks(project_root, state).and_then(|()| adapter.preflight(state))
    {
        let context = format!(
            "[never-silent] preflight failed for stage {stage}: {} — human review needed \
             (retry, loop-to-code, or abort)",
            truncate_reason(&reason)
        );
        match run_gate(project_root, state, stage, &context)? {
            GateAction::Advance => {
                let _ = Gates::cleanup(project_root, state.phase, stage);
                state.gate_pending = false;
                launch_stage(state, None, None)?;
            }
            GateAction::LoopBack(_) => {
                let _ = Gates::cleanup(project_root, state.phase, stage);
                launch_stage(state, None, None)?;
            }
            GateAction::Abort(reason) => abort(project_root, state, &reason)?,
        }
        return Ok(false);
    }
    Ok(true)
}
```

And in `launch_stage`:

```rust
let project_root = state.project_root.clone();
if !run_preflight(&project_root, state, adapter.as_ref())? {
    return Ok(());
}
```

Add a regression test that pre-seeds an `Advance` gate response for a failing preflight check and asserts exactly one `stage_launched` event (or exactly one `monitor::spawn_monitor` call, via a test seam) is recorded — the current tests only cover the `Abort` arm.

## Warnings

### WR-01: `enforce_build_staleness` prints a near-universally-firing, misleading warning for every non-self-dogfood project

**File:** `crates/devflow-cli/src/main.rs:997-1037` (`enforce_build_staleness`), `crates/devflow-cli/src/main.rs:861-878` (`embedded_commit_is_stale`)

**Issue:** `embedded_commit_is_stale` shells `git merge-base --is-ancestor <DevFlow's own embedded build commit> HEAD` inside `project_root` — which, for the overwhelmingly common case (DevFlow driving *some other* project, not its own workspace), is a completely unrelated repository. `git merge-base --is-ancestor` on a commit hash that doesn't exist in that repo's object database exits `128` (verified empirically), which `embedded_commit_is_stale` correctly classifies as `Staleness::Indeterminate` — but `staleness_outcome` maps `Indeterminate` to `StalenessOutcome::Warn` for *every* project, self-dogfood or not (`main.rs:980-987`). The result: on essentially every single stage launch of every ordinary (non-DevFlow) project driven by DevFlow, this prints:

```
warning: build provenance staleness check did not confirm a fresh build for
stage {stage} — proceeding (only DevFlow's own workspace is ever hard-blocked, D-18)
```

This is accurate but semantically meaningless in that context (DevFlow's own commit hash was never expected to relate to an unrelated project's git history), yet it fires on effectively every stage of every phase for every ordinary user — training operators to ignore DevFlow warnings generally ("crying wolf"), which undermines the value of every *other* warning-level message the pipeline emits (including the ones from this exact codepath in the rare cases they matter).

**Fix:** Skip the `combined_staleness` computation (and its warning) entirely for non-self-dogfood projects — the design already treats `Indeterminate`/non-self-dogfood-`Stale` as a no-op-equivalent outcome, so there's no information lost by short-circuiting:

```rust
fn enforce_build_staleness(...) -> Result<(), CliError> {
    if !is_self_dogfood_workspace(project_root) {
        return Ok(()); // the gate only ever applies to DevFlow's own workspace (D-18)
    }
    let staleness = combined_staleness(project_root, embedded_commit, build_timestamp);
    match staleness_outcome(true, staleness) {
        ...
    }
}
```

### WR-02: `is_self_dogfood_workspace` uses substring matching, which can false-positive-block an unrelated project

**File:** `crates/devflow-cli/src/main.rs:949-966`

**Issue:** After locating the `members = [...]` array's text bounds, the check is:

```rust
members.contains("crates/devflow-core") && members.contains("crates/devflow-cli")
```

`str::contains` is a substring match, not an exact array-element match. A workspace with member paths like `"crates/devflow-core-extras"` and `"crates/devflow-cli-plugin"` (or any project that happens to fork/vendor/rename DevFlow's own crates with those directory names as prefixes) would satisfy both `contains` checks without literally containing the `crates/devflow-core`/`crates/devflow-cli` members, and would incorrectly be classified as `is_self_dogfood_workspace() == true`. Combined with `enforce_build_staleness`'s hard `Block` outcome for self-dogfood + `Stale`, this could hard-block an unrelated (but similarly-named) project's entire pipeline — the one outcome this whole feature set is designed never to inflict on an ordinary project.

**Fix:** Match on quoted, comma/whitespace-delimited array elements rather than raw substrings, e.g. split `members` on `,` and trim/strip quotes from each element before comparing for exact equality:

```rust
let entries: Vec<&str> = members
    .split(',')
    .map(|s| s.trim().trim_matches('"').trim_matches('\''))
    .collect();
entries.contains(&"crates/devflow-core") && entries.contains(&"crates/devflow-cli")
```

### WR-03: `gate_or_abort_infra`'s doc comment misdescribes the call graph, risking a future double-increment bug

**File:** `crates/devflow-cli/src/main.rs:1306-1309`

**Issue:** The doc comment on `gate_or_abort_infra` states it is:

> shared by [`handle_infra_outcome`] and the `AutoResume` arm's infra-ceiling branch (which bumps `infra_failures` itself before calling this, so the counter is never bumped twice for the same outcome)

But `handle_rate_limited_outcome`'s ceiling branch (main.rs:1348-1350) does **not** bump `state.infra_failures` itself and does **not** call `gate_or_abort_infra` directly — it calls `handle_infra_outcome(project_root, state, stage, reason)`, which is the function that performs the increment (`state.infra_failures = state.infra_failures.saturating_add(1)`, main.rs:1301) before calling `gate_or_abort_infra`. The current behavior is correct (the counter is bumped exactly once), but the comment describes a call graph that doesn't match the code — a future maintainer trusting this comment while modifying `handle_rate_limited_outcome` could plausibly add a bump before the `handle_infra_outcome(...)` call, reintroducing a double-increment.

**Fix:** Correct the doc comment to describe the actual call graph:

```rust
/// The ceiling check + gate-or-abort half of the infra path. Called only
/// from [`handle_infra_outcome`], which performs the `saturating_add`
/// increment before invoking this — including when reached via
/// `handle_rate_limited_outcome`'s ceiling branch, which delegates to
/// `handle_infra_outcome` rather than bumping the counter itself.
```

## Info

### IN-01: `preflight_gh_auth_check` does not set `current_dir` for the `gh auth status` probe

**File:** `crates/devflow-cli/src/main.rs:755-773`

**Issue:** Every other git/gh-adjacent subprocess call in this file (`phase_artifact_on_develop`, `embedded_commit_is_stale`, `run_git_stdout`, `tracked_source_newer_than_build`) explicitly sets `.current_dir(project_root)`. `preflight_gh_auth_check`'s `Command::new("gh").args(["auth", "status"]).output()` does not, so it runs in the CLI process's own working directory rather than the driven project's. `gh auth status` reports host-level credential state and is largely directory-independent, so this is unlikely to matter in practice, but it's an inconsistency with the surrounding code's convention and could matter if `gh`'s auth resolution ever becomes repo-scoped (e.g. per-remote host detection).

**Fix:** Add `.current_dir(project_root)` (thread `project_root` into this function, or read it from `state.project_root`) for consistency with the rest of the module's subprocess calls.

---

_Reviewed: 2026-07-19T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
