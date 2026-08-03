# DevFlow — project instructions

Deliberately minimal. Global rules live in `~/.config/agents/AGENTS.md`; this file holds only
constraints specific to how *this* repository is worked on.

## Never run git operations while an executor holds the working tree

Some plans run their executor on the **main checkout** rather than in a worktree — deliberately,
because they are experiments measuring real process behaviour and a worktree would add an
uncontrolled variable to the thing under test (30-02 and 30-04 in phase 30 are the precedent).

While such an executor is running, the orchestrator must not touch git at all: no `add`, no
`commit`, no `push`, no branch or tag operations. Wait for the executor to report.

Two separate failures on 2026-08-02, both from ignoring this:

1. **A `git push` mid-run failed the pre-push gate.** `build_provenance` copies every tracked file
   and panics if one is missing; the executor had just deleted a trial file it had already
   committed, so `git ls-files` listed a path that was momentarily absent. It read as a broken
   build. Nothing was broken.
2. **A blanket `git add -A` swept the executor's in-progress evidence into an unrelated commit.**
   The executor caught it and corrected 21 files, but only because it re-verified HEAD against
   disk. It could as easily have shipped.

Worktree-isolated executors do not have this problem — they own a separate tree. The rule applies
specifically to the main-checkout case.

## Verification habits this repo has already paid for

- **`cargo test --exact <name>` exits 0 when the name matches nothing.** Assert on a real
  `1 passed` with a non-zero `filtered out` count. The package is `devflow`, not `devflow-cli`.
- **A pipeline's exit code is the last command's.** `git push … | tail` reports `tail`'s success.
  Capture the exit code of the command you care about, not the pipeline.
- **Old phases use a bare `PLAN.md`**, not `NN-PLAN.md`. A glob for `*-PLAN.md` silently misses
  them, and `.planning/superseded/` holds abandoned plans that should not be counted at all.
