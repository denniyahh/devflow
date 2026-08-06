---
phase: 17-pipeline-dogfood-followup
plan: 10
subsystem: infra
tags: [rust, cli, git, build-provenance, hooks, gap-closure, dogfood-finding]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "Plan 05's staleness gate, Plan 06's strict-ancestor tightening, Plan 07's Ahead classification"
provides:
  - "tracked_source_newer_than_build only considers build inputs (*.rs, Cargo.toml, Cargo.lock, build.rs, rust-toolchain.toml) — a dirty doc no longer reads as a stale binary"
  - "content hooks (DocsUpdate, ChangelogAppend) execute against the phase's worktree; terminal hooks (Merge, VersionBump, BranchCleanup) still execute against the primary checkout"
  - "Phase 17's changelog entry now lives on the branch it describes"
affects: [18-hermes-support]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Hook batches carry a target-tree policy: content hooks author into the branch's worktree, checkout hooks operate on the primary checkout"

status: complete
completed: 2026-07-19
---

# 17-10: DevFlow blocked its own Ship stage

## What happened

Phase 17's **Ship** stage was hard-blocked by `self_dogfood_stale_blocked`,
entirely self-inflicted, via a three-step cascade:

1. The Validate→Ship transition ran `DocsUpdate` and `ChangelogAppend`
   (`hook_run`, ts `1784492315`).
2. Both hooks executed against **`project_root` (`develop`)** rather than the
   worktree holding all of Phase 17's commits. `ChangelogAppend` left
   `CHANGELOG.md` dirty on `develop`; `DocsUpdate` went further and **committed**
   `f5692d3 docs: update generated docs` — an **empty** commit, zero files
   changed.
3. That produced two independent blocks:
   - `combined_staleness`'s mtime arm saw a dirty tracked file (`CHANGELOG.md`,
     16:18:35) newer than the build (14:40:54) and returned `Stale`.
   - The empty commit moved `develop`'s HEAD, so `develop` and
     `feature/phase-17` **genuinely diverged** (merge-base fell back to the fork
     point `a2c314f`). Both ancestry probes then exited 1, which is correctly
     `Stale` — a divergent build really is stale.

The second block is worth stating plainly: Plan 07's logic was **not** wrong
there. A no-op commit created by the pipeline's own misdirected hook diverged the
branches, and the gate did exactly what it should.

## What changed

**Task 1 — mtime arm scoped to build inputs.** `tracked_source_newer_than_build`
now filters `git ls-files -m` through a new `affects_compiled_binary` helper
(`*.rs`, `Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml`). A dirty
`.md` or `.planning/` file cannot change a compiled binary and no longer
contributes to a `Stale` verdict.

**Task 2 — hook target tree resolved per batch.** New `hook_context_root`:
content hooks for the Validate→Ship transition run in `state.worktree_path` when
it exists; the terminal batch keeps `project_root`, because merging *into* the
base branch, tagging it, and deleting the feature branch are primary-checkout
operations that would break if retargeted. Falls back to `project_root` when no
worktree is configured, so `--no-worktree` runs are unaffected. The checkout lock
stays keyed on `project_root` — it serializes cross-phase contention, which is
unrelated to where a content hook writes.

**Task 3 — stranded entry reunited with its branch.** The changelog entry was
moved into the worktree and committed on `feature/phase-17` (`bde8f73`), and
`develop` was restored to a clean tree. The empty `f5692d3` was dropped by
resetting `develop` to `a2c314f`, after verifying it was empty, local-only
(`origin/develop` sits far behind at `c034ad7`), contained in no remote branch,
and that the working tree was clean.

## Verification

- Task 1 had a genuine behavioral RED: the new test failed with
  `left: Some(true), right: Some(false)` on a dirty `CHANGELOG.md` — the exact
  live Ship-block condition — then passed.
- Task 2's RED was only a compile error (the helper did not yet exist), because
  the implementation was written before its test. Weaker evidence than Task 1;
  recorded honestly rather than presented as equivalent.
- `combined_staleness_mtime_arm_flags_dirty_tree_newer_than_build` broke and was
  **corrected, not deleted**: its fixture dirtied `a.txt`, encoding the
  over-broad behavior. Switched to `src/lib.rs`, preserving the test's intent
  (mtime flips an ancestry-Fresh result to Stale) under the corrected scoping.
  Same class of correction Plan 06 applied to a test that had encoded a bug.
- `cargo test --workspace` green; `cargo clippy --workspace --all-targets --
  -D warnings` clean; `cargo fmt --check` clean. Plan 06's strict-ancestor block
  test and Plan 07's Ahead test both still pass.

## Findings recorded, not fixed

- **A third flake class.** During verification,
  `terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished`
  failed once with `Os { code: 2, kind: NotFound }` while spawning `git` — a PATH
  race between tests mutating process-global env. It passed 3/3 in isolation and
  the full suite passed on re-run. Not caused by this plan; had it not been
  investigated it would have prompted a fix to a nonexistent bug. Distinct from
  GAP-2's gate-poll hang.
- **`ChangelogAppend` writes placeholder content.** The entry body is
  "Released phase via DevFlow" — it describes nothing about what the phase
  delivered. A content-quality question, deliberately out of scope here.
- **`DocsUpdate` can produce empty commits.** `f5692d3` changed zero files. Even
  once retargeted at the worktree, a hook that commits nothing should probably
  not commit at all.
