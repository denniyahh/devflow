---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
plan: 04
subsystem: loop-termination
tags: [999.78, HARDEN-02, WR-01, WR-04, IN-02, D-07, A-11, breaking-change]
status: complete

requires:
  - "devflow_core::mode::consecutive_failures_made_progress"
  - "devflow_core::workflow::{load_state, clear_state, state_path}"
provides:
  - "State::phase_validate_failures (additive, #[serde(default)])"
  - "mode::MAX_PHASE_VALIDATE_FAILURES (additive)"
  - "mode::phase_failure_ceiling_reached (additive)"
  - "Mode::should_gate -> takes the per-phase total (breaking, D-08)"
  - "LoopBackReason (devflow-cli only, IN-02)"
  - "commands::fresh_state_carrying_phase_failures (devflow-cli only, A-11)"
affects:
  - "crates/devflow-core/src/state.rs"
  - "crates/devflow-core/src/mode.rs"
  - "crates/devflow-cli/src/pipeline_outcomes.rs"
  - "crates/devflow-cli/src/pipeline_gate.rs"
  - "crates/devflow-cli/src/commands.rs"

tech-stack:
  added: []
  patterns:
    - "widen a predicate's signature so the compiler enumerates its mirrors"
    - "single-implementation named predicate shared by message and control flow"
    - "mutation control restored from a checksummed backup"
    - "premise assertion that fails first when the mechanism under test is absent"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/mode.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs

decisions:
  - "F-3 implemented as carry-forward: start() reads persisted state for the same phase and copies phase_validate_failures only"
  - "MAX_PHASE_VALIDATE_FAILURES = 10, with a const assertion that it stays strictly above MAX_CONSECUTIVE_FAILURES"
  - "the ceiling read is taken once into `ceiling_gate` and shared by the message clause and the reset"
  - "F-7's third dry-run probe is an ADDITIVE clause, not a third else-if — as an else-if it is unreachable (measured)"
  - "IN-02 carried by a LoopBackReason enum threaded through loop_back_to_code, so the compiler enumerates all 7 call sites"

metrics:
  duration: "39m"
  completed: 2026-08-07

actuals:
  tokens: 18340
  tasks: 3
  commits: 3
---

# Phase 35 Plan 04: Loop-Termination and Baseline Correctness Summary

A Code↔Validate loop that commits a trivial artifact every cycle now reaches a bound. The
per-phase Validate-failure total accumulates independently of the commit count, survives a
`devflow start --force`, gates rather than aborts at ten, and leads the Supervise gate message.

## What Changed

**999.78/WR-01 — the bound.** `State::phase_validate_failures` increments once per recorded
Validate failure, on *both* the measured and the unmeasurable-count arms, with a saturating add.
Nothing in `handle_validate_outcome` resets it — a rising commit count cannot touch it, which is
the entire point: the Code stage's fix command is a GSD command that commits `.planning/`
artifacts on cycles that changed no source, so the streak resets every cycle and
`MAX_CONSECUTIVE_FAILURES` is unreachable in the ordinary case.

**999.78/WR-04 — the message.** The gate context now leads with the cumulative total, names it as
a per-phase quantity, and relegates the streak to a parenthetical:

```
Validation has failed 5 time(s) for this phase (1 in the current consecutive streak) — human review needed.
```

At the ceiling it gains a clause that says explicitly that the run is paused, not killed:

```
 The per-phase ceiling of 10 is reached: this run is paused for a human, not aborted — approve to ship, reject to loop back for another pass, or abort.
```

**D-07 — gate, never abort.** The ceiling adds no abort path. `Advance`, `LoopBack` and `Abort`
are the same three choices an ordinary Validate gate offers, and the ceiling test asserts the
phase's persisted state still exists after the gate fires.

**F-6 — telling a ceiling gate from an ordinary one.** `mode::phase_failure_ceiling_reached` is
the single implementation of the comparison. `handle_validate_outcome` reads it **once** into
`ceiling_gate` and uses that one value for both the message's ceiling clause and the reset, so the
two cannot disagree. Neither is keyed on "a gate fired": in Supervise every Validate gates, so
that condition carries no information.

**The gating predicate was widened, not disjuncted at the call site.** `Mode::should_gate` now
takes the per-phase total. Five tests re-derived the old two-argument expression to mirror the
production decision; a call-site disjunct would have left all five compiling and silently no
longer mirroring anything. `cargo build --workspace --all-targets` enumerated every one.

