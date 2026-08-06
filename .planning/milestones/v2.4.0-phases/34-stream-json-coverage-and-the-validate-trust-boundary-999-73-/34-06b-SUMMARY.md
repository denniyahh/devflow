---
phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-
plan: 06b
subsystem: test-harness
tags: [integration-tests, stream-json, widening, delivery-canary, dogfood-03, criterion-1]
status: complete

requires:
  - "plan 34-06 — this is its continuation; it recorded these five failures and declined to fix them"
  - "plan 34-02 — the legacy-opt-out idiom, and the canary-answering fake CLI this plan reuses"
provides:
  - "run_devflow_legacy_launch() — the integration-suite counterpart of 34-06's in-process opt-out"
  - "a phase7_cli suite that is 0 failed under a full five-stage widening, with two negative controls"
  - "parallel_creates_two_worktrees_and_spawns_two_monitors traversing the real canary against a stubbed CLI"
affects:
  - "plan 34-05 — its last precondition is now discharged: both the binary and integration suites survive widening"

tech-stack:
  added: []
  patterns:
    - "stubbing an outcome at the existing process-boundary seam (a fake binary on PATH) instead of adding a test-only seam to production code"
    - "a negative control aimed at the specific claim (disable the stub, confirm the test goes red on the exact refusal) rather than a generic induced assertion"

key-files:
  created:
    - .planning/phases/34-stream-json-coverage-and-the-validate-trust-boundary-999-73-/34-06b-SUMMARY.md
  modified:
    - crates/devflow-cli/tests/phase7_cli.rs

decisions:
  - "No production code was changed. The seam the wave test needed already existed — the fake `claude` on PATH — so the narrowly-scoped test-only seam the briefing authorised as a fallback was not added"
  - "The wave test's stub reuses reference_and_cleanup_worktree_cli_flow's script verbatim rather than a new one, so one fixture idiom answers the canary everywhere in this file"
  - "No assertion was added to the wave test either. An added canary-confirmed assertion would be racy under the committed narrow constant (the canary fires at Code, asynchronously) — the stream-path claim is backed by a paired control instead of by a flaky in-test check"

metrics:
  duration: "~35 min"
  completed: 2026-08-05

actuals:
  tokens: 2400
  tasks: 1
  commits: 2
---

# Phase 34 Plan 06b: Closing the Integration-Suite Widening Gap — Summary

`cargo test --test phase7_cli` is now **17 passed, 0 failed, exit 0 under a full five-stage
`STREAM_JSON_STAGES` widening**, where it was 12 passed / 5 failed / exit 101 at the same commit an
hour earlier. The four `start_*` tests got D-11's legacy opt-out; the multi-plan wave test did not,
and instead answers the delivery canary with a stubbed CLI — so the coverage the canary was added to
provide is exercised more thoroughly than before, not deleted.

**No production code was touched.** The briefing authorised adding a narrowly-scoped test-only seam
if none existed. One already did, so none was added. `git diff` for this plan is a single file,
`crates/devflow-cli/tests/phase7_cli.rs`.

## The seam question, answered by reading rather than by assuming

The briefing was right that the in-process `canary_gate(run_canary)` parameter is unreachable from an
integration test driving the real binary. But `canary_gate` is not the only seam. Reading
`canary.rs` shows the guard's entire trust decision runs against a **capture written by whatever
`claude` the child resolves on `PATH`** — and `phase7_cli.rs` already controls `PATH` for every
`devflow` it spawns, via `fake_bin_dir`.

`reference_and_cleanup_worktree_cli_flow` (`phase7_cli.rs:585` pre-change) had already used exactly
this to answer the canary at `Stage::Code` since 31-03. The wave test needed that same script and
nothing more.

This is strictly better than an injected `CanaryOutcome::Confirmed` would have been. An injected
outcome would skip the guard; the stub makes the guard **run** — `declare_token`, `canary_prompt`,
`ClaudeCanaryLauncher`'s real two-pipe threading, the capture read, and
`agent_result::token_reported_in_capture`'s provenance check — and reach `Confirmed` on its own.

