---
phase: 51-rate-limit-agent-failover
fixed_at: 2026-11-08T14:55:00Z
review_path: .planning/phases/51-rate-limit-agent-failover/51-REVIEW.md
iteration: 1
findings_in_scope: 6
fixed: 6
skipped: 0
status: all_fixed
---

# Phase 51: Code Review Fix Report

**Fixed at:** 2026-11-08T14:55:00Z
**Source review:** .planning/phases/51-rate-limit-agent-failover/51-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 6 (fix_scope `all`)
- Fixed: 6. WR-01 and WR-02 change behavior and need a human to confirm the logic.
- Skipped: 0

**Where verification ran:** in the phase worktree `.worktrees/phase-51` on `feature/phase-51`,
which is the tree these commits are on. `workflow.use_worktrees` is `false`, so no separate
review-fix worktree was created.

## Fixed Issues

### WR-01: `devflow parallel` arms `devflow.toml`'s `fallback_agent`

**Files modified:** `crates/devflow-cli/src/commands.rs`, `crates/devflow-cli/src/main.rs`,
`crates/devflow-cli/src/parallel.rs`, `crates/devflow-cli/tests/fallback_agent_config.rs`
**Commit:** 9886a95
**Status:** fixed: requires human verification (behavior choice)
**Applied fix:** Option 1 from the review: `parallel` never arms a fallback. The configuration
guide already says the key "is read by `devflow start` only". `start` now takes a
`FallbackSource`. `devflow start` passes `FlagOrConfig(flag)`, which behaves as before. `parallel`
passes `NoFallback`, which reads no key. The `parallel` comment now matches the code.
**Test:** `fallback_agent_config::parallel_ignores_the_config_key` runs a real `devflow parallel`
with `fallback_agent = "claude"` and agent claude. It asserts that the launch succeeds, that no
fallback line is printed, and that the saved state has `fallback_agent: null`.
**Negative control:** with `parallel` passing `FlagOrConfig(None)`, the test fails with
`devflow.toml's fallback_agent (claude) is refused: ...`.

### WR-02: A retry answer at the failed-canary gate ends the run with no gate (closes the gate-answer entry point of 999.187)

**Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`,
`crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-cli/tests/failover_e2e.rs`,
`OPERATIONS.md`
**Commit:** 7e8ee1a
**Status:** fixed: requires human verification (logic change)
**Applied fix:** A retry or loop-back answer now goes through the new `relaunch_failed_stage` in
`handle_stage_failure`. It checks `recorded_canary_refusal(state)` first. That check uses the
same predicate as `canary_gate`: a stream launch and a recorded `Absent` or `Unverified`
outcome. When the recorded canary refuses the run's agent, the stage is not relaunched. The
supervisor still owns the record, and while it does it:
- marks the run `stopped`, with a `stop_reason` that names the refusal and
  `devflow resume --phase N --agent <other>`;
- clears `gate_pending`;
- emits a `gate_relaunch_refused` event (`stage`, `agent`, `reason`);
- returns `Ok`.

The review's first option, re-opening the gate, was not used. A supervisor waiting at a gate is
live, `resume` refuses over a live run (`refuse_launch_over_a_live_run`), and `stop` aborts and
deletes the state. A re-opened gate would therefore leave abort as the only way on.
`FailoverRelaunchGuard` was not reused either, because a permanent source-scan test pins its
use to `fail_over_on_rate_limit`. OPERATIONS.md documents the event and the new outcome.
**Tests:** `failover_e2e::approving_the_failed_canary_gate_stops_the_run_and_names_the_way_on`
and `failover_e2e::rejecting_the_failed_canary_gate_stops_the_run_and_names_the_way_on`. Each
drives a real run to the gate left by `a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate`,
answers it, and asserts:
- the event and its fields;
- `stopped`, plus the exact `devflow resume --phase N --agent <other>` text in `stop_reason`;
- no gate is pending;
- `agy` was invoked only for its first canary.

The existing gate test is unchanged and still passes.
**Negative control:** with the pre-check disabled, the approve test fails after 120 s with
`timed out ... waiting for one of ["gate_relaunch_refused", "workflow_finished"]`. That is the
999.187 silent end.
**Backlog 999.187:** this closes 999.187 only at this entry point: a gate answer whose relaunch
the recorded canary refuses. It does not change the two reproductions 999.187's acceptance
names. Those are the `Ambiguous` re-drive and an ordinary transition whose launch is refused or
never publishes. A gate answer whose relaunch fails for another reason, such as a preflight
error or a spawn failure, also still clears the record and ends the run silently. ROADMAP.md was
not edited.

### IN-01: The failover guard also covers `run_preflight`'s gate, contrary to its doc comments

**Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
**Commit:** 982d443
**Applied fix:** This is a comment-only change, with no code change. The doc comments on
`refuse_launch`, `record_failed_launch` and `FailoverRelaunchGuard` now say what the code does.
The guard lasts for the whole `launch_stage` call, including `run_preflight`. A preflight gate
opened there, and the launch its answer starts, keep the record. A launch started by the
failover's own infra gate runs unguarded. The claim was checked against `preflight.rs`: its
gate arms call `launch_stage_inner` and `launch_stage`.

### IN-02: The `devflow.toml` key parses agent names more strictly than the flag

**Files modified:** `crates/devflow-core/src/config.rs`
**Commit:** cd35c7a
**Applied fix:** `FallbackAgentKey.fallback_agent` is now an `Option<String>`, parsed with
`AgentKind::from_str`, the parser the flag uses. An invalid value maps into the existing
`devflow.toml: cannot read \`fallback_agent\`` message. Only a private struct changed. The
signature of `config::fallback_agent` did not.
**Test:** `config::tests::fallback_agent_key_resolves_from_devflow_toml` now also asserts that
`"Codex"` resolves to Codex and `"open-code"` resolves to OpenCode.
**Negative control:** parsing with serde fails the test with
`unknown variant \`Codex\``.

