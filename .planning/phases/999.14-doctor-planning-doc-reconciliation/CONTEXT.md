---
status: backlog
source: found 2026-07-21 — STATE.md/ROADMAP.md claimed Phase 18 was "not yet merged to main / released" after it had been merged, tagged, and published as v1.5.0
---

# Backlog: Extend `devflow doctor` Reconciliation to Planning-Doc Staleness

## Problem

`devflow doctor`'s 18a reconciliation (shipped in v1.5.0) checks phase state
against the event log, live process IDs, open gates, and branch ancestry — but
nothing checks whether the *planning narrative itself* (`ROADMAP.md`, `STATE.md`)
still matches reality once a phase's outcome is decided by a manual, out-of-band
action (a merge, a tag, a crates.io publish).

This is not a one-off: it is the same class of bug `17-REVIEW.md` WR-06 already
named once — 19e/19f were found already closed by `17-13` but `ROADMAP.md` still
described them as open, and that staleness went unnoticed until an explicit audit.
Found again 2026-07-21: after v1.5.0 released, `STATE.md`/`ROADMAP.md` still said
Phase 18 was "Not yet merged to main / released," caught only because the operator
asked a status question and it was manually cross-checked against git.

## Proposed shape

Extend `doctor`'s reconciliation (or add a sibling check reusing its
`PhaseFacts`/`PhaseFinding` pattern from `crates/devflow-core`) to compare
`ROADMAP.md`'s `## Shipped` / `## Completed` table's version column against:

- The actual latest git tag reachable from `main` for phases claimed shipped.
- Optionally, the published crate version on crates.io (network-dependent, so
  likely opt-in/best-effort rather than a hard requirement).

A finding here would look like 18a's existing findings: read-only, reported in
`doctor`'s text/`--json` output, naming the specific stale claim and where it
lives, not auto-editing the docs.

## Notes

Deliberately scoped as detection only, not auto-correction — planning docs carry
human narrative and rationale alongside the factual claims (see how much context
`STATE.md`'s Decisions log carries), and auto-rewriting prose is a much larger and
riskier feature than flagging "this version claim doesn't match git anymore."

Promote with `/gsd-review-backlog` when ready.
