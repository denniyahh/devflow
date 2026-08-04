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

## Prefer GSD commands over doing it by hand

When a GSD command covers the task, **use it** — `/gsd:phase` to add or edit a phase,
`/gsd:discuss-phase` / `/gsd:plan-phase` / `/gsd:execute-phase` for the phase lifecycle,
`/gsd:progress --next` to decide what comes next. Doing the same work by hand is the exception,
not the default, and it needs a stated reason.

This project *is* a workflow tool; hand-editing `.planning/` behind GSD's back is how STATE.md and
ROADMAP.md drift from what the tooling believes, and the drift is invisible until something
downstream reads the stale value.

Legitimate reasons to bypass, all of which should be said out loud rather than assumed:

- The command is known-broken for this case, and the defect is recorded in
  `.planning/UPSTREAM-GSD-ISSUES.md`.
- The command produced a wrong result that needs correcting by hand afterwards — record what it
  got wrong (e.g. `phase.add` files the new entry at the document's last `---`, which lands it in
  archived prose on this roadmap; the entry then has to be moved).
- No GSD command covers it.

Bypassing silently is the thing to avoid. Correcting a command's output by hand is fine; skipping
the command because hand-editing seemed quicker is not.

## Where the upstream GSD issue ledger lives

`.planning/UPSTREAM-GSD-ISSUES.md` is a **symlink**, not a file. The tracked copy lives in the
sibling gsd-core checkout:

```
.planning/UPSTREAM-GSD-ISSUES.md -> ../../gsd-core/scratch/UPSTREAM-GSD-ISSUES.md
```

File new GSD-core defects there, not in a new file here. If the path reads as missing, the
symlink was deleted (it is gitignored, so `git clean -fdx` removes it and a fresh clone never
has it) — recreate it with the `ln -s` target above, or just make any commit: `scripts/hooks/post-commit`
restores it when the target resolves.

**Do not "fix" this by tracking the symlink.** `tests/build_provenance.rs` copies every path from
`git ls-files` with `std::fs::copy`, which follows symlinks and panics on failure — on CI, or any
clone without a `gsd-core` sibling, the link dangles and the test panics.
