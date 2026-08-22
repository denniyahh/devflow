---
status: backlog
source: TEST-SUITE-QA-REVIEW.md (Codex, 2026-07-21), P0 recommendation #1 — reviewed and re-scoped by Claude same day
---

# Backlog: Hermetic Tests for Shell Entry Points

## Problem

Three shell scripts have user-facing, side-effecting behavior with no direct
behavioral tests:

- `scripts/install.sh` — downloads tools, clones repositories, builds, copies
  files. Every new user's first-run experience.
- `scripts/sync-main-to-develop.sh` — fetches and mutates real git history.
  Already self-verifying (asserts before/after tree-hash equality before
  proceeding), which lowers but does not eliminate its risk.
- `scripts/deploy.sh` — builds and pushes the docs wiki to `gh-pages` via
  `mkdocs gh-deploy --force`.

## Re-scoped priority (2026-07-21, Claude review)

The original QA review treated all three as equally P0. Reviewed each
directly: `deploy.sh` only touches the `gh-pages` branch for documentation —
worst case on failure is a stale wiki, not a broken release or corrupted
history. Demoted relative to the other two. `install.sh` and
`sync-main-to-develop.sh` remain the real P0 surface: one is the highest-blast-
radius first-run path for every user, the other mutates real branch history
(with only partial self-verification).

## Proposed shape

- Add `shellcheck` to CI for all four scripts (including `scripts/hooks/pre-push`
  for completeness, even though it's lower-risk).
- Behavioral tests that run each script with a fake `PATH`, recording argv and
  simulating success/failure for `curl`, `git`, `cargo`, `cp`, and deployment
  commands. Must use temporary repositories; must never touch the network or a
  real remote.
- Acceptance targets: every external command and destructive step has a tested
  failure path; fail-fast ordering is observable through captured invocations;
  re-running `install.sh` is tested for idempotency; sync/deploy tests prove
  the intended ref/remote rather than matching script source text.

## Notes

The fake-`PATH` harness itself becomes something to maintain — every time
`install.sh`'s real download targets or tool versions change, the harness needs
updating too. Budget for that as an ongoing cost, not a one-time build.

Promote with `/gsd-review-backlog` when ready.
