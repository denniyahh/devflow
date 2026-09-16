---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 08
subsystem: testing
tags: [rust, cargo-test, child-process, path-isolation, pipeline-outcomes]
requires:
  - phase: 48-04
    provides: Child-test PATH isolation for four pipeline-outcome fixtures
  - phase: 48-06
    provides: Module-local parent-child PATH fixture pattern
provides:
  - Child isolation for every abort-fixture test in pipeline_outcomes.rs
  - First-statement CHILD_TEST_ENV assertions in the three shared fixture helpers
  - abort_fixture_helper_refuses_to_run_outside_a_child — the guard's negative control
affects: [48-09, TEST-01]
tech-stack:
  added: []
  patterns:
    - A should-panic control whose expected substring comes from the guard's own message
    - Resolving a needed binary in the parent and passing an absolute path to the child
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
key-decisions:
  - "Explicit per-test child prologue rather than a module-local macro: the coverage gate scans source text, so a macro call would not satisfy it, and 48-04 already set the explicit style in this file."
  - "`sh` symlinked into the child PATH for the notify-hook test; a shell cannot be mistaken for an agent CLI, so the 999.80 guarantee is unchanged."
  - "The new env var is read through a const, so doc_check does not demand a test fixture in operator docs."
patterns-established:
  - "A guarded helper is converted together with every one of its callers, in the same change."
requirements-completed: []
plan_head_before: 4e3d627
commits: 1
actuals:
  tokens: 3016
  tasks: 2
  commits: 1
coverage:
  - id: D1
    description: Every abort-fixture function in pipeline_outcomes.rs runs in a child or refuses outside one; the coverage scan reads 0.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: "awk fixture-coverage scan over crates/devflow-cli/src/pipeline_outcomes.rs"
        status: pass
  - id: D2
    description: The structural guard demonstrably fires rather than merely being present.
    requirement: TEST-01
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_outcomes::tests::abort_fixture_helper_refuses_to_run_outside_a_child -- --exact"
        status: pass
---

# Plan 48-08 — Structurally Agent-Free Abort Fixtures

## What This Closes

999.80: the string `"abort: test cleanup"` inside a JSON note was the only
thing keeping eighteen fixture tests off `launch_stage`. Reword the note, or
change how `GateAction::from_response` classifies it, and the gate resolves as
LoopBack instead — which spawns whatever `claude` the developer happens to have
on PATH, from a unit test. The protection is now structural: those tests cannot
see an agent binary at all.

## Accomplishments

1. **Ten tests converted** to a guarded child run with an agent-free PATH
   directory.
2. **Three shared helpers guarded** with a first-statement `CHILD_TEST_ENV`
   assertion — `drive_validate_advance_and_read_gate_context`,
   `arm_a_ambiguous_outcome_gates_on_cycle_one`,
   `arm_b_genuine_failures_reach_the_ceiling`.
3. **Every caller converted with them.** Guarding the first helper broke its
   three callers (`validate_gaps_does_not_advance_to_ship`,
   `validate_missing_verdict_does_not_advance`, `validate_pass_advances`) —
   which is the guard working — and all three were converted in the same
   change. `arm_a`/`arm_b`'s sole caller was already child-isolated by 48-04.
4. **A negative control for the guard itself**,
   `abort_fixture_helper_refuses_to_run_outside_a_child`.

## Verification

Both `<automated>` gates print `gate_rc=0`. Workspace clippy exits 0.
`devflow-core` 792 passed / 0 failed; `devflow` 376 passed / 0 failed.

Gate figures: `fixture_fns_without_child=0`, `fixture_lines=18`,
`base_tests=72`, `now_tests=73`.

### Why the 0 is not a broken scan

Two independent checks, because a coverage count that can only ever read 0 is
indistinguishable from a working one:

- **Before/after.** The scan read **14** at the start of this plan — not the 18
  the plan's acceptance criterion cites, because 48-04 had already covered four
  functions. 14 is the honest baseline and it is recorded here in preference to
  the plan's stale figure.
- **Live discrimination.** Reverting one conversion
  (`ship_agent_failed_fires_gate`) made the same scan read **1**, then it was
  restored byte-identical. The scan discriminates now, not merely on 2026-09-14.

### The control proves the guard FIRES, not that a keyword is present

Counting guarded functions cannot show the assertion executes. Removing the
guard from `drive_validate_advance_and_read_gate_context` makes
`abort_fixture_helper_refuses_to_run_outside_a_child` fail with:

```
note: panic did not contain expected string
      panic message: "advance() must force a Validate gate, not advance silently"
```

