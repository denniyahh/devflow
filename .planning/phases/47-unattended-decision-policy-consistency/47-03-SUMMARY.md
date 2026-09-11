---
phase: 47-unattended-decision-policy-consistency
plan: 03
subsystem: prompt-rendering
tags: [prompt, decision-policy, tdd, insta, snapshot, checkpoint-resume, claude]

requires:
  - phase: 47-02
    provides: "CODE_STAGE_POLICY on the FullExecute fix arm (so turn 1 already carried the policy section) and the fix-prompt baseline this plan drifts again"
provides:
  - "`pub const GATE_RESOLUTION_RULE`, defined once through `gate_resolution_rule!()` and spliced into CODE_STAGE_POLICY — the single definition of who may resolve a blocking-human gate (D-04)"
  - "CODE_STAGE_POLICY with the two-class sentence split: an unconditional package-verification prohibition plus a blocking-human carve-out, pinned character for character (D-02)"
  - "`checkpoint_auto_decide_prompt` names blocking-human and states the same condition in the affirmative"
  - "`resume_launch_shape(phase, session_id)` — the checkpoint-resume argv as a builder testable without spawning, which 47-04's pair snapshots consume"
  - "Layered contradiction test (constant level + two-turn delivery), both observed red on the pre-fix tree (D-03)"
affects: [47-04, 47-05, "DECN-03", "Phase 49 observation item"]

actuals:
  tokens: 4880        # chars/4 over the realized code diff f98e404..cab8d54 -- crates (19522 bytes)
  tasks: 3
  commits: 3          # MEASURED: git rev-list --count f98e404..HEAD before the docs commits
plan_head_before: f98e404d6c42b914726fb37fdb3a9974676088a1

tech-stack:
  added: []
  patterns:
    - "A sentence shared by a concat!-built const and a pub const lives in one macro_rules! literal, because concat! rejects a const name"
    - "Controls are asserted one by one first; rule checks are collected so a red run names every contradiction, not only the first"
    - "A delivery test captures each turn from the production constructor that delivers it and never joins them"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap
    - .planning/phases/47-unattended-decision-policy-consistency/deferred-items.md

key-decisions:
  - "resume_launch_shape(phase, session_id) builds the instruction itself, and relaunch_checkpoint_session keeps its own copy for the pre-spawn checkpoint_auto_decided event. A debug_assert pins that copy to argv[1]. The plan's third-tuple option would have put the call before the emit and failed the plan's own source-order criterion."
  - "The turn-1 strengthening ADDS contains(GATE_RESOLUTION_RULE) next to the absence check for the old conjunction, rather than replacing that check"
  - "checkpoint_auto_decide_prompt writes blocking-human without backticks, the same wording GATE_RESOLUTION_RULE uses"

patterns-established:
  - "Pin text an operator settled with a test-owned literal, never one built from the constants under test, and prove the pin by widening it"

requirements-completed: [DECN-03]  # copied verbatim from PLAN frontmatter; NOT marked in REQUIREMENTS.md — requirements.ready-ids reported 0/1 ready (47-01, 47-04 and 47-05 also declare DECN-03; 47-04/47-05 have no SUMMARY)

