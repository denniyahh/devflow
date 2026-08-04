---
phase: 23-end-to-end-dogfood
plan: 17
artifact: acceptance-run-3
recorded: 2026-07-27T10:30:00Z
outcome: run-incomplete-stages-1-3-succeeded
supersedes: none (23-ACCEPTANCE-RUN-2.md records attempt 2; this is attempt 3, additive)
---

# Phase 23 — Acceptance Run Record (Round 3)

Target: Phase 24 (`release --check` signing-key inline classification).

**Verdict up front.** The run reached **Define → Plan → Code, all unattended, all
succeeding** — the furthest any DevFlow-driven run has ever gone in this project's
history — and then halted at the **Validate** stage boundary on a
`self_dogfood_stale_blocked` event. The halt was a **correct firing of D-18**, not
a defect and not a silent stall. The phase's literal acceptance criterion
(`devflow evidence --phase 24 --require-shipped` exiting 0) was **not** met and is
now permanently unmeetable for phase 24; see "Why the oracle is dead" below.

This is a **VALID RECORD** of an **INCOMPLETE acceptance attempt**, judged
separately from the record's own validity.

---

## 1. Preconditions, re-measured (not carried forward)

Every check below was re-run against the tree and binary at launch time.

**Primary checkout confirmed** — `git rev-parse --git-dir` and `--git-common-dir`
both resolve to `.git`, so this is not a linked worktree.

**The load-bearing precondition this attempt added.** `commands.rs:146` calls
`ensure_phase_reachable_on_base(project_root, phase, DEVELOP)` where `DEVELOP` is
the literal string `"develop"` (`config.rs:17`) — the **local** branch. The phase-24
worktree also forks from local `develop`. Local `develop` was 0 ahead / 21 behind
`origin/develop`, a strict ancestor:

```
$ git merge-base --is-ancestor develop origin/develop; echo $?
0
$ git rev-list --left-right --count develop...origin/develop
0	21
$ git fetch origin develop:develop
   0dad20d..9916e2f  develop -> develop
```

Applied via a non-checkout refspec, which git permits only for a fast-forward — an
independent structural confirmation, not this record's own arithmetic. `HEAD`
stayed on `feature/phase-23` and `git status --porcelain` was empty throughout.

Phase 24 confirmed reachable on the **local** ref after the fast-forward:

```
$ git show develop:.planning/ROADMAP.md | rg '^### Phase 24:'
### Phase 24: `release --check` Signing-Key Inline Classification
$ git ls-tree -r --name-only develop -- .planning/phases/ | rg '^\.planning/phases/24-'
.planning/phases/24-release-check-signing-key-inline-classification/.gitkeep
```

**Binary rebuilt.** The binary in place was `b5db079a…6dc98` — the pre-23g build
from attempt 2, i.e. the very binary whose staleness logic caused that attempt's
block. Rebuilt from `HEAD` = `b2b97ea` (clean tree), producing `262a4b9e…620320`.
The PATH entry is a **symlink** into `target/release/`, so the rebuild propagated
with no reinstall step; both hashes re-confirmed identical afterwards. The embedded
commit `b2b97ea…` was confirmed present in the new binary.

**Pre-run oracle baseline, labelled before launch:**

```
$ devflow evidence --phase 24 --require-shipped
... error: phase 24 has not shipped — DevFlow has no record of a completed Ship
EXIT=1
$ rg -c workflow_shipped .devflow/events.jsonl
0
```

**Gate chain:** `cargo test --workspace && cargo clippy --workspace --all-targets
-- -D warnings && cargo fmt --check` → **CHAIN_EXIT=0**, 611 passed / 0 failed
across 17 binaries (up from the 608 baseline; 23-16 added 3). Re-confirmed
independently inside the pinned devcontainer by the pre-push hook
(`check.sh: all OK`) when the recovery ref was pushed.

**Recovery ref:** `recovery/pre-23-17-acceptance-9916e2f` pushed to `origin` at
`9916e2f…`, read back and SHA-matched exactly. Deliberately remote-only — `devflow
cleanup` judges branches by ancestry and a recovery ref is always an ancestor of
what it protects (999.43 / `23-FINDINGS.md` §B2).

