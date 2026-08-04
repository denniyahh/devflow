# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: gsd-hygiene — GSD Workflow Hygiene

**Shipped:** 2026-08-04 (unversioned — pure `.planning/` documentation, no crates code, nothing
published to crates.io)
**Phases:** 1 | **Plans:** 1 (backfilled) | **Sessions:** 1

### What Was Built
- Confirmed `.planning/ROADMAP.md`'s layout — the active milestone's `### Phase N:` heading and
  `## Progress` table living inside its own heading-to-next-heading window — was already correct,
  closing backlog 999.72/999.72a (`roadmap.analyze` phase_count:0 misfire; `milestone.complete
  --dry-run`'s pass-all degrade).
- Closed the loop: flipped 999.72/999.72a's backlog status labels, added a CLAUDE.md convention
  documenting the two root causes for future milestone boundaries, and independently re-verified
  the phase goal via a standalone `gsd-verifier` run rather than trusting the prior claim.
- Resolved an unrelated open item found during close (`stale-blockers-gate-gsd-next` debug
  session) — its STATE.md fix was already live from an earlier session, and its coupled
  "isComplete two-scale bug" concern is now genuinely resolved as a side effect of this
  milestone's own fix.

### What Worked
- **Live re-verification instead of trusting a prior session's claim.** STATE.md asserted the
  ROADMAP fix was "verified live" before this session started; re-running the same three checks
  independently (not just reading the claim) is what surfaced that the milestone-close workflow's
  own readiness gate would still misfire for a different reason (issue 18 below) — trusting the
  first claim would have missed it.
- **`--dry-run` before every write operation that archives or restructures.** `milestone.complete
  --dry-run` correctly previewed the exact scope (1 phase) before the real run touched anything;
  used the same discipline for `phase.complete` and `roadmap.analyze` throughout.

### What Was Inefficient
- **Two `git mv`/rename operations left their "delete" half uncommitted.** The `commit` query
  wrapper, when passed only the new path, staged the add but left the pre-staged deletion of the
  old path sitting uncommitted in the working tree — twice (the debug-session file move, and the
  phase-directory archival). Caught both times by checking `git status` after, not by the wrapper
  itself. Worth remembering for any future `git mv` in this project: verify `git status` is clean
  after, don't assume the wrapper captured the whole rename.
- **A tooling gap (gsd-core issue 18) required a workaround mid-close.** Discovering that
  `init.manager`'s `phase_complete` predicate hardcodes `false` for any zero-plan phase — and that
  `/gsd-complete-milestone`'s readiness gate depends on exactly that field — cost real
  investigation time mid-session rather than being known going in. The workaround (an
  explicitly-labeled backfilled PLAN/SUMMARY pair) was operator-approved rather than silently
  applied, which was the right call, but a milestone with more zero-plan phases would hit this
  every time.

### Patterns Established
- **Backfilled artifacts must be labeled as backfilled, in their own body, not just in a commit
  message.** Both `32-01-PLAN.md` and `32-01-SUMMARY.md` open with an explicit "this is backfilled,
  not prospective" statement — matches the precedent Phase 22's backfilled SUMMARY/VERIFICATION
  set, and keeps provenance legible to any future reader who doesn't have this session's context.
- **Unversioned milestones use a plain label, not a fabricated version.** `gsd-hygiene` archived to
  `.planning/milestones/gsd-hygiene-ROADMAP.md` (not `v0.0.0` or similar), and skipped the
  `git tag` step entirely — an operator decision, made explicit rather than assumed, when
  `/gsd-complete-milestone`'s own contract expected a real `v[X.Y]`.

### Key Lessons
1. A tool's own success on a `--dry-run` or a downstream command (here, `phase.complete`
   succeeding) does not guarantee every OTHER code path that reads the same state agrees — verify
   the specific field the next workflow step actually gates on (`init.manager`'s
   `phase_complete`), not just a nearby command's exit code.
2. When a milestone-close workflow assumes shipped-and-versioned by default, an intentionally
   unversioned/docs-only milestone needs its own explicit handling for every version-shaped field
   (archive filename, git tag) rather than silently substituting a placeholder.
3. `audit-open`'s "1 open item" can be stale bookkeeping, not a live blocker — but the fix is to
   verify that directly (re-run the underlying check, read the actual file) and update the
   session's own status frontmatter, not to just re-acknowledge-and-defer indefinitely.

### Cost Observations
- Model mix: not tracked this session
- Sessions: 1 (interactive, `/gsd-discuss-phase 32` through `/gsd-complete-milestone`)
- Notable: this milestone's entire scope was closed within a single session because the
  underlying fix had already landed — the actual work was verification, bookkeeping, and closing
  two independently-discovered gaps (the debug session and the new gsd-core issue), not
  implementation.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| gsd-hygiene | 1 | 1 | First milestone closed by an interactive session end-to-end same-day; first use of a backfilled PLAN/SUMMARY pair to satisfy a completion-projection tooling gap; first unversioned/plain-label milestone archive |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| gsd-hygiene | N/A — no crates code | N/A | 0 |

### Top Lessons (Verified Across Milestones)

1. Live re-verification of a claimed-fixed condition, rather than trusting the claim, is what
   this project's own history keeps rewarding — repeated across the v2.3.0 close (`milestone.complete`
   bypassed after catching a live over-sweep) and this milestone's close (issue 18 found the same
   way).
