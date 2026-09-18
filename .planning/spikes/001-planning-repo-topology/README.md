---
spike: 001
idea: planning-repo-topology
name: planning-repo-topology
type: standard
validates: "Given a spec repo holding .planning/ with the code repo as a child directory, when GSD plans and executes one phase and DevFlow runs one stage, then planning commits land in the spec repo, code commits land in the code repo, and DevFlow's checks can find both"
verdict: PENDING
related: []
tags: [gsd, devflow, git, worktrees, sub_repos, planning]
---

# Spike 001: Where `.planning/` Lives Relative to the Code Repo

**Status (2026-09-18):** documented, **not run**. The operator wants to reason further about the
pros, cons and possibilities of each approach before running any trial. Everything below is desk
analysis: reading code, scripts and GSD's documentation. No experiment has been built.

**Provenance.** Statements marked *(operator)* are the operator's. Everything else is Claude's
analysis from 2026-09-18 and is not a decision.

## What This Validates

Given a spec repo holding `.planning/` with the code repo as a child directory (option B), when GSD
plans and executes one phase and DevFlow runs one stage, then planning commits land in the spec
repo, code commits land in the code repo, and DevFlow's checks can find both.

The trial is deferred. The rest of this document is the reasoning it would test.

## Background

- **How this came up.** 999.110 was rescoped on 2026-09-18 into a `devflow init` + `devflow doctor`
  initiative that writes `base_branch` to `devflow.toml`. The value is the project's whole
  integration trunk: phases fork from it and merge back into it (`crates/devflow-core/src/config.rs:103-111`).
  Pointing it at a personal branch that tracks `.planning/` therefore makes Ship merge phase work
  into that personal branch instead of `develop`.
- **The operator's proposal *(operator)*.** Have `devflow.toml` hold two settings, one for the spec
  and environment repo and one for the code repo, instead of a single location.
- **Two readings of that proposal.** Two *branches* in one repository does not remove the
  conflict. Two separate *repositories* does, at a cost. Both are analysed below.

## The Options

### A — `.planning/` tracked on a personal branch (what runs today)

The operator described this as copying `.planning/` in and blocking its commits with hooks. The
actual mechanism, verified 2026-09-18:

- `.planning/` is **tracked** on `workspace/denniyahh`. `develop`'s `.gitignore` ignores it (line 63
  on `origin/develop`), and the personal branch drops that line.
- `scripts/phase-worktree.sh` creates `.worktrees/phase-N` **from `workspace/denniyahh`**, not
  `develop`, and asserts that the base tracks `.planning/`. It syncs the workspace branch with
  `develop` first, via `scripts/sync-workspace.sh`.
- `.planning/` commits are **allowed** on phase branches. `scripts/hooks/pre-commit` leaves
  `.planning/` off its personal-artifact list on purpose, because GSD's executor worktrees fork from
  the phase branch's committed HEAD and would not see uncommitted planning files.
- The boundary is at **push time**. `scripts/hooks/pre-push` scans every commit a push introduces
  and refuses personal or `.planning/` paths going to `main`, `develop`, `feature/*`, `fix/*`,
  `chore/*` and `release/*`.
- Code reaches `develop` through `scripts/cut-pr-branch.sh`, which builds a filtered `-pr` branch
  with the personal and planning paths removed (for example PR #213,
  `feature/phase-47-pr` → `develop`, merged). `scripts/sync-workspace.sh` then merges `develop` back
  into the workspace branch.
- These five scripts and hooks total about 708 lines. `workspace/denniyahh` is 253 commits ahead of
  `origin/develop` and 0 behind (2026-09-18).

### A′ — DevFlow automates option A

The repository layout stays the same. DevFlow gets two settings: the planning branch (fork source)
and the code trunk (merge and PR target). Ship builds the filtered PR branch itself, doing what
`cut-pr-branch.sh` does today.

- Fixes Ship's merge target and removes the manual steps without changing the layout.
- The history filtering and the ~708 lines move into DevFlow; they do not disappear.
- Rewritten PR commits mean the SHAs on `develop` are not the SHAs that were tested on the phase
  branch. The code is identical, but the history link is lost.

### A as two branches in one repo — why that reading does not resolve it

With `code = develop` and `spec = workspace/denniyahh`, a worktree is still a checkout of one
branch. Forked from `develop`, it has no `.planning/`, so DevFlow would have to bring it in from the
other branch. That is the copy/symlink step the 2026-08-23 decision rejected. And every PLAN.md or
SUMMARY.md the agents commit during the phase lands on the code branch and reaches `develop` at
merge.