coverage:
  - id: D1
    description: "A resumed Claude session gets one gate rule: the layered contradiction test failed on the pre-fix tree for both halves and passes after the fix"
    requirement: DECN-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::the_gate_rule_has_one_definition_site"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns"
        status: pass
      - kind: other
        ref: "47-03-PLAN.md Task 2 gate (red, gate_rc=0) and Task 3 gate run 3 (gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The carve-out covers blocking-human only; the package-verification prohibition is pinned verbatim and the pin was observed failing under a widening"
    requirement: DECN-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::package_verification_prohibition_is_unconditional"
        status: pass
      - kind: other
        ref: "Task 3 gate pinned_section_in_snapshot=1; widening control exit 101 then 0"
        status: pass
    human_judgment: false
  - id: D3
    description: "resume_launch_shape extracted with the audit emission unmoved; the three relaunch_checkpoint_session tests are byte-identical to the fork point and pass"
    requirement: DECN-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::relaunch_checkpoint_session_emits_exactly_one_audit_event"
        status: pass
      - kind: other
        ref: "Task 1 gate (unmodified_exit=0 x3, resume_shape_code_sites=2, gate_rc=0) + source-order check emit 1129 < call 1142 < spawn 1153"
        status: pass
    human_judgment: false
  - id: D4
    description: "Fix-prompt snapshot re-blessed; the diff is one line, and nothing outside the pinned section grants authority"
    requirement: DECN-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::claude_style_full_execute_fix_prompt_snapshot"
        status: pass
    human_judgment: true
    rationale: "The plan names snapshot-diff review as the ONLY guard against an authorization added outside the pinned section. The executor read a one-line diff; phase review should confirm it independently rather than trust the executor's reading."

duration: 28min
completed: 2026-09-11
status: complete
---

# Phase 47 Plan 03: One Gate Rule for the Code Policy and the Resume Prompt Summary

**A resumed Claude session now receives one rule about who may resolve a `blocking-human` gate.
`GATE_RESOLUTION_RULE` is a single macro-backed definition. `CODE_STAGE_POLICY` includes it after an
unconditional, pinned package-verification prohibition, and `checkpoint_auto_decide_prompt` names
`blocking-human` with the same condition. A layered test that failed on both halves before the fix
now passes.**

## Performance

- **Duration:** ~28 min (code); the docs and state steps followed
- **Started:** 2026-09-11T18:46:20Z
- **Completed:** 2026-09-11T19:14:36Z (last code commit)
- **Tasks:** 3 (Task 1 auto extraction, Task 2 RED, Task 3 GREEN)
- **Files modified:** 4 (3 code/snapshot + `deferred-items.md`)

## Accomplishments

- The contradiction is gone at the prompt level. Before, `CODE_STAGE_POLICY` forbade self-resolving a
  `blocking-human` gate outright, while the resume instruction told the agent to resolve a
  "human-blocking checkpoint". Now the policy states the prohibition with one named condition, and the
  resume instruction states that same condition in the affirmative.
- The carve-out is scoped to `blocking-human` alone. Package verification keeps an unconditional
  prohibition, pinned word for word.
- The resume instruction a real resume delivers is testable without spawning a process
  (`resume_launch_shape`). The audit event still fires before the spawn.

## Task Commits

1. **Task 1: extract resume_launch_shape** — `e2fb6c7` (refactor; `type="auto"`, not a TDD gate)
2. **Task 2: layered contradiction tests, RED** — `94ef699` (test)
3. **Task 3: one definition site, split sentence, tightened resume prompt, re-blessed snapshot** — `cab8d54` (feat)

## Criterion 3 evidence — pre-fix `test result: FAILED` (tree at `94ef699`)

Constant level:

```
running 1 test
test prompt::tests::the_gate_rule_has_one_definition_site ... FAILED
thread 'prompt::tests::the_gate_rule_has_one_definition_site' panicked at crates/devflow-core/src/prompt.rs:1252:9:
the two gate-rule texts disagree: [
    "CODE_STAGE_POLICY still forbids a blocking-human gate unconditionally, in the same sentence as package verification",
    "checkpoint_auto_decide_prompt does not name the blocking-human gate class it resumes the agent to resolve",
]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 767 filtered out; finished in 0.00s
```

Two-turn delivery:

```
running 1 test
test pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns ... FAILED
thread 'pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns' panicked at crates/devflow-cli/src/pipeline_launch.rs:4078:9:
the delivered turns disagree on the gate rule: [
    "turn 1 forbids a blocking-human gate unconditionally",
    "turn 2 does not name the blocking-human gate it resumes the agent to resolve",
]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 363 filtered out; finished in 0.00s
```

