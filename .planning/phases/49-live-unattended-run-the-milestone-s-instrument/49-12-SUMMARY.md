---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 12
subsystem: testing
tags: [live-run, evidence, sanitize, verdict, decn-03, surv-01, backlog]
requires:
  - phase: 49-11
    provides: "Attempts 1-3 on disk; attempt gate verdict=shipped attempt=3"
  - phase: 49-05
    provides: sanitize-evidence.py (the only publish path into the repository, D-26)
  - phase: 49-04
    provides: surv-watch.py analyze and its closing sentence
  - phase: 49-01
    provides: check-evidence.sh arms and the attempt gate
provides:
  - "49-LIVE-RUN.md: VERIFY-01 verdict (met, on attempt 3), the record, DECN-03 outcome (Followed), SURV-01 observation, limits, gap disposition, pending operator cleanup"
  - "49-evidence/attempt-{1,2,3}/: sanitized curated evidence; attempt 3's copy passes positive 11/0, negative 5/0, pair 3/0 in place"
  - "Backlog entries 999.147-999.150 for the non-blocking findings"
affects: [phase-49-verification, phase-49-close-out]

actuals:
  tokens: 1154357   # chars/4 over the whole realized diff, 4.6 MB of it committed evidence; the authored text (49-LIVE-RUN.md + ROADMAP.md) is 7368
  tasks: 3
  commits: 3        # measured: git rev-list --count 0d1da50..HEAD before the SUMMARY commit
plan_head_before: 0d1da5034364e4e85ec738d087750254970264a1
plan_head_after: fbf9a605792890bc6fa382895644fdceec523861

tech-stack:
  added: []
  patterns:
    - "Sanitize into a fresh staging directory, then publish; nested git control files never enter evidence"

key-files:
  created:
    - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-LIVE-RUN.md
    - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/ (249 files, 4,583,094 bytes)
    - .planning/phases/999.147-sanitize-evidence-py-s-email-redaction-swallows-a-json-esca/.gitkeep
    - .planning/phases/999.148-collected-evidence-carries-devflow-s-gitignore-so-a-plain-gi/.gitkeep
    - .planning/phases/999.149-check-evidence-sh-labels-a-repeated-stage-gate-unexpected-f/.gitkeep
    - .planning/phases/999.150-archived-stage-captures-do-not-keep-the-prompt-each-turn-rec/.gitkeep
  modified:
    - .planning/ROADMAP.md

key-decisions:
  - "VERIFY-01 written as met, from the attempt gate's verdict=shipped attempt=3 and the committed copy passing all three checker modes; the shipped binary is from unmerged fix-branch commit 084625c, which the verdict states"
  - "DECN-03 outcome Followed, equal to the class Task 2's gate derives from attempt 3's committed events and captures; claude only (D-15)"
  - "Attempt 2's two sanitizer-refused captures omitted whole, by operator decision 2026-10-08 (option A); the operator's 'fixtures' ruling is recorded as a ruling, not a verified fact"
  - "collected/devflow/.gitignore (a single '*') excluded from every attempt's evidence so the commit cannot silently drop the event log and captures"
  - "VERIFY-01 left unmarked in REQUIREMENTS.md; verification decides it"

patterns-established: []

requirements-completed: [VERIFY-01]

