---
status: backlog
source: Phase 20 discuss-phase decision D-03 (2026-07-22)
---

# Backlog: Release-Cut Executor (`devflow release` that executes)

## Goal

A `devflow release` command that *executes* the full release-cut sequence —
version-bump PR → merge to `main` → signed tag → sync `develop` → publish
`devflow-core` then `devflow` to crates.io — not just the read-only preflight.

Phase 20's 20d (`devflow release --check`, DEN-38) delivers only the preflight:
it verifies the four things that break a release cut (workspace self-pin match,
`develop`/`main` divergence, crates.io publish order, tag-signing viability) and
reports pass/fail. It deliberately does **not** run the cut. Phase 20 CONTEXT.md
D-03 locked `--check`-only for that phase and recorded this executor as the
follow-up so the larger design question wasn't lost.

## Why this matters

`--check` catches 3 of the 4 v1.5.0 release-cut failures *before* the operator
starts, but the cut itself stays a hand-run checklist (`CONTRIBUTING.md`
§ "Cutting a Release"). Every manual release is another chance to skip
`scripts/sync-main-to-develop.sh`, publish in the wrong order, or fumble the
signed tag — the exact failure classes 20d only *warns* about. Automating the
execution is what actually retires the checklist.

## Why it was deferred (not folded into Phase 20)

This is a materially larger and riskier unit than the preflight: it drives
irreversible operations — a squash-merge to `main`, a signed tag, and a
crates.io publish that can never be un-published or reused. It deserves its own
discuss-phase design pass on failure/rollback semantics (what happens when the
tag lands but the publish fails? when core publishes but cli does not?), not a
ride-along on the `--check` unit. See also the manual-merge discipline captured
in the operator's own release notes (`[[feedback-manual-merge-must-replicate-ship]]`).

## Possible shapes (not yet decided)

- A staged `devflow release --execute` that runs the same four checks as
  `--check` first, hard-stops on any failure, then drives merge → tag → sync →
  publish with an explicit gate before the irreversible publish step.
- Reuse of the existing after-ship hook batch machinery (`VersionBump`,
  `ChangelogAppend`) rather than a second version-writing path — the same
  "one effect, don't reimplement" principle Phase 20's 20e applied to
  `finish_workflow`.
- Publish ordering must encode the crates.io constraint 20d only asserts:
  `devflow-core` must be live on the registry at a satisfying version before
  `devflow`'s publish/verify can resolve its path dependency.
- Tag signing must reuse 20d's `gpg.format`-aware viability check as a
  precondition, not rediscover the `ssh_askpass` failure at tag time.

## Notes

Blocks on Phase 20's 20a (self-pin fix) and 20d (`--check`) landing first —
the executor's preflight step *is* 20d's check, and its `VersionBump` step
inherits 20a's self-pin correctness.

Priority: High — it's what actually removes the manual release checklist.
Size: L — drives irreversible `main`/tag/crates.io operations; needs its own
rollback-semantics design pass.

Promote with `/gsd-review-backlog` when ready.
