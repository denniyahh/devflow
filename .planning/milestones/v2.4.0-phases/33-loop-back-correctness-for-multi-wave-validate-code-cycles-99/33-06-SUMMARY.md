---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
plan: 06
subsystem: pipeline
tags: [refactor, test-hygiene, raii, code-review-closure]
status: complete
requires:
  - "33-05 (the CR-01 caller fix this plan hardens)"
provides:
  - "`phase_verification_exists(evidence_root, phase)` / `phase_review_path(evidence_root, …)` — the callee's signature is now the contract"
  - "a single owned `evidence_root: PathBuf` in `handle_validate_outcome`, consumed by all three loop-back arms"
  - "`NeutralPath` RAII guard in `test_support.rs`"
  - "two independently-reportable worktree-mode scenario tests"
affects:
  - "crates/devflow-core/src/agent_result.rs"
  - "crates/devflow-cli/src/pipeline_outcomes.rs"
  - "crates/devflow-cli/src/test_support.rs"
tech-stack:
  added: []
  patterns:
    - "RAII guard for process-global env mutation, modeled on `ReapMonitorOnDrop`"
    - "owned `PathBuf` binding to hoist a resolution past a `&mut` borrow"
key-files:
  created: []
  modified:
    - "crates/devflow-core/src/agent_result.rs"
    - "crates/devflow-cli/src/pipeline_outcomes.rs"
    - "crates/devflow-cli/src/test_support.rs"
decisions:
  - "Scoped the `NeutralPath` guard in a block rather than binding it for the whole test body, so PATH is restored before the assertions run — behaviourally identical to the trailing-statement form it replaces, but unconditional."
  - "Named the split-out scenario B `worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator` and led its doc comment with what it is the only test in the workspace to catch."
metrics:
  duration: "~25 min"
  completed: 2026-08-05
actuals:
  tokens: 4200
  tasks: 3
  commits: 3
---

# Phase 33 Plan 06: Gap Closure for WR-07 / WR-08 / WR-05 / IN-06 / IN-07 Summary

Closed the five code-review findings that are Phase 33's own new code — a callee whose parameter
name still invited the defect its caller was fixed for, a triplicated evidence-root resolution, two
non-RAII PATH regions, a packed two-scenario test hiding the suite's only anti-OR-both-roots
discriminator, and three fixtures claiming a stage production cannot be in. Zero production
behaviour change; no existing assertion edited.

## Commits

| Task | Commit | Description |
|---|---|---|
| 1 | `3aff2c7` | `refactor(33-06): name the evidence root in the callee, not just the caller` |
| 2 | `670be34` | `refactor(33-06): bind the evidence root once so a fourth arm cannot get it wrong` |
| 3 | `6b0d3c3` | `test(33-06): RAII path guard, split the OR-both-roots discriminator, honest stage` |

## Task 1 — WR-07: the callee's signature is now the contract

Before → after, both in `crates/devflow-core/src/agent_result.rs`:

```rust
// before
fn phase_review_path(project_root: &Path, phase: u32) -> Option<PathBuf>
pub fn phase_verification_exists(project_root: &Path, phase: u32) -> bool

// after
fn phase_review_path(evidence_root: &Path, phase: u32) -> Option<PathBuf>          // :2549
pub fn phase_verification_exists(evidence_root: &Path, phase: u32) -> bool         // :2588
```

`phase_verification_exists`' doc comment gained the contract sentence: `evidence_root` is the root
the Validate agent actually wrote to (the phase's worktree when `state.worktree_path` is set, else
the project root), because `.planning/` is tracked and in worktree mode the artifact lands on
`feature/phase-N` and is invisible from the main checkout for the phase's entire in-flight duration.
It also states explicitly that this is NOT interchangeable with `phase_commit_count`'s root.

### Prohibition negative control — `phase_commit_count` was NOT renamed

```
$ rg -n "fn phase_verification_exists|fn phase_review_path|fn phase_commit_count" \
     crates/devflow-core/src/agent_result.rs
1841:pub fn phase_commit_count(project_root: &Path, git_flow: &GitFlowConfig, phase: u32) -> u32 {
2549:fn phase_review_path(evidence_root: &Path, phase: u32) -> Option<PathBuf> {
2588:pub fn phase_verification_exists(evidence_root: &Path, phase: u32) -> bool {
```

`phase_commit_count` still reads `project_root`, which is correct for it — git refs and the object
database are shared across worktrees. The control is meaningful precisely because the same `rg`
invocation returned `evidence_root` for the two functions that were supposed to change: a pattern
that matched nothing would have shown no rows at all, not three.

`cargo build --workspace` exited 0.

## Task 2 — WR-08: one owned binding, three consumers

`handle_validate_outcome` now resolves the evidence root once, before the match, as an owned
`PathBuf` (owned, so it holds no borrow of `state` across the `&mut state` calls in each arm —
which is what made 33-05 believe the resolution could not be hoisted):

