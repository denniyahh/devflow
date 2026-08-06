---
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
plan: 01
subsystem: outcome-cascade
tags: [security, trust-boundary, validate, layer0, d-15, 999.74]
status: complete

requires:
  - "crates/devflow-core/src/agent_result.rs — evaluate_layer0's affirmative-success arm"
  - "crates/devflow-core/src/agent_result.rs — evaluate_layer1's AgentResult (status AND verdict)"
provides:
  - "reconcile_layer0_verdict gated on Layer 1's own AgentStatus before the verdict transplant"
  - "layer0_verdict_graft_declines_when_layer1_status_is_not_success — the named D-15 regression pin"
  - "layer0_verdict_graft_still_transplants_a_passing_layer1_verdict — NC-5's opposite-result control"
  - "layer0_disabled_routes_a_self_reported_failure_to_gate_review — NC-6"
  - "the corrected in-source record of how the Validate inversion is actually reached"
affects:
  - "plan 34-04 (999.76) — its precondition; moving Layer 0 discovery to the execution root makes decided_by_layer == Some(0) common, which is this graft's precondition"
  - "plan 34-03 — corrects the third overstated note (classify_validate_outcome's own) and lands criterion 3"

tech-stack:
  added: []
  patterns:
    - "Option::filter before and_then to gate a cross-layer field transplant on the source layer's own status"
    - "paired opposite-result tests: every discrimination claim carries the case that must NOT produce the same answer"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs

decisions:
  - "The graft gate is expressed as Option::filter on layer1.status == Success; the three-way early-return guard above it is left byte-identical because it already excludes an idle timeout twice over"
  - "NC-6 declares no external_verify probe, so the control is deterministic under every value of DEVFLOW_EXTERNAL_VERIFY_ENABLED rather than racing config's own env-mutating test"
  - "D-07's originally-specified pre-fix pin test was NOT written; its amended two-part deliverable is discharged by the written finding plus task 1's cascade-level demonstration"

metrics:
  duration: "~25 min"
  completed: 2026-08-05

actuals:
  tokens: 4769
  tasks: 3
  commits: 3
---

# Phase 34 Plan 01: The reconcile_layer0_verdict Graft Fix — Summary

`reconcile_layer0_verdict` now consults Layer 1's own `AgentStatus` before transplanting its
`verdict`, closing the live route by which an agent's self-reported failure was laundered into an
affirmative pair that `decide_action` advances and `classify_validate_outcome` reads as `Passed`.

## What Changed

**The production fix is one expression** (`crates/devflow-core/src/agent_result.rs`):

```rust
let verdict = evaluate_layer1(project_root, state.phase)
    .filter(|layer1| layer1.status == AgentStatus::Success)
    .and_then(|layer1| layer1.verdict);
```

The three-way early-return guard above it is unchanged, as the plan required — it already excludes
an idle timeout by two independent conditions and that reasoning is recorded in the doc comment.
`decided_by_layer`, `status`, `reason`, `commits`, `summary` and the `..result` struct-update tail
are all untouched; the fix moves `.verdict` only.

**Three new tests plus one extended fixture**, all against the real cascade
(`evaluate_agent_result_inner`), never a mock or a direct classifier call.

## The Pre-Fix Red State (Task 1 acceptance criterion 3)

The fourth marker case was observed failing before the fix was written, at
`crates/devflow-core/src/agent_result.rs:5569`:

```
assertion `left == right` failed: a verdict attached to a self-reported failure must not be grafted (D-15)
  left: Some(Pass)
 right: None
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 546 filtered out
```

`left: Some(Pass)` is the exploit reproduced end-to-end: a marker of
`{"status":"failed","verdict":"pass"}` reaching `(Success, Some(Pass), Some(0))`. The non-zero
`filtered out` count confirms the selector matched a real test rather than exiting 0 on an empty
set.

## Negative Controls — Each Reported With Its Opposite Case

Per the plan's prohibition, no measurement below is reported alone.

