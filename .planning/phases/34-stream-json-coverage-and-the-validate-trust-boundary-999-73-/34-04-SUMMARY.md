---
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
plan: 04
subsystem: layer0-discovery
tags: [security, worktree, layer0, external-verify, checkpoint, 999.76, criterion-6]
status: complete

requires:
  - "plan 34-01 — the reconcile_layer0_verdict graft fix; asserted present at HEAD before task 1"
  - "crates/devflow-core/src/verify.rs — the shared phase_plan_files discovery"
provides:
  - "evaluate_layer0 discovering declared commands from execution_root (999.76 call site 1)"
  - "pipeline_launch.rs's GateReview arm passing execution_root to phase_has_blocking_human_checkpoint (999.76 call site 2)"
  - "external_probe_discovers_from_the_worktree_when_the_main_checkout_lacks_the_plan — the inverted fixture"
  - "external_probe_discovers_from_project_root_across_every_stage_without_a_worktree — its main-checkout mirror, converted and renamed"
  - "phase_has_blocking_human_checkpoint_reads_the_execution_root_in_worktree_mode + _still_reads_the_project_root_without_a_worktree, each with its opposite-root control"
  - "the in-source record that this overturns a prior peer-review decision, and the three project_root reads deliberately preserved"
affects:
  - "plan 34-05 — Layer 0 now actually fires in worktree mode, so decided_by_layer == Some(0) becomes common rather than rare"

tech-stack:
  added: []
  patterns:
    - "converting a fixture whose premise a fix overturns into the fix's opposite-result mirror, rather than deleting it"
    - "mutating the production change back and re-running the whole suite, to measure whether any test actually covers it"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/verify.rs
    - crates/devflow-cli/src/pipeline_launch.rs

decisions:
  - "Discovery reads the execution root ONLY — no project_root fallback, because a fallback would make both the correct and incorrect root produce a passing test"
  - "The pre-existing fixture was converted in place (worktree_path cleared, probe file written to project_root, renamed ..._without_a_worktree) keeping every original assertion, rather than deleted"
  - "Task 3's NC-7 is recorded as a provenance measurement in this SUMMARY, not committed as a test — a test asserting on live git ls-tree output goes stale the moment the phase ships"

metrics:
  duration: "~20 min"
  completed: 2026-08-05

actuals:
  tokens: 8100
  tasks: 3
  commits: 4
---

# Phase 34 Plan 04: Layer 0 Discovery Moves to the Execution Root — Summary

Both 999.76 call sites now discover a phase's plans from the **execution root** — the worktree when
one is set. Layer 0's declared probe set actually runs in worktree mode, DevFlow's default operating
shape, where it previously mis-fired the "PLAN declaration was removed" veto instead; and the
plan-28-03 checkpoint auto-decide path is no longer silently dead there. ROADMAP criterion 6 is
closed for the code change. **One coverage gap is demonstrated rather than glossed** — see
"What these results do NOT establish".

## Precondition (asserted before task 1, per the plan's binding sequencing constraint)

```
cargo test -p devflow-core --lib \
  agent_result::tests::layer0_verdict_graft_declines_when_layer1_status_is_not_success -- --exact
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 550 filtered out
```

Plan 34-01's graft fix is present at HEAD. The non-zero `filtered out` count confirms the selector
matched a real test rather than exiting 0 on an empty set.

## What Changed

**The production change is two arguments.**

`crates/devflow-core/src/agent_result.rs` — `evaluate_layer0`'s discovery call:

```rust
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
let commands = crate::verify::external_verify_commands(execution_root, state.phase);
```

`crates/devflow-cli/src/pipeline_launch.rs:993-995` — the `Action::GateReview` arm:

```rust
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
let checkpoint_confirmed = state.agent == AgentKind::Claude
    && verify::phase_has_blocking_human_checkpoint(execution_root, phase)
    && agent_result::checkpoint_reported_in_capture(project_root, phase);
```