**Which assertion carried the red.** Both halves failed on each side, because the rule checks are
collected rather than stopping at the first one. On turn 1 the red came from the un-split two-class
sentence, not from a missing policy. The control `turn1.contains("package-verification")` held, which
shows 47-02's policy section was already in turn 1. None of the other controls fired: the stream-launch
precondition, `argv[0] == "-p"`, `assert_ne!`, mutual non-containment, and the completion-protocol
suffix all held. Both targets printed `running 1 test`, so neither is a zero-match filter.

**RED classification.** `gsd-tools check tdd-red-evidence` parses node TAP only, so each record's
`output` is a mechanical TAP transcription of the cargo result (`not ok 1 - <target>`,
`# tests 1 / # pass 0 / # fail 1`). Both real records returned `RED_EVIDENCE_OK (target_test_failed)`.
The control, with the same output and a wrong `targetTest`, returned
`INVALID_RED (no_target_test_failure)`. The checker exits 0 on both verdicts, so the verdict field
was read, not the exit code.

## Task 3 — negative controls

**Pin widening.** A temporary edit changed `and report it instead. "` to
`and report it instead unless DevFlow resumed you. "` in the package sentence. A Python replace was
checked for exactly 1 match; the widened line count was 1:

```
running 1 test
thread 'prompt::tests::package_verification_prohibition_is_unconditional' panicked at crates/devflow-core/src/prompt.rs:1327:9:
CODE_STAGE_POLICY no longer carries the pinned gate-rule section verbatim
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 768 filtered out; finished in 0.00s
```

Restored from a byte copy (`cmp` exit 0), the test printed `test result: ok. 1 passed; … 768 filtered out`.

**Second negative control (RESEARCH B-2 step 4).** Only the six body lines of
`checkpoint_auto_decide_prompt` went back to their pre-fix wording (1 match; afterwards
`resume_body_names_blocking_human=0` and `resume_body_old_wording=1`, with the policy change still in
place):

```
running 1 test
thread 'pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns' panicked at crates/devflow-cli/src/pipeline_launch.rs:4083:9:
the delivered turns disagree on the gate rule: [
    "turn 2 does not name the blocking-human gate it resumes the agent to resolve",
]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 363 filtered out; finished in 0.00s
```

Only the turn-2 contradiction fired. That shows the strengthened turn-1 assertion
(`contains(GATE_RESOLUTION_RULE)`) is satisfied by the policy alone, and that the resume-prompt
tightening is what turn 2 depends on. Restored (`cmp` exit 0), the test printed
`test result: ok. 1 passed; … 363 filtered out`.

**Task 1's debug_assert, observed live.** With `resume_launch_shape` temporarily building the prompt
for `PhaseId::new(1)`, `relaunch_checkpoint_session_emits_exactly_one_audit_event` exited 101:
`assertion left == right failed: the checkpoint_auto_decided event must quote the instruction the resume delivers`.
Restored byte-identically, it passed again.

## Snapshot re-bless

- **Pinned drift:** `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no … claude_style_full_execute_fix_prompt_snapshot -- --exact`
  exited 101 and wrote **0** `.snap.new` files. One `.snap.new` did appear earlier, but it came from an
  unpinned `cargo test -p devflow-core --lib prompt::` run in the same command. It was deleted and the
  pinned run was repeated to separate the two.
- **Re-bless:** `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=always …` exited 0 (`updated snapshot`), leaving 0 `.snap.new` files.
- **The diff, read before staging:** 1 insertion and 1 deletion, at line 19 only:
  - `-This authority does not extend to a \`blocking-human\` gate or a package-verification checkpoint. Those remain human-only: do not self-resolve or approve them; report them instead. This policy must not pause execution or request human input.`
  - `+This authority never extends to a package-verification checkpoint: do not self-resolve or approve one under any circumstances, and report it instead. A blocking-human gate is human-only, with one exception: if DevFlow resumed you specifically to resolve that gate, resolve it yourself and record your reasoning in your final message; otherwise report it instead. This policy must not pause execution or request human input.`

  No other line of the rendered prompt changed, so no authorization was added outside the pinned section.

## Did any pre-existing test have to change?