## The measurements, each with the control that makes it one

Every exit code below was captured with `EXIT=$?` on the command itself, never through a pipe
(CLAUDE.md).

### 1. Paired widened/unwidened readings

| Tree | `STREAM_JSON_STAGES` | Result | `EXIT` |
|---|---|---|---|
| Base `37aa681`, unrepaired | `&[Stage::Code]` | 17 passed; 0 failed | **0** |
| Base `37aa681`, unrepaired | all five variants | **12 passed; 5 failed** | **101** |
| **Repaired** | all five variants | **17 passed; 0 failed** | **0** |
| **Repaired** | `&[Stage::Code]` | **17 passed; 0 failed** | **0** |

Rows 1–2 reproduce the orchestrator's finding independently. Rows 3–4 are the deliverable. All five
pre-repair failures carried the identical `background-task notification delivery is ABSENT` refusal
at `Stage::Define`, confirmed by reading the captured stderr, not inferred from the count.

### 2. Two negative controls, not one

A `0 failed` with no paired red reading is indistinguishable from a suite that stopped testing
anything. Both controls were run **still widened**.

| Control | What it perturbs | Result | `EXIT` |
|---|---|---|---|
| **NC-A** | `assert!(false, "NC-A induced control")` inserted in the wave test, before its `load_state` assertions | `16 passed; 1 failed` | **101** |
| **NC-B** | the stub's canary branch pattern changed to one the prompt cannot match | `16 passed; 1 failed`, on `delivery is ABSENT` | **101** |

**NC-B is the one that carries the coverage claim, and NC-A alone would not have.** NC-A only shows
the harness can still report a failure. NC-B shows something specific: with the stub's canary branch
disabled, **the wave test and only the wave test** goes red, on the exact canary refusal — so under
widening that test genuinely traverses `canary_gate` → `ClaudeCanaryLauncher` →
`token_reported_in_capture`, and its green reading is caused by the stub satisfying the guard rather
than by the guard being skipped. That is the difference between "it passes" and "it passes for the
reason claimed".

Both perturbations were removed. `rg 'NC-A induced|NC_B_DISABLED'` over the tree returns nothing, and
neither string appears in either commit.

### 3. Coverage preservation for the wave test, quoted

The wave test's diff is **one line of executable change** — the fake `claude` script — plus a comment
block. Its assertions are byte-identical:

```
@@ -169,10 +206,47 @@ fn parallel_creates_two_worktrees_and_spawns_two_monitors() {
     let repo = tempfile::tempdir().unwrap();
     let root = repo.path();
     init_repo(root);
+    // 34-06b: this test does NOT take D-11's legacy opt-out, ... [26 comment lines]
     let fake_bin = fake_bin_dir(&[
         (
             "claude",
-            "#!/bin/sh\nprintf 'fake claude\\nDEVFLOW_RESULT: {\"status\":\"success\"}\\n'\n",
+            r#"#!/bin/sh
+read -r turn
+case "$turn" in
+  *DEVFLOW_DELIVERY_CANARY_*)
+    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
+    printf '{"type":"result","subtype":"success",...,"result":"%s"}\n' "$token"
+    ;;
+  *)
+    printf 'fake claude\nDEVFLOW_RESULT: {"status":"success"}\n'
+    ;;
+esac
+"#,
         ),
         (
             "codex",
```

The hunk ends before the first assertion. Settled mechanically rather than by inspection:
`git diff -U0 | grep '^-' | grep -v '^---'` over the whole file returns exactly **10** lines, and not
one is an assertion —

| Deleted line | Why |
|---|---|
| 4 lines of `run_devflow`'s body (`Command::new(...)` … `.expect("run devflow")`) | moved verbatim into `run_devflow_inner`; the `assert!` below them was not in the deleted range |
| 4 × `run_devflow(` | renamed to `run_devflow_legacy_launch(` at the four `start_*` call sites |
| 1 × the old fake-claude script line | replaced by the stub above |
| 1 comment line | `run_devflow` → "the helper" in prose |

