---
status: backlog
source: dogfood attempt 2026-07-20 — tried to run GSD planning for Phase 18 through devflow and found no way to stop after Plan
---

# Backlog: Plan-Only Pipeline Mode (`--until <stage>`)

## Goal

`devflow start` always runs the full Define → Plan → Code → Validate → Ship
pipeline. `--mode supervise` only changes *where it gates* (Validate and
Ship) — the Code stage still runs unattended. There is no `--until` flag
and no config knob (verified 2026-07-20: no `stop_after`/`until_stage`/
`plan_only`/`stage_limit` anywhere in `crates/`, and `config.rs` exposes
only `capture_retention`, `review_angles`, `external_verify_enabled`).

Consequence: "use devflow to just do the planning" is not expressible. The
only way to stop after Plan is to kill the monitor mid-pipeline, which
strands phase state and orphans a worktree — precisely the mess `18a`
(doctor reconciliation) and `18b` (monitor liveness) exist to clean up.

## Proposed shape

Add `devflow start --until <stage>` that halts cleanly after the named
stage completes: persist a terminal-but-not-failed state, emit a
`workflow_finished` event with an explicit "stopped at requested stage"
reason, and leave no polling monitor behind. `--until plan` is the
motivating case (produce PLAN.md files, then hand back to a human), but
`--until code` and `--until validate` are equally reasonable.

## Why it matters beyond convenience

Dogfooding is this project's highest-yield bug source, and the cheapest
dogfood run is the one that exercises the fewest stages. Without a clean
stop point, every dogfood run is all-or-nothing: either run the full
pipeline (which merges, tags, and releases) or don't dogfood at all. That
directly discourages the small, frequent runs that surface the most
findings.

## Notes

Discovered while attempting to plan Phase 18 through devflow itself.
Operator chose to plan directly rather than kill a monitor mid-run.

Promote with `/gsd-review-backlog` when ready.
