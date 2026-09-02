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
- **`cargo test -p devflow --lib` verifies nothing.** `devflow` is binary-only (`devflow[bin]`, no
  `src/lib.rs`), so cargo exits non-zero with `error: no library targets found in package 'devflow'`
  before running a single test. Use `-p devflow --bin devflow`. `-p devflow-core --lib` *is* valid —
  that crate has a lib target. Phase 35 shipped this formulation into 21 `<automated>` acceptance
  blocks across four plans; three separate executors each rediscovered it on the clock.
- **A revert that hangs is not a revert that fails.** When proving a regression test catches a
  reverted change, a wedged harness and a failed assertion are different observations and only one
  is evidence. 35-02's first attempt polled a never-answered gate for 600s and died to SIGTERM; the
  fixture needed a pre-written abort response before the revert could produce a real
  `test result: FAILED`. Require the failing direction to print a failure, not merely to not-pass.
- **A symbol search does not find tests that reference a deleted item through its strings.** Those
  break at run time, not compile time, so the workspace still builds and only the suite catches
  them. 35-03's plan warned about this for one file and still missed two such tests in a second.
  After deleting a `pub` item, grep its *reason strings* and message literals as well as its name.
- **A grep over source counts comment prose.** 35-01's region check reported a surviving
  `unwrap_or(0)` that existed only inside a comment. Strip comments before counting, or the measure
  reports on documentation rather than on code.
- **A branch workflow run list (`gh run list`) does not establish PR check status (`gh pr checks`).**
  Workflow runs can pass on older commits while the PR's current HEAD commit has zero reported checks (e.g. if an intermediate metadata or doc commit was pushed with `[ci skip]`, or if checks are pending/untriggered). Never report that a PR is green or CI has passed without asserting directly on `gh pr checks <PR>` against the current `HEAD_SHA`.

- **`rg -c <pat> | rg '^0$'` is a constant-fail, not a zero-check.** `rg -c` prints *nothing*
  and exits 1 when a pattern has zero matches, so the downstream `rg '^0$'` never gets an input
  line. Verified both directions: a green suite and a red suite both exit 1 — the check never
  discriminated at all. Codex caught this during Phase 44's own review
  (`44-.../review_codex_terra.md:114`), Phase 44 wrote it up in `44-CODEX-E2E.md` and
  `44-04-SUMMARY.md`, and the phase still closed `status: passed` with the dead command left in
  `44-01`..`44-04` and `43-02`. Documenting a broken gate is not fixing it. Print the count and
  the command's own exit code on separate lines and assert on those.

## Prefer GSD commands over doing it by hand

When a GSD command covers the task, **use it** — `/gsd:phase` to add or edit a phase,
`/gsd:discuss-phase` / `/gsd:plan-phase` / `/gsd:execute-phase` for the phase lifecycle,
`/gsd:progress --next` to decide what comes next. Doing the same work by hand is the exception,
not the default, and it needs a stated reason.

This project *is* a workflow tool; hand-editing `.planning/` behind GSD's back is how STATE.md and
ROADMAP.md drift from what the tooling believes, and the drift is invisible until something
downstream reads the stale value.

Legitimate reasons to bypass, all of which should be said out loud rather than assumed:

- The command is known-broken for this case, and the defect is filed as a GitHub issue against
  `@opengsd/gsd-core`.
- The command produced a wrong result that needs correcting by hand afterwards — record what it
  got wrong (e.g. `phase.add` files the new entry at the document's last `---`, which lands it in
  archived prose on this roadmap; the entry then has to be moved).
- No GSD command covers it.

Bypassing silently is the thing to avoid. Correcting a command's output by hand is fine; skipping
the command because hand-editing seemed quicker is not.

## Create a git worktree for every new phase before doing its GSD work

Manual GSD lifecycle work (discuss / plan / execute done as `gsd-*` invocations rather than via
`devflow start`) must run inside a dedicated worktree, not on the main checkout. The pattern is
fully deterministic — branch `feature/phase-{N}`, path `.worktrees/phase-{N}`, base `develop`:

```bash
git worktree add -b feature/phase-35.3 .worktrees/phase-35.3 develop
```

Reason: phase work leaves the main checkout on a half-finished branch, and a later `git commit`
(see the branch-check rule above) lands on whatever is checked out. A worktree isolates the phase's
commits from that accident, the same reason `devflow start` creates one. The only step that cannot
be scripted is the branch name when a phase is renumbered (e.g. 36 → 35.3) — confirm the branch
name against `ROADMAP.md`'s current `### Phase N:` heading before running the command.

## Sync `workspace/denniyahh` before starting a new phase's branch

Before creating a new phase's branch or worktree (the step above), first bring the personal
tracking branch current with `develop`:

```bash
git checkout workspace/denniyahh
scripts/sync-workspace.sh
```

Reason: a phase branch's fork point is computed relative to `workspace/denniyahh`, not raw
`develop` — `scripts/cut-pr-branch.sh`'s own default (`WORKSPACE_BASE=workspace/denniyahh`) does
this deliberately, because `workspace/denniyahh` carries `.planning/` and personal tooling that a
raw `develop` fork wouldn't. If `workspace/denniyahh` hasn't synced recently, that merge-base sits
however many commits behind `develop`'s actual tip the last sync left it — and every one of those
commits rides along inside the new phase branch's own history, indistinguishable from the phase's
own work until something tries to separate them again.

Phase 45 hit this for real (2026-09-02): the last `sync-workspace.sh` run was 2026-08-28, so by
the time the phase branch was cut five days later, cutting a clean PR from it required manually
excluding two pre-phase-45 commits — genuinely already-superseded content whose diffs no longer
applied cleanly to `develop`'s independently-evolved `.gitignore` and hook scripts — plus an
unrelated `graphify-out/` exclusion the cutting script doesn't know about. Every one of those was
individually verifiable as safe to drop, but none of it should have been reachable from the phase
branch in the first place. Running the sync first removes the condition that produces this class
of problem, rather than requiring it be diagnosed and unwound after the fact.

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

## Keep DEV-SETUP-CHECKLIST.md in sync

`.planning/user/DEV-SETUP-CHECKLIST.md` exists so this repo's dev setup — git policy, hooks, CI,
devcontainer/toolchain pins, GSD config — can be replicated on another project. When a commit
touches any of those, update the checklist in the **same commit**, not as an afterthought.

`scripts/hooks/post-commit` mechanically warns (never edits) when one of those files changes
without the checklist moving too — that is the backstop for when this instruction gets missed,
not a substitute for following it. Proactive beats reactive: a warning after the fact means the
checklist was stale for however long between commits.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- In a temporary plan worktree, update locally but do not commit Graphify output. After the phase's source changes are integrated, refresh once and make one standalone snapshot commit on `workspace/denniyahh`, containing only `graph.json`, `manifest.json`, and `.graphify_analysis.json`; never carry that snapshot into a `develop` or `main` PR.