`git diff --diff-filter=D --name-only HEAD~1 HEAD` is empty — no file was removed.

### 4. `scripts/check.sh all`

**`EXIT=0`**, at the committed unwidened state. `phase7_cli` 17/17; `devflow_core` 554/554; every
other target green. Neither known flake (`staleness::…hostile_git_dir`,
`concurrent_ship_advances_finish_both_phases_independently`) fired, so **this run confirms nothing
either way about them** — recording the absence rather than claiming a result I did not observe.

`cargo fmt --all` was run before the commit (the pre-commit hook runs gitleaks, not rustfmt);
`cargo clippy -p devflow --all-targets -- -D warnings` exit 0.

## What was repaired, and why each got the mechanism it did

| Test | Subject | Repair |
|---|---|---|
| `start_defaults_to_worktree` | worktree-by-default | legacy opt-out |
| `start_no_worktree_uses_feature_branch` | `--no-worktree` keeps the feature branch | legacy opt-out |
| `start_worktree_mode_ignores_main_checkout_divergence` | divergence check's scope (WR-10) | legacy opt-out |
| `start_until_plan_halts_cleanly` | where the `--until` cap halts | legacy opt-out |
| `parallel_creates_two_worktrees_and_spawns_two_monitors` | **the multi-plan wave the canary protects** | **stubbed canary outcome** |

The opt-out travels through `DEVFLOW_CLAUDE_LEGACY_LAUNCH=true` on the spawned `devflow` process.
Verified by reading, not assumed: `commands.rs:151` calls `apply_legacy_launch_opt_out`, which ORs
`config::claude_legacy_launch()` into `state.legacy_claude_launch` and persists it — so it reaches
the **detached monitor's** later stages, which is what `start_until_plan_halts_cleanly` needs, since
Plan is launched by the monitor chain and not by the process that saw the environment variable.

## What these results do NOT establish

- **They do not establish that any stage should be widened.** A green suite under widening says
  nothing about whether `Define` or `Validate` belongs on the stream path. That rests on per-stage
  production capture evidence — 34-05's deliverable, untouched here.
- **The wave test's green reading does not establish that background-task delivery works.** It
  establishes that the guard runs and correctly accepts a CLI that *does* deliver. The stub is a
  shell script, not Claude Code. Only 34-05's acceptance run witnesses the real behaviour, and
  `canary.rs`'s own header already states that even a real `Confirmed` never means the dispatched
  work happened.
- **Each widened reading is one observation, not a rate.** Rows 3–4 were taken once each. This suite
  has documented timing sensitivity (`wait_for_pid`, `archive_phase_files` races) and a documented
  ~25% flake elsewhere in the crate, so a single green run is a weak bound on stability.
- **`check.sh all` exit 0 is regression surface, not evidence about this plan.** Only the five
  repaired tests bear on it.
- **NC-B does not prove the wave test exercises the stream path under the COMMITTED constant.** Under
  `&[Stage::Code]`, phase 7's Define and Plan still take the legacy path and the canary fires later,
  at Code, asynchronously — see the finding below.

## A finding worth surfacing: what the wave test was doing before, at Code

Under the committed `&[Stage::Code]` constant, phase 7's Code stage already resolved to the stream
path, so `canary_gate` already ran there — and with the **old** fake `claude` it must have **refused**
that launch. The wave test passed anyway, because that refusal happens inside the detached monitor's
`advance`, whose output goes nowhere and which none of the test's assertions inspect.

So the fixture change is not inert at the committed constant: it converts a silent Code-stage canary
refusal into a confirmation. That is an improvement and it changed no assertion, but it is the sort of
thing worth saying out loud rather than filing under "the test still passes".

