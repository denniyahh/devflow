---
status: backlog
source: TEST-SUITE-QA-REVIEW.md (Codex, 2026-07-21), P1 recommendation — initial scope re-prioritized by Claude same day
---

# Backlog: Mutation Testing (`cargo-mutants`)

## Goal

Introduce `cargo-mutants` as a scheduled or manual gate (not blocking every
PR — mutation testing rebuilds and retests per mutant and is too slow for a
required PR check on this codebase's size). Track surviving mutants rather
than treating line coverage as the primary quality signal.

## Initial scope (2026-07-21, Claude review — re-prioritized from the QA review's list)

1. **`verify.rs`** first — this session's own review found a real fail-open
   bug here (empty external-verification commands silently passing), making it
   the highest-confidence-return target of anything in the codebase.
2. **`outcome_policy::decide_action`** — small, pure, exhaustively matched,
   gates whether a stage can silently advance. A surviving mutant here (e.g.
   flipping `Advance`/`GateReview` for one outcome) is close to worst-case.
3. **`agent_result.rs`'s Layer 0–3 evaluators** — the actual completion-
   evaluation logic; the most safety-critical code in the project.
4. Git safety logic (`git.rs`'s tag/`commit_path` functions) — 999.11 (`commit_path`
   empty commits) is exactly the class of bug mutation testing could catch
   proactively.

**Deliberately excluded from initial scope:** `main.rs`'s display/formatting/CLI
dispatch code. Lower stakes, and `cargo-mutants`' noise-to-signal ratio there
would bury findings from the modules above.

## Acceptable surviving mutants

Mutants in `Display`/`Debug` impls, log/error message text, and other
non-decision-making output are acceptable to leave unaddressed. Not acceptable:
any surviving mutant in `decide_action`, `evaluate_layer0`–`evaluate_layer3`,
`parse_external_verification_approval`, `command_from_frontmatter`, or any
git-mutating function's guard conditions.

## Notes

Promote with `/gsd-review-backlog` when ready.
