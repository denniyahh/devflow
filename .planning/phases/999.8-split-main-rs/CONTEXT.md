---
status: backlog
source: surfaced 2026-07-20 during Phase 18 planning — the same-wave zero-file-overlap rule forced 6 near-serial waves because 6 of 7 plans touch main.rs
blocked_by: Phase 18 (deliberately — see Sequencing Rationale)
---

# Backlog: Split `crates/devflow-cli/src/main.rs`

## Problem

`main.rs` is **6,239 lines** — 3,307 production (lines 1–3307) plus a 2,931-line
`#[cfg(test)]` module (lines 3308–6239, 71 tests). It is the single largest file
in the workspace by ~2.6x (next is `agent_result.rs` at 2,397).

Its size is now the binding constraint on execution parallelism. GSD's same-wave
zero-file-overlap rule keys on file path, so any two plans touching `main.rs`
cannot share a wave regardless of whether they touch disjoint functions. Phase 18
was forced into **6 near-serial waves** for 7 plans purely on this basis — the
planner flagged it explicitly as "serial by necessity, not by choice."

## Measured cluster boundaries (2026-07-20, at `main.rs` HEAD)

The production half already decomposes cleanly:

| Cluster | Lines | Representative functions | Phase 18 plans touching it |
|---|---|---|---|
| env/config parse | 30–53 | `parse_gate_timeout`, `checkout_lock_timeout` | — |
| dispatch | 329–520 | `main`, `run`, `resolve_gate_target` | — |
| preflight | 671–834 | `run_preflight`, `preflight_gh_auth_check`, `ensure_agent_binary` | 18-07 |
| staleness/provenance | 835–1136 | `enforce_build_staleness`, `combined_staleness`, `embedded_commit_is_stale` | 18-06 |
| pipeline state machine | 1137–1900 | `launch_stage`, `advance`, `transition`, `handle_*_outcome`, `run_gate` | 18-04, 18-05, 18-07 |
| parallel/sequentagent | 2018–2400 | `parallel`, `sequentagent`, `run_agent_blocking` | — |
| commands/display | 2470–3160 | `status`, `doctor`, `logs`, `gate_list`, `recover_cmd`, `list` | 18-01, 18-03 |

**Projected gain:** 6 waves → 3. Wave 1 could run 18-01 (commands), 18-02 (tests),
18-04 (pipeline), 18-06 (staleness) in parallel; wave 2 gets 18-03 + 18-05; wave 3
gets 18-07. The residual serialization is *logical* dependency (18b extends 18a's
reconciliation; 18e is causally entangled with 18d) and no refactor removes it.

## Sequencing Rationale — why this is deliberately blocked on Phase 18

**The primary risk is `ENV_MUTEX`, which appears 22 times in `main.rs`.** It is a
process-global mutex serializing env-var mutation across tests. Redistributing 71
tests across new module boundaries while preserving those serialization guarantees
is precisely the failure class this project has the worst track record with:

- **19i** — process-global `PATH` race via `set_var`; hit **2/2 in CI** on the
  v1.4.0 release PR after passing locally most of the time.
- **GAP-2** — concurrent-ship gate-poll hang; ~33–40% of isolated runs.
- **999.4** — version-tag contention; caught only by instrumentation, both phases
  inside `version_bump` ~1.8ms apart.

All three were invisible on a dedicated workstation and expensive to diagnose.

Phase 18 is the work that makes this class observable: 18a adds `doctor`
state/event/PID/gate reconciliation, 18b makes a dead monitor and a stuck pipeline
representable instead of rendering identically to a healthy between-stages moment.
Running this refactor *after* those land means having instrumentation when it
misbehaves; running it before means debugging a flaky refactor with the same blind
tooling that cost ~4h twice during the Phase 17 run.

Two secondary reasons, not decisive alone:

- Phase 18 reshapes the exact functions that determine the seams — 18f splits
  `launch_stage` into `launch_stage`/`launch_stage_inner`, and 18e changes the
  Validate outcome type to a `ValidateOutcome` enum. Choosing module boundaries
  before that is guessing; choosing after is evidence-based.
- It would invalidate 7 plans that passed the plan-checker clean with zero
  blockers and zero warnings (~25 min of planner + checker agent time to redo).

## Proposed shape when promoted

- **Pure-move refactor, zero behavioral change.** No logic edits bundled in — that
  is what makes the existing 381-test suite valid as the equivalence proof.
- **Split the test module in the same operation.** Leaving 2,931 test lines in
  `main.rs` defeats the purpose. Rust unit tests reach parent-module private items,
  so each cluster's tests move with its code; tests spanning clusters need explicit
  handling. `ENV_MUTEX` must become a shared item whose serialization still holds
  across modules — this is the part to review hardest.
- **Verify on a branch with CI.** Feature-branch CI now runs on every push (as of
  `f25c670`), and 19i is direct evidence that CI's shared runners widen race windows
  relative to this workstation. Do not accept local-green as sufficient.
- Keep `main.rs` as thin dispatch (`main`, `run`, arg routing) only.

## Open questions for discuss-phase

- Module layout: flat siblings (`preflight.rs`, `staleness.rs`, `pipeline.rs`,
  `commands.rs`) vs. a `commands/` subdirectory with one file per subcommand.
- Whether any of these clusters belong in `devflow-core` rather than `devflow-cli`
  — `staleness` and `preflight` are arguably core logic currently living in the CLI
  crate. Moving them changes the public API surface and is a larger decision than
  the file split itself; may warrant deferring to keep this a pure move.

## Addendum (2026-07-21)

Worth treating this as bigger than a file-size problem when picked up. `ENV_MUTEX`
is now a *repeat* root cause across three separate expensive-to-diagnose failures:
19i (`PATH` race, hit 2/2 in CI after mostly passing locally), GAP-2
(concurrent-ship gate-poll hang, ~33–40% of isolated runs), and 999.4
(version-tag contention, caught only by instrumentation). The scrutiny during this
split should be on whether `ENV_MUTEX`'s serialization guarantees can actually
survive being distributed across module boundaries — not just on relocating code
cleanly. If they can't be preserved without a structural change to how tests
serialize env mutation, that's a finding worth surfacing on its own, not something
to patch around silently mid-refactor.

Promote with `/gsd-review-backlog` when ready — **after Phase 18 ships**.
