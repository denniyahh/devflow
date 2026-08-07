---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
plan: 05
subsystem: loop-back-dispatch
tags: [999.79, HARDEN-03, A-05, A-12, F-10, F-11, H-1, NC-7, additive]
status: complete

requires:
  - "devflow_core::agent_result::phase_verification_exists"
  - "devflow_core::state::State"
provides:
  - "devflow_core::agent_result::phase_verification_fingerprint (additive)"
  - "State::last_verification_fingerprint (additive, #[serde(default)])"
  - "verification_authored_this_run (devflow-cli only, pure predicate)"
  - "select_loop_back_fix -> takes &mut State (devflow-cli private, not public API)"
affects:
  - "crates/devflow-core/src/agent_result.rs"
  - "crates/devflow-core/src/state.rs"
  - "crates/devflow-cli/src/commands.rs"
  - "crates/devflow-cli/src/pipeline_outcomes.rs"

tech-stack:
  added: []
  patterns:
    - "explicitly-implemented FNV-1a/64 where a cross-process stable hash is required"
    - "pure predicate extracted from a side-effecting selector so a truth table can enumerate it"
    - "two-stub demonstration: the same table must fail under both over-corrections"
    - "source-position assertion with a negative control at the tempting-but-wrong site"
    - "premise assertions that fail first when the fixture does not set up the case claimed"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs

decisions:
  - "fingerprint is FNV-1a/64 written out, not DefaultHasher, whose output is not stable across toolchain versions (T-35-24)"
  - "the artifact path scan is extracted once; phase_verification_exists keeps signature, visibility and behaviour"
  - "the stale/fresh decision is a pure predicate over (current, run_start_baseline) so the four-row table can enumerate it (H-1)"
  - "the baseline is recorded only on the fresh branch, per the plan; the absent-artifact case deliberately leaves it untouched"
  - "F-11's kill window accepted, not closed — no second save_state was added"

metrics:
  duration: "38m"
  completed: 2026-08-07

actuals:
  tokens: 9509
  tasks: 3
  commits: 5
---

# Phase 35 Plan 05: Loop-Termination and Baseline Correctness Summary

`{N}-VERIFICATION.md` now goes stale. A `devflow start --phase N --force` re-run no longer
inherits the previous run's committed verdict: the loop-back selector compares the artifact's
content fingerprint against a baseline recorded at run start, and an artifact unchanged since
then dispatches a full execute instead of a `--gaps-only` pass against zero matching plans.

## What Changed

**999.79 / criterion 3 — the rule.** `select_loop_back_fix` no longer asks "does the artifact
exist". It asks whether the artifact was authored during THIS run, by comparing
`agent_result::phase_verification_fingerprint(evidence_root, phase)` against
`state.last_verification_fingerprint`. The decision lives in that one function — the three call
arms (`:416` ambiguous-gate, `:564` failure-gate, `:580` plain-Failed tail) did not each grow a
copy. Verified: exactly **2** production occurrences of `last_verification_fingerprint` in
`pipeline_outcomes.rs`, both inside the selector; the other 5 are in the test module.

**The planner's departure from the backlog entry is what shipped.** 999.79 proposes comparing a
recorded plan count against the phase's current plan set — indicative, and a false negative
whenever a replan happens to produce the same count. A content fingerprint is exact. ROADMAP
criterion 3's "or equivalent" permits it.

**A-05 — where the capture goes, and why the obvious site is wrong.** The capture sits in
`start()` at `commands.rs:344`, after the `if worktree { ... }` fork assigns
`state.worktree_path` at `:287`. The artifact lives under the *evidence root*, which in worktree
mode is created by `ensure_phase_worktree` inside that fork — it does not exist when
`fresh_state_carrying_phase_failures` builds the state at `:167`. Capturing there would record
"absent" for every worktree run and make the very first freshness check read as fresh, which is
the failure direction the baseline exists to prevent.

**A-12 branch (a) taken.** The baseline is recorded *before* the Validate agent runs, so the
agent's rewrite registers as a change. The alternative — recording only when Validate finishes —
makes the first check see no baseline and then compares an unchanged stale hash forever, a silent
permanent regression of Phase 33. The two-direction test is the only thing that discriminates the
two branches, which is why it is mandatory rather than nice-to-have.