coverage:
  - id: D1
    description: "Sanitized evidence committed for attempts 1-3, with the closing attempt still proving criteria 1-3 in place"
    requirement: VERIFY-01
    verification:
      - kind: e2e
        ref: "49-12 Task 1 <verify> (trap dropped, L=exec-49-12/t1): committed_copy positive 11/0, negative 5/0, pair 3/0; sanitizer_scan_exit=0; gate_rc=0"
        status: pass
    human_judgment: true
    rationale: "The gate proves the committed copy is consistent and that the excluded paths are absent. Whether one shipped sample, from a binary built on an unmerged fix branch, satisfies VERIFY-01 is for phase verification to decide."
  - id: D2
    description: "DECN-03 outcome and SURV-01 observation recorded, bound to the committed evidence"
    requirement: VERIFY-01
    verification:
      - kind: other
        ref: "49-12 Task 2 <verify> (trap dropped, L=exec-49-12/t2): supported_outcome=Followed written_outcome=Followed; surv fields equal; surv_unnegated_claims=0; gate_rc=0"
        status: pass
    human_judgment: true
    rationale: "The quoted reasoning and the turn-placement note (which rests on the uncommitted transcript) need a human read; the gate matches words, not meaning."
  - id: D3
    description: "Non-blocking findings filed as 999.147-999.150, and the operator cleanup listed"
    verification:
      - kind: other
        ref: "49-12 Task 3 <verify> (trap dropped, L=exec-49-12/t3): four backlog headings each unique; cleanup items present; roadmap_analyze_exit=0; gate_rc=0"
        status: pass
    human_judgment: true
    rationale: "Whether every finding was caught, and whether each 'reviewed and not filed' reason holds, is a judgment."

duration: "24h 26m wall clock (2026-10-08T00:21Z to 2026-10-09T00:47Z), most of it waiting at the attempt-2 sanitizer checkpoint for the operator's decision"
completed: 2026-10-09
status: complete
---

# Phase 49 Plan 12: Live-Run Write-Up Summary

**`49-LIVE-RUN.md` gives VERIFY-01 as met on attempt 3 and DECN-03 as Followed, and records SURV-01's no-interleaving observation without claiming anything from it. The evidence behind it is committed sanitized under `49-evidence/`, and attempt 3's copy still passes positive 11/0, negative 5/0 and pair 3/0. Four non-blocking findings are filed as 999.147 to 999.150.**

## Read first: what argues against the result

- **The binary that shipped is not this branch's code.** Attempt 3 ran a binary built from `084625c`. Attempt 2 ran one built from `e3f9ddd`. Both commits are on `fix/49-checkpoint-subagent-detection`, and neither is on `feature/phase-49` (`git merge-base --is-ancestor` exits 1 for both). The verdict covers that binary until the operator routes the commits into this branch.
- **VERIFY-01 rests on one sample.** One attempt passed. Before it came one stopped attempt and one attempt whose raw evidence fails the positive arm, `CHECK passed=8 failed=3`: no Ship snapshot, and the PR's base is `integration`. Why attempt 3 was run is unrecorded.
- **DECN-03's turn placement depends on evidence that is not committed.** Which turn carried `CODE_STAGE_POLICY` and which carried `checkpoint_auto_decide_prompt` is readable only in the local transcript, which D-26 keeps out of the repository. The committed captures do not echo prompts. Filed as 999.150.
- **This run cannot separate recency from compliance.** Turn 1 had already written the merit analysis favouring budget-120 when it reported the gate. Both texts state the same condition, so the outcome does not tell us whether the later turn's position decided anything.
- **Two committed attempt-3 captures are invalid JSON.** The sanitizer's email redaction caused it (999.147). Neither file is a checker input.

## Performance

- **Duration:** 24h 26m wall clock, mostly the operator-decision wait. Active execution time was not measured separately.
- **Started:** 2026-10-08T00:21:30Z
- **Completed:** 2026-10-09T00:47:25Z
- **Tasks:** 3/3
- **Files modified:** 255, of which 249 are evidence files.

## Accomplishments

- Sanitized evidence for attempts 1-3 is committed: 249 files, 4,583,094 bytes, none over 5 MiB. The excluded paths are
  absent: `project`, `negctl/repo`, `collected/sessions` and `prelaunch/agent-env-names.txt`. The sanitizer scan passes,
  and gitleaks in the pre-commit hook reported no leaks over the 4.59 MB.
- In `49-LIVE-RUN.md`:
  - **Verdict.** `**VERIFY-01: met**`, with its basis and limits.
  - **Attempts table.**
  - **Criteria 1-3.** Checker lines from the committed copy, the fork-point triple (`0f16ce6` three times), and the
    merge into `integration` with the PR to `main`.
  - **Gates and deviations.** Attempt 1's extra Code gate, attempt 2's rate-limit pause and resume, and the re-run with
    no recorded reason.
