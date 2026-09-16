---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 09
subsystem: testing
tags: [rust, clippy, lint, environment, test-isolation]
requires:
  - phase: 48-04
    provides: PATH mutator removal in pipeline_outcomes tests
  - phase: 48-05
    provides: PATH mutator removal in pipeline_launch tests
  - phase: 48-06
    provides: PATH mutator removal in preflight and pipeline_gate tests
  - phase: 48-08
    provides: Structural child isolation for every abort fixture
  - phase: 48-15
    provides: Checkpoint-response test mutations, annotated here
provides:
  - clippy.toml disallowing std::env::set_var and std::env::remove_var
  - 21 reasoned, test-only expect(clippy::disallowed_methods) exceptions
  - A three-spelling lint negative control (fully qualified, module import, renamed alias)
affects: [TEST-01]
tech-stack:
  added: [clippy.toml]
  patterns:
    - expect over allow, so an exception that stops being needed becomes a warning
    - Short attribute reason plus an unbounded comment above it, to stay rustfmt-stable
key-files:
  created: [clippy.toml]
  modified:
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/monitor.rs
key-decisions:
  - "expect, never allow: under -D warnings an unfulfilled expectation warns, so the exception list cannot outlive its reason."
  - "The attribute's reason is short by necessity (rustfmt width); the specific per-site justification lives in the comment above it."
patterns-established:
  - "Before relying on a gate's text pattern, check what rustfmt actually emits — a long attribute is silently re-wrapped."
requirements-completed: [TEST-01]
plan_head_before: a0dbc7c
commits: 1
actuals:
  tokens: 2264
  tasks: 2
  commits: 1
coverage:
  - id: D1
    description: Process-global env mutation is denied workspace-wide, with the child-Command alternative named in the lint reason.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: "cargo clippy --workspace --all-targets -- -D warnings (exit 0 with clippy.toml present)"
        status: pass
  - id: D2
    description: All 21 remaining non-PATH mutations carry a reasoned, test-only expectation; no production code and no allow attribute.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: "48-09 Task 1 gate: allow_attrs=0, cli_expect_attrs=16, core_expect_attrs=5"
        status: pass
  - id: D3
    description: Three import spellings of the mutator all trip the lint, and the probe file is restored byte-identical.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: "48-09 Task 2 gate: negative_control_exit=101, negative_control_disallowed_lines=3, probe_file_restore_bytes=0"
        status: pass
---

# Plan 48-09 — Make the TEST-01 Boundary Enforce Itself

## What This Closes

48-04, 48-05, 48-06 and 48-08 removed every process-global PATH mutation from
the test binary. Nothing stopped one coming back. A workspace Clippy guard now
denies the mutators outright, and each of the 21 deliberately-retained non-PATH
mutations is an explicit, reasoned, test-only exception.

## Accomplishments

1. **`clippy.toml`** disallowing `std::env::set_var` and
   `std::env::remove_var`, each reason explaining that the mutation is
   process-global and racy across concurrent tests and the children they spawn,
   and naming `Command::env` / `env_remove` / `env_clear` as the alternative.
2. **21 reasoned exceptions** on the smallest enclosing test-only item, each
   preceded by a comment naming the exact variable(s) and why per-`Command`
   scoping cannot replace it — THIS process reads the value, so scoping it to a
   child would not reach the reader. 16 in `devflow-cli`, 5 in `devflow-core`.
3. **A three-spelling negative control** proving the guard cannot be walked
   around by an import alias.

## Verification

Both gates print `gate_rc=0`. `cargo clippy --workspace --all-targets -- -D warnings`
exits 0. `devflow-core` 792 passed / 0 failed; `devflow` 376 passed / 0 failed.

### The guard is self-policing, and that is the strongest claim here

`expect` rather than `allow` is the load-bearing choice. Under `-D warnings`:

- a mutation site **without** an annotation is a hard error, and
- an annotation that is **no longer needed** is an unfulfilled-expectation
  warning, which is also an error.

So a clean clippy run does not merely mean "no complaints" — it means the
exception list is **exactly** the set of real mutation sites, neither short nor
padded. That is a property of the mechanism, not of anyone's diligence in
pruning a list.

### Negative controls

| Control | Expected | Observed |
|---|---|---|
| `clippy.toml` present, sites unannotated | lint fires | 36 CLI + 12 core `disallowed method` diagnostics |
| Probe: fully qualified `std::env::set_var` | rejected | part of the 3 below |
| Probe: `use std::env` + `env::set_var` | rejected | part of the 3 below |
| Probe: `use std::env::set_var as mutate_env` | rejected | part of the 3 below |
| All three together | non-zero exit, >=3 diagnostics | `negative_control_exit=101`, `disallowed_lines=3` |
| Probe file restored | byte-identical | `probe_file_restore_bytes=0`, explicit-base diffs match |