**This is also why no canary-confirmed assertion was added to the wave test.** Under the narrow
constant the event is emitted asynchronously, well after `run_devflow` returns, so a direct assertion
would be racy and a polling one would add flake to a test the repo already documents as
timing-sensitive. `reference_and_cleanup_worktree_cli_flow` can assert it because it first waits for
the Validate gate; the wave test has no comparable synchronisation point.

## Deviations from Plan

There is no PLAN.md for 34-06b; the executor briefing was the specification. Two departures from it:

### 1. [Scope reduction] No test-only production seam was added

The briefing authorised adding one "if a suitable seam does not exist". One existed — the fake
`claude` on `PATH`, already used for this exact purpose by a sibling test in the same file — and the
briefing's own instruction was to prefer an existing mechanism. Nothing in `crates/*/src/` changed.
There is consequently **no production-code change to report prominently**, which the briefing asked
for had one been made.

### 2. [Method] A second, claim-specific negative control was added

The briefing asked for one induced-fault control (NC-A). NC-A alone shows only that the harness can
go red; it does not distinguish "the wave test traverses the canary and the stub satisfies it" from
"the wave test no longer reaches the canary at all" — and those two are exactly what this plan's
central decision turns on. NC-B was added to separate them. Both are reported.

## Known Stubs

The wave test's fake `claude` is a **fixture stub, by design and by the operator's binding decision**
— not an implementation stub. It is confined to `#[cfg(test)]` integration-test code, is the same
idiom this file has carried since 31-03, and its presence is what NC-B measures rather than something
NC-B has to work around.

Otherwise none: no `TODO`/`FIXME` added, no test skipped or `#[ignore]`d, no assertion removed or
weakened, no placeholder value. Both induced perturbations were removed and their absence verified by
`rg`; neither appears in any commit. The five-stage widening was applied once and reverted once —
`STREAM_JSON_STAGES` reads `&[Stage::Code]` in both of this plan's commits, verified by `git diff` and
by reading the constant back at `pipeline_launch.rs:470`, not from memory.

Nothing was appended to `.planning/WINDOWS.md` — the briefing forbids touching it (pre-existing
frontmatter/entry count mismatch). Nothing needed deferring to `deferred-items.md` either: this plan
closed the item 34-06 recorded there rather than adding one.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern, or schema change. Every changed line
is inside an integration test. The delivery canary's production behaviour is byte-identical.

One threat-adjacent note, stated because the opposite reading is available: the stub makes a guard
report `Confirmed` in an environment where real background-task delivery has not been demonstrated.
That is contained — it is a fixture on a test-controlled `PATH`, reachable only by a `devflow` child
spawned from this test file, and it cannot influence any real run. The distinction the guard exists
to protect (a *proxy* for the behaviour vs. the behaviour) is unaffected: the stub is not asserted to
be evidence about Claude Code, only about the guard's own wiring.

## Rollback

`git revert --no-commit d0be169^..HEAD` and commit once.

**Ordering constraint, same shape as 34-06's.** Do not revert this plan while 34-05's widening is in
place: these five repairs are what keep `phase7_cli` green under it, and reverting them under a
widened constant turns `scripts/check.sh all` red — demonstrated directly by this plan's own row-2
reading. If only 34-06b must be withdrawn, narrow `STREAM_JSON_STAGES` back to `&[Stage::Code]` in the
same operation.

## Self-Check: PASSED

- `crates/devflow-cli/tests/phase7_cli.rs` — FOUND, modified, committed in `d0be169`.
- `.planning/phases/34-…/34-06b-SUMMARY.md` — written and committed in this step.
- `d0be169` — FOUND in `git log`.
- `STREAM_JSON_STAGES` — confirmed `&[Stage::Code]` at `pipeline_launch.rs:470`; the temporary
  widening appears in neither commit; `git status --porcelain` showed only the test file before
  staging.
- `NC-A induced` / `NC_B_DISABLED` — confirmed absent from the tree and from both commits.
- STATE.md and ROADMAP.md deliberately NOT modified — worktree mode; the orchestrator owns those.
