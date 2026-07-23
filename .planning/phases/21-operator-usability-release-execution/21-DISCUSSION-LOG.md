# Phase 21: Operator Usability & Release Execution — Discussion Log

**Date:** 2026-07-23
**Mode:** Autonomous (DevFlow-driven, non-interactive) — no interactive gray-area
selection was possible, so decisions were made from recommended defaults and are
marked in CONTEXT.md as "to validate at plan time."

> Human-reference record only. Not consumed by researcher/planner/executor.

## Starting condition

- ROADMAP goal for Phase 21 was `[To be planned]` (scaffold commit `56a1835`,
  "goals TBD pending discuss/plan"). REQUIREMENTS.md empty; no `/gsd-review-backlog`
  promotion had run; no SPEC.md. Phase dir held only `.gitkeep`.
- The one Phase-21-specific design artifact was `999.28`'s CONTEXT, filed the same
  day from "the phase-21 dogfood-launch design discussion."

## Scope derivation

Boundary was proposed from the phase **name** ("Operator Usability & Release
Execution") plus the backlog candidates matching it, with concurrency-flavored
items fenced out to Phase 22.

| Area | Decision | Source |
|---|---|---|
| Unit set | 21a discoverability (999.3), 21b `--base` (999.28), 21c release executor (999.25); 999.5 optional | phase name + backlog match |
| Out to Phase 22 | 999.4, 999.26, 999.2, `--base`-in-`parallel` | concurrency, not single-writer |
| Out to Phase 23 | 999.15/17/18/19/20/22 | test/CI |

## Decisions captured

- **21b `--base`:** explicit flag, default `develop`, never infer from current
  branch (D-01); default path stays develop-rooted (D-02); ship/merge target for
  a stacked base left as an open one-way question for the planner (D-03);
  reject-missing / warn-non-ancestor validation (D-04); `start`-only scope (D-05).
- **21c release executor:** run `release --check` first and hard-stop (D-06);
  explicit operator gate before irreversible publish (D-07); reuse after-ship
  hook machinery, not a second version path (D-08); encode core-then-cli publish
  order (D-09); planner must design partial-failure rollback semantics (D-10).
- **21a discoverability:** additive UX only, sequence first (D-11).

## Deferred / redirected

- Concurrency governance items redirected to Phase 22; test/CI to Phase 23;
  999.5 changelog content carried as optional-only.

## Open questions handed to research/planning

- Ship/merge target for a phase based on an unmerged predecessor (D-03).
- Partial-failure rollback semantics for the release executor: tag-lands-publish-
  fails, core-publishes-cli-does-not (D-10).
- Whether 999.5 is folded in at all (capacity-dependent).
- Confirm the proposed unit set — it did not go through `/gsd-review-backlog`.

## Operator scope recut (2026-07-23, interactive)

The headless proposal was reviewed and revised by the operator:

- **Removed 999.25 (release executor):** irreversible (crates.io publish / signed
  tag / merge-to-main), untestable inside a dogfood loop without a real publish,
  and its own dossier demands a dedicated interactive discuss on rollback
  semantics. → **own dedicated phase.**
- **Removed 999.28 (`--base`):** speculative under the current sequential-
  supervised cadence (a merged phase gives the next its work for free); its value
  is concurrency/stacking. → **Phase 22**, whole (not split).
- **Backfilled** with two legibility/observability units + one stretch:
  **999.14** (doctor planning-doc staleness reconciliation — the exact bug this
  session hit), **999.2** (sequentagent's untracked second process, narrowed
  since 18b shipped the monitor half), and optional **999.5**.
- **Renamed** phase "Operator Usability & Release Execution" → "Operator
  Legibility & Observability." Final set: 21a 999.3, 21b 999.14, 21c 999.2,
  optional 21d 999.5. CONTEXT.md rewritten to match.