**T-35-24 — the hash is written out, not borrowed.** FNV-1a/64, pinned by its two published
constants. `std::collections::hash_map::DefaultHasher` does not guarantee output stability across
toolchain versions, and this value is persisted by `devflow start` and compared by a later
`devflow advance`. An unstable algorithm would read as "changed" after a Rust upgrade — the
**fail-open** direction, dispatching gaps-only exactly where a full execute was correct.

**F-10 — the predicted borrow conflict does not occur, and this was checked rather than assumed.**
`select_loop_back_fix` gained `state: &mut State` as its last parameter and
`cargo build --workspace --all-targets` succeeded on the first attempt, with `evidence_root` left
as an owned `PathBuf` and no restructuring of either gate-response match. The second review pass's
C3 prediction was wrong; the plan's re-derivation was right.

## Negative Controls — every one performed, none asserted from reading the fix

### NC-7's automated half: the truth table fails under BOTH over-corrections

The plan requires the four-row table to be shown to catch *both*, not one. Mutations applied to
the shipped tree from a checksummed backup (`md5sum` verified identical on restore).

| Stub | Result | Row that failed |
|---|---|---|
| `verification_authored_this_run` ⇒ **always stale** (`false`) | `test result: FAILED. 0 passed; 1 failed; 295 filtered out` | `row 2: an artifact existing where the baseline recorded none was authored this run` |
| `verification_authored_this_run` ⇒ **always fresh** (`true`) | `test result: FAILED. 0 passed; 1 failed; 295 filtered out` | `row 1: an absent artifact is never 'authored this run'` |

**The two stubs fail on different rows.** That is the exhaustiveness evidence — a table that
failed the same row under both would be testing one thing twice. Restored, re-run: `59 passed; 0
failed`.

### NC-7's performed mutation: the direction pair fails asymmetrically

Same always-stale mutation, applied to the shipped tree, against the two direction tests:

```
stale_verification_artifact_dispatches_full_execute ... ok
  test result: ok. 1 passed; 0 failed; 295 filtered out

verification_written_this_run_dispatches_gaps_only ... FAILED
  assertion `left == right` failed: an artifact whose content changed since this run
  started was authored by THIS run's Validate agent and must still reach the gaps-only path
    left: String("FullExecute")
   right: "GapsOnly"
  test result: FAILED. 0 passed; 1 failed; 295 filtered out
```

**The asymmetry is the evidence.** If both had still passed, the pair would be measuring nothing
and criterion 3 would have no coverage.

**The mirror was also run, and it inverts exactly as it must.** Under the always-*fresh* stub the
asymmetry reverses: `stale_verification_artifact_dispatches_full_execute` **FAILED** while
`verification_written_this_run_dispatches_gaps_only` passed (`1 passed; 295 filtered out`). Each
direction test is therefore discriminating, not merely co-passing.

### The always-stale rule also breaks Phase 33 — measured, not argued

The RED step used the always-stale stub, and it did not only fail the new fresh test. It failed
Phase 33's two pre-existing gaps-only tests as well:

```
genuine_gaps_loop_back_still_issues_gaps_only ......... FAILED (left: "FullExecute")
worktree_mode_genuine_gaps_loop_back_issues_gaps_only . FAILED (left: "FullExecute")
verification_freshness_truth_table_is_exhaustive ...... FAILED (row 2)
verification_written_this_run_dispatches_gaps_only .... FAILED
test result: FAILED. 55 passed; 4 failed; 237 filtered out
```

That is the concrete cost of the over-correction this plan's `critical_risk` warns about, observed
rather than reasoned about: an always-stale rule reverts what Phase 33 built, and it would pass
every test asserting only the stale direction.

### Task 1's constant-implementation control

The RED step's `phase_verification_fingerprint` returned a **constant** for every existing
artifact. `phase_verification_fingerprint_differs_when_content_differs` failed on exactly the
assertion that matters:

```
assertion `left != right` failed: different bytes must produce different fingerprints — a
constant implementation would report every re-authored artifact as unchanged
  left: 0 / right: 0
test result: FAILED. 2 passed; 1 failed; 566 filtered out
```

Its companion half (identical bytes ⇒ identical values) is the control against the opposite error
— a value derived from a timestamp or an inode would mark every artifact fresh forever.

### Task 1's serde control

`State::last_verification_fingerprint` was `#[serde(skip)]` in the RED step:
`last_verification_fingerprint must appear in persisted JSON` — `FAILED. 1 passed; 1 failed; 567
filtered out`. The key-presence assertion precedes the value round-trip deliberately: a field that
never reaches disk still passes a naive in-memory round trip, and this baseline is written by one
process and read by another.

