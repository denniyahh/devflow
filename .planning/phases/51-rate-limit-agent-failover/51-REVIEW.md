---
phase: 51-rate-limit-agent-failover
reviewed: 2026-11-08T18:30:05Z
depth: deep
files_reviewed: 13
files_reviewed_list:
  - OPERATIONS.md
  - README.md
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/tests/failover_e2e.rs
  - crates/devflow-cli/tests/fallback_agent_config.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/monitor.rs
  - docs/guides/configuration.md
findings:
  critical: 0
  warning: 2
  info: 3
  total: 5
  necessity: 0
status: issues_found
---

# Phase 51: Code Review Report (re-review after fix round 2 and the external-review fixes)

**Reviewed:** 2026-11-08T18:30:05Z
**Depth:** deep
**Files Reviewed:** 13
**Status:** issues_found

## Summary

Scope: `git diff b30f806 HEAD` over the 13 listed files. That covers:
- the fix round for the previous review (5ced4d8 WR-01, 1106d67 WR-02, 02a2af7 IN-01,
  1c77853 IN-02, a722892 IN-03, e0683ce IN-04);
- the external-review fixes: 73303a9 and 91e1b76 (a failover handoff inside a phase worktree),
  b94d356 (gate list context at 300 chars), 17a47cf (a torn final line outranks a parsed quota
  denial), and the doc commits ee7451a, e4bf0c5 and 2213057.

I traced these call chains:
- `recovery_hints` → `recorded_canary_refusal`;
- `resume_with_fallback` → `resume_loaded` → `refuse_recorded_canary_for_resume` →
  `apply_legacy_launch_opt_out`;
- `fail_over_on_rate_limit` → `handle_infra_outcome` → `gate_or_abort_infra` →
  `handle_stage_failure` → `run_gate` → the banner, `gate list` and notify renderings;
- `fire_idle_timeout` → `capture_rate_limit_denial` → `classify_capture_denial`, then the
  supervisor's reader thread (`read_until`, the tee at EOF) → `evaluate_layer1` →
  `parse_claude_event_result`;
- `AutoChainGuard` → `gsd_config::set_auto_chain_active`, to check whether the worktree test's
  `tree_dirty` assertion discriminates.

No Critical issue was found. Two new Warnings:
1. **The IN-03 "way on" sentence never fits in the pending-gate banner or `gate list`.** Its
   fixed parts alone are 272 characters, and both renderings keep 261. So the command it names
   is always cut, whatever the reason's length.
2. **The X-01 fix (17a47cf) closes the disagreement only when the torn line reaches the capture
   before the monitor checks it.** The code itself says a killed child's partial line arrives
   at EOF, after that check. In that order the monitor still logs "classified as rate-limited
   (resumable)" and writes no record, while the verdict is an indeterminate `Failed`. The
   outcome is still a gate with no failover, so it fails safe. But the log, OPERATIONS.md's new
   sentence and the "cannot disagree" doc comment are wrong for that order.

**How this was checked:** by reading code and diffs only, using `git` and `rg`, plus one Python
length calculation. Per the constraints, no `cargo` build or test was run.

**What this does not establish:**
- None of the new or changed tests was run here.
- The two Warnings were not reproduced at runtime. Each rests on traced code, plus a character
  count for WR-01.
- `scripts/check.sh all` was not run.

## Prior Finding Verdicts

| Prior finding | Verdict | Evidence |
|---|---|---|
| WR-01: `status` sends the operator to a plain `resume`, which is refused and overwrites the way on | **Resolved** | `commands.rs:1654-1655` names `--agent <other>` whenever `recorded_canary_refusal` is `Some`, and that covers every hint built from the continue verb (Stale, GoneWhileActive, stopped). `refuse_recorded_canary_for_resume` (`pipeline_launch.rs:3568`, body `:3692-3714`) runs after `refuse_fallback_for_resume` and before `commit_notify_env`, `close_gates_for_resume` and the `stopped` clear (`:3627-3633`), so a refused plain resume writes nothing. A `--agent X` with X different from the current agent skips the refusal and goes to `hand_off_agent`, which clears the canary. The e2e `after_the_refused_relaunch_status_and_resume_lead_to_another_agent` covers status, then a plain resume (`stop_reason` and `canary` unchanged), then `--agent claude`. Small leftover: new IN-01 (the env opt-out). |
| WR-02: `parallel` silently ignores a configured `fallback_agent` | **Resolved** | `parallel.rs:119-128` reads `config::fallback_agent` once, before the first launch. An `Err` refuses; `Some` prints the note. README, the OPERATIONS.md `parallel` row and `configuration.md` were all updated. The new test refuses `"nosuch"` and has a no-file control. |
| IN-01: `recorded_canary_refusal` duplicates `canary_gate`'s mapping | **Resolved** | `recorded_canary_refusal_agrees_with_canary_gate_on_every_recorded_outcome` (`pipeline_launch.rs:7344`) enumerates agents × stages × legacy × outcomes and has an admitted/refused control. The fix report states its stated limit (the stream flag comes from the same `stream_launch_enabled` call). |
| IN-02: a gate retry with no recorded canary can still end silently | **Recorded, not fixed** (as dispositioned) | No code change. Recorded against 999.187. Not re-raised. |
| IN-03: the gate offers retry/loop-back when the recorded canary already refuses both | **Partially resolved** | The sentence is appended (`pipeline_outcomes.rs:935-944`), it reaches `gate_fired` and the notify hook in full (`pipeline_gate.rs:591`, `gates.rs:1552`), and the e2e asserts it. It is never visible in the banner or in `gate list`: new WR-01. |
| IN-04: the torn-line warning names Claude on every agent's run | **Resolved** | `monitor.rs:2786-2798` now uses conditional wording, and the monitor test asserts it. |

