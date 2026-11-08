---
phase: 51-rate-limit-agent-failover
fixed_at: 2026-11-08T19:50:51Z
review_path: .planning/phases/51-rate-limit-agent-failover/51-REVIEW.md
iteration: 1
fix_scope: all
findings_in_scope: 5
fixed: 5
skipped: 0
status: all_fixed
---

# Phase 51: Code Review Fix Report

**Fixed at:** 2026-11-08T19:50:51Z
**Source review:** .planning/phases/51-rate-limit-agent-failover/51-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 5 (fix_scope `all`)
- Fixed: 5. WR-02 was closed by documenting the residual (operator decision, option A),
  tracked as backlog 999.207; its monitor and verdict logic are unchanged. WR-01 and IN-01
  change behavior.
- Skipped: 0

**Where verification ran:** in the phase worktree `.worktrees/phase-51` on `feature/phase-51`.

**Checks run before each code commit:** `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings`, with the commit gated on their exit
code. Each new test was run by exact name with `cargo nextest run --no-tests=fail -E 'test(=...)'`
and has a negative control: the fix was reverted or the constant changed in place, the test
failed with the expected assertion, and the source was restored.

**After all commits:** the `commands::`, `pipeline_outcomes::` and `pipeline_launch::` unit
tests of `devflow` (521 passed), the `monitor::`, `agent_result::` and `doc_check::` unit tests
of `devflow-core` (317 passed), and `--test failover_e2e` (7 passed).
`cargo semver-checks check-release -p devflow-core`: no semver update required.

**What this does not establish:** `scripts/check.sh all`, the container check and the full
`devflow` / `devflow-core` suites were not run. WR-02's residual order was not reproduced at
runtime; it rests on the review's and the verifier's code trace.

## Fixed Issues

### WR-01: The IN-03 "way on" sentence is always cut from the pending-gate banner and `gate list`