### The A-05 source-position assertion has a negative control

The check is "the capture line is strictly greater than the worktree-path assignment line".

| Site | Line | `> 287`? |
|---|---|---|
| the capture, as shipped | 344 | **yes** — required |
| state construction (`fresh_state_carrying_phase_failures`), the tempting-but-wrong site A-05 rules out | 167 | **no** — correct: the check discriminates |

Without the second row the assertion could not fail and would establish nothing.

### The comment-stripping controls

Two greps in this plan's acceptance would have measured documentation rather than code — the trap
CLAUDE.md records from 35-01. Both are reported with their raw and stripped counts, and the
stripper itself has a control:

| Measure | Raw | Comment-stripped |
|---|---|---|
| `DefaultHasher` in `agent_result.rs` | **1** | **0** |
| `last_verification_fingerprint` in `commands.rs` | 1 | **1** |

The single raw `DefaultHasher` hit is my own doc comment explaining why it is *not* used — the
plan's action requires that reasoning be documented at the function, and documenting it means
naming the type. The plan's literal `! rg -q 'DefaultHasher'` therefore fails on prose; the
comment-stripped count of **0** is the real measurement. Stripper control: `wrapping_mul` = 1 and
`phase_verification_fingerprint` = 9 in the same stripped file, so the stripper is not simply
deleting everything.

## F-11 — the accepted window, with its direction stated correctly

The selector's baseline update does **not** reach disk through the `save_state` earlier in
`handle_validate_outcome` — that one runs before the selector. It is persisted a few statements
later by `prepare_loop_back_to_code`'s own `save_state`, reached via `loop_back_to_code` on the
line immediately after each selector call.

**The failure direction is fail-OPEN, not fail-safe.** If a process is killed in that interval, a
later same-run loop-back compares the artifact against the *older* baseline. An artifact unchanged
since then still differs from that older value, so it reads **fresh** and dispatches `--gaps-only`
where a full execute was correct — the exact 999.79 direction this plan exists to close. A
fail-safe framing would have justified leaving it alone; it is not fail-safe.

**Accepted, not closed.** The interval contains no blocking wait (the gate wait *precedes* the
mutation) and spans a handful of statements. The only fix is a second `save_state` on every
loop-back, doubling the persistence cost of the common path. None was added.

**F-11's testable half IS closed.** The real risk is not the kill — it is a selector that mutates a
value nothing ever writes out, which would lose the update every time rather than only under a
kill. `verification_written_this_run_dispatches_gaps_only` sub-case 1 reads the phase's persisted
state file back from disk after the loop-back completed and asserts the selector's new baseline is
present in it. Reading the in-memory `State` would not have distinguished those two.

## Which direction test round-trips through persisted state

**Sub-case 1 of `verification_written_this_run_dispatches_gaps_only` does.** It writes state via
`workflow::save_state`, reloads it with `workflow::load_state`, asserts the run-start baseline
survived the round trip (a premise assertion, so a broken fixture fails *first* rather than
vacuously passing), drives `handle_validate_outcome` on the reloaded value, and then re-reads the
file after the loop-back.

**Nothing else does, and that is a real limit.** `stale_verification_artifact_dispatches_full_execute`
and sub-case 2 mutate an in-memory `State`. Neither establishes anything about multi-process
behaviour on its own; they establish the dispatch decision only.

## Stated Limits — what this green run does NOT establish

**HARDEN-03 unclassified — the rule cannot establish provenance.** It keys on content change
alone. An artifact whose bytes change for any reason other than this run's Validate agent — a
mid-run branch switch, a worktree merge-back, an operator editing the file — reads as
authored-this-run and dispatches `--gaps-only`, which is the failure direction HARDEN-03 exists to
prevent. Real provenance would need the artifact to carry a run identifier; that is a larger change
than 999.79 asks for and was not attempted.

**No test drives capture-then-compare inside a single `devflow start --force` run.** The capture
site is pinned **only** by the source-position assertion above. `start()` does git plumbing, base-ref
currency checks, agent-binary probes and worktree scaffolding, none of which a unit test can stand
up; the plan anticipated this and accepted it. A future edit that moved the capture out of
`start()`'s path while leaving the expression intact would still satisfy the grep. This is the same
structural gap 35-04 recorded for its carry-forward.

**The four-row table tests the predicate, not the wiring.** It would still pass if
`select_loop_back_fix` stopped calling the predicate entirely. That gap is what NC-7's performed
mutation covers, and that mutation is a one-time act — it does not re-run on `cargo test`. The two
halves are complementary and neither subsumes the other.