Probe base: `base_sha=a0dbc7c9d55b05df68de97740c823aee1a417cd6`, resolved and
verified before use; the pre- and post-probe diffs of `lib.rs` against that
same revision are identical, so any change already present before the probe is
treated as baseline rather than residue. `crates/devflow-core/src/lib.rs` is
byte-identical to its pre-probe state.

### What this does NOT establish

- **The negative control establishes lint DETECTION, not the absence of future
  test flakiness** — the plan says this explicitly and it is worth repeating.
  999.38's remaining symptom, the parallel PATH race in `commands.rs`, is a
  different mechanism and is untouched.
- Three spellings are proven; the lint is name-resolution based, so a mutation
  reached through a trait object, a macro-generated path, or a re-export chain
  was not probed.
- The 21 retained mutations are still process-global. D-09 accepts that
  (T-48-09-03) and ENV_MUTEX still bounds them. Nothing here makes them safe;
  it makes them **visible and audited**.

## Task Commits

1. **Task 1: lint config + CLI annotations** — `15370f7`
2. **Task 2: core annotations + three-spelling probe** — `15370f7` (same operator-approved orchestrator normal-hook commit)

## Decisions Made

- **The attribute's reason is short, and that was forced rather than chosen.**
  rustfmt's `attr_fn_like_width` (70% of `max_width`, and there is no
  `rustfmt.toml` here so `max_width` is the default 100) applies to the
  attribute's INNER arguments. `clippy::disallowed_methods, reason = "..."`
  must stay at or under 70 characters, leaving 31 for the text. The
  per-site specifics therefore live in a comment directly above each
  attribute, where nothing truncates them.

## Deviations from Plan

1. **A first attempt produced a gate that read `cli_expect_attrs=0` while 16
   annotations were present.** Long per-site reasons in the attribute were
   silently re-wrapped by `cargo fmt` into
   `#[expect(\n    clippy::disallowed_methods,\n    reason = ...)]`, so the
   plan's single-line `expect\(clippy::disallowed_methods` pattern matched
   nothing. Found by running the gate rather than by inspection. The threshold
   was then measured in a throwaway crate (80 chars stayed on one line, 92
   split) instead of guessed.
2. **The plan's `files_modified` is accurate and needed no extension.** An
   initial `rg` suggested `commands.rs`, `staleness.rs` and
   `test_support.rs` also mutate the environment; all three matches are DOC
   COMMENTS describing the hazard, not calls. Clippy reported no sites there.
3. **Core diagnostics were nearly missed to a caching artifact.** A workspace
   clippy run after an earlier `-p devflow` run emitted only CLI sites,
   because `devflow-core`'s results were cached and re-emit nothing. The 12
   core sites were found by touching `lib.rs` to force a fresh lint. A
   workspace run alone would have under-reported.

## Known Stubs

None.

## Threat Flags

- **T-48-09-01 (mitigated):** Clippy guard plus the three-spelling control.
- **T-48-09-02 (mitigated):** `allow` attributes are zero and the gate rejects
  them; every exception is a reasoned `expect`.
- **T-48-09-03 (accepted, per D-09):** the 21 non-PATH mutations remain
  process-global under ENV_MUTEX, now individually justified in-source.

## TDD Gate Compliance

An `execute` plan configuring a lint and annotating existing tests, not a
`type: tdd` plan, so the runtime RED/GREEN commit gate does not apply — the
disposition 48-06 and 48-08 recorded. The known-positive controls ran in both
directions: 48 diagnostics before annotation, 0 after, and 3 from the probe.

## Requirements

`requirements-completed: [TEST-01]`. This is the last plan in Phase 48 carrying
TEST-01, and the requirement's PATH-isolation scope is now enforced structurally.
**It is not a claim that 999.38 is fully resolved:** the parallel PATH race in
`commands.rs` that 999.38 also describes is out of this phase's TEST-01 scope
and still reproduces intermittently.

## Next Phase Readiness

Next in the wave order: 48-10, 48-11 and 48-17. Two known-flaky tests remain,
unrelated to this plan and not fixed by it:
`agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`
and
`commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`.

**Anyone adding an environment mutation from here on** must either scope it to a
child `Command` or add a reasoned `expect` whose inner arguments stay within
70 characters; the build will refuse it otherwise.

## Self-Check: PASSED

Verified in this session: both gates print `gate_rc=0`; the lint fired on 48
unannotated sites and 0 after annotation; all three probe spellings were
rejected and `lib.rs` restored byte-identical against an explicitly resolved
base; all 21 expectations are inside `#[cfg(test)]` modules; workspace clippy 0
and both suites fully green.