**IN-02.** A `LoopBackReason` enum threaded through `loop_back_to_code` gives the absent-baseline
case its own `loop_back` reason (`validate_failure_no_commit_baseline` vs `validate_failure`), and
the event now also carries `phase_validate_failures`. The discriminator is read *before* the
baseline write, which would otherwise erase it within the same call.

## The F-3 Persistence Decision, As Implemented

The plan resolved F-3 rather than escalating it, and A-11 requires the choice be stated. It is
implemented as **carry-forward**, and it is genuinely per-phase:

`commands::fresh_state_carrying_phase_failures(project_root, phase, agent, mode)` builds the fresh
`State` and, if `.devflow/state-{NN}.json` for the same phase loads, copies
`phase_validate_failures` **and nothing else**. `start()` calls it in place of the bare
`State::new` at what was `commands.rs:124`. An absent or unreadable file means zero.

The two reset events A-11 demands be *real events*:

| Event | Mechanism | Code added |
|---|---|---|
| Phase completion | `finish_workflow_with_gate_timeout` → `workflow::clear_state` deletes the file, so the next start finds nothing to carry | none — it already did this |
| Operator approval at the ceiling gate | `reset_phase_failures_at_ceiling(state, ceiling_gate)` on `Advance` and `LoopBack` | one call per arm |

A `--force` restart on its own resets nothing. Both events are tested, so `35-VALIDATION.md`'s
Open Risk requirement is discharged as *tested*, not as accepted-not-tested.

## Negative Controls — every one performed, none asserted from reading the fix

Each mutation was applied to a checksummed copy, the failure observed, and the file restored with
`md5sum` verified identical before continuing.

**NC-5 — the ceiling actually bounds the loop.** Removed the ceiling disjunct from
`Mode::should_gate`'s Validate arm and re-ran the trivial-commit test:

```
looping back to Code (10 validate failure(s) this phase, 1 in the current streak)

panicked at pipeline_outcomes.rs:2188:
reaching the per-phase ceiling must fire a Validate gate
test result: FAILED. 0 passed; 1 failed; 289 filtered out
```

The mutated code's own stdout is the evidence: ten cycles, the streak pinned at **1** every single
one, and no gate. That is the unbounded loop 999.78 describes, reproduced.

**NC-6 — the message reports the total.** Reverted the format string to interpolate only
`consecutive_failures`:

```
assertion `left != right` failed: WR-04: the 1st and 5th Supervise gate must not read
identically — that identity IS the defect
  left: "Validation has failed 1 time(s) — human review needed."
 right: "Validation has failed 1 time(s) — human review needed."
test result: FAILED. 0 passed; 1 failed; 289 filtered out
```

The assertion that failed is the required one — the two numbers differing at the later gate — and
the failure output *is* WR-04's complaint verbatim.

**F-6's own control — the clause is keyed on the predicate, not on gating.** The message block
sits inside `if should_gate(..)`, so "keyed on a gate having fired" is literally an unconditional
append. Applied that, in Supervise:

```
panicked: a below-ceiling Supervise gate must NOT carry the ceiling clause — if it does, the
clause is keyed on gating rather than on the predicate: Validation has failed 1 time(s) for
this phase (1 in the current consecutive streak) — human review needed. The per-phase ceiling
of 10 is reached: ...
test result: FAILED. 0 passed; 1 failed; 289 filtered out
```

**T-35-20b — the RESET is keyed on the predicate too.** Made `reset_phase_failures_at_ceiling`
reset unconditionally:

```
assertion `left == right` failed: an ordinary below-ceiling Supervise gate must leave the total
untouched — a reset on every gate would clear it at every failure and the bound would never
accumulate
  left: 0
 right: 3
test result: FAILED. 0 passed; 1 failed; 292 filtered out
```

**A-11's carry-forward control.** Removed the carry-forward from
`fresh_state_carrying_phase_failures`:

```
phase_validate_failures_survive_a_forced_restart:
  assertion `left == right` failed: the per-phase total must be carried across a forced restart
  left: 0 / right: 6

phase_validate_failures_reset_when_the_phase_completes:
  assertion `left == right` failed: premise: while the state file exists, the total IS carried
  left: 0 / right: 8

test result: FAILED. 0 passed; 2 failed; 291 filtered out
```