- **DECN-03 inputs.** `auto_decided_events=1`, `session_shared=yes`, `progressed_past_gate=yes` and
  `chosen_option=budget-120` (the favoured option, not the first-listed). The resolution and the reasoning are quoted
  verbatim.
- **SURV-01 fields.** `writers=7 temp_writes=34 interleaved_windows=0 overflow_windows=0 stop_marker=present` and
  `advance_failed_events=0`. The watch gap is unmeasured, because `.devflow/` existed before `watch_added`. The
  section carries 49-04's closing sentence.
- **Gap disposition.** The blocking fixes `e3f9ddd` and `084625c` are named and marked unmerged. Four backlog entries
  are filed, and the reviewed-and-not-filed items are each listed with a reason.
- **Operator cleanup** is listed for all three attempts: units including authprobe, scoped gate sweep, registry entry,
  three repositories after verification, and local records last.

## Task Commits

1. **Task 1: sanitize and commit the evidence, re-check it in place, write the verdict and the record.** `cb1bc48`
   (docs).
2. **Task 2: record the DECN-03 outcome and the SURV-01 observation.** `d01fa62` (docs).
3. **Task 3: dispose of the findings and list the pending cleanup.** `fbf9a60` (docs).

**Plan metadata:** this SUMMARY commit.

### Verify tails (each run with the trap dropped and `L` pre-created under `scratchpad/exec-49-12/`)

- Task 1: `committed_copy[--arm positive]=0 CHECK passed=11 failed=0` / `[--arm negative]=0 … 5 failed=0` / `[--pair]=0 … 3 failed=0` / `excluded[*]=0` ×4 / `files_over_5MiB=0 total_bytes=4583094` / `sanitizer_scan_exit=0` / `evidence_files=249` / `gate_rc=0`
- Task 2: `supported_outcome=Followed written_outcome=Followed` / both quotes `verbatim_in_resumed_capture=1` / five `surv_field` equal / `advance_failed_events=0 writeup=0` / `closing_sentence=1` / `surv_unnegated_claims=0 surv_banned_shapes=0` / `limits_bullets=6` / `gate_rc=0`
- Task 3: `backlog_refs=4`, each `roadmap_heading=1 entries_with_its_title=1` / six cleanup items present / `roadmap_analyze_exit=0` / `writeup_scan_exit=0` / `gate_rc=0`
- **Negative controls, each restored after with `cmp`.** Each gate printed `gate_rc=1` on:
  - a wrong verdict line (Task 1);
  - a `Void` label, and separately an unnegated "supports the Phase 48 guarantee" sentence (Task 2);
  - a cited `999.998` with no heading (Task 3).
- **What the gates do not check:** whether the prose is correct beyond the tokens they parse; that committed counts
  match on-disk counts (999.148); and the turn-placement note.

## Files Created/Modified

- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-LIVE-RUN.md`: the phase's write-up.
- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-evidence/attempt-{1,2,3}/`: sanitized evidence.
- `.planning/ROADMAP.md`: backlog entries 999.147-999.150 added under `## Backlog`.
- `.planning/phases/999.14{7,8,9}-*/.gitkeep`, `.planning/phases/999.150-*/.gitkeep`: backlog directories created per
  `add-backlog.md` step 4.

## Decisions Made

See `key-decisions`. One point sits outside this plan's remit and is recorded, not decided: whether attempt 2's quota
pause and resume counts as unattended, and whether it counts toward the D-11 cap.

## Deviations from Plan

**1. [Orchestrator instruction] Verify commands run without their cleanup trap**
- **Found during:** each task's verify.
- **Issue:** a Claude Code safety check refuses the plan's `trap "rm -rf \"$L\"" EXIT`.
- **Fix:** each `<automated>` command was extracted from the PLAN file byte for byte. Only `L=$(mktemp -d); ` was
  replaced with `L=<scratchpad>/exec-49-12/t<N>; `, and the trap clause was deleted. The script asserts that each
  substitution happens exactly once, and a `diff` shows those two edits as the only change.