```rust
let evidence_root: PathBuf = state
    .worktree_path
    .clone()
    .unwrap_or_else(|| project_root.to_path_buf());
```

All three arms (Ambiguous gate loop-back, gated loop-back, plain-Failed tail) now call
`select_loop_back_fix(&evidence_root, state.phase)`. The three near-identical inline comments
collapsed into one at the binding; each call site keeps a one-line pointer.

### Idiom count — the triplication is gone from this file

```
$ rg -c "worktree_path.as_deref\(\).unwrap_or\(project_root\)" \
     crates/devflow-cli/src/pipeline_outcomes.rs
(no output — 0 matches)
```

**Positive control for that regex** (an empty `rg -c` result is indistinguishable from a broken
pattern, so the same pattern was run against the whole crate tree):

```
$ rg -c --no-heading "worktree_path.as_deref\(\).unwrap_or\(project_root\)" crates/
crates/devflow-core/src/agent_result.rs:1
crates/devflow-cli/src/preflight.rs:2
crates/devflow-cli/src/staleness.rs:1
```

The pattern is well-formed and still matches 4 sites elsewhere — `pipeline_outcomes.rs` is absent
from that list because it genuinely has zero, not because the regex was wrong. The
`agent_result.rs` hit is the `evaluate_layer0` site filed as 999.76, deliberately untouched.

### Prohibition negative control — the commit-count root is unchanged

```
$ rg -n "phase_commit_count\(project_root" crates/devflow-cli/src/pipeline_outcomes.rs
347:            agent_result::phase_commit_count(project_root, &GitFlowConfig::default(), state.phase);
```

Exactly 1 hit, as required.

## Task 3 — WR-05 + IN-06 + IN-07

**`NeutralPath`** added to `test_support.rs`, modeled on `ReapMonitorOnDrop`: `install()` captures
the current `PATH`, replaces it with an `agent_free_git_only_path_dir()`, and owns the `TempDir`;
`Drop` restores the captured value (or removes the var if there was none). A type's own `Drop::drop`
runs before its fields drop, so `PATH` is restored before the `TempDir` is deleted and never
transiently names a removed directory. The doc comment records the `ENV_MUTEX` precondition.

**Regions converted:** exactly the two 33-05 added. Because scenario B was split into its own test,
those two regions became three guard sites:

```
$ rg -n "NeutralPath::install" crates/devflow-cli/src/
crates/devflow-cli/src/pipeline_outcomes.rs:1632
crates/devflow-cli/src/pipeline_outcomes.rs:1688
crates/devflow-cli/src/pipeline_outcomes.rs:1752
```

**IN-06 split.** `worktree_mode_mid_arc_loop_back_issues_plain_execute` keeps scenario A (phase 94);
scenario B (phase 95) moved to
`worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator`. `git diff -U0`
confirms both `assert_eq!` blocks moved with their messages byte-identical — nothing was edited,
softened, or removed.

**IN-07.** All three fixtures now set `Stage::Validate` (`:1603`, `:1673`, `:1729`). No assertion
changed as a result, which is the expected outcome: `select_loop_back_fix` never reads
`state.stage`.

### Exact-name runs, with the fabricated-name negative control

```
$ cargo test -p devflow --bins -- --exact \
    pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute
running 1 test
test pipeline_outcomes::tests::worktree_mode_mid_arc_loop_back_issues_plain_execute ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 271 filtered out; finished in 0.00s

$ cargo test -p devflow --bins -- --exact \
    pipeline_outcomes::tests::worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator
running 1 test
test pipeline_outcomes::tests::worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 271 filtered out; finished in 0.01s
```

Both show a real `1 passed` with a non-zero `filtered out` (271 + 1 = 272, the full bin count).

```
$ cargo test -p devflow --bins -- --exact \
    pipeline_outcomes::tests::this_test_name_does_not_exist_negative_control
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 272 filtered out; finished in 0.00s
```

The fabricated name reports `0 passed` and **still exits 0** — the trap CLAUDE.md records,
demonstrated rather than reasoned about. This is what makes the two runs above evidence: a name
that matches nothing produces `running 0 tests`, which neither of them did.

Note: the package is `devflow`, and `--lib` does not work on it (`error: no library targets found in
package 'devflow'` — it is a binary crate). `--bins` is the correct selector; the plan's
`cargo test -p devflow --lib` in Task 2's verify block was adjusted accordingly.

### Reconciled workspace counts

| Target | 33-05 baseline | This plan | Reconciliation |
|---|---|---|---|
| `devflow` (bin) | 271 | **272** | +1 net new, exactly the IN-06 split (one test became two) |
| `devflow-core` (lib) | 547 | **547** | unchanged |