The deliberate split is visible on adjacent lines. No signature changed: `phase_plan_files`,
`external_verify_commands` and `phase_has_blocking_human_checkpoint` all keep
`(project_root: &Path, phase: u32)`. **No `project_root` fallback was added** — a fallback would
make both the correct and the incorrect root produce a passing test, so the measurement would agree
with itself and prove nothing.

**The overturned prior decision is recorded in source**, not patched quietly. The replacement
paragraph at `agent_result.rs:2025-2046`, quoted as the acceptance criterion requires:

> Both DISCOVERY and probe EXECUTION read `execution_root` — the worktree when one is set,
> `project_root` otherwise (999.76, ROADMAP criterion 6).
>
> This knowingly OVERTURNS a recorded prior peer-review decision (review Plan 03 MEDIUM, OpenCode).
> That decision held the two roots must stay distinct, discovery reading `project_root` because
> `.planning/phases/` "lives there, not in a worktree checkout". **The premise has the direction
> backwards.** `.planning/` is TRACKED content, so an in-flight phase's `{N}-PLAN.md` is committed
> on `feature/phase-{N}` and therefore exists INSIDE the worktree while absent from the main
> checkout for the phase's whole duration. Discovering from `project_root` meant a
> correctly-declared probe set silently never ran in worktree mode — DevFlow's default operating
> shape — with no error and no log, and the "PLAN removed" veto below fired in its place. Recorded
> as an overturn rather than patched quietly, so a later reader can see the direction was
> reconsidered on evidence rather than overlooked.
>
> Three sibling reads deliberately KEEP `project_root` and must not be "corrected" to match:
> [`phase_commit_count`] (git worktrees share refs and the object database, so counting from the
> main checkout is right), and [`checkpoint_reported_in_capture`] and [`evaluate_layer1`] (both read
> the stdout capture under `.devflow/`, which lives in the project root).

## Task 1 — The Red State, and the Fixture Conversion

### RED (commit `61debe4`, test only, 72 insertions / 0 deletions)

The inverted fixture was committed failing, against the pre-fix discovery:

```
thread 'agent_result::tests::external_probe_discovers_from_the_worktree_when_the_main_checkout_lacks_the_plan'
panicked at crates/devflow-core/src/agent_result.rs:5437:9:
expected a failing-probe reason; a PLAN-removed reason means discovery silently returned zero
commands — i.e. discovery still reads project_root and 999.76's fix did not land:
Some("external verification approval mismatch; PLAN declaration was removed")

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 551 filtered out
```

`Some("external verification approval mismatch; PLAN declaration was removed")` is the veto the
plan's source trace predicted from `agent_result.rs:2043-2050`, reproduced verbatim.

### The pre-existing fixture failed exactly as predicted, and was converted — not deleted

After the discovery change and **before** the conversion:

```
test agent_result::tests::external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree ... FAILED
test agent_result::tests::external_probe_discovers_from_the_worktree_when_the_main_checkout_lacks_the_plan ... ok

panicked at crates/devflow-core/src/agent_result.rs:5370:9:
expected a failing-probe reason, not a false PLAN-removed veto:
Some("external verification approval mismatch; PLAN declaration was removed")

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 550 filtered out
```

**A correction to the plan's prediction, stated rather than smoothed over.** The plan predicted
"the fixture's `"external verification failed"` reason assertion **and** its final `Success`
assertion both fail". Only the first was *observed* failing — the test aborts at the first panicking
assertion, so the final `Success` assertion was never reached. The failure mechanism is exactly as
traced; the count of failing assertions is not, and reporting "both failed" would have been a claim
about something never executed.

The fixture was converted to its main-checkout mirror and renamed
`external_probe_discovers_from_project_root_across_every_stage_without_a_worktree`. What changed:
`state.worktree_path = None` (so the two roots coincide), the `phase-worktree` directory dropped,
and the probe file written to `dir.path().join("implemented")` instead of `worktree.join(...)`.
**Every original assertion is kept** — both `Failed` status assertions, the
`"external verification failed"` reason assertion with its original message, the `Stage::Plan` →
`Stage::Code` sweep, the final `AgentStatus::Success` and the `decided_by_layer == Some(0)`.

