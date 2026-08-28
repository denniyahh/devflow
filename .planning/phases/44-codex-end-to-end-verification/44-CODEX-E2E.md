# Phase 44 Codex End-to-End Dogfood — Outcome Record

**Written:** 2026-08-27, Task 3 of 44-04-PLAN.md, from the evidence in `44-evidence/` and the
operator-confirmed facts supplied for this task. Every claim below traces to a named evidence file
or is explicitly marked as an operator-confirmed fact this task could not re-derive from disk.

## Verdict (D-03)

**The run completed a phase** through `--agent codex` — specifically, the Codex-drivable portion of
a real phase (Code and Validate, the two stages Codex was asked to own), on a disposable throwaway
target chosen so a failure would cost nothing. It did not proceed to Ship, by deliberate operator
design (below), not because anything failed.

## The record

- **Target:** Phase 900, "Throwaway Codex Dogfood Target" — a disposable scratch phase created only
  for this run, not a real roadmap phase (operator-confirmed; matches `target-proposal.md` §4's
  recommendation to avoid spending real backlog-item capacity or risking a genuine phase).
- **Host:** a dedicated fresh worktree off `develop`
  (`/var/home/denniyahh/Github/devflow/.worktrees/phase-900`, branch `feature/phase-900`) —
  confirmed live in `44-evidence/dogfood-devflow-status.txt`.
- **Launch command:** `devflow resume --phase 900 --agent codex`, run directly by the operator (not
  by any agent) — matches `target-proposal.md` §4's exact recommended command; operator-confirmed.
- **Wall-clock span:** `dogfood-state-final.json.started_at` = `1787821637` = 2026-08-27T09:07:17Z.
  The Validate-stage verification capture (`last_verification_mtime_nanos`) lands at
  2026-08-27T10:28:07.918861Z. `dogfood-devflow-status.txt` (captured after that, itself undated
  but self-reporting "started ... 1h ago", "in stage validate: 5m ago") is consistent with a run
  of roughly 1h20m–1h30m total. No single file stamps the exact end-of-run instant, so this is
  bounded, not exact — see "what this run does not establish."

### Every captured turn, in order

1. **`dogfood-run-01-define-stdout.jsonl`** — Define, under **Claude** (`"model":"claude-sonnet-5"`
   in the stream), not Codex. Required by D-02: Codex must never run Define. Exit 0.
2. **`dogfood-run-02-plan-stdout.jsonl`** — Plan, under **Claude**, same reason. Exit 0.
3. **`dogfood-run-03-codex-preflight-stdout.jsonl`** — first **Codex** turn (Codex's own
   `thread.started`/`item.completed` JSON schema, distinct from Claude's stream format). Its one
   substantive message: *"I'm checking for the required phase plan first; if it is present, I'll
   stop without touching any artifacts."* — it finds `900-01-PLAN.md` already on disk and returns
   `DEVFLOW_RESULT: {"status": "success"}` without touching anything. This is a live positive
   control for D-02's `RequiresExistingArtifact` policy: Codex declined to regenerate an artifact
   that already existed, rather than a unit test's *claim* that it would. Exit 0.
4. **`dogfood-run-04-codex-code-validate-stdout.jsonl`** — the one real Code attempt
   (operator-confirmed: "exactly one attempt was needed at the Code stage"). Under Codex: 37
   `agent_message`, 70 `command_execution`, 6 `file_change` items. Implemented
   `buckets_needed` in `crates/devflow-core/src/dogfood_scratch.rs` (`dogfood-diff.txt`), wrote and
   ran its own unit test, produced a clean code review
   (`dogfood-run-04-codex-REVIEW.md`: `status: clean`, 0 findings), ran verification, and the turn
   ended at the pending Validate gate. Ends with `DEVFLOW_RESULT: {"status": "success"}`. Exit 0.
5. **`dogfood-run-05-codex-code-attempt2-killed-stdout.jsonl`** — a **second captured turn**, not a
   second Code attempt. The operator rejected the pending Validate gate (`devflow gate reject 900
   --stage validate`), intending to end the throwaway target, not requesting rework
   (operator-confirmed). The relaunch re-ran Validate under Codex: 6 `agent_message`, 16
   `command_execution`, 2 `file_change` items, both touching only
   `900-VALIDATION.md` — no `git commit` calls, and the one commit attempt it did make
   (`node gsd-tools.cjs query commit "docs(phase-900): ..." --files ".../900-VALIDATION.md"`)
   returned `{"committed": false, "skipped": true, "reason": "skipped_gitignored"}` (`.planning/`
   is gitignored in this repository). `dogfood-commits.txt` lists exactly 2 commits, both from
   attempt 4 — confirming zero new commits landed in this turn. Its code-review output
   (`dogfood-run-05-codex-code-attempt2-killed-REVIEW.md`) is byte-identical to run 4's (same
   `reviewed:` timestamp, same 0-finding verdict), consistent with re-checking already-clean work
   rather than redoing it. Ends with `DEVFLOW_RESULT: {"status": "success", "verdict": "pass"}`.
   Exit 0. (The filename's "killed" does not match its content — exit 0, clean success line, no
   truncated stream. Evidence content, not the label chosen when the file was saved, is what this
   record relies on, per P-01.)