**No.** This was measured, not inferred:

- `relaunch_checkpoint_session_*` (all three): each body is byte-identical to the fork point
  `034f5b6`. The windows were 43, 41 and 39 lines with one `fn` each, and `unmodified_exit=0` for all
  three (Task 1 gate). Assumption A5 held.
- `git diff -U0 f98e404..HEAD` removes 12 lines from `prompt.rs` (old lines 61-62, 86-89, 549,
  563-568) and 1 from `pipeline_launch.rs` (old line 1119). `mod tests` starts at old line 633 and
  1632 respectively, so **0 removed lines fall inside either test module**. The removed text is the
  policy header and paragraph, the resume-prompt doc and body, and the one `exec_resume_command` call.
- The `read_first` tests all pass unmodified: `code_policy_excludes_blocking_human_and_package_checkpoints`,
  `code_stage_prompt_is_deterministic` and the four `checkpoint_auto_decide_prompt_*` tests. The
  `prompt::` suite read 29 passed with only the snapshot failing before the re-bless, and every test
  was green after it.

## Automated gate counters (as printed)

| Gate | Printed counters | Exit |
|---|---|---|
| Task 1 | `fork_point=034f5b6…`; per test `base_lines=43/41/39`, `fn_decls=1/1/1`, `unmodified_exit=0/0/0`; `resume_shape_code_sites=2` `cargo_exit=0` (363 passed) `failed_binaries=0` `gate_rc=0` | 0 |
| Task 2 (on the formatted tree that was committed) | `cargo_exit_cli=101` `cli_failed_binaries=1` `cli_zero_passed_lines=1` `cli_test_actually_ran=1` `cargo_exit_core=101` `core_failed_binaries=1` `core_zero_passed_lines=1` `core_test_actually_ran=1` `test_body_lines=81` `stream_precondition_in_test=1` `assert_ne_in_test=1` `gate_rc=0` | 0 |
| Task 3 run 1 | all counters met except `check_test_exit=101` → `gate_rc=1` (pre-existing race, see Issues) | 1 |
| Task 3 run 2 | all counters met except `check_test_exit=101` → `gate_rc=1` (a different pre-existing race, see Issues) | 1 |
| Task 3 run 3 | `pv_test_one_passed=1` `pv_filtered=768 filtered out` `fix_prompt_snapshot_files=1` `pinned_section_in_snapshot=1` `gate_rule_sites_in_prompt_rs=5` `gate_rule_sites_in_cli=2` `cli_one_passed=1` `cli_filtered=363 filtered out` `core_one_passed=1` `core_filtered=768 filtered out` `check_test_exit=0` `gate_rc=0` | 0 |

**Task 2 gate limitation.** Its `-A 80` window runs past the end of the 67-line test. On the unformatted
tree it counted `stream_precondition_in_test=3`, picking up the next test's occurrences. Counted within
the test's own body (`fn` line to closing brace), the values are `stream_launch_enabled=1`,
`assert_ne=1`, and 0 `format!`/`concat!` joins of the two turns.

**Task 3 run 3 totals** (from the `check.sh` log): 30 binaries `ok`, 0 `FAILED`, 1281 passed, 0 failed,
0 `.snap.new` files. That count reconciles: 47-02 recorded 1278, and this plan added 3 tests. fmt
(`cargo fmt --all --check`) and `cargo clippy --workspace --all-targets -- -D warnings` both exited 0
on the committed bytes.

## Files Created/Modified

- `crates/devflow-core/src/prompt.rs`:
  - adds `gate_resolution_rule!` and `pub const GATE_RESOLUTION_RULE`;
  - builds `CODE_STAGE_POLICY` with `concat!` and splits its final paragraph;
  - tightens `checkpoint_auto_decide_prompt` and its doc;
  - adds `the_gate_rule_has_one_definition_site` and `package_verification_prohibition_is_unconditional`.
- `crates/devflow-cli/src/pipeline_launch.rs`:
  - adds `resume_launch_shape`;
  - `relaunch_checkpoint_session` now calls it, with a `debug_assert_eq!`;
  - adds `the_gate_rule_holds_in_both_delivered_turns`.