### B — separate spec repo, code repo as a child directory (GSD `sub_repos`)

GSD documents this layout in `~/.claude/gsd-core/references/git-integration.md`
(`<sub_repos_support>`) and `references/planning-config.md`:

- An outer directory holds `.planning/`. Code repos are child directories with their own `.git`,
  listed in `.planning/config.json` under `planning.sub_repos`. The list is auto-detected and
  re-synced on every config load.
- Executors commit code with `gsd_run query commit-to-subrepo`, which routes each file to its repo.
  `agents/gsd-executor.md` records each repo's commit hash in SUMMARY.
- Project-root resolution (`bin/lib/project-root.cjs`) walks up to a parent whose `sub_repos`
  claims the current directory.
- **The documented mode leaves `.planning/` uncommitted:** `commit_docs: false`, "planning stays
  local". A spec repo that is itself version-controlled is a variation, and GSD's support for it has
  **not been verified**.
- `references/worktree-path-safety.md` has a "registered-submodule allowance: sub_repos plans
  legitimately commit inside an immediate submodule of the pinned checkout". That suggests a spec
  repo with the code repo as a git submodule is anticipated. It has **not been tested**, and git
  worktrees combined with submodules are known to be awkward.

Variants within B to reason about:

- **B1:** the outer directory is not a git repo. `.planning/` has no history (GSD's documented
  mode).
- **B2:** the outer directory is a git repo and the code repo is a **submodule** of it. This matches
  GSD's worktree-safety allowance.
- **B3:** the outer directory is a git repo and the code repo is an **ignored nested repo**, not a
  submodule. Simpler git, but the outer repo records nothing about which code commit a plan
  targeted.

## Pros and Cons

| | A: today | B: separate spec repo |
|---|---|---|
| **Keeping `.planning/` out of `develop`** | Enforced by rules: a pattern check in `pre-push` plus a filtered copy of the branch. Fails late, at push time. | Built in: `.planning/` is not in the code repo at all. |
| **Scripts and hooks to maintain** | About 708 lines across 5 files. | Mostly gone; GSD routes commits to the right repo. |
| **Getting code to `develop`** | Filtered copy of the branch (new SHAs), then `sync-workspace.sh` merges `develop` back. | Phase branch forks from `develop` and merges back directly. |
| **DevFlow Ship, start to finish** | Can't: Ship would merge into the personal branch (999.110 open question). A′ would fix this. | Could, once DevFlow supports B. |
| **DevFlow support today** | Full. Its checks were built for this layout. | None. See "DevFlow coupling" below. |
| **Parallel phases** | Isolated: each phase branch carries its own copy of `.planning/`. | Shared: phases write one spec checkout, unless the spec repo also gets a branch per phase. |
| **Linking plans to code** | One commit history: any commit shows the plan that produced it. | By reference only: SUMMARY records commit hashes from each repo. B2 pins the code commit through the submodule pointer. |
| **`.planning/` history** | Version-controlled and pushed with the workspace branch. | None in B1. B2/B3 version it, but GSD support is unverified. |
| **GSD support** | Its standard single-repo setup. | Documented (`sub_repos`). Executor worktrees with a nested repo are untested here. |
| **Other users of DevFlow** | Needs a personal branch plus custom hooks; not something a new user gets by default. | Needs two repos; clearer boundary, but a bigger setup step (a natural job for `devflow init`). |

## DevFlow Coupling (what B would require)

DevFlow assumes `.planning/` lives on the phase's own branch and in its worktree. Found 2026-09-18:

- **Plan is commit-gated.** `crates/devflow-core/src/agent_result.rs:2504`
  (`matches!(stage, Stage::Plan | Stage::Code)`) counts commits on `feature/phase-N` ahead of the
  trunk in the code repo. Under B, Plan's commits land in the spec repo, so the count would be zero
  and Plan would read as "Failed — no work done".
- **CONTEXT.md on the base.** `phase_artifact_on_base` (`crates/devflow-cli/src/commands.rs:97`)
  runs `git ls-tree` for `.planning/phases/NN-*-CONTEXT.md` on the code repo's base branch before an
  auto-mode Define.
- **VERIFICATION.md evidence.** `select_loop_back_fix`
  (`crates/devflow-cli/src/pipeline_outcomes.rs:317`) reads `{N}-VERIFICATION.md` from the phase
  worktree (the "evidence root").