| Control | Marker | Result | What it establishes |
|---|---|---|---|
| **Exploit** | `{"status":"failed","verdict":"pass"}` | `verdict: None` | The graft declines |
| **NC-5 positive half** | `{"status":"success","verdict":"pass"}` | `verdict: Some(Pass)` | The fix is discriminating, not indiscriminate |
| **NC-5a** | `{"status":"failed"}` | `None` both pre- and post-fix | The failed status ALONE is not the exploit |
| **NC-5b** | `{"status":"failed","verdict":"gaps"}` | never `Some(Pass)`; `None` post-fix | The exploit needs `verdict: pass` specifically |
| **NC-6** | same marker, Layer 0 removed | `status: Failed`, `decided_by_layer: Some(1)`, `decide_action == GateReview` | The GRAFT is the mechanism — not the classifier, not `decide_action` |

The exploit case and the NC-5 positive half produce **opposite** results from the same fixture
shape. Had both gone to `None`, the fix would have disabled 18e's legitimate reconciliation
wholesale and the pair would have proved nothing.

### Two extra negative controls run beyond the plan's requirements

1. **The new named regression test was verified to fail without the fix.** Its fixture is separate
   (own tempdir, own PLAN), so a fixture bug could have produced `verdict: None` for the wrong
   reason and passed vacuously. Neutralising the status check to
   `layer1.status == AgentStatus::Success || true` reproduced `left: Some(Pass), right: None` at
   `agent_result.rs:5650`. The fix was then restored and both tests re-run green.

2. **The `idle_timeout_result` equality check was verified to discriminate.** A single-sentence
   grep returning 1 on both refs cannot distinguish "unchanged" from "this sentence survived a
   rewrite around it", so the whole 21-line block was diffed instead (`diff` exit 0), and that
   comparison was itself checked against a deliberately tampered copy (`diff` exit 1).

## Criterion 5 — The Written Finding

**The Validate trust inversion IS reachable, but not by the route the first pass identified.**

The first pass proved `classify_validate_outcome`'s inputs are always `Success` and inferred the
`(_, Some(Verdict::Pass))` wildcard was unreachable. The proof was correct; the inference was not.
The status is **laundered upstream** by `reconcile_layer0_verdict` — it genuinely *is* `Success` by
the time the classifier runs, which is exactly why checking a function's inputs cannot establish a
whole-system property.

Specifically:

- **The reachable route is the graft, not the wildcard.** `reconcile_layer0_verdict` attached
  Layer 1's `verdict` to Layer 0's `Success` without reading Layer 1's status, producing
  `(Success, Some(Pass), Some(0))`. `decide_action` had nothing to intercept, because the status
  it saw was affirmative.
- **The classifier's own inputs genuinely are always `Success`.** `decide_action` routes every
  non-`Success` status to a gate before `classify_validate_outcome` is reached. That is why the
  classifier fix (34-03, criterion 3) does not and could not close criterion 4 — it passes cleanly
  against this exploit.
- **The `Ambiguous` arms' safety depends on a routing decision in another crate** which
  `decide_action`'s own comment marks revisitable (the deferred `Failed`/`Unknown` collapse at
  `outcome_policy.rs:44-52`).