- `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap`: re-blessed (one line).
- `.planning/phases/47-unattended-decision-policy-consistency/deferred-items.md`: two pre-existing test races recorded.

## Decisions Made

- **Instruction built inside `resume_launch_shape`.** The plan offered two ways to handle `instruction`
  being consumed twice. Returning it as a third tuple element forces the call before the emit,
  violating the acceptance criterion that the emit precedes the call. Passing it in would make the test
  supply its own instruction, so the test would no longer exercise the builder a resume uses. The chosen
  shape keeps the artifacts table's `resume_launch_shape(phase, session_id)` signature, which 47-04 depends on.
- **Controls first, then collected rule checks.** A control that fails is a broken test, not a red.
  Collecting the rule checks is what let both RED captures name both halves.
- **No backticks around blocking-human in the resume prompt**, matching `GATE_RESOLUTION_RULE`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] debug_assert tying the audit event to the delivered instruction**
- **Found during:** Task 1
- **Issue:** Once `resume_launch_shape` builds the instruction itself, the `checkpoint_auto_decided`
  event quotes a separately built copy. The two can only agree while both call the same builder with
  the same phase, and nothing enforced that.
- **Fix:** `debug_assert_eq!(args[1], instruction, …)` in `relaunch_checkpoint_session`, after the call
  and before the spawn. Release behaviour is unchanged.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Verification:** A mutated builder made the audit-event test fail with the assertion message
  (exit 101); restored, it passed.
- **Committed in:** `e2fb6c7`

**2. [Plan interpretation] The turn-1 assertion was strengthened by addition, not replacement**
- **Found during:** Task 3
- **Issue:** The plan says to "strengthen … from the Task 2 string literal to `contains(GATE_RESOLUTION_RULE)`".
- **Fix:** Added the positive check and kept the absence check. Removing a working assertion was not
  needed, and keeping it cannot weaken the test.
- **Committed in:** `cab8d54`

---

**Total deviations:** 1 auto-fixed (Rule 2), 1 plan interpretation.
**Impact on plan:** No scope creep. Both deviations are additive guards within the two edited files.

## Issues Encountered

- **Task 3 gate run 1 failed on a pre-existing cli test race.**
  `test_support::tests::an_empty_path_child_cannot_resolve_git_while_the_parent_still_can` saw PATH
  with a `/tmp/.tmp…` stub prefix (363 passed, 1 failed). The test reads global PATH without
  `env_lock()` while sibling tests set it under `env_lock()`. It passed alone and in 5 other full-bin
  runs on the same tree.
- **Task 3 gate run 2 failed on a different pre-existing devflow-core race.** The lib reported 668
  passed and 101 failed in 2.35s, every one a spawn `NotFound`. The source is `PathGuard` in
  `agents/pi.rs` and `agents/opencode.rs`: it replaces global PATH with a stub-only tempdir under a
  **module-local** `ENV_MUTEX`, while the `git.rs`, `hooks.rs`, `version.rs` and `worktree.rs` tests
  spawn through PATH with no lock at all. The lib passed 3 of 3 immediate re-runs (769 passed).
- **Why neither is this plan's.**
  - 0 of 240 added lines touch `PATH`, `env_lock`, `set_var` or `prepend_path`.
  - PATH-writer counts in `crates/devflow-cli/src` are 117 at `f98e404` and 117 at HEAD.
  - `test_support.rs`, `pi.rs` and `opencode.rs` are all unchanged since `f98e404`.

  Both are recorded in `deferred-items.md` and were not fixed (scope boundary). Run 3 passed.
  **Not established:** neither failure was reproduced on the pre-47-03 tree, and neither failure rate
  was measured. Two failures in three `check.sh` runs means the required CI Test job is intermittently
  red on this host for reasons unrelated to the change under test.
