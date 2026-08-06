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
- **`git commit` runs against whatever branch is currently checked out, not the branch you last
  reasoned about.** An unrelated `git checkout` earlier in a long tool sequence (2026-08-06:
  switching to `main` to delete a merged feature branch) silently changed what a much-later commit
  landed on — `develop`/`main` are both protected, so this is normally caught, but only if the push
  actually targets the branch that moved. `git push origin <branch>` pushes the *named local
  branch*, not the checked-out one; if they've diverged, a stale-but-unpushed named branch can
  report "Everything up-to-date" while the real mistake sits uncommunicated on whatever's checked
  out. Before any `git commit` not immediately preceded by a `git checkout` in the same call, run
  `git rev-parse --abbrev-ref HEAD` and confirm it matches the intended target — protected-branch
  rejection is the backstop, not a substitute for checking, and it only fires if the push actually
  reaches the branch that's wrong.

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

## Keep the active milestone's phase headings inside its own window

`gsd-tools`' milestone-scoped parsers (`roadmap.analyze`'s `extractCurrentMilestone`,
`deriveProgressFromRoadmap`) scope to the *active* milestone's heading-to-next-heading window in
`ROADMAP.md` — from its `## ` milestone heading up to the next `## ` heading. If that window ends
up prose-only (no `### Phase N:` headings, no `## Progress` table), the parsers misfire silently:
`roadmap.analyze` reports `phase_count: 0`, `milestone.complete --dry-run` falls back to a
pass-all degrade that sweeps every directory on disk, and `state.validate` falls back to a
two-scale STATE.md comparison instead of roadmap-derived counts. This happened for real (999.72,
999.72a) and was resolved for the `gsd-hygiene` milestone incidentally, by `gsd-roadmapper`'s own
write when creating it — nothing guarantees the next milestone lands the same way.

When declaring or archiving a milestone by hand (bypassing `/gsd-new-milestone` /
`/gsd-complete-milestone`, or double-checking their output):

- The active milestone's own `### Phase N:` headings must land inside its own
  heading-to-next-heading window — not below a later closed-milestone heading.
- `ROADMAP.md` must keep its `## Progress` table (columns `Phase`, `Plans Complete`, `Status`,
  `Completed`). There is no repair verb if it goes missing — `phase.complete` only maintains an
  existing table, it does not create one.
- After any milestone declare/archive edit, spot-check with `gsd-tools query roadmap.analyze` —
  a non-zero `phase_count` and a real `next_phase` confirm the window is intact.

See ROADMAP.md's `999.72` / `999.72a` backlog entries (resolved by Phase 32) for the full
root-cause history.

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

## Keep DEV-SETUP-CHECKLIST.md in sync

`.planning/DEV-SETUP-CHECKLIST.md` exists so this repo's dev setup — git policy, hooks, CI,
devcontainer/toolchain pins, GSD config — can be replicated on another project. When a commit
touches any of those, update the checklist in the **same commit**, not as an afterthought.

`scripts/hooks/post-commit` mechanically warns (never edits) when one of those files changes
without the checklist moving too — that is the backstop for when this instruction gets missed,
not a substitute for following it. Proactive beats reactive: a warning after the fact means the
checklist was stale for however long between commits.
