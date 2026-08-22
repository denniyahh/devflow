---
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
plan: 03
subsystem: validate-classifier
tags: [security, trust-boundary, validate, exhaustive-match, d-06, d-08, 999.74]
status: complete

requires:
  - "crates/devflow-core/src/agent_result.rs — AgentStatus' seven variants and AgentResult::decided_by_layer"
  - "crates/devflow-core/src/outcome_policy.rs — decide_action's wildcard-free match, the precedent this rewrite follows"
  - "plan 34-01 — the graft fix whose corrected reachability record this plan's doc comment reproduces"
provides:
  - "classify_validate_outcome as a match over (layer0, status, verdict) with every AgentStatus variant named in the status position"
  - "classify_validate_outcome_sweeps_all_forty_two_cells — D-08's full matrix sweep with the visited count pinned to 42"
  - "verdict_pass_classifies_as_passed_regardless_of_layer — NC-1"
  - "external_verify_gaps_is_ambiguous_only_when_layer0_decided — NC-2 with its paired mirror"
  - "external_verify_absent_verdict_is_ambiguous_only_when_layer0_decided — NC-3 with its paired mirror"
  - "grafted_failure_shape_gates_instead_of_shipping — criterion 4's downstream routing half"
  - "non_success_status_never_classifies_as_passed_even_with_verdict_pass — the RED gate, and the only cell whose classification changed"
  - "classifier_fixture — the matrix fixture that sets decided_by_layer explicitly in both directions"
affects:
  - "any future AgentStatus variant — it is now an E0004 at this call site rather than a silent join"

tech-stack:
  added: []
  patterns:
    - "positional wildcard ban: `_` permitted in the layer and verdict positions, forbidden in the status position, in BOTH destination directions"
    - "paired mirrors inside one test function so a discrimination claim cannot be split and half-deleted"
    - "recorded compile experiment as a negative control, with its own control: the pre-rewrite match compiled cleanly against the same eighth variant"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs

decisions:
  - "The RED gate is its own test (non_success_status_never_classifies_as_passed_even_with_verdict_pass) and its own commit; it is the only assertion in this plan that was red before the rewrite"
  - "NC-4b required temporarily satisfying devflow-core's own two exhaustive matches so the check could reach devflow-cli; both were reverted and verified by blob hash"
  - "grafted_failure_shape_gates_instead_of_shipping asserts the routing half only in the gating direction; the Passed -> Ship direction is left to the pre-existing external_verify_agreement_advances_to_ship, which needs ENV_MUTEX and a neutralized PATH"

metrics:
  duration: "~35 min"
  completed: 2026-08-05

actuals:
  tokens: 5846
  tasks: 3
  commits: 4
---

# Phase 34 Plan 03: The classify_validate_outcome Exhaustive-Match Rewrite — Summary

`classify_validate_outcome`'s `Passed` arm is now gated on the derived `AgentStatus`
structurally: all seven variants are named in the status position of a wildcard-free match, so an
eighth is a compile error rather than a silent join in either direction. ROADMAP criterion 3 is
closed. Criterion 4 is **not** closed by this plan — see the explicit limits section below.

## What Changed

**The production change** (`crates/devflow-cli/src/pipeline_outcomes.rs:228-269`) replaces

```rust
let external = result.decided_by_layer == Some(0) && result.status == AgentStatus::Success;
match (external, result.verdict) {
    (_, Some(Verdict::Pass)) => ValidateOutcome::Passed,
    ...
    _ => ValidateOutcome::Failed,
}
```

with a layer-only normaliser and a five-arm match over `(layer0, status, verdict)` in which the six
non-`Success` statuses share one named or-pattern arm. `let external` is gone entirely — it was
D-06's named trap, folding an `AgentStatus` equality test back into the normaliser and conflating
Layer-1-Success with Layer-0-Failed.

