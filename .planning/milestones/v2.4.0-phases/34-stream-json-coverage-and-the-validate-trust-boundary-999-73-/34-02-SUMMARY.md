---
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
plan: 02
subsystem: stream-json-rollout
tags: [criterion-7, criterion-1, canary-gate, capture-retention, evidence-layout, dogfood-03]
status: complete

requires:
  - "crates/devflow-cli/src/pipeline_launch.rs — claude_stream_launch_enabled's legacy_opt_out term"
  - "crates/devflow-core/src/agent_result.rs — prune_history's rsplit_once stamp grouping"
provides:
  - "canary_gate_only_applies_to_the_stream_launch_path rebuilt on the legacy opt-out — survives full five-stage widening"
  - "canary_gate_still_fires_for_a_widened_stage_without_the_opt_out — the gate's fire-direction half"
  - "DEFAULT_CAPTURE_RETENTION = 12 with its transition arithmetic recorded in source"
  - "prune_history_retains_a_full_five_stage_run_with_loop_backs — the retention regression pin"
  - "STREAM_JSON_STAGES' in-source stage-blind-argv record (criterion 1)"
  - ".planning/phases/34-.../34-evidence/{define,plan,code,validate,ship}/ — the capture landing spot"
affects:
  - "plan 34-05 — its precondition: the widening it performs now lands against a suite that survives it and an evidence tree that already exists"

tech-stack:
  added: []
  patterns:
    - "discriminator selection for tests that must outlive a rollout: prefer a term the predicate always respects (the opt-out) over one the rollout is designed to change (stage membership)"
    - "paired opposite-result halves at the level of the mechanism's effects, not just its return value"

key-files:
  created:
    - .planning/phases/34-stream-json-coverage-and-the-validate-trust-boundary-999-73-/34-evidence/README.md
    - .planning/phases/34-stream-json-coverage-and-the-validate-trust-boundary-999-73-/34-evidence/{define,plan,code,validate,ship}/.gitkeep
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/agent_result.rs

decisions:
  - "The canary test's false-branch premise moved to D-11's legacy opt-out rather than a non-Claude agent, per RESEARCH.md's recommendation — the opt-out keeps agent and stage constant, so only one variable moves between the paired halves"
  - "The 14 further test failures observed under a temporarily-widened constant were RECORDED, not fixed — widening is plan 34-05's deliverable and fixing its collateral here would have pre-empted it"
  - "cargo test selector corrected from --lib to --bin devflow: package devflow has no library target"

metrics:
  duration: "~16 min"
  completed: 2026-08-05

actuals:
  tokens: 5059
  tasks: 3
  commits: 3
---

# Phase 34 Plan 02: Criterion 7 Collateral and the Capture Campaign's Landing Spot — Summary

Three mechanical hazards that would have detonated during plan 34-05's widening are now defused
ahead of it: a canary test whose premise the widening destroys, a retention default that evicts an
unread capture on the first loop-back, and an evidence directory that did not exist.

## What Changed

**Task 1 — the canary test's discriminator moved off stage membership.**
`canary_gate_only_applies_to_the_stream_launch_path` took `Stage::Plan`'s absence from
`STREAM_JSON_STAGES` as its false-branch premise. `claude_stream_launch_enabled` is
`!legacy_opt_out && agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)`, so the
opt-out is a separate `&&` term the predicate respects whatever the constant contains. The test now
holds agent and stage constant (`Stage::Code`, which is on the stream path today and stays on it)
and moves only `legacy_claude_launch`. Its negative control is the same agent and the same stage
with the opt-out cleared — a control that widening cannot invalidate.

A companion test `canary_gate_still_fires_for_a_widened_stage_without_the_opt_out` covers the
gate's other direction (`calls.get() == 1`, outcome persisted), using `CanaryOutcome::Confirmed`.
The sibling can use `Absent` only because its gate never invokes the canary; here it does, and
`Absent` would fail the `unwrap` for a reason unrelated to what is under test.

**Task 2 — `DEFAULT_CAPTURE_RETENTION` 5 → 12**, with the arithmetic in its own doc comment: a
clean five-stage run produces 4 archive events, each Validate→Code loop-back adds 2, so
4 + (4 × 2) = 12 **exactly** — zero headroom at four loop-backs, real headroom through three. The
doc comment states that bound accurately and explicitly forbids the "survives four with headroom"
phrasing. `capture_retention`'s env/TOML precedence chain is untouched.

