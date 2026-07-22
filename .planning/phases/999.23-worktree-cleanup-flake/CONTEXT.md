---
status: backlog
source: v1.6.0 release PR #13, CI run 29939619958 (2026-07-22)
---

# Backlog: Flaky `reference_and_cleanup_worktree_cli_flow`

## Goal

Stop `reference_and_cleanup_worktree_cli_flow`
(`crates/devflow-cli/tests/phase7_cli.rs:82`) from intermittently failing on
GitHub Actions runners with:

```
devflow ["cleanup", "--force"] failed
error: git worktree command failed: error: failed to delete '.git/worktrees/phase-08': Directory not empty
```

## Evidence that this is a flake, not a regression

Observed on the v1.6.0 release PR at SHA `e804280`:

- That commit's code is **byte-identical** to `1e2ddbb`, which passed the same
  CI job minutes earlier — the commit touched only `.planning/` docs.
- The test is untouched by Phase 19.
- It passed **5/5** consecutive local runs.
- Re-running the failed CI job passed with no code change.

## Why this matters

This sits in the release gate. A test that fails on a coin flip makes
release-day CI unreliable and, worse, trains the reader to re-run red CI
instead of investigating it — the exact reflex that lets a real regression
through.

Fourth instance of this family in the project's history (WR-03 / 18-02
parallel-worktree capture timing; 17-09 GAP-2 concurrent-ship gate wedge; 19i
PATH race), so treat it as a recurring structural weakness in worktree-touching
tests rather than one bad test.

## Likely cause

`git worktree remove` racing the filesystem: something still holds a handle
inside `.git/worktrees/phase-08` between the removal attempt and the directory
unlink. Slower/parallel CI runners widen a window a local run almost never hits.

## Possible shapes (not yet decided)

- Make `cleanup --force` tolerate `Directory not empty` by retrying with
  bounded backoff, then falling back to `git worktree prune`. This is arguably
  a **product** fix, not a test fix — a user hitting this gets the same opaque
  error.
- Isolate the fixture so each test gets its own git dir.
- If it is genuinely only test-harness sequencing, fix teardown ordering rather
  than adding a sleep.

Prefer the product-level fix if the race is reachable by a real user running
`devflow cleanup --force`. Determine that first — do not paper over it in the
test if the CLI has the same hole.

Promote with `/gsd-review-backlog`.