**Runtime delta is one cell.** `(non-Success, Some(Verdict::Pass))` classified as `Passed` before
and classifies as `Failed` now. Every other destination is byte-identical. That cell is unreachable
in production — `decide_action` gates every non-`Success` status before this function is called —
so the change is defence in depth, exactly as D-05's amendment describes.

## The Pre-Fix Red State

`non_success_status_never_classifies_as_passed_even_with_verdict_pass` was committed on its own
(`5a1cb52`) and observed failing against the superseded match before the rewrite was written:

```
thread 'pipeline_outcomes::tests::non_success_status_never_classifies_as_passed_even_with_verdict_pass'
panicked at crates/devflow-cli/src/pipeline_outcomes.rs:1176:13:
assertion `left == right` failed: an agent-written verdict:pass must not outrank the derived status Failed
  left: Passed
 right: Failed
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 272 filtered out
```

`left: Passed` is the trust inversion in the classifier's own terms. The non-zero `filtered out`
count confirms the selector matched a real test rather than exiting 0 on an empty set.

## NC-4 — The Two Exhaustiveness Compile Experiments

**Isolation branch taken: isolated worktree — NC-4b ran here.** `git worktree list` at the time of
the decision:

```
/var/home/denniyahh/Github/devflow                                           4f5a177 [feature/phase-34]
/var/home/denniyahh/Github/devflow/.claude/worktrees/agent-a130c7031d0dcf947 616e303 [worktree-agent-a130c7031d0dcf947] locked
/var/home/denniyahh/Github/devflow/.claude/worktrees/agent-a6423c30adeaa4c00 61c8e67 [worktree-agent-a6423c30adeaa4c00] locked
```

Same-wave plan 34-02 holds `agent-a6423c30adeaa4c00`, a separate tree at a separate commit. No edit
of mine could reach it and none of its edits could reach me, so the concurrency hazard the plan's
isolation gate guards against does not arise. NC-4b was **not** deferred.

### NC-4a — deleting a named status arm

Removed `| AgentStatus::IdleTimeout` from the six-variant arm, then `cargo check -p devflow`
(exit 101):

```
error[E0004]: non-exhaustive patterns: `(true, AgentStatus::IdleTimeout, _)` and `(false, AgentStatus::IdleTimeout, _)` not covered
   --> crates/devflow-cli/src/pipeline_outcomes.rs:230:11
    |
230 |     match (layer0, result.status, result.verdict) {
    |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ patterns `(true, AgentStatus::IdleTimeout, _)` and `(false, AgentStatus::IdleTimeout, _)` not covered
    |
    = note: the matched value is of type `(bool, AgentStatus, Option<Verdict>)`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern, a match arm with multiple or-patterns as shown, or multiple match arms
    |
266 ~         ) => ValidateOutcome::Failed,
267 ~         (true, AgentStatus::IdleTimeout, _) | (false, AgentStatus::IdleTimeout, _) => todo!(),
    |

For more information about this error, try `rustc --explain E0004`.
error: could not compile `devflow` (bin "devflow") due to 1 previous error
```

### NC-4b — adding an eighth `AgentStatus` variant

Added a temporary `AgentStatus::Nc4bProbe` to `crates/devflow-core/src/agent_result.rs`.

**An extra step was required that the plan did not anticipate.** `devflow-core`'s own two
exhaustive matches — `AgentStatus::as_wire_str` (`agent_result.rs:95`) and `decide_action`
(`outcome_policy.rs:39`) — raise E0004 first, so `cargo check -p devflow` never reached
`devflow-cli`. Both were temporarily given an arm so the crate compiled and the check could reach
the match under test. That is itself a finding worth recording: **an eighth variant is a compile
error in three independent places, two of them in `devflow-core`.** With core satisfied
(exit 101):

