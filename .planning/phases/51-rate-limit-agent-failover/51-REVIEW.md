---
phase: 51-rate-limit-agent-failover
reviewed: 2026-11-08T00:58:00Z
depth: deep
files_reviewed: 22
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/tests/failover_e2e.rs
  - crates/devflow-cli/tests/fallback_agent_config.rs
  - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/doc_check.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/state.rs
  - doc-check-allowlist.toml
  - docs/guides/configuration.md
  - docs/guides/unattended-mode.md
  - OPERATIONS.md
  - README.md
findings:
  critical: 0
  warning: 2
  info: 4
  total: 6
  necessity: 0
status: issues_found
---

# Phase 51: Code Review Report

**Reviewed:** 2026-11-08T00:58:00Z
**Depth:** deep
**Files Reviewed:** 22
**Status:** issues_found

## Summary

Scope: the hunks changed since `88f3b679` in the 22 listed files, plus the call chains they enter:
`advance_with` → `failover_target` / `fail_over_on_rate_limit` → `hand_off_agent` → `launch_stage` →
`run_preflight` / `canary_gate` / `spawn_agent_and_record` / `publish_record` / `shut_down_unrecorded` /
`record_failed_launch`, the `code_fix` lifecycle (`transition`, `prepare_loop_back_to_code`,
`default_stage_prompt`), `start` / `resume_loaded` / `parallel` → `config::fallback_agent` /
`validate_fallback_agent`, and the idle-timeout path (`fire_idle_timeout` → `capture_rate_limit_denial` /
`idle_timeout_cause` → `IdleTimeoutRecordOnDisk` → `parse_idle_timeout_side_channel`).

No Critical issue was found. The failover core holds up under the traces I made:
- every handoff check runs on a candidate copy before the single commit;
- the consumed mark is written in the same commit as the agent swap;
- `commit` reloads from disk, so the guard's "keep the record" leaves the on-disk record naming the current
  supervisor, including through `shut_down_unrecorded`;
- the only production writes of `stage` are `transition` and `prepare_loop_back_to_code`, and both maintain
  `code_fix`;
- no test function was deleted, and the moved `resume_re_marks_stopped_when_launch_stage_fails_outright`
  still exists;
- the new public surface (`State` fields, `config::fallback_agent`, the `FixType` serde derives) is additive,
  because `State` and `FixType` are `#[non_exhaustive]`;
- both test seams are compiled out of `cargo install` builds: dev-dependency features do not apply there.

The main defect: `devflow parallel` now arms `devflow.toml`'s `fallback_agent`, although its own comment
says its runs carry no fallback. With the key set, parallel cannot launch a phase whose agent equals the key,
and the remedy the refusal suggests is a flag `parallel` does not have. WR-01 has an empirical reproduction
with a negative control. Every other finding comes from reading the code.

**What this review does not establish:** I did not run `cargo test` or `scripts/check.sh all`, so no test
result is claimed here. The SUPV-01 evidence experiments (publish-wait and post-save probe seams) were
checked only for compile-out and env-var hygiene, not re-run.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: `devflow parallel` arms `devflow.toml`'s `fallback_agent`, contradicting its comment, and its refusal names a flag `parallel` lacks

**File:** `crates/devflow-cli/src/parallel.rs:130-143`, `crates/devflow-cli/src/commands.rs:410-441`
**Issue:** The comment in `parallel` says "`parallel` takes no `--fallback-agent`, so its runs carry none".
But `parallel` calls `start(..., None)`, and since 51-14 `start` resolves `config::fallback_agent(project_root)`
whenever the flag is `None` (`commands.rs:410-412`), then validates the result and arms it. So every
`parallel` run carries the `devflow.toml` key, and the key is validated against each phase's agent:
- With `fallback_agent = "antigravity"`, `devflow parallel --agents antigravity` is refused for that phase.
  The refusal tells the operator to "pass --fallback-agent <other> ... or remove fallback_agent from
  devflow.toml", and `parallel` has no such flag. Removing a tracked project setting is the only way through.
- With `fallback_agent = "codex"` (or `opencode`/`pi`/`hermes`), every `parallel` phase in its default
  `--mode auto` is refused by D-03's launch-shape check.
- The refusal goes up through `?` mid-loop (`parallel.rs:132-144`), so phases launched earlier in the same
  invocation keep running and the later ones never launch.

Reproduced at HEAD (`target/debug/devflow`, built after the last `crates/` commit), in a scratch repo with a
`.devflow/` dir and `fallback_agent = "antigravity"`. Then
`devflow parallel --phases 1 --agents antigravity --mode supervise` exits 1 with
`devflow.toml's fallback_agent (antigravity) is refused: --fallback-agent antigravity is the run's own agent ...`.
Negative control: the same command shape (`start --dry-run`, same agent) with the key removed exits 0.
**Fix:** Decide what `parallel` should do, then make the code and the comment agree.
- Option 1, parallel never arms a fallback: give `start` an explicit source enum
  (`FallbackSource::Flag(Option<AgentKind>)` vs `::FlagOrConfig(Option<AgentKind>)`) and have `parallel` pass
  the no-config variant.
- Option 2, parallel honours the key: add `--fallback-agent` to `Command::Parallel`, thread it through, and
  fix the comment.

Either way, add a `fallback_agent_config.rs` test that drives `devflow parallel` with the key set.

