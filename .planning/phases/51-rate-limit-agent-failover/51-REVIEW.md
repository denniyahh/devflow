---
phase: 51-rate-limit-agent-failover
reviewed: 2026-11-08T16:30:00Z
depth: deep
files_reviewed: 11
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/tests/fallback_agent_config.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/tests/failover_e2e.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/monitor.rs
  - OPERATIONS.md
findings:
  critical: 0
  warning: 2
  info: 4
  total: 6
  necessity: 0
status: issues_found
---

# Phase 51: Code Review Report (re-review after fix round 1)

**Reviewed:** 2026-11-08T16:30:00Z
**Depth:** deep
**Files Reviewed:** 11
**Status:** issues_found

## Summary

This re-review covers the six fix commits after `68a0fc5` (9886a95, 7e8ee1a, 982d443, cd35c7a,
0f6c79c, c6e29e0). It read the fix hunks and the code they call or are called from:
- `start` → `config::fallback_agent` → `validate_fallback_agent`;
- `handle_stage_failure` → `relaunch_failed_stage` → `recorded_canary_refusal`, compared with
  `canary_gate`;
- the supervisor's post-advance path (`record_supervisor_stop`, `finish`);
- `resume_with_fallback` → `resume_loaded` → `launch_stage` → `canary_gate` → `refuse_launch`;
- `status` / `doctor` `recovery_hints`.

No Critical issue was found. Every prior finding's defect is closed at the place it was
reported. Two new problems were found:
- The WR-02 replacement records its "way on" (`devflow resume --phase N --agent <other>`) only in
  `stop_reason`. `devflow status` then recommends the plain `devflow resume --phase N`. The
  recorded canary refuses that command, and the refused resume overwrites `stop_reason` with
  text that does not mention `--agent`.
- The WR-01 fix makes `devflow parallel` silently ignore a configured `fallback_agent` key, even an
  invalid one. `config.rs`'s own doc names that outcome as the reason the key fails hard.

**Checked at HEAD (bca5446):**
- These four named tests passed, one at a time, with
  `cargo nextest run --no-tests=fail -E 'test(=...)'`:
  - `config::tests::fallback_agent_key_resolves_from_devflow_toml`
  - `fallback_agent_config::parallel_ignores_the_config_key`
  - `failover_e2e::approving_the_failed_canary_gate_stops_the_run_and_names_the_way_on`
  - `failover_e2e::rejecting_the_failed_canary_gate_stops_the_run_and_names_the_way_on`
- The working tree was still clean afterwards, and no test process was left running.

**What this does not establish:**
- I did not re-run the fixer's negative controls, because that needs a source edit. So "the test
  fails with the fix reverted" is read from the test code (each WR-02 test waits for
  `gate_relaunch_refused`, which only the new function emits) and from 51-REVIEW-FIX.md. I did
  not observe it.
- I did not run `scripts/check.sh all`.
- New WR-01 and IN-02 come from reading the code and were not reproduced at runtime.

## Prior Finding Verdicts

| Prior finding | Verdict | Evidence |
|---|---|---|
| WR-01: `parallel` arms `devflow.toml`'s `fallback_agent` | **Resolved** | `parallel.rs:144` passes `FallbackSource::NoFallback`, and `commands.rs:431` maps it to `(None, false)` without reading the file. `commands.rs:425` is the only production call of `config::fallback_agent`, and it runs only under `FlagOrConfig`. The only other production caller of `start` is `main.rs:646` (`FlagOrConfig`). The new test (key `"claude"` = agent `claude`) would fail on the `start` refusal if `parallel` read the key, and its `fallback_agent: null` assertion also catches arming without the refusal. Passed at HEAD. The fix introduces new WR-02. |
| WR-02: the gate after a canary-refused failover offers retry/loop answers that end the run silently | **Partially resolved** | The silent end is closed. Both gate arms (`pipeline_outcomes.rs:937`, `:945`) go through `relaunch_failed_stage`, which commits `stopped` + `stop_reason` + `gate_pending=false` before emitting `gate_relaunch_refused` (`:977-990`). Its predicate (`pipeline_launch.rs:636-650`) matches `canary_gate`'s (`:570-626`) for the same `state`. I traced `advance_for_stage`'s `Completed` arm (`pipeline_launch.rs:1657-1696`): nothing later overwrites the stop. Both new e2e tests passed at HEAD, and they wait on the new event, so they discriminate. Not resolved: the "names the way on" half. The operator-facing surfaces still point at the dead-end command (new WR-01). One narrow entry point of the same class remains (new IN-02). |
| IN-01: guard comments understate the guard's scope | **Resolved** | Comment-only change. The new text at `pipeline_launch.rs:667-670`, `:2089-2093` and `:2122-2123` matches the code: the guard spans the whole `launch_stage` call at `:3264-3267`, which includes `run_preflight` (`:3030`). |
| IN-02: key parsed more strictly than the flag | **Resolved** | `FallbackAgentKey.fallback_agent` is `Option<String>`, parsed with `AgentKind::from_str` (`config.rs:547-549`). The flag uses the same `FromStr`: `main.rs:126-127` is a plain `#[arg(long)] Option<AgentKind>`, and `AgentKind` has no `ValueEnum`. The test asserts `"Codex"` and `"open-code"`. Passed. |
| IN-03: a syntax error blamed on the `fallback_agent` key | **Resolved** | The file is parsed as `toml::Table` first, and a syntax error maps to `devflow.toml: {error}` (`config.rs:542-543`), the same text `base_branch` uses (`config.rs:405`). Only a parseable file whose value is not an agent gets the key-specific text. The test checks both directions, and the e2e `a_valid_flag_does_not_mask_an_invalid_config_key` still asserts only `devflow.toml` for the unparseable case. Passed. |
| IN-04: the torn-line clause claims a denial gap for agents the classifier cannot read | **Resolved** (clause) | `TORN_FINAL_LINE_SEGMENT` is agent-neutral (`agent_result.rs:2612`), its pinned literal was updated (`:7583`), and no stale copy remains in `crates/`, `docs/` or `OPERATIONS.md`. One small leftover: the monitor's own warn and log lines (new IN-04). |

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: After the WR-02 stop, `devflow status` sends the operator to the plain `devflow resume`, which is refused and overwrites the only record of the way on