```
error[E0004]: non-exhaustive patterns: `(true, AgentStatus::Nc4bProbe, _)` and `(false, AgentStatus::Nc4bProbe, _)` not covered
   --> crates/devflow-cli/src/pipeline_outcomes.rs:230:11
    |
230 |     match (layer0, result.status, result.verdict) {
    |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ patterns `(true, AgentStatus::Nc4bProbe, _)` and `(false, AgentStatus::Nc4bProbe, _)` not covered
    |
    = note: the matched value is of type `(bool, AgentStatus, Option<Verdict>)`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern, a match arm with multiple or-patterns as shown, or multiple match arms
    |
267 ~         ) => ValidateOutcome::Failed,
268 ~         (true, AgentStatus::Nc4bProbe, _) | (false, AgentStatus::Nc4bProbe, _) => todo!(),
    |

For more information about this error, try `rustc --explain E0004`.
error: could not compile `devflow` (bin "devflow") due to 1 previous error
```

### NC-4b's own negative control (run beyond the plan's requirements)

An E0004 appearing after a variant is added does not by itself establish that the **rewrite** is
what catches it. With `Nc4bProbe` still present, the pre-rewrite `pipeline_outcomes.rs` was restored
and `cargo check -p devflow` run again: **exit 0**. The superseded `_ => ValidateOutcome::Failed`
arm absorbed the eighth variant silently and compiled clean. That is the weakness this plan
eliminates, demonstrated rather than asserted, and it attributes the two E0004s above to the rewrite
rather than to the variant addition.

### Reverts, verified against each file's own pre-mutation blob

| File | Pre-mutation | Post-revert | Match |
|---|---|---|---|
| `crates/devflow-core/src/agent_result.rs` | `f37e3af8e983f3e83038f0b7719f5843e58b2db3` | `f37e3af8e983f3e83038f0b7719f5843e58b2db3` | yes |
| `crates/devflow-core/src/outcome_policy.rs` | `3bdcab44d66922410d7b4fff8485a33953d53a47` | `3bdcab44d66922410d7b4fff8485a33953d53a47` | yes |
| `crates/devflow-cli/src/pipeline_outcomes.rs` | (task-3 good state) | `diff` exit 0 | yes |

`git status --porcelain` was empty immediately after the reverts. That check discriminates — the
same command printed ` M crates/devflow-cli/src/pipeline_outcomes.rs` at four other points in this
session. **Neither `agent_result.rs` nor `outcome_policy.rs` appears in any commit from this plan.**

### Secondary structure check — the arm count

The rewritten match has **5 arms** naming **7 distinct `AgentStatus` variants** across **10
pattern occurrences**.

Derivation: `rg -n 'fn classify_validate_outcome\(' -A 41 <file> | rg -c '=> ValidateOutcome::'`
returns 5 (arms). Piping the same window through `rg -v '^\s*[0-9]+[-:]\s*//'` (to exclude the
inline comments, one of which legitimately names `AgentStatus::RateLimited`) and then
`rg -o 'AgentStatus::[A-Za-z]+' | sort | uniq -c` yields `Success` × 4 and each of `Failed`,
`Unknown`, `RateLimited`, `ResourceKilled`, `AgentUnavailable`, `IdleTimeout` × 1 — 7 distinct
names, 10 occurrences. This is a weaker check than the two E0004s and is recorded as a secondary
cross-check, not as the exhaustiveness evidence.

## Mutation Controls for the Sweep (Task 2)

A 42-cell sweep reported green with no mutation control measures the sweep's ability to agree with
itself. Both required controls were run and reverted.

| Mutation | Effect | Tests observed RED |
|---|---|---|
| Deleted the `(true, Success, Some(Gaps))` `Ambiguous` arm, widening the `Failed` arm to absorb the cell | that cell falls to `Failed` | `external_verify_gaps_is_ambiguous_only_when_layer0_decided` (exit 101) and `classify_validate_outcome_sweeps_all_forty_two_cells` (exit 101, `right: Ambiguous("external verification passed but the agent reported gaps")`) |
| Changed `classifier_fixture` to set `decided_by_layer: None` for the `layer0 = true` case | `layer0` collapses to false in half the matrix | `external_verify_gaps_is_ambiguous_only_when_layer0_decided` (exit 101) and `external_verify_absent_verdict_is_ambiguous_only_when_layer0_decided` (exit 101) |

