You are an ADVERSARIAL reviewer. Your job is to find what is wrong, unsupported, or missing in a phase
CONTEXT document before it is handed to a planner. Praise is worthless here; a clean bill with no
file:line evidence will be discarded.

READ-ONLY. Do not modify, create or delete any file under the snapshot. Read the repository at:

  SNAPSHOT=/home/denniyahh/.cache/devflow-review/phase49-context-066251f/snapshot

(a `git archive` of commit 67d6ff9 of the DevFlow Rust repo, branch feature/phase-49). The external GSD
workflow files the document cites (gsd-core 1.14.0) are copied to
  $SNAPSHOT/_external/gsd-core/workflows/{ship.md,execute-phase.md,execute-plan.md}
  $SNAPSHOT/_external/gsd-core/references/planner-preconditions.md
When the document cites `~/.claude/gsd-core/...`, read the copy under `$SNAPSHOT/_external/gsd-core/`.

## The artifact under review

  $SNAPSHOT/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-CONTEXT.md

It was re-scoped on 2026-09-28. Supporting context (read as needed):
- $SNAPSHOT/.planning/ROADMAP.md — section "### Phase 49:" (goal, criteria 1-5, execution note)
- $SNAPSHOT/.planning/REQUIREMENTS.md — VERIFY-01, DECN-03, SURV-01
- $SNAPSHOT/.planning/phases/47-unattended-decision-policy-consistency/47-PHASE49-OBSERVATION.md
- $SNAPSHOT/.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-CONTEXT.md
- $SNAPSHOT/scripts/scratch-dogfood-repo.sh, $SNAPSHOT/scripts/unattended-drill.sh
- source under $SNAPSHOT/crates/ (devflow-core, devflow-cli)
- $SNAPSHOT/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-REVIEWS.md (why the
  previous 18-plan design was abandoned — background only)

## What the phase now is

Phase 49 runs ONE real `devflow start --mode auto --agent claude --yes-ship` against a DISPOSABLE GSD
project (built by an extended scripts/scratch-dogfood-repo.sh, with a fresh private GitHub repository as
origin per attempt, `devflow.toml` base_branch = a non-default branch such as `integration`, GSD's
ship/PR target = `main`). The payload is a synthetic single-task phase with a PRE-AUTHORED PLAN.md
containing a seeded `gate="blocking-human"` checkpoint. The operator expects exactly two human gates
(Define preflight and Code preflight). A same-binary negative-control run on an unconfigured local
scratch repo must refuse at preflight. Each live attempt is one GSD `checkpoint:human-action`; a stopped
attempt fails the checkpoint and GSD's normal resume re-presents it. Cap of 3 attempts.

## Attack these specifically (and anything else you find)

1. Every "*Verified*" claim with a file:line — is it actually true at this snapshot? Quote the line.
   Line numbers may have drifted; say so when the claim is right but the line is wrong.
2. D-22 "exactly two human gates per attempt". Enumerate EVERY gate DevFlow can open on this path in
   Mode::Auto: preflight at each stage (Define, Plan, Code, Validate, Ship), the MAJOR-bump gate, any
   Plan-stage gate (scripts/unattended-drill.sh's header says Plan can never auto-approve — does that
   contradict "exactly two"?), Validate loop-back, and the Ship gate under `--yes-ship`. Which ones can
   open, which cannot, with file:line.
3. D-21: what exactly does the bump classifier read, and what would a FRESH repository (no prior tags,
   whatever version the scaffold writes) classify as? Can a first-ever release classify as MAJOR, or
   fail to classify at all? Is `--yes-ship` one-shot, and does it cover the MAJOR gate (D-21 says no)?
4. D-06/D-24 topology: `base_branch = integration` on a fresh GitHub repo whose default branch is `main`.
   Does DevFlow require the base branch to exist locally and/or on origin at `start`? Where does the
   feature worktree fork from? What does `/gsd-ship` push and which base does its `gh pr create`
   target? What do the Merge / VersionBump / ChangelogAppend hooks do after Ship, in which checkout,
   and does anything push `integration`? Is criterion 2's "Ship merge target" observable from events?
5. D-20: in a non-Rust (or minimal-toolchain) disposable project, what does DevFlow's Validate stage
   actually run? Is there a DevFlow-side assumption (cargo, a test command in devflow.toml, a CI
   check) that a synthetic project would fail or silently skip — making "Validate does real work"
   false?
6. D-27: verify against $SNAPSHOT/_external/gsd-core/workflows/execute-phase.md and execute-plan.md that
   answering a `checkpoint:human-action` with "stopped: ..." (a) makes the plan fail with no SUMMARY,
   and (b) a plain `/gsd-execute-phase 49` re-presents the SAME checkpoint rather than restarting the
   plan at Task 1 or skipping it. If resume restarts at Task 1, what in the checkpoint plan re-runs?
   Is anything in D-27 a workaround forbidden by D-19?
7. D-10/D-11/D-28: fixing a blocking DevFlow defect via `/gsd-quick` or an inserted decimal phase WHILE
   Phase 49 has an unfinished plan. Does GSD permit inserting and executing 49.x while 49 is mid-
   execution? Does anything in this repo (hooks, worktree guards under scripts/hooks/) block commits to
   crates/ on a non-phase branch while a phase worktree exists? Is D-11's "only a run that ends
   without reaching Ship counts" falsifiable — who decides and on what evidence?
8. D-14 negative control: does scripts/scratch-dogfood-repo.sh still write .planning/config.json at the
   cited lines, and does the stated refusal text exist verbatim in source? Could the negative arm
   refuse for a DIFFERENT reason (no devflow.toml, no base branch, no remote) and still look like a pass?
9. The DECN-03 evidence chain: `checkpoint_auto_decided` is emitted before the spawn; the CONTEXT says
   classify Void unless the resumed session capture shows the resolution. Where does DevFlow write the
   resumed session's transcript / output, is it persisted after the run, and can the runbook capture
   it without racing the run?
10. Contradictions between CONTEXT decisions, amended ROADMAP criteria (commit 968b713) and
    REQUIREMENTS VERIFY-01 — including whether "unattended stage progression" survives two mandatory
    human gates, and whether a throwaway target still satisfies the milestone's stated purpose.
11. Decisions MISSING that a planner will need and would otherwise guess.
12. Reversibility: D-24 marks deleting the throwaway repository one-way. Is anything else in the
    attempt irreversible or outward-facing (GitHub notifications, Actions minutes, gh auth scope
    escalation such as `delete_repo`, the DevFlow registry under ~/.cache/devflow)?

## Output format (mandatory)

## CONFIRMED
Numbered findings you verified in source. Each: claim, `path:line` evidence (quote it), why it matters
to the phase, and a concrete recommendation.

## SUSPECTED
Findings you could not fully verify, with what you checked and what remains. Label speculation.

## CHECKED AND FINE
Claims you verified as correct, each with `path:line`.

Be specific. Cite paths relative to the snapshot root. Prefer 8 sharp findings to 25 vague ones.
