---
phase: 25-end-to-end-dogfood-blockers
plan: 04
subsystem: docs
tags: [contributing, roadmap, project-md, release-signing, versioning-policy]

# Dependency graph
requires: []
provides:
  - "CONTRIBUTING.md release procedure step 5 corrected to the maintainer-key signing form (25f)"
  - "ROADMAP.md June-2026 reorg bullet amended to record the D-06 lift instead of a live ban"
  - "PROJECT.md Constraints Versioning bullet rewritten to describe the D-06/D-07/D-11 scheme"
  - "ROADMAP.md Phase 25 Acceptance paragraph rewritten per D-15/D-16 to close on unit-level verification"
affects: [phase-25-plans, release-procedure, versioning-derivation]

# Tech tracking
tech-stack:
  added: []
  patterns: []

key-files:
  created: []
  modified:
    - CONTRIBUTING.md
    - .planning/ROADMAP.md
    - .planning/PROJECT.md

key-decisions:
  - "Step 5 now routes through the same devflow.releaseSigningKey indirection the signing-keys section already documents, cross-referenced by name so the two cannot drift apart again"
  - "The June-2026 versioning-ban bullet is amended in place (never deleted) to record the 2026-07-27 lift and the D-06 decision that superseded it"
  - "The Phase 25 Acceptance paragraph now closes the phase on unit-level verification of 25a-25f per D-15, with the single-run closure criterion explicitly marked reversible"

patterns-established: []

requirements-completed: ["25f"]

coverage:
  - id: D1
    description: "CONTRIBUTING.md release procedure step 5 directs the reader to the explicit devflow.releaseSigningKey form and drops the stale tag.gpgsign=false justification"
    requirement: "25f"
    verification:
      - kind: other
        ref: "rg -q 'devflow\\.releaseSigningKey' CONTRIBUTING.md && ! rg -q 'tag\\.gpgsign=false' CONTRIBUTING.md && ! rg -q 'user\\.signingkey[= ]+[/~]' CONTRIBUTING.md"
        status: pass
    human_judgment: true
    rationale: "Prose-correctness property (is the documented command followable end-to-end on a machine where the agent's key is user.signingkey) — no assertion can prove this; recorded below as a completed human-check."
  - id: D2
    description: "ROADMAP.md line 36 and PROJECT.md Constraints no longer ban commit-message-based versioning; both record the D-06 lift and describe the reachable-tag scheme"
    requirement: "25f"
    verification:
      - kind: other
        ref: "rg -c 'no commit-message-based versioning' .planning/ROADMAP.md .planning/PROJECT.md (0) && rg -c 'reachable' .planning/PROJECT.md (>=1) && rg -c 'version\\.rs' .planning/PROJECT.md (>=1)"
        status: pass
    human_judgment: true
    rationale: "Doc-consistency judgment (does the amended constraint read as forbidding what 25-01 implements) — recorded below as a completed human-check."
  - id: D3
    description: "ROADMAP.md Phase 25 Acceptance paragraph closes the phase on unit-level verification per D-15/D-16, not on a single unattended run"
    requirement: "25f"
    verification:
      - kind: other
        ref: "rg -c 'require-shipped' within the Phase 25 entry (0); git diff --stat -- .planning/ROADMAP.md (well under 60 lines); rg -c '^### Phase ' unchanged (50/50)"
        status: pass
    human_judgment: true
    rationale: "Doc-consistency judgment (would a verifier now mark a correctly-completed phase COMPLETE) — recorded below as a completed human-check."

duration: ~20min
completed: 2026-07-27
status: complete
---

# Phase 25 Plan 04: Documentation Drift Corrections Summary

**Three prose amendments closing 25f/D-06/D-16: CONTRIBUTING.md's release step 5 now signs with the maintainer key by explicit indirection, and both planning documents record the June-2026 versioning-ban lift and the D-15 acceptance-decoupling policy instead of contradicting Phase 25's locked design.**

## Performance

- **Duration:** ~20 min
- **Completed:** 2026-07-27
- **Tasks:** 2 (both single-commit)
- **Files modified:** 3 (`CONTRIBUTING.md`, `.planning/ROADMAP.md`, `.planning/PROJECT.md`)

## Accomplishments

- Fixed CONTRIBUTING.md's release procedure step 5, stale since PR #38: it now routes tag signing through `git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" tag -s …` — the same form the signing-keys section already documents — instead of a bare `git tag -s` that signs with the agent's key on a machine where the agent works. The stale "`tag.gpgsign=false` means `-a` alone will not sign" justification is replaced with the actual current risk: a tag signed with the wrong key that looks correct everywhere because both keys share `user.email`.
- Amended `.planning/ROADMAP.md`'s June-2026 reorg bullet (line 36) to record that the commit-message-versioning ban was lifted 2026-07-27 (D-06), rather than reading as a live prohibition Phase 25's 25c design would otherwise appear to violate. The bullet was amended in place, never deleted.
- Rewrote `.planning/PROJECT.md`'s Constraints Versioning bullet to describe what plan 25-01 actually implements: reachable-tag baseline plus conventional-commit intent, `Cargo.toml` as a derived output (D-11), and the gated major bump (D-09). The § Context milestone note reserving the 2.0.0 slot for a genuinely breaking change was left untouched, as required.
- Rewrote `.planning/ROADMAP.md`'s Phase 25 "Acceptance" paragraph per D-15/D-16: the phase now closes on unit-level verification of 25a-25f rather than a single unattended `devflow start … --require-shipped` run, since D-15 is a standing operator-confirmed policy change decoupling that run from any phase's completion. The paragraph states the suspension is deliberately reversible.