### WR-02: The never-silent gate after a canary-refused failover offers retry and loop options that end the run with no gate (999.187, made reachable by this phase)

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:933-946`, `crates/devflow-cli/src/pipeline_launch.rs:648-653`, `OPERATIONS.md:331-337`
**Issue:** When the fallback's delivery canary refuses, `fail_over_on_rate_limit` opens the infra gate via
`handle_infra_outcome` → `handle_stage_failure`. The gate text offers "(retry, loop-to-code, or abort)", and
on both `Advance` and `LoopBack` `handle_stage_failure` calls `launch_stage(state, None, Some(stage))`.
That relaunch is outside the `FailoverRelaunchGuard`. `canary_gate` meets the recorded refusal again,
`refuse_launch` clears `supervisor`, and the error ends the supervisor. Result: the run is at Code with no
record, no gate and no event, which is the 999.187 shape. OPERATIONS.md documents this outcome as expected
("the run is left not live with no new gate"). The defect is already filed (999.187, which says Phase 51's
failover is gated), but the failover gate is a new, routine entry point: two of its three offered answers
are known dead ends.
**Fix:** Until 999.187 is fixed generally, check the canary in `handle_stage_failure`'s retry arms. If
`state.canary` records a refusal for `state.agent`, re-open the gate with a context that names
`devflow resume --phase N --agent <other>` as the only way on. Alternatively, keep the record and open a
fresh infra gate on a refused relaunch from a gate answer, as the failover path does. Either option needs a
test driving the gate answer after `a_handoff_to_antigravity_that_fails_its_canary_ends_at_a_gate`.

## Info

### IN-01: The failover guard also covers `run_preflight`'s own gate and the relaunch its answer starts, contrary to its doc comments

**File:** `crates/devflow-cli/src/pipeline_launch.rs:3238-3242` (guard scope), `:643-647` and `:2063-2068` (doc claims)
**Issue:** `FailoverRelaunchGuard` is held across the whole `launch_stage` call. That call includes
`run_preflight` (`preflight.rs:1448-1538`), which can open a preflight gate, wait on it, and then call
`launch_stage_inner` or `launch_stage` from the gate's answer, all still inside the guard. The doc comments
say "a launch the gate's answer starts later runs unguarded". This is reachable only when the preflight
result changes between `hand_off_agent`'s checks and `run_preflight`'s identical ones a moment later, for
example a flaky `gh auth` probe or a flaky driver health check. In that case the second launch keeps the
record and opens another infra gate. That behaviour is arguably better than 999.187's silent end, but it is
not what the comments say.
**Fix:** Either narrow the comment ("including a preflight gate opened inside this relaunch and the launch
its answer starts"), or engage the guard only around the post-preflight part of the launch.

### IN-02: The `devflow.toml` key parses agent names more strictly than the `--fallback-agent` flag

**File:** `crates/devflow-core/src/config.rs:495-498`
**Issue:** `FallbackAgentKey` deserializes `AgentKind` through serde (`rename_all = "lowercase"`).
`--fallback-agent` parses through `FromStr`, which is case-insensitive and also accepts `open-code`. So
`fallback_agent = "Codex"` or `"open-code"` refuses `start`, while the same spelling works as a flag. The
refusal is loud, not silent.
**Fix:** Deserialize the key as a `String` and parse it with `AgentKind::from_str`, mapping the error into
the existing `devflow.toml: cannot read fallback_agent` message.

### IN-03: An unparseable `devflow.toml` is now reported as a `fallback_agent` problem even when the file has no such key

**File:** `crates/devflow-core/src/config.rs:525-537`, `crates/devflow-cli/src/commands.rs:410`
**Issue:** `config::fallback_agent` parses the whole file, so any TOML syntax error (for example an
unterminated string in `review_angles`) returns
`devflow.toml: cannot read \`fallback_agent\`: ... fix the file, or remove the \`fallback_agent\` key`.
`start` evaluates this before `config::base_branch`, so it is the first error the operator sees. The
operator may not have the key at all, and the advice to remove it is then wrong. Not a regression in
outcome: `base_branch` already refused an unparseable file.
**Fix:** Distinguish the two failures. A file that does not parse gets the generic `devflow.toml: {error}`
used by `base_branch`; the key-specific text is reserved for a parseable file whose `fallback_agent` value
names no agent.

### IN-04: The torn-final-line clause claims "a quota denial cannot be ruled out" for agents whose denials the classifier cannot detect at all

**File:** `crates/devflow-core/src/agent_result.rs:2607`, `:2652-2653`; `crates/devflow-core/src/monitor.rs:2742-2777`
**Issue:** `capture_rate_limit_denial` returns `TornAfterLastEvent` for any capture with a torn `{` line
after its last event, whatever the agent. `fire_idle_timeout` also runs for Antigravity, which has its own
`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`, and Antigravity is now the expected Code agent after an auto-mode
failover. But `detect_claude_stream_rate_limit` only recognizes Claude's `rate_limit_event`, so for an
Antigravity capture the "denial" is undetectable whether or not the line is torn. The clause therefore
suggests a classification gap that applies to every Antigravity idle timeout, torn or not. It changes only
the wording; the verdict is unaffected.
**Fix:** Gate the torn segment on the run's agent being Claude, or reword it agent-neutrally (for example
`torn final line after the last parsed event`).

---

_Reviewed: 2026-11-08T00:58:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