That is the intended discrimination: `expected` is a substring of the guard's
own message, so an unrelated panic on the way cannot satisfy it.

**The same mutated run also demonstrated the threat, unprompted.** Without the
guard, the helper running in the parent printed `looping back to Code` and
`gate written: .devflow/gates/01-code.json` — i.e. it reached the LoopBack path
that leads to `launch_stage`, in a process whose PATH is the developer's real
one. 999.80 is not hypothetical.

**A note on how that mutation had to be run.** Unguarded, the helper first
*hung* rather than failing, because the gate it drives polls the three-day
production default; an unbounded hang cannot be distinguished from a wedged
harness. It was re-run with `DEVFLOW_GATE_TIMEOUT_SECS=2` so the failing
direction produced a real assertion message. The hang is itself evidence the
helper reaches a live gate in the parent.

### What this does NOT establish

- **Reachability to an agent spawn is still not measured per site**, exactly as
  the plan states. Converting every site makes the protection structural
  whether or not a given site can reach a spawn; it does not show that each one
  could have.
- The guarantee is that no *agent CLI* is reachable. `git` was already on these
  children's PATH, and this plan adds `sh` to one of them (below). Neither can
  be mistaken for an agent by `launch_stage`.
- Nothing here addresses 999.38's other half, the parallel PATH race in
  `commands.rs` — a different mechanism in a different file.

## Task Commits

1. **Task 1: tracer + helper guard + should-panic control** — `942c85c`
2. **Task 2: cover every remaining abort-fixture function** — `942c85c` (same operator-approved orchestrator normal-hook commit)

## Files Modified

- `crates/devflow-cli/src/pipeline_outcomes.rs` — thirteen child conversions,
  three helper guards, one new control test.

## Decisions Made

- **Explicit per-test prologue, not a module-local macro.** 48-06 used a macro
  in `preflight.rs`, but this plan's coverage gate scans SOURCE TEXT for
  `CHILD_TEST_ENV` or `in_child_test(` inside each function — a macro
  invocation would expand to them and still read as uncovered. The explicit
  form also matches what 48-04 already wrote in this same file.
- **The plan's stale 18 was not copied forward.** The baseline is 14.

## Deviations from Plan

**`non_validate_failure_fires_gate_and_hook` needed two things the git-only
PATH does not carry**, and the plan did not anticipate either:

1. `sh`, symlinked into the child's PATH directory.
   `gates::run_notify_command` executes the operator's hook as
   `Command::new("sh").arg("-c")`, and an unresolvable `sh` fails **fail-soft**
   — a `warn!`, no error — so the sentinel silently never appeared and the
   hook looked "not fired" for a reason unrelated to the behaviour under test.
   This cost two diagnostic rounds: resolving `touch` alone did not fix it,
   which is what pointed at the shell.
2. `touch`, resolved in the parent and passed to the child as an absolute path,
   so the hook's command string needs no PATH lookup.

Its new env var is read through a `const`, deliberately: `doc_check` flags a
`DEVFLOW_*` name only when it is a literal inside `var_os`, and would otherwise
demand this test fixture appear in the operator docs — the same defect `c7d5971`
fixed for 48-04 earlier in this session.

## Known Stubs

None.

## Threat Flags

- **T-48-08-01 (mitigated).** Child with an agent-free PATH on every fixture
  site; helpers panic outside a child; the should-panic control proves the
  panic fires and the mutated run showed the LoopBack path being reached in the
  parent.

## TDD Gate Compliance

This is an `execute` plan converting test fixtures, not a `type: tdd` plan, so
the runtime RED/GREEN commit gate does not apply — the same disposition 48-06
recorded. The known-positive controls were run in both directions: the scan
read 14 before conversion and 1 with a single conversion reverted, against 0
after.

## Requirements

`requirements-completed` is empty deliberately. TEST-01 stays **Pending**: Plan
48-09 still owns remaining work under it, and 999.38's parallel PATH race in
`commands.rs` is untouched by this plan.

## Next Phase Readiness

48-09 is next in the wave order. Two known-flaky tests remain unrelated to this
work: `agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`
(`/proc` visibility) and
`commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`
(999.38's PATH race). Both passed in this session's final runs; that is luck on
a race, not a resolution.

## Self-Check: PASSED

Verified in this session: both gates print `gate_rc=0`; the scan read 14 before
and 0 after, and 1 with one conversion reverted; the should-panic control fails
with `panic did not contain expected string` when its guard is removed; every
mutation reverted byte-identical; workspace clippy 0; both suites fully green.