### IN-03: An unparseable `devflow.toml` is reported as a `fallback_agent` problem

**Files modified:** `crates/devflow-core/src/config.rs`
**Commit:** 0f6c79c
**Applied fix:** The file is parsed as a `toml::Table` first. A syntax error now returns
`devflow.toml: <error>`, the same text `base_branch` uses. The key-specific refusal is kept for
a parseable file whose `fallback_agent` value does not name an agent, including a value that is
not a string.
**Test:** the same test now asserts two things:
- a file with no key and a syntax error (`review_angles = ["security"`) starts with
  `devflow.toml: ` and does not contain `fallback_agent`;
- `fallback_agent = 3` still names the key.

**Negative control:** with the old error mapping, the test fails with
`a parse error does not blame the fallback_agent key`.

### IN-04: The torn-final-line clause claims a quota denial cannot be ruled out for every agent

**Files modified:** `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/monitor.rs`,
`OPERATIONS.md`
**Commit:** c6e29e0
**Applied fix:** This changes wording only. The clause segment now reads
`torn final line after the last parsed event`, and the test's pinned literal was updated to
match. The monitor's warn and log lines now say "a Claude quota denial cannot be ruled out".
OPERATIONS.md explains that the classifier reads only Claude's denials. The verdict is
unchanged.

## Verification

- Each commit was gated on `cargo fmt --all` and on a clean
  `cargo clippy --workspace --all-targets -- -D warnings`. Each also passed its named tests, run
  with `cargo nextest run --no-tests=fail -E 'test(=...)'`. The pre-commit hook, gitleaks,
  passed on every commit, and none used `--no-verify`.
- `cargo semver-checks check-release -p devflow-core`: 229 pass, 31 skip, "no semver update
  required".
- `scripts/check.sh all` passed and exited 0 (fmt, clippy, `cargo test --workspace`):
  - 45 test binaries, all `ok`, and no `test result: FAILED` line;
  - **2113 passed, 0 failed, 6 ignored**, summed from the `test result:` lines;
  - a second count, of the per-test `... ok` lines, gives 2112. The one-line gap is probably a
    line interleaved with other output, but this was not confirmed.

  The known intermittent test `status_and_doctor_name_a_held_state_write_lock` (999.188) passed
  in this run.

**What this does not establish:**
- The WR-02 tests use stub agents, not a live Claude or Antigravity CLI.
- No test covers `devflow resume --phase N --agent <other>` after the new stop. That it works
  is read from `resume_loaded`: it clears `stopped`, and `hand_off_agent` clears the canary.
- `devflow status` shows the `devflow resume --phase N` continue hint for a stopped run, but it
  does not print `stop_reason`. The way on is therefore in the state file, the event log and
  OPERATIONS.md, but not in the status output.

---

_Fixed: 2026-11-08T14:55:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