The second failure is worth naming. The completion test's conclusion is `total == 0` after
`clear_state`, which a no-carry-forward implementation *also* satisfies — it would have passed
vacuously. Its premise assertion is what fails instead, and that is the only reason the control
discriminates at all.

**F-7 — measured, and the plan's specified ordering was wrong.** See the deviation below; the
pre-fix and post-fix `--dry-run` outputs are both recorded there.

## Deviations from Plan

### [Rule 1 - Bug] F-7's third dry-run probe is unreachable as an `else if`

- **Found during:** Task 1, at the acceptance check.
- **Issue:** the plan specifies the three probes in order, the third as an `else if`. For
  `Stage::Validate` that branch can never be taken: in Auto the second probe
  (`should_gate(s, MAX_CONSECUTIVE_FAILURES, 0)`) already returns true, and in Supervise the first
  (`should_gate(s, 0, 0)`) already does. The preview would therefore never name the new gate —
  which is exactly the T-35-20c harm F-7 exists to prevent, reintroduced by the plan's own
  ordering. The acceptance grep (`awk ... | grep -q 'MAX_PHASE_VALIDATE_FAILURES'`) would still
  have passed on the unreachable code, so the grep alone is a proxy for the behaviour.
- **Measured, not inferred.** Built the binary and ran `devflow start --phase 7 --mode auto
  --dry-run`:

  | | Validate line |
  |---|---|
  | plan's ordering | `validate /gsd-validate-phase 7 [GATE after 3 consecutive failures]` |
  | after the fix | `validate /gsd-validate-phase 7 [GATE after 3 consecutive failures] [GATE at 10 validate failures for this phase]` |

- **Fix:** the per-phase probe is a separate, additive clause rather than a third `else if`,
  suppressed where `should_gate(s, 0, 0)` is already true — Ship and Supervise-mode Validate gate
  regardless of any failure count, so naming a failure ceiling there would be noise. The literal
  probe idiom, the reworded *consecutive* label, and the "preview describes the pipeline, not this
  run's position" comment are all as the plan specified.
- **Supervise's line is unchanged (`[GATE]`)** and that is correct: in Supervise the ceiling
  changes nothing about where the run stops.

### [Rule 3 - Blocking] `LoopBackReason` needed a parameter, not a state read

- **Found during:** Task 2.
- **Issue:** deriving the reason inside `prepare_loop_back_to_code` from
  `state.last_validate_failure_commit_count.is_none()` — the smallest change, and the one "composed
  where the existing reason is composed" reads as — does not work. `handle_validate_outcome`'s
  measured arm sets the baseline to `Some(current)` *before* the loop-back runs, so in the
  realistic IN-02 scenario (operator upgrades a binary mid-phase, the next failure measures a real
  count) the distinct reason would never fire. The signal has to be captured at recording time.
- **Fix:** `LoopBackReason` is a parameter on `loop_back_to_code` / `prepare_loop_back_to_code`,
  captured from a single `baseline_absent` binding read ahead of the recording. The compiler
  enumerated all seven call sites; the five that are not Validate failures pass
  `GateResponse`, which makes no baseline claim. The reason string is still composed in exactly
  one place (`LoopBackReason::as_str`), which is the property the plan's wording was protecting.

### [Rule 1 - Bug] Two existing message-asserting tests, not three

- **Found during:** Task 2, first full-suite run.
- **Issue:** the plan says three tests near `:999-1045` assert on the message text. Two assert on
  the failure message (`validate_gaps_does_not_advance_to_ship`,
  `validate_missing_verdict_does_not_advance`); the third asserts on the *pass* message, which
  this plan does not change. Both failures were loud, not silent.
- **Fix:** updated the two failure assertions to `"Validation has failed"`, keeping their intent
  (a message is produced for the given outcome). The third was correctly left alone.

### [Rule 2 - Missing] The saturating test asserts through the message, not the field

- **Found during:** Task 2, while writing the test that Task 3 would later invalidate.
- **Issue:** at `u32::MAX` the ceiling is already reached, so the failure gates — and Task 3's
  reset zeroes the total on the loop-back. A post-call read of `state.phase_validate_failures`
  would be reading the *reset*, not the increment, and would report `0` whether the add saturated
  or wrapped. The two causes are indistinguishable at that observation point.
- **Fix:** the assertion reads the gate message, which is rendered from the incremented value
  before the gate opens: `u32::MAX` if it saturated, `0` if it wrapped. The test asserts both
  directions (contains the max, does not contain `failed 0 time(s)`).