**File:** `crates/devflow-cli/src/commands.rs:1648-1652`, `:1674-1676`; `crates/devflow-cli/src/pipeline_launch.rs:3630-3636`, `:3664-3671`, `:600-611`; `crates/devflow-cli/src/pipeline_outcomes.rs:972-981`
**Issue:** `relaunch_failed_stage` writes the way on (`devflow resume --phase N --agent <other>`) into
`stop_reason` and the event log only. The operator's surfaces then contradict it:
1. `recovery_hints` gives a stopped, not-live phase exactly one hint, `continue_verb` =
   `devflow resume --phase N` (`commands.rs:1674-1676`). `status` does not print `stop_reason`, as
   51-REVIEW-FIX.md itself notes. So the only advice the operator sees is the command the recorded
   canary refuses.
2. Running that command, `resume_loaded` clears `stopped` and `stop_reason` (`pipeline_launch.rs:3632-3635`),
   then `launch_stage` → `canary_gate` meets the recorded `Absent` and refuses. Then
   `resume_loaded` re-marks the run stopped with
   `stop_reason = "resume launch failed: <canary text>"` (`:3669`). The canary text
   (`:600-611`) names `claude_code_version 2.1.220` even for an Antigravity run. It never mentions
   `--agent`. After the first wrong attempt, the state file no longer names the way on anywhere.

Before 7e8ee1a, the operator also had to find `--agent` from OPERATIONS.md, so this is not a
regression in outcome. But the fix's stated goal ("names the way on") is not met on the path an
operator would actually take, and no test covers the stop → `status` → `resume` sequence. Read
from the code; not reproduced at runtime.
**Fix:** Name the way on where the operator looks, and stop the dead end from erasing it:
```rust
// commands.rs, recovery_hints: a stopped run whose recorded canary refuses its agent
PhaseLiveness::Gone | PhaseLiveness::NotLaunched if state.stopped => {
    if crate::pipeline_launch::recorded_canary_refusal(state).is_some() {
        vec![format!("devflow resume --phase {phase} --agent <other>")]
    } else {
        vec![continue_verb]
    }
}
```
In `resume_loaded`, refuse before the commit that clears `stopped`, when no `--agent` handoff is
requested and `recorded_canary_refusal(&state)` is `Some`. The refusal should name `--agent <other>`
and write nothing, so `stop_reason` survives. Add an e2e that, after
`approving_the_failed_canary_gate_...`:
- checks `devflow status` for the `--agent` hint;
- checks that a plain `devflow resume` leaves `stop_reason` unchanged;
- runs `devflow resume --agent claude` and checks the canary clears and Code relaunches.

That last step is the remedy the stop prescribes, and no test runs it today.

### WR-02: `devflow parallel` now silently ignores a configured `fallback_agent` key, including an invalid one

**File:** `crates/devflow-cli/src/parallel.rs:130-145`, `crates/devflow-cli/src/commands.rs:431`, `crates/devflow-core/src/config.rs:519-522`
**Issue:** The WR-01 fix chose "parallel never arms a fallback" and implemented it as a silent no-op.
`NoFallback` does not read `devflow.toml`, prints nothing, and saves `fallback_agent: null`. That
is the outcome `config::fallback_agent`'s own doc gives as the reason the key fails hard
(`config.rs:519-522`: "a silently ignored fallback would leave the operator believing a fallback
is armed"):
- An operator who set `fallback_agent = "antigravity"` and runs `devflow parallel` gets runs with
  no failover. No line on stdout says so.
- A key that names no agent (`"nosuch"`) refuses `start` but passes `parallel` silently.
- README.md:110 says failover applies when "a fallback agent is configured (`--fallback-agent`, or
  `fallback_agent` in `devflow.toml`)", with no exception for `parallel`. The OPERATIONS.md
  `parallel` row (`:40`) does not mention it.
