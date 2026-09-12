---
phase: 47-unattended-decision-policy-consistency
reviewed: 2026-09-12T17:15:45Z
depth: deep
diff_range: ead8443..7a7770a
files_reviewed: 13
files_reviewed_list:
  - ARCHITECTURE.md
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap
  - scripts/test-phase-worktree-guard.sh
findings:
  critical: 0
  warning: 2
  info: 5
  total: 7
status: issues_found
---

# Phase 47: Code Review Report (gap-closure re-review, plans 47-06 / 47-07)

**Reviewed:** 2026-09-12T17:15:45Z
**Depth:** deep
**Files Reviewed:** 13
**Status:** issues_found

This report replaces the earlier review at this path. The original findings (CR-01, WR-01) are at
`git show ead8443:.planning/phases/47-unattended-decision-policy-consistency/47-REVIEW.md`. The
implementer's "Finding Resolutions" section was not used as evidence.

## Summary

**CR-01 is closed at the level of prompt wording and the parser. That says nothing about whether a
live model complies.** Every prompt that carries the decision-reasoning mandate
(`code_stage_prompt`, `fix_prompt(FullExecute)`, `workflow_code_prompt` for None/FullExecute, and
`checkpoint_auto_decide_prompt`) now ends with a completion protocol that puts reasoning above a
final `DEVFLOW_RESULT` line. None of those prompts has any "final message must be exactly" wording
left. I confirmed this against all nine snapshots in scope, not only the diff hunks. Every adapter
reads the marker through the same `parse_marker_lines` tail scan: plain text and the Claude envelope
via `parse_devflow_result`, the Claude stream at `agent_result.rs:1710`, Codex at `:801`, OpenCode
at `:959`, and Antigravity at `:1919`. So reasoning above the marker parses the same way for every
agent. The new parser test exercises only plain text, the Claude single-document envelope and the
Claude v3 stream. The Codex, OpenCode and Antigravity coverage comes from reading the code, not from
a test.

**WR-01 is closed only for the two cases it reported: inherited hooks and inherited signing.** The
wider problem, inherited git config in general, is still open (WR-01 below). Removing either
fixture-local pin by itself fails control 7c, and the unmutated copy passes. But a global
`core.excludesFile` aborts the harness before cases 2, 3, 6 and 7 run, which is the same failure
shape WR-01 described. A leaked `GIT_OBJECT_DIRECTORY` also escapes the fixture. On top of that, no
CI job, `check.sh` target or hook runs the harness (WR-02), so the "permanent" 7c control never runs
unless someone invokes it by hand.

No Critical findings. Neither change set introduces incorrect runtime behaviour that I could
demonstrate.

### What the checks here do NOT establish

- Whether a live Claude, Codex, OpenCode, Antigravity, Hermes or Pi agent actually writes its
  reasoning above the marker, or avoids copying the gate-declaration line. Phase 49's live
  observation still owns that.
- Whether the real operator global config would have broken the fixture today. On this host, the
  mutants with one pin removed still passed cases 1–6: the global gitleaks hook ran and SSH signing
  succeeded. So the host run of cases 1–6 is **not** evidence for the fix. Only the synthetic 7c
  control is.
- Whether the Claude CLI puts earlier assistant text blocks into the final `result`. If a model
  writes reasoning, then calls a tool, then emits the marker, the reasoning is absent from the
  capture the audit relies on.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: Fixture hermeticity closes two config keys, not the inherited-config class

**Classification:** WARNING

**File:** `scripts/test-phase-worktree-guard.sh:15`, `scripts/test-phase-worktree-guard.sh:49-58`

**Issue:** The fix pins `core.hooksPath` and `commit.gpgsign` in the fixture's local config and
adds two names to a hand-written `unset` list. Any other inherited global or system setting, or any
repo-local git environment variable not on that list, still reaches the fixture. Both escapes were
reproduced against the unmodified script:

- **E1**, a global `core.excludesFile` that ignores `.planning/` (`GIT_CONFIG_GLOBAL=<file>`).
  `git add -A` silently skips `.planning/`, and line 88's `git add .planning` then exits 1 under
  `set -e`. The harness exits 1 with no `passed=` line, after only cases 0/5/1/4, so controls 2, 3,
  6 and 7a–7c never run. That is the WR-01 failure mode: an environment-dependent abort that stops
  the controls and reads like a guard failure. A negative control with an empty excludes file passed
  13/0. The default `$XDG_CONFIG_HOME/git/ignore` applies even without `core.excludesFile`, so this
  needs no unusual setup. On this host that file exists but matches no fixture path.
- **E2**, `GIT_OBJECT_DIRECTORY` exported, which is not in the `unset` list. The harness passes
  13/0 but writes 17 objects outside `$TMP`, where the `trap` cleanup never reaches. Git's own list
  of repo-local variables (`git rev-parse --local-env-vars`) has 15 names. Line 15 clears 8 of them;
  `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_IMPLICIT_WORK_TREE`,
  `GIT_GRAFT_FILE`, `GIT_NO_REPLACE_OBJECTS`, `GIT_REPLACE_REF_BASE` and `GIT_SHALLOW_FILE` are
  missing.

The comment on line 12, "Hermetic: the host checkout's git env must not leak into the fixture",
claims more than the code does.

**Fix:** Remove the class for the whole harness. Keep the local pins as defence in depth so 7c stays
meaningful:

```bash
# after `set -euo pipefail`
unset $(git rev-parse --local-env-vars)
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
```

I verified this on scratch copies. Under E1 it passes 13/0. Under E2 it passes 13/0 with 0 objects
outside `$TMP`. With the fix applied and the `core.hooksPath` pin removed, 7c still FAILs (exit 1),
so the control still catches a removed pin. `lint-phase-worktree.sh` reads no git config, so a null
global config does not bypass the behaviour under test. 7a–7c export their own `GIT_CONFIG_GLOBAL`
inside subshells and are unaffected.

### WR-02: The harness that evidences WR-01's closure is never executed by any gate

**Classification:** WARNING

**File:** `scripts/test-phase-worktree-guard.sh:1-149`; `scripts/check.sh` (no invocation);
`.github/workflows/ci.yml:63,75,87,180` (runs only `scripts/check.sh test|clippy|fmt|all`);
references only in comments at `scripts/hooks/pre-commit:95` and `scripts/lint-phase-worktree.sh:28`

**Issue:** A repo-wide search, excluding `.planning/`, `graphify-out/` and `target/`, finds
`test-phase-worktree-guard.sh` only in its own mode check and in two comments. Nothing runs it: not
`check.sh`, not CI, not a hook. So the 7a–7c controls added by 47-07, and the guard's cases 1–6,
cannot fail any automated gate. A regression in `init_fixture_repo` or in `lint-phase-worktree.sh`
would ship green. The pre-commit comment "Both directions are exercised by
scripts/test-phase-worktree-guard.sh" implies coverage that does not exist.

**Fix:** Run it from the `test` target of `scripts/check.sh`, which CI already calls in the pinned
container:

```bash
# scripts/check.sh, test target
bash scripts/test-phase-worktree-guard.sh
```

Then confirm on the PR that the CI log shows `passed=13 failed=0`, and that a deliberately broken
guard makes the job fail. The harness's case-0 mode check needs a real git checkout. CI has one;
I did not check whether the container user can create `mktemp -d` worktrees there.

## Info

### IN-01: Completion contract now has two divergent definitions; ARCHITECTURE.md over-generalises

**File:** `crates/devflow-core/src/prompt.rs:294-315` (`VALIDATE_VERDICT_CONTRACT`),
`prompt.rs:272`, `:352`, `:480`, `:564`; `ARCHITECTURE.md:112-115`