All other workspace targets green and unchanged (build_provenance 3, ci_parity_guards 7,
gate_sweep_e2e 4, git_env_hermeticity 1, gitignore_coverage 1, help_snapshot 1, log_format_env 3,
phase7_cli 17, pre_push_signing_policy 5, reap_strays_e2e 2, release_check 10,
start_reachability_e2e 2, stop_e2e 9, workspace_version_pin 1, yes_ship_config 5,
devflow_dir_gitignore 2, monitor_e2e 2).

### Gate commands and their exit codes

Each was run unpiped so the reported exit code is the command's own, not a pipeline's:

- `cargo test --workspace --no-fail-fast --quiet` → exit 0
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0
- `cargo fmt --all -- --check` → exit 0

## A transient failure that was investigated, not waved away

The **first** `cargo test -p devflow --bins` run after the split failed. The root failure was
`pipeline_gate::tests::concurrent_ship_advances_finish_both_phases_independently` at
`pipeline_gate.rs:1049` (`got 0/2 successes`), with git reporting
`Unable to create '/tmp/.tmpcNaiSN/.git/index.lock': File exists` — the test's own two concurrent
ship threads contending on the same index. That panic poisoned `ENV_MUTEX`, cascading into ~15
`PoisonError` failures in unrelated tests.

This is a **documented pre-existing race**, recorded in `STATE.md` under Phase 17 / 17-09 GAP-2:
debug instrumentation there caught both phases computing the identical version tag ~1.8ms apart,
*"proving the checkout lock occasionally fails to fully serialize the two threads' terminal
hooks"* — explicitly recorded as out of scope and not fixed. 17-09 bounded the resulting *wedge*
(the unbounded gate poll); it did not remove the underlying contention.

Evidence and its limits:

- 6 full bin-suite runs on this branch: **1 failed, 5 passed.** That is a weak bound. It establishes
  the failure is not deterministic; it does not establish a rate, and 5 passes is nowhere near
  enough to call it rare.
- Nothing in this plan touches git, ship, terminal hooks, or that test. The failing assertion is
  about concurrent ship completion; the changed code is a parameter rename, a hoisted binding, and
  three test fixtures.
- **What I cannot rule out:** this plan adds one test (271 → 272), which changes thread scheduling
  under the default parallel harness. I have not run the pre-change commit under equivalent load, so
  I cannot exclude that the extra scheduling pressure makes the pre-existing race surface more often.
  What I can say is that the race's mechanism is independent of anything this plan changed.
- Ironically the cascade is the exact shape WR-05 describes — but note that `NeutralPath` does **not**
  fix it. The guard makes the PATH *restore* unconditional in two regions; the `ENV_MUTEX` poisoning
  half of WR-05 (the `unwrap_or_else(PoisonError::into_inner)` recommendation) is out of this plan's
  scope and remains open.

## Deviations from Plan

**1. [Rule 3 - Blocking] `cargo test -p devflow --lib` does not work on this package**

- **Found during:** Task 2 verification
- **Issue:** the plan's verify step specifies `cargo test -p devflow --lib`; `devflow` is a binary
  crate, so this fails with `error: no library targets found in package 'devflow'` (exit 101).
- **Fix:** used `cargo test -p devflow --bins`, which runs the same 271/272-test unit suite.
- **Files modified:** none — verification-command change only.

No other deviations. No production behaviour changed; no existing assertion was edited, softened, or
removed.

## Findings deliberately NOT closed

Stated plainly so none is mistaken for resolved:

| Finding | Status | Where it lives |
|---|---|---|
| CR-01 in `evaluate_layer0` (`agent_result.rs:2041-2042`) | **open, untouched** | backlog 999.76 / DEN-98 — its scope includes rewriting the green test at `:5259` |
| WR-03, the transient-git counter reset (`pipeline_outcomes.rs:334-358`) | **open, untouched** apart from consuming the new binding | backlog 999.77 / DEN-99 |
| The ten pre-existing non-RAII PATH regions | **open, untouched** | WR-05; the sweep is its own plan, which `NeutralPath` now makes cheap |
| WR-05's `ENV_MUTEX` poison-recovery half | **open** | WR-05, second paragraph of its Fix |
| WR-01 | **open, carried** | 33-REVIEW.md |
| WR-02 | **open, carried** | 33-REVIEW.md |
| WR-04 | **open, carried** | 33-REVIEW.md |
| WR-06 (spawn hardening on 3 of 4 sites) | **open, carried** | 33-REVIEW.md |
| IN-01 … IN-05 | **open, carried** | 33-REVIEW.md |

## Self-Check: PASSED

- `crates/devflow-core/src/agent_result.rs` — modified, present
- `crates/devflow-cli/src/pipeline_outcomes.rs` — modified, present
- `crates/devflow-cli/src/test_support.rs` — modified, present
- Commits `3aff2c7`, `670be34`, `6b0d3c3` — all present in `git log 5d439a8..HEAD`