## 2. The staleness prediction, made before spending the launch

The binary would embed `b2b97ea` while the phase-24 worktree HEAD would be
`develop`'s tip `9916e2f`. Neither is an ancestor of the other — genuinely
divergent lineages, which is exactly the arm 23g added. Evaluated by hand first:

```
$ git merge-base --is-ancestor b2b97ea 9916e2f; echo $?   # 1
$ git merge-base --is-ancestor 9916e2f b2b97ea; echo $?   # 1  -> divergent
$ git diff --name-only b2b97ea 9916e2f
.planning/ROADMAP.md
.planning/STATE.md
.planning/phases/23-end-to-end-dogfood/23-16-SUMMARY.md
.planning/phases/23-end-to-end-dogfood/23-REVIEW.md
.planning/phases/23-end-to-end-dogfood/23-STALENESS-FIX-RECORD.md
.planning/phases/23-end-to-end-dogfood/23-VERIFICATION.md
```

Zero build-affecting paths → `ancestry_range_affects_build` false → **predicted
`Fresh`**. The launch confirmed it.

**This is the first live exercise of the 23g fix in a real run.** Attempt 2 died at
exactly this point with the pre-23g binary; attempt 3 passed it with the post-23g
binary, on a materially similar divergence. That contrast is the strongest
available evidence that 23-16's fix works in production, not just in unit tests.

## 3. Launch and stage progression

```
$ devflow start --phase 24 --agent claude --mode auto --yes-ship
created worktree: <repo-root>/.worktrees/phase-24 (branch feature/phase-24)
stage define → launched Claude Code (monitor pid <pid>)
started phase 24 in auto mode at 1785140875 — monitor will auto-advance
```

| Event | ts | Stage |
|---|---|---|
| `workflow_started` (`commit: b2b97ea`, `dirty: false`) | 1785140875 | — |
| `stage_launched` | 1785140875 | define |
| `transition` → plan | 1785141256 | define → plan (~6.4 min) |
| `transition` → code | 1785143069 | plan → code (~30 min) |
| `transition` → validate | 1785145280 | code → validate (~37 min) |
| `self_dogfood_stale_blocked` | 1785145280 | **validate — HALT** |

**No `self_dogfood_stale_blocked` at launch** — the contrast with attempt 2, whose
event trail is `workflow_started` immediately followed by the block.

**Deliverables verified on disk, not trusted from agent self-reports:**

- Define → `24-CONTEXT.md` (18 KB), `24-DISCUSSION-LOG.md` (9 KB), 2 commits
- Plan → `24-01-PLAN.md`, `24-02-PLAN.md` (waves 1 and 2), `24-RESEARCH.md`,
  `24-PATTERNS.md`; ROADMAP moved from `0 plans` to `2 plans`
- Code → 15 commits total, including `e5f69d3 feat(24-01): classify inline
  user.signingkey via git's own prefix rules`, its tests, both plan SUMMARYs,
  `24-REVIEW.md` (0 critical), `24-VERIFICATION.md`, `24-UAT.md`. Code stage exit
  code `0`.

## 4. The halt — correct behaviour, not a defect

At the Validate boundary the driven worktree had accumulated build-affecting
changes relative to the binary's embedded commit:

```
$ git diff --name-only b2b97ea 6dc6e09 | rg '\.rs$|Cargo\.(toml|lock)$'
crates/devflow-cli/tests/release_check.rs
crates/devflow-core/src/git.rs
```

Those are phase 24's own deliverable. `enforce_build_staleness` is called inside
`launch_stage` (`pipeline_launch.rs:93`), which **every** stage launch funnels
through — so the check re-ran at the transition and correctly classified the
binary as stale relative to the code it was about to validate. That is precisely
the Phase 16 false-evidence scenario D-18 exists to prevent.