- **Four in-source comments asserted the superseded story.** This plan corrects two
  (`reconcile_stream_success_against_exit_code`'s `verdict: None` section and
  `MARKER_SUCCESS_CLAIMING_PASS`'s doc comment); plan 34-03 corrects the third
  (`classify_validate_outcome`'s own); the fourth — `idle_timeout_result`'s — is **not** overstated
  and was deliberately left alone.

**`idle_timeout_result`'s comment documents a live guard.** `reconcile_layer0_verdict` sources its
verdict from `evaluate_layer1`, whose first statement is the idle-timeout side channel, so a
timeout carrying a verdict would graft and ship a run that never reported. Verified byte-identical
to `develop` over the whole doc-comment block.

**D-07's amended two-part deliverable is discharged in full:** (a) this written finding, and (b)
task 1's executable demonstration through `evaluate_agent_result_inner`. The originally-specified
pre-fix pin test was **not** written — it could only have been built by calling
`classify_validate_outcome` directly, bypassing `advance()`; it would have passed; and it would
have measured a state production cannot produce. That is the proxy-measurement shape criterion 4
exists to avoid.

## Reviewed Diff Hunks (Task 3 acceptance criterion 3)

`git diff develop...HEAD -- crates/devflow-core/src/agent_result.rs` produces six hunks:

| Hunk | Content |
|---|---|
| `@@ -2140,6 +2140,39 @@` | `reconcile_layer0_verdict` doc comment — the D-15 record and the `project_root` asymmetry note |
| `@@ -2151,7 +2184,9 @@` | The fix itself (the `.filter` clause) |
| `@@ -2198,15 +2233,31 @@` | `reconcile_stream_success_against_exit_code`'s corrected `verdict: None` section |
| `@@ -5485,6 +5536,14 @@` | Extended fixture's doc comment |
| `@@ -5540,6 +5599,244 @@` | Fourth marker case + the three new tests |
| `@@ -5860,10 +6157,17 @@` | `MARKER_SUCCESS_CLAIMING_PASS`'s doc comment |

**No hunk touches `idle_timeout_result`.** Its doc comment spans lines 1743-1750 and its signature
is at 1751; the earliest hunk begins at 2140.

## Verification Results

| Check | Result |
|---|---|
| `layer0_affirmative_success_consults_layer1_verdict_at_validate --exact` | `1 passed`, 547 filtered out |
| `layer0_verdict_graft_declines_when_layer1_status_is_not_success --exact` | `1 passed`, 547 filtered out |
| `agent_result::tests::layer0_` selector | `7 passed; 0 failed`, 543 filtered out |
| `outcome_policy::` selector | `9 passed; 0 failed`, 541 filtered out — baseline unchanged |
| `agent_result::` module selector | `154 passed; 0 failed`, 396 filtered out |
| `stream_success_cannot_stand_against_nonzero_exit_code --exact` | `1 passed`, 549 filtered out |
| `scripts/check.sh all` | **exit 0** (captured directly, not via a pipeline) |
| devflow-core lib suite | `550 passed; 0 failed` (547 before, +3 new) |
| `rg -c 'D-15'` | 6 (criterion asked for ≥2) |
| `rg -c 'NC-5\|NC-6'` | 7 (criterion asked for ≥3) |
| `rg -c 'MARKER_SUCCESS_CLAIMING_PASS'` | **3** — unchanged; value diffed byte-clean against `develop` |
| `idle_timeout_result` guard block vs `develop` | `diff` exit 0; tampered-copy control `diff` exit 1 |

### What these results do NOT establish

- **They do not close the Validate trust boundary.** Criterion 3 (the classifier's structural fix)
  is plan 34-03's deliverable. This plan closes criteria 4 and 5 only, and the two are explicitly
  separate deliverables under D-15.
- **They do not establish that a real agent emits a self-contradictory marker in practice.** No
  parser cross-checks `status` against `verdict`. The exploit is demonstrated as reachable, not as
  observed in the wild.
- **The `550 passed` figure is a suite-level pass, not evidence about the graft.** Only the five
  named cases above bear on D-15; the rest is regression surface.

## Deviations from Plan

### 1. [Rule 1 — Test-reliability defect I would have introduced] NC-6 hardened against an env race

- **Found during:** Task 2
- **Issue:** The plan specified disabling Layer 0 for NC-6 by writing `external_verify_enabled =
  false` to `devflow.toml`. But `config::external_verify_enabled` consults
  `DEVFLOW_EXTERNAL_VERIFY_ENABLED` **before** `devflow.toml` and returns early
  (`config.rs:172-184`), and `config::tests::env_overrides_file_external_verification`
  (`config.rs:329`) sets that variable to `"true"` process-globally under a mutex private to its own
  module — which cannot serialize against `agent_result::tests`. NC-6 is the first test in this
  crate that depends on Layer 0 being *disabled*, so a parallel run of that config test could have
  re-enabled Layer 0 and flaked this control into a green.
- **Fix:** NC-6 keeps the `devflow.toml` disable (the documented mechanism) but declares **no**
  `external_verify` probe. With no declared commands and no approval vector, `evaluate_layer0`
  abstains whatever the environment says, so the control is deterministic under every value of the
  variable. Reasoning recorded in the test's own comment.
- **Verified:** NC-6 passes with `DEVFLOW_EXTERNAL_VERIFY_ENABLED=true` set explicitly, and with it
  unset.
- **Limit of that verification:** the env-precedence claim is read from `config.rs:172-184`
  (unconditional early return), not demonstrated by reproducing the race — a race that rare cannot
  be demonstrated cheaply. The hardened test's immunity under both env values *was* demonstrated.
- **Commit:** 38aaaf2

### 2. [Acceptance-criterion correction] `cargo clippy -p devflow-core --all-targets` cannot exit 0 in this repo

- **Found during:** Task 1
- **Issue:** Task 1's final acceptance criterion specified
  `cargo clippy -p devflow-core --all-targets -- -D warnings`. That command fails with four `E0433`
  errors in `tests/monitor_e2e.rs` and `tests/devflow_dir_gitignore.rs`: they reference
  `devflow_core::test_support`, which is gated behind `#[cfg(any(test, feature = "test-support"))]`
  and is not enabled when the crate is built alone.
- **Established as pre-existing, not caused by this plan:** `git status --short` showed
  `agent_result.rs` as the only modified file; the failing files were never touched, and `E0433` is
  a module-resolution error under a feature gate.
- **Resolution:** used `cargo clippy --workspace --all-targets -- -D warnings`, which is what
  `scripts/check.sh` line 38 actually gates on. **Exit code 0**, captured directly. Out of scope to
  fix per the scope boundary; no source change made.

### 3. [Observation, no action taken] A residual instance of the superseded claim

`agent_result.rs:6219` — an inline comment inside
`stream_success_cannot_stand_against_nonzero_exit_code` — still reads "Carrying
`Some(Verdict::Pass)` over would leave Validate classified Passed". This repeats the overstated
reachability claim, but Task 3 authorised correcting exactly two named sites and this is not one of
them. Recorded here for plan 34-03 rather than edited.

## TDD Gate Compliance

The plan is `type: tdd` and Task 1 carried `tdd="true"`. The RED/GREEN cycle was executed and the
red state recorded (see "The Pre-Fix Red State" above), but **the gate's commit sequence is not
literally present in git log**: Task 1's RED and GREEN were committed as a single atomic commit
(`a90cb90`, typed `fix`), because the executor's task-commit protocol commits once per task and
Task 1's `<action>` explicitly bundles "work RED first … then GREEN" into one tracer task. The
`test(34-01)` commit at `38aaaf2` follows the fix rather than preceding it, since it carries Task
2's negative controls, not Task 1's failing test.

A reviewer looking for a `test(...)` → `feat(...)` pair will not find one. The red state is
evidenced by the quoted `left: Some(Pass)` failure above and by the reproducible neutralisation
control, not by commit ordering.

## Tracer Feedback Gate

Task 1 was `type="tracer"`. Its `<verify>` was re-run end-to-end after the commit and passed
(`1 passed` on both named selectors, non-zero `filtered out` on each) before any expansion task
began.

## Known Stubs

None. No placeholder values, no `TODO`/`FIXME` markers, no skipped tests, and every `<verify>`
block in the plan was executed.

## Threat Flags

None. The plan's threat register (`T-34-01-01` … `T-34-01-SC`) is fully covered: `T-34-01-01`
mitigated by the fix and pinned by the named regression test; `T-34-01-02` (over-broad fix)
mitigated by NC-5's positive half; `T-34-01-03` (in-source record) by task 3; `T-34-01-04`
(`idle_timeout_result` guard) by the block-level equality check plus its tampered-copy control.
No new network endpoint, auth path, file-access pattern, or schema change was introduced — the
change is one expression in a pure function plus tests and comments.

## Rollback

`git revert --no-commit a90cb90^..e60ad2c` and commit once.

**Reverting this plan alone is the one unsafe ordering.** It reopens `T-34-01-01`, and plan 34-04
(999.76) makes `decided_by_layer == Some(0)` common in worktree mode — this graft's precondition.
If this fix must be withdrawn, revert 34-04 in the same operation or gate Validate manually until
it is restored, and say so in the revert message.

## Self-Check: PASSED

- `34-01-SUMMARY.md` — FOUND on disk.
- `a90cb90`, `38aaaf2`, `e60ad2c`, `128a4e4` — all FOUND in `git log`.
- `crates/devflow-core/src/agent_result.rs` — modified, committed, working tree clean.
- STATE.md and ROADMAP.md deliberately NOT modified (worktree mode; the orchestrator owns those
  writes after the wave completes). The post-commit hook's staleness notice is expected.
