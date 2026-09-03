# Phase 45: Adversarial Review — Round 2 (PLAN set)

**Date:** 2026-09-02
**Subject:** `45-01-PLAN.md`, `45-02-PLAN.md`, `45-03-PLAN.md`
**Status:** REJECT — both lanes independently returned REJECT

## Lane roster (typed outcomes — a dropped lane is never a pass)

| Lane | Model | Outcome | Findings | Citations |
|---|---|---|---|---|
| codex | `codex exec`, reasoning=high | completed | 9 CONFIRMED, 2 SUSPECTED | 32 |
| antigravity (`agy`) | gemini-3.1-pro-high | completed on 3rd attempt | 2 CONFIRMED | 2 |

**agy operational note.** Attempts 1 and 2 failed with `Error: There was a network issue
connecting to the server` after ~18 min each. Root cause was NOT network: a controlled probe
with identical flags and a tiny prompt succeeded instantly (`PROBE_OK`, exit 0, no TTY). The
cause is the **114 KB prompt passed through argv**. Passing the prompt file *path* and letting
agy read it succeeded in 460s. Record this for future review runs.

**agy has a false negative on executability.** Its Section A explicitly cleared the
`<automated>` commands as "viable ... without being constant-pass/fail". That is wrong and was
disproven empirically (see F1). Per review protocol this clean bill is NOT folded into
consensus — a low-citation pass is a claim about the reviewer, not the artifact.

## Verified findings (checked against source by the orchestrator)

### F1 — Clippy check is a true constant-pass  [codex; VERIFIED, worse than reported]
`45-01:518-519`, `45-02:345-346`, `45-03:357-358`
`cargo clippy ... | rg 'Finished|error'` exits 0 in BOTH directions — measured:
`clippy_ok -> exit 0`, `clippy_fail -> exit 0` (matches the literal `error`).
A failing clippy satisfies the gate. **Fix:** capture clippy's exit status separately; assert
status 0 AND absence of `^error`.

### F2 — Full-suite check has inverted exit status  [codex; VERIFIED with nuance]
`45-01:515-516`, `45-02:342-343`, `45-03:354-355`
Measured: green suite `grep -c` prints `0` exit **1**; red suite prints `1` exit **0**.
Nuance codex missed: the `<fails_when>` asserts on PRINTED OUTPUT, not exit status, so read
literally the check is sound. Risk is that harnesses abort on non-zero exit before parsing
stdout. **Fix:** append `; true` (or `|| true`) after the grep.
Note: the plans warn against the Phase-44 `rg -c` form and then reintroduce the same class
with `grep -c`.

### F3 — `--no-worktree` still forks from `develop`  [codex; VERIFIED]
`crates/devflow-cli/src/commands.rs:343`
The else-branch calls `GitFlow::new(project_root)` then `feature_start`. `GitFlow::new` is the
sole constructor and hardcodes `GitFlowConfig::default()` (`git.rs:119-123`).
**Impact:** AUTO-01's core promise fails on the `--no-worktree` path.

### F4 — `phase_artifact_on_develop` hardcodes `"develop"`  [codex; VERIFIED]
`crates/devflow-cli/src/commands.rs:90-100`, callers at `:293`, `:303`, `preflight.rs:618`
Passes a literal `"develop"` to `git ls-tree`. Negative control: the string
`phase_artifact_on_develop` appears **0 times** in `45-01-PLAN.md`.
**Impact:** a configured planning branch holding CONTEXT.md/PLAN.md is invisible to preflight.

### F5 — `commands::reference` ignores configured base  [agy ONLY; VERIFIED]
`crates/devflow-cli/src/commands.rs:552`
`let branch = branch.unwrap_or_else(|| DEVELOP.to_string());`
Negative control: `reference` appears **0 times** in `45-01-PLAN.md`.
**Codex did not find this.** This is the two-lane payoff.

### F6 — Plan directs an edit to a nonexistent call site  [agy ONLY; VERIFIED]
`45-01` Task 3 says to edit ":1056's sibling use in `evaluate_agent_result`" in
`pipeline_outcomes.rs`. Verified: `evaluate_agent_result` occurs only at lines 3277+, and
`#[cfg(test)]` begins at **1097** — every use is test-only. No production call site exists.
**Impact:** executor burns cycles hunting a phantom, or fails the step.

### F7 — Fail-open WARNING TEXT is not pinned  [codex vs agy CONTRADICTION; codex correct]
`45-01:34`, `45-01:340-355`, `preflight.rs:540-547`
agy said the contract is pinned; a test with a negative control does exist (Task 2 deletes the
`Undeterminable => Ok(())` arm and requires failure). But `45-01:34` requires the warning to
CONTAIN the words `fail-open`, and the stated control exercises the RETURN VALUE, not the
message. Deleting only the warning print leaves the test green. codex's narrower claim stands.

## Reported but NOT independently verified

Carried forward for the replan; treat as unconfirmed until checked.

- codex: scoped staleness rule contradicts its own `..` negative control (45-02)
- codex: required RED test cannot be produced by deleting the match arm
- codex: base validation permits commit-ish aliases and production refs
- codex: operator output still says branches are behind `develop`
- codex [SUSPECTED]: monitor commit enumeration may resolve config from the wrong root
- codex [SUSPECTED]: required `main` refusal bypassed by resolver fail-soft fallback

## Agreed as sound by both lanes

- 45-03 is honest about the upstream competing instruction (T-45-17) and routes the policy
  through BOTH `code_stage_prompt` and `workflow_code_prompt`.
- `files_modified` across the three plans is mutually exclusive — parallel wave 1 is safe.
- 45-01 carries the same-commit docs edit required by `doc_check`.

## Raw artifacts
- `review2-codex.md` (stdout) / `codex.err` (606 KB trace)
- `review2-agy.md`
- `review2-prompt.md` (the 114 KB assignment both lanes received)
