# Adversarial review: Phase 49 plan set (2026-09-29)

**Target:** `49-01`…`49-12-PLAN.md` at `9733dcf`. **Lens:** adversarial. **Lanes:** agy
(gemini-3.8-flash-high), codex (gpt-5.6-terra) and pi (deepseek-flash). All three lanes completed and none
dropped. The review ran against a `git archive` snapshot and wrote nothing to the repository.

The orchestrator re-checked the findings marked **V** (verified) in source or plan text. **U** marks findings
that were not re-checked. **R** marks findings that were refuted or downgraded.

## Change the outcome of an attempt or a verdict

| # | Finding | Lanes | Status |
|---|---------|-------|--------|
| 1 | The DECN-03 gate in 49-12 T2 checks only that a label is present. With one `checkpoint_auto_decided` event, `Followed` passes without the resumed-turn capture, which is exactly the Void case in `47-PHASE49-OBSERVATION.md:16-18`. The same bare-word checks hold for SURV-01. | codex 5, pi 3, agy 4 | V (gate text `49-12:173`) |
| 2 | The SURV-01 banned-word check fails the honest sentence that `REQUIREMENTS.md` asks for ("no interleaving has been observed…") and passes an overstatement ("guarantee upheld"). | pi 1 | U (the lane ran the expression) |
| 3 | 49-12 cannot be retried: the sanitizer refuses an existing destination (`49-05:22`), and 49-12 T1 commits `$D/attempt-<N>` before its verify step. A failed verify therefore wedges the plan. | codex 6 | V |
| 4 | The negative arm commits in the default-mode scaffold, and that scaffold leaves signing on. The host has `commit.gpgsign=true`, so the commit can hang headless or be signed with the operator's key. | agy 3 | V (`git config --global commit.gpgsign` = true; `49-02:160` disables signing only in `--live-payload`; `49-07:105` commits) |
| 5 | 49-12 T1's action says to run `--arm positive` on a `cap_reached` closing attempt, which always fails; its gate skips that step. | agy 5 | V (`49-12:118-120` vs `:132`) |
| 6 | 49-10 content item 2 puts two tools on one runbook line, and the same task's gate fails any runbook line naming two tools. Following the plan verbatim fails its own gate. | pi 8 | V (`49-10:117`) |
| 7 | Where the negative arm's snapshots resolve is ambiguous. 49-08:97 resolves `<root>/gates/<basename>`, but the negative arm's root is `$EV/negctl` (`49-03:94`, `49-07:116`), and 49-10 and 49-12 pass `$EV`. | agy 2 | V (the ambiguity is real; whether it breaks depends on the executor's reading) |
| 8 | The DECN-03 count discards jq's exit status. With jq missing, the count is 0, so the gate *requires* "Path not exercised". | pi 2 | U |
| 9 | `cap_reached` counts `verdict.json` files outside the manifest, with no binding to the directory index. The attempt gate proves internal consistency, not where the evidence came from. | pi 5, codex 4 | U (threat: planted evidence by the operator or an agent) |

## Autonomous plans do real external work (operator's call)

| # | Finding | Lanes | Status |
|---|---------|-------|--------|
| 10 | 49-09 T1 (autonomous) makes a real `claude -p` call. If it fails, GSD's node-repair can retry it autonomously instead of stopping for the operator. | codex 2, agy 1 | V (autonomous: true; the call is in the gate) |
| 11 | 49-07 T3 (autonomous) runs a real `devflow start` with the real HOME. It writes the operator's `~/.cache/devflow` registry, and on a failure path it leaves an entry behind by design. Its `cargo build` is not `--locked`. | codex 3, pi 7 | V (the plan says so, `49-07:187-194`) |

## Hardening, lower impact

- 49-11 uses `gate="blocking"`, and `workflow.auto_mode` is `true`. **R, downgraded:** `checkpoints.md:11` rule 5
  says human-action still stops in auto mode. However, `execute-plan.md:144` calls `blocking` auto-approvable,
  so the two docs contradict each other. `gate="blocking-human"` removes the doubt at no cost (pi 4).
- Stopped attempts: KillMode=process leaves the monitor alive, and cleanup deletes the project directory
  without reaping it (codex 1). 49-10:193 checks that the monitor is dead before collecting, so this is
  partly covered.
- The 49-09 auth-probe EXIT trap does not `reset-failed` the unit (agy 7).
- `caller_claude_names` is printed but not asserted (codex 7). The planner chose this so the gate also passes
  from a plain terminal.
- `--home` has a default only in prose (pi 6). The runbook sets `P49` explicitly (`49-10:73`).
- The fix-vehicle build needs a clean, committed `Cargo.lock` on the fix branch (agy 6).
- The gsd-tools path is hard-coded (agy 8). The 49-01 `git cat-file aaad4b9:` object would be missing in a
  shallow clone (codex S1). Both are low impact.

## Checked and fine (lanes agree)
The `cargo test --exact` trap is defended in 49-06. The merge-commit lookup matches the terminal hook order.
All 29 gates pass `bash -n`. The scaffold tree hash reproduces. `rg -c` zero-match handling is correct.
49-11 is one task with no commits before `done`.