### Both halves, reported together (the plan's prohibition)

| Fixture | Layout | Result |
|---|---|---|
| `..._from_the_worktree_when_the_main_checkout_lacks_the_plan` | PLAN in worktree only, `project_root/.planning/phases` absent | `ok` |
| `..._from_project_root_across_every_stage_without_a_worktree` | PLAN in project root, no worktree set | `ok` |

`cargo test -p devflow-core --lib agent_result::tests::external_probe_` →
**`2 passed; 0 failed`, 550 filtered out.**

Neither is reported alone. The worktree fixture was red before the fix and green after (shown
above); the mirror is green in both states, which is precisely its job — it establishes that the
main-checkout path is unaffected. A single fixture could not distinguish "discovery reads the
execution root" from "discovery reads whichever root happens to hold the PLAN".

## Task 2 — The Second Call Site, and a Demonstrated Coverage Gap

The two new `verify.rs` tests each carry an opposite-root assertion, so neither can pass by merely
detecting that a PLAN exists somewhere:

| Test | Root with PLAN → | Other root → |
|---|---|---|
| `..._reads_the_execution_root_in_worktree_mode` | worktree: `true` | project root: `false` |
| `..._still_reads_the_project_root_without_a_worktree` | project root: `true` | empty sibling: `false` |

`verify::` module selector: **11 passed before → 13 passed after** (+2), `0 failed`, 541 filtered
out. Pre-task `phase_has_blocking_human_checkpoint_*` count re-derived at HEAD: **6**, matching the
plan's review-corrected number.

### These two tests are NOT a RED gate, and the reason matters

Both were run **before** the `pipeline_launch.rs` change and **passed**:

```
test verify::tests::phase_has_blocking_human_checkpoint_reads_the_execution_root_in_worktree_mode ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 553 filtered out
```

That is inevitable, not an oversight: the defect lives at the **call site**, and
`phase_has_blocking_human_checkpoint` reads whatever root it is handed both before and after. The
tests pin the property that makes the caller's choice matter — the answer depends on the root —
which is a real and previously-unpinned property. They do not exercise the fix.

### The negative control on my own claim

Asserting "no test covers the call site" would have been a guess. It was measured. The call site was
reverted to `project_root` and the **entire** binary suite re-run:

```
sed -i 's/…(execution_root, phase)/…(project_root, phase)/' crates/devflow-cli/src/pipeline_launch.rs
cargo test -p devflow --bin devflow
test result: ok. 279 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**279 of 279 pass with the fix removed.** No test in this repository covers the `Action::GateReview`
call-site change. The mutation was restored and verified byte-identical (`diff` exit 0) before
committing; `rg` confirms `execution_root` at `pipeline_launch.rs:994`.

The evidence for call site 2 is therefore: (a) the function is provably root-sensitive in both
directions, (b) the call site provably passes the execution root, (c) the capture-reading sibling
provably still passes `project_root` — an inference from two verified halves, **not** an end-to-end
demonstration. Closing that gap needs a fixture that drives `advance()` through `GateReview` in
worktree mode with a session id and a capture; it is not in this plan's scope, and it is the honest
residual of criterion 6.

## Task 3 — NC-7, the Two-Ref Provenance Control

| Measurement | Value |
|---|---|
| `git ls-tree -r develop --name-only -- .planning/phases \| grep -c '/34-'` | **0** |
| `git ls-tree -r HEAD --name-only -- .planning/phases \| grep -c '/34-'` | **21** |
| `develop` SHA | `91a1b58` |
| `HEAD` SHA | `1f3a5a3` |
| `origin/develop` SHA | `91a1b58` — identical to local `develop` |

Both halves read the **local** `develop` ref, stated explicitly. No `git fetch` was run: it updates
`FETCH_HEAD` and the remote-tracking ref, not the local branch ref that `git ls-tree develop` reads,
so it would have implied a freshness it does not provide. In this case the question is moot —
local `develop` and `origin/develop` resolve to the same commit, verified above rather than assumed.

**Interpretation.** A non-zero `HEAD` count against a zero `develop` count is a live demonstration
that an in-flight phase's `.planning/phases/34-…` content exists **only** on the feature branch —
i.e. only inside the worktree — which is precisely the premise the fix rests on.

### Two controls on the control

A count of 0 is exactly what a *broken* measurement returns, so `develop=0` cannot be trusted alone.

1. **The `-r` trap, reproduced.** `git ls-tree HEAD --name-only -- .planning/phases | grep -c '/34-'`
   (no `-r`) returns **0** — on the very ref that genuinely has 21 matching paths. The
   non-recursive form is incapable of producing a non-zero answer, so it would have "confirmed"
   any hypothesis. `-r` is load-bearing, demonstrated rather than quoted.
2. **`develop=0` is an absence, not an empty read.** `git ls-tree -r develop --name-only --
   .planning/phases | wc -l` returns **38**. The ref is being read and does yield phase files; none
   of them is phase 34. Without this, `develop=0` would be indistinguishable from a path filter
   matching nothing at all.

`external_probe_` re-run after the measurement: `2 passed; 0 failed`, 552 filtered out.

**Task 3's source deliverable landed early.** The NC-7 citation in the inverted fixture's doc
comment (`agent_result.rs:5421`, `:5424`) was written as part of task 1's RED commit rather than a
separate task-3 commit, so task 3 produced no commit of its own. Recording that rather than
manufacturing one.

### What NC-7 does NOT establish

It demonstrates the **premise** — where tracked phase docs live during a phase — and nothing about
whether the code change works. The code claim is carried by task 1's inverted fixture and its
main-checkout mirror. That is why NC-7 is recorded alongside them rather than instead of them, and
why it is a SUMMARY record rather than a committed test: a test asserting on live `git ls-tree`
output goes stale the moment the phase ships (CONTEXT.md's `/33-` version already did).

## Verification Results

| Check | Result |
|---|---|
| `agent_result::tests::external_probe_` selector | `2 passed; 0 failed`, 550 filtered out |
| `agent_result::` module selector | `156 passed; 0 failed`, 396 filtered out (155 before, +1) |
| `verify::` module selector | `13 passed; 0 failed`, 541 filtered out (11 before, +2) |
| `pipeline_launch::` selector (`--bin devflow`) | `31 passed; 0 failed`, 248 filtered out |
| devflow-core lib suite | `554 passed; 0 failed` (551 before, +3) |
| devflow bin suite | `279 passed; 0 failed` (unchanged — this plan adds no bin tests) |
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0**, captured directly |
| `cargo fmt --all --check` | exit 0, run before every commit |
| `scripts/check.sh all` | **exit 0**, captured directly (see the flake note below) |
| new fixture name present / old name absent | `1` / `0` |
| `external_verify_commands` first argument | `execution_root` at `agent_result.rs:2057` |
| `evaluate_layer0 -A 15` `unwrap_or` count | `1` — still derived from `state.worktree_path` |
| `phase_commit_count` "never a worktree path" doc | `1` — asymmetry intact |
| `phase_has_blocking_human_checkpoint` signature | unchanged at `(project_root: &Path, phase: u32) -> bool` |
| `checkpoint_reported_in_capture` in `pipeline_launch.rs` | still `project_root`, at `:995` |

**T-34-04-02 confirmation (required by the threat register):** the approval-mismatch arm is
untouched. The complete non-comment diff of `agent_result.rs` outside the test module is one line —
`external_verify_commands(project_root, …)` → `(execution_root, …)`. `commands != approved_commands`
at `:2085` and the exact-array approval parse are byte-identical. This fix changes which root
declarations are read from, not the approval requirement.

### A `scripts/check.sh all` failure that is a known pre-existing flake

`check.sh all` was run four times across this plan: **exit 0, exit 0, exit 101, exit 0.** The
101 run and the exit-0 runs on either side of it were on a **byte-identical** working tree
(`git status --short` empty, tree equal to commit `1f3a5a3`), so the failure is non-deterministic by
direct demonstration, not by inference.

Root failure: `pipeline_gate::tests::concurrent_ship_advances_finish_both_phases_independently` —
`at least one phase must finish independently of the other; got 0/2 successes` — followed by ~30
`PoisonError` failures across every `ENV_MUTEX`-using test. That is **the exact mechanism ROADMAP
line 1472 already documents**, naming the same test: "Observed live during 33-06: a single
`index.lock` failure in `concurrent_ship_advances_finish_both_phases_independently` cascaded into
~15 unrelated `PoisonError` failures." Ten regions still use the trailing-statement `PATH` restore.
The test passes standalone (`1 passed`, 278 filtered out).

Pre-existing, already on the backlog, out of scope per the scope boundary — **not** fixed, and no
new entry filed since one exists.

### What these results do NOT establish

- **No test covers the `pipeline_launch.rs` call-site change** — demonstrated above by reverting it
  and watching all 279 bin tests still pass. Criterion 6's second half rests on greps plus a
  root-sensitivity proof, not on an executable end-to-end demonstration.
- **`554 passed` / `279 passed` are regression surface.** Only the four named tests bear on 999.76.
- **The flake bound is weak.** Three `check.sh all` passes against one failure is a ~75% observed
  pass rate on four runs; it establishes non-determinism, not a rate. I did **not** measure the
  baseline tree's flake rate, so I cannot rule out that this plan's changes make it *more* likely —
  only that the failure is not deterministic and matches a documented pre-existing mechanism.
- **Nothing here measures a real worktree run.** Every fixture uses tempdirs standing in for a
  worktree; `state.worktree_path` is set to an ordinary directory, not a linked git worktree. The
  discovery path does not care (it is a filesystem read), but that is an argument, not a measurement.
- **NC-7 does not establish the code works** — see its own section.

## Deviations from Plan

### 1. [Rule 3 — blocking] `cargo test -p devflow --lib` names a non-existent target

- **Found during:** flagged in the executor briefing before task 2; confirmed as recorded by plans
  34-02 and 34-03.
- **Issue:** task 2's `<verify>` block specifies `cargo test -p devflow --lib pipeline_launch::`.
  The `devflow` package is binary-only, so this fails with `error: no library targets found`.
- **Fix:** used `cargo test -p devflow --bin devflow pipeline_launch::`. No source change. This is
  the third plan in the phase to hit it; **34-05's plan carries the same wrong selector.**

### 2. [Acceptance-criterion correction] `cargo clippy -p devflow-core --all-targets` cannot exit 0

- **Found during:** task 1.
- **Issue:** task 1's `<verify>` block specifies `cargo clippy -p devflow-core --all-targets`, which
  fails with pre-existing `E0433` errors — `tests/monitor_e2e.rs` and `tests/devflow_dir_gitignore.rs`
  reference `devflow_core::test_support`, gated behind a feature not enabled when the crate builds
  alone. Established as pre-existing by plan 34-01 and unchanged here.
- **Resolution:** used `cargo clippy --workspace --all-targets -- -D warnings`, which is what
  `scripts/check.sh` actually gates on. Exit 0, captured directly.

### 3. [Prediction correction] Only one of the pre-existing fixture's two assertions was observed failing

Recorded in full under task 1 above. The plan predicted both the reason assertion and the final
`Success` assertion would fail; the test aborts at the first, so the second was never reached.
The failure *mechanism* matched the plan's source trace exactly.

### 4. [Task 3 produced no commit] The NC-7 doc-comment note landed in task 1's commit

The citation was written into the inverted fixture's doc comment when that fixture was first
authored (commit `61debe4`), so task 3 had no source change left to make. Its deliverable is this
SUMMARY's NC-7 section. Recorded rather than manufacturing an empty commit.

## TDD Gate Compliance

The plan is `type: tdd`. Task 1's gate sequence **is** literally present in git log:

| Gate | Commit | Content |
|---|---|---|
| RED | `61debe4` | `test(34-04)` — the inverted fixture alone, 72 insertions / 0 deletions, verified failing before commit |
| GREEN | `8fd7c4f` | `fix(34-04)` — the discovery argument, the doc rewrite, the fixture conversion |

No REFACTOR commit; none was needed.

**Task 2 carries `tdd="true"` but has no RED gate, and this is a genuine gap rather than a
bookkeeping one.** Its tests pass against the unfixed call site — demonstrated, not assumed (see
task 2 above). A reviewer looking for a `test(...)` → `fix(...)` pair on the `pipeline_launch.rs`
change will not find one, because no test in the repo can distinguish that change.

## Known Stubs

None. No placeholder values, no `TODO`/`FIXME` markers added, no skipped or ignored tests. Every
`<verify>` block in the plan was executed, with the `--lib` → `--bin devflow` and
`-p devflow-core` → `--workspace` clippy corrections recorded above.

The one substantive incompleteness is **not** a stub but a coverage gap, recorded in full under
task 2: no test exercises the `Action::GateReview` call site.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern or schema change — the production
delta is two arguments.

Register coverage:

- **T-34-04-01** (probe set silently never executes) — mitigated by task 1; pinned by the inverted
  fixture whose failure message distinguishes "probe failed" from "discovery returned zero commands".
- **T-34-04-02** (approval/discovery TOCTOU) — **confirmed untouched** and stated above: the
  approval comparison and exact-array parse are byte-identical.
- **T-34-04-03** (interaction with 34-01's graft) — mitigated by the `depends_on` edge and the
  precondition assertion, whose output is quoted at the top of this SUMMARY.
- **T-34-04-04** (dead checkpoint auto-decide path) — call site fixed and both roots pinned with
  opposite-result assertions, **but** with the coverage gap named above; treat as mitigated-by-
  construction rather than mitigated-by-demonstration.
- **T-34-04-05** (over-broad retargeting) — `phase_commit_count`, `checkpoint_reported_in_capture`
  and `evaluate_layer1` all still read `project_root`, verified by grep and named in one place in
  `evaluate_layer0`'s doc comment.
- **T-34-04-SC** — no packages installed; no `Cargo.toml` change.

## Rollback

`git revert --no-commit 61debe4^..1f3a5a3` and commit once.

**This revert is not purely mechanical.** It restores discovery to `project_root` AND must restore
the prior peer-review decision's reasoning in `evaluate_layer0`'s doc comment — a revert that
silently drops the overturn record leaves a future reader with no trace that the direction was ever
reconsidered. That is why task 1 is rated `costly`. It also re-inverts the two fixtures: the
converted mirror would need its `worktree_path` and worktree-resident probe file restored, and the
new inverted fixture deleted.

**Ordering constraint.** Do not revert this plan while plan 34-05's widening is in place if that
widening was justified by worktree-mode captures — check whether `STREAM_JSON_STAGES` names any
stage whose evidence directory records a worktree-mode run first. Conversely, plan 34-01's graft fix
must **not** be reverted while this plan stands: this plan is what makes `decided_by_layer ==
Some(0)` common, which is the graft's precondition.

## Self-Check: PASSED

- `.planning/phases/34-.../34-04-SUMMARY.md` — written to the phase directory; committed in this step.
- `61debe4`, `8fd7c4f`, `1f3a5a3` — all present in `git log a6ff7ff..HEAD`.
- `crates/devflow-core/src/agent_result.rs`, `crates/devflow-core/src/verify.rs`,
  `crates/devflow-cli/src/pipeline_launch.rs` — all modified and committed; working tree clean
  before this commit.
- The temporary `pipeline_launch.rs` mutation from task 2's coverage control was restored and
  verified with `diff` (exit 0); it appears in no commit.
- STATE.md and ROADMAP.md deliberately NOT modified — worktree mode; the orchestrator owns those
  writes after the wave completes.
