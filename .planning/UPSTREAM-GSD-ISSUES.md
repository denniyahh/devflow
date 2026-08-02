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

## 7. No way to express "`Agent` exists, but my session will not outlive this turn"

**Status:** READY — not yet filed
**Type: compatibility gap / feature request**, not a defect report. GSD's behavior here is correct
under the runtime it targets; this asks for a distinction it currently cannot make.
**Found:** 2026-07-31, while running GSD under DevFlow (`denniyahh/devflow` phase 29)
**Component:** `gsd-core/workflows/execute-phase.md` (`:24-26`, secondary `:18`)
**Severity:** medium — costs a wave of executor work per occurrence in affected runtimes, but only
in runtimes GSD does not currently claim to support.

### The ask, in one sentence

`execute-phase` decides whether to spawn subagents by testing **tool availability**; the property
that actually matters is **session survivability**, and there is no way to express the difference.

### Why availability is the wrong predicate

`:24-26` states the rule:

> **Other runtimes:** If `Agent`/`agent` tool is genuinely unavailable (e.g. a backgrounded Claude
> Code agent per #853, or a non-Claude runtime), use sequential inline execution as the fallback for
> executor parallelization only. If `Agent` IS available (top-level Claude Code), you MUST spawn
> gsd-executor agents — inline execution is not authorized. **Check for actual tool availability,
> not runtime name.**

Under a **non-interactive one-shot launch** (`claude -p "<prompt>"`), `Agent` *is* available — so
this rule mandates spawning. But in that launch mode **the agent's turn ending terminates the
process**. For waves with 2+ plans, `:596-599` correctly prescribes `run_in_background: true` (to
serialize `git worktree add` against `.git/config.lock`), and a backgrounded executor's completion
notification is then delivered to a session that no longer exists.

The distinction is already half-present — #853's "backgrounded Claude Code agent" is exactly a
session-lifetime concern — it is just keyed off whether the tool exists rather than whether the
session will still be there to receive a result.

### Repro (contributed as evidence; the failure itself is the host's fault, not GSD's)

DevFlow drives every GSD stage as one-shot `claude -p`. Phase 29: 7 plans across 6 waves, with
wave 1 = 1 plan, **wave 2 = 2 plans**, waves 3–6 = 1 plan each.

| Wave | Plans | Path | Outcome |
|---|---|---|---|
| 1 | 1 | below the 2+ threshold | merged normally |
| 2 | 2 | **2+ → `run_in_background: true`** | orphaned |

The orchestrator's final message was verbatim *"Wave 2 is running — two executors in isolated
worktrees, plus a backup completion watcher. I'll pick up when they return."* — then the process
exited (`stop_reason: end_turn`). The two executors completed **5 commits** on `worktree-agent-*`
branches and neither wrote its `SUMMARY.md`, having been killed before that step. Wave 2 was the
only multi-plan wave in the phase and the only one that failed.

**To be explicit about ownership:** the lost work is the *host's* fault. DevFlow chose a launch
model that kills the session at turn end and then scored the stage successful anyway. GSD is not
responsible for that, and the host-side fix is tracked separately. What GSD could offer is the
means for a host like this to opt into the safe path.

### Why the existing fallback cannot cover it

`:31-34` — *"If a spawned agent completes its work but the orchestrator never receives the completion
signal, treat it as successful based on spot-checks and continue. Never block indefinitely — always
verify via filesystem and git state."*

This is the right instinct and is unreachable here: it presumes the orchestrator is **alive** to
spot-check. Under one-shot launch it is not.

### Suggested shapes (any one would close the gap)

1. **An explicit opt-out** — a config key (e.g. `execution.session_outlives_turn: false`) or a
   documented env var that forces `run_in_background: false` and/or sequential inline execution,
   independent of tool availability.
2. **Serialize what actually races.** The stated rationale for backgrounding is `git worktree add`
   contending on `.git/config.lock` — which argues for serializing *worktree creation*, then
   dispatching synchronously. That would make the background path unnecessary for this case entirely.
3. **Orchestrator-owned `SUMMARY.md`.** Today the requirement is an instruction to an executor that
   may be killed before honoring it; writing it from the orchestrator after collection makes the
   partial-work state recoverable rather than ambiguous.

### Secondary, minor — a stale statement worth correcting either way

`:18` states *"**Claude Code:** Uses `Agent(...)` — blocks until complete, returns result."* Current
Claude Code runs subagents **in background by default** (`run_in_background: false` is the explicit
opt-out), so this is no longer accurate even for single-agent waves. It loses nothing on its own,
but it is the basis on which the workflow concludes Claude Code is safe to spawn into.

### Local workaround

`"parallelization": false` — serializes within a wave so the 2+ branch is never reached. Costs
parallelism, and does not address subagents backgrounding by default.


## 8. `query commit --files <path>` silently drops any absolute path, because it double-joins it onto `cwd`

**Status:** READY — not yet filed
**Found:** 2026-07-25, DevFlow phase 23 planning (`23-VALIDATION.md`, `23-PATTERNS.md`)
**RECURRED:** 2026-08-02, DevFlow phase 30 planning (`30-VALIDATION.md`) — same call shape,
confirmed and root-caused this time instead of just observed. See "Root cause" below.
**Component:** `gsd-core/bin/lib/commands.cjs`, `cmdCommit` (`:664-693`)
**Severity:** high — the command reports a normal-looking `{"committed": false, "reason":
"nothing_to_commit"}` on exit 0 for a file that plainly exists and plainly has changes; nothing
distinguishes this from the legitimate "nothing changed" case, so a caller that doesn't diff
`git status` afterward believes the commit succeeded.
**Reproducibility: confirmed 2/2**, and the mechanism is unconditional in the source — not a race
or an environment quirk.

### What happens

`plan-phase.md` (and other workflow steps) resolve `PHASE_DIR` from `init.plan-phase`'s JSON,
which returns an **absolute path**:

```json
"phase_dir": "/var/home/denniyahh/Github/devflow/.planning/phases/30-keep-the-session-alive-past-turn-end"
```

Every workflow step that commits a freshly-written phase artifact naturally builds its `--files`
argument from that variable, e.g. step 5.5:

```bash
gsd_run query commit "docs(phase-${PHASE}): add validation strategy" --files "${PHASE_DIR}/${PADDED_PHASE}-VALIDATION.md"
```

`cmdCommit` then does, unconditionally:

```js
const fullPath = node_path_1.default.join(cwd, file);   // commands.cjs:668
if (!node_fs_1.default.existsSync(fullPath)) {           // commands.cjs:669
    if (explicitFiles) { continue; }                      // silently skipped, no warning
    ...
}
```

`path.join` does **not** special-case an absolute second argument — it concatenates and
normalizes. When `file` is already absolute and equals (or is under) `cwd`, the result is `cwd`
duplicated onto itself, which never exists on disk:

```
$ node -e "console.log(require('path').join(
    '/var/home/denniyahh/Github/devflow',
    '/var/home/denniyahh/Github/devflow/.planning/phases/30-.../30-VALIDATION.md'))"
/var/home/denniyahh/Github/devflow/var/home/denniyahh/Github/devflow/.planning/phases/30-.../30-VALIDATION.md
```

`existsSync` on that doubled path is `false`, so the file is treated exactly like a caller-declared
file that doesn't exist — `continue`, no `git add`, nothing pushed to `stagedPaths`. Every file in
the `--files` list can fail this way independently; if all of them do, `stagedPaths.length === 0`
and `cmdCommit` returns `{"committed": false, "hash": null, "reason": "nothing_to_commit"}`
(`commands.cjs:692-694`) — indistinguishable in shape from "there were genuinely no changes."

### Root cause is a real path resolution bug, not specifically an "untracked files" issue

An earlier write-up of this symptom (project-local notes, not filed) attributed it to untracked
files specifically — that was a mis-diagnosis by correlation, corrected here. The doubling happens
purely from the absolute-path collision; it does not consult git's index at all, and would equally
silently drop an **already-tracked** file if `--files` were passed its absolute path (unverified
live, since no reproduction case surfaced with a tracked absolute path, but the code path is
unconditional and has no track-status branch before the `existsSync` check).

**The codebase already has the correct idiom elsewhere in the same file**, just not applied here:

```js
// commands.cjs:306, a different function
const fullPath = node_path_1.default.isAbsolute(targetPath) ? targetPath : node_path_1.default.join(cwd, targetPath);
```

### Reproduction

```bash
cd <repo root>          # cwd == repo root, the normal orchestrator launch condition
touch .planning/phases/NN-slug/NN-NEWFILE.md
gsd-tools query commit "docs: test" --files "$(pwd)/.planning/phases/NN-slug/NN-NEWFILE.md"
# → {"committed": false, "hash": null, "reason": "nothing_to_commit"}
git status --porcelain .planning/phases/NN-slug/NN-NEWFILE.md
# → ?? .planning/phases/NN-slug/NN-NEWFILE.md   (still untracked — commit never touched it)
```

### Suggested fixes (any one is sufficient)

1. **Apply the existing idiom.** Change `commands.cjs:668` to
   `node_path_1.default.isAbsolute(file) ? file : node_path_1.default.join(cwd, file)`, matching
   `commands.cjs:306`. Minimal, one-line, no behavior change for the common relative-path case.
2. **Fail loud instead of silent-skip.** If `explicitFiles` and a declared file resolves to a path
   that doesn't exist, emit a warning naming the exact resolved path checked — would have surfaced
   the doubled path immediately instead of reading as "no changes."
3. **Workflow-side normalization.** `plan-phase.md` and other workflow files could relativize
   `PHASE_DIR`-based paths before passing them to `--files`, but this only hides the underlying bug
   for GSD's own call sites — any other host or hand-invocation with an absolute path still breaks.

### Local workaround

After any `query commit` call whose `--files` argument could be absolute, check
`git status --porcelain <path>` — if the file still shows as untracked/modified, fall back to
`git add <path> && git commit -m "<msg>"` directly.

---

## 9. `state.planned-phase` silently rewrites unrelated frontmatter via a body→frontmatter resync that has no preserve-guard for `status` or `last_activity_desc`

**Status:** READY — not yet filed
**Found:** 2026-07-31, DevFlow phase 29 planning (`/gsd-plan-phase 29`)
**RECURRED:** 2026-08-02, DevFlow phase 30 planning (`/gsd-plan-phase 30`) — identical symptom,
identical stray text (`last_activity_desc` overwritten with the exact same stale string on both
occasions, three days apart, for two different phase numbers). Root-caused this time.
**Component:** `gsd-core/bin/lib/state-transition.cjs` (`plannedPhaseCore`, `:764-826`),
`gsd-core/bin/lib/state.cjs` (`syncStateFrontmatter`, `:1581-1660`; `buildStateFrontmatter`,
`:1353-1365`), `gsd-core/bin/lib/state-document.cjs` (`normalizeStateStatus`, `:112-134`)
**Severity:** high — every `/gsd-plan-phase` run silently destroys the frontmatter completion
record of whatever phase most recently finished, and reports only `{"updated": ["Status"]}`,
massively under-reporting the actual blast radius.
**Reproducibility: confirmed 2/2**, with the exact same corrupted value both times.

### What happens

After planning completes, plan-phase.md step 13b runs:

```bash
gsd_run query state.planned-phase --phase "${PHASE_NUMBER}" --name "${PHASE_NAME}" --plans "${PLAN_COUNT}"
```

This reported `{"updated": ["Status"], "phase": "30", "plan_count": 5}` on 2026-08-02 — implying a
single, narrow body-field change. The actual `git diff` on `.planning/STATE.md` immediately after:

```diff
-status: shipped — PR #63 open to develop
+status: executing
-last_updated: "2026-07-31T08:35:00.000Z"
+last_updated: "2026-08-02T11:50:33.571Z"
-last_activity: 2026-07-30
+last_activity: 2026-07-30
-last_activity_desc: "Phase 28 complete: 6/6 plans, 779 tests green, SECURED (threats_open 0), ..."
+last_activity_desc: "Phase 28 execution started"
-  total_phases: 17
+  total_phases: 21
-  total_plans: 124
+  total_plans: 129
```

Five frontmatter fields changed; one was reported. `status` and `last_activity_desc` are the
damaging ones — both replace an accurate, hand/executor-authored completion record with stale or
wrong text, and nothing in the tool's own output signals that this happened.

**Identical recurrence, three days and one phase-number apart:** planning phase 29 on 2026-07-31
produced `last_activity_desc: "Phase 28 execution started"` in frontmatter. Planning phase 30 on
2026-08-02 produced the **exact same string**, byte for byte, even though the intent object in
both calls carried a different `phaseNumber`/`planCount`. This is the strongest evidence that the
value is not being freshly derived from the current call's intent at all — it's a stale value found
somewhere else in the document and copied forward unchanged, twice.

### Root cause — a two-part mechanism, both parts confirmed against live source and live document state

**Part 1 — `plannedPhaseCore` targets body fields that partially don't exist in this project's
`STATE.md` shape.** It calls `stateReplaceField` (unconditional) targeting a field literally named
`'Last Activity Description'` (`state-transition.cjs:817`). DevFlow's `STATE.md` has no such field
— `grep -n '^Last Activity Description' STATE.md` returns nothing. It only has a combined prose
line:

```
Last activity: 2026-07-30 — Phase 28 execution started      (STATE.md:160)
```

`stateReplaceField` finds no match, returns null, and the replace is a silent no-op — consistent
with `plannedPhaseCore`'s own `updated` array never including `'Last Activity Description'` in
either observed run. Separately, `plannedPhaseCore` calls `stateReplaceFieldIfTemplate` for
`'Last Activity'` (`:809`), which is **template-aware**: it only replaces the field when the
existing value matches a known placeholder default. `"2026-07-30 — Phase 28 execution started"` is
real content, not a placeholder, so this replace also no-ops and the line is left untouched —
which is exactly why it is still reading a stale 2026-07-30 date and description on 2026-08-02.

**Part 2 — the wrapping `readModifyWriteStateMd` re-derives frontmatter from the body on every
write, and two fields have no preserve-guard.** After the body transform, `readModifyWriteStateMd`
(`state.cjs:2002`) always calls `syncStateFrontmatter(modified, cwd)`, which calls
`buildStateFrontmatter(body, cwd)` to compute fresh frontmatter values purely from body content,
then selectively falls back to the pre-existing frontmatter value when the derived one looks wrong
— but only for a specific allowlist:

```js
// state.cjs:1590 — only guard for status, and only the 'unknown' case:
if (derivedFm['status'] === 'unknown' && existingFm['status'] && existingFm['status'] !== 'unknown') {
    derivedFm['status'] = existingFm['status'];
}
// stopped_at, paused_at, current_phase, current_phase_name, current_plan,
// progress (if fully absent), milestone/milestone_name all have their own
// explicit "prefer existing when derived is empty" guards (:1631-1660).
// last_activity_desc has NO such guard anywhere in this file.
```

Since `buildStateFrontmatter` re-derives `lastActivityDesc` straight from the stale body line
(`state.cjs:1365`: `stateExtractField(bodyContent, 'Last Activity Description') ??
proseLastActivity.description` — the second branch fires, extracting `"Phase 28 execution
started"` from the untouched prose line), and there is no guard protecting it, the frontmatter's
detailed, accurate completion description is unconditionally overwritten by that stale fragment —
every single time `readModifyWriteStateMd` runs and the no-op-detection allows the write through
(which it does here, because the *`Status`* body field, a separate line under `## Current
Position`, genuinely did change).

**The `status` corruption has a second, independent contributing bug: a substring conflation in
`normalizeStateStatus`.** `plannedPhaseCore` deliberately sets the body's `## Current Position`
`Status:` line to the literal string `"Ready to execute"` (`:801-804`, meaning "planning just
finished, nothing is running yet"). `buildStateFrontmatter` extracts that string and normalizes it
via `normalizeStateStatus` (`state-document.cjs:112`):

```js
else if (statusLower.includes('executing') || statusLower.includes('in progress')) {
    normalizedStatus = 'executing';
}
...
else if (statusLower.includes('ready to execute')) {
    normalizedStatus = 'executing';        // state-document.cjs:130-132
}
```

`"ready to execute"` and `"executing"` are opposite states — one means *nothing has started*, the
other means *mid-run* — but the substring-based classifier folds them into the same normalized
value, so a phase that just finished planning gets stamped into frontmatter as `status: executing`,
overwriting a correct `status: shipped — PR #63 open to develop` for a *different, already-shipped*
phase.

### Why this is worse than it looks

The intent-level `updated` report (`{"updated": ["Status"]}`) reflects only what `plannedPhaseCore`
itself changed on the **body**. It has no visibility into what `syncStateFrontmatter` changes on
the **frontmatter** afterward as a side effect of the same write — so the report is not merely
incomplete, it is structurally incapable of describing the actual damage, because the two layers
that make the change don't share an accounting mechanism.

### Suggested fixes (any one materially helps; 1+2 together close it)

1. **Add a preserve-guard for `last_activity_desc`**, matching the existing pattern for
   `stopped_at`/`paused_at`/etc. (`state.cjs:1631-1660`): if the derived value looks identical to
   what a stale, unrelated body line would produce (or simply: prefer existing frontmatter when the
   *specific* transition being applied — `plannedPhase` — never actually touched the description
   field), don't overwrite it.
2. **Fix the `normalizeStateStatus` conflation.** `"ready to execute"` should map to a distinct
   normalized status (e.g. `'planned'` or `'ready'`), not collapse into `'executing'`. This is a
   one-line, low-risk change (`state-document.cjs:130-132`) that removes an actively misleading
   state transition.
3. **Make `plannedPhaseCore` and `syncStateFrontmatter` share one accounting.** Either have
   `readModifyWriteStateMd` report every field the *resync* changed (not just the caller's own
   transform), or have `plannedPhaseCore` write directly to frontmatter for the fields it owns
   (`status`, `last_activity_desc`) instead of relying on body→frontmatter re-derivation to infer
   them indirectly.
4. **Give `plannedPhaseCore` a real `'Last Activity Description'` body target**, or update
   `buildStateFrontmatter`'s prose fallback to not silently treat a years-old unrelated line as
   "the latest activity" when a more specific transition (like `plannedPhase`) is what triggered the
   write.

### Local workaround

After `query state.planned-phase` (or any `readModifyWriteStateMd`-wrapped verb), diff
`.planning/STATE.md`'s frontmatter block specifically — not just the reported `updated` array — and
hand-restore `status` / `last_activity_desc` in the same commit as any legitimate changes from that
call. Both recorded occurrences were caught and fixed this way with no data loss (git preserves the
pre-corruption value), but it must be checked every time; the tool's own report cannot be trusted to
surface it.

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

**Filing status: none of these nine entries has been filed upstream.** That is the single highest-
leverage action remaining — entries 1, 6, 8, and 9 are now each confirmed reproducible (2/2, 4/4,
2/2, 2/2 respectively), which is the evidence an upstream maintainer needs.

---

*Created 2026-07-28 during DevFlow phase 25. Updated 2026-07-31 during phase 28 with recurrence
records for entries 1 and 6, and the "Preventing recurrence" section. Updated 2026-08-02 during
phase 30 planning with entries 8 (`query commit` double-joins absolute `--files` paths) and 9
(`state.planned-phase` frontmatter resync has no preserve-guard for `status`/`last_activity_desc`),
both root-caused against live source and both confirmed 2/2 recurring. Update `Status:` and record
the issue link when each entry is filed upstream.*