**Task 3 —** the stage-blind-argv fact is recorded beside the constant it qualifies, and the
five-stage evidence tree exists with a README carrying the `run.log` field list, the
copy-out-never-`git add -f` rule, the three PII fields, and the PATTERNS.md correction.

## Verification — Each Result With Its Opposite Case

Per the plan's prohibition, no check below is reported without the case that had to disagree.

### Task 1: the widening control

This is the measurement that matters, and it was run in both directions.

| Condition | `canary_gate_only_applies_to_the_stream_launch_path` |
|---|---|
| **Pre-rebuild test, constant widened to all five stages** | **FAILED** — `Stage::Plan must still resolve to the legacy path for this test to mean anything`, `pipeline_launch.rs:1777` |
| Rebuilt test, constant narrow (`&[Stage::Code]`) | `1 passed`, 272 filtered out |
| **Rebuilt test, constant temporarily widened to all five** | **`1 passed`**, 272 filtered out |
| Rebuilt companion test, widened | `1 passed`, 272 filtered out |

The first row is the negative control that makes the third row mean something. Without it, "the
rebuilt test passes under widening" would be consistent with a test that was never sensitive to the
constant at all. The old test *was* sensitive, and it broke — so the hazard was real, not
hypothetical, and the rebuild genuinely removes it.

**The temporary widening was made and reverted.** `STREAM_JSON_STAGES` is back to
`&[Stage::Code]` at `pipeline_launch.rs:446`, confirmed by grep after the revert and again in the
committed diff. Nothing in this plan widens the constant.

| Check | Result |
|---|---|
| `canary_gate_only_applies_to_the_stream_launch_path --exact` | `1 passed`, 272 filtered out |
| `canary_gate_still_fires_for_a_widened_stage_without_the_opt_out --exact` | `1 passed`, 272 filtered out |
| `legacy_launch_flag_forces_the_single_document_path --exact` | `1 passed`, 272 filtered out |
| `Stage::Plan` in the rebuilt test body (`-A 40`) | count **0** — the premise is gone |
| `legacy_claude_launch = true` on the state under test | present at `:1781` |
| second `claude_stream_launch_enabled(..., false)` asserted true | present at `:1797` |
| `cargo clippy -p devflow --all-targets -- -D warnings` | **exit 0** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0** |

### Task 2: both halves, reported together

| Condition | Result |
|---|---|
| **Test at the OLD default of 5 (RED)** | **FAILED** — `left: 5, right: 12`; 7 of 12 generations destroyed, survivors `{...007-0 … ...011-0}` |
| Test at the new default of 12, retain half | 12 stamps written, **all 12 survive** |
| Same test, evict half (13th stamp added) | exactly **one** stamp group removed, and it is the lowest-stamped |
| The evicted stamp's `-exit` file | also removed — eviction operates on the GROUP, not on a single file |

The RED run is doing double duty as a fixture control. Under the bare-`{nanos}-{seq}` naming trap
the review flagged, `prune_history` would have derived the wrong stamp and deleted nothing, so the
retain half would have false-passed via the `stamps.len() <= retain` early return. It instead
deleted 7 groups — which is only possible if the `-stdout` suffix is being parsed as
`prune_history` actually parses it.

| Check | Result |
|---|---|
| `prune_history_retains_a_full_five_stage_run_with_loop_backs --exact` | `1 passed`, 550 filtered out |
| `config::` module selector | `14 passed; 0 failed`, 537 filtered out — precedence chain undisturbed |
| `rg -n 'DEFAULT_CAPTURE_RETENTION: usize = 12'` | exactly 1 match |
| Doc comment carries `criterion 7` + `4 + (4 × 2) = 12` + the zero-headroom correction | confirmed in the `-B 26` window |
| Fixture filenames end in `-stdout`, one also `-exit` | confirmed at `:6175` and `:6180` |

### Task 3: the doc-comment window, not the file-wide count

The plan warned that `rg -c 'exec_command' … >= 1` is pre-satisfied and cannot fail.
**Pre-task measured count: 8** — the plan predicted 9; recording what was measured, not what was
predicted. The check actually run was the window check:

| Check | Result |
|---|---|
| `rg -B 30 'const STREAM_JSON_STAGES'` window contains `exec_command` | `:449` |
| …contains `_extra_writable_roots` | `:451` |
| …contains `byte-identical` | `:453` |
| `pipeline_launch::` module selector | `31 passed; 0 failed`, 242 filtered out |
| `LAYOUT_OK` + five stage subdirectories | confirmed, each with `.gitkeep` |
| `git status --porcelain` on the evidence tree | listed the tree as untracked — not swallowed by an ignore rule |
| `home_path` / `os_username` / `session_identifier` in README | three **separate** greps, each returning 1 |
| `three separate runs` in README | 1 |

**`_extra_writable_roots` confirmed unused at HEAD by reading the body, not the underscore.**
`ClaudeAgent::exec_command` (`crates/devflow-core/src/agents/claude.rs:46-64`) returns
`("claude", vec![...])` — a fixed literal. None of `_phase`, `_prompt` or `_extra_writable_roots`
appears anywhere in the body. The recorded claim matches source.

### Plan-level

| Check | Result |
|---|---|
| `scripts/check.sh all` | **exit 0**, captured directly (`CHECK_EXIT=0`), not via a pipeline |
| `cargo fmt --check` | exit 0 |
| devflow-core lib suite | `551 passed; 0 failed` (550 before, +1 new) |
| devflow bin suite | `273 passed; 0 failed` (271 before, +2 new) |

### What these results do NOT establish

- **They do not establish that plan 34-05's widening will be green.** They establish the opposite,
  and that is the most important finding here — see "Recorded for 34-05" below. This plan closes
  criterion 7's *named* collateral only.
- **The retention test does not prove 12 is sufficient for a real phase.** It proves
  `prune_history` retains 12 and evicts the 13th. Whether a real phase produces ≤ 12 archive events
  rests on the arithmetic in the doc comment, which is read from `archive_phase_files`' call
  pattern, not measured against a live run.
- **`551 passed` is regression surface, not evidence about this plan.** Only the four named tests
  bear on criteria 1 and 7.
- **The stage-blind-argv record is a fact about HEAD.** If a future change gives
  `exec_command` a use for `_phase`, the doc comment becomes wrong. It cites the file and line so
  the claim is checkable rather than merely asserted.

## Recorded for 34-05: widening breaks 14 further tests

Discovered while running the task-1 control, and deliberately **not fixed** — widening is 34-05's
deliverable and this plan's `success_criteria` explicitly forbid pre-empting it.

With `STREAM_JSON_STAGES` widened to all five stages, `cargo test -p devflow --bin devflow
pipeline_launch::` reports **14 failed** single-threaded (16 when parallel, the extra two being
`PoisonError` cascade). The root failure is a **live canary refusal**, not a broken assertion:

```
launch_stage_persists_monitor_pid_for_reload  pipeline_launch.rs:1073
  called `Result::unwrap()` on an `Err` value:
  Message("background-task notification delivery is ABSENT: ...")
```

Tests that previously exercised a legacy-path stage now route down the stream path, which invokes
the real `ClaudeCanaryLauncher`; it cannot confirm delivery in a test environment, so `canary_gate`
refuses and the launch `unwrap` fails. The first such failure poisons `ENV_MUTEX`, which is why the
failure count looks larger than the underlying cause.

**This is one mechanism, not 14 independent breakages**, and 34-05 will need to address it — most
plausibly by injecting a canary outcome in these fixtures the way the D-15 gate tests already do.
Flagging it now because a plan that budgets for "widen the constant" and discovers 14 red tests is
a plan that will be tempted to rush the fix.

## Deviations from Plan

### 1. [Rule 3 — Blocking] `cargo test -p devflow --lib` does not work; the target is `--bin devflow`

- **Found during:** Task 1, first command run.
- **Issue:** Every `<verify>` block for `pipeline_launch` specified `cargo test -p devflow --lib`.
  That fails immediately with `error: no library targets found in package devflow`. The
  `devflow-cli` crate's package name is `devflow` (as CLAUDE.md records) but it is a **binary**
  crate — `crates/devflow-cli/src/` has `main.rs` and no `lib.rs`.
- **Fix:** used `cargo test -p devflow --bin devflow` throughout. No source change.
- **Why this matters beyond a typo:** the failure mode is loud here (`no library targets`, non-zero
  exit), so it could not have false-passed. But it sits one keystroke away from this repo's
  recorded `--exact`-matches-nothing trap, and the plans for 34-03/34-04/34-05 carry the same
  wrong selector. **Every plan in this phase that tests `pipeline_launch` needs the same
  correction.**
