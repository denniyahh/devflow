---
status: backlog
source: 17-REVIEW.md WR-04, triaged 2026-07-20 and re-verified still present at HEAD
---

# Backlog: Layer 0 Unapproved-Probe Veto — Test and Doc Coverage

## What this is (and is not)

**Not a defect.** 17-03 removed `evaluate_layer0`'s `Stage::Code` guard
deliberately (documented as D-05 gap 1). The consequence is that a PLAN carrying
`external_verify:` without `DEVFLOW_TRUST_EXTERNAL_VERIFY` exported now returns
`Failed` → `Action::GateReview` at **Define, Plan, Code, Validate and Ship**
instead of Code alone. With `external_verify_enabled` defaulting to `true`, the
blast radius of a forgotten env var grew 5×.

This item is the **coverage debt** on that deliberate trade, not a request to
reverse it.

## Verified gaps at HEAD (2026-07-20)

1. **No test covers the unapproved path at any stage.** `evaluate_layer0`
   (`crates/devflow-core/src/agent_result.rs`) has three veto arms — "PLAN
   declaration was removed", "not approved", and "PLAN commands changed". Only
   the mismatch arm is tested (`agent_result.rs:1644`, asserting
   `reason.contains("approval mismatch")`). The **"external verification is not
   approved"** arm — the one a forgotten env var actually hits — has no test at
   all, at Code or anywhere else.

2. **The detached-monitor env inheritance requirement is undocumented.**
   `docs/guides/configuration.md:41,53` documents `DEVFLOW_TRUST_EXTERNAL_VERIFY`
   and states the requirement in terms of "the parent DevFlow process". It never
   says the **detached monitor subprocess must inherit it** — which is where the
   forgotten-env-var failure actually manifests, since the monitor is what
   evaluates the result. `OPERATIONS.md` does not cover it either.

## Suggested scope

- Add tests for the "not approved" veto arm at a non-Code stage (Validate or Ship
  are the interesting ones, since that is what the guard removal newly exposed).
- Document the monitor-inheritance requirement in
  `docs/guides/configuration.md` and/or `OPERATIONS.md`.

## Adjacency warning for whoever picks this up

Phase 18's plan **18-05** modifies the same file (`agent_result.rs`) and adds
Layer 0/Validate tests for a *different* issue (18e — the affirmative-success arm
discarding the agent's verdict). Check what 18-05 landed before writing tests
here; there may be reusable fixtures, and there is some risk of overlapping test
names around Layer 0 at `Stage::Validate`.

This was deliberately **not** folded into 18-05 during triage: 18-05 had already
passed the plan-checker clean, and adding a documented-deliberate-trade's coverage
debt to a verified bug-fix plan is scope creep of exactly the kind that made prior
phases balloon.

Promote with `/gsd-review-backlog` when ready — ideally shortly after Phase 18
ships, while 18-05's changes are still fresh.