**Issue:** 47-06 reworded `COMPLETION_PROTOCOL` but not `VALIDATE_VERDICT_CONTRACT`. That constant
is a second `## Completion Protocol (REQUIRED)` block, and it still says "your FINAL message must be
exactly one of" and "Output nothing after it". That is nearly the sentence the new test forbids in
`COMPLETION_PROTOCOL` (`agent_result.rs`, `!protocol.contains("When all work is done, your FINAL
message must be exactly:")`). The Plan and Ship branch lines also still say "must be exactly", and
then append the new protocol that allows content above the marker. None of these prompts carries the
reasoning mandate, so this is not CR-01 again. Still, `ARCHITECTURE.md:112-115` now states for
*every* prompt that the result is "required as the last line … with any decision reasoning above
it", which is not what the Validate prompt says.

**Fix:** Either align `VALIDATE_VERDICT_CONTRACT` and the branch sentences with "the LAST line of
your FINAL message", or narrow the ARCHITECTURE.md sentence to the stages that use
`COMPLETION_PROTOCOL`.

### IN-02: Resume-prompt detector test cannot fail for the change it accompanies; the mitigation is prompt wording only

**File:** `crates/devflow-core/src/agent_result.rs:3713-3744`
(`resume_prompt_does_not_read_as_a_blocking_human_checkpoint`);
`crates/devflow-core/src/prompt.rs:599-604,621`; `crates/devflow-cli/src/pipeline_launch.rs:1594-1596`

**Issue:** The negative assertion `!text_reports_human_gate(&resume)` would also have passed on the
pre-47-06 prompt ("gate, but" has no colon). It tests the prompt text, but the Legacy resume capture
is a single-document `--output-format json` envelope and the prompt is a positional argument
(`pipeline_launch.rs:1148-1160`), so the prompt is never echoed into the capture the detector reads.
The risk the new sentence targets is the agent copying a `**Gate:** \`blocking-human\`` line, and no
test at this level can cover that. The only new thing the test pins is the literal
`"gate-declaration line"`. The detector (`agent_result.rs:693-713`) was not narrowed. It still
matches any `gate` substring followed by `:` and `blocking-human`, so prose such as "Resolved gate:
blocking-human (Task 2)" matches.

The same detector also re-reads failed first-turn Code captures (`pipeline_launch.rs:1594-1596`),
where `CODE_STAGE_POLICY` now requires free-text reasoning. That prompt got no equivalent sentence.
The consequence is bounded: the detector runs only on `GateReview`, and a false positive costs a
spurious auto-resume up to `MAX_CHECKPOINT_RESUMES`, recorded in the `checkpoint_auto_decided` event.

**Fix:** Describe the test as a regression pin on prompt wording rather than a boundary proof, and
state the residual risk in the `prompt.rs:599-604` doc comment. If the false positive matters,
narrow the detector to a line-anchored `**Gate:**` label, rerunning the live-rendering tests it
documents.

### IN-03: Negative controls accept any non-success, including results reached for unrelated reasons

**File:** `crates/devflow-core/src/agent_result.rs:3696-3706`

**Issue:** `assert_ne!(status_for(&after), Some(AgentStatus::Success))` and its stream twin pass for
`None`, `Failed` or any other status. For the stream variant, the fixture's turns 1 and 2 carry
`MARKER_FAILED`. If the stream parser regressed to scanning all turns, the control would still pass,
on a `Failed` from turn 2 rather than on the tail budget it is meant to measure.

**Fix:** Pin the expected value: `assert_eq!(status_for(&after), None)` for plain text, and assert
the specific status or reason `evaluate_layer1` returns for `after_stream`.

### IN-04: New rustdoc intra-doc links are unresolved

**File:** `crates/devflow-core/src/prompt.rs:601`, `crates/devflow-core/src/prompt.rs:603`

**Issue:** `cargo doc -p devflow-core --no-deps` reports `unresolved link to
crate::agent_result::text_reports_human_gate` (a private fn) and `unresolved link to
crate::agent_result::tests::resume_prompt_does_not_read_as_a_blocking_human_checkpoint` (the
`tests` module does not exist outside `cfg(test)`). These are 2 of 43 doc warnings; the others
predate this change. No CI step runs rustdoc with `-D warnings`, so nothing fails.

