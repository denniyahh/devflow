---
phase: 17-pipeline-dogfood-followup
status: spike
created: 2026-07-18
source: Phase 16 DevFlow dogfood run
---

# Phase 17 Spike: Pipeline Dogfood Follow-Up

## Purpose

Capture the operational evidence produced while DevFlow executed Phase 16 and
decide which reliability gaps must be repaired before adding Hermes-specific
behavior. This is a decision and scoping artifact, not an assertion that every
item below belongs in the same implementation phase.

## Confirmed Finding: Dogfood Used a Stale Executable

Phase 16 verification states that a failed merge stops terminal hooks, reopens
an actionable Ship gate, and emits `workflow_finished` only after every
required hook succeeds. The Phase 16 runtime event stream appears to show the
opposite sequence:

1. `hook_run` for `Merge` reports `ok:false`.
2. `VersionBump` and `BranchCleanup` report `ok:true`.
3. `workflow_finished` is emitted.

The evidence was reconciled against the executable provenance. The dogfood
command resolved to `~/.linuxbrew/bin/devflow`, which linked to
`target/release/devflow` built before every Phase 16 terminal-hook fix. The
event therefore records real behavior from an obsolete binary, not final Phase
16 source behavior. The current source has a targeted regression test proving
that a failed Merge reopens the Ship gate and does not emit
`workflow_finished`.

This is still a reliability issue: DevFlow cannot let a dogfood operator run
an unidentifiable or stale build while assuming the checkout's source behavior.
Do not make it a Hermes-only concern.

**Candidate capability:** compile and expose build provenance (version, commit
when available, build timestamp, executable path); emit it in
`workflow_started`; and provide `devflow doctor` guidance that compares it to
the current source checkout when DevFlow is dogfooding itself. A strict mode
should block self-dogfood runs on a known stale build, while ordinary projects
remain able to use a released binary without a source checkout.

## Other Dogfood Findings

### Completion and Retry Policy

- Rate limits, exit code 137, and generic exit code 1 all repeatedly looped
  back to Code and opened gates without an actionable recovery classification.
- An `unknown` Code result (process gone, commits present) advanced directly
  to Validate. Unknown completion must require a human gate or a declared
  external post-condition; it must not transition automatically.
- Stage success events with `reason: null` reduce forensic value. Require a
  typed completion evidence record for every terminal monitor decision.

**Candidate capability:** typed agent outcomes (`rate_limited`,
`resource_killed`, `agent_unavailable`, `unknown`, `failed`), deterministic
resume/backoff guidance, and an explicit policy for each outcome.

### Preflight Readiness

- A Code plan required interactive selection while the active execution mode
  could not request input.
- Ship discovered an empty reviewer receiver set only after its work had run.
- A late Ship preflight found both a missing required security artifact and
  invalid GitHub authentication.

**Candidate capability:** before launching a phase, scan planned operations
and configured adapters for required interaction, reviewer availability,
security artifacts, external credentials, and declared post-condition probes.
Fail as a named preflight gate before agent time is consumed.

### State and Event Reconciliation

- Current project state and recent event history can disagree about the active
  phase after interrupted or terminal runs.
- Events should distinguish observed effect, hook result, and workflow state
  transition so an operator can determine whether a run is safe to resume.

**Candidate capability:** `devflow doctor` or `devflow recover --check` that
compares state, live PIDs, gates, hook events, branch ancestry, and retained
captures; it should report a repair plan without mutating anything by default.

### Test Reliability

Phase 16 review already records WR-03: a parallel CLI test checks a live
capture path after the monitor may have archived it. Stabilize this test by
asserting a captured generation immediately or accepting its retained-history
counterpart.

## Relationship to Phase 18 Hermes Support

Hermes should consume the generic outcome and preflight model rather than
define competing behavior. Its native completion envelope can improve outcome
classification and its session UI can surface resume/gate guidance, but it
must not be used to bypass the terminal Ship invariant or infer missing
readiness data.

## Decision Gate

Before planning implementation, answer these questions with a final-HEAD
reproduction:

1. Did terminal hooks actually continue after a failed Merge, or does the
   event stream mix pre-fix and post-fix attempts? **Resolved:** the event
   came from a stale executable; retain a provenance check to prevent repeats.
2. What exact condition allowed `unknown` to transition to Validate?
3. Which readiness checks are universally applicable, and which are adapter or
   provider-specific?
4. Should this become a focused Phase 17 repair, leaving Hermes as Phase 18,
   or should only proven defects be pulled forward as a Phase 16 remediation?

## Initial Acceptance Criteria

1. A failed Merge leaves the feature branch intact, prevents VersionBump and
   BranchCleanup, preserves state, and opens a Ship gate.
2. `workflow_started` records executable and build provenance; self-dogfood
   can detect and reject a stale binary before stage launch.
3. `unknown` completion cannot reach the next stage without explicit approval
   or a successful, declared external post-condition.
4. A non-interactive plan, unavailable reviewer, missing security artifact,
   or invalid required credential is reported before the stage launch.
5. `devflow doctor` can explain any disagreement between state, events, and
   branch ancestry without changing project files.