- zsh's `$pipestatus` was not needed: every exit code was read inside `bash -c`. One interim reading was
  corrected: `doc_refs_exit=0` came from `head`, not `rg`. Re-run unpiped with a positive control,
  `cargo doc` appears in none of CI, `check.sh` or pre-push.

## What this does NOT establish

- **Which instruction a model follows** (ROADMAP criterion 4). That is deliberately not claimed here;
  47-05 hands it to Phase 49.
- **Composition inside the session transcript.** The two-turn test proves the text each production
  constructor delivers. It does not observe a real resumed Claude session holding both turns.
- **Other adapters.** DECN-03 closes for claude only. The resume route is gated on `AgentKind::Claude`,
  and `exec_resume_command` exists only on `ClaudeDriver` (D-11).
- **Authorizations elsewhere in the policy.** The pin covers only its section. The one-line snapshot
  diff was read here, and phase review should confirm it (coverage D4).
- **Repeatability.** Each negative control ran once, and nothing ran in the pinned CI container.

## Threat Model Coverage

- **T-47-09:** mitigated. The pin test was observed red under a widening, and the gate found the pinned
  text exactly once in the re-blessed snapshot. The out-of-section residual stands as the plan states it.
- **T-47-10:** mitigated. The "record your reasoning in your final message" clause is in
  `GATE_RESOLUTION_RULE`, and `checkpoint_auto_decide_prompt` keeps its "MUST record your reasoning" clause.
- **T-47-11:** mitigated.
  - The three relaunch tests are byte-identical to the fork point.
  - The emit (line 1129) precedes the call (1142) and the spawn (1153).
  - The debug_assert binds the emitted text to the delivered text.
- **T-47-12:** mitigated. There is no join, the own-body count of `format!`/`concat!` is 0, and
  `assert_ne!` plus the non-containment check are present.
- No new security surface: prompt text, a pure builder, a debug-only assertion, and tests.

## TDD Gate Compliance

- **RED:** `94ef699` `test(47-03)`. Both targets failed on assertions (`RED_EVIDENCE_OK`, with the
  wrong-target control reading `INVALID_RED`).
- **GREEN:** `cab8d54` `feat(47-03)`. RED is an ancestor of GREEN (`merge-base --is-ancestor` exit 0),
  and no `feat(47-03)` commit existed before RED (count 0; the control count for `feat(47-02)` was 1).
- **REFACTOR:** none needed after GREEN. The only `refactor(47-03)` commit, `e2fb6c7`, is Task 1's
  plain extraction (`type="auto"`) and **precedes** RED; it is not a REFACTOR gate.
- The RED tests are inline `#[cfg(test)]` tests in `src/`. They fall outside the gate's
  `tests/**` / `*.test.*` path heuristic, as the Rust convention does.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 47-04. `resume_launch_shape(phase, session_id)` and `resolve_launch_shape` are the
  constructors its pair snapshots use, and `the_gate_rule_holds_in_both_delivered_turns` exists in
  `pipeline_launch.rs`'s `mod tests`.
- DECN-03 is still unmarked in REQUIREMENTS.md: 47-04 and 47-05 also declare it.
- `graphify-out/` was refreshed locally and left uncommitted; `.planning/user/errors/` was left uncommitted.
- Scratch artifacts (RED records, control script, logs) live in the session scratchpad, not the worktree.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/prompt.rs`, `crates/devflow-cli/src/pipeline_launch.rs`, and the `.snap` baseline.
- FOUND: commits `e2fb6c7`, `94ef699` and `cab8d54`, all ancestors of HEAD. Measured count `f98e404..HEAD` = 3.
- FOUND, 1 each with comment lines stripped:
  - `fn resume_launch_shape(`
  - `pub const GATE_RESOLUTION_RULE: &str = gate_resolution_rule!();`
  - `macro_rules! gate_resolution_rule`
  - the three new test functions
- Stub scan over added lines: `stub_hits_added_lines=0`; the control scan of the same lines for `GATE_RESOLUTION_RULE` returned 11.
- The old conjunction left in `prompt.rs` code (comments stripped): 0.
