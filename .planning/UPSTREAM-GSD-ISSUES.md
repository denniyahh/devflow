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
**Component:** `gsd-core/workflows/ship.md`, step `track_shipping`
**Severity:** high — makes `/gsd-ship` produce an unmergeable PR on any repo with required checks

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

---

*Created 2026-07-28 during DevFlow phase 25. Update `Status:` and record the issue link when each
entry is filed upstream.*