**Total: 2 commits landed** (`154162c` test, `557877c` feat — `dogfood-commits.txt`), both from the
single Code attempt.

### Stages reached and where the run ended

Define → Plan (Claude) → Code → Validate (Codex, one Code attempt, one Validate pass, one Validate
re-check) → **stopped at a second pending Validate gate**. `dogfood-devflow-status.txt` shows
`stage: validate | gate: pending` at capture time. Ship was never invoked — see below.

### Why Ship was never reached (by design, not failure)

The Ship gate was never approved. This is deliberate and correct for a throwaway target: phase 900
must never be merged or pushed anywhere (operator-confirmed). After the second Validate re-check,
the operator tore down the phase's worktree, both branches (`feature/phase-900`,
`throwaway/codex-dogfood-900`), and local `devflow` state by hand (`git worktree remove`, `branch
-D`, state-file cleanup) — expected cleanup for a disposable target, not an incomplete or abandoned
run.

## What this run does not establish

One run through one throwaway phase is a single observation, not a reliability claim. Specifically,
this run does not establish:

- **Ship under Codex.** Codex never drove a Ship stage in this run — deliberately excluded, since
  approving Ship for phase 900 would push/PR a target designed to be discarded. `--agent codex`'s
  behavior at Ship (or any Codex-driven push/PR flow) is unproven by this evidence.
- **Auto/unattended mode.** The run went through `mode: supervise` with a live pending gate at
  Validate (`dogfood-devflow-status.txt`), not `mode: auto`. Codex's behavior driving an unattended
  chain across multiple stages without a human present is not exercised here.
- **A genuine Codex failure or retry.** The one real Code attempt succeeded on its first try
  (operator-confirmed). Run 5 is a second captured *turn*, not a second Code *attempt* in the P-02
  sense — no new code was produced, and the Validate re-check found nothing to redo. This run says
  nothing about what a real mid-stage Codex failure, timeout, or rate-limit event looks like in
  practice, or how cleanly a retry recovers.
- **44-02's cron-consumption logic (resume-side or ship-side).** No rate-limit event ever occurred
  for phase 900, so no `.devflow/cron-instructions-900.json` was ever created — there was nothing
  for either the resume-side or ship-side deletion path to consume. `44-02`'s behavior is unit- and
  integration-tested (see 44-02-SUMMARY.md), but this dogfood run did not independently exercise it.
- **44-03's Hermes schedule rendering.** Same reasoning — no rate-limit/cron event occurred for
  phase 900, so `cron_hint_line`'s current (D-10-fixed) output was never rendered for this run's own
  cron record. (A stale, unrelated phase-43 cron record incidentally visible in
  `dogfood-devflow-status.txt` still shows the *old* `--from-devflow` string — see the note under
  Gap Disposition below; it is not evidence about this run and is not double-counted here.)
- **Multi-target, multi-attempt, or high-volume behavior.** This is one phase, one Code attempt,
  one Validate re-check. It says nothing about Codex's behavior across many phases, concurrent
  handoffs, or repeated rate-limit cycles.
- **A precise end-of-run timestamp.** The wall-clock span above is bounded from two data points
  (`started_at`, a verification mtime) plus a self-reported status line, not a single authoritative
  start/end pair — see "The record" above.

### Was the 44-01/44-02/44-03 hardening surface exercised? (checked, not assumed)

- **44-01 (`resume --agent` handoff): YES, directly.** The entire dogfood mechanism *is*
  `devflow resume --phase 900 --agent codex`. `dogfood-state-final.json` shows `"agent": "codex"`
  and `dogfood-devflow-status.txt` shows `agent: OpenAI Codex` — the handoff took effect and held
  for the rest of the run.