**Files modified:** `crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-cli/src/commands.rs`, `crates/devflow-cli/tests/failover_e2e.rs`
**Commit:** cce00cc
**Status:** fixed: requires human verification (behavior change: the gate context's word order)
**Applied fix:**
- New `failure_gate_context` builds the never-silent context. When the recorded canary refuses
  the run's agent, the way-on sentence now leads, before the capped reason:
  `[never-silent] stage code failed: a retry or loop-back will not relaunch under {agent} (its
  recorded delivery canary refuses it); answer the gate, then run `devflow resume --phase {phase}
  --agent <other>`. Cause: <reason> — human review needed (...)`. With `antigravity` and a
  4-digit phase the text up to `Cause: ` is 214 characters, inside the 261 shown.
- `stage_failure_context` takes the lead as a parameter; `handle_stage_failure` uses
  `failure_gate_context`. The e2e sentence assertion was updated to the new wording.
- Test `commands::tests::the_failure_gate_way_on_survives_the_banner_and_gate_list_for_a_long_reason`:
  a reason over 400 characters; the list, the all-roots row and the banner each show
  `devflow resume --phase 9999 --agent <other>`. In-test control: a `Confirmed` canary carries
  no way on.
- Negative control: with the lead moved back after the reason, the test failed with
  "list cut the way on".
- Limit: the idle-timeout cause clause's budget assumes no lead before the reason. The two do
  not meet in practice (a refusing recorded canary refuses the launch, so no agent runs to go
  idle), but no test asserts that.

### WR-02: The X-01 fix closes the monitor/verdict disagreement only for a torn line that is already in the capture; a killed child's partial line still reproduces it

**Files modified:** `OPERATIONS.md`, `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/monitor.rs`
**Commit:** ff649ff
**Status:** fixed by documenting the residual (operator decision); tracked as backlog 999.207
**Applied fix:**
- No logic change. The three overstatements now name the order as a known residual:
  - OPERATIONS.md: "recorded as one, flagged torn" now holds only when the torn line reached
    the capture before the monitor's idle check. A new paragraph says that a line cut by the
    kill arrives at end of output; after a complete denial the monitor logs rate-limited
    (resumable), writes no idle-timeout record and kills the agent, and the gate shows an
    indeterminate failure (`stream capture ends in an unparseable line`). Still a gate, with no
    failover and no cron resume.
  - `classify_capture_denial`'s doc: the shared rule makes the two answer alike for the same
    bytes, not "cannot disagree"; it names the EOF order and 999.207.
  - The monitor test doc: the read side re-checks only when the monitor wrote a record; the
    order is not tested there.
- Also corrected, same overstatement: `capture_rate_limit_denial`'s doc now says the read-time
  re-check does not cover a monitor that answered `Denied`.
- Docs only, so no test; `doc_check::` passed.

### IN-01: The status hint ignores the environment's legacy opt-out, but `resume`'s refusal applies it

**Files modified:** `crates/devflow-cli/src/commands.rs`, `crates/devflow-cli/src/pipeline_launch.rs`
**Commit:** d149da8
**Status:** fixed: requires human verification (behavior change: the status hint)
**Applied fix:**
- New `pipeline_launch::resume_canary_refusal(state, flag, env)` judges a clone with the legacy
  opt-out combined exactly as `apply_legacy_launch_opt_out` does (both now call one
  `combine_legacy_launch_opt_out`). `refuse_recorded_canary_for_resume` and `recovery_hints`
  both judge through it.
- `recovery_hints` takes the environment's value as a parameter; `status_hints` passes
  `config::claude_legacy_launch()`. Tests pass it explicitly, so they read no process env.
- Test `commands::tests::recovery_hints_judge_the_environment_legacy_opt_out_as_resume_does`:
  a Claude run at Code with a recorded `Absent` canary; without the env opt-out both the hint
  (`--agent <other>`) and `resume_canary_refusal` refuse, with it both admit (plain `resume`).
- Negative control: with the hint ignoring the env value, the test failed
  (`env opt-out true`: left `--agent <other>`, right plain resume).

### IN-02: A stale "100-char" doc comment on `gate_show`

**Files modified:** `crates/devflow-cli/src/commands.rs`
**Commit:** fc92c6b
**Applied fix:** the `gate_show` doc comment now names `gate_list`'s `GATE_CONTEXT_DISPLAY_CHARS`
truncation. A search of `crates/`, README.md, OPERATIONS.md, ARCHITECTURE.md, CONTRIBUTING.md
and `docs/` found no other 100-character gate-context claim. The remaining `100`s are a cron
hint's retry-after bound and test inputs of `render_gate_context`, not the gate display bound.

### IN-03: Nothing ties `GATE_CONTEXT_DISPLAY_CHARS` to the 300 that `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` was derived from

**Files modified:** `crates/devflow-cli/src/commands.rs`, `crates/devflow-core/src/agent_result.rs`
**Commit:** 744739a
**Applied fix:**
- Took the test option. `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` is now `pub` (additive;
  `cargo semver-checks`: no semver update required).
- Test `commands::tests::the_budgeted_idle_timeout_clause_closes_inside_the_gate_display_bound`
  builds a Validate failure gate (the longest prefix) whose clause is exactly the core budget,
  through `failure_gate_context` (so the reason cap applies), and asserts the closed clause in
  the list, the all-roots row and the banner. Control: a clause 24 characters past the budget
  is cut in all three.
- `GATE_CONTEXT_DISPLAY_CHARS`'s doc names the test.
- Negative controls: `GATE_CONTEXT_DISPLAY_CHARS = 290` failed it; `MAX_IDLE_TIMEOUT_CLAUSE_CHARS
  = 210` failed it. Both restored.
- Limit: `truncate_reason` keeps its own literal 300. The test catches a change to it that cuts
  the clause, but it is not one constant.

---

_Fixed: 2026-11-08T19:50:51Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