The second is the T-34-03-03 failure mode reproduced deliberately: it is the omission that would
have left both `Ambiguous` arms unexercised and a regression deleting them both green. Both
mutations were reverted with `diff` exit 0 against the saved good state, and the full module re-run
green afterwards.

## Verification Results

| Check | Result |
|---|---|
| `pipeline_outcomes::` module selector | `46 passed; 0 failed`, 232 filtered out |
| `classify_validate_outcome_sweeps_all_forty_two_cells --exact` | `1 passed`, 276 filtered out |
| `verdict_pass_classifies_as_passed_regardless_of_layer --exact` (NC-1) | `1 passed`, 276 filtered out |
| `external_verify_` selector | `6 passed; 0 failed`, 271 filtered out (4 pre-existing + NC-2 + NC-3) |
| `grafted_failure_shape_gates_instead_of_shipping --exact` | `1 passed`, 277 filtered out |
| Pre-existing `external_verify_*` tests, unmodified | all 4 pass; none needed editing |
| `let external` in the production match window | 0 (control: 1 on the pre-rewrite file) |
| `let layer0` in the production match window | 1 |
| `_ =>` in the production match window, comments filtered | 0 (control: 1 on the pre-rewrite file) |
| `pub(crate) fn classify_validate_outcome\(` | 1 — visibility unchanged |
| `cargo build -p devflow` | exit 0 |
| `cargo clippy -p devflow --all-targets -- -D warnings` | exit 0 (captured directly, not via a pipeline) |
| `scripts/check.sh all` | **exit 0** (captured directly) |

**Window size used for the grep criteria: `-A 41`, not `-A 45`.** The rewritten function spans
lines 228-269 — 42 lines including the signature — so `-A 41` reaches the closing brace exactly.
`-A 45` overshoots by three lines into the following `ValidateResult` doc comment; those lines are
`///` and would be removed by the comment filter anyway, but the narrower window was used so the
criterion is read off the function and nothing else. The plan's anchoring instruction
(`fn classify_validate_outcome\(` with the literal open paren) was followed throughout; the bare
substring would also match `classify_validate_outcome_sweeps_all_forty_two_cells`.

### What these results do NOT establish

- **They do not close the Validate trust boundary.** This plan closes criterion 3. Criterion 4's
  graft fix is plan 34-01's, and **neither alone closes the pair** — the classifier rewrite passes
  cleanly over the exploit precisely because the status is laundered upstream before it runs.
- **`46 passed` is a module-level pass, not evidence about the classifier.** Only the six named
  tests bear on D-06/D-08; the rest is regression surface that was already green.
- **The two E0004 diagnostics establish that the enumeration is compiler-enforced. They say nothing
  about whether the destinations are correct.** Correctness of the 42 destinations rests on the
  sweep, and the sweep's expected-outcome table is a second hand-written statement of the same
  decision table — which is why the mutation controls, not the sweep's green, are the load-bearing
  evidence there.
- **`grafted_failure_shape_gates_instead_of_shipping` exercises the routing half in one direction
  only.** It drives the real `handle_validate_outcome` for the `Ambiguous` shape and asserts it does
  not reach `Stage::Ship`. The opposite-result case — `Passed` reaching `Stage::Ship` through the
  same function — is the pre-existing `external_verify_agreement_advances_to_ship`, which passes and
  which is what makes the `assert_ne!` non-vacuous. That control lives in a sibling test rather than
  inside this one; reproducing it here would require `ENV_MUTEX` and a neutralized `PATH`.