- **44-02 (cron-instruction consumption): NO.** No rate-limit/cron-instructions record was ever
  created for phase 900 (checked: no `cron-instructions-900.json` reference appears anywhere in the
  evidence), so neither the resume-side nor the ship-side deletion path had anything to act on.
- **44-03 (Hermes schedule rendering): NO**, for the same reason — no cron hint was ever rendered
  for phase 900's own record.

D-01 says the run *should* exercise the hardening "where practical" — for 44-02/44-03 it was not
practical, because the dogfood run never hit a rate limit, and this is recorded plainly rather than
implied otherwise.

## Gap disposition

Every gap found during this phase, whether surfaced by the dogfood run itself or by the baseline/
regression checks this task re-ran before writing this record:

| # | Gap | Evidence | Classification | Disposition |
|---|-----|----------|-----------------|-------------|
| 1 | `phase7_cli.rs::status_prints_cron_hint_when_cron_instructions_exist` asserted the old, D-10-removed `--from-devflow` string and failed on Task 1's baseline HEAD. | `44-evidence/cargo-test-workspace-nofailfast.txt` (line 633, captured 2026-08-26/27); `44-evidence/target-proposal.md` §1 and §6, which flagged it for Task 2's disposition. | DevFlow (stale test assertion, not a Codex or environment defect) | **Closed in this phase**, commit `ce1856a` — rebuilds the expected line from `instructions`' own fields instead of the old hand-written string. Confirmed passing in this task's own re-run of `cargo test --workspace` (see Regression Check below). |
| 2 | `pre_push_signing_policy.rs::pre_push_guards_against_personal_artifacts_on_clean_branches` asserted the pre-push hook's old `git ls-tree -r --name-only` check; commit `872df37` (unrelated hook-hygiene work, landed on this branch after Task 1's baseline capture) replaced it with `git diff --name-only --diff-filter=A`, and this structural test was never updated to match. | Discovered by this task's own `cargo test --workspace` re-run, not by the dogfood run — `scripts/hooks/pre-push` line 69 vs. the test's old assertion string. | DevFlow (stale test assertion in an unrelated repo-hygiene commit; **not** part of the Codex dogfood or CODE-01 surface — noted here only because it blocked this task's own zero-FAILED regression gate) | **Closed in this task**, commit `ab655e5` — updates the assertion to the hook's current command string. Same stale-test class as gap #1. |
| — | A stale, pre-existing `.devflow/cron-instructions-43.json` record (unrelated to phase 900) still renders the pre-D-10 `hermes cron create --from-devflow ...` hint text in `dogfood-devflow-status.txt`. | `44-evidence/dogfood-devflow-status.txt`, line "Cron instruction pending (phase 43): ...". | Not a gap of this run — `cron_hint_line` (commands.rs) reconstructs its output from the *stored* `hermes_cron.command` field of whatever JSON was written at record-creation time; this record predates the D-10 fix and 44-02's consumption logic was never triggered to clean it up (no ship/resume-consumption event has fired for phase 43 since). | **Not dispositioned as a CODE-01 gap** — it was not surfaced by the phase-900 dogfood run, it is incidental output from an unrelated active phase captured in the same status check, and phase 43 is already shipped and outside this phase's declared scope. Recorded here for transparency per this task's own "check, don't assume" standard, not swept under the rug. |
| — | `build_provenance.rs::build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` FAILED on Task 1's baseline, per the project's own documented worktree-isolation issue (missing `.planning/UPSTREAM-GSD-ISSUES.md` symlink target). | `44-evidence/cargo-test-workspace.txt`. | Environmental (documented in `CLAUDE.md` § "Where the upstream GSD issue ledger lives"), explicitly **not a Codex-driver regression** — Task 1 already classified it this way. | Not a CODE-01 gap requiring disposition (Task 1 correctly scoped it out). Independently resolved by two later, unrelated commits on this branch (`71a795e`, `67b584b`, which drop the symlink pattern entirely) — confirmed passing in this task's own re-run. Noted for completeness only. |

No gap surfaced by the phase-900 dogfood run itself required filing as a GitHub issue — the one
real gap the run's baseline captured (#1) was closed in this phase with a cited commit, and the one
gap this task's own re-verification surfaced (#2) was likewise closed with a cited commit. Nothing
was reclassified as out-of-scope to make the phase read as complete (P-03): the two items marked
"not dispositioned as a CODE-01 gap" above are recorded with their reasoning, not silently dropped.

## A-EDGE-01 resolution

The edge probe found that CODE-01's requirement text does not define what counts as a "real phase,"
whether a partial-stage run counts as end-to-end, or whether a re-filed gap satisfies the
requirement. This task applies the following reading, stated explicitly per A-EDGE-01's own
instruction:

**CODE-01 is satisfied by completion, not by evidenced re-filing.** The run drove the entirety of
the portion of the pipeline Codex was asked to own — Code and Validate, the two stages a Codex
handoff is meaningful for under D-02's constraints — to a clean, successful conclusion, on a real
(if disposable) phase, with real commits and a real passing verification. It surfaced exactly one
concrete gap (the phase7_cli.rs stale assertion), and that gap was closed in this same phase with a
cited commit, not re-filed as an open issue. No unresolved gap remains that would force a
"re-filing" verdict instead.

"Real phase" is read as: a phase driven through DevFlow's real pipeline machinery (`devflow
resume`, real state files, real git commits, real gates) against a real repository, as opposed to a
mock or simulated harness — not as "a phase whose backlog value matters," which is a different and
stricter reading CONTEXT.md's D-01/D-02 discussion does not require. The throwaway target was
chosen specifically so a bad outcome would cost nothing (`target-proposal.md` §3, §5), which is
orthogonal to whether the machinery driving it was real.

"End-to-end" is read as covering Code→Validate under Codex specifically, not Code→Ship — because
D-02 itself scopes Codex's headless contract to stages where `RequiresExistingArtifact` either
doesn't apply or is already satisfied, and Ship was never something this dogfood needed to prove:
CODE-01's own text names "a real phase" and "surfaced gaps," not a full Define-to-Ship traversal.

## Regression check (evidence for ROADMAP success criterion 3 only — NOT end-to-end Codex evidence)

This section is local test results. It has no bearing on the D-03 verdict above and must not be
read as end-to-end proof of anything Codex-related; it only answers "did 44-00/44-01/44-02/44-03
regress the existing Codex driver or the wider workspace."

- **Codex driver conformance suite:** `cargo test -p devflow-core --lib agents::tests::codex` →
  **5 passed, 0 failed** (`44-evidence/cargo-test-codex-driver.txt`, and re-confirmed live by this
  task: `test result: ok. 5 passed; 0 failed; ... 730 filtered out`).
- **Codex launch-contract parity (D-04):** `git diff <merge-base-with-origin/develop> --
  crates/devflow-core/src/agents/codex.rs` → **zero-line diff**, both at Task 1's baseline capture
  (`44-evidence/codex-diff-vs-merge-base.txt`) and re-confirmed live by this task against the
  current worktree HEAD. `codex.rs` was never touched by any 44-00/44-01/44-02/44-03 work; D-04
  holds trivially.
- **Full workspace:** `cargo test --workspace` (default fail-fast) → **zero `test result: FAILED`
  lines**, re-run live by this task after closing gap #2 above. (The plan's own literal
  `<verify>` pipeline for this check — `... | rg -c '^test result: FAILED' | rg '^0$'` — exits
  non-zero even when the count is genuinely zero, because `rg -c` prints nothing rather than the
  literal string `0` when a pattern has no matches; `rg '^0$'` then has no input line to match.
  Confirmed directly: `rg -c '^test result: FAILED' <captured output>` produces no output and exits
  1, and manual inspection of the captured output shows no `FAILED` result line anywhere. This is a
  shell-pipeline quirk in the plan's verify script, not a real failure — the underlying substance,
  zero failing test binaries, is independently confirmed by inspection.)
- **devflow-core lib, full run (`--no-fail-fast`):** **735 passed, 0 failed**
  (`44-evidence/cargo-test-workspace-nofailfast.txt` line 1484), including all seven `resume
  --agent` tests from 44-01 (`resume_with_agent_hands_off_and_relaunches_under_the_new_driver`,
  `resume_with_agent_refuses_before_touching_state_when_target_cannot_run_the_stage`, etc. — all
  green).
- **devflow-cli, full run:** **347 passed, 0 failed** (same file, line 490), including
  `phase7_cli.rs`'s 28/28 (post-fix) and `pre_push_signing_policy.rs`'s 6/6 (post-fix, this task).

This is a regression check, nothing more. It says nothing about whether Codex can drive a real
phase unattended, over a long span, or under a genuine failure — see "what this run does not
establish" above for that.