**`scripts/check.sh all` is n=1.** 22 suites, `all OK`, 0 failed, on one run. That supports "this
change does not break the suite"; it says nothing about flakiness. No repeated-run stability
measurement was taken for the new tests.

**No end-to-end run.** Every test drives `handle_validate_outcome` directly. No real agent ran, no
monitor spawned, no actual `--force` re-run was performed against a real repository.

**FNV-1a is not collision-resistant and no security property is claimed.** It is change detection
over a planning document already committed to the repository. Anyone who can write the artifact can
already write whatever verdict they like into it.

## Deviations from Plan

### [Rule 1 - Bug] The plan's `! rg -q 'DefaultHasher'` check measures comment prose

- **Found during:** Task 1 acceptance checks.
- **Issue:** the plan's action requires documenting *why* the standard library's default hasher is
  unsuitable, at the function. Documenting that reasoning means naming the type, which makes the
  literal `rg` check fail against a correct implementation.
- **Fix:** kept the doc comment (the plan asked for it) and reported the comment-stripped count as
  the real measurement — **raw 1, stripped 0** — with a control proving the stripper is not
  vacuous. Both numbers recorded above rather than only the flattering one. This is the same trap
  CLAUDE.md records from 35-01, in a second plan.

### [Rule 3 - Blocking] The plan's line anchors were stale

- **Found during:** Task 3.
- **Issue:** the plan cites `select_loop_back_fix` at `:315` with call sites at `:389`, `:441` and
  `:454`, and `phase_verification_exists` at `:2654`. 35-04 merged into the base first and shifted
  all of them.
- **Fix:** re-derived before editing, as the plan instructs. Actual: selector at `:316` (now
  `:329` after the added comment block), call sites at `:416`, `:564`, `:580`;
  `phase_verification_exists` at `:2720`. Loud, not silent — nothing depended on the numbers.

### [Rule 2 - Missing] `phase_verification_exists` now has no in-workspace caller

- **Found during:** Task 3.
- **Issue:** the selector was its only caller, and the selector now calls
  `phase_verification_fingerprint` instead. `phase_verification_exists` is a `pub` item of
  `devflow-core`, so neither rustc nor clippy warns.
- **Decision:** kept, per the plan's explicit surface choice — its signature, visibility and
  behaviour are unchanged, so D-08's possible "third public-API change" does not materialise from
  this plan, and its existing test passes byte-unchanged. Flagged here because a `pub` function
  with zero callers is the kind of thing a later reader will assume is live. Its logic is not dead:
  both it and the fingerprint now route through the same extracted `phase_verification_path`.

## Verification

| Check | Result |
|---|---|
| `scripts/check.sh all` | **`==> check.sh: all OK`** — fmt clean, clippy clean under `-D warnings`, 22 suites, **0 failed** |
| `cargo test -p devflow-core --lib` | 569 passed, 0 failed |
| `cargo test -p devflow --bin devflow` | 296 passed, 0 failed |
| `cargo test -p devflow --bin devflow pipeline_outcomes::` | **59 passed, 0 failed**, 237 filtered out — both directions green in the SAME run |
| `cargo build --workspace --all-targets` | succeeds; F-10's predicted borrow conflict did not occur |
| production occurrences of `last_verification_fingerprint` in `pipeline_outcomes.rs` | **2**, both in `select_loop_back_fix` — not repeated across the three arms |
| production occurrences in `commands.rs` (comment-stripped) | **1** — the capture appears exactly once, not once per fork branch |
| capture line (344) vs worktree assignment (287) | 344 > 287 ✓; negative control (state construction, 167) correctly fails |
| different-roots note above `select_loop_back_fix` | 30-line extraction, **29 doc lines byte-identical**; the only diff is the signature line the plan widens |
| CR-01 note above `handle_validate_outcome` | 12-line extraction, `diff` exit **0** — unedited |
| owned-`PathBuf` comment + binding (F-10) | 7-line extraction, `diff` exit **0** — `evidence_root: PathBuf` still owned, comment unedited |
| existing `phase_verification_exists` test body vs base | 22-line extraction each, `diff` exit **0** — byte-identical, and passes in the same run |
| `DefaultHasher` in `agent_result.rs` (comment-stripped) | **0** (raw 1, comment only) |

Each named test verified individually with a real `1 passed` line and a **non-zero** `filtered out`
count — `--exact` on a name matching nothing exits 0, so `test result: ok` alone is not evidence a
test ran:

| Test | Result |
|---|---|
| `agent_result::tests::phase_verification_fingerprint_differs_when_content_differs` | 1 passed; 568 filtered out |
| `agent_result::tests::phase_verification_fingerprint_is_none_when_the_artifact_is_absent` | 1 passed; 568 filtered out |
| `state::tests::last_verification_fingerprint_round_trips_through_serde` | 1 passed; 568 filtered out |
| `state::tests::last_verification_fingerprint_absent_from_json_defaults_to_none` | 1 passed; 568 filtered out |
| `pipeline_outcomes::tests::stale_verification_artifact_dispatches_full_execute` | 1 passed; 295 filtered out |
| `pipeline_outcomes::tests::verification_written_this_run_dispatches_gaps_only` | 1 passed; 295 filtered out |
| `pipeline_outcomes::tests::verification_freshness_truth_table_is_exhaustive` | 1 passed; 295 filtered out |

## For 35-06 to collect

**Breaking: none.** This plan adds no public-surface break, so D-08's release cut is unaffected by
it.

**Additive, non-breaking:**

- `devflow_core::agent_result::phase_verification_fingerprint` — new `pub` function returning
  `Option<u64>`.
- `devflow_core::state::State::last_verification_fingerprint` — new `pub` field. **Additive rather
  than breaking because `State` carries `#[non_exhaustive]` (`state.rs:33`)**, so no external crate
  can build it with a struct literal, and it is `#[serde(default)]` so existing state files load.
  35-06 does not need to re-derive this — it is the same argument 35-04 recorded for
  `phase_validate_failures`.
- `State`'s persisted JSON shape — one additional `#[serde(default)]` key.

`phase_verification_exists` is **unchanged** in signature, visibility and behaviour, and must not
be listed as a change of any kind. `phase_verification_path`, `verification_authored_this_run` and
the widened `select_loop_back_fix` are all private and are not public API.

## Known Stubs

None. No hardcoded empty values, placeholder text, TODO/FIXME markers, `todo!`/`unimplemented!`, or
unwired components were introduced in any of the four modified files. No tests are `#[ignore]`d.
Every mutation used as a negative control was restored from a checksummed backup with `md5sum`
verified identical (`c3d7f2b2b1aa1244b64b76386ad6eaff`) and `git status --short` empty afterwards.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern or trust-boundary schema change
beyond the one additive `#[serde(default)]` state key already enumerated in the plan's threat
register. T-35-21, T-35-22, T-35-22b, T-35-23 and T-35-24 are mitigated as planned; T-35-25 and
T-35-SC are accepted as planned (this plan installs no packages).

## Still open — needs the operator's word, not my assumption

1. **The freshness rule's inability to establish provenance is a live, unclosed gap, not a
   theoretical one.** The plan records it as `must_haves` "HARDEN-03 unclassified" with
   `verification: backstop`, so it ships knowingly. But the concrete scenario — a worktree
   merge-back changing the artifact's bytes mid-run, after which the rule reads it as
   authored-this-run and dispatches `--gaps-only` — is the exact failure HARDEN-03 exists to
   prevent, reached by a different route. Whether that is acceptable for the release cut, or wants
   a run identifier embedded in the artifact, is a scope decision I should not settle by inference.

2. **The tracer feedback gate was run as the autonomous variant, not the interactive one.**
   Same condition 35-01 reported and the operator has not yet ruled on: `workflow.auto_advance` is
   absent from `.planning/config.json` (so the executor spec's detection reads "not auto") while
   `workflow.auto_mode` is `true`, and I was spawned into a worktree with no channel to receive a
   checkpoint answer. I ran the gate's substantive check — re-running the verification end-to-end —
   rather than emitting a checkpoint nobody could answer. Re-surfaced here because it is still open.

## Self-Check: PASSED

Files verified present on disk: `crates/devflow-core/src/agent_result.rs`,
`crates/devflow-core/src/state.rs`, `crates/devflow-cli/src/commands.rs`,
`crates/devflow-cli/src/pipeline_outcomes.rs`, and this SUMMARY.

Commits verified in `git log`: `167dcc9`, `9b731f0`, `980c8d6`, `e5aaaae`, `2a70a0f`.

Per the orchestrator's instruction for parallel wave execution, `STATE.md` and `ROADMAP.md` were
**not** modified by this executor — `git diff --stat` against the wave base touches four source
files and this SUMMARY only.