**The later fixes:**
- **b94d356 (gate list at 300): resolved.** All three renderings use `GATE_CONTEXT_DISPLAY_CHARS`,
  and the new unit test compares the list, the all-roots row and the banner. One stale doc
  comment is left (IN-02).
- **73303a9 / 91e1b76 (worktree handoff): sound.**
  - The `tree_dirty == true` assertion discriminates. The workspace builds `serde_json` with
    `preserve_order` (`Cargo.toml:27`), and the rig commits `_auto_chain_active: false`
    (`failover_e2e.rs:210`). When `AutoChainGuard` drops, before the advance
    (`pipeline_launch.rs:1586-1599`), the file round-trips to the committed bytes. So at handoff
    time only the primary's leftover work makes the tree dirty.
  - The config-equality check removes only `_auto_chain_active`.
- **17a47cf (torn line outranks a denial): partially resolved.** See new WR-02.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: The IN-03 "way on" sentence is always cut from the pending-gate banner and `gate list`

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:917-922`, `:935-944`; `crates/devflow-cli/src/commands.rs:2043`, `:2060`, `:2104`, `:2160`
**Issue:** `handle_stage_failure` appends the way-on sentence after the whole
`stage_failure_context`. The banner (`devflow status`) and `devflow gate list` keep at most 261
characters: `render_gate_context(.., 300)` keeps 300, minus its 39-character truncation suffix.
The fixed parts of the context are:
- `[never-silent] stage code failed: `: 34 characters;
- ` — human review needed (retry, loop-to-code, or abort)`: 54 characters;
- the appended sentence for `antigravity` / phase 51: 184 characters.

That is 272 characters before any reason text. Even the shortest reason (`no details available`)
pushes the sentence's closing `devflow resume --phase N --agent <other>` past the cut.

In the case IN-03 was about, the reason is
`the handoff to antigravity could not relaunch the code stage: <canary refusal text>`, which is
longer than 300, so `truncate_reason` caps it at 300. The sentence then starts at about
character 389, and none of it shows in the banner or the list. The operator who checks
`devflow status` or `gate list` before answering sees the start of the canary text and
`… [truncated; full output in .devflow/]`, and nothing saying that a retry will not relaunch.
Only the notify hook, the `gate_fired` event and `gate show` carry it.

This is the same class of defect that b94d356 fixed for the D-15 cause clause, where
`MAX_IDLE_TIMEOUT_CLAUSE_CHARS` was budgeted to close inside the display bound. The new
sentence was never budgeted. The e2e test asserts only the full `gate_fired` context, so it
cannot catch this.

**Fix:** Put the sentence where the display bound keeps it: before the reason, as D-15 places
its cause clause. Keep it short, and pin it with a test that renders it through the banner.
```rust
// pipeline_outcomes.rs, handle_stage_failure
let way_on = recorded_canary_refusal(state).map(|_| {
    format!(
        "retry/loop-back will not relaunch under {} (recorded canary refuses it); \
         answer, then `devflow resume --phase {} --agent <other>`. ",
        state.agent, state.phase
    )
});
let context = match way_on {
    Some(lead) => format!("[never-silent] stage {stage} failed: {lead}{}", truncate_reason(&reason_text)),
    None => stage_failure_context(stage, reason),
};
```
Then add a unit test asserting that
`render_pending_gate_banner(&open, now).unwrap().contains("--agent <other>")` for a gate whose
reason is 400 characters long.

### WR-02: The X-01 fix closes the monitor/verdict disagreement only for a torn line that is already in the capture; a killed child's partial line still reproduces it

**File:** `crates/devflow-core/src/monitor.rs:2751-2766`, `:4318-4322`; `crates/devflow-core/src/agent_result.rs:2604-2621`, `:1903-1913`; `OPERATIONS.md:315-319`
**Issue:** `fire_idle_timeout` classifies the capture while the child is still alive. The reader
thread tees whole lines only (`monitor.rs:1342`, `read_until(b'\n')`). The test doc at
`monitor.rs:4318-4322` says it outright: "a killed child's partial line arrives at EOF, after
this check (the read side covers that case)". For a denial followed by a partial line, the
order is:
1. The capture holds `init` and `rate_limit_event rejected`. The partial `{"type":"res` is still
   in the pipe, so `classify_capture_denial` returns `Denied`.
2. The monitor logs `[idle-timeout] suppressed ...: classified as rate-limited (resumable), not
   as a hang`, writes **no** idle-timeout record, and kills the group.
3. At EOF the partial line is teed. `evaluate_layer1` finds no record, so
   `parse_idle_timeout_side_channel`'s read-time torn re-check (`agent_result.rs:2675-2676`)
   never runs. `parse_claude_event_result`'s torn guard (`:1903-1907`) then returns
   `indeterminate_capture_failure()`: `Failed`, "stream capture ends in an unparseable line".

This is X-01's disagreement: the monitor says rate-limited and the verdict says something
else. The outcome is still a gate with no failover and no cron (option B's outcome), so nothing
unsafe happens. But three new statements are wrong for this order:
- `classify_capture_denial`'s doc says the shared rule means "the two cannot disagree"
  (`agent_result.rs:2606`).
- The test doc says "the read side covers that case". It covers it only when the monitor wrote
  a record, which it skips on `Denied`.
- OPERATIONS.md:318 says "At an idle timeout it is recorded as one, flagged torn". In this order
  there is no record, no commit list in the reason, and the gate reads as an indeterminate
  failure.

The new monitor test prints the torn line with a newline (`printf '%s\n'`). That is the shape
the test doc calls "the only way a torn line can reach the capture before the kill", so the
realistic order is not tested.

**Fix:** Re-classify after the reader has drained to EOF, and write the flagged record when the
answer changed. For example, have `fire_idle_timeout` return the suppression decision, and let
the caller, after the reader thread ends, call
`capture_rate_limit_denial(project_root, phase)` again. If that now returns
`TornAfterLastEvent`, write the idle-timeout record with `torn = true` (the commits can be
enumerated then) and log `[idle-timeout] suppression withdrawn: a torn line followed the
denial`. Add a monitor test whose stub prints the denial and then `printf '%s' '{"type":"res'`
(no newline) before sleeping. If the fix is deferred instead, correct the three statements
above to name this order as a residual.

## Info

### IN-01: The status hint ignores the environment's legacy opt-out, but `resume`'s refusal applies it

**File:** `crates/devflow-cli/src/commands.rs:1654`; `crates/devflow-cli/src/pipeline_launch.rs:3700-3702`
**Issue:** `recovery_hints` calls `recorded_canary_refusal(state)` on the persisted
`legacy_claude_launch`. `refuse_recorded_canary_for_resume` first applies
`apply_legacy_launch_opt_out`, which reads `DEVFLOW_CLAUDE_LEGACY_LAUNCH`. Take a Claude run
with a recorded `Absent` canary and that variable set in the operator's shell: `status` says
`resume --agent <other>`, yet a plain `resume` is admitted and launches through the legacy path.
The hint errs toward the safe side, so this is Info. Still, the two surfaces judge the same
state differently, which is the drift `apply_legacy_launch_opt_out`'s doc warns against.
**Fix:** Either state in the hint's doc comment that the hint reads the persisted state only, or
judge a clone with `apply_legacy_launch_opt_out(&mut candidate, false)` in `recovery_hints`, as
the refusal does.

### IN-02: A stale "100-char" doc comment on `gate_show`

**File:** `crates/devflow-cli/src/commands.rs:4415-4416`
**Issue:** The comment still reads "the discoverability counterpart to `gate_list`'s 100-char
table truncation". b94d356 changed that to `GATE_CONTEXT_DISPLAY_CHARS` (300). The comment at
`:4444-4445` and `main.rs:503` were both updated, but this one was missed.
**Fix:** Change it to "`gate_list`'s `GATE_CONTEXT_DISPLAY_CHARS` truncation".

### IN-03: Nothing ties `GATE_CONTEXT_DISPLAY_CHARS` to the 300 that `MAX_IDLE_TIMEOUT_CLAUSE_CHARS` was derived from

**File:** `crates/devflow-cli/src/commands.rs:2039-2043`; `crates/devflow-core/src/agent_result.rs:2340-2349`; `crates/devflow-cli/src/commands.rs:8299`
**Issue:** The new constant's doc says the cause clause "is budgeted to close inside this
bound". The budget is a hand derivation in a `devflow-core` doc comment (300 − 39 = 261,
minus a 38-character prefix). The new test hard-codes `<= 261`, and `truncate_reason` uses its
own literal 300 (`pipeline_outcomes.rs:889`). If someone changes the CLI constant, no test
fails, and the clause that b94d356 exists to show would silently be cut again.
**Fix:** In the new test, derive the bound from the constants rather than the literal. For
example, assert that a context built from the longest prefix plus a clause of
`MAX_IDLE_TIMEOUT_CLAUSE_CHARS` survives
`render_gate_context(.., GATE_CONTEXT_DISPLAY_CHARS)`, and expose the core constant as
`pub(crate)` → `pub` (additive) or mirror it with a `const _: () = assert!(...)`.

---

_Reviewed: 2026-11-08T18:30:05Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