- **Nothing here establishes that a real agent ever emits a self-contradictory `DEVFLOW_RESULT`
  marker.** See the flagged assumption below.

## Flagged Assumption Carried Forward — Spec-Less Probe Row 4

The plan carries DOGFOOD-04's edge-probe row as `unclassified` / **`unresolved`**, and it remains
unresolved. The 42-cell sweep plus NC-4's two E0004 controls are this plan's strongest available
edge coverage for the requirement — every input combination has a decided destination and the
enumeration is compiler-enforced — but they do not resolve the open question, which is whether a
real agent emits a self-contradictory marker in practice. No parser cross-checks `status` against
`verdict` and no archived capture contains such a line; absence there is weak evidence, not a bound.
**A verifier must treat row 4 as open, not as covered by this plan.**

## Deviations from Plan

### 1. [Rule 3 — blocking issue] Every `cargo test` command in the plan names a non-existent target

- **Found during:** Task 1, on the first verification run.
- **Issue:** The plan's `<verify>` blocks and acceptance criteria all use
  `cargo test -p devflow --lib …`. The `devflow` package is **binary-only** (`crates/devflow-cli`
  has `src/main.rs` and no `src/lib.rs`), so every one of those commands fails with
  `error: no library targets found in package 'devflow'`.
- **Why it was caught rather than absorbed:** the command exits non-zero and prints an error, so it
  could not have been mistaken for a pass. This is a milder cousin of the repo's known
  `cargo test --exact` false-green trap, and the same habit caught it — asserting on a real
  `N passed` with a non-zero `filtered out` count rather than on an exit code alone.
- **Fix:** substituted `cargo test -p devflow --bin devflow …` throughout. Every count reported in
  this SUMMARY comes from that form.
- **Not fixed in the plan file:** `34-03-PLAN.md` still carries the wrong invocation. Plans are not
  edited during execution; recorded here for the verifier. The plan's prose note that "the package
  is `devflow`, not `devflow-cli`" is correct — it is the `--lib` flag that is wrong.

### 2. [Rule 2 — TDD gate] A sixth test was added, beyond the six artifacts the plan enumerates

- **Found during:** Task 1.
- **Issue:** Task 1 is `tdd="true"` and the plan is `type: tdd`, but the plan's
  `<artifacts_this_phase_produces>` table names no test for task 1 — only the rewrite. Without one
  there is no RED state to observe, and the RED/GREEN commit sequence the gate requires cannot exist.
- **Fix:** added `non_success_status_never_classifies_as_passed_even_with_verdict_pass`, which pins
  the single cell whose classification the rewrite actually changes. It is the only assertion in the
  plan capable of being red beforehand — every other cell's destination is unchanged, so a test
  written first would have passed against the old match and measured nothing.
- **Commits:** `5a1cb52` (RED, test only, 41 insertions / 0 deletions) then `fdaca14` (GREEN).

### 3. [Rule 3 — blocking issue] NC-4b needed two more temporary arms than the plan specified

Recorded in full under NC-4b above. The plan assumed adding the eighth variant would surface E0004
at `classify_validate_outcome`; in fact `devflow-core` fails first in two places. Two temporary arms
were added there and reverted, verified by blob hash. Both files are declared touched-transiently in
spirit; `outcome_policy.rs` was **not** in the plan's `files_touched_transiently` list, which named
only `agent_result.rs`. It should have been.

### 4. [Rule 1 — defect I introduced] Task 2's commit was not rustfmt-clean

- **Found during:** Task 3, by `scripts/check.sh all` (exit 1). `cargo clippy` exits 0 on the same
  tree, so clippy alone would not have caught it.
- **Issue:** two hunks inside `classify_validate_outcome_sweeps_all_forty_two_cells` and
  `external_verify_absent_verdict_is_ambiguous_only_when_layer0_decided` were committed in `616e303`
  with formatting rustfmt would reflow. The repo's pre-commit hook runs gitleaks, not rustfmt, so
  nothing blocked it.