## Stated Limits — what this green run does NOT establish

**The bound is on RECORDED failures, not on wall-clock or cycles.** Nothing here bounds a loop
that fails in a way `handle_validate_outcome` never sees — an agent that hangs, a monitor that
dies, a stage that never reports. 999.85's idle-timeout path is untouched and out of scope.

**Ten is a judgement, not a measurement.** No data was gathered on how many Validate failures a
genuinely-converging phase takes. The const assertion pins only the *relation* to
`MAX_CONSECUTIVE_FAILURES`; the absolute value is the orchestrator's suggestion, adopted.

**The carry-forward is tested at the construction site, not through a real `devflow start`.**
`phase_validate_failures_survive_a_forced_restart` calls
`fresh_state_carrying_phase_failures` — the function `start()` itself calls — not `start()`, whose
body also does git plumbing, agent-binary probes and worktree scaffolding. If a future edit moved
the carry-forward out of `start()`'s path while leaving the helper intact, the test would still
pass. That is a structural property of the harness, not a live defect.

**No end-to-end run.** Every test drives `handle_validate_outcome` directly with a pre-seeded gate
response. No real agent ran, no real monitor spawned, and the `--dry-run` measurements are the only
observations made through the actual binary.

**`scripts/check.sh all` is n=1.** 22 suites, 0 failures, on one run. That supports "this change
does not break the suite"; it does not support any claim about flakiness. No repeated-run stability
measurement was taken for the new tests.

**The ceiling gate's downstream behaviour is unasserted.** The tests assert that a gate fires, that
its context names the ceiling, and that state survives. Nothing asserts what a human's `Advance` at
the ceiling then does beyond the reset — `transition` to Ship is exercised only incidentally.

## For 35-06 to collect

**Breaking:**
- `devflow_core::mode::Mode::should_gate` — signature widened to
  `(self, stage, consecutive_failures, phase_validate_failures)`.

**Additive, non-breaking:**
- `devflow_core::state::State::phase_validate_failures` — a new `pub` field. **Additive because
  `State` carries `#[non_exhaustive]` (`state.rs:33`)**, so no external crate can build it with a
  struct literal, and the field is `#[serde(default)]` so existing state files load. `35-06` does
  not need to re-derive this.
- `devflow_core::mode::MAX_PHASE_VALIDATE_FAILURES` — new `pub` constant, value `10`.
- `devflow_core::mode::phase_failure_ceiling_reached` — new `pub` function.
- `State`'s persisted JSON shape — one additional `#[serde(default)]` key.

`LoopBackReason`, `reset_phase_failures_at_ceiling`, `loop_back_reason` and
`fresh_state_carrying_phase_failures` are all `pub(crate)`/private in `devflow-cli` and are not
public API.

## Verification

| Check | Result |
|---|---|
| `scripts/check.sh all` | **OK** — fmt clean, clippy clean under `-D warnings`, **22 suites, 0 failed** |
| `cargo test -p devflow-core --lib state::` | 23 passed, 0 failed, 542 filtered out |
| `cargo test -p devflow-core --lib mode::` | 15 passed, 0 failed, 550 filtered out |
| `cargo test -p devflow --bin devflow pipeline_outcomes::` | 56 passed, 0 failed, 237 filtered out |
| `cargo test -p devflow --bin devflow` | 292 passed, 0 failed |
| `cargo test -p devflow-core --lib` | 565 passed, 0 failed |
| `cargo build --workspace --all-targets` | succeeds — the compiler found and closed every `should_gate` mirror |
| `rg 'should_gate\(Stage::Validate, state\.consecutive_failures\)' crates/` | no matches — no two-argument form survives |
| `rg '>= MAX_PHASE_VALIDATE_FAILURES' crates/` | **one** match, `mode.rs:207` — the comparison has exactly one implementation |
| `awk '/fn print_dry_run/,/^}/' ... \| sed 's\|//.*\|\|' \| grep -c 'should_gate'` | **3** (comment-stripped; the raw count is 4 and includes comment prose) |
| `awk '/fn handle_validate_outcome/,/^}/' ... \| sed 's\|//.*\|\|' \| grep -q 'phase_failure_ceiling_reached'` | succeeds — the reset and the clause read the shared predicate |
| `rg 'phase_validate_failures' crates/devflow-cli/src/commands.rs` | matches; the surrounding doc comment names both reset events |

