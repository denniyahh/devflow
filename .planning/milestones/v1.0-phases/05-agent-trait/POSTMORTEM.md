# Branch Divergence Post-Mortem

> 2026-06-18 — Analysis of feature/phase-04 vs develop divergence

## What Happened

Two branches independently developed the same codebase for **months**, starting from the same MVP commit (`d4266e8`), producing competing implementations of the same features.

```
d4266e8  ← MVP (the only common ancestor)
├── feature/phase-04 (12 commits)  ← agent trait, ship fixes, config unification
└── develop (18 commits)           ← CI, version bumper, verify/docs, 84 tests
```

**14 files were modified on BOTH branches**, 6 of which are core Rust modules with conflicting implementations. The branches were never rebased onto each other.

## Timeline Reconstruction

| When | What |
|---|---|
| **Dec 2025** | MVP commit. Both branches diverge from here. |
| **Jun 16** | `feature/phase-04` created (originally a "working branch") — accumulated 12 commits: agent trait, ship fixes, config merge, version bumps, dogfooding logs. **Commits span v0.2.0 through v0.5.0.** |
| **Jun 17** | GSD initialized on `develop`. Phases 1-3 executed cleanly: CI pipeline, 84 tests, version bumper expansion, verify/docs commands. |
| **Jun 17** | Phase 4 Hermes skill **also committed to develop** (`3359395`) — overlapping with phase-04's skill work. |
| **Jun 18** | Discovered: `feature/phase-03` (dead OMX cache branch, deleted). `feature/phase-04` had 12 unmerged commits with 14-file conflicts. |

## Root Causes

### 1. No Single Source of Truth for "Current Branch"

`feature/phase-04` was NOT created via `devflow start`. It was a manually-created working branch that accumulated work across multiple phases. Nobody knew it existed because:
- `devflow status` only shows the active pipeline, not all branches
- `STATE.md` was stale (said "Phase 1 complete, ready for Phase 2" while Phase 2-3 were already merged)
- No command exists to list all devflow-managed branches

### 2. `devflow start` Has No Pre-Flight Checks

The current `feature_start` uses `git checkout -B` which **force-resets** an existing branch. If you run `devflow start --phase 4` when `feature/phase-04` already exists (from a prior session), it silently nukes it — no warning, no error.

### 3. No Divergence Detection

Once a feature branch is created, nothing tracks whether it's fallen behind develop. The branch can sit for weeks accumulating commits while develop races ahead. Result: 14-file conflict nightmare.

### 4. No Branch Lifecycle Enforcement

After merging a phase to develop, nothing verifies the feature branch was actually deleted. `feature/phase-03` should have been deleted when Phase 3 merged — instead it sat there with OMX cache files and stale code.

### 5. Manual Workflow Bypasses Safety

The devflow skill explicitly recommends driving Claude directly (`git checkout -b`, `claude -p`, `git merge`), bypassing devflow's state machine entirely. This is documented as a workaround for the multiline prompt bug, but it means branches are created outside devflow's awareness.

## Concrete Fixes for DevFlow

### High Impact / Low Effort

| # | Fix | What it prevents |
|---|---|---|
| 1 | **`devflow start` refuses if branch exists** | Change `checkout -B` to `checkout -b` (error if exists). Add `--force` flag for override. | Overwriting work from a previous session. |
| 2 | **`devflow list` command** | Show all `feature/phase-*` branches with divergence from develop (commits ahead/behind). | Branches going unnoticed for months. |
| 3 | **`devflow status` shows all open branches** | Add a "Open branches" section to status output with divergence counts. | Same as above, but in the command users already run. |

### Medium Impact / Medium Effort

| # | Fix | What it prevents |
|---|---|---|
| 4 | **Pre-start divergence check** | On `devflow start`, warn if current branch is >N commits behind develop. Require `--force` if >50 behind. | Starting from a stale base. |
| 5 | **`devflow rebase` command** | `devflow rebase` → rebases current feature branch onto develop. `devflow rebase --all` → rebases all open feature branches. | Branches going stale. |
| 6 | **Post-merge cleanup verification** | After `feature_finish`, verify the branch was deleted. If not, warn and offer `devflow cleanup`. | Ghost branches accumulating. |

### Lower Priority / Larger Effort

| # | Fix | What it prevents |
|---|---|---|
| 7 | **Branch scope enforcement** | On `devflow start`, scan for commits on develop that reference the same phase number. Warn if phase work already exists on develop. | Duplicate phase work. |
| 8 | **Pre-commit hook detection** | `devflow check` could verify the feature branch hasn't diverged beyond a threshold, or that no other feature branches touch the same files. | Merge conflict surprises. |
| 9 | **Git hook integration** | Ship an optional pre-push hook that warns about stale feature branches. | Catching divergence before it reaches remote. |

## What To Do First

The **three high-impact/low-effort fixes** (#1-3) should be Phase 5 (agent trait) or a dedicated Phase 5b:

1. **`devflow list`** — most immediate value, lets you SEE the problem
2. **`devflow start` safety** — prevents creating the problem
3. **`devflow status` enhancement** — surfaces the problem in daily workflow

These are ~200 lines of Rust, mostly in `git.rs` (new `list_branches` method) and `main.rs` (new `List` subcommand + status enhancement).