- **Commit:** n/a — verification command only.

### 2. [Scope boundary — recorded, not fixed] 14 test failures under widening

See "Recorded for 34-05" above. Out of scope per the plan's own success criteria; no source change
made and no fix attempted.

### 3. [Measurement correction] Pre-task `exec_command` count is 8, not 9

The plan's acceptance criterion cites 9 occurrences at HEAD as the reason the bare `rg -c` check is
pre-satisfied. The measured count in this worktree is **8** (8 matching lines, 8 matches — checked
both ways). The criterion's *reasoning* is unaffected: 8 ≥ 1 just as surely as 9 does, so the bare
count check remains pre-satisfied and the window check remains the right one. Recording the
discrepancy rather than repeating the plan's number.

## TDD Gate Compliance

Both `tdd="true"` tasks ran a genuine RED before their GREEN, and in both cases the RED is quoted
above with its actual failure text.

The gate's **commit sequence** is partially visible in git log: `0ed4f6b` is typed `test(...)` and
`24ca16a` is typed `fix(...)`, but they cover different tasks rather than forming a RED→GREEN pair
for one feature. Task 2's RED (test at retention 5) and GREEN (constant raised to 12) landed in the
single commit `24ca16a`, because the executor's task-commit protocol commits once per task.

A reviewer looking for a `test(...)` → `feat(...)` pair on the retention change will not find one.
The red state is evidenced by the quoted `left: 5, right: 12` failure, which is reproducible by
setting `DEFAULT_CAPTURE_RETENTION` back to `5`.

## Known Stubs

The five `34-evidence/{stage}/` directories are **intentionally empty**, holding only `.gitkeep`.
This is the plan's stated deliverable, not an unfinished one: task 3's action explicitly says "Do
not create or copy any capture in this task — there is nothing to capture until plan 34-05 widens
the constant and rebuilds the binary. This task creates the landing spot only."

**Resolved by:** plan 34-05, which performs the capture campaign and lands `raw_output.jsonl` +
`run.log` into each directory.

No placeholder values in source, no `TODO`/`FIXME` markers added, no skipped tests, and every
`<verify>` block in the plan was executed.

## Threat Flags

None. The plan's register is covered as written:

- **T-34-02-01** (retention DoS) — mitigated by task 2 and pinned by both halves of the new test.
- **T-34-02-02** (vacuous canary test) — mitigated by task 1, verified under a fully-widened
  constant with the pre-rebuild failure as the control.
- **T-34-02-03** (evidence-directory disclosure) — the README names the three PII fields and states
  the copy-out rule. **The blocking control remains plan 34-05's checkpoint**; this plan only makes
  the requirement visible at the landing spot, and an empty directory cannot itself disclose
  anything.
- **T-34-02-04** / **T-34-02-SC** — accepted as planned; no dependency added.

No new network endpoint, auth path, file-access pattern or schema change. The only production
change is one integer constant.

## Rollback

`git revert --no-commit 0ed4f6b^..61c8e67` and commit once.

**Do not revert this plan ahead of plan 34-05.** The canary rebuild is what lets the suite survive
34-05's widening; reverting it while all five stages are widened turns
`canary_gate_only_applies_to_the_stream_launch_path` red — demonstrated directly by this plan's own
RED control. If only 34-02 must be withdrawn, narrow `STREAM_JSON_STAGES` back to `&[Stage::Code]`
in the same operation.

**`DEFAULT_CAPTURE_RETENTION` is independently revertible** — back to `5` touches nothing else, and
the env/TOML overrides are unchanged either way. Note that reverting it re-opens T-34-02-01 and will
turn the new retention test red, which is the intended alarm.

**Do not revert the `34-evidence/` tree** once 34-05 has landed captures into it; scope the revert
to the three source files.

## Self-Check: PASSED

- `.planning/phases/34-.../34-02-SUMMARY.md` — being written now; committed in the same step.
- `34-evidence/README.md` and all five `.gitkeep` files — FOUND on disk and in commit `61c8e67`.
- `0ed4f6b`, `24ca16a`, `61c8e67` — all FOUND in `git log`.
- `STREAM_JSON_STAGES` — confirmed back to `&[Stage::Code]`; the temporary widening is not in any
  commit.
- STATE.md and ROADMAP.md deliberately NOT modified (worktree mode; the orchestrator owns those
  writes after the wave completes).
