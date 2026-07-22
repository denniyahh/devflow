---
phase: 19-release-integrity-main-rs-decomposition
plan: 04
subsystem: testing
tags: [gsd-skills, code-review, test-quality, contributing-docs]

# Dependency graph
requires: []
provides:
  - ".claude/skills/ai-change-acceptance/ project skill (SKILL.md + 2 rules files)"
  - "CONTRIBUTING.md § AI Change Acceptance section"
  - ".gitignore fix so .claude/skills/** is tracked while other .claude/ runtime state stays ignored"
affects: [19-05, gsd-code-review]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Project-scoped GSD skill directory (.claude/skills/<name>/SKILL.md + rules/*.md) as the project-scoped extension point for /gsd-code-review, instead of net-new lint/CI tooling"

key-files:
  created:
    - .claude/skills/ai-change-acceptance/SKILL.md
    - .claude/skills/ai-change-acceptance/rules/change-acceptance.md
    - .claude/skills/ai-change-acceptance/rules/test-signal-rejection.md
  modified:
    - CONTRIBUTING.md
    - .gitignore

key-decisions:
  - "Fixed .gitignore's blanket `.claude/` ignore (Rule 2): without a carve-out, the new skill would never be committed, silently defeating the plan's core truth that the contract 'exists in this repository.' Mirrored the existing `.codex/*` + negation pattern already in the file."
  - "Used ReviewerSetTestAdapter (deleted commit 4e8bf9c) as the required real worked example for rejection pattern 2, and additionally sourced decide_action_is_deterministic (same commit) for pattern 3 and the still-live devflow_test_clippy_matches_ci_scope for pattern 4 — all three are real in-repo instances rather than synthetic examples."
  - "Pattern 1 (assert-only-constants) has no in-repo instance found by history search; used a labeled synthetic example per the plan's fallback instruction."

patterns-established:
  - "GSD project-skill discovery pattern established for this repo for the first time: .claude/skills/<name>/SKILL.md as a lightweight index (~130 lines max) with rules/*.md loaded on demand — future project skills should follow this same two-tier structure."

requirements-completed: [19g]

coverage:
  - id: D1
    description: "ai-change-acceptance project skill created (SKILL.md index + change-acceptance.md + test-signal-rejection.md), all five D-19 requirements and four rejection patterns present with reviewer check + failure signature, grounded in at least one real in-repo worked example"
    requirement: "19g"
    verification:
      - kind: unit
        ref: "rg -c '^#{2,3} ' .claude/skills/ai-change-acceptance/rules/change-acceptance.md == 5"
        status: pass
      - kind: unit
        ref: "rg -c '^#{2,3} ' .claude/skills/ai-change-acceptance/rules/test-signal-rejection.md == 5 (4 patterns + boundary section)"
        status: pass
      - kind: unit
        ref: "rg -c 'ReviewerSetTestAdapter' .claude/skills/ai-change-acceptance/rules/test-signal-rejection.md >= 1"
        status: pass
      - kind: unit
        ref: "wc -l .claude/skills/ai-change-acceptance/SKILL.md == 58 (within 40-140)"
        status: pass
    human_judgment: false
  - id: D2
    description: "CONTRIBUTING.md states the contract in prose, placed between PR Process and Cutting a Release, names /gsd-code-review as enforcement, points at the skill as source of truth"
    requirement: "19g"
    verification:
      - kind: unit
        ref: "rg -n '^## (PR Process|AI Change Acceptance|Cutting a Release)' CONTRIBUTING.md -> strictly increasing line numbers"
        status: pass
      - kind: unit
        ref: "git diff CONTRIBUTING.md shows only one inserted section + one inserted cross-reference line, no other section touched"
        status: pass
    human_judgment: false
  - id: D3
    description: "Zero Rust source touched, zero out-of-repo files modified, no new lint/CI/script/dependency"
    requirement: "19g"
    verification:
      - kind: unit
        ref: "git status --porcelain -- crates/ (empty)"
        status: pass
      - kind: unit
        ref: "git status --porcelain -- .github/ Cargo.toml Cargo.lock (empty)"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 04: AI Change Acceptance Contract (19g) Summary

**New `.claude/skills/ai-change-acceptance/` project skill plus a `CONTRIBUTING.md` section codify the five D-19 acceptance requirements and four rejection patterns as review criteria for `/gsd-code-review` — zero new tooling, grounded in this repo's own deleted-test history.**

## Performance

- **Duration:** 20 min
- **Started:** 2026-07-22T00:48:00Z (approx)
- **Completed:** 2026-07-22T01:08:32Z
- **Tasks:** 2
- **Files modified:** 5 (3 created, 2 modified)

## Accomplishments

- Created the first `.claude/skills/` directory in this repository: `ai-change-acceptance` with a 58-line `SKILL.md` index and two `rules/*.md` files carrying the full detail.
- All five D-19 acceptance requirements written as assertion + reviewer check + failure signature, including this project's two documented false-green traps (`cargo test --exact` matching nothing yet exiting 0; `-p devflow-cli` not running the CLI's own tests) and the confirmed `cargo test --lib` dead end from STATE.md's 18-01 entry.
- All four D-19 rejection patterns written with recognition guidance and worked examples — three drawn from this repository's real history (`ReviewerSetTestAdapter`, `decide_action_is_deterministic`, both deleted in commit `4e8bf9c`; `devflow_test_clippy_matches_ci_scope`, still live in `devcontainer_ci_failfast.rs`), one labeled synthetic (assert-only-constants, no in-repo instance found).
- `CONTRIBUTING.md` now has a `## AI Change Acceptance` section between `## PR Process` and `## Cutting a Release`, summarizing the contract for human contributors, naming `/gsd-code-review` as where enforcement lives, and pointing at the skill as the single source of truth ("skill wins" on disagreement) — plus one cross-reference line added to `### Testing notes`.
- Found and fixed a repository bug that would have silently defeated the plan's objective: `.gitignore` blanket-ignored `.claude/` (line 18), which would have prevented the new skill from ever being committed. Carved out `!.claude/skills/**`, mirroring the existing `.codex/*` negation pattern in the same file.

## Task Commits

Each task was committed atomically:

1. **Task 1: Create the `ai-change-acceptance` project skill** — `dc37488` (feat) — includes the `.gitignore` fix as a bundled Rule 2 deviation, since the skill files and the fix that makes them trackable are one indivisible change.
2. **Task 2: State the contract in `CONTRIBUTING.md` prose** — `04998cb` (docs)

**Plan metadata:** (pending — this SUMMARY's commit)

## Files Created/Modified

- `.claude/skills/ai-change-acceptance/SKILL.md` - lightweight index: when the skill applies, one-line summaries of all 9 items, pointers to `rules/`
- `.claude/skills/ai-change-acceptance/rules/change-acceptance.md` - the five D-19 requirements, each with assertion/check/failure-signature, plus the two false-green traps
- `.claude/skills/ai-change-acceptance/rules/test-signal-rejection.md` - the four D-19 rejection patterns with worked examples, plus a closing section on the source-grep-vs-source-property boundary
- `CONTRIBUTING.md` - new `## AI Change Acceptance` section + one cross-reference line in `### Testing notes`
- `.gitignore` - `.claude/` blanket ignore converted to `.claude/*` + `!.claude/skills/**` carve-out

## Decisions Made

- Fixed the `.gitignore` gap immediately rather than deferring it, since without it the entire plan's deliverable would be invisible to git and to every other contributor/clone — this is exactly the "exists in this repository" truth the plan's first must-have asserts. Documented as a Rule 2 (missing critical functionality) deviation.
- Selected `ReviewerSetTestAdapter` as the mandated worked example for rejection pattern 2 (reproduces/invents production algorithm in the test), and additionally sourced two more real examples the plan did not mandate by name (`decide_action_is_deterministic` for pattern 3, `devflow_test_clippy_matches_ci_scope` for pattern 4) by searching this repo's git history (`git log -S`) for the exact deleted test bodies, so three of the four patterns are grounded in real code rather than synthetic illustrations.
- Left pattern 1 (assert-only-constants) as a labeled synthetic example after confirming via history search that no such test exists in this repository's history — consistent with the plan's explicit fallback instruction.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical Functionality] Fixed `.gitignore` blanket-ignoring `.claude/`**
- **Found during:** Task 1 (creating the skill directory)
- **Issue:** `.gitignore:18` ignored `.claude/` entirely (comment: "Agent runtimes — local state, not project code"). This is correct for `settings.local.json`, `worktrees/`, and `scheduled_tasks.lock`, but it also silently swallowed the new `.claude/skills/ai-change-acceptance/` — `git status --porcelain` showed nothing for the new skill files, meaning they would never be committed and the contract would exist only on this one machine, contradicting the plan's must-have truth that the skill "exists in this repository for the first time" for any GSD agent to discover.
- **Fix:** Converted `.claude/` to `.claude/*` plus `!.claude/skills/` and `!.claude/skills/**`, mirroring the `.codex/*` negation pattern already present in the same file for `agents/`, `skills/`, and `prompts/`. Verified `.claude/settings.local.json`, `.claude/worktrees`, and `.claude/scheduled_tasks.lock` are still ignored (`git check-ignore -v`) while `.claude/skills/ai-change-acceptance/SKILL.md` is not.
- **Files modified:** `.gitignore`
- **Verification:** `git check-ignore -v` confirmed the split; `git status --porcelain` then showed exactly `?? .claude/` (the new skill dir) and ` M .gitignore` before staging.
- **Committed in:** `dc37488` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 2 - missing critical functionality)
**Impact on plan:** Essential — without this fix the plan's entire deliverable would silently fail to be tracked by git, defeating its stated purpose. No scope creep: the fix is a two-line `.gitignore` change confined to the existing agent-runtime-ignore block, using a pattern the file already establishes for a sibling tool (`.codex/`).

## Issues Encountered

None beyond the `.gitignore` deviation documented above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 19g is complete and independent of the `main.rs` split waves (D-21 zero source overlap held: `git status --porcelain -- crates/` was empty throughout).
- Plan `19-05` (per its stated dependency on this plan) can now run its dogfood acceptance test: running `/gsd-code-review` against a deliberately non-compliant test-only diff and confirming the new skill causes it to be flagged. That check is explicitly out of scope for this plan and owned by 19-05.

---
*Phase: 19-release-integrity-main-rs-decomposition*
*Completed: 2026-07-22*

## Self-Check: PASSED

All claimed files found on disk (`.claude/skills/ai-change-acceptance/SKILL.md`,
`rules/change-acceptance.md`, `rules/test-signal-rejection.md`, `CONTRIBUTING.md`,
`.gitignore`, this SUMMARY). Both task commits (`dc37488`, `04998cb`) verified
present in `git log --oneline --all`.
