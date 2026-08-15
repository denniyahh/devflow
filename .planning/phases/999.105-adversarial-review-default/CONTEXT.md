---
status: backlog
source: Phase 36 dogfood, 2026-08-15 — hand-run adversarial review (claude/codex/antigravity)
---

# Backlog: Adversarial Cross-Model Review as a Default Phase Gate

> Filed after a hand-run adversarial review caught, in a single phase (36), three CRITICAL and two
> HIGH findings that would otherwise have shipped. The review was run against both the docs
> (SPEC/CONTEXT) and the plan (PLAN.md), and both passes paid for themselves.

## Goal

Make a cross-model adversarial review of a phase's CONTEXT (post-discuss) and PLAN (post-plan) a
default, gated part of the phase lifecycle — not an operator's ad-hoc step.

## What the Phase 36 run demonstrated

- The **plan pass** caught that 999.104 targeted `release_finish`, which has no production caller —
  the real release tag is hand-cut by `scripts/cut-release.sh` (which already had the deterministic
  override). Hardening dead code while deleting the live guard was the exact failure a default gate
  would have prevented.
- It caught `DEVFLOW_PI_PROVIDER` being treated as a credential (it is a provider *name*; codex
  proved `pi auth check` returns `credentials_not_configured` even with it set) — a false-green.
- It caught `--approve` as unsandboxed-code-execution risk (Pi has no sandbox; `--approve` trusts
  project-local extensions).
- The **doc pass** caught the SPEC/CONTEXT authority contradiction and the "end-to-end" claim that
  the code couldn't satisfy.

## Two failure modes this item must pin (learned live)

1. **Reviewer CWD is an input.** Launching reviewers from the wrong checkout handed them a stale
   SPEC. The procedure must resolve the review root (the phase worktree) and launch each reviewer
   from it, and sanity-check citations against that root. (Encoded in `adversarial-review` SKILL.md.)
2. **`codex` anchors to `~/.codex/config.toml`, not CWD.** Its project dir is config-pinned; a
   reviewer can read a different checkout even when launched from the right CWD. The procedure must
   pass `--cd`/`--add-dir` explicitly and verify citations.

## Scope (to be decided in discuss-phase)

- **Trigger:** discuss:post (CONTEXT) and plan:post (PLAN) — a gate before execute.
- **Reviewers:** the `adversarial-review` skill (`~/.agents/skills/adversarial-review/`), defaulting
  to a diverse 2-3 reviewer set (e.g. claude/codex/antigravity).
- **Feedback loop:** findings land in `REVIEWS.md` / `reviews/` and feed `gsd-plan-phase --reviews`
  (or a devflow-native equivalent); blocking findings gate execute.
- **Relationship to `gsd-review`:** `gsd-review` is plan-only and CLI-level; this item is the
  model/effort-aware, CWD-disciplined promotion into the default flow.

## References

- `~/.agents/skills/adversarial-review/SKILL.md` — the reusable review procedure (model/effort
  selection, review-root/CWD discipline, citation verification, consolidation).
- `.planning/reviews/plan/SUMMARY.md` (Phase 36 worktree) — the concrete findings that motivated
  this item.
- `.planning/UPSTREAM-GSD-ISSUES.md` — if the promotion is wired as GSD lifecycle hooks, upstream
  coordination may be needed.