Every named test also verified individually with a real `1 passed` line and a non-zero
`filtered out` count:

| Test | Result |
|---|---|
| `state::tests::phase_validate_failures_round_trips_through_serde` | 1 passed; 564 filtered out |
| `state::tests::phase_validate_failures_absent_from_json_defaults_to_zero` | 1 passed; 564 filtered out |
| `mode::tests::phase_failure_ceiling*` (3 tests) | 3 passed; 562 filtered out |
| `pipeline_outcomes::tests::phase_validate_failure_ceiling_gates_despite_trivial_commit_progress` | 1 passed; 289 filtered out |
| `pipeline_outcomes::tests::validate_gate_message_leads_with_the_per_phase_total` | 1 passed; 289 filtered out |
| `pipeline_outcomes::tests::ceiling_clause_appears_only_at_the_ceiling_even_in_supervise_mode` | 1 passed; 289 filtered out |
| `pipeline_outcomes::tests::loop_back_reason_is_distinct_when_no_commit_baseline_exists` | 1 passed; 289 filtered out |
| `pipeline_outcomes::tests::phase_validate_failures_increment_saturates` | 1 passed; 289 filtered out |
| `pipeline_outcomes::tests::phase_validate_failures_reset_on_operator_approval_at_the_ceiling_gate` | 1 passed; 292 filtered out |
| `commands::tests::phase_validate_failures_survive_a_forced_restart` | 1 passed; 292 filtered out |
| `commands::tests::phase_validate_failures_reset_when_the_phase_completes` | 1 passed; 292 filtered out |

The non-zero `filtered out` counts are asserted deliberately: `--exact` on a name matching nothing
exits 0, so `test result: ok` alone is not evidence a test ran.

## Known Stubs

None. No hardcoded empty values, placeholder text, TODO/FIXME markers, `todo!`/`unimplemented!`, or
unwired components were introduced in any of the five modified files. No tests are `#[ignore]`d.
Every mutation used as a negative control was restored from a checksummed backup with `md5sum`
verified identical.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern or trust-boundary schema change
beyond the one additive `#[serde(default)]` state key already enumerated in the plan's threat
register. T-35-16, T-35-17, T-35-18, T-35-19, T-35-20, T-35-20b and T-35-20c are mitigated as
planned; T-35-20c's mitigation changed shape (an additive clause rather than a third `else if`)
because the planned shape was unreachable — measured, recorded above.

## Still open — needs the operator's word, not my assumption

1. **RESOLVED AS PROVISIONAL 2026-08-07 (operator, during 35-verify-work).**
   **`MAX_PHASE_VALIDATE_FAILURES = 10` is unmeasured.** The plan authorised the planner to argue
   the number and it adopted the orchestrator's suggestion. Nothing in this phase establishes how
   many Validate failures a genuinely-converging phase takes, so ten could be too tight (an
   unattended run parks on a gate that a human then rubber-stamps every time) or too loose (ten
   wasted cycles before anyone is summoned).

   **Disposition: keep 10, explicitly provisional, revisit on evidence.** The two alternatives were
   rejected for the same reason — neither produces data. Measuring retroactively is impossible:
   `phase_validate_failures` did not exist before this phase, so there is no history to mine and any
   reconstruction from event logs would be a proxy for the quantity of interest. Picking a different
   number now would be a second guess, not a measurement.

   **Where the evidence will come from.** Phase 35.1's simulated unattended run and Phase 36's live
   run both exercise the Code↔Validate loop and will produce real per-phase Validate-failure counts
   as a side effect. Recorded in both phases' records so the observation is collected rather than
   left to memory. What is pinned today is only the *relation* — a const assertion keeps the ceiling
   strictly above `MAX_CONSECUTIVE_FAILURES` — and that is unaffected by whatever the absolute value
   becomes. The value is already documented as a judgement at `mode.rs:20-41` and in `CHANGELOG.md`'s
   Known Issues, so nothing is silently wrong in the meantime.

## Self-Check: PASSED

Files verified present on disk: `crates/devflow-core/src/state.rs`,
`crates/devflow-core/src/mode.rs`, `crates/devflow-cli/src/pipeline_outcomes.rs`,
`crates/devflow-cli/src/pipeline_gate.rs`, `crates/devflow-cli/src/commands.rs`, and this SUMMARY.

Commits verified in `git log`: `503d9b8`, `cf37bab`, `f468d24`.