- **Fix:** `cargo fmt --all`, folded into task 3's commit (`827fb8f`) and called out in its message.
  `scripts/check.sh all` then exits 0.
- **Habit worth keeping:** run `scripts/check.sh` before each task commit, not once at the end.

## TDD Gate Compliance

The plan is `type: tdd` and the gate sequence **is** literally present in git log, unlike plan
34-01's:

| Gate | Commit | Content |
|---|---|---|
| RED | `5a1cb52` | `test(34-03)` — the failing test alone, 41 insertions / 0 deletions, verified failing (exit 101) before it was committed |
| GREEN | `fdaca14` | `fix(34-03)` — the rewrite, 81 insertions / 28 deletions |
| (expansion) | `616e303`, `827fb8f` | `test(34-03)` — the sweep, the NC controls, criterion 4's downstream pin |

No REFACTOR commit: none was needed. The RED commit was constructed by reconstructing the
pre-rewrite file from `git show HEAD:…` and splicing in only the new test, then confirming the
splice was test-only (`41 insertions(+)`, `0 deletions`) and that it failed, before committing.

## Known Stubs

None. No placeholder values, no `TODO`/`FIXME` markers, no skipped or ignored tests, and every
`<verify>` block in the plan was executed (with the `--lib` → `--bin devflow` correction recorded
above).

## Threat Flags

None. No new network endpoint, auth path, file-access pattern, or schema change — the change is one
pure function plus tests and comments.

Register coverage: **T-34-03-01** (spoofing via the discarded status) mitigated by the rewrite and
pinned by the RED-gate test plus NC-4's two E0004 controls. **T-34-03-02** (collapsing the ordinary
auto-loop into an immediate gate) mitigated by NC-2 and NC-3's `layer0 = false` mirrors, both
asserting `Failed`. **T-34-03-03** (the fixture's `decided_by_layer`) mitigated by
`classifier_fixture` setting it explicitly in both directions, and demonstrated by the second
mutation control. **T-34-03-04** (the stale doc comment) mitigated by the rewritten comment, which
reproduces 34-01's corrected reachability record. **T-34-03-05** (`RateLimited`'s destination)
accepted, with the inline comment naming `decide_action`'s `AutoResume` routing as the tension to
confront if the cell ever becomes reachable. **T-34-03-SC**: no packages installed; NC-4 ran as a
recorded compile experiment rather than via a `trybuild`-class dependency, as the plan directed.

## Rollback

`git revert --no-commit 5a1cb52^..827fb8f` and commit once. One file, a pure function, no caller
depends on the match's internal shape.

**What a revert restores, and what it does not:** it brings back the `_` wildcard and the composite
`external` normaliser — reopening the structural weakness — without reopening 999.74's graft, which
is plan 34-01's and independently revertible. It also restores the superseded reachability story in
the doc comment, which would then contradict 34-01's corrected record. **If this plan is reverted
while 34-01 stands, fix the doc comment by hand rather than leaving the two in conflict.**

NC-4a and NC-4b left nothing to revert; both mutations were made, measured and reverted inside
task 3, verified by blob-hash equality.

## Self-Check: PASSED

- `34-03-SUMMARY.md` — written to the phase directory.
- `5a1cb52`, `fdaca14`, `616e303`, `827fb8f` — all present in `git log 4f5a177..HEAD`.
- `crates/devflow-cli/src/pipeline_outcomes.rs` — modified and committed.
- `crates/devflow-core/src/agent_result.rs` and `crates/devflow-core/src/outcome_policy.rs` —
  touched transiently for NC-4b, reverted, blob hashes equal to their pre-mutation values, and
  absent from every commit in this plan.
- STATE.md and ROADMAP.md deliberately NOT modified — worktree mode; the orchestrator owns those
  writes after the wave completes.