- `docs/guides/configuration.md:14` says "read by `devflow start` only". `parallel` calls `start`
  internally, so an operator cannot tell from that sentence that `parallel` is excluded.

**Fix:** Keep the behaviour, but make it loud. In `parallel`, before the loop, read
`config::fallback_agent(project_root)` once. On `Ok(Some(agent))`, print:
`note: devflow.toml's fallback_agent ({agent}) is not armed by devflow parallel; use devflow resume --phase N --fallback-agent {agent} per run`.
On `Err`, refuse as `start` does. Add "takes no fallback; ignores `devflow.toml`'s `fallback_agent`"
to the OPERATIONS.md `parallel` row and to README.md:110. Extend
`parallel_ignores_the_config_key` to assert the note.

## Info

### IN-01: `recorded_canary_refusal` is a second copy of `canary_gate`'s refusal mapping, and nothing pins the two together

**File:** `crates/devflow-cli/src/pipeline_launch.rs:636-650` (copy), `:596-626` (original), `:183-186` (the drift warning)
**Issue:** `launch_stage_inner` computes the stream predicate once, with the comment "two separate
evaluations of 'is this the stream path?' would be free to drift" (`:183-186`). The new
`recorded_canary_refusal` re-evaluates `stream_launch_enabled` and re-implements the
`Confirmed`/`Absent`/`Unverified` → refuse mapping. No unit test asserts that it agrees with
`canary_gate`. Suppose a future change makes `canary_gate` re-run the canary on `Unverified`, or
admits a stage. The pre-check would then stop runs whose relaunch would succeed, and every
current test would still pass. The doc comment says "the same predicate", but that is enforced
only by keeping the two copies identical by hand.
**Fix:** Extract `fn canary_refuses(outcome: &CanaryOutcome) -> bool` (or one returning the message
kind) and call it from both `canary_gate` and `recorded_canary_refusal`. Or add a unit test that
enumerates `{stream, legacy} × {None, Confirmed, Absent, Unverified}` and asserts
`recorded_canary_refusal(&s).is_some()` iff `canary_gate(&mut s.clone(), stream, || unreachable!())`
returns `Err`.

### IN-02: A gate retry whose canary has not yet been recorded can still end the run silently (999.187 residual at the same entry point)

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:967-969`, `crates/devflow-cli/src/pipeline_launch.rs:3192-3210`, `:671-676`
**Issue:** The pre-check sees only a *recorded* refusal. `hand_off_agent` clears `canary`
(`:3208`). If the failover relaunch fails before `canary_gate` runs, for example a binary removed
between `hand_off_agent`'s check and `launch_stage`'s `ensure_agent_binary`, or a `run_preflight`
error, the infra gate opens with `canary == None`. A retry answer then passes the pre-check. It
runs a fresh canary in an unguarded `launch_stage`. If that canary refuses, `refuse_launch`
clears the supervisor record, and the run ends the 999.187 way. Reachable only through a
transient failure between two identical checks, so this is Info. 51-REVIEW-FIX.md names the
broader residual ("a gate answer whose relaunch fails for another reason"), but not this
canary-specific one.
**Fix:** In `relaunch_failed_stage`, treat an `Err` from `launch_stage` the way the pre-check
treats a recorded refusal: commit `stopped` + `stop_reason`, emit `gate_relaunch_refused`. That
needs the record kept on that path, so either a guard variant scoped to gate-answer relaunches
(and the source-scan pin widened), or an explicit re-check of `state.canary` after the error. Or
record the case against 999.187 explicitly.

### IN-03: The never-silent gate still offers "retry, loop-to-code, or abort" when the recorded canary has already decided that two of the three stop the run

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:918-923`, `:931`
**Issue:** `stage_failure_context` is built before `run_gate`, without consulting
`recorded_canary_refusal`. In the failover-canary case the operator is asked to choose between
three answers, and both non-abort answers lead to the WR-02 stop. The gate text names
`the handoff to <agent> could not relaunch...`, but not that a retry will not relaunch, and not
the `--agent` way on. The notify hook carries this same context, so an unattended operator
decides from it alone.
**Fix:** When `recorded_canary_refusal(state)` is `Some` at the top of `handle_stage_failure`, append:
`a retry will not relaunch under {agent} (the recorded canary refuses it): answer it, then run devflow resume --phase {phase} --agent <other>`.

### IN-04: The monitor's torn-line warning now says "a Claude quota denial cannot be ruled out" on every agent's run, including the Antigravity run IN-04 was about

**File:** `crates/devflow-core/src/monitor.rs:2780-2794`
**Issue:** `fire_idle_timeout` has no agent parameter (`:2711-2717`), so the reworded warn and
`[idle-timeout]` log lines name Claude for an Antigravity capture too. In the case the original
IN-04 described (an Antigravity Code stage after a failover), the operator's monitor log now
names an agent that is not running. It does not change any verdict.
**Fix:** Make the wording conditional ("if the agent is Claude, a quota denial cannot be ruled
out"), or pass `AgentKind` into `fire_idle_timeout` and drop the clause for non-Claude agents.

---

_Reviewed: 2026-11-08T16:30:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
