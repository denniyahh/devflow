---
created: 2026-09-19T01:09:40.493Z
title: Resolve the TESTING.md conflict at phase 48 merge-back
area: planning
severity: minor
files:
  - .planning/codebase/TESTING.md
  - crates/devflow-cli/src/test_support.rs
---

## Problem

Merging `feature/phase-48` back into `workspace/denniyahh` will conflict in
`.planning/codebase/TESTING.md`. Both branches edited it after their fork point `d581384`
("docs(48): pause phase 48 at 10/17 plans"):

- `workspace/denniyahh`: the 2026-09-18 codebase-map refresh (`f1307cf`) rebuilt the whole
  document from the code at `d581384`.
- `feature/phase-48`: plan 48-17 (`c1f54a7`, "docs(48-17): record test isolation evidence",
  2026-09-17) changed 9 lines of the **old** document to record the new test-isolation mechanism.

A simulated merge (`git merge-tree --write-tree HEAD feature/phase-48`, run 2026-09-18 from the main
checkout) reported `CONFLICT (content)` in TESTING.md only. It produced 4 conflict chunks, in the
Mocking, Fixtures and Factories (twice) and Snapshot Tests sections. ROADMAP.md and STATE.md
auto-merged. There are 4 chunks from a 9-line change because the refresh rewrote the text around
every line 48-17 touched.

**The risk is resolving by picking one side.** Taking `feature/phase-48` wholesale discards the
refresh (and its `last_mapped_commit` stamp). Taking `workspace/denniyahh` wholesale drops 48-17's
facts, and the map would describe test isolation as it was before phase 48.

48-17's facts, which the refreshed document lacks:
- `PATH` is supplied only on a child `Command` through
  `devflow_core::test_support::run_test_in_child`, never installed in the parent test process. Its
  replacement is always a directory, not a removed value. Every child result is checked with
  `assert_child_ran_exactly_one_passing_test` using the module-qualified test name, so an exact
  filter that runs zero tests cannot look green.
- The remaining process-global test mutations (`DEVFLOW_GATE_TIMEOUT_SECS`,
  `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, `DEVFLOW_GATE_NOTIFY_CMD`,
  `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`) are "deliberately deferred". Each must hold
  `crate::test_support::ENV_MUTEX` for the complete save, mutate, exercise and restore sequence, and
  carry a reasoned `#[expect(clippy::disallowed_methods, reason = "...")]` on its smallest enclosing
  test-only item. No second mutex may be declared.
- The D-04 invariant is reworded to "every **remaining** process-global env var is guarded by
  exactly one mutex".

Phase 48 was still active in another session when this was captured (worktree
`.worktrees/phase-48`, HEAD `2f596ab`, review-fix work uncommitted). Later phase-48 commits may add
more conflicts, including in other `.planning/codebase/` documents, because GSD's execute-plan
updates the maps after each plan.

## Solution

1. Before the merge-back, re-run `git merge-tree --write-tree --name-only workspace/denniyahh
   feature/phase-48` to get the current conflict list, not just this one.
2. Resolve TESTING.md by keeping the `workspace/denniyahh` (refreshed) version, then re-apply 48-17's
   facts above in the matching sections: take the new wording from `git show c1f54a7 --
   .planning/codebase/TESTING.md` and check each claim against `crates/devflow-cli/src/test_support.rs`
   at the merged HEAD.
3. Or take the refreshed version and re-map just that document after phase 48 lands: run
   `/gsd-map-codebase` choosing Update → TESTING.md. It stamps with `--files` automatically on an
   Update. Never delete `.planning/codebase/`; see the memory note on map-codebase refresh traps.
4. Either way, afterwards confirm TESTING.md's front-matter still carries a `last_mapped_commit`, and
   that no conflict markers remain (`rg -n '^(<<<<<<<|=======|>>>>>>>)' .planning/codebase/`).

## Resolution (2026-09-23)

Done in `42853ec` (merge of `feature/phase-48` into `workspace/denniyahh`). `git merge-tree` reported
TESTING.md as the only conflict. Kept the 2026-09-18 refresh (`last_mapped_commit` stamp intact) and
re-applied 48-17's facts in `## Environment Isolation`: child-only `PATH` via `run_test_in_child`,
`assert_child_ran_exactly_one_passing_test`, the four deferred variables, and the "remaining" D-04
wording — each checked against `test_support.rs` at the merged HEAD. No conflict markers remain.
