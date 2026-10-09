---
phase: 49-live-unattended-run-the-milestone-s-instrument
verified: 2026-10-09T01:30:00Z
status: human_needed
score: 5/5 must-haves verified
covered_files:
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-01-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-01-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-02-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-02-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-03-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-03-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-04-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-04-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-05-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-05-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-07-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-07-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-09-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-09-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-11-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-11-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-SUMMARY.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-LIVE-RUN.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-REVIEW-DISPOSITION.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-REVIEW.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-RUNBOOK.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/attempt.json"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/collected/devflow/events.jsonl"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/collected/git.txt"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/collected/pr.json"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/gates/001-define/devflow/state-01.json"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/gates/003-ship/git.txt"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/identity.txt"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/ledger.jsonl"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/negctl/bin_sha256"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/negctl/events.jsonl"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/negctl/start.stdout"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/prelaunch/fork-base.txt"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/start.stdout"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/surv/analysis.txt"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-3/verdict.json"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/EVIDENCE-LAYOUT.md"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/check-evidence.sh"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/collect-attempt.sh"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/launch-attempt.sh"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/negctl.sh"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/sanitize-evidence.py"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/snapshot-hook.sh"
  - ".planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/surv-watch.py"
  - "crates/devflow-cli/src/pipeline_gate.rs"
  - "crates/devflow-cli/src/staleness.rs"
  - "crates/devflow-core/src/agent_result.rs"
  - "crates/devflow-core/src/gates.rs"
  - "crates/devflow-core/src/monitor.rs"
  - "crates/devflow-core/src/prompt.rs"
  - "crates/devflow-core/src/state.rs"
  - "scripts/scratch-dogfood-repo.sh"
covered_digest: "v3:sha256:68ac3ae1080cc1746a43bed4e7d52307fb621e0f098f808134366deaaebd22fe"
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Decide WR-03 before the post-completion merge-back: the Ship prompt (prompt.rs ship_stage_prompt and workflow_ship_prompt, from 084625c, now on feature/phase-49 via bbfe6df) tells EVERY project's agent to target PR base `main` 'while DevFlow merges into integration locally'. `integration` is this phase's scratch-repo branch name."
    expected: "Operator chooses: fix (interpolate config::MAIN and the resolved base) before merge-back to workspace/develop, or explicitly defer with a backlog entry. 49-REVIEW-DISPOSITION.md currently records WR-03 as `open`, which is neither."
    why_human: "Scope/release decision. It also limits criterion 2: attempt 3's PR-to-main observation measures the agent obeying a hard-coded literal, not DevFlow resolving a PR base."
  - test: "Correct or annotate 49-LIVE-RUN.md lines 15-16, 240 and 249-251, which state the fix commits e3f9ddd/084625c are NOT on feature/phase-49."
    expected: "Since bbfe6df (2026-10-09) both are ancestors of HEAD (git merge-base --is-ancestor exit 0 for each); the only code delta from 084625c to HEAD is the rustfmt commit 5574841 (gates.rs, state.rs line splits). The record should say so before the post-completion gate byte-compares the 'final' 49-LIVE-RUN.md blob."
    why_human: "Editing the phase record is outside the verifier's remit. 49-LIVE-RUN.md is in covered_files, so editing it marks this verification stale and needs a re-verify."
  - test: "Acknowledge the judgment-tier prohibitions (49-03, 49-04/49-12, 49-09, 49-10, 49-11). The LLM-judge verdict for each is in the report's 'Prohibitions' table and found no violation."
    expected: "Operator accepts or rejects each verdict. One wording conflict needs a ruling: 49-09 says 'No Phase 49 tool answers a DevFlow gate', but negctl.sh:249 runs `gate reject` on the negative arm's own Define gate, as 49-07's must-have requires."
    why_human: "Judgment-tier prohibitions are non-authoritative without a human ruling (ADR-550 D4)."
  - test: "Carried from 49-LIVE-RUN.md, still undecided: does attempt 2's quota pause plus operator-scheduled resume count as 'unattended', and does attempt 2 count toward the D-11 cap?"
    expected: "Operator rules, or explicitly drops it. It does NOT gate VERIFY-01: the verdict rests on attempt 3, whose events contain 0 rate-limit/cron-resume/advance_failed/workflow_aborted events. The same search finds 3 in attempt 2, so the search can find them."
    why_human: "Policy ruling that no file records."
---