## Task Commits

Each task was committed atomically:

1. **Task 1: Correct CONTRIBUTING.md's release procedure step 5 (25f)** - `701a021` (docs)
2. **Task 2: Amend the two planning documents a locked decision would otherwise contradict (D-06, D-16)** - `a376fa0` (docs)

_Note: Both commits are `docs` type — this plan contains no code changes; `.gitconfig` was deliberately not modified since the drift was in the documentation, not the policy._

## Files Created/Modified

- `CONTRIBUTING.md` - Release procedure step 5 rewritten to the explicit maintainer-key signing form; stale justification removed
- `.planning/ROADMAP.md` - June-2026 reorg bullet (line 36) amended to record the D-06 lift; Phase 25 "Acceptance" paragraph rewritten per D-15/D-16
- `.planning/PROJECT.md` - Constraints Versioning bullet rewritten to describe the D-06/D-07/D-11 scheme

## Verification Detail — Pre/Post Change Counts

| Assertion | Pre-change | Post-change |
|---|---|---|
| `rg -c 'devflow\.releaseSigningKey' CONTRIBUTING.md` | 4 | 5 |
| `rg -c 'tag\.gpgsign=false' CONTRIBUTING.md` | 1 | 0 |
| `rg -c 'user\.signingkey[= ]+[/~]' CONTRIBUTING.md` (literal key path) | 0 | 0 |
| `rg -c 'no commit-message-based versioning' .planning/ROADMAP.md .planning/PROJECT.md` | 1 (ROADMAP only) | 0 |
| `rg -c '2026-07-27' .planning/ROADMAP.md` | 25 | 27 |
| June-2026 reorg list bullet count | 6 | 6 (unchanged) |
| `rg -c 'reachable' .planning/PROJECT.md` | 0 | 1 |
| `rg -c 'version\.rs' .planning/PROJECT.md` | 2 (one pre-existing in § Requirements, one in the Constraints bullet) | 2 (both lines still present; the Constraints occurrence rewritten in place) |
| `rg -c 'require-shipped' .planning/ROADMAP.md` (whole file) | 4 (1 in Phase 25's old Acceptance paragraph, 3 in Phase 23's unrelated entries) | 3 (all in Phase 23's entries; 0 within the Phase 25 entry, both before edit-adjacent Phase 23 uses and after) |
| `git diff --stat -- .planning/ROADMAP.md` | — | 1 file changed, 2 insertions(+), 2 deletions(-) |
| `rg -c '^### Phase ' .planning/ROADMAP.md` | 50 | 50 (unchanged) |
| `rg -n 'reserved for a' .planning/PROJECT.md` (milestone note) | present (line 105) | present, untouched |

Also confirmed: the amended `git -c user.signingkey="..." tag -s …` form is syntactically valid — a dry-run against a nonexistent key path (`/nonexistent/key`) reached git's signing step and failed only on "Couldn't load public key" (expected), with no tag object created and no other files touched.

## Human-Check Reviews (25-VALIDATION.md § Manual-Only Verifications, rows 1-3)

1. **CONTRIBUTING.md release step 5 against `.gitconfig`:** Confirmed. `.gitconfig` sets `[tag] gpgsign = true` and documents the exact two-key policy (`user.signingkey` = agent's key, `devflow.releaseSigningKey` = maintainer's). The rewritten step 5 names `devflow.releaseSigningKey` via the same `git config --get` indirection already used in the signing-keys section, and is followable end-to-end on a machine where the agent's key is `user.signingkey` — the command overrides `user.signingkey` inline via `-c` for that one invocation only.
2. **Rewritten Acceptance paragraph leads to a COMPLETE verdict on unit-level evidence alone:** Confirmed. The paragraph no longer requires `devflow evidence --require-shipped` to exit 0; it explicitly states the phase closes when 25a-25f are each verified on their own merits, and that a future unofficial end-to-end run's findings are filed to the backlog rather than gating closure.
3. **PROJECT.md's amended Constraints bullet does not read as forbidding 25-01's algorithm:** Confirmed. The bullet now describes the reachable-tag-baseline + conventional-commit-intent scheme by name and cites `version.rs`, D-06/D-07/D-09/D-11, and the 2026-07-27 lift date — it affirmatively describes rather than bans the algorithm.

## Decisions Made

- Cross-referenced CONTRIBUTING.md's step 5 to the signing-keys section by name (`[§ Release signing](#release-signing)`) rather than duplicating the explanation, so the two locations cannot drift apart again — matches the plan's explicit intent.
- Amended the June-2026 reorg bullet in place rather than deleting it, per the plan's explicit instruction that "an unexplained disappearance is worse than an amended record."
- Used `Edit` exclusively for all `.planning/ROADMAP.md` and `.planning/PROJECT.md` changes (never a wholesale `Write`), confirmed by the diff-stat structural assertion (4 lines changed in ROADMAP.md across two edits, 11 lines in PROJECT.md).

## Deviations from Plan

None - plan executed exactly as written. Both tasks matched their `<action>` blocks; all acceptance criteria and both `<verify>` blocks passed on the first attempt.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required. `.gitconfig` was deliberately not modified, since the drift being closed was in the documentation, not the signing policy itself.

## Next Phase Readiness

- 25f, D-06, and D-16 are closed. Plan 25-01 (25c derivation) and downstream Phase 25 plans can now proceed without their locked design reading as a contradiction of stale planning-doc prose.
- No blockers for the remaining Phase 25 waves.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-27*
