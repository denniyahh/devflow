---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
plan: 04
subsystem: pipeline test harness (devflow-cli)
tags: [test-repair, flake-closure, gap-closure, 999.66, CR-01, D-01]
status: complete
requirements: [DOGFOOD-01, DOGFOOD-02]

requires:
  - "33-03's consecutive_failures_made_progress wiring in handle_validate_outcome"
  - "crates/devflow-cli/src/test_support.rs::agent_free_git_only_path_dir"
provides:
  - "abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response, restored to its gate/Abort path and structurally unable to spawn an agent"
  - "failure_gate_loop_back_respects_the_mid_arc_check, restored to the consecutive-failure-gated arm and able to detect being moved off it"
  - "a workspace-wide classification of every direct consecutive_failures seed site"
affects:
  - "scripts/check.sh all — now green on repeated captured runs"

tech-stack:
  added: []
  patterns:
    - "branch-pinning assertion: a test that seeds state to steer a branch asserts the branch it expects to have run, not only side effects both branches produce"
    - "PATH neutralization under ENV_MUTEX around any unit-test call that can reach launch_stage"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs

decisions:
  - "Bound agent_free_git_only_path_dir() INSIDE the ENV_MUTEX block rather than before it, so PATH is read under the mutex — binding it earlier would race a sibling PATH-swapping test into the helper's expect(\"git must be resolvable on PATH\")."
  - "Proceeded past the tracer feedback gate without a human-verify checkpoint: the plan is autonomous: true with zero checkpoint tasks, and both of Task 1's verify gates passed."

metrics:
  duration: "~15 min (2026-08-05T02:52:26Z to 2026-08-05T03:07:46Z)"
  completed: 2026-08-05

actuals:
  tokens: 45900   # chars/4 over the two changed files (183,658 chars)
  tasks: 3
  commits: 2      # Task 3 is measurement-only and changed no source
---

# Phase 33 Plan 04: Loop-Back Test-Harness Repair Summary

Two pre-existing tests that 999.66's reset-vs-accumulate change had silently moved off their
asserted code paths are restored to those paths and given branch discriminators, so the next
semantics change fails loudly instead of relocating them a third time — and the one that had been
launching a real `claude` agent during `cargo test` can no longer resolve an agent binary at all.

## What changed

| Symbol | File | Change |
|---|---|---|
| `abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response` | `pipeline_gate.rs` | baseline seed, agent-free PATH under `ENV_MUTEX`, counter assertion, stage assertion, doc comment |
| `failure_gate_loop_back_respects_the_mid_arc_check` | `pipeline_outcomes.rs` | baseline seed, `loop_back`-event counter assertion, doc comment |

No production code changed. No new public symbols. No assertion deleted, loosened, ignored,
filtered, or excluded; `scripts/check.sh` gained no retry, serialization flag, or filter.

## Task 1 — abort test (commit `add79bd`)

**Precondition:** `consecutive_failures_made_progress` present in `pipeline_outcomes.rs:303`. Met.

**Step 1 RED (PATH neutralized, seed absent).** Literal output:

```
---- pipeline_gate::tests::abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response stdout ----
looping back to Code (validate failures: 1)

thread '...abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response' (2603701)
panicked at crates/devflow-cli/src/pipeline_gate.rs:1134:80:
called `Result::unwrap()` on an `Err` value:
Message("agent binary `claude` not found — is it installed? (run `devflow doctor`)")

test result: FAILED. 0 passed; 1 failed; ... 268 filtered out; finished in 0.01s
```

This is a **launch failure**, not a compile error, a git error, or an unrelated panic
(ai-change-acceptance requirement 3): the panic names the missing agent binary, and it is raised at
the `handle_validate_outcome(...).unwrap()`. The stdout line `looping back to Code (validate
failures: 1)` is independent corroboration of the diagnosis — the counter had been reset to 1 and
the test was on the loop-back arm, printed by `prepare_loop_back_to_code` itself.

**Step 2 GREEN.** With the seed added: `test result: ok. 1 passed; 0 failed; ... 268 filtered out;
finished in 1.00s`. No git error, which **discharges the plan's open assumption 2** —
`agent_free_git_only_path_dir()` does keep `git` resolvable, so `phase_commit_count`'s
no-repository 0 still comes back.

**Step 3.** Both branch-pinning assertions added ahead of the three gate-file assertions, transcribed
verbatim. Still green at 1.01s.

**Step 4 negative control (seed removed, PATH neutral).** Produced the opposite result — the test
FAILED:

```
looping back to Code (validate failures: 1)
panicked at crates/devflow-cli/src/pipeline_gate.rs:1149:80:
called `Result::unwrap()` on an `Err` value:
Message("agent binary `claude` not found — is it installed? (run `devflow doctor`)")
```

Seed restored byte-identically; test passed again.

**The limit of that control, stated explicitly.** Because PATH was neutral, it failed at the launch
`unwrap` on the agent-binary error, **not at either new assertion** — execution never reached them.
It therefore establishes only that **the seed is load-bearing**. It does **not**, on its own,
establish that the two branch assertions can detect a relapse. The evidence for that second claim is
cited, not re-derived:

- the operator's already-captured live-PATH run of this same control, which exited 101 panicking on
  `gate threshold must have been reached (got 1) — a reset means the gate never fired`, **after**
  first emitting `launched Claude Code (monitor pid 2530819)`;
- Task 2's control below, which trips its own discriminator for free.

**The live-PATH spawning variant of the negative control was NOT run in this execution.** It spawns
a real unattended agent and burns quota; its absence is stated here rather than left to be inferred.

**Acceptance evidence**

| Check | Result |
|---|---|
| single-test regex (`1 passed` + non-zero `filtered out` + `finished in` < 5s) | `test result: ok. 1 passed; 0 failed; ... 268 filtered out; finished in 1.01s` |
| whole `pipeline_gate::` slice | `test result: ok. 17 passed; 0 failed; ... finished in 4.32s` |
| `rg -n 'last_validate_failure_commit_count' pipeline_gate.rs` | exactly 1 match |
| `rg -c 'a reset means the gate never fired'` | `1` |
| `rg -c 'it silently looped back and tried to launch an agent'` | `1` |
| assertion ordering | confirmed by reading the diff hunk — both new assertions precede the first `Gates::gate_path(...).exists()` |
| production `state.consecutive_failures =` above `#[cfg(test)]` | still `1` (only `transition()`'s reset) |
| `git diff --stat` | `pipeline_gate.rs` only |
| clippy `--workspace --all-targets -D warnings` / `cargo fmt --check` | own exit codes 0 / 0 |

**ENV_MUTEX counts, both recorded.** Textual matches went **17 → 20**; lock *acquisitions* went
**5 → 6**. The criterion expected "+1", which holds for acquisitions; the +3 textual is 1 acquisition
plus the 2 mandated `// SAFETY:` comments copied verbatim from the sibling idiom.

## Task 2 — gated-arm test and the sweep (commit `c210978`)

**RED for the discriminator (assertion added, seed withheld).** This is the plan's one demonstration
that the branch-pinning layer detects the defect rather than merely accompanying the seed:

```
---- pipeline_outcomes::tests::failure_gate_loop_back_respects_the_mid_arc_check stdout ----
looping back to Code (validate failures: 1)

panicked at crates/devflow-cli/src/pipeline_outcomes.rs:1635:9:
must be the consecutive-failure-GATED loop-back arm (counter 1) — a value below the threshold
means this ran the ungated tail arm instead

test result: FAILED. 0 passed; 1 failed; ... 268 filtered out; finished in 0.00s
```

It failed **on that specific assertion**, with observed sub-threshold counter **1** — not a compile
error, not the `expect` on a missing `loop_back` event, not an unrelated panic. The `loop_back` event
existed and carried a counter of 1, which is precisely the ungated tail arm the test's own doc
comment says it exists not to cover.

**GREEN after seeding:** `test result: ok. 1 passed; 0 failed; ... 268 filtered out; finished in
0.00s`. The pre-existing `last["fix"] == "FullExecute"` assertion did **not** break under the seed,
independently re-confirming the plan's discharged assumption 1.

**Acceptance evidence**

| Check | Result |
|---|---|
| single-test regex | `1 passed`, `268 filtered out` (non-zero) |
| `rg -c 'last_validate_failure_commit_count = Some\(0\)' pipeline_outcomes.rs` | `4` (lines 792, 843, 1616, 1846) |
| `rg -c 'consecutive-failure-GATED loop-back arm'` | `1` |
| whole `pipeline_outcomes::` slice | `test result: ok. 37 passed; 0 failed; ... finished in 2.24s` |
| assertion ordering | confirmed by reading the diff hunk — precedes the `last["fix"]` assertion |
| `git diff --stat` | `pipeline_outcomes.rs` only; all three hunks sit inside `mod tests` (lines 1591+), so no line inside `handle_validate_outcome`, `select_loop_back_fix`, or `handle_ship_outcome` is touched |
| clippy / fmt | own exit codes 0 / 0 |

### Part B — reconciled sweep, two independent counts

- **Count A** (receiver-bound, `state\.consecutive_failures\s*=[^=]` over `crates/`): **12**
- **Count B** (receiver-agnostic, `\bconsecutive_failures\s*(=[^=]|:)` over `crates/`): **18**

**Count B ≥ Count A confirmed** (18 ≥ 12). B's 6 extra matches split into **2 genuine seed sites**
the narrow pattern structurally cannot see (`state_a` / `state_b` receivers) and **4 declarations**
that are not seeds at all: the field declaration (`state.rs:50`), the `State::new` struct-literal
initializer (`state.rs:263`), and two function parameters (`mode.rs:170` `should_gate`,
`pipeline_outcomes.rs:834` the `drive_validate_advance...` helper).

Real assignment sites = 12 + 2 = **14**, matching the planner's figure.

Every verdict below was derived by reading the enclosing item's call path **before** the planner's
table was consulted. Matching is by enclosing function name; line numbers are current-tree.

| Site | Enclosing item | Reaches `handle_validate_outcome`'s Failed branch? | Verdict |
|---|---|---|---|
| `state.rs:361` | `consecutive_failures_persists_across_advance_calls` | no — pure serde round-trip, no pipeline call | N/A |
| `pipeline_outcomes.rs:313`, `:318` | **production** — the reset/accumulate branch itself | n/a | N/A |
| `pipeline_outcomes.rs:785` | `validate_failure_threshold_forces_gate_then_aborts` | yes — direct call | already seeded (`:792`) |
| `pipeline_outcomes.rs:839` | `drive_validate_advance_and_read_gate_context` | yes — via `advance()` on a Validate stage | already seeded (`:843`) |
| `pipeline_outcomes.rs:1112` | `resource_killed_on_code_bumps_infra_failures_not_consecutive_failures` | no — `advance()` on a **Code** stage with exit 137 routes to the infra arm; test also asserts no Validate gate ever appeared | N/A |
| `pipeline_outcomes.rs:1148` | `resource_killed_on_validate_bumps_infra_not_consecutive_failures` | no — calls `handle_infra_outcome` directly | N/A |
| `pipeline_outcomes.rs:1611` | `failure_gate_loop_back_respects_the_mid_arc_check` | **yes** | **SEEDED — Task 2** |
| `pipeline_outcomes.rs:1842` | `consecutive_failures_increment_saturates` | yes — direct call | already seeded (`:1846`) |
| `pipeline_gate.rs:96` | **production** — `transition()`'s existing reset | n/a | N/A |
| `pipeline_gate.rs:1119` | `abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response` | **yes** | **SEEDED — Task 1** |
| `pipeline_gate.rs:1287` | `repeated_code_to_validate_transition_is_idempotent_on_the_counter` | no — calls `transition()` only | N/A |
| `pipeline_gate.rs:1638`, `:1643` | `consecutive_failures_are_independent_across_phases` (`state_a`/`state_b`) | no — calls `transition()` only; already PATH-neutralized under `ENV_MUTEX` | N/A |

**Discrepancies against the planner's table.** No verdict disagrees. One stated-number discrepancy
worth recording: the plan says "a receiver-agnostic search returns **14**", whereas my receiver-agnostic
pattern returned **18** — because it also matched the 4 declarations listed above. This is exactly the
outcome the plan predicted for a wider pattern ("those are declarations, not seeds"), and it does not
change the seed-site set. All 14 assignment sites and all four `Some(0)` seeds are accounted for.

## Task 3 — flake closure, with the strength stated

**Ten full `devflow` bin-binary runs at default parallelism** (each run's own exit code captured, not
a pipeline's): `run 1..10 exit=0`, **`FAILED_RUNS=0`**, loop exit status 0.

**Passed-count reconciliation (the second, independent count):** all ten parent runs report an
identical `test result: ok. 269 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. No run was
green with a smaller count.

One thing the raw `uniq -c` output showed that needed explaining before the reconciliation could be
trusted: 20 extra `1 passed; 268 filtered out` lines across the ten logs. These are **not** the parent
runs. Two pre-existing tests (`...tag_exists_and_reachable_resolves_caller_root_under_a_hostile_git_dir`
and `...embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir`) re-invoke the test
binary as a subprocess, so each run's captured stdout carries 2 child result lines plus 1 parent line
— 2×10 + 10 = 30, stable across runs. Unrelated to this plan's change.

**Five isolated timed runs of the repaired abort test:** `TIMING_FAILED_RUNS=0`, loop exit status 0.
The counter incremented on two independent conditions per run (non-zero `cargo test` exit code, and a
`test result:` line failing the three-property regex), so a green-but-slow run would have failed the
gate. `finished in` values: **fastest 1.00s, slowest 1.01s** (runs: 1.01, 1.00, 1.00, 1.01, 1.01).
Against the 23.06s pre-fix measurement, and above the ~1s floor the deliberate `poll_response` wait
imposes.

**`scripts/check.sh all` twice, own exit code captured immediately after each invocation:**
`check1 exit=0`, `check2 exit=0`; the gate's final statement tests both codes rather than ending on an
`echo`, so its own status is not masked. Both logs end with `==> check.sh: all OK`. `check.sh` itself
runs under `set -euo pipefail`, so its exit code is the first failing command's.

**No test was re-run after a failure to obtain a green result.** Every failure recorded in this
SUMMARY is a deliberate negative control, not a flake that was retried away.

### How strong this evidence actually is

- **Zero failures in ten runs bounds the per-run failure rate at roughly 30% or less at 95%
  confidence** (rule of three: 3/10). That is a **weak** bound, and it is **not** what makes this fix
  trustworthy. It could not distinguish "fixed" from "three times rarer".
- **The durable guarantee against spawning is Task 1's neutralized PATH**, which makes an agent launch
  *structurally impossible* rather than improbable: with PATH replaced by a directory holding only a
  `git` symlink, `agent_binary_available`'s scan has zero possible matches regardless of which branch
  the test takes.
- **The durable guarantee against silent relocation is the branch-assertion layer** in both tests —
  the seed alone would have been the third patch of this same shape.
- The repeat runs only confirm that **no second, unrelated race remains**.
- **What these runs do not establish:** they were performed on one machine, in one load condition, and
  they say nothing about test binaries outside the `devflow` bin. They also say nothing about whether
  a *future* semantics change is safe — that is what the assertions, not the run count, are for.

## Deviations from Plan

**1. [Interpretation] `agent_free_git_only_path_dir()` bound inside the `ENV_MUTEX` block, not before it**
- **Found during:** Task 1, Step 1
- **Issue:** Step 1's wording asks for "a local that outlives the block". Binding it before the block
  would call the helper *outside* the mutex, where it reads `PATH` to locate `git` — racing any sibling
  test that has PATH swapped, and tripping its `expect("git must be resolvable on PATH to run this test")`.
- **Resolution:** Bound inside the block immediately after the guard, matching the adjacent
  `transition_resets_infra_failures` idiom exactly (which the same step instructs to mirror). The
  `TempDir` still outlives the `handle_validate_outcome` call, which is the property that matters.
- **Commit:** `add79bd`

**2. [Rule 3 — blocking] Doc-comment wording adjusted so an acceptance criterion holds as written**
- **Found during:** Task 1, Step 3
- **Issue:** Step 3 requires extending the test's doc comment; my first wording named
  `last_validate_failure_commit_count`, which made the criterion `rg -n 'last_validate_failure_commit_count'
  ... matches exactly once` match **twice**.
- **Resolution:** Reworded to "the seeded 999.66 forward-progress baseline". The criterion's purpose —
  confirming exactly one seed site — now holds literally, and the doc requirement is still satisfied.
- **Commit:** `add79bd`

**3. [Environment] Task 3 logs written to the session scratchpad instead of `/tmp`**
- Loop logic, exit-code capture and assertions are byte-identical to the plan's verify commands; only
  the log directory differs, per this environment's scratchpad rule. Not load-bearing.

**4. [Process] Tracer feedback gate passed without a human-verify checkpoint**
- Task 1 is `type="tracer"`. The plan is `autonomous: true` with zero checkpoint tasks, and both of
  Task 1's `<verify>` gates passed (plus clippy/fmt), so execution continued to the expansion tasks.
  Recorded here rather than left implicit.

**5. [Expected] Task 3 produced no commit**
- Task 3 is measurement-only by design ("no source changes in this task"); its output is the evidence
  recorded above, committed with this SUMMARY.

## Known Stubs

None. Both changes are additive test edits with executed assertions; nothing was stubbed, skipped, or
deferred, and no `<verify>` went unrun.

## Threat Flags

None. This plan crosses no input boundary; its only new external interaction is *removing* entries
from a subprocess `PATH`. T-33-09 (a unit test reaching `launch_stage`) is now mitigated structurally
as its register row prescribed.

## Out of scope, surfaced not absorbed

`.planning/REQUIREMENTS.md`'s DOGFOOD-01/02 checkboxes still read unchecked/"Pending" despite Phase 33
shipping both. 33-VERIFICATION.md classes this Info-level doc hygiene, not a `gaps:` entry, and the
plan deliberately does not claim it. Left for the operator / phase-completion tooling.
