---
created: 2026-09-19T00:33:21.144Z
title: Merge devcontainer comment fix into phase 48 before its combined PR
area: tooling
severity: minor
files:
  - .devcontainer/devcontainer.json:1-3
---

## Problem

The header comment in `.devcontainer/devcontainer.json` described `rust-toolchain.toml` as
`channel = "stable"`, but that file pins an exact version (`1.97.1`). The fix is committed as
`24365ae` on branch `chore/devcontainer-toolchain-comment`, which was cut from `origin/develop` and
lives in worktree `.worktrees/chore-devcontainer-comment`. It is open as PR #216 against `develop`.

**Operator decision (2026-09-18):** do not merge #216 on its own. Once phase 48 is done, merge the
chore branch into `feature/phase-48` and ship both in one combined PR to `develop`.

State of #216 when this was captured: every check passed except one of the two
`Devcontainer / Build + test in devcontainer` runs. The `pull_request` run `35408772328` (attempt 1)
timed out in `start_defaults_to_worktree` (`crates/devflow-cli/tests/phase7_cli.rs:1311`, helper
`wait_for_settled`), while the concurrent `push` run `35408739085` passed on the same commit. That is
a timing flake of the 999.55 / 999.123 class, recorded in ROADMAP 999.55. The change is comment-only,
and `scripts/assert-image-parity.sh` passes with the extracted image.

## Solution

1. After phase 48 finishes, merge `chore/devcontainer-toolchain-comment` into `feature/phase-48`.
   Expect exactly one new commit (`24365ae`): `workspace/denniyahh` was 0 commits behind
   `origin/develop` at capture time, so the branch's base `add41a6` is already in phase 48's
   history. Re-check with `git log feature/phase-48..chore/devcontainer-toolchain-comment`.
2. Cut the combined PR to `develop` the usual way (`scripts/cut-pr-branch.sh`). Confirm
   `.devcontainer/devcontainer.json` is in the filtered PR branch, since it is not a personal
   artifact.
3. Close #216 with a comment linking the combined PR. Then delete the branch and remove the worktree
   (`git worktree remove .worktrees/chore-devcontainer-comment`).

## Resolution (2026-09-23) — superseded

PR #216 was merged into `develop` on its own at 2026-09-23T17:36Z (merged by the `denniyahh` account),
so the combined-PR plan no longer applies. `24365ae` is on `develop`, the chore branch is gone locally
and on `origin`, and its worktree is removed. The Phase 48 PR (`feature/phase-48-pr`) is cut from a
`develop` that already contains it.