# Phase 49: Live Unattended Run — Verification Report

**Phase Goal:** Record a real `devflow start --mode auto` run against a disposable GSD project end to end, against a
configured base: fork point, preflight, unattended progression and Ship merge target. The milestone's earlier fixes are
then measured on a live chain, not asserted from unit evidence.
**Verified:** 2026-10-09
**Status:** human_needed
**Re-verification:** No. This is the initial verification; no earlier VERIFICATION.md existed.

## Goal Achievement

Bottom line: the live record exists and holds up under independent reads of the committed evidence, the raw evidence
and the live GitHub state. All five roadmap success criteria are VERIFIED for **one run (attempt 3) of one binary**
(`e8cf0077946f…`, built from `084625c`). Status is `human_needed`, not `passed`, for two reasons. First, the phase merged
a production prompt change tuned to its own fixture (WR-03). Second, the phase record now states something false about
where that code lives. Both need an operator decision.

### Observable Truths (ROADMAP success criteria, the contract)

| # | Truth | Status | Evidence (checked this session) | What it does NOT establish |
|---|-------|--------|---------------------------------|----------------------------|
| 1 | Disposable project commits `devflow.toml` with a non-default `base_branch`; persisted `State::base_branch` reads it | ✓ VERIFIED | `state-01.json` in all three attempt-3 gate snapshots (define, code, ship): `base_branch=integration`. `start.stdout`: `note: base branch is \`integration\` (from devflow.toml (base_branch))`. The negative arm's state reads `develop`, so the field discriminates. | Covers one value (`integration`) only. Does not show the default is used when `devflow.toml` exists without the key. |
| 2 | Recorded fork point through Ship merge target: one fork point, `unattended_config_condition = Holds`, unattended progression apart from the Define and Code gates, Ship gate answered by `--yes-ship`, Ship merge target observed | ✓ VERIFIED | **Holds:** `[holds] GSD config can hold the chain flag` in `start.stdout`. That line is printed by `unattended_config_condition()` (preflight.rs:990, :1167). The separate `[DOES NOT HOLD] no plan declares a human-only checkpoint` line is the D-22 seeded checkpoint, by design. **Fork point:** `fork-base.txt` base_sha = Define snapshot worktree_head = Ship snapshot merge_base = `0f16ce6a…`. **Gates:** events give define gate → operator approve (`responded_by: denniyahh`, `via: cli`), code gate → operator approve, ship gate → `responded_by: "--yes-ship"` resolved in the same second (1791412415). No other `gate_fired`. Attempt 3 has 0 rate-limit, relaunch, `advance_failed` or `workflow_aborted` events and no `resume.stdout`. **Merge target:** `collected/git.txt` has one first-parent merge `311271e` into `integration` with parents (fork point, feature_head `f142d99`); `main` is unmoved (`2451b5e` before and after). **PR:** live `gh pr view 1 --repo denniyahh/p49-live-attempt-3` returns base `main`, head `feature/phase-01`, OPEN. On the live PR branch, `probe/budget.py` reads `DEFAULT_RETRY_BUDGET_SECS: int = 120`; the file is absent on `main`. | **The PR-to-`main` result is produced by a literal in the prompt (WR-03).** 084625c added "target base branch `main` … while DevFlow merges into integration locally" to every project's Ship prompt. Attempt 2, which ran before that commit, opened its PR against `integration` (confirmed live). The PR base therefore shows the agent obeying the literal; it does not show DevFlow working out the base. The DevFlow-owned target, the local merge into `integration`, is a genuine observation. `responded_by: denniyahh` is the OS user, and negctl's scripted reject records the same value, so it does not by itself prove a human typed the approvals. Unattended progression is shown for one run, with Define and Plan as short pre-baked stages (8 s and 6 s, real claude launches that returned `success`). |
| 3 | Negative arm re-run with this phase's binary: unconfigured repo forks from `develop` and refuses on missing `.planning/config.json`; both arms from one binary | ✓ VERIFIED | `negctl/bin_sha256` equals `attempt.json` `bin_sha256` (`e8cf0077…`). Both `workflow_started` events record commit `084625c`. `negctl/start.stdout` has the verbatim `[DOES NOT HOLD] GSD config can hold the chain flag — no \`.planning/config.json\` …` line and no base-branch note. The snapshot's `base_branch=develop`. `develop.sha` = `worktree-head.sha` = `96e8ec1…`. Events: gate_fired define → reject → `workflow_aborted`. | "Refuses" here means the auto-mode preflight parks at a gate that the control then aborts. It is not a hard exit. Shows one unconfigured repo shape only. |
| 4 | DECN-03 answered against this run, or the run records that the path was not exercised | ✓ VERIFIED | 49-LIVE-RUN.md records exactly one outcome, **Followed**. Attempt 3 has exactly 1 `checkpoint_auto_decided` event (counted). The turn-1 capture and the resumed capture both carry `session_id uuid-2`. Both verbatim quotes appear exactly once in the resumed capture `1791412108556407377-0-stdout` (`rg -c -F` = 1 each). A `transition code→validate` follows with no intervening gate. budget-120 confirmed on the live PR branch. The turn-1 capture ends in a real report of the seeded `checkpoint:decision gate=blocking-human`, with "no code was written". | One model (claude 2.1.292) and one sample. The record itself says the run cannot separate recency from compliance. "Which turn carried which rule" rests on the **uncommitted** transcript (999.150). The comparison was already written in turn 1. |
| 5 | SURV-01 field question recorded without overclaiming | ✓ VERIFIED | `surv/analysis.txt`: writers=7, temp_writes=34, interleaved_windows=0, overflow_windows=0, stop_marker=present, plus the verbatim "establishes nothing" sentence. 49-LIVE-RUN.md copies these values, lists blind spots (watch gap before `watch_added`) and states that this neither confirms nor weakens Phase 48. | One watcher run. The watch gap means early writes before `watch_added` are unobserved. WR-06 (analyze can flag dead writers' orphaned temp files) is a false-positive risk, not a false-negative one, so it does not undercut `0` here. |

**Score:** 5/5 truths verified (0 present, behavior-unverified). These are behavioral observations of a live run, not
symbol presence: the run is the behavior.

**Not evaluated here: the post-completion integration gate.** The ROADMAP schedules it *after* goal verification and
`phase.complete` (merge-back into `workspace/denniyahh`, sync, divergence check, push, then the 49-LIVE-RUN.md blob
compare). It cannot be verified now and is not counted in the score. It stays an orchestrator obligation.

### Plan must-haves (spot-checked, additive to the contract)

| Plan | Check | Result |
|------|-------|--------|
| 49-01 | VERIFY-01 closing sentence (`criterion 1, amended in`); criterion 2 names Define and Code; no tracked file under `49.1-configurable-test-wait-budget/`; `roadmap.analyze` exit 0 | all hold (0 tracked files; analyze rc=0, 7 phases) |
| 49-02/49-06 | `scripts/scratch-dogfood-repo.sh --selftest` | rc=0, `SELFTEST passed=26 failed=0` |
| 49-03 | snapshot-hook / collect-attempt selftests | 6/0, 11/0 |
| 49-04 | surv-watch selftest | 7/0 |
| 49-05 | sanitize-evidence selftest; `scan` over committed `49-evidence/` and 49-LIVE-RUN.md | 12/0; scan rc=0 on both; **negative control:** planted `ghp_` token → rc=1 |
| 49-07/49-08 | check-evidence selftest; arms over committed copies | 34/0. Attempt 3 positive 11/0, negative 5/0, pair 3/0. **Negative controls:** attempt 1 positive rc=1 (4/7), attempt 2 positive rc=1 (8/3), so the checker can fail |
| 49-08/49-11 | `--attempt-gate ~/.cache/devflow-phase49` (read-only; source read first) | rc=0, `ATTEMPT_GATE verdict=shipped attempt=3`, including `sha256sum -c` of the raw collected manifest |
| 49-09 | launch-attempt selftest; `prelaunch/repo.json` | 17/0; `denniyahh/p49-live-attempt-3` `PRIVATE` |
| 49-10 | runbook contains `--attempt-gate`; DEV-SETUP-CHECKLIST contains `delete_repo`; no `--no-verify`/hooksPath bypass in runbook | hold |
| 49-12 | backlog 999.147–150 exist in ROADMAP; cleanup listed as pending | hold |
| fix commits | 21 named tests from e3f9ddd/084625c (`blocking_human_checkpoint_reported_*`, `ship_prompt_instructs_pr_targeting_main`, `notify_hook_fallback_runs_when_env_unset`) | `21 passed; 0 failed` |

Not established by these: the selftests prove the cases they plant. The sanitizer scan passing does **not** cover the
AWS, Slack, PEM or JWT shapes WR-04 says it misses. A separate `rg` for those shapes over the committed evidence found 0
hits, with a planted AKIA control found.

### Prohibitions (judgment tier, non-authoritative LLM-judge verdicts; see human_verification item 3)

| Source | Statement (short) | Verdict | Basis |
|--------|-------------------|---------|-------|
| 49-03 | No capture tool creates, deletes or pushes to a GitHub repo; the collector only runs `gh pr view` | no violation found | tools call only `gh repo view`, `gh auth status`, `gh pr view`; no `git push` in tools |
| 49-04/49-12 | No SURV-01 confirmation wording; no bare auto-decide event as a DECN-03 result | no violation found | SURV wording negated; Followed rests on session-matched quotes and progression |
| 49-09/49-11 | No tool touches a GitHub repo; run never targets this repo | no violation found | repo.json = throwaway PRIVATE repo |
| 49-09/49-11 | No tool answers or pre-writes a gate response; Define and Code answered by operator, Ship only by `--yes-ship` | no violation in the live run; **wording conflict** | launch-attempt.sh has no gate verbs. negctl.sh:249 `gate reject`s its own control gate, which 49-07 requires |
| 49-10 | Runbook never instructs a hook bypass or pre-written responses | no violation found | grep found none |

### Requirements Coverage

| Requirement | Source Plans | Status | Evidence |
|-------------|--------------|--------|----------|
| VERIFY-01 | all 12 plans declare it | ✓ SATISFIED (for attempt 3's binary) | Criteria 1-3 above; negative arm re-run with the same binary. REQUIREMENTS.md still shows Pending, which is correct: `phase.complete` owns that flip. |

No orphaned requirement IDs: REQUIREMENTS.md maps only VERIFY-01 to Phase 49.

### CR-01 assessment (deferred to Phase 49.1)

- **Attempt 3 depended on the CR-01 path.** The turn-1 `DEVFLOW_RESULT` reason contains `checkpoint:decision`,
  `gate=blocking-human` and `checkpoint`. It was recognised by the prose heuristic e3f9ddd introduced, and attempt 1
  stopped precisely because recognition was missing then.
- **The detection was a true positive.** It matched the payload's seeded checkpoint (Plan 01-01 Task 1), the agent wrote
  no code first, there is exactly 1 auto-decide event, and Validate succeeded afterwards with no spurious second resume.
  CR-01's false-positive class did not fire in this run, so it does not invalidate criteria 2 or 4.
- **What this does not establish:** that the shipped detector is sound. CR-01's probe shows it auto-resumes ordinary
  failures. Phase 49.1 will replace this exact detector, so after 49.1 the live chain's auto-decide path is unmeasured
  unless 49.1 re-runs live.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| crates/devflow-core/src/prompt.rs | ship prompts (from 084625c) | Fixture branch name `integration` and literal `main` hard-coded into every project's Ship prompt (WR-03, disposition `open`) | ⚠️ Warning | Wrong instruction for default and other-base projects once merged downstream; limits criterion 2's PR-base observation |
| 49-LIVE-RUN.md | 15-16, 240, 249-251 | States fix commits are not on feature/phase-49; false since bbfe6df | ⚠️ Warning | Record inaccuracy ahead of the blob-compare in the post-completion gate |
| crates/devflow-core/src/agent_result.rs | 762-782 | CR-01 prose heuristic | ⚠️ Warning (deferred to 49.1 by operator) | See CR-01 assessment |
| 49-evidence/attempt-3 | two `phase-01-stdout` files | Invalid JSON from sanitizer redaction (999.147) | ℹ️ Info | Not cited by any criterion; filed |

No TBD/FIXME/XXX debt markers in modified files. The only match is the `mktemp` template `XXXXXX` in launch-attempt.sh:265.

### Gaps Summary

No must-have truth FAILED. The phase delivered what its goal states: a live, end-to-end, base-configured run recorded
from fork point to Ship merge target, a same-binary negative arm, and honest DECN-03 and SURV-01 records. The committed
copy carries its own proof.

Strength of the result: one shipped run, the third of three, on claude 2.1.292, one payload, one base topology. It bounds
nothing about reliability.

Weakest link: the Ship PR-base half of criterion 2. It became observable only after a production-prompt literal tuned to
this fixture (WR-03) was added mid-checkpoint and then merged onto this branch.

---

_Verified: 2026-10-09_
_Verifier: Claude (gsd-verifier)_
