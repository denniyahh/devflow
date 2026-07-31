# Upstream GSD Issues

Defects in GSD core (`@opengsd/gsd-core`, installed at `~/.claude/gsd-core/`) found while
dogfooding DevFlow. **Not DevFlow defects** — DevFlow can only work around them. Filed here
because this repo has no GSD upstream remote configured; each entry is written to be pasted
into a GSD repo issue as-is.

Status legend: `READY` = written up, not yet filed · `FILED` = filed upstream, link recorded.

---

## 1. `ship.md` `track_shipping` pushes `[ci skip]`, wedging any PR with required status checks

**Status:** READY — not yet filed
**Found:** 2026-07-28, DevFlow phase 25 ship (`denniyahh/devflow` PR #47)
**RECURRED:** 2026-07-31, DevFlow phase 28 ship (`denniyahh/devflow` PR #63) — identical
symptom, identical cause, ~3 days later. See "Recurrence record" below.
**Component:** `gsd-core/workflows/ship.md`, step `track_shipping`
**Severity:** high — makes `/gsd-ship` produce an unmergeable PR on any repo with required checks.
**Reproducibility: confirmed 2/2.** This is not an intermittent or environment-specific fault; it
fires every time `/gsd-ship` runs to completion against a repo with required checks.

### What happens

`track_shipping` commits the ship note and pushes it onto the PR branch:

```bash
gsd_run query commit "docs(${padded_phase}): ship phase ${PHASE_NUMBER} — PR #${PR_NUMBER} [ci skip]" --files .planning/STATE.md
git push origin ${CURRENT_BRANCH}
```

The `[ci skip]` trailer is deliberate — the workflow's own comment says it "suppresses the
redundant pipeline the push would otherwise trigger."

The problem is that this push makes the ship note the **PR head commit**. On a repository with
required status checks, the head commit then has zero checks, and none will ever arrive, because
CI was told to skip. GitHub reports:

```
mergeable:         MERGEABLE
mergeStateStatus:  BLOCKED
statusCheckRollup: []
```

The PR cannot merge. `/gsd-ship` reports success and hands back a wedged PR.

### Reproduction

1. A repo whose default branch requires one or more status checks (classic branch protection
   *or* a repository ruleset — see the detection note below).
2. Run `/gsd-ship <phase>` to completion.
3. `gh pr view <n> --json mergeStateStatus` → `BLOCKED`; `gh pr checks <n>` → "no checks reported".

Observed on `denniyahh/devflow` PR #47, ruleset `develop-merge-or-squash`, required contexts
`Test`, `Clippy`, `Format`, `Build + test in devcontainer`.

### Why the obvious recovery does not work

Closing and reopening the PR does **not** re-fire the checks, even though both workflows declare
`on: pull_request: branches: [main, develop]` and `reopened` is in the default event set. Verified:
after close+reopen, `gh run list` still showed every run pinned to the pre-ship-note SHA. The only
reliable recovery is a new head commit that does not carry the skip token.

### Suggested fixes (any one is sufficient)

1. **Order the ship note before PR creation.** Commit and push `STATE.md` in `push_branch`, before
   `create_pr`, so the ship note is never the head commit. Cleanest — no skip token needed at all.
2. **Drop the skip token when required checks exist.** Detect required checks (both mechanisms) and
   omit `[ci skip]` when any are present. The "redundant pipeline" it saves is cheaper than a
   wedged PR.
3. **Warn and self-heal.** Keep the token, then after pushing check
   `gh pr view --json mergeStateStatus`; if `BLOCKED` with an empty `statusCheckRollup`, push an
   empty commit without the token and say so.

### Required-check detection is itself a trap (worth documenting alongside the fix)

`gh api repos/OWNER/REPO/branches/BRANCH/protection` returns **no** `required_status_checks` field
when the requirement comes from a repository **ruleset** rather than classic branch protection.
Both must be queried:

```bash
gh api repos/OWNER/REPO/branches/BRANCH/protection      # classic
gh api repos/OWNER/REPO/rulesets                        # rulesets
gh api repos/OWNER/REPO/rulesets/<id>                   # ...then read rules[].type == "required_status_checks"
```

DevFlow's own `.github/workflows/devcontainer.yml` header documents this same trap after a
deleted workflow silently wedged every merge to `develop`.

### Related footgun found while recovering

`[ci skip]` is matched **anywhere in the commit message**, not only the subject. An empty commit
whose body *explained* the problem — and therefore quoted the token — suppressed CI again. If the
fix keeps any skip-token logic, a guard is worth adding:

```bash
git log -1 --format='%B' | grep -qE '\[(ci skip|skip ci)\]' && echo "refusing: message contains a CI skip token"
```

### Recurrence record — 2026-07-31, phase 28, PR #63

Second confirmed occurrence, three days after the first write-up. Same workflow step, same token,
same outcome. Evidence captured this time:

| Commit | Message | Check runs on that SHA |
|---|---|---|
| `0feb477` | `docs(28): mark phase 28 complete …` | **8** |
| `d62b8de` | `docs(28): ship phase 28 — PR #63 [ci skip]` | **0** |

`gh pr view 63` immediately after `/gsd-ship` reported `mergeable: MERGEABLE`,
`mergeStateStatus: BLOCKED`, `statusCheckRollup: []` — the wedged state described above,
reproduced exactly.

**Recovery used (a fourth option, cheaper than the three suggested above when the ship note is
already pushed):** amend the ship-note commit to drop the token and force-push with lease.

```bash
git commit --amend -m "docs(NN): ship phase N — PR #M"   # same content, token removed
git push --force-with-lease origin <feature-branch>
```

CI then ran on the new head (`3823ee8`) and `mergeStateStatus` went `BLOCKED` → `CLEAN`. Safe here
because the branch had a single author and was not yet reviewed; it would not be safe on a branch
others have pulled.

**What the recurrence tells us that the first occurrence did not:** writing the issue down did not
prevent it. The entry existed, was accurate, and was read by nobody at the moment `/gsd-ship` ran —
because nothing in the workflow consults it. Until this is filed and fixed upstream, the only
durable mitigation is a **local guard**, not a document (see "Preventing recurrence" at the end of
this file).

---

## Also observed this session — not yet written up

Same category (GSD core, found while dogfooding), recorded so the evidence is not lost. Each needs
its own write-up before filing.

### 2. `api-coverage.verify-pre` fires on negated prose

`gsd-tools check api-coverage.verify-pre` blocked `/gsd-verify-work 25` reporting "external-API
integration detected without a coverage matrix". The triggering text was `25-01-PLAN.md:105`:

> "This phase integrates no external API, SDK or hosted service."

The compound verb+noun detector (`gsd-core/bin/lib/api-coverage.cjs`, `detectApiIntegration`) has
no negation handling, so a sentence explicitly denying API integration satisfies it. The gate is
`blocking: true, onError: halt`, so a false positive halts verification and the documented remedy
is to author a `COVERAGE.md` enumerating an API surface that does not exist.

### 3. `check predicate` implements no predicate kinds

The capability registry declares the security ship gate as:

```json
{"kind": "artifact-frontmatter-equals", "artifact": "SECURITY.md", "field": "threats_open", "equals": 0}
```

Invoking it directly fails:

```
Error: gate predicate evaluation failed: Unknown predicate kind:
"artifact-frontmatter-equals". Known kinds: command-exit-zero
```

`command-exit-zero` appears to be a bare fallback string with no implementation behind it. The gate
still enforces correctly only because `ship.md` step 6 reads the frontmatter directly in-context
rather than going through `check predicate` — so the declared mechanism and the enforcing mechanism
are different code, and only one works. Fails closed (`onError: halt`), so not exploitable, but the
declaration is decorative.

### 4. `phase.complete` and `state.update` advance into backlog headings

Both wrote `current_phase: 999.1 / BACKLOG` into `STATE.md` after phase 25 completed, treating a
`999.x` backlog heading as the next sequential phase. Corrected twice manually in one session.
DevFlow's own `STATE.md` history log records the identical bug being caught after phase 20, so this
is a recurrence, not a one-off. Backlog items are supposed to require `/gsd-review-backlog`
promotion.

### 5. `broken-windows` capability description overstates enforcement

The capability's top-level `description` says it "Blocks `/gsd-ship` while any window is open",
with no qualifier. `WINDOWS.md`'s generated header says the same. Only the `workflow.windows_enforce`
knob description is accurate: the gate is **opt-in and off by default**; tracking is on, enforcement
is not. Two of three documentation surfaces assert a guarantee the default configuration does not
provide — which misled this session into believing the ledger was gating a ship it never gated.

### 6. `query commit` will commit onto a protected integration branch with no guard

`gsd_run query commit "<msg>" --files <paths>` commits to whatever branch the working tree is
currently on, with no check against the project's own declared branch model. Observed twice in one
session on 2026-07-30: `/gsd-discuss-phase 27`'s `git_commit` and `update_state` steps ran
`query commit` while the main checkout sat on `develop`, landing `docs(27): capture phase context`
and `docs(state): record phase 27 context session` directly onto the integration branch. Caught
before push only because the branch was checked manually; recovered with `git branch` + `git reset
--hard origin/develop`.

`develop` on this repository is protected server-side (`develop-merge-or-squash`,
`enforcement: active`, empty bypass list), so the push would have been rejected — but that is
GitHub catching it, not GSD. On a repo without a ruleset, or for any workflow step that pushes
after committing, this lands silently.

GSD already knows the branch model it should be respecting: `.planning/config.json` carries
`git.main`, `git.develop`, and `git.feature_prefix`, and `gsd-tools` reads that file for other
purposes. The fix is to have `query commit` refuse (or warn loudly) when `HEAD` is on
`config.git.main` or `config.git.develop`, naming the branch and suggesting a feature branch —
matching the fail-loud posture the rest of the toolchain uses.

**Note this is specifically a GSD-side gap, not DevFlow's.** DevFlow's own production commit
sites (`hooks::docs_update`'s `commit_all`, `hooks::changelog_append` and `hooks::version_bump`'s
`commit_path`) commit to `develop` *deliberately*, in the terminal Ship batch after `Merge` has
already put the main checkout there — that is the designed behavior, and a blanket protected-branch
refusal would break it. `devflow start --no-worktree` likewise calls `GitFlow::feature_start` and
checks out `feature/phase-NN` before any agent runs. The unguarded path is GSD's alone.

#### RECURRED 2026-07-31 — phase 28, at far larger scale

Third and fourth occurrences, and the worst so far: **all 55 phase-28 commits landed directly on
`develop`** — every plan commit, every executor worktree merge, every tracking update, across the
entire phase. Caught only at ship time, when a PR *to* `develop` proved impossible because the work
was already on it. Recovered with `git branch feature/phase-28` + `git branch -f develop
origin/develop` (nothing lost — every commit was preserved on the new branch), then shipped as
PR #63.

**Root cause is broader than `query commit`, and this is the important correction to entry 6's
original diagnosis.** `query commit` is only the proximate mechanism. The actual reason nothing ever
left `develop` is that GSD's **`git.branching_strategy` is unset**, which resolves to `none`:

```
$ gsd-tools query config-get git.branching_strategy   →  (unset)
$ gsd-tools query init.execute-phase 28               →  "branching_strategy": "none"
```

(Note the key is `git.branching_strategy`. A top-level `branching_strategy` is rejected as an
unknown key — worth stating explicitly, because the init JSON reports the resolved value under the
bare name `branching_strategy`, which invites setting the wrong key and silently changing nothing.)

`execute-phase.md`'s `handle_branching` step then reads, in full:

> **"none":** Skip, continue on current branch.

So the phase ran to completion on whatever branch happened to be checked out — `develop`. No step
in plan-phase, execute-phase, or verify-phase ever creates a branch under this setting, and none
warns that it is committing to an integration branch. `/gsd-ship`'s preflight *does* warn ("If on
`${BASE_BRANCH}`: warn — should be on a feature branch"), but that fires at the very end, after all
55 commits already exist.

**`.planning/config.json` already declares the intended model and GSD ignores it:**

```json
"git": { "main": "main", "develop": "develop", "feature_prefix": "feature/", "auto_branch": true }
```

`auto_branch: true` and `feature_prefix: "feature/"` are DevFlow's keys, consumed by DevFlow's own
`GitFlow::feature_start` when `devflow start` drives a phase. GSD reads neither — it looks only at
the top-level `branching_strategy`, which is absent. The project therefore *declares* auto-branching
and *gets* none, with no diagnostic anywhere.

**Suggested upstream fixes, in order of preference:**

1. **Change the default.** `branching_strategy` unset should default to `phase`, not `none`.
   Committing a multi-plan phase onto an integration branch is never the safe default.
2. **Warn at the start, not at ship.** `execute-phase.md`'s `handle_branching` should emit a visible
   warning when strategy is `none` *and* `HEAD` is on `config.git.main`/`config.git.develop` — the
   same condition `/gsd-ship` already checks, moved to where it is still cheap to act on.
3. **Honor the declared model.** When `branching_strategy` is unset but `config.git.feature_prefix`
   /`auto_branch` are present, either adopt them or say plainly that they are being ignored.

---

---

## Preventing recurrence — the meta-finding (2026-07-31)

Two entries in this file (**1** and **6**) recurred in phase 28, three days after being written up
here accurately and in detail. Nothing about the write-ups was wrong. They simply had no effect,
because **a document is not a control**: no workflow step reads this file, and the failure modes
both occur inside automated steps that run without a human in the loop.

The same lesson phase 28 itself produced, in a different register: 776 passing tests did not catch a
broken feature, because every test asserted a prediction rather than an observation. Here, an
accurate issue log did not catch a repeat defect, because logging is not enforcement.

**Concrete local guards worth adding (none require upstream changes):**

| Risk | Guard |
|---|---|
| Phase commits landing on `develop` | Set `git.branching_strategy: "phase"` (**not** top-level `branching_strategy` — that key is rejected) so `execute-phase` creates a branch off `origin/HEAD` before any plan runs. **Applied 2026-07-31**, together with `git.phase_branch_template: "feature/phase-{phase}"` to match this repo's own convention; verified `init.execute-phase` now resolves `branch_name: feature/phase-28`. |
| Same, as a backstop | A `pre-commit` hook that refuses a commit touching `.planning/phases/**` while `HEAD` is on `main`/`develop` |
| `[ci skip]` wedging a PR | After `/gsd-ship`, assert `gh pr view <n> --json mergeStateStatus` is not `BLOCKED` with an empty rollup; if it is, amend the ship note and force-push with lease |

**Filing status: none of these six entries has been filed upstream.** That is the single highest-
leverage action remaining — entries 1 and 6 are now each confirmed reproducible (2/2 and 4/4
respectively), which is the evidence an upstream maintainer needs.

---

*Created 2026-07-28 during DevFlow phase 25. Updated 2026-07-31 during phase 28 with recurrence
records for entries 1 and 6, and the "Preventing recurrence" section. Update `Status:` and record
the issue link when each entry is filed upstream.*