- **Committed in:** no repository change.

**2. [Orchestrator constraint] Attempt 3's `surv/analysis.txt` derived on a staged copy**
- **Issue:** the plan runs `surv-watch.py analyze "$P49/attempt-<N>/surv"`, which writes into the raw tree. The
  orchestrator made that tree read-only.
- **Fix:** ran `analyze` on the staged `surv/`. Its output depends only on `watch.jsonl`.
- **Committed in:** `cb1bc48`.

**3. [Plan stop rule, then operator decision] Attempt 2's sanitizer refusal**
- **Issue:** the sanitizer refused `collected/devflow/phase-01-stdout` (`github_token_prefix`,
  `credential_named_assignment`) and `collected/devflow/history/phase-01/1791408678899986668-0-stdout`
  (`openai_key_prefix`, `github_token_prefix`). I stopped and reported, as the plan requires.
- **Probe:** no value was printed. None of the matches equals the live `gh auth token`, none is in the project's git
  history, and one is a `user.signingkey=` git-config line.
- **Decision:** the operator chose option A on 2026-10-08. Both files are omitted whole, and neither was edited.
- **Committed in:** `cb1bc48`. The omission is recorded in `49-LIVE-RUN.md`.

**4. [Rule 3 - Blocking] `collected/devflow/.gitignore` excluded from the evidence**
- **Issue:** each attempt's collected `.devflow/.gitignore` holds `*`. `git status --ignored` showed every
  `collected/devflow/` as ignored, so a plain `git add` would have committed none of the event logs or captures, while
  the on-disk gates still passed.
- **Fix:** excluded the file at staging. After the fix, `git status --ignored` lists nothing ignored, and 250 files were
  staged: 249 evidence files plus the write-up.
- **Committed in:** `cb1bc48`. Filed as 999.148.

**5. [Scope] Attempt 2's `resume.stdout` added beyond the curated list**
- The file is 419 bytes and is the only record of the Ship relaunch. The write-up cites it.

**6. [Minor] Backlog numbering and order**
- `phase.next-decimal` returned 999.147 before the edit and 999.151 after it, which matches 147 to 150 as the next four
  numbers.
- The four entries sit in ascending order above 999.146. The project's convention is newest first.

**7. [Minor] Commit subjects**
- The plan-mandated subjects for Tasks 1 and 3 are 73 characters. The commit-msg hook warned and did not refuse.

**Total deviations:** 7. Two were imposed by the orchestrator, one was a stop that the operator decided, one was a
Rule 3 fix, and three are minor.
**Impact on plan:** none of them changes a gate's result. Deviation 4 prevented a commit that would have looked
complete and been empty.

## Issues Encountered

- The sanitizer refusal on attempt 2 (deviation 3) was resolved by the operator's decision.
- I found the sanitizer's JSON damage (999.147) by checking that every committed JSON file parses as its raw source
  does. Only the two attempt-3 Ship-stage `phase-01-stdout` copies differ.

## User Setup Required

None from this plan. The operator's cleanup is listed in `49-LIVE-RUN.md` "Cleanup pending (operator)" and waits for
Phase 49 verification.

## Next Phase Readiness

- Phase 49 verification can start: all 12 plans have a SUMMARY.
- **Blockers for close-out, not for verification:**
  - Route `e3f9ddd` and `084625c` into `feature/phase-49` (or `develop`).
  - Rule on whether attempt 2 counts as unattended and toward the D-11 cap.
- **Scratch left for the operator to delete:**
  - `exec-49-12/` in the session scratchpad: staging copies, parked copies, probe scripts and `t1`-`t3`.
  - The ledger file `.git/gsd-plan-head-before-49-12` (in the worktree's git dir).

---
*Phase: 49-live-unattended-run-the-milestone-s-instrument*
*Completed: 2026-10-09*

## Self-Check: PASSED

Commits cb1bc48, d01fa62, fbf9a60 are ancestors of HEAD; 49-LIVE-RUN.md, 49-evidence/ and the four backlog .gitkeep files exist.
