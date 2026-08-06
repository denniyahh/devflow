# Phase 26: Release-Cut Automation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-29
**Phase:** 26-release-cut-automation
**Areas discussed:** Automation ceiling, Failure/rollback semantics, Sync-step delivery (999.52), Signing-check fix scope (999.54/999.50)

---

## Automation ceiling

| Option | Description | Selected |
|--------|-------------|----------|
| Open PRs, human merges | Executor opens all three PRs, stops before each merge | |
| Merge everything via gh | Executor drives `gh pr merge` for all three PRs including main | |
| Merge low-risk, gate the rest | Auto-merge develop-bound PRs, gate main + publish | (superseded by user's own proposal below) |

**User's choice (free text, not from the options above):** Eliminate the PR requirement for merges to `develop` entirely — DevFlow merges to `develop` without human intervention via a GitHub ruleset bypass the operator will configure separately. `main` stays PR-gated. Future direction (not this phase): Claude reviews the develop→main PR instead of a human.

Follow-up questions and answers:
- Develop push mechanism: **Direct push** (not PR + auto-merge).
- Ruleset bypass setup: **Operator sets it up themselves**, out of phase scope.
- `--yes-release` flag scope: **One flag, whole sequence** (bump→tag→sync→publish), separate from `--yes-ship`.

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, in order | Executor runs `cargo publish` for both crates in dependency order | ✓ |
| No, stop before publish | Executor stops before publish, prints manual commands | |

**Notes:** Confirmed DevFlow's own `merge_feature_into_develop` (`git.rs:82-88`) never pushes today — purely local merge. This meant the operator's proposal wasn't blocked by any existing code; it's a repo-configuration change (ruleset bypass) plus new push code, not a code change to remove an existing restriction.

---

## Failure/rollback semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, same philosophy (fail-fast, no rollback) | Matches existing `hooks_after_ship` precedent | ✓ |
| No, something more | Automatic compensation for some steps | |

| Option | Description | Selected |
|--------|-------------|----------|
| Auto-skip completed steps | Re-run detects live git/registry state and resumes | ✓ |
| Always start fresh, fail loud | No auto-resume, manual diagnosis required | |

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, check first | Query crates.io before publish, idempotent | ✓ |
| No, just run cargo publish | Let cargo's own duplicate-version rejection be the signal | |

**Notes:** No follow-up needed — user confirmed and moved to next area.

---

## Sync-step delivery (999.52)

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, direct push | Sync merges via direct push, matching the develop-merge decision | ✓ |
| No, keep it a PR | Asymmetric handling vs. regular develop merges | |

| Option | Description | Selected |
|--------|-------------|----------|
| Standalone + internal | `devflow sync` exists as its own command AND is reused internally | ✓ |
| Internal only | Sync logic lives only inside the executor | |

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, keep the refusal | Byte-identical-tree check stays load-bearing, refuse on mismatch | ✓ |
| Something different | | |

**Notes:** Confirmed this decision resolves 999.52's original failure mode (human squash-merging the sync PR) by construction — no PR means no squash-vs-merge-commit button to click wrong.

---

## Signing-check fix scope (999.54/999.50)

| Option | Description | Selected |
|--------|-------------|----------|
| Precedence: devflow.releaseSigningKey first, fallback to user.signingkey | The backlog's own stated fix direction | User questioned the premise instead of answering directly |

**User's response (free text):** Questioned whether this kind of config-precedence logic belongs hardcoded in DevFlow at all, vs. git config / gh CLI / project context (CLAUDE.md). Stated the actual goal: Claude uses its own SSH key for ordinary commits; Claude should not be able to sign merges to main, by virtue of repo rules.

**Claude's response:** Clarified `devflow.releaseSigningKey` is an already-existing, already-documented (CONTRIBUTING.md) git-config convention, not new policy Phase 26 would invent. The actual bug (999.54) is that `release --check`'s predictor reads the wrong of two already-existing config values. Offered a follow-up choice: keep and fix the predictor, or drop prediction entirely and have the executor just run the real signed `git tag -s` command.

| Option | Description | Selected |
|--------|-------------|----------|
| Keep it, fix the predictor | release --check stays a useful pre-flight, fixed to read the right key | |
| Drop from Phase 26 | Executor runs the real tag command; release --check's bug stays in the backlog, untouched | ✓ (initial answer) |

**User's follow-up (free text):** Doesn't want to overengineer the release executor; the environment should already be configured idiomatically (Claude's key for ordinary commits, maintainer's key for releases/main) so DevFlow shouldn't need to check or know about this ahead of time at all. Asked whether there's a better solution.

**Claude's response:** Agreed dropping the predictor entirely is the stronger engineering answer, not just simpler — a predictor is a second implementation of "will signing work?" that must stay in sync with git's real behavior, which is the exact bug class 999.50/999.54 are about. Proposed: executor's tag step just runs CONTRIBUTING.md's already-documented explicit-key-selection `git tag -s` command and reads git's real result. The "Claude can't sign main" guarantee falls out of the automation-ceiling decision (Claude's unattended path stops at `develop`), not from any signing-check code.

**Final confirmation:** User confirmed dropping 999.54/999.50 entirely, AND removing both from the backlog (not just deferring) — stated no intention of ever implementing signing prediction in DevFlow. Asked to also drop "any other stories related to this functionality" (checked: none found beyond these two; 999.27, a different already-shipped classification bug from Phase 24, is unaffected).

---

## Capacity re-allocation (arose from dropping 999.54/999.50)

User asked to add more stories from the original "Phase 25 candidates" table given freed capacity.

| Option | Description | Selected |
|--------|-------------|----------|
| Add them anyway | Bundle 999.31/999.15/999.21 despite domain mismatch | |
| Same-domain items instead | 999.5 (changelog content) and 999.4 (concurrent-ship tag race) are actually in the release-mechanics domain | ✓ |
| New phase for the rest | Keep Phase 26 tight, stand up Phase 27 for the rest | |

**999.5 follow-up:** Confirmed reusing Phase 25's conventional-commit classification as the changelog content source (resolves the "no content source" reason it was deferred 3x).

**999.4 follow-up:** User didn't understand the race scenario; asked for clarification. Claude traced it to `devflow parallel` (`main.rs:147-158`, whole-phase concurrency) via direct code read. User confirmed they never use, and would never want, whole-phase concurrency for a single user, and questioned whether `devflow parallel` itself should be removed or repurposed for intra-phase workstream parallelization instead. Claude gave an opinion (real simplification vs. breaking-change risk, GSD's own wave-based execution may already cover the intra-phase use case) but declined to decide it inline — captured as a deferred idea for its own future phase. User confirmed 999.4 removed from Phase 26 and from the backlog entirely, given they'll never hit the race in practice.

---

## Claude's Discretion

- Exact CLI arg shape for `--execute`/`--yes-release` (new fields on `Command::Release` vs. a distinct variant).
- Where the direct-push code lives in `git.rs` (new method alongside `merge_feature_into_develop` vs. standalone function).
- Exact function signature for `devflow sync`'s standalone-vs-internal duality.
- Retry/backoff shape (if any) for the crates.io pre-publish check.
- Exact CONTRIBUTING.md wording changes reflecting partial automation of the 7-step checklist.

## Deferred Ideas

- **`devflow parallel`'s future** — remove whole-phase concurrency (closes 999.4/999.26 by deletion, but may be the first genuinely-breaking CLI change), repurpose for intra-phase workstream parallelization (possibly redundant with GSD's own wave-based execution), or leave alone. Needs its own phase with real investigation before any action.
- **`gh pr merge`-driven auto-merge for the develop→main release PR** — the operator's stated future direction (Claude reviewing that PR) implies eventual automation of the currently-manual main-merge step; not built in this phase.
- **999.31 (modular agent driver), 999.15 (hermetic shell-entrypoint tests), 999.21 (AI-acceptance wiring)** — considered for Phase 26's freed capacity, explicitly declined due to domain mismatch; remain open backlog candidates for a future phase.