- **Unattended preflight.** `unattended_config_condition`
  (`crates/devflow-cli/src/preflight.rs:994`) requires `.planning/config.json` under the launch root.
- **Scale.** `.planning` appears in 26 Rust source files under `crates/`. That is a rough indicator,
  not an audit of which uses depend on this assumption.

Because DevFlow is the product, B is not only a change to the operator's workflow. Supporting a
two-repo layout would be a DevFlow feature for every user.

## Open Questions (for further reasoning)

1. **Is B's structural boundary worth DevFlow-wide changes, or does A′ capture most of the value?**
   A′ fixes Ship and removes the manual steps; B also removes the history rewriting and the push-time
   guard.
2. **Which B variant?** B1 gives up `.planning/` history. B2 (submodule) keeps a code-commit pin but
   brings submodule friction. B3 (ignored nested repo) is simpler but unpinned.
3. **Parallel phases under B.** Does each phase need its own spec branch and worktree alongside its
   code worktree (two worktrees per phase), or is a shared spec checkout acceptable given how GSD
   writes STATE.md and ROADMAP.md?
4. **Where does "environment" config live?** The operator's framing groups spec *and environment*
   (personal agent config such as `.claude/`, `CLAUDE.md`, `.mcp.json`, currently kept off shared
   branches by the same hooks). Does it move with `.planning/` into the spec repo?
5. **What `devflow.toml` holds.** One trunk (A as today), planning branch plus code trunk (A′), or
   spec-repo path plus code-repo path (B). This decides 999.110's schema, which should be able to
   express two locations from the start so it does not need migrating later.
6. **Tracking `devflow.toml` itself.** Under A/A′ it would name a personal branch, so it must not
   reach `develop`. Under B it can live in the spec repo.
7. **Migration cost for this repository.** Under B, moving `.planning/` history out of
   `workspace/denniyahh` (for example with `git subtree split` or `git filter-repo`), updating
   `scripts/phase-worktree.sh`, `CLAUDE.md` and the hooks, and keeping phase 48's in-flight worktree
   intact.

## How to Run (deferred — not run)

If and when the operator chooses to trial B, the cheapest experiment that crosses the seams:

1. In a scratch directory, create a spec repo with a copy of this repo's `.planning/`, plus a clone
   of the code repo as a child directory: once as a submodule (B2), once as an ignored nested repo
   (B3).
2. Set `planning.sub_repos` and `commit_docs: true`, then run one small `/gsd-plan-phase` +
   `/gsd-execute-phase` cycle on a throwaway phase.
3. Run one DevFlow stage (Plan) against it with a pre-built binary.

Do not run it in the main checkout or while a phase worktree is active.

## What to Expect

The trial answers three questions, each with a result that would count against B:

- **Does GSD commit `.planning/` to the outer repo?** Against B: GSD refuses or commits planning
  files into the child repo.
- **Do GSD's per-plan executor worktrees work with a nested code repo (B2 and B3)?** Against B: the
  worktree-safety pin fails, or executors commit to the wrong repo.
- **How many DevFlow checks fail?** Expected, per the coupling list: at least the Plan commit gate
  and the CONTEXT.md check. A failure outside the four listed coupling points would mean that list
  is incomplete.

## Investigation Trail

- 2026-09-18: 999.110 rescoped to the `init`/`doctor` initiative. The trunk semantics
  (`config.rs:103-111`) surfaced the personal-branch merge-target problem.
- 2026-09-18: the operator proposed separate spec/environment and code configs. Analysed the
  two-branches reading (does not resolve it) and the two-repos reading (resolves it, at a cost).
- 2026-09-18: found GSD's `sub_repos` support. Read `git-integration.md`, `planning-config.md`,
  `project-root.cjs`, `worktree-path-safety.md` and `gsd-executor.md`.
- 2026-09-18: verified option A's actual mechanism in `scripts/` and `scripts/hooks/`. It differs
  from the operator's description: `.planning/` is tracked and forked, not copied, and its commits
  are blocked at push, not at commit.
- 2026-09-18: the operator deferred any trial in order to reason further *(operator)*.

## Results

**PENDING — no trial run.** Desk analysis only, as of 2026-09-18. It supports the claims about
option A's mechanism and DevFlow's coupling points, which were read from the code. It does **not**
establish:

- whether GSD supports a version-controlled spec repo (B2/B3);
- whether executor worktrees work with a nested code repo;
- whether the coupling list above is complete.
