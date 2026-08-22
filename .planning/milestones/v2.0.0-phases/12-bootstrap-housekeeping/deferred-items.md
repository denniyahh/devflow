# Deferred Items — Phase 12 (Bootstrap + Housekeeping)

Pre-existing, out-of-scope items noticed during plan execution. Not fixed per the
scope-boundary rule (only issues directly caused by the current task's changes are
auto-fixed).

## `gsd-tools query state.advance-plan` incompatible with this project's STATE.md format

- **Noticed during:** 12-09 execution (state_updates step).
- **Issue:** `state.advance-plan` errors with `"Cannot parse Current Plan or Total
  Plans in Phase from STATE.md"`. This project's `.planning/STATE.md` uses a
  custom `## Active` / `## Completed` narrative format (predates strict adoption
  of the standard GSD state-file template) rather than the `Current Plan: X /
  Total Plans in Phase: Y` fields the verb expects.
- **Workaround used:** `state.update-progress` (recalculates the progress bar
  from SUMMARY.md counts on disk) still works correctly and was run instead —
  progress is accurately reflected (75%, 9/12 plans). `state.record-metric`,
  `state.add-decision`, and `state.record-session` also work fine against this
  STATE.md's format.
- **Not fixed here:** Restructuring STATE.md to the standard template is an
  unrelated, out-of-scope change for a test-coverage plan. Flagging for a future
  housekeeping plan if per-plan `Current Plan` tracking via `advance-plan`
  becomes needed.
- **Resolved 2026-07-22:** Added a standard `## Current Position` section
  (`Phase:` / `Plan:` / `Status:` / `Last activity:` fields) to STATE.md,
  additive alongside the existing narrative sections. `state begin-phase`
  writes real `Plan: N of M` values into this section when a phase starts,
  and `state advance-plan` now parses and increments it correctly — verified
  against a scratch copy simulating a Phase 20 kickoff (`1 of 5` → `2 of 5`).
  No changes made to `gsd-tools` itself; the gap was this project's STATE.md
  missing the section the parser expects, not a tooling bug.