**This was structurally guaranteed at scoping time.** Phase 24 was promoted as the
acceptance target for being "low-stakes *by consequence*" — a preflight advisory
touching no merge, version, or ship control flow. That criterion measured blast
radius, not self-modification. **Any** phase editing DevFlow's own compiled source
hits this at the first stage boundary after its code lands, regardless of blast
radius. Phase 24 was therefore never a viable unattended target, and no choice
made during this run could have recovered it.

**What the halt proves positively.** It is **not a silent stall** — the failure
mode Phase 23 was created to eliminate. The guard fired loudly, emitted a typed
event, left `state_present` intact with no open gate, and both the monitor and
agent exited cleanly. Phase 17's two silent monitor deaths cost ~4h each; this
halt was legible within seconds of inspection.

## 5. Resolution — why no rebuild, and why the oracle is dead

The guard's own message instructs a human to "rebuild and reinstall the binary
before resuming." There is **no automated self-rebuild path** in the codebase.

A mid-run rebuild was proposed and **rejected by the operator** on 2026-07-27:

> "I don't want unvalidated code to be used to rebuild the binary mid-run. Only
> validated and pushed code should ever be used. What if the code fails
> validation? Then faulty code was used by the binary. That doesn't seem smart."

That objection is correct and is now the design position recorded in **999.48 /
DEN-73**: separate **driver** from **subject**. The driver's trustworthiness comes
from provenance, not from matching the worktree it drives; the subject is validated
by `cargo test` compiled from source. Rebuilding mid-run would have made phase 24's
unvalidated change partly responsible for certifying itself.

It would also have bought nothing: `check_ssh_signing_viability` has exactly one
caller (`git.rs:831`) and sits on no pipeline path — `hooks_after_ship` is
Merge → VersionBump → ChangelogAppend → BranchCleanup, and neither Validate nor
Ship invokes `release --check`. All risk, no benefit.

**Why the oracle is now permanently unmeetable.** Phase 24 was completed manually
(GSD invoked directly) and merged via PR #34 into `develop` at `40fce19`. Its
deliverables now exist, so re-running `devflow start --phase 24` would find each
stage's output already present and no-op straight through. `workflow_shipped` for
phase 24 can never be emitted. The criterion is not "still failing" — it is
**closed off**, and the reason is filed as DEN-73 rather than left implicit.

## 6. Version defect — predicted, and avoided rather than exercised

Re-measured at `9916e2f` before launch: `compute_version` would yield major=1
(Cargo.toml), minor=11 (raw tag count, including the non-version
`archive-planning-docs-2026-07-24`), patch=359 (distance from `v1.4.0`, which
`git describe` prefers over `v1.8.1` because this repo's main/develop sync-merge
topology makes the older tag "nearer") ⇒ **≈ 1.11.359**.

Because the run never reached Ship and phase 24 shipped by PR,
`hooks_after_ship` never ran and this was **not** exercised. It remains fully
latent and will fire on the first DevFlow-driven ship that ever succeeds. Filed as
**999.49 / DEN-74**.

## 7. Tree-state changes made by this attempt

Recorded explicitly rather than left as silent side effects:

1. Local `develop` fast-forwarded `0dad20d` → `9916e2f` (pure FF, 0 ahead / 21
   behind, applied by non-checkout refspec).
2. `target/release/devflow` rebuilt `b5db079a…` → `262a4b9e…`; PATH symlink
   followed automatically.
3. `recovery/pre-23-17-acceptance-9916e2f` pushed to `origin` (remote-only).
4. Worktree `<repo-root>/.worktrees/phase-24` and branch `feature/phase-24`
   created by `devflow start`; phase 24 subsequently completed and merged via
   PR #34.

## 8. Redaction check

```
$ rg -n '/home/denniyahh|/var/home/denniyahh' 23-ACCEPTANCE-RUN-3.md
(no match)
```

Absolute operator paths are redacted to `<repo-root>`. The `github.com/denniyahh`
URL form is retained per the precedent set in `23-GUARD-SHIP-RECORD.md`: this
operator's OS username and public GitHub account are the same string, the
repository is public, and prior committed artifacts already retain it. The 999.10
leak class this check targets is local filesystem paths, none of which appear.