**Fix:** Use plain code spans (`` `agent_result::text_reports_human_gate` ``) instead of link
brackets for private and test-only items.

### IN-05: Isolation controls 7a–7c are under-specified

**File:** `scripts/test-phase-worktree-guard.sh:112-115`, `:76`, `:137-140`

**Issue:**
1. The hostile `post-checkout` hook (lines 112-115) never fires: 7a, 7b and 7c perform no checkout.
   The only post-checkout trigger in the harness is the main fixture's `git worktree add` at line
   76, which runs under the host config with no hostile control. Local `core.hooksPath` covers it
   in practice, but no case shows that.
2. 7b asserts only a non-zero exit with the hook not fired. It never checks that signing caused the
   failure. I confirmed the cause is signing on this host ("Couldn't load public key …", exit 128;
   the same commit with `-c commit.gpgsign=false` exits 0). The test itself would pass on any other
   failure, for example a broken `git init`.

**Fix:** Either drop the unused `post-checkout` hook, or add a hostile `git worktree add` case.
Capture 7b's stderr and require the signing error text before counting the exit code.

---

## Closure assessment

| Original finding | Verdict | Basis |
|---|---|---|
| CR-01 (reasoning vs. "exactly" final message) | **Closed** in prompt wording and the parser | The policy-carrying prompts and all 9 in-scope snapshots contain no conflicting "exactly" wording. `parse_marker_lines` is shared by every adapter. Test `decision_reasoning_above_the_result_line_parses_to_that_result` passes on plain text, the envelope and the v3 stream, with more than 5,000 characters of reasoning above the marker. Live model compliance is not established. |
| WR-01 (fixture inherits global hooks) | **Closed for hooks and signing; wider class open** | Removing either pin by itself fails 7c (hooksPath removed: exit 1; gpgsign removed: exit 128), and the unmutated copy passes 13/0. E1 and E2 still escape the fixture (WR-01 above), and no gate runs the harness (WR-02). |

## Checks actually run

All cargo commands used `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`. No snapshot files were
written.

| Command | Result |
|---|---|
| `cargo test -p devflow-core --lib -- --exact agent_result::tests::decision_reasoning_above_the_result_line_parses_to_that_result agent_result::tests::resume_prompt_does_not_read_as_a_blocking_human_checkpoint` | `2 passed; 0 failed; 770 filtered out`, both names printed |
| `cargo test -p devflow-core --lib -- prompt::tests` | `31 passed; 0 failed; 741 filtered out` (includes `claude_style_full_execute_fix_prompt_snapshot`, `policy_carrying_full_execute_fix_prompt_snapshots`) |
| `cargo test -p devflow-core --lib -- agent_result::tests` | `198 passed; 0 failed; 574 filtered out` |
| `cargo test -p devflow --bin devflow -- pipeline_launch::tests` | `54 passed; 0 failed; 310 filtered out` (includes `the_gate_rule_holds_in_both_delivered_turns`, which owns both CLI snapshots) |
| `cargo doc -p devflow-core --no-deps` | exit 0, 43 warnings, 2 introduced here (IN-04) |
| `bash scripts/test-phase-worktree-guard.sh` (host config) | `passed=13 failed=0`, exit 0 |
| Scratch copies, unmutated / hooksPath pin removed / gpgsign pin removed | 13/0; 7c FAIL (exit 1); 7c FAIL (exit 128) |
| Unmodified harness under E1 (global excludes `.planning/`) / empty-excludes control / E2 (`GIT_OBJECT_DIRECTORY`) | exit 1, aborted before 2/3/6/7x; 13/0; 13/0 with 17 objects outside `$TMP` |
| Proposed WR-01 fix under E1 / E2 / with hooksPath pin removed | 13/0; 13/0 with 0 objects outside; 7c FAIL (exit 1) |

Side effect: the pin-removed scratch runs executed the operator's real global pre-commit hook and SSH
signing against throwaway repos under `mktemp -d` and the session scratchpad. No repository state in
this worktree was touched.

---

_Reviewed: 2026-09-12T17:15:45Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
